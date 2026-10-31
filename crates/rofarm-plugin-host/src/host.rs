use crate::manifest::{PluginManifest, PluginPermissions};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use wasmi::{Caller, Config, Engine, Linker, Memory, Module, Store, StoreLimits, StoreLimitsBuilder};

const MAX_WASM_BYTES: u64 = 8 * 1024 * 1024;
const MAX_HTTP_BYTES: usize = 256 * 1024;
const FUEL: u64 = 20_000_000;

#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Manifest(#[from] crate::manifest::ManifestError),
    #[error("wasm: {0}")]
    Wasm(String),
    #[error("permission denied: {0}")]
    Denied(String),
    #[error("plugin fuel exhausted")]
    Fuel,
    #[error("plugin is too large or missing")]
    TooLarge,
    #[error("plugin is disabled")]
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub manifest: PluginManifest,
    pub granted: PluginPermissions,
    pub enabled: bool,
    pub installed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub directory: PathBuf,
    pub signed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRun {
    pub logs: Vec<String>,
    pub notifications: Vec<String>,
    pub network_calls: Vec<String>,
    pub emitted: Vec<String>,
    pub fuel_used: u64,
    pub duration_ms: u64,
}

#[derive(Debug, Deserialize, Serialize)]
struct GrantFile {
    granted: PluginPermissions,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    installed_at: Option<chrono::DateTime<chrono::Utc>>,
}

fn default_true() -> bool {
    true
}

pub struct PluginHost {
    root: PathBuf,
    trusted_keys: Vec<[u8; 32]>,
    allow_unsigned: bool,
}

impl std::fmt::Debug for PluginHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginHost").field("root", &self.root).field("trusted", &self.trusted_keys.len()).finish()
    }
}

struct HostState {
    grants: PluginPermissions,
    data_dir: PathBuf,
    logs: Vec<String>,
    notifications: Vec<String>,
    network_calls: Vec<String>,
    emitted: Vec<String>,
    limits: StoreLimits,
}

fn read_string(caller: &Caller<'_, HostState>, ptr: i32, len: i32) -> Option<String> {
    if ptr < 0 || len < 0 || len > 64 * 1024 {
        return None;
    }
    let memory = caller.get_export("memory").and_then(|e| e.into_memory())?;
    let mut buffer = vec![0u8; len as usize];
    memory.read(caller, ptr as usize, &mut buffer).ok()?;
    Some(String::from_utf8_lossy(&buffer).to_string())
}

fn path_granted(relative: &str, allowed: &[String]) -> bool {
    allowed.iter().any(|base| {
        base == relative
            || relative.strip_prefix(base.trim_end_matches('/')).is_some_and(|rest| rest.starts_with('/'))
    })
}

fn safe_join(base: &Path, relative: &str) -> Result<PathBuf, ()> {
    if relative.is_empty() || relative.contains('\0') || Path::new(relative).is_absolute() {
        return Err(());
    }
    if Path::new(relative).components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
        return Err(());
    }
    Ok(base.join(relative))
}

fn write_bytes(caller: &mut Caller<'_, HostState>, memory: &Memory, ptr: i32, data: &[u8]) -> i32 {
    if ptr < 0 || data.len() > 256 * 1024 {
        return -1;
    }
    match memory.write(caller, ptr as usize, data) {
        Ok(()) => data.len() as i32,
        Err(_) => -1,
    }
}

