use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::crypto::{b64_decode, b64_encode, open, seal, Sealed};
use crate::error::{VaultError, VaultResult};
use crate::kdf::{self, KdfParams};

pub const BACKUP_MAGIC: &[u8; 6] = b"ROFBK1";
pub const ARCHIVE_MAGIC: &[u8; 7] = b"ROFARC1";
pub const DEFAULT_CHUNK_SIZE: usize = 1024 * 1024;
pub const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_FILES: usize = 50_000;
pub const MAX_PATH_LEN: usize = 240;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupHeader {
    pub schema_version: u16,
    pub format: String,
    pub app_version: String,
    pub created_at: DateTime<Utc>,
    pub kdf: KdfParams,
    pub chunk_size: u32,
    pub total_size: u64,
    pub total_chunks: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupFileEntry {
    pub path: String,
    pub size: u64,
    pub blake3: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackupInfo {
    pub header: BackupHeader,
    pub files: Vec<BackupFileEntry>,
    pub chunks: u32,
    pub bytes_read: u64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BackupArchive {
    files: BTreeMap<String, Vec<u8>>,
}

pub fn validate_relative_path(path: &str) -> Result<(), VaultError> {
    if path.is_empty() || path.len() > MAX_PATH_LEN {
        return Err(VaultError::UnsafePath("path length is invalid".into()));
    }
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(VaultError::UnsafePath("absolute path".into()));
    }
    if path.contains('\0') || path.chars().any(|c| (c as u32) < 32) {
        return Err(VaultError::UnsafePath("control character".into()));
    }
    if path.contains(':') || path.starts_with("\\\\") {
        return Err(VaultError::UnsafePath("drive or UNC path".into()));
    }
    let parts: Vec<&str> = path.split(['/', '\\']).collect();
    if parts.iter().any(|p| p.is_empty() || *p == "." || *p == "..") {
        return Err(VaultError::UnsafePath("path traversal or empty segment".into()));
    }
    const RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
        "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    for part in parts {
        let stem = part.split('.').next().unwrap_or(part).to_ascii_uppercase();
        if RESERVED.contains(&stem.as_str()) {
            return Err(VaultError::UnsafePath("Windows reserved name".into()));
        }
    }
    Ok(())
}

impl BackupArchive {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, path: impl Into<String>, data: Vec<u8>) -> Result<(), VaultError> {
        let path = path.into();
        validate_relative_path(&path)?;
        if self.files.contains_key(&path) {
            return Err(VaultError::UnsafePath("duplicate path".into()));
        }
        if self.files.len() >= MAX_FILES {
            return Err(VaultError::BackupInvalid("too many files".into()));
        }
        let total: u64 = self.files.values().map(|v| v.len() as u64).sum::<u64>() + data.len() as u64;
        if total > MAX_ARCHIVE_BYTES {
            return Err(VaultError::BackupInvalid("archive is too large".into()));
        }
        self.files.insert(path, data);
        Ok(())
    }

    pub fn files(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.files
    }

    pub fn total_size(&self) -> u64 {
        self.files.values().map(|v| v.len() as u64).sum()
    }

    pub fn from_directory(root: impl AsRef<Path>, include: impl Fn(&Path) -> bool) -> VaultResult<Self> {
        let root = root.as_ref();
        let mut archive = Self::new();
        fn walk(
            root: &Path,
            dir: &Path,
            include: &impl Fn(&Path) -> bool,
            archive: &mut BackupArchive,
        ) -> VaultResult<()> {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if entry.file_type()?.is_dir() {
                    walk(root, &path, include, archive)?;
                } else if include(&path) {
                    let rel = path.strip_prefix(root).map_err(|_| VaultError::UnsafePath("path escape".into()))?;
                    let rel = rel.to_string_lossy().replace('\\', "/");
                    archive.insert(rel, fs::read(&path)?)?;
                }
            }
            Ok(())
        }
        walk(root, root, &include, &mut archive)?;
        Ok(archive)
    }
}

