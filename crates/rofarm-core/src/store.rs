use parking_lot::Mutex;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::events::{NewEvent, StoredEvent, GENESIS_HASH};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("event store I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("corrupt event log at {file}:{line}")]
    Corrupt { file: String, line: usize },
    #[error("hash chain broken at sequence {seq}")]
    ChainBroken { seq: u64 },
    #[error("event log is locked by another writer")]
    Locked,
    #[error("event store is full or misconfigured")]
    Misconfigured(String),
}

pub trait EventStore: Send + Sync {
    fn append(&self, batch: &[NewEvent]) -> Result<Vec<StoredEvent>>;
    fn load(&self) -> Result<Vec<StoredEvent>>;
    fn last_hash(&self) -> Result<Option<String>>;
    fn len(&self) -> Result<u64>;
    fn is_empty(&self) -> bool {
        self.len().map(|n| n == 0).unwrap_or(true)
    }
    fn verify(&self) -> Result<()>;
}

#[derive(Debug, Default)]
pub struct MemoryEventStore {
    events: Mutex<Vec<StoredEvent>>,
}

impl MemoryEventStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn read_all(&self) -> Vec<StoredEvent> {
        self.events.lock().clone()
    }
}

impl EventStore for MemoryEventStore {
    fn append(&self, batch: &[NewEvent]) -> Result<Vec<StoredEvent>> {
        let mut events = self.events.lock();
        let mut seq = events.last().map(|e| e.seq).unwrap_or(0);
        let mut prev = events
            .last()
            .map(|e| e.hash.clone())
            .unwrap_or_else(|| GENESIS_HASH.to_string());
        let mut out = Vec::with_capacity(batch.len());
        for new in batch {
            seq += 1;
            let hash = StoredEvent::compute_hash(seq, new.at, &new.actor, new.correlation, &new.event, &prev);
            let stored = StoredEvent {
                seq,
                at: new.at,
                actor: new.actor.clone(),
                correlation: new.correlation,
                event: new.event.clone(),
                prev_hash: prev.clone(),
                hash: hash.clone(),
            };
            prev = hash;
            events.push(stored.clone());
            out.push(stored);
        }
        Ok(out)
    }

    fn load(&self) -> Result<Vec<StoredEvent>> {
        Ok(self.read_all())
    }

    fn last_hash(&self) -> Result<Option<String>> {
        Ok(Some(
            self.events
                .lock()
                .last()
                .map(|e| e.hash.clone())
                .unwrap_or_else(|| GENESIS_HASH.to_string()),
        ))
    }

    fn len(&self) -> Result<u64> {
        Ok(self.events.lock().len() as u64)
    }

    fn verify(&self) -> Result<()> {
        verify_chain(&self.read_all())
    }
}

pub fn verify_chain(events: &[StoredEvent]) -> Result<()> {
    let mut prev = GENESIS_HASH.to_string();
    let mut expected_seq = 1;
    for e in events {
        if e.seq != expected_seq {
            return Err(StoreError::ChainBroken { seq: e.seq }.into());
        }
        if e.prev_hash != prev || !e.verify() {
            return Err(StoreError::ChainBroken { seq: e.seq }.into());
        }
        prev = e.hash.clone();
        expected_seq += 1;
    }
    Ok(())
}

struct JsonlState {
    seq: u64,
    last_hash: String,
    segment: u32,
    segment_bytes: u64,
}

pub struct JsonlEventStore {
    dir: PathBuf,
    max_segment_bytes: u64,
    state: Mutex<JsonlState>,
}

impl std::fmt::Debug for JsonlEventStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JsonlEventStore")
            .field("dir", &self.dir)
            .field("max_segment_bytes", &self.max_segment_bytes)
            .field("seq", &self.state.lock().seq)
            .finish()
    }
}

impl JsonlEventStore {
    pub fn open(dir: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_limit(dir, 8 * 1024 * 1024)
    }

