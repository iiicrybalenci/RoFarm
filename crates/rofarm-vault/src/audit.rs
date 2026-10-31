use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use crate::error::{VaultError, VaultResult};

pub const AUDIT_GENESIS: &str = "0000000000000000000000000000000000000000000000000000000000000000";
pub const MAX_AUDIT_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditKind {
    Unlock,
    Lock,
    UnlockFailed,
    Reveal,
    Copy,
    Export,
    Restore,
    BackupCreated,
    BackupVerified,
    BindingAdded,
    BindingRemoved,
    RecoveryIssued,
    PassphraseChanged,
    RemotePairing,
    RemoteLogin,
    RemoteLoginFailed,
    RemoteRevoked,
    PluginPermission,
    AutomationChanged,
    KillAll,
    UpdateApplied,
    UpdateRolledBack,
    SettingsChanged,
    Reauth,
    ImportCompleted,
    VaultCreated,
    SecretWritten,
    ClientLaunched,
    FleetKilled,
}

impl AuditKind {
    pub fn requires_reauth(&self) -> bool {
        matches!(self, AuditKind::Reveal | AuditKind::Copy | AuditKind::Export | AuditKind::Restore)
    }

    pub fn severity(&self) -> &'static str {
        match self {
            AuditKind::UnlockFailed | AuditKind::Lock | AuditKind::KillAll | AuditKind::UpdateRolledBack => "warning",
            AuditKind::Reveal | AuditKind::Export | AuditKind::Restore | AuditKind::PassphraseChanged | AuditKind::SecretWritten => "critical",
            _ => "info",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub seq: u64,
    pub at: DateTime<Utc>,
    pub kind: AuditKind,
    pub subject: String,
    pub detail: String,
    pub prev_hash: String,
    pub hash: String,
}

#[derive(Serialize)]
struct HashPayload<'a> {
    seq: u64,
    at_micros: i64,
    kind: AuditKind,
    subject: &'a str,
    detail: &'a str,
    prev_hash: &'a str,
}

impl AuditEntry {
    fn compute(seq: u64, at: DateTime<Utc>, kind: AuditKind, subject: &str, detail: &str, prev: &str) -> String {
        let payload = HashPayload { seq, at_micros: at.timestamp_micros(), kind, subject, detail, prev_hash: prev };
        let bytes = serde_json::to_vec(&payload).expect("audit hash serialization");
        blake3::hash(&bytes).to_hex().to_string()
    }

    pub fn verify(&self) -> bool {
        self.hash == Self::compute(self.seq, self.at, self.kind, &self.subject, &self.detail, &self.prev_hash)
    }
}

fn sanitize(text: &str) -> VaultResult<String> {
    let lower = text.to_ascii_lowercase();
    if text.contains("_|WARNING")
        || lower.contains("roblosecurity")
        || lower.contains("secret=")
        || lower.contains("token=")
        || lower.contains("password=")
        || lower.contains("api/webhooks/")
        || text.starts_with("eyJ")
    {
        return Err(VaultError::Invalid("Sensitive material cannot be written to the audit log.".into()));
    }
    if text.len() > 1024 {
        return Err(VaultError::Invalid("Audit detail is too long.".into()));
    }
    Ok(text.to_string())
}

pub struct AuditLog {
    path: PathBuf,
    entries: Vec<AuditEntry>,
}

impl std::fmt::Debug for AuditLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuditLog").field("path", &self.path).field("entries", &self.entries.len()).finish()
    }
}