fn archive_bytes(archive: &BackupArchive) -> VaultResult<Vec<u8>> {
    let mut out = Vec::with_capacity(archive.total_size() as usize + 1024);
    out.extend_from_slice(ARCHIVE_MAGIC);
    out.extend_from_slice(&(archive.files.len() as u32).to_le_bytes());
    for (path, data) in &archive.files {
        validate_relative_path(path)?;
        let path_bytes = path.as_bytes();
        out.extend_from_slice(&(path_bytes.len() as u16).to_le_bytes());
        out.extend_from_slice(path_bytes);
        out.extend_from_slice(&(data.len() as u64).to_le_bytes());
        out.extend_from_slice(data);
        out.extend_from_slice(blake3::hash(data).as_bytes());
    }
    Ok(out)
}

fn parse_archive(bytes: &[u8]) -> VaultResult<BackupArchive> {
    if bytes.len() < 11 || &bytes[..7] != ARCHIVE_MAGIC {
        return Err(VaultError::BackupInvalid("archive magic".into()));
    }
    if bytes.len() as u64 > MAX_ARCHIVE_BYTES + 1024 * 1024 {
        return Err(VaultError::BackupInvalid("archive too large".into()));
    }
    let mut archive = BackupArchive::new();
    let mut o = 7usize;
    let count = u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap()) as usize;
    o += 4;
    if count > MAX_FILES {
        return Err(VaultError::BackupInvalid("too many files".into()));
    }
    for _ in 0..count {
        if o + 2 > bytes.len() {
            return Err(VaultError::BackupInvalid("truncated path length".into()));
        }
        let path_len = u16::from_le_bytes(bytes[o..o + 2].try_into().unwrap()) as usize;
        o += 2;
        if path_len == 0 || path_len > MAX_PATH_LEN || o + path_len > bytes.len() {
            return Err(VaultError::BackupInvalid("bad path length".into()));
        }
        let path = std::str::from_utf8(&bytes[o..o + path_len])
            .map_err(|_| VaultError::BackupInvalid("path is not UTF-8".into()))?
            .to_string();
        o += path_len;
        if o + 8 > bytes.len() {
            return Err(VaultError::BackupInvalid("truncated file size".into()));
        }
        let size = u64::from_le_bytes(bytes[o..o + 8].try_into().unwrap()) as usize;
        o += 8;
        if size > MAX_ARCHIVE_BYTES as usize || o + size + 32 > bytes.len() {
            return Err(VaultError::BackupInvalid("bad file size".into()));
        }
        let data = bytes[o..o + size].to_vec();
        o += size;
        let expected = &bytes[o..o + 32];
        o += 32;
        if blake3::hash(&data).as_bytes() != expected {
            return Err(VaultError::BackupInvalid("file checksum mismatch".into()));
        }
        archive.insert(path, data)?;
    }
    if o != bytes.len() {
        return Err(VaultError::BackupInvalid("trailing archive data".into()));
    }
    Ok(archive)
}

fn header_aad(header: &BackupHeader) -> VaultResult<Vec<u8>> {
    let bytes = serde_json::to_vec(header)?;
    Ok(blake3::hash(&bytes).as_bytes().to_vec())
}