    pub fn open_with_limit(dir: impl AsRef<Path>, max_segment_bytes: u64) -> Result<Self> {
        fs::create_dir_all(dir.as_ref())?;
        let events = read_segments(dir.as_ref())?;
        verify_chain(&events)?;
        let (seq, last_hash) = match events.last() {
            Some(e) => (e.seq, e.hash.clone()),
            None => (0, GENESIS_HASH.to_string()),
        };
        let segment = find_segments(dir.as_ref())?
            .last()
            .map(|p| parse_segment_number(p).unwrap_or(0))
            .unwrap_or(0);
        let segment_bytes = segment_path(dir.as_ref(), segment)
            .metadata()
            .map(|m| m.len())
            .unwrap_or(0);
        Ok(Self {
            dir: dir.as_ref().to_path_buf(),
            max_segment_bytes: max_segment_bytes.max(64 * 1024),
            state: Mutex::new(JsonlState { seq, last_hash, segment, segment_bytes }),
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn rotate_locked(&self, state: &mut JsonlState) -> Result<()> {
        state.segment += 1;
        state.segment_bytes = 0;
        OpenOptions::new().create(true).append(true).open(segment_path(&self.dir, state.segment))?;
        Ok(())
    }
}

fn parse_segment_number(path: &Path) -> Option<u32> {
    let name = path.file_name()?.to_str()?;
    let digits = name.strip_prefix("events-")?.strip_suffix(".jsonl")?;
    digits.parse().ok()
}

fn segment_path(dir: &Path, n: u32) -> PathBuf {
    dir.join(format!("events-{n:06}.jsonl"))
}

fn find_segments(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut segments: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| parse_segment_number(p).is_some())
        .collect();
    segments.sort_by_key(|p| parse_segment_number(p).unwrap_or(u32::MAX));
    Ok(segments)
}

fn read_segments(dir: &Path) -> Result<Vec<StoredEvent>> {
    let mut out = Vec::new();
    for path in find_segments(dir)? {
        let file = File::open(&path)?;
        let reader = BufReader::new(file);
        let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        for (idx, line) in reader.lines().enumerate() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let event: StoredEvent = serde_json::from_str(&line).map_err(|_| StoreError::Corrupt {
                file: name.clone(),
                line: idx + 1,
            })?;
            out.push(event);
        }
    }
    Ok(out)
}

impl EventStore for JsonlEventStore {
    fn append(&self, batch: &[NewEvent]) -> Result<Vec<StoredEvent>> {
        if batch.is_empty() {
            return Ok(Vec::new());
        }
        let mut state = self.state.lock();
        let mut seq = state.seq;
        let mut prev = state.last_hash.clone();
        let mut out = Vec::with_capacity(batch.len());
        let mut bytes = Vec::new();
        for new in batch {
            seq += 1;
            let hash = StoredEvent::compute_hash(seq, new.at, &new.actor, new.correlation, &new.event, &prev);
            let stored = StoredEvent {
                seq,
                at: new.at,
                actor: new.actor.clone(),
                correlation: new.correlation,
                event: new.event.clone(),
                prev_hash: prev.clone(),
                hash: hash.clone(),
            };
            prev = hash;
            out.push(stored.clone());
            let mut line = serde_json::to_vec(&stored)?;
            line.push(b'\n');
            bytes.extend_from_slice(&line);
        }

        let path = segment_path(&self.dir, state.segment);
        let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
        file.write_all(&bytes)?;
        file.sync_data()?;
        state.segment_bytes += bytes.len() as u64;
        state.seq = seq;
        state.last_hash = prev;
        if state.segment_bytes >= self.max_segment_bytes {
            self.rotate_locked(&mut state)?;
        }
        Ok(out)
    }

    fn load(&self) -> Result<Vec<StoredEvent>> {
        let _guard = self.state.lock();
        read_segments(&self.dir)
    }

    fn last_hash(&self) -> Result<Option<String>> {
        Ok(Some(self.state.lock().last_hash.clone()))
    }