impl AuditLog {
    pub fn in_memory() -> Self {
        Self { path: PathBuf::from("<memory>"), entries: Vec::new() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn open(path: impl AsRef<Path>) -> VaultResult<Self> {
        let path = path.as_ref().to_path_buf();
        let mut entries = Vec::new();
        if path.exists() {
            let meta = std::fs::metadata(&path)?;
            if meta.len() > MAX_AUDIT_BYTES {
                return Err(VaultError::Corrupt);
            }
            let file = File::open(&path)?;
            for (idx, line) in BufReader::new(file).lines().enumerate() {
                let line = line?;
                if line.trim().is_empty() {
                    continue;
                }
                let entry: AuditEntry = serde_json::from_str(&line).map_err(|_| VaultError::Corrupt)?;
                if entry.seq != idx as u64 + 1 {
                    return Err(VaultError::Corrupt);
                }
                entries.push(entry);
            }
            verify_entries(&entries)?;
        }
        Ok(Self { path, entries })
    }

    pub fn append(&mut self, kind: AuditKind, subject: &str, detail: &str) -> VaultResult<()> {
        if self.path.as_os_str() == "<memory>" {
            let subject = sanitize(subject)?;
            let detail = sanitize(detail)?;
            let seq = self.entries.len() as u64 + 1;
            let at = Utc::now();
            let prev = self.entries.last().map(|e| e.hash.clone()).unwrap_or_else(|| AUDIT_GENESIS.to_string());
            let hash = AuditEntry::compute(seq, at, kind, &subject, &detail, &prev);
            self.entries.push(AuditEntry { seq, at, kind, subject, detail, prev_hash: prev, hash });
            return Ok(());
        }
        let subject = sanitize(subject)?;
        let detail = sanitize(detail)?;
        let seq = self.entries.len() as u64 + 1;
        let at = Utc::now();
        let prev = self.entries.last().map(|e| e.hash.clone()).unwrap_or_else(|| AUDIT_GENESIS.to_string());
        let hash = AuditEntry::compute(seq, at, kind, &subject, &detail, &prev);
        let entry = AuditEntry { seq, at, kind, subject, detail, prev_hash: prev, hash };
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new().create(true).append(true).open(&self.path)?;
        serde_json::to_writer(&mut file, &entry)?;
        file.write_all(b"\n")?;
        file.sync_data()?;
        self.entries.push(entry);
        Ok(())
    }

    pub fn entries(&self) -> &[AuditEntry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn verify(&self) -> VaultResult<()> {
        verify_entries(&self.entries)
    }

    pub fn integrity_check(&self) -> Option<usize> {
        verify_entries(&self.entries).err().map(|_| {
            self.entries
                .iter()
                .enumerate()
                .find(|(idx, e)| {
                    let prev = if *idx == 0 { AUDIT_GENESIS.to_string() } else { self.entries[idx - 1].hash.clone() };
                    e.prev_hash != prev || !e.verify()
                })
                .map(|(idx, _)| idx + 1)
                .unwrap_or(0)
        })
    }

    pub fn export_redacted(&self) -> String {
        let mut out = String::from("# RoFarm audit export (redacted subjects)\n");
        for e in &self.entries {
            out.push_str(&format!(
                "{} {:<22} {:<10} {}\n",
                e.at.to_rfc3339(),
                format!("{:?}", e.kind),
                e.subject.chars().take(8).collect::<String>(),
                e.detail
            ));
        }
        out
    }

    pub fn detect_truncation(&self, last_known_seq: u64) -> bool {
        let actual = self.entries.last().map(|e| e.seq).unwrap_or(0);
        actual < last_known_seq
    }
}

fn verify_entries(entries: &[AuditEntry]) -> VaultResult<()> {
    let mut prev = AUDIT_GENESIS.to_string();
    for (idx, entry) in entries.iter().enumerate() {
        if entry.seq != idx as u64 + 1 || entry.prev_hash != prev || !entry.verify() {
            return Err(VaultError::Corrupt);
        }
        prev = entry.hash.clone();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_chain_round_trip_and_tamper_detection() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audit.jsonl");
        {
            let mut log = AuditLog::open(&path).unwrap();
            log.append(AuditKind::VaultCreated, "vault", "created").unwrap();
            log.append(AuditKind::Unlock, "vault", "passphrase").unwrap();
            log.append(AuditKind::Reveal, "account-123", "cookie").unwrap();
            assert_eq!(log.len(), 3);
            log.verify().unwrap();
            assert_eq!(log.integrity_check(), None);
            assert!(!log.detect_truncation(3));
        }
        let log = AuditLog::open(&path).unwrap();
        assert_eq!(log.len(), 3);
        assert!(log.entries()[2].verify());
        let text = std::fs::read_to_string(&path).unwrap();
        let tampered = text.replacen("\"detail\":\"cookie\"", "\"detail\":\"other\"", 1);
        std::fs::write(&path, tampered).unwrap();
        assert!(AuditLog::open(&path).is_err());
    }

    #[test]
    fn secrets_are_rejected_from_audit_details() {
        let dir = tempfile::tempdir().unwrap();
        let mut log = AuditLog::open(dir.path().join("a.jsonl")).unwrap();
        for bad in [
            "cookie _|WARNING:-DO-NOT-SHARE|_abcdef",
            "ROBLOSECURITY=abc",
            "token=abc",
            "https://discord.com/api/webhooks/123/abc",
            "eyJhbGciOiJIUzI1NiJ9.payload.sig",
        ] {
            assert!(log.append(AuditKind::Export, "test", bad).is_err(), "{bad}");
        }
        log.append(AuditKind::Export, "test", "3 accounts exported").unwrap();
        assert_eq!(log.len(), 1);
    }

    #[test]
    fn truncation_and_redacted_export() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audit.jsonl");
        let mut log = AuditLog::open(&path).unwrap();
        log.append(AuditKind::Unlock, "Builderman", "ok").unwrap();
        log.append(AuditKind::Lock, "Builderman", "idle").unwrap();
        assert!(log.detect_truncation(3));
        assert!(!log.detect_truncation(2));
        let text = log.export_redacted();
        assert!(!text.contains("Builderman"));
        assert!(text.contains("Build"));
        assert!(AuditKind::Reveal.requires_reauth());
    }

    #[test]
    fn entry_requires_correct_prev_hash() {
        let at = Utc::now();
        let hash = AuditEntry::compute(1, at, AuditKind::Unlock, "s", "d", AUDIT_GENESIS);
        let entry = AuditEntry {
            seq: 1,
            at,
            kind: AuditKind::Unlock,
            subject: "s".into(),
            detail: "d".into(),
            prev_hash: AUDIT_GENESIS.into(),
            hash,
        };
        assert!(entry.verify());
        let mut forged = entry.clone();
        forged.prev_hash = "ff".repeat(32);
        assert!(!forged.verify());
    }
}