impl PluginHost {
    pub fn new(root: impl Into<PathBuf>, trusted_keys: Vec<[u8; 32]>, allow_unsigned: bool) -> Self {
        Self { root: root.into(), trusted_keys, allow_unsigned }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn install(&self, source: impl AsRef<Path>) -> Result<PluginInfo, PluginError> {
        let source = source.as_ref();
        let manifest_raw = std::fs::read(source.join("manifest.json"))?;
        let manifest: PluginManifest = serde_json::from_slice(&manifest_raw)?;
        manifest.validate()?;
        let signed = if self.trusted_keys.is_empty() {
            false
        } else {
            manifest.verify_signed(&self.trusted_keys).is_ok()
        };
        if !signed && !self.allow_unsigned {
            return Err(PluginError::Manifest(crate::manifest::ManifestError::Signature));
        }
        let wasm_path = source.join(&manifest.entry);
        let meta = std::fs::metadata(&wasm_path)?;
        if meta.len() > MAX_WASM_BYTES {
            return Err(PluginError::TooLarge);
        }
        let destination = self.root.join(&manifest.id);
        std::fs::create_dir_all(&destination)?;
        std::fs::copy(source.join("manifest.json"), destination.join("manifest.json"))?;
        std::fs::copy(&wasm_path, destination.join("plugin.wasm"))?;
        let grant = GrantFile {
            granted: PluginPermissions::default(),
            enabled: false,
            installed_at: Some(chrono::Utc::now()),
        };
        self.write_grant(&manifest.id, &grant)?;
        self.info(&manifest.id)
    }

    fn grant_path(&self, id: &str) -> PathBuf {
        self.root.join(id).join("grant.json")
    }

    fn read_grant(&self, id: &str) -> GrantFile {
        std::fs::read(self.grant_path(id))
            .ok()
            .and_then(|raw| serde_json::from_slice(&raw).ok())
            .unwrap_or(GrantFile { granted: PluginPermissions::default(), enabled: false, installed_at: None })
    }

    fn write_grant(&self, id: &str, grant: &GrantFile) -> Result<(), PluginError> {
        std::fs::create_dir_all(self.root.join(id))?;
        std::fs::write(self.grant_path(id), serde_json::to_vec_pretty(grant)?)?;
        Ok(())
    }

    pub fn set_grant(&self, id: &str, permissions: PluginPermissions, enabled: bool) -> Result<(), PluginError> {
        permissions.validate()?;
        let mut grant = self.read_grant(id);
        grant.granted = permissions;
        grant.enabled = enabled;
        self.write_grant(id, &grant)
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), PluginError> {
        let mut grant = self.read_grant(id);
        grant.enabled = enabled;
        self.write_grant(id, &grant)
    }

    pub fn uninstall(&self, id: &str) -> Result<(), PluginError> {
        let dir = self.root.join(id);
        if !dir.starts_with(&self.root) {
            return Err(PluginError::Denied("path escapes plugin root".into()));
        }
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    pub fn info(&self, id: &str) -> Result<PluginInfo, PluginError> {
        let dir = self.root.join(id);
        let manifest: PluginManifest = serde_json::from_slice(&std::fs::read(dir.join("manifest.json"))?)?;
        let grant = self.read_grant(id);
        let signed = !self.trusted_keys.is_empty() && manifest.verify_signed(&self.trusted_keys).is_ok();
        Ok(PluginInfo {
            manifest,
            granted: grant.granted,
            enabled: grant.enabled,
            installed_at: grant.installed_at,
            directory: dir,
            signed,
        })
    }

    pub fn list(&self) -> Vec<PluginInfo> {
        let Ok(entries) = std::fs::read_dir(&self.root) else { return Vec::new() };
        entries
            .flatten()
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().to_str().map(str::to_string))
            .filter_map(|id| self.info(&id).ok())
            .collect()
    }

    pub fn invoke(&self, id: &str, input: &serde_json::Value) -> Result<PluginRun, PluginError> {
        let info = self.info(id)?;
        if !info.enabled {
            return Err(PluginError::Disabled);
        }
        if !info.manifest.permissions.is_subset_of(&info.granted) {
            return Err(PluginError::Denied("manifest requests permissions that were not granted".into()));
        }
        let wasm_path = info.directory.join(&info.manifest.entry);
        let meta = std::fs::metadata(&wasm_path)?;
        if meta.len() > MAX_WASM_BYTES {
            return Err(PluginError::TooLarge);
        }
        let wasm = std::fs::read(&wasm_path)?;
        let started = Instant::now();
        let mut config = Config::default();
        config.consume_fuel(true);
        let engine = Engine::new(&config);
        let module = Module::new(&engine, &wasm[..]).map_err(|e| PluginError::Wasm(e.to_string()))?;
        let data_dir = info.directory.join("data");
        let _ = std::fs::create_dir_all(&data_dir);
        let mut store = Store::new(
            &engine,
            HostState {
                grants: info.granted.clone(),
                data_dir,
                logs: Vec::new(),
                notifications: Vec::new(),
                network_calls: Vec::new(),
                emitted: Vec::new(),
                limits: StoreLimitsBuilder::new()
                    .memory_size(32 * 1024 * 1024)
                    .table_elements(10_000)
                    .instances(4)
                    .tables(8)
                    .memories(2)
                    .build(),
            },
        );
        store.limiter(|state| &mut state.limits);
        store.set_fuel(FUEL).map_err(|e| PluginError::Wasm(e.to_string()))?;
        let mut linker = Linker::new(&engine);
        linker
            .func_wrap("env", "log", |mut caller: Caller<'_, HostState>, ptr: i32, len: i32| {
                if let Some(text) = read_string(&caller, ptr, len) {
                    caller.data_mut().logs.push(text.chars().take(2000).collect());
                }
            })
            .map_err(|e| PluginError::Wasm(e.to_string()))?;
        linker
            .func_wrap("env", "notify", |mut caller: Caller<'_, HostState>, ptr: i32, len: i32| -> i32 {
                if !caller.data().grants.notifications {
                    return -1;
                }
                if let Some(text) = read_string(&caller, ptr, len) {
                    caller.data_mut().notifications.push(text.chars().take(2000).collect());
                    return 0;
                }
                -1
            })
            .map_err(|e| PluginError::Wasm(e.to_string()))?;
        linker
            .func_wrap("env", "emit", |mut caller: Caller<'_, HostState>, ptr: i32, len: i32| -> i32 {
                if let Some(text) = read_string(&caller, ptr, len) {
                    caller.data_mut().emitted.push(text);
                    return 0;
                }
                -1
            })
            .map_err(|e| PluginError::Wasm(e.to_string()))?;
        linker
            .func_wrap(
                "env",
                "file_read",
                |mut caller: Caller<'_, HostState>, path_ptr: i32, path_len: i32, out_ptr: i32, out_cap: i32| -> i32 {
                    let Some(relative) = read_string(&caller, path_ptr, path_len) else { return -2 };
                    if !path_granted(&relative, &caller.data().grants.read_paths) {
                        return -3;
                    }
                    let base = caller.data().data_dir.clone();
                    let Ok(path) = safe_join(&base, &relative) else { return -3 };
                    let Ok(data) = std::fs::read(path) else { return -4 };
                    let memory = caller.get_export("memory").and_then(|e| e.into_memory());
                    let Some(memory) = memory else { return -2 };
                    let take = data.len().min(out_cap.max(0) as usize).min(256 * 1024);
                    write_bytes(&mut caller, &memory, out_ptr, &data[..take])
                },
            )
            .map_err(|e| PluginError::Wasm(e.to_string()))?;
        linker
            .func_wrap(
                "env",
                "file_write",
                |mut caller: Caller<'_, HostState>, path_ptr: i32, path_len: i32, data_ptr: i32, data_len: i32| -> i32 {
                    let Some(relative) = read_string(&caller, path_ptr, path_len) else { return -2 };
                    if !path_granted(&relative, &caller.data().grants.write_paths) {
                        return -3;
                    }
                    let base = caller.data().data_dir.clone();
                    let Ok(path) = safe_join(&base, &relative) else { return -3 };
                    let Some(contents) = read_string(&caller, data_ptr, data_len) else { return -2 };
                    if contents.len() > 256 * 1024 {
                        return -1;
                    }
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    match std::fs::write(path, contents.as_bytes()) {
                        Ok(()) => 0,
                        Err(_) => -4,
                    }
                },
            )
            .map_err(|e| PluginError::Wasm(e.to_string()))?;
        linker
            .func_wrap(
                "env",
                "http_get",
                |mut caller: Caller<'_, HostState>, url_ptr: i32, url_len: i32, out_ptr: i32, out_cap: i32| -> i32 {
                    let Some(url) = read_string(&caller, url_ptr, url_len) else { return -2 };
                    let Ok(parsed) = url::Url::parse(&url) else { return -2 };
                    if parsed.scheme() != "https" {
                        return -3;
                    }
                    let Some(host) = parsed.host_str().map(str::to_ascii_lowercase) else { return -3 };
                    if !caller.data().grants.network_hosts.iter().any(|h| h.eq_ignore_ascii_case(&host)) {
                        return -3;
                    }
                    let Ok(response) = ureq::AgentBuilder::new().timeout(Duration::from_secs(8)).build().get(&url).call() else {
                        return -4;
                    };
                    let mut body = Vec::new();
                    if response.into_reader().take(MAX_HTTP_BYTES as u64).read_to_end(&mut body).is_err() {
                        return -4;
                    }
                    caller.data_mut().network_calls.push(format!("GET {host}{}", parsed.path()));
                    let memory = caller.get_export("memory").and_then(|e| e.into_memory());
                    let Some(memory) = memory else { return -2 };
                    let cap = out_cap.max(0) as usize;
                    let take = body.len().min(cap);
                    write_bytes(&mut caller, &memory, out_ptr, &body[..take])
                },
            )
            .map_err(|e| PluginError::Wasm(e.to_string()))?;
        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| PluginError::Wasm(e.to_string()))?
            .start(&mut store)
            .map_err(|e| PluginError::Wasm(e.to_string()))?;
        let input_bytes = serde_json::to_vec(input)?;
        if input_bytes.len() > 32 * 1024 {
            return Err(PluginError::TooLarge);
        }
        let memory = instance
            .get_memory(&store, "memory")
            .ok_or_else(|| PluginError::Wasm("plugin does not export memory".into()))?;
        const INPUT_OFFSET: usize = 1024;
        if memory.data_size(&store) < INPUT_OFFSET + input_bytes.len() {
            return Err(PluginError::Wasm("plugin memory is too small for input".into()));
        }
        memory
            .write(&mut store, INPUT_OFFSET, &input_bytes)
            .map_err(|e| PluginError::Wasm(e.to_string()))?;
        let run = instance
            .get_typed_func::<(i32, i32), i32>(&store, "run")
            .map_err(|e| PluginError::Wasm(e.to_string()))?;
        let status = run
            .call(&mut store, (INPUT_OFFSET as i32, input_bytes.len() as i32))
            .map_err(|e| {
                if store.get_fuel().is_ok_and(|f| f == 0) {
                    PluginError::Fuel
                } else {
                    PluginError::Wasm(e.to_string())
                }
            })?;
        if status != 0 {
            return Err(PluginError::Wasm(format!("plugin returned status {status}")));
        }
        let state = store.data();
        let fuel_used = FUEL.saturating_sub(store.get_fuel().unwrap_or(0));
        Ok(PluginRun {
            logs: state.logs.clone(),
            notifications: state.notifications.clone(),
            network_calls: state.network_calls.clone(),
            emitted: state.emitted.clone(),
            fuel_used,
            duration_ms: started.elapsed().as_millis() as u64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{PluginManifest, PluginPermissions};
    use std::io::Write;

    fn wasm() -> Vec<u8> {
        wat::parse_str(
            r#"(module
              (import "env" "log" (func $log (param i32 i32)))
              (memory (export "memory") 2)
              (data (i32.const 100) "hello from plugin")
              (func (export "run") (param i32 i32) (result i32)
                (call $log (i32.const 100) (i32.const 17))
                (i32.const 0)))"#,
        )
        .unwrap()
    }

    fn plugin_dir(root: &Path, id: &str) -> PathBuf {
        let dir = root.join(id);
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = PluginManifest {
            id: id.into(),
            name: "Test plugin".into(),
            version: "1.0.0".into(),
            description: String::new(),
            api_version: 1,
            entry: "plugin.wasm".into(),
            permissions: PluginPermissions::default(),
            signature: None,
        };
        std::fs::write(dir.join("manifest.json"), serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
        let mut file = std::fs::File::create(dir.join("plugin.wasm")).unwrap();
        file.write_all(&wasm()).unwrap();
        dir
    }

    #[test]
    fn unsigned_plugins_can_run_only_with_explicit_permission() {
        let temp = tempfile::tempdir().unwrap();
        let source = plugin_dir(temp.path(), "hello");
        let host = PluginHost::new(temp.path().join("installed"), vec![], false);
        assert!(host.install(&source).is_err());
        let host = PluginHost::new(temp.path().join("installed"), vec![], true);
        let info = host.install(&source).unwrap();
        assert!(!info.enabled);
        assert!(matches!(host.invoke("hello", &serde_json::json!({})), Err(PluginError::Disabled)));
        host.set_enabled("hello", true).unwrap();
        let run = host.invoke("hello", &serde_json::json!({})).unwrap();
        assert_eq!(run.logs, vec!["hello from plugin"]);
        assert_eq!(run.emitted.len(), 0);
    }

    #[test]
    fn permissions_are_enforced() {
        let temp = tempfile::tempdir().unwrap();
        let host = PluginHost::new(temp.path().join("p"), vec![], true);
        let mut perms = PluginPermissions::default();
        perms.network_hosts.push("example.com".into());
        let manifest = PluginManifest {
            id: "net".into(),
            name: "net".into(),
            version: "1".into(),
            description: String::new(),
            api_version: 1,
            entry: "plugin.wasm".into(),
            permissions: perms.clone(),
            signature: None,
        };
        let dir = temp.path().join("net");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("manifest.json"), serde_json::to_vec(&manifest).unwrap()).unwrap();
        std::fs::write(dir.join("plugin.wasm"), wasm()).unwrap();
        let info = host.install(&dir).unwrap();
        assert!(!info.granted.network_hosts.contains(&"example.com".to_string()));
        host.set_enabled("net", true).unwrap();
        assert!(matches!(host.invoke("net", &serde_json::json!({})), Err(PluginError::Denied(_))));
        host.set_grant("net", perms, true).unwrap();
        assert!(host.invoke("net", &serde_json::json!({})).is_ok());
    }

    #[test]
    fn runaway_plugins_are_stopped_by_fuel() {
        let temp = tempfile::tempdir().unwrap();
        let host = PluginHost::new(temp.path().join("p"), vec![], true);
        let dir = temp.path().join("loop");
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = PluginManifest {
            id: "loop".into(),
            name: "loop".into(),
            version: "1".into(),
            description: String::new(),
            api_version: 1,
            entry: "plugin.wasm".into(),
            permissions: PluginPermissions::default(),
            signature: None,
        };
        std::fs::write(dir.join("manifest.json"), serde_json::to_vec(&manifest).unwrap()).unwrap();
        let looping = wat::parse_str(
            r#"(module (memory (export "memory") 2)
                (func (export "run") (param i32 i32) (result i32)
                  (loop $l (br $l)) (i32.const 0)))"#,
        )
        .unwrap();
        std::fs::write(dir.join("plugin.wasm"), looping).unwrap();
        host.install(&dir).unwrap();
        host.set_enabled("loop", true).unwrap();
        let result = host.invoke("loop", &serde_json::json!({}));
        assert!(result.is_err(), "runaway plugin should be stopped");
    }
}