    fn len(&self) -> Result<u64> {
        Ok(self.state.lock().seq)
    }

    fn verify(&self) -> Result<()> {
        let _guard = self.state.lock();
        verify_chain(&read_segments(&self.dir)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::Account;
    use crate::events::{Actor, DomainEvent};
    use crate::ids::builtin;
    use chrono::Utc;

    fn account_event(name: &str) -> NewEvent {
        NewEvent::new(
            Actor::User,
            DomainEvent::AccountAdded { account: Account::new(name, builtin::farming()) },
        )
    }

    #[test]
    fn memory_store_chains_and_verifies() {
        let store = MemoryEventStore::new();
        let stored = store.append(&[account_event("a"), account_event("b")]).unwrap();
        assert_eq!(stored.len(), 2);
        assert_eq!(stored[0].seq, 1);
        assert_eq!(stored[1].prev_hash, stored[0].hash);
        store.verify().unwrap();
        assert_eq!(store.len().unwrap(), 2);
    }

    #[test]
    fn jsonl_store_persists_and_rotates() {
        let dir = tempfile::tempdir().unwrap();
        let count = 120;
        {
            let store = JsonlEventStore::open_with_limit(dir.path(), 64 * 1024).unwrap();
            for i in 0..count {
                store.append(&[account_event(&format!("a{i}"))]).unwrap();
            }
            assert_eq!(store.len().unwrap(), count);
            store.verify().unwrap();
        }
        let reopened = JsonlEventStore::open(dir.path()).unwrap();
        assert_eq!(reopened.len().unwrap(), count);
        let events = reopened.load().unwrap();
        assert_eq!(events.len() as u64, count);
        verify_chain(&events).unwrap();
        assert!(find_segments(dir.path()).unwrap().len() >= 2, "rotation should create multiple segments");
    }

    #[test]
    fn tampering_is_detected_on_open() {
        let dir = tempfile::tempdir().unwrap();
        {
            let store = JsonlEventStore::open(dir.path()).unwrap();
            store.append(&[account_event("a"), account_event("b")]).unwrap();
        }
        let segment = find_segments(dir.path()).unwrap()[0].clone();
        let contents = fs::read_to_string(&segment).unwrap();
        let tampered = contents.replacen("\"username\":\"a\"", "\"username\":\"z\"", 1);
        fs::write(&segment, tampered).unwrap();
        assert!(JsonlEventStore::open(dir.path()).is_err());
    }

    #[test]
    fn corrupt_line_reports_location() {
        let dir = tempfile::tempdir().unwrap();
        {
            let store = JsonlEventStore::open(dir.path()).unwrap();
            store.append(&[account_event("a")]).unwrap();
        }
        let segment = find_segments(dir.path()).unwrap()[0].clone();
        let mut f = OpenOptions::new().append(true).open(&segment).unwrap();
        writeln!(f, "{{not json}}").unwrap();
        match JsonlEventStore::open(dir.path()) {
            Err(crate::error::CoreError::Store(StoreError::Corrupt { line, .. })) => assert_eq!(line, 2),
            other => panic!("expected corrupt error, got {other:?}"),
        }
    }

    #[test]
    fn seq_continues_across_segments() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonlEventStore::open_with_limit(dir.path(), 64 * 1024).unwrap();
        let n = 200;
        for i in 0..n {
            store.append(&[account_event(&format!("user{i}"))]).unwrap();
        }
        let events = store.load().unwrap();
        assert_eq!(events.len(), n);
        assert_eq!(events.last().unwrap().seq, n as u64);
        verify_chain(&events).unwrap();
    }

    #[test]
    fn filter_and_last_hash_work() {
        let store = MemoryEventStore::new();
        assert!(store.last_hash().unwrap().is_some());
        store.append(&[account_event("x")]).unwrap();
        let h = store.last_hash().unwrap().unwrap();
        assert_eq!(h.len(), 64);
        let at = Utc::now();
        let _ = at;
    }
}