pub fn write_backup(
    dest: impl AsRef<Path>,
    passphrase: &str,
    archive: &BackupArchive,
    app_version: &str,
    params: KdfParams,
) -> VaultResult<BackupInfo> {
    if passphrase.chars().count() < 8 {
        return Err(VaultError::Invalid("Backup passphrase must be at least 8 characters.".into()));
    }
    params.validate()?;
    let payload = archive_bytes(archive)?;
    if payload.len() as u64 > MAX_ARCHIVE_BYTES {
        return Err(VaultError::BackupInvalid("payload too large".into()));
    }
    let chunk_size = DEFAULT_CHUNK_SIZE;
    let total_chunks = payload.len().div_ceil(chunk_size) as u32;
    let header = BackupHeader {
        schema_version: 1,
        format: "rofarm-archive-1".into(),
        app_version: app_version.into(),
        created_at: Utc::now(),
        kdf: params.clone(),
        chunk_size: chunk_size as u32,
        total_size: payload.len() as u64,
        total_chunks,
    };
    let key = kdf::derive(passphrase, &params)?;
    let header_bytes = serde_json::to_vec(&header)?;
    let aad_base = header_aad(&header)?;
    let mut out = Vec::with_capacity(header_bytes.len() + payload.len() + 64 + total_chunks as usize * 28);
    out.extend_from_slice(BACKUP_MAGIC);
    out.push(1u8);
    out.extend_from_slice(&(header_bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(&header_bytes);
    for (idx, chunk) in payload.chunks(chunk_size).enumerate() {
        let mut aad = aad_base.clone();
        aad.extend_from_slice(&(idx as u64).to_le_bytes());
        aad.extend_from_slice(&header.total_size.to_le_bytes());
        let sealed = seal(&key, &aad, chunk)?;
        let nonce = b64_decode(&sealed.nonce).map_err(|_| VaultError::Crypto)?;
        let ct = b64_decode(&sealed.ciphertext).map_err(|_| VaultError::Crypto)?;
        out.extend_from_slice(&(ct.len() as u32).to_le_bytes());
        out.extend_from_slice(&(idx as u64).to_le_bytes());
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ct);
    }
    write_atomic(dest.as_ref(), &out)?;
    let files = archive
        .files
        .iter()
        .map(|(path, data)| BackupFileEntry { path: path.clone(), size: data.len() as u64, blake3: blake3::hash(data).to_hex().to_string() })
        .collect();
    Ok(BackupInfo { header, files, chunks: total_chunks, bytes_read: out.len() as u64 })
}

fn parse_header(src: &[u8]) -> VaultResult<(BackupHeader, usize)> {
    if src.len() < 11 || &src[..6] != BACKUP_MAGIC {
        return Err(VaultError::BackupInvalid("not a RoFarm backup".into()));
    }
    if src[6] != 1 {
        return Err(VaultError::BackupInvalid("unsupported backup version".into()));
    }
    let header_len = u32::from_le_bytes(src[7..11].try_into().unwrap()) as usize;
    if header_len == 0 || header_len > 64 * 1024 || 11 + header_len > src.len() {
        return Err(VaultError::BackupInvalid("bad header length".into()));
    }
    let header: BackupHeader = serde_json::from_slice(&src[11..11 + header_len])
        .map_err(|_| VaultError::BackupInvalid("bad header".into()))?;
    header.kdf.validate()?;
    if header.chunk_size == 0 || header.chunk_size > 16 * 1024 * 1024 {
        return Err(VaultError::BackupInvalid("bad chunk size".into()));
    }
    if header.total_size > MAX_ARCHIVE_BYTES {
        return Err(VaultError::BackupInvalid("backup is too large".into()));
    }
    if header.total_chunks as u64 != header.total_size.div_ceil(header.chunk_size as u64) {
        return Err(VaultError::BackupInvalid("chunk count mismatch".into()));
    }
    Ok((header, 11 + header_len))
}

fn decrypt_payload(src: &[u8], header: &BackupHeader, passphrase: &str) -> VaultResult<Vec<u8>> {
    let key = kdf::derive(passphrase, &header.kdf).map_err(|_| VaultError::BackupAuthenticationFailed)?;
    let aad_base = header_aad(header)?;
    if src.len() < 11 {
        return Err(VaultError::BackupInvalid("truncated".into()));
    }
    let mut o = 11 + u32::from_le_bytes(src[7..11].try_into().unwrap()) as usize;
    let mut payload = Vec::with_capacity(header.total_size as usize);
    for idx in 0..header.total_chunks {
        if o + 4 + 8 + 24 > src.len() {
            return Err(VaultError::BackupInvalid("truncated chunk".into()));
        }
        let ct_len = u32::from_le_bytes(src[o..o + 4].try_into().unwrap()) as usize;
        o += 4;
        let chunk_index = u64::from_le_bytes(src[o..o + 8].try_into().unwrap());
        o += 8;
        if chunk_index != idx as u64 {
            return Err(VaultError::BackupInvalid("chunk order".into()));
        }
        if ct_len < 16 || ct_len > header.chunk_size as usize + 16 || o + 24 + ct_len > src.len() {
            return Err(VaultError::BackupInvalid("bad chunk length".into()));
        }
        let nonce = &src[o..o + 24];
        o += 24;
        let ct = &src[o..o + ct_len];
        o += ct_len;
        let mut aad = aad_base.clone();
        aad.extend_from_slice(&(idx as u64).to_le_bytes());
        aad.extend_from_slice(&header.total_size.to_le_bytes());
        let sealed = Sealed { nonce: b64_encode(nonce), ciphertext: b64_encode(ct) };
        let plain = open(&key, &aad, &sealed).map_err(|_| VaultError::BackupAuthenticationFailed)?;
        payload.extend_from_slice(&plain);
        if payload.len() as u64 > header.total_size {
            return Err(VaultError::BackupInvalid("payload overflow".into()));
        }
    }
    if o != src.len() {
        return Err(VaultError::BackupInvalid("trailing data".into()));
    }
    if payload.len() as u64 != header.total_size {
        return Err(VaultError::BackupInvalid("size mismatch".into()));
    }
    Ok(payload)
}

pub fn verify_backup(src: impl AsRef<Path>, passphrase: &str) -> VaultResult<BackupInfo> {
    let bytes = read_limited(src.as_ref())?;
    let (header, _) = parse_header(&bytes)?;
    let payload = decrypt_payload(&bytes, &header, passphrase)?;
    let archive = parse_archive(&payload)?;
    let files = archive
        .files
        .iter()
        .map(|(path, data)| BackupFileEntry { path: path.clone(), size: data.len() as u64, blake3: blake3::hash(data).to_hex().to_string() })
        .collect::<Vec<_>>();
    let chunks = header.total_chunks;
    Ok(BackupInfo { header, files, chunks, bytes_read: bytes.len() as u64 })
}

pub fn read_backup(src: impl AsRef<Path>, passphrase: &str) -> VaultResult<(BackupInfo, BackupArchive)> {
    let bytes = read_limited(src.as_ref())?;
    let (header, _) = parse_header(&bytes)?;
    let payload = decrypt_payload(&bytes, &header, passphrase)?;
    let archive = parse_archive(&payload)?;
    let files = archive
        .files
        .iter()
        .map(|(path, data)| BackupFileEntry { path: path.clone(), size: data.len() as u64, blake3: blake3::hash(data).to_hex().to_string() })
        .collect::<Vec<_>>();
    let info = BackupInfo { chunks: header.total_chunks, bytes_read: bytes.len() as u64, header, files };
    Ok((info, archive))
}

fn read_limited(path: &Path) -> VaultResult<Vec<u8>> {
    let meta = fs::metadata(path)?;
    if meta.len() > MAX_ARCHIVE_BYTES + 8 * 1024 * 1024 {
        return Err(VaultError::BackupInvalid("backup file is too large".into()));
    }
    Ok(fs::read(path)?)
}

pub fn plan_retention(
    backups: &[(PathBuf, DateTime<Utc>)],
    keep_count: u32,
    keep_days: u32,
    now: DateTime<Utc>,
) -> Vec<PathBuf> {
    if backups.is_empty() {
        return Vec::new();
    }
    let mut sorted: Vec<&(PathBuf, DateTime<Utc>)> = backups.iter().collect();
    sorted.sort_by_key(|(_, at)| std::cmp::Reverse(*at));
    let keep_count = keep_count.max(1) as usize;
    let cutoff = now - Duration::days(keep_days.max(1) as i64);
    sorted
        .iter()
        .enumerate()
        .filter(|(idx, entry)| *idx >= keep_count && entry.1 < cutoff)
        .map(|(_, (path, _))| path.clone())
        .collect()
}

fn write_atomic(path: &Path, bytes: &[u8]) -> VaultResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut tmp = path.to_path_buf();
    tmp.set_file_name(format!(
        "{}.partial",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("backup")
    ));
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    if let Err(_e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(path);
        fs::rename(&tmp, path)?;
    }
    Ok(())
}

