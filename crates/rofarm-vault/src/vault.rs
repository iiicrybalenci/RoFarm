use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use zeroize::Zeroizing;

use rofarm_core::ids::AccountId;

use crate::binding::{BindingKind, KeyBinder};
use crate::crypto::{b64_decode, constant_time_eq, keyed_hash, open, seal, SecretKey, Sealed};
use crate::error::{VaultError, VaultResult};
use crate::kdf::{self, KdfParams};

pub const VAULT_MAGIC: &str = "ROFARMV2";
pub const VAULT_VERSION: u16 = 2;
pub const MAX_VAULT_BYTES: u64 = 32 * 1024 * 1024;
const MASTER_AAD: &[u8] = b"rofarm-master-key-v2";
const RECORD_SCHEMA_EPOCH: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordKind {
    AccountDataKey,
    Cookie,
    NotifierCredential,
    SmtpCredential,
    PairingToken,
    PluginGrant,
    Generic,
}

impl RecordKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            RecordKind::AccountDataKey => "account_data_key",
            RecordKind::Cookie => "cookie",
            RecordKind::NotifierCredential => "notifier_credential",
            RecordKind::SmtpCredential => "smtp_credential",
            RecordKind::PairingToken => "pairing_token",
            RecordKind::PluginGrant => "plugin_grant",
            RecordKind::Generic => "generic",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingEntry {
    pub binder_id: String,
    pub kind: BindingKind,
    pub label: String,
    #[serde(with = "crate::crypto::b64")]
    pub blob: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryEntry {
    pub kdf: KdfParams,
    pub wrapped_master_key: Sealed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VaultRecord {
    pub kind: RecordKind,
    pub account_id: Option<AccountId>,
    pub generation: u64,
    pub sealed: Sealed,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VaultHeader {
    pub magic: String,
    pub version: u16,
    pub vault_id: Uuid,
    pub generation: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub kdf: KdfParams,
    pub wrapped_master_key: Sealed,
    pub bindings: Vec<BindingEntry>,
    pub recovery: Option<RecoveryEntry>,
    pub record_generations: BTreeMap<String, u64>,
    pub header_mac: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VaultFile {
    pub header: VaultHeader,
    pub records: BTreeMap<String, VaultRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindingSummary {
    pub binder_id: String,
    pub kind: BindingKind,
    pub label: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct LockoutFile {
    failures: u32,
    locked_until: Option<DateTime<Utc>>,
}

pub struct Vault {
    path: PathBuf,
    header: VaultHeader,
    records: BTreeMap<String, VaultRecord>,
    master: Option<SecretKey>,
    data_keys: BTreeMap<AccountId, SecretKey>,
    lockout: LockoutFile,
    lockout_path: PathBuf,
}

impl std::fmt::Debug for Vault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vault")
            .field("path", &self.path)
            .field("vault_id", &self.header.vault_id)
            .field("records", &self.records.len())
            .field("unlocked", &self.master.is_some())
            .field("generation", &self.header.generation)
            .finish()
    }
}

fn master_aad(vault_id: Uuid, created_at: DateTime<Utc>) -> Vec<u8> {
    let mut out = MASTER_AAD.to_vec();
    out.extend_from_slice(vault_id.as_bytes());
    out.extend_from_slice(&created_at.timestamp_micros().to_le_bytes());
    out
}

fn record_aad(
    vault_id: Uuid,
    created_at: DateTime<Utc>,
    key: &str,
    kind: RecordKind,
    account_id: Option<AccountId>,
    generation: u64,
) -> Vec<u8> {
    format!(
        "ROFARM/RECORD/{RECORD_SCHEMA_EPOCH}|{}|{}|{}|{}|{}|{}",
        vault_id,
        created_at.timestamp_micros(),
        key,
        kind.as_str(),
        account_id.map(|a| a.to_string()).unwrap_or_else(|| "-".into()),
        generation
    )
    .into_bytes()
}

fn header_mac(header: &VaultHeader, master: &[u8; 32]) -> VaultResult<String> {
    let mut clone = header.clone();
    clone.header_mac = String::new();
    let bytes = serde_json::to_vec(&clone)?;
    let mac = keyed_hash(master, &[b"rofarm-header-mac-v2", &bytes]);
    Ok(hex_encode(&mac))
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn validate_secret_name(name: &str) -> VaultResult<()> {
    if name.is_empty() || name.len() > 80 {
        return Err(VaultError::Invalid("Secret name must be 1-80 characters.".into()));
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')) {
        return Err(VaultError::Invalid("Secret name has invalid characters.".into()));
    }
    Ok(())
}

impl Vault {
    pub fn create(path: impl AsRef<Path>, passphrase: &str, params: KdfParams) -> VaultResult<Self> {
        if passphrase.chars().count() < 8 {
            return Err(VaultError::Invalid("Use a passphrase of at least 8 characters.".into()));
        }
        params.validate()?;
        let path = path.as_ref().to_path_buf();
        if path.exists() {
            return Err(VaultError::AlreadyExists);
        }
        let vault_id = Uuid::new_v4();
        let created_at = Utc::now();
        let master = crate::crypto::random_key();
        let kek = kdf::derive(passphrase, &params)?;
        let wrapped_master_key = seal(&kek, &master_aad(vault_id, created_at), master.as_ref())?;
        let mut header = VaultHeader {
            magic: VAULT_MAGIC.into(),
            version: VAULT_VERSION,
            vault_id,
            generation: 0,
            created_at,
            updated_at: created_at,
            kdf: params,
            wrapped_master_key,
            bindings: Vec::new(),
            recovery: None,
            record_generations: BTreeMap::new(),
            header_mac: String::new(),
        };
        header.header_mac = header_mac(&header, &master)?;
        let mut vault = Self {
            lockout_path: lockout_path_for(&path),
            path,
            header,
            records: BTreeMap::new(),
            master: Some(master),
            data_keys: BTreeMap::new(),
            lockout: LockoutFile::default(),
        };
        vault.persist()?;
        tracing::info!(vault_id = %vault.header.vault_id, "vault created");
        Ok(vault)
    }

    pub fn open_file(path: impl AsRef<Path>) -> VaultResult<Self> {
        let path = path.as_ref().to_path_buf();
        let meta = fs::metadata(&path)?;
        if meta.len() > MAX_VAULT_BYTES {
            return Err(VaultError::Corrupt);
        }
        let bytes = fs::read(&path)?;
        let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| VaultError::Corrupt)?;
        let version = value.get("header").and_then(|h| h.get("version")).and_then(|v| v.as_u64()).unwrap_or(0) as u16;
        if version != VAULT_VERSION {
            return Err(VaultError::UnsupportedVersion(version));
        }
        let file: VaultFile = serde_json::from_value(value).map_err(|_| VaultError::Corrupt)?;
        if file.header.magic != VAULT_MAGIC || file.header.version != VAULT_VERSION {
            return Err(VaultError::Corrupt);
        }
        file.header.kdf.validate()?;
        for (key, record) in &file.records {
            if key.is_empty() || key.len() > 160 {
                return Err(VaultError::Corrupt);
            }
            if b64_decode(&record.sealed.ciphertext).is_err() || b64_decode(&record.sealed.nonce).is_err() {
                return Err(VaultError::Corrupt);
            }
        }
        let lockout = read_lockout(&lockout_path_for(&path)).unwrap_or_default();
        Ok(Self {
            lockout_path: lockout_path_for(&path),
            path,
            header: file.header,
            records: file.records,
            master: None,
            data_keys: BTreeMap::new(),
            lockout,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn vault_id(&self) -> Uuid {
        self.header.vault_id
    }

    pub fn generation(&self) -> u64 {
        self.header.generation
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.header.created_at
    }

    pub fn is_unlocked(&self) -> bool {
        self.master.is_some()
    }

    pub fn kdf(&self) -> &KdfParams {
        &self.header.kdf
    }

    pub fn available_bindings(&self) -> Vec<BindingSummary> {
        self.header
            .bindings
            .iter()
            .map(|b| BindingSummary { binder_id: b.binder_id.clone(), kind: b.kind, label: b.label.clone() })
            .collect()
    }

    pub fn has_recovery(&self) -> bool {
        self.header.recovery.is_some()
    }

    pub fn lockout_remaining(&self) -> Option<Duration> {
        self.lockout
            .locked_until
            .filter(|until| *until > Utc::now())
            .map(|until| until - Utc::now())
    }

    fn check_lockout(&self) -> VaultResult<()> {
        if let Some(remaining) = self.lockout_remaining() {
            return Err(VaultError::TooManyAttempts(remaining.to_std().unwrap_or_default()));
        }
        Ok(())
    }

    fn record_failure(&mut self) {
        self.lockout.failures = self.lockout.failures.saturating_add(1);
        if self.lockout.failures >= 5 {
            let exponent = (self.lockout.failures - 5).min(5);
            let delay = Duration::seconds(30 * (1i64 << exponent)).min(Duration::minutes(15));
            self.lockout.locked_until = Some(Utc::now() + delay);
        }
        let _ = write_lockout(&self.lockout_path, &self.lockout);
    }

    fn record_success(&mut self) {
        self.lockout = LockoutFile::default();
        let _ = fs::remove_file(&self.lockout_path);
    }

    pub fn unlock(&mut self, passphrase: &str) -> VaultResult<()> {
        self.check_lockout()?;
        let kek = match kdf::derive(passphrase, &self.header.kdf) {
            Ok(k) => k,
            Err(_) => {
                self.record_failure();
                return Err(VaultError::AuthenticationFailed);
            }
        };
        let opened = match open(
            &kek,
            &master_aad(self.header.vault_id, self.header.created_at),
            &self.header.wrapped_master_key,
        ) {
            Ok(bytes) => bytes,
            Err(_) => {
                self.record_failure();
                return Err(VaultError::AuthenticationFailed);
            }
        };
        let master = to_master_key(opened)?;
        if !self.verify_header_mac(&master)? {
            self.record_failure();
            return Err(VaultError::Corrupt);
        }
        self.master = Some(master);
        self.record_success();
        tracing::info!("vault unlocked");
        Ok(())
    }

    pub fn unlock_with_recovery(&mut self, code: &str) -> VaultResult<()> {
        self.check_lockout()?;
        let normalized = kdf::normalize_recovery_code(code)?;
        let recovery = self.header.recovery.clone().ok_or(VaultError::NotFound)?;
        let key = kdf::derive(&normalized, &recovery.kdf)?;
        let opened = open(
            &key,
            &master_aad(self.header.vault_id, self.header.created_at),
            &recovery.wrapped_master_key,
        )
        .map_err(|_| {
            VaultError::AuthenticationFailed
        })?;
        let master = to_master_key(opened)?;
        if !self.verify_header_mac(&master)? {
            self.record_failure();
            return Err(VaultError::Corrupt);
        }
        self.master = Some(master);
        self.record_success();
        Ok(())
    }

    pub fn unlock_with_binding(&mut self, binder: &dyn KeyBinder) -> VaultResult<()> {
        self.check_lockout()?;
        let entry = self
            .header
            .bindings
            .iter()
            .find(|b| b.binder_id == binder.id())
            .ok_or(VaultError::NotFound)?
            .clone();
        let name = format!("RoFarm/{}", self.header.vault_id);
        let sealed = binder.unseal(&name, &entry.blob).map_err(|_| VaultError::BindingFailed)?;
        let master = match <[u8; 32]>::try_from(sealed.as_slice()) {
            Ok(m) => Zeroizing::new(m),
            Err(_) => return Err(VaultError::BindingFailed),
        };
        if !self.verify_header_mac(&master)? {
            self.record_failure();
            return Err(VaultError::Corrupt);
        }
        self.master = Some(master);
        self.record_success();
        Ok(())
    }

    pub fn unlock_with_master_key(&mut self, master: &[u8]) -> VaultResult<()> {
        let key = to_master_key(Zeroizing::new(master.to_vec()))?;
        if !self.verify_header_mac(&key)? {
            return Err(VaultError::Corrupt);
        }
        self.master = Some(key);
        self.record_success();
        Ok(())
    }

    fn verify_header_mac(&self, master: &[u8; 32]) -> VaultResult<bool> {
        let actual = header_mac(&self.header, master)?;
        Ok(constant_time_eq(self.header.header_mac.as_bytes(), actual.as_bytes()))
    }

    pub fn reauthenticate(&mut self, passphrase: &str) -> VaultResult<bool> {
        if !self.is_unlocked() {
            return Err(VaultError::Locked);
        }
        self.check_lockout()?;
        let kek = kdf::derive(passphrase, &self.header.kdf)?;
        let opened = open(
            &kek,
            &master_aad(self.header.vault_id, self.header.created_at),
            &self.header.wrapped_master_key,
        );
        match opened {
            Ok(bytes) => {
                let candidate = to_master_key(bytes)?;
                let master = self.master.as_ref().ok_or(VaultError::Locked)?;
                let equal = constant_time_eq(&candidate[..], &master[..]);
                if equal {
                    self.record_success();
                    Ok(true)
                } else {
                    self.record_failure();
                    Ok(false)
                }
            }
            Err(_) => {
                self.record_failure();
                Ok(false)
            }
        }
    }

    pub fn lock(&mut self) {
        self.master = None;
        self.data_keys.clear();
        tracing::info!("vault locked");
    }

    fn master(&self) -> VaultResult<&SecretKey> {
        self.master.as_ref().ok_or(VaultError::Locked)
    }

    fn persist(&mut self) -> VaultResult<()> {
        let master = self.master.as_ref().ok_or(VaultError::Locked)?;
        self.header.generation += 1;
        self.header.updated_at = Utc::now();
        self.header.header_mac = header_mac(&self.header, master)?;
        let file = VaultFile { header: self.header.clone(), records: self.records.clone() };
        let bytes = serde_json::to_vec(&file)?;
        atomic_write(&self.path, &bytes)?;
        Ok(())
    }

    pub fn save(&mut self) -> VaultResult<()> {
        self.persist()
    }

    pub fn add_binding(&mut self, binder: &dyn KeyBinder) -> VaultResult<()> {
        if !binder.available() {
            return Err(VaultError::BindingUnavailable);
        }
        let master = self.master.as_ref().ok_or(VaultError::Locked)?.clone();
        let name = format!("RoFarm/{}", self.header.vault_id);
        let blob = binder.seal(&name, master.as_ref())?;
        let entry = BindingEntry {
            binder_id: binder.id().to_string(),
            kind: binder.kind(),
            label: binder.label(),
            blob,
        };
        self.header.bindings.retain(|b| b.binder_id != entry.binder_id);
        self.header.bindings.push(entry);
        self.persist()
    }

    pub fn remove_binding(&mut self, binder_id: &str) -> VaultResult<()> {
        self.master()?;
        let before = self.header.bindings.len();
        self.header.bindings.retain(|b| b.binder_id != binder_id);
        if self.header.bindings.len() == before {
            return Err(VaultError::NotFound);
        }
        self.persist()?;
        Ok(())
    }

    pub fn set_recovery_code(&mut self, code: &str) -> VaultResult<()> {
        self.set_recovery_code_with_params(code, KdfParams::new_random(64 * 1024, 3, 4))
    }

    pub fn set_recovery_code_with_params(&mut self, code: &str, params: KdfParams) -> VaultResult<()> {
        let master = self.master.as_ref().ok_or(VaultError::Locked)?.clone();
        let normalized = kdf::normalize_recovery_code(code)?;
        params.validate()?;
        let key = kdf::derive(&normalized, &params)?;
        let wrapped = seal(&key, &master_aad(self.header.vault_id, self.header.created_at), master.as_ref())?;
        self.header.recovery = Some(RecoveryEntry { kdf: params, wrapped_master_key: wrapped });
        self.persist()
    }

    pub fn remove_recovery(&mut self) -> VaultResult<()> {
        self.master()?;
        self.header.recovery = None;
        self.persist()
    }

    pub fn create_recovery_shares(&self, threshold: u8, shares: u8) -> VaultResult<Vec<String>> {
        let master = self.master()?;
        let parts = kdf::split_secret(master.as_ref(), threshold, shares)?;
        Ok(parts.iter().map(kdf::encode_share).collect())
    }

    pub fn change_passphrase(&mut self, new_passphrase: &str, params: KdfParams) -> VaultResult<()> {
        if new_passphrase.chars().count() < 8 {
            return Err(VaultError::Invalid("Use a passphrase of at least 8 characters.".into()));
        }
        let master = self.master.as_ref().ok_or(VaultError::Locked)?.clone();
        params.validate()?;
        let kek = kdf::derive(new_passphrase, &params)?;
        self.header.wrapped_master_key =
            seal(&kek, &master_aad(self.header.vault_id, self.header.created_at), master.as_ref())?;
        self.header.kdf = params;
        self.persist()?;
        tracing::info!("vault passphrase changed");
        Ok(())
    }

    fn account_data_key(&mut self, account_id: AccountId) -> VaultResult<SecretKey> {
        if let Some(key) = self.data_keys.get(&account_id) {
            return Ok(key.clone());
        }
        let record_key = format!("account/{account_id}/__data_key");
        let master = self.master.as_ref().ok_or(VaultError::Locked)?.clone();
        if let Some(record) = self.records.get(&record_key) {
            self.verify_record(&record_key)?;
            let aad = record_aad(
                self.header.vault_id,
                self.header.created_at,
                &record_key,
                RecordKind::AccountDataKey,
                Some(account_id),
                record.generation,
            );
            let opened = open(&master, &aad, &record.sealed)?;
            let key = to_master_key(opened)?;
            self.data_keys.insert(account_id, key.clone());
            return Ok(key);
        }
        let key = crate::crypto::random_key();
        let generation = self.next_generation(&record_key);
        let aad = record_aad(
            self.header.vault_id,
            self.header.created_at,
            &record_key,
            RecordKind::AccountDataKey,
            Some(account_id),
            generation,
        );
        let sealed = seal(&master, &aad, key.as_ref())?;
        let now = Utc::now();
        self.records.insert(
            record_key,
            VaultRecord { kind: RecordKind::AccountDataKey, account_id: Some(account_id), generation, sealed, created_at: now, updated_at: now },
        );
        self.data_keys.insert(account_id, key.clone());
        Ok(key)
    }

    pub fn ensure_account_key(&mut self, account_id: AccountId) -> VaultResult<()> {
        let _ = self.account_data_key(account_id)?;
        self.persist()
    }

    pub fn put_account_secret(&mut self, account_id: AccountId, name: &str, value: &[u8]) -> VaultResult<()> {
        self.put_account_secret_with_kind(account_id, name, RecordKind::Cookie, value)
    }

    pub fn put_account_secret_with_kind(
        &mut self,
        account_id: AccountId,
        name: &str,
        kind: RecordKind,
        value: &[u8],
    ) -> VaultResult<()> {
        validate_secret_name(name)?;
        if value.is_empty() || value.len() > 16 * 1024 {
            return Err(VaultError::Invalid("Secret size is invalid.".into()));
        }
        let key = self.account_data_key(account_id)?;
        let record_key = format!("account/{account_id}/{name}");
        let generation = self.next_generation(&record_key);
        let aad = record_aad(
            self.header.vault_id,
            self.header.created_at,
            &record_key,
            kind,
            Some(account_id),
            generation,
        );
        let sealed = seal(&key, &aad, value)?;
        let now = Utc::now();
        let created_at = self.records.get(&record_key).map(|r| r.created_at).unwrap_or(now);
        self.records.insert(
            record_key,
            VaultRecord { kind, account_id: Some(account_id), generation, sealed, created_at, updated_at: now },
        );
        self.persist()
    }

    pub fn get_account_secret(&mut self, account_id: AccountId, name: &str) -> VaultResult<Zeroizing<Vec<u8>>> {
        let record_key = format!("account/{account_id}/{name}");
        let record = self.records.get(&record_key).cloned().ok_or(VaultError::NotFound)?;
        self.verify_record(&record_key)?;
        if record.account_id != Some(account_id) {
            return Err(VaultError::Corrupt);
        }
        let key = self.account_data_key(account_id)?;
        let aad = record_aad(
            self.header.vault_id,
            self.header.created_at,
            &record_key,
            record.kind,
            Some(account_id),
            record.generation,
        );
        open(&key, &aad, &record.sealed)
    }

    pub fn list_account_secrets(&self, account_id: AccountId) -> Vec<String> {
        let prefix = format!("account/{account_id}/");
        self.records
            .keys()
            .filter_map(|k| k.strip_prefix(&prefix).map(str::to_string))
            .filter(|n| n != "__data_key")
            .collect()
    }

    pub fn delete_account(&mut self, account_id: AccountId) -> VaultResult<u32> {
        self.master()?;
        let prefix = format!("account/{account_id}/");
        let keys: Vec<String> = self.records.keys().filter(|k| k.starts_with(&prefix)).cloned().collect();
        for key in &keys {
            self.records.remove(key);
            self.header.record_generations.remove(key);
        }
        self.data_keys.remove(&account_id);
        self.persist()?;
        Ok(keys.len() as u32)
    }

    pub fn put_generic_secret(&mut self, name: &str, kind: RecordKind, value: &[u8]) -> VaultResult<()> {
        validate_secret_name(name)?;
        if matches!(kind, RecordKind::AccountDataKey) {
            return Err(VaultError::Invalid("Use the account API for data keys.".into()));
        }
        let master = self.master.as_ref().ok_or(VaultError::Locked)?.clone();
        let record_key = format!("global/{name}");
        let generation = self.next_generation(&record_key);
        let aad = record_aad(self.header.vault_id, self.header.created_at, &record_key, kind, None, generation);
        let sealed = seal(&master, &aad, value)?;
        let now = Utc::now();
        let created_at = self.records.get(&record_key).map(|r| r.created_at).unwrap_or(now);
        self.records.insert(
            record_key,
            VaultRecord { kind, account_id: None, generation, sealed, created_at, updated_at: now },
        );
        self.persist()
    }

    pub fn get_generic_secret(&self, name: &str) -> VaultResult<Zeroizing<Vec<u8>>> {
        let master = self.master.as_ref().ok_or(VaultError::Locked)?.clone();
        let record_key = format!("global/{name}");
        let record = self.records.get(&record_key).cloned().ok_or(VaultError::NotFound)?;
        self.verify_record(&record_key)?;
        let aad = record_aad(self.header.vault_id, self.header.created_at, &record_key, record.kind, None, record.generation);
        open(&master, &aad, &record.sealed)
    }

    pub fn delete_generic_secret(&mut self, name: &str) -> VaultResult<()> {
        self.master()?;
        let record_key = format!("global/{name}");
        if self.records.remove(&record_key).is_none() {
            return Err(VaultError::NotFound);
        }
        self.header.record_generations.remove(&record_key);
        self.persist()
    }

    pub fn secret_count(&self) -> usize {
        self.records.len()
    }

    pub fn has_secret(&self, name: &str) -> bool {
        self.records.contains_key(&format!("global/{name}"))
    }

    fn next_generation(&mut self, key: &str) -> u64 {
        let next = self.header.record_generations.get(key).copied().unwrap_or(0) + 1;
        self.header.record_generations.insert(key.to_string(), next);
        next
    }

    fn verify_record(&self, key: &str) -> VaultResult<()> {
        let record = self.records.get(key).ok_or(VaultError::NotFound)?;
        let expected = self.header.record_generations.get(key).copied().ok_or(VaultError::Corrupt)?;
        if record.generation != expected {
            return Err(VaultError::Corrupt);
        }
        Ok(())
    }
}

fn to_master_key(opened: Zeroizing<Vec<u8>>) -> VaultResult<SecretKey> {
    if opened.len() != 32 {
        return Err(VaultError::Corrupt);
    }
    let mut key = Zeroizing::new([0u8; 32]);
    key.copy_from_slice(&opened);
    Ok(key)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> VaultResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut tmp = path.to_path_buf();
    let tmp_name = format!(
        "{}.tmp-{}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("vault"),
        std::process::id()
    );
    tmp.set_file_name(tmp_name);
    {
        use std::io::Write;
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    #[cfg(windows)]
    {
        if fs::rename(&tmp, path).is_err() {
            let _ = fs::remove_file(path);
            fs::rename(&tmp, path)?;
        }
    }
    #[cfg(not(windows))]
    {
        fs::rename(&tmp, path)?;
    }
    Ok(())
}

fn lockout_path_for(path: &Path) -> PathBuf {
    let mut p = path.to_path_buf();
    let name = format!(
        "{}.lockout.json",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("vault")
    );
    p.set_file_name(name);
    p
}

fn read_lockout(path: &Path) -> VaultResult<LockoutFile> {
    let bytes = fs::read(path)?;
    serde_json::from_slice(&bytes).map_err(|_| VaultError::Corrupt)
}

fn write_lockout(path: &Path, lockout: &LockoutFile) -> VaultResult<()> {
    let bytes = serde_json::to_vec(lockout)?;
    atomic_write(path, &bytes)
}

pub fn migrate_v1(src: &Path, passphrase: &str, dest: &Path, params: KdfParams) -> VaultResult<Vault> {
    #[derive(Deserialize)]
    struct V1Record {
        nonce: String,
        ciphertext: String,
    }
    #[derive(Deserialize)]
    struct V1 {
        version: u16,
        #[serde(rename = "salt")]
        salt_b64: String,
        m_cost_kib: u32,
        t_cost: u32,
        p_cost: u32,
        wrapped_key: Sealed,
        records: BTreeMap<String, V1Record>,
    }
    let bytes = fs::read(src)?;
    if bytes.len() as u64 > MAX_VAULT_BYTES {
        return Err(VaultError::Corrupt);
    }
    let v1: V1 = serde_json::from_slice(&bytes).map_err(|_| VaultError::Corrupt)?;
    if v1.version != 1 {
        return Err(VaultError::UnsupportedVersion(v1.version));
    }
    let legacy_params = KdfParams {
        algorithm: "argon2id".into(),
        m_cost_kib: v1.m_cost_kib,
        t_cost: v1.t_cost,
        p_cost: v1.p_cost,
        salt: b64_decode(&v1.salt_b64).map_err(|_| VaultError::Corrupt)?,
    };
    let kek = kdf::derive(passphrase, &legacy_params).map_err(|_| VaultError::AuthenticationFailed)?;
    let master = to_master_key(open(&kek, b"", &v1.wrapped_key).map_err(|_| VaultError::AuthenticationFailed)?)?;
    let mut vault = Vault::create(dest, passphrase, params)?;
    for (key, legacy) in v1.records {
        let sealed = Sealed { nonce: legacy.nonce, ciphertext: legacy.ciphertext };
        let plaintext = open(&master, b"", &sealed).map_err(|_| VaultError::Corrupt)?;
        let safe_key = format!("global/migrated-{}", blake3::hash(key.as_bytes()).to_hex());
        let generation = vault.next_generation(&safe_key);
        let aad = record_aad(
            vault.header.vault_id,
            vault.header.created_at,
            &safe_key,
            RecordKind::Generic,
            None,
            generation,
        );
        let reencrypted = seal(vault.master.as_ref().ok_or(VaultError::Locked)?, &aad, &plaintext)?;
        let now = Utc::now();
        vault.records.insert(
            safe_key,
            VaultRecord { kind: RecordKind::Generic, account_id: None, generation, sealed: reencrypted, created_at: now, updated_at: now },
        );
    }
    vault.persist()?;
    tracing::info!("migrated v1 vault to v2");
    Ok(vault)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::b64_encode;
    use crate::binding::MockBinder;
    use crate::kdf::generate_recovery_code;

    fn vault() -> (tempfile::TempDir, Vault) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.rofarm");
        let pass = "correct horse battery staple";
        let v = Vault::create(&path, pass, KdfParams::testing()).unwrap();
        (dir, v)
    }

    #[test]
    fn create_unlock_wrong_passphrase_and_lock() {
        let (dir, mut v) = vault();
        let path = dir.path().join("test.rofarm");
        v.lock();
        assert!(!v.is_unlocked());
        assert!(matches!(v.get_generic_secret("x"), Err(VaultError::Locked)));
        v.unlock("wrong").unwrap_err();
        v.unlock("correct horse battery staple").unwrap();
        assert!(v.is_unlocked());
        drop(v);
        let reopened = Vault::open_file(&path);
        assert!(reopened.is_ok());
        let mut reopened = reopened.unwrap();
        assert!(!reopened.is_unlocked());
        reopened.unlock("correct horse battery staple").unwrap();
        assert!(reopened.generation() > 0);
    }

    #[test]
    fn account_secret_round_trip_and_crypto_shredding() {
        let (_dir, mut v) = vault();
        let id = AccountId::new();
        v.put_account_secret(id, "cookie", b"_|WARNING:-DO-NOT-SHARE|_secret").unwrap();
        assert_eq!(
            &v.get_account_secret(id, "cookie").unwrap()[..],
            b"_|WARNING:-DO-NOT-SHARE|_secret"
        );
        assert_eq!(v.list_account_secrets(id), vec!["cookie".to_string()]);
        assert_eq!(v.delete_account(id).unwrap(), 2);
        assert!(matches!(v.get_account_secret(id, "cookie"), Err(VaultError::NotFound)));
    }

    #[test]
    fn generic_secrets_and_kinds() {
        let (_dir, mut v) = vault();
        v.put_generic_secret("discord.1", RecordKind::NotifierCredential, b"https://discord.com/api/webhooks/x").unwrap();
        assert!(v.has_secret("discord.1"));
        assert_eq!(
            &v.get_generic_secret("discord.1").unwrap()[..],
            b"https://discord.com/api/webhooks/x"
        );
        v.delete_generic_secret("discord.1").unwrap();
        assert!(!v.has_secret("discord.1"));
        assert!(v.put_generic_secret("x", RecordKind::AccountDataKey, b"no").is_err());
        assert!(v.put_generic_secret("bad name", RecordKind::Generic, b"x").is_err());
    }

    #[test]
    fn passphrase_change_keeps_records_readable() {
        let (_dir, mut v) = vault();
        let id = AccountId::new();
        v.put_account_secret(id, "cookie", b"secret-value").unwrap();
        v.change_passphrase("a brand new passphrase", KdfParams::testing()).unwrap();
        v.lock();
        assert!(v.unlock("correct horse battery staple").is_err());
        v.unlock("a brand new passphrase").unwrap();
        assert_eq!(&v.get_account_secret(id, "cookie").unwrap()[..], b"secret-value");
    }

    #[test]
    fn recovery_code_unlocks() {
        let (_dir, mut v) = vault();
        let code = generate_recovery_code();
        v.set_recovery_code_with_params(&code, KdfParams::testing()).unwrap();
        v.lock();
        assert!(v.unlock_with_recovery("AAAAAAAAA").is_err());
        v.unlock_with_recovery(&code).unwrap();
        assert!(v.is_unlocked());
        assert!(v.has_recovery());
        let bad = format!("{}0", &code[..8]);
        v.lock();
        assert!(v.unlock_with_recovery(&bad).is_err());
    }

    #[test]
    fn binding_unlocks_without_passphrase() {
        let (_dir, mut v) = vault();
        let binder = MockBinder::new();
        v.add_binding(&binder).unwrap();
        assert_eq!(v.available_bindings().len(), 1);
        v.lock();
        v.unlock_with_binding(&binder).unwrap();
        assert!(v.is_unlocked());
        let unavailable = crate::binding::Fido2Binder;
        assert!(matches!(v.add_binding(&unavailable), Err(VaultError::BindingUnavailable)));
    }

    #[test]
    fn master_key_shares_reconstruct_and_unlock() {
        let (_dir, mut v) = vault();
        let shares = v.create_recovery_shares(2, 3).unwrap();
        let decoded: Vec<_> = shares.iter().map(|s| crate::kdf::decode_share(s).unwrap()).collect();
        let master = crate::kdf::combine_shares(&[decoded[0].clone(), decoded[2].clone()]).unwrap();
        v.lock();
        v.unlock_with_master_key(&master).unwrap();
        assert!(v.is_unlocked());
    }

    #[test]
    fn tampering_with_header_or_records_is_detected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v.rofarm");
        let mut v = Vault::create(&path, "correct horse battery staple", KdfParams::testing()).unwrap();
        let id = AccountId::new();
        v.put_account_secret(id, "cookie", b"top secret").unwrap();
        v.save().unwrap();
        drop(v);
        let raw = fs::read_to_string(&path).unwrap();
        let mut file: VaultFile = serde_json::from_str(&raw).unwrap();
        file.header.generation += 1;
        fs::write(&path, serde_json::to_vec(&file).unwrap()).unwrap();
        let mut v = Vault::open_file(&path).unwrap();
        assert!(matches!(
            v.unlock("correct horse battery staple"),
            Err(VaultError::Corrupt) | Err(VaultError::AuthenticationFailed)
        ));

        fs::write(&path, &raw).unwrap();
        let mut file: VaultFile = serde_json::from_str(&raw).unwrap();
        let key = format!("account/{id}/cookie");
        let mut c = b64_decode(&file.records[&key].sealed.ciphertext).unwrap();
        c[0] ^= 1;
        file.records.get_mut(&key).unwrap().sealed.ciphertext = b64_encode(&c);
        fs::write(&path, serde_json::to_vec(&file).unwrap()).unwrap();
        let mut v = Vault::open_file(&path).unwrap();
        v.unlock("correct horse battery staple").unwrap();
        assert!(matches!(v.get_account_secret(id, "cookie"), Err(VaultError::AuthenticationFailed)));
    }

    #[test]
    fn record_rollback_is_rejected_via_generation_map() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v.rofarm");
        let mut v = Vault::create(&path, "correct horse battery staple", KdfParams::testing()).unwrap();
        let id = AccountId::new();
        v.put_account_secret(id, "cookie", b"first").unwrap();
        v.put_account_secret(id, "cookie", b"second").unwrap();
        let stale_record = v.records.get(&format!("account/{id}/cookie")).unwrap().clone();
        v.put_account_secret(id, "cookie", b"third").unwrap();
        drop(v);
        let raw = fs::read_to_string(&path).unwrap();
        let mut file: VaultFile = serde_json::from_str(&raw).unwrap();
        let key = format!("account/{id}/cookie");
        file.records.insert(key.clone(), stale_record);
        fs::write(&path, serde_json::to_vec(&file).unwrap()).unwrap();
        let mut v = Vault::open_file(&path).unwrap();
        v.unlock("correct horse battery staple").unwrap();
        assert!(matches!(v.get_account_secret(id, "cookie"), Err(VaultError::Corrupt)));
    }

    #[test]
    fn lockout_backs_off_after_failed_attempts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v.rofarm");
        let v = Vault::create(&path, "correct horse battery staple", KdfParams::testing()).unwrap();
        drop(v);
        let mut v = Vault::open_file(&path).unwrap();
        for _ in 0..5 {
            assert!(v.unlock("nope").is_err());
        }
        assert!(matches!(v.unlock("correct horse battery staple"), Err(VaultError::TooManyAttempts(_))));
        assert!(v.lockout_remaining().is_some());
        let mut lock = read_lockout(&lockout_path_for(&path)).unwrap();
        lock.locked_until = Some(Utc::now() - Duration::seconds(1));
        write_lockout(&lockout_path_for(&path), &lock).unwrap();
        let mut v = Vault::open_file(&path).unwrap();
        v.unlock("correct horse battery staple").unwrap();
        assert!(v.is_unlocked());
        assert!(read_lockout(&lockout_path_for(&path)).is_err());
    }

    #[test]
    fn rejects_bad_passphrase_and_missing_vault() {
        let dir = tempfile::tempdir().unwrap();
        assert!(Vault::create(dir.path().join("x"), "short", KdfParams::testing()).is_err());
        assert!(Vault::open_file(dir.path().join("missing")).is_err());
        let (dir, _v) = vault();
        assert!(Vault::create(dir.path().join("test.rofarm"), "correct horse battery staple", KdfParams::testing()).is_err());
    }

    #[test]
    fn v1_migration_reencrypts_records() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("legacy.json");
        let dest = dir.path().join("new.rofarm");
        let legacy_params = KdfParams::testing();
        let kek = kdf::derive("correct horse battery staple", &legacy_params).unwrap();
        let master = crate::crypto::random_key();
        let wrapped = seal(&kek, b"", master.as_ref()).unwrap();
        let secret = seal(&master, b"", b"legacy cookie").unwrap();
        let legacy = serde_json::json!({
            "version": 1,
            "salt": b64_encode(&legacy_params.salt),
            "m_cost_kib": legacy_params.m_cost_kib,
            "t_cost": legacy_params.t_cost,
            "p_cost": legacy_params.p_cost,
            "wrapped_key": wrapped,
            "records": { "account/old/cookie": secret },
        });
        fs::write(&src, serde_json::to_vec(&legacy).unwrap()).unwrap();
        let migrated = migrate_v1(&src, "correct horse battery staple", &dest, KdfParams::testing()).unwrap();
        assert_eq!(migrated.secret_count(), 1);
        assert!(dest.exists());
        let mut reopened = Vault::open_file(&dest).unwrap();
        assert!(reopened.unlock("correct horse battery staple").is_ok());
    }
}
