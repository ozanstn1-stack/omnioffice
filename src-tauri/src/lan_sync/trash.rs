//! Local trash for confirmed LAN deletions (D2).
//!
//! A deletion in the LAN plan is permanent by design, which makes a misclick
//! expensive. Confirmed deletions now first copy the document into a local
//! trash for 30 days: a sealed index (same platform cipher as the pairing
//! store) plus the file bodies under the app config directory. The UI lists
//! them as "Recently deleted" with a restore action; the pairing store's
//! tombstone history keeps the plan from resurrecting or re-reporting the file
//! in the meantime.
//!
//! Limits: 200 entries and one transfer cap per file; the oldest entry is
//! dropped when the limit is reached. Nothing here is ever uploaded or synced.

use crate::secret;
use pdfcore::error::PdfError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use synccore::metadata;

pub(super) const RETENTION_SECS: u64 = 30 * 24 * 60 * 60;
const DIRECTORY: &str = "lan-trash";
const INDEX_FILE: &str = "lan-trash.index";
const MAX_ENTRIES: usize = 200;
const MAX_FILE_BYTES: u64 = synccore::MAX_TRANSFER_BYTES;

/// One deleted document waiting in the trash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct TrashEntry {
    pub(super) id: String,
    pub(super) path: String,
    pub(super) peer_device_id: String,
    pub(super) peer_name: String,
    /// `local` (removed on this device) or `remote` (removed on the peer).
    pub(super) side: String,
    pub(super) sha256: String,
    pub(super) size: u64,
    pub(super) deleted_at: u64,
}

/// A document about to be deleted, with the bytes the trash keeps.
pub(super) struct TrashCandidate {
    pub(super) path: String,
    pub(super) peer_device_id: String,
    pub(super) peer_name: String,
    pub(super) side: String,
    pub(super) sha256: String,
    pub(super) bytes: Vec<u8>,
}

fn trash_dir(config_dir: &Path) -> PathBuf {
    config_dir.join(DIRECTORY)
}

fn index_path(config_dir: &Path) -> PathBuf {
    config_dir.join(INDEX_FILE)
}

fn body_path(config_dir: &Path, id: &str) -> PathBuf {
    trash_dir(config_dir).join(format!("{id}.bin"))
}

fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed| elapsed.as_secs()).unwrap_or(0)
}

fn load_index(config_dir: &Path) -> Result<Vec<TrashEntry>, PdfError> {
    let path = index_path(config_dir);
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(PdfError::from_io(error)),
    };
    let bytes = secret::open_bytes(secret::platform_cipher(), &content)?;
    serde_json::from_slice(&bytes)
        .map_err(|error| PdfError::InvalidInput(format!("the LAN trash index is not valid: {error}")))
}

fn save_index(config_dir: &Path, entries: &[TrashEntry]) -> Result<(), PdfError> {
    let path = index_path(config_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(PdfError::from_io)?;
    }
    let json = serde_json::to_vec(entries).map_err(|error| PdfError::Internal(error.to_string()))?;
    let sealed = secret::seal_bytes(secret::platform_cipher(), &json);
    secret::write_sealed(&path, sealed.as_bytes())
        .map_err(|error| PdfError::Internal(format!("the LAN trash index could not be written: {error}")))
}

fn prune(entries: &mut Vec<TrashEntry>, config_dir: &Path, now: u64) -> usize {
    let before = entries.len();
    entries.retain(|entry| now.saturating_sub(entry.deleted_at) <= RETENTION_SECS);
    let removed = before - entries.len();
    // Bodies of expired entries go too; a missing file is fine.
    let live: Vec<String> = entries.iter().map(|entry| entry.id.clone()).collect();
    if let Ok(bodies) = std::fs::read_dir(trash_dir(config_dir)) {
        for body in bodies.flatten() {
            let name = body.file_name().to_string_lossy().to_string();
            let id = name.strip_suffix(".bin").unwrap_or(&name);
            if !live.iter().any(|live_id| live_id == id) {
                let _ = std::fs::remove_file(body.path());
            }
        }
    }
    removed
}

fn new_id(path: &str, sha256: &str) -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed| elapsed.as_nanos()).unwrap_or(0);
    let digest = metadata::hash_bytes(format!("{path}|{sha256}|{nanos}").as_bytes());
    digest[..16].to_string()
}

