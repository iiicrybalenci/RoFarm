use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Component, Path};

pub const PLUGIN_API_VERSION: u32 = 1;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginPermissions {
    pub network_hosts: Vec<String>,
    pub read_paths: Vec<String>,
    pub write_paths: Vec<String>,
    pub notifications: bool,
    pub clipboard_read: bool,
    pub clipboard_write: bool,
    pub launch_accounts: bool,
    pub kill_clients: bool,
}

impl PluginPermissions {
    pub fn validate(&self) -> Result<(), ManifestError> {
        for host in &self.network_hosts {
            if host.contains('*') || host.contains('/') || host.contains(':') || host.trim().is_empty() {
                return Err(ManifestError::Invalid(format!("Network host must be an exact hostname: {host}")));
            }
        }
        for path in self.read_paths.iter().chain(self.write_paths.iter()) {
            if !safe_relative(path) {
                return Err(ManifestError::Invalid(format!("Unsafe plugin path: {path}")));
            }
        }
        if self.network_hosts.len() > 16 || self.read_paths.len() > 16 || self.write_paths.len() > 16 {
            return Err(ManifestError::Invalid("Too many permission entries (max 16 each).".into()));
        }
        Ok(())
    }

    pub fn is_subset_of(&self, granted: &PluginPermissions) -> bool {
        let requested: BTreeSet<&String> = self.network_hosts.iter().collect();
        let allowed: BTreeSet<&String> = granted.network_hosts.iter().collect();
        requested.is_subset(&allowed)
            && self.read_paths.iter().all(|p| granted.read_paths.contains(p))
            && self.write_paths.iter().all(|p| granted.write_paths.contains(p))
            && (!self.notifications || granted.notifications)
            && (!self.clipboard_read || granted.clipboard_read)
            && (!self.clipboard_write || granted.clipboard_write)
            && (!self.launch_accounts || granted.launch_accounts)
            && (!self.kill_clients || granted.kill_clients)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub api_version: u32,
    pub entry: String,
    #[serde(default)]
    pub permissions: PluginPermissions,
    #[serde(default)]
    pub signature: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("{0}")]
    Invalid(String),
    #[error("manifest is not valid JSON: {0}")]
    Json(String),
    #[error("plugin targets API v{0}, this RoFarm supports v1")]
    UnsupportedApi(u32),
    #[error("plugin signature is missing or invalid")]
    Signature,
}

pub fn safe_relative(path: &str) -> bool {
    let candidate = Path::new(path);
    if candidate.is_absolute() || path.contains('\0') || path.len() > 240 {
        return false;
    }
    candidate.components().all(|c| match c {
        Component::Normal(part) => !part.to_string_lossy().starts_with('.'),
        Component::CurDir => true,
        _ => false,
    })
}

impl PluginManifest {
    pub fn validate(&self) -> Result<(), ManifestError> {
        let valid_id = !self.id.is_empty()
            && self.id.len() <= 64
            && self
                .id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | '.'));
        if !valid_id || self.id.starts_with('.') {
            return Err(ManifestError::Invalid("Plugin id must be lowercase a-z, 0-9, '-', '_' or '.'".into()));
        }
        if self.name.trim().is_empty() || self.name.len() > 64 {
            return Err(ManifestError::Invalid("Plugin name is required (max 64 chars).".into()));
        }
        if self.version.trim().is_empty() || self.version.len() > 32 {
            return Err(ManifestError::Invalid("Plugin version is required.".into()));
        }
        if self.api_version != PLUGIN_API_VERSION {
            return Err(ManifestError::UnsupportedApi(self.api_version));
        }
        if self.entry != "plugin.wasm" {
            return Err(ManifestError::Invalid("entry must be plugin.wasm".into()));
        }
        self.permissions.validate()?;
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut clone = self.clone();
        clone.signature = None;
        let mut value = serde_json::to_value(&clone).unwrap_or(serde_json::Value::Null);
        canonicalize(&mut value);
        serde_json::to_vec(&value).unwrap_or_default()
    }

    pub fn verify_signed(&self, trusted_keys: &[[u8; 32]]) -> Result<(), ManifestError> {
        let Some(signature) = &self.signature else {
            return Err(ManifestError::Signature);
        };
        let sig_bytes = hex::decode(signature).map_err(|_| ManifestError::Signature)?;
        let signature = Signature::from_slice(&sig_bytes).map_err(|_| ManifestError::Signature)?;
        let bytes = self.canonical_bytes();
        for key in trusted_keys {
            let Ok(verifying) = VerifyingKey::from_bytes(key) else { continue };
            if verifying.verify(&bytes, &signature).is_ok() {
                return Ok(());
            }
        }
        Err(ManifestError::Signature)
    }
}

fn canonicalize(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            let mut entries: Vec<(String, serde_json::Value)> = std::mem::take(map).into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            for (key, mut value) in entries {
                canonicalize(&mut value);
                map.insert(key, value);
            }
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(canonicalize),
        _ => {}
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub download_url: String,
    pub sha256: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginIndex {
    pub schema: u32,
    pub generated_at: chrono::DateTime<chrono::Utc>,
    pub plugins: Vec<IndexEntry>,
    pub signature: String,
}

impl PluginIndex {
    pub fn verify(&self, trusted_key: &[u8; 32]) -> Result<(), ManifestError> {
        let sig = hex::decode(&self.signature).map_err(|_| ManifestError::Signature)?;
        let signature = Signature::from_slice(&sig).map_err(|_| ManifestError::Signature)?;
        let mut clone = self.clone();
        clone.signature.clear();
        let mut value = serde_json::to_value(&clone).map_err(|_| ManifestError::Signature)?;
        canonicalize(&mut value);
        let bytes = serde_json::to_vec(&value).map_err(|_| ManifestError::Signature)?;
        let key = VerifyingKey::from_bytes(trusted_key).map_err(|_| ManifestError::Signature)?;
        key.verify(&bytes, &signature).map_err(|_| ManifestError::Signature)
    }

    pub fn validate_downloads(&self) -> Result<(), ManifestError> {
        for entry in &self.plugins {
            let url = url::Url::parse(&entry.download_url).map_err(|_| ManifestError::Invalid("bad download URL".into()))?;
            if url.scheme() != "https" {
                return Err(ManifestError::Invalid("plugin downloads must use HTTPS".into()));
            }
            if entry.sha256.len() != 64 || !entry.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(ManifestError::Invalid("plugin sha256 must be 64 hex characters".into()));
            }
        }
        Ok(())
    }
}
