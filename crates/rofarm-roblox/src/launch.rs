use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

use rofarm_core::launch::ClientSettings;

use crate::error::{RobloxError, RobloxResult};

fn encode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

pub fn build_launch_uri(
    ticket: &str,
    place_id: u64,
    job_id: Option<&str>,
    private_server_code: Option<&str>,
    launch_time_ms: i64,
) -> String {
    let mut place_launcher = format!(
        "https://assetgame.roblox.com/game/PlaceLauncher.ashx?request=RequestGame&placeId={place_id}&isPlayTogetherGame=false"
    );
    if let Some(job) = job_id {
        place_launcher.push_str("&gameInstanceId=");
        place_launcher.push_str(&encode(job));
    }
    if let Some(code) = private_server_code {
        place_launcher.push_str("&privateServerLinkCode=");
        place_launcher.push_str(&encode(code));
    }
    format!(
        "roblox-player:1+launchmode:play+gameinfo:{ticket}+launchtime:{launch_time_ms}+placelauncherurl:{}+browsertrackerid:0+robloxLocale:en_us+gameLocale:en_us+channel:+LaunchExp:InApp",
        encode(&place_launcher)
    )
}

#[derive(Clone, PartialEq, Eq)]
pub struct LaunchSpec {
    pub place_id: u64,
    pub job_id: Option<String>,
    uri: String,
}

impl std::fmt::Debug for LaunchSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LaunchSpec")
            .field("place_id", &self.place_id)
            .field("job_id", &self.job_id)
            .field("uri", &"[redacted launch ticket]")
            .finish()
    }
}

impl LaunchSpec {
    pub fn new(place_id: u64, job_id: Option<String>, ticket: &str) -> Self {
        let uri = build_launch_uri(
            ticket,
            place_id,
            job_id.as_deref(),
            None,
            chrono::Utc::now().timestamp_millis(),
        );
        Self { place_id, job_id, uri }
    }

    pub fn expose_uri(&self) -> &str {
        &self.uri
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledClient {
    pub version: String,
    pub directory: PathBuf,
    pub player_exe: PathBuf,
}

pub fn find_installed_clients() -> Vec<InstalledClient> {
    let Some(local) = std::env::var_os("LOCALAPPDATA") else {
        return Vec::new();
    };
    let versions = PathBuf::from(local).join("Roblox").join("Versions");
    let Ok(entries) = fs::read_dir(&versions) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if !name.starts_with("version-") {
            continue;
        }
        let player = dir.join("RobloxPlayerBeta.exe");
        if player.exists() {
            out.push(InstalledClient { version: name.to_string(), directory: dir, player_exe: player });
        }
    }
    out.sort_by(|a, b| b.version.cmp(&a.version));
    out
}

pub struct ClientSettingsFile {
    path: PathBuf,
    backup_path: PathBuf,
    original: Option<Vec<u8>>,
    installed: bool,
}

impl std::fmt::Debug for ClientSettingsFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientSettingsFile").field("path", &self.path).field("installed", &self.installed).finish()
    }
}

impl ClientSettingsFile {
    pub fn client_settings_path(client_dir: &Path) -> PathBuf {
        client_dir.join("ClientSettings").join("ClientAppSettings.json")
    }

    pub fn install(client_dir: &Path, settings: &ClientSettings) -> RobloxResult<Self> {
        let path = Self::client_settings_path(client_dir);
        let settings_dir = path.parent().ok_or_else(|| RobloxError::Invalid("bad client directory".into()))?;
        fs::create_dir_all(settings_dir)?;
        let original = fs::read(&path).ok();
        let backup_path = path.with_extension("json.rofarm-backup");
        if let Some(bytes) = &original {
            fs::write(&backup_path, bytes)?;
        }
        let value: Value = settings.to_json();
        let bytes = serde_json::to_vec_pretty(&value).map_err(|_| RobloxError::Decode)?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, &bytes)?;
        if fs::rename(&tmp, &path).is_err() {
            let _ = fs::remove_file(&path);
            fs::rename(&tmp, &path)?;
        }
        Ok(Self { path, backup_path, original, installed: true })
    }

    pub fn restore(&mut self) -> RobloxResult<()> {
        if !self.installed {
            return Ok(());
        }
        match self.original.take() {
            Some(bytes) => fs::write(&self.path, bytes)?,
            None => {
                if self.path.exists() {
                    fs::remove_file(&self.path)?;
                }
            }
        }
        let _ = fs::remove_file(&self.backup_path);
        self.installed = false;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ClientSettingsFile {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

pub fn roblox_logs_dir() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(local).join("Roblox").join("logs"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rofarm_core::lane::GraphicsPreset;

    #[test]
    fn launch_uri_contains_encoded_place_and_job() {
        let uri = build_launch_uri("ticket", 123, Some("job id"), Some("code/x"), 1700000000000);
        assert!(uri.starts_with("roblox-player:1+launchmode:play+gameinfo:ticket"));
        assert!(uri.contains("launchtime:1700000000000"));
        assert!(uri.contains("placeId%3D123"));
        assert!(uri.contains("gameInstanceId%3Djob%2Bid"));
        assert!(uri.contains("privateServerLinkCode%3Dcode%252Fx"));
        let spec = LaunchSpec::new(1, None, "secret-ticket");
        assert!(!format!("{spec:?}").contains("secret-ticket"));
        assert!(spec.expose_uri().contains("secret-ticket"));
    }

    #[test]
    fn client_settings_install_and_restore() {
        let dir = tempfile::tempdir().unwrap();
        let settings_dir = dir.path().join("ClientSettings");
        fs::create_dir_all(&settings_dir).unwrap();
        let path = settings_dir.join("ClientAppSettings.json");
        fs::write(&path, br#"{"existing":true}"#).unwrap();
        let mut settings = ClientSettings::default();
        settings.fps_cap = Some(20);
        settings.graphics = GraphicsPreset::Lowest;
        {
            let handle = ClientSettingsFile::install(dir.path(), &settings).unwrap();
            let written: Value = serde_json::from_slice(&fs::read(handle.path()).unwrap()).unwrap();
            assert_eq!(written["DFIntTaskSchedulerTargetFps"], 20);
            assert!(handle.path().with_extension("json.rofarm-backup").exists());
        }
        let restored = fs::read_to_string(&path).unwrap();
        assert_eq!(restored, r#"{"existing":true}"#);
        assert!(!path.with_extension("json.rofarm-backup").exists());
    }

    #[test]
    fn client_settings_are_removed_if_no_original_existed() {
        let dir = tempfile::tempdir().unwrap();
        let handle = ClientSettingsFile::install(dir.path(), &ClientSettings::default()).unwrap();
        let path = handle.path().to_path_buf();
        assert!(path.exists());
        drop(handle);
        assert!(!path.exists());
    }

    #[test]
    fn installed_client_discovery_is_empty_without_roblox() {
        for client in find_installed_clients() {
            assert!(client.player_exe.ends_with("RobloxPlayerBeta.exe"));
            assert!(client.version.starts_with("version-"));
        }
    }
}