pub fn file_fingerprint(path: &Path) -> VaultResult<String> {
    let bytes = fs::read(path)?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn archive() -> BackupArchive {
        let mut a = BackupArchive::new();
        a.insert("accounts.json", br#"{"accounts":[]}"#.to_vec()).unwrap();
        a.insert("logs/today.log", b"a log line".to_vec()).unwrap();
        a
    }

    #[test]
    fn backup_round_trip_and_verify() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("backup.rofarm");
        let info = write_backup(&path, "backup passphrase", &archive(), "0.1.0", KdfParams::testing()).unwrap();
        assert_eq!(info.files.len(), 2);
        assert_eq!(info.chunks, 1);
        let verified = verify_backup(&path, "backup passphrase").unwrap();
        assert_eq!(verified.files.len(), 2);
        let (_, restored) = read_backup(&path, "backup passphrase").unwrap();
        assert_eq!(restored.files()["logs/today.log"], b"a log line");
        assert!(verify_backup(&path, "wrong passphrase").is_err());
        assert!(file_fingerprint(&path).unwrap().len() == 64);
    }

    #[test]
    fn tampering_and_truncation_are_detected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("b.rofarm");
        write_backup(&path, "backup passphrase", &archive(), "0.1.0", KdfParams::testing()).unwrap();
        let mut bytes = fs::read(&path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        fs::write(&path, &bytes).unwrap();
        assert!(matches!(verify_backup(&path, "backup passphrase"), Err(VaultError::BackupAuthenticationFailed)));
        bytes.pop();
        fs::write(&path, &bytes).unwrap();
        assert!(verify_backup(&path, "backup passphrase").is_err());
        fs::write(&path, b"ROFBK1").unwrap();
        assert!(verify_backup(&path, "backup passphrase").is_err());
    }

    #[test]
    fn path_validation_blocks_traversal_and_system_names() {
        for bad in ["../evil", "/etc/passwd", "C:\\Windows\\x", "a/../../b", "a//b", "CON", "lpt1.txt", "a\0b", ""] {
            assert!(validate_relative_path(bad).is_err(), "{bad} should be rejected");
        }
        for good in ["accounts.json", "logs/2026-10-30.log", "settings/theme.json"] {
            assert!(validate_relative_path(good).is_ok(), "{good} should be accepted");
        }
        let mut a = BackupArchive::new();
        a.insert("ok.txt", b"x".to_vec()).unwrap();
        assert!(a.insert("../bad.txt", b"x".to_vec()).is_err());
        assert!(a.insert("ok.txt", b"y".to_vec()).is_err());
    }

    #[test]
    fn archive_parser_rejects_corrupt_payloads() {
        assert!(parse_archive(b"ROFARC1").is_err());
        assert!(parse_archive(b"nope").is_err());
        let bytes = archive_bytes(&archive()).unwrap();
        let mut tampered = bytes.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 1;
        assert!(parse_archive(&tampered).is_err());
    }

    #[test]
    fn retention_keeps_newest_and_respects_age() {
        let now = Utc::now();
        let backups: Vec<(PathBuf, DateTime<Utc>)> = (0..5)
            .map(|i| (PathBuf::from(format!("b{i}.rofarm")), now - Duration::days(i * 10)))
            .collect();
        let planned = plan_retention(&backups, 2, 15, now);
        assert!(planned.contains(&PathBuf::from("b3.rofarm")));
        assert!(planned.contains(&PathBuf::from("b4.rofarm")));
        assert!(!planned.contains(&PathBuf::from("b0.rofarm")));
        assert!(!planned.contains(&PathBuf::from("b1.rofarm")));
        let keep_all = plan_retention(&backups, 10, 0, now);
        assert!(keep_all.is_empty());
    }

    #[test]
    fn empty_archive_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.rofarm");
        let mut a = BackupArchive::new();
        a.insert("empty.txt", vec![]).unwrap();
        write_backup(&path, "backup passphrase", &a, "0.1.0", KdfParams::testing()).unwrap();
        let (_, restored) = read_backup(&path, "backup passphrase").unwrap();
        assert_eq!(restored.files()["empty.txt"], Vec::<u8>::new());
    }

    #[test]
    fn large_payload_spans_multiple_chunks() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("large.rofarm");
        let mut a = BackupArchive::new();
        let data = vec![7u8; 2 * 1024 * 1024 + 123];
        a.insert("big.bin", data.clone()).unwrap();
        let info = write_backup(&path, "backup passphrase", &a, "0.1.0", KdfParams::testing()).unwrap();
        assert!(info.chunks >= 3);
        let (_, restored) = read_backup(&path, "backup passphrase").unwrap();
        assert_eq!(restored.files()["big.bin"].len(), data.len());
    }

    #[test]
    fn header_validation_rejects_bad_versions() {
        let h = BackupHeader {
            schema_version: 1,
            format: "rofarm-archive-1".into(),
            app_version: "x".into(),
            created_at: Utc::now(),
            kdf: KdfParams::testing(),
            chunk_size: 1024,
            total_size: 2048,
            total_chunks: 2,
        };
        assert_eq!(h.total_chunks as u64, h.total_size.div_ceil(1024));
        assert!(parse_archive(&[]).is_err());
        assert!(b64_decode(&b64_encode(&[1u8, 2, 3])).is_ok());
    }
}