/// Copies the document into the trash (pruning expired entries and the oldest
/// one when the limit is reached) and returns its index entry.
pub(super) fn push(config_dir: &Path, candidate: TrashCandidate) -> Result<TrashEntry, PdfError> {
    if candidate.bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(PdfError::InvalidInput("the document is larger than the trash limit".into()));
    }
    let now = now_unix();
    let mut entries = load_index(config_dir)?;
    prune(&mut entries, config_dir, now);
    while entries.len() >= MAX_ENTRIES {
        let oldest = entries.iter().min_by_key(|entry| entry.deleted_at).map(|entry| entry.id.clone());
        match oldest {
            Some(id) => {
                entries.retain(|entry| entry.id != id);
                let _ = std::fs::remove_file(body_path(config_dir, &id));
            }
            None => break,
        }
    }
    let entry = TrashEntry {
        id: new_id(&candidate.path, &candidate.sha256),
        path: candidate.path,
        peer_device_id: candidate.peer_device_id,
        peer_name: candidate.peer_name,
        side: candidate.side,
        sha256: candidate.sha256,
        size: candidate.bytes.len() as u64,
        deleted_at: now,
    };
    std::fs::create_dir_all(trash_dir(config_dir)).map_err(PdfError::from_io)?;
    secret::write_sealed(&body_path(config_dir, &entry.id), &candidate.bytes)
        .map_err(|error| PdfError::Internal(format!("the trash copy could not be written: {error}")))?;
    entries.push(entry.clone());
    save_index(config_dir, &entries)?;
    Ok(entry)
}

/// The live trash entries, newest first. Expired ones are pruned on the way.
pub(super) fn list(config_dir: &Path) -> Result<Vec<TrashEntry>, PdfError> {
    let now = now_unix();
    let mut entries = load_index(config_dir)?;
    let removed = prune(&mut entries, config_dir, now);
    if removed > 0 {
        save_index(config_dir, &entries)?;
    }
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.deleted_at));
    Ok(entries)
}

/// Reads one entry's bytes and drops it from the trash (restore consumes it).
pub(super) fn take(config_dir: &Path, id: &str) -> Result<Option<(TrashEntry, Vec<u8>)>, PdfError> {
    let mut entries = load_index(config_dir)?;
    let Some(index) = entries.iter().position(|entry| entry.id == id) else { return Ok(None) };
    let bytes = std::fs::read(body_path(config_dir, id)).map_err(PdfError::from_io)?;
    let entry = entries.remove(index);
    save_index(config_dir, &entries)?;
    let _ = std::fs::remove_file(body_path(config_dir, id));
    Ok(Some((entry, bytes)))
}

/// Drops entries older than the retention window; returns how many went.
pub(super) fn purge_expired(config_dir: &Path) -> Result<usize, PdfError> {
    let now = now_unix();
    let mut entries = load_index(config_dir)?;
    let removed = prune(&mut entries, config_dir, now);
    if removed > 0 {
        save_index(config_dir, &entries)?;
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(path: &str, body: &[u8]) -> TrashCandidate {
        TrashCandidate {
            path: path.to_string(),
            peer_device_id: "peer-1".to_string(),
            peer_name: "Telefon".to_string(),
            side: "local".to_string(),
            sha256: metadata::hash_bytes(body),
            bytes: body.to_vec(),
        }
    }

    #[test]
    fn a_deleted_document_round_trips_through_the_trash() {
        let dir = tempfile::tempdir().unwrap();
        let entry = push(dir.path(), candidate("rapor.docx", b"deleted bytes")).expect("push");
        assert_eq!(entry.path, "rapor.docx");
        let listed = list(dir.path()).expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, entry.id);
        let (entry, bytes) = take(dir.path(), &listed[0].id).expect("take").expect("entry");
        assert_eq!(entry.path, "rapor.docx");
        assert_eq!(bytes, b"deleted bytes");
        assert!(list(dir.path()).expect("list after take").is_empty());
        assert!(take(dir.path(), "missing").expect("missing").is_none());
    }

    #[test]
    fn expired_entries_are_pruned_from_the_index_and_the_disk() {
        let dir = tempfile::tempdir().unwrap();
        let entry = push(dir.path(), candidate("eski.docx", b"old")).expect("push");
        let mut entries = load_index(dir.path()).expect("index");
        entries[0].deleted_at = now_unix().saturating_sub(RETENTION_SECS + 1);
        save_index(dir.path(), &entries).expect("save");
        assert_eq!(purge_expired(dir.path()).expect("purge"), 1);
        assert!(list(dir.path()).expect("list").is_empty());
        assert!(!body_path(dir.path(), &entry.id).exists(), "the body is removed too");
    }

    #[test]
    fn the_oldest_entry_makes_room_at_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        let mut entries = Vec::new();
        for index in 0..MAX_ENTRIES {
            entries.push(TrashEntry {
                id: format!("id-{index}"),
                path: format!("doc-{index}.docx"),
                peer_device_id: "peer-1".into(),
                peer_name: "Telefon".into(),
                side: "local".into(),
                sha256: metadata::hash_bytes(b"x"),
                size: 1,
                deleted_at: now_unix().saturating_sub(index as u64),
            });
        }
        save_index(dir.path(), &entries).expect("seed");
        let entry = push(dir.path(), candidate("yeni.docx", b"new")).expect("push");
        let listed = list(dir.path()).expect("list");
        assert_eq!(listed.len(), MAX_ENTRIES);
        assert!(listed.iter().any(|known| known.id == entry.id));
    }
}
