//! Sealed store for the LAN sync: identity, paired peers and the last common
//! hashes the three-way plan needs. Everything in one file under the app
//! config directory, sealed with the platform cipher (DPAPI / Android
//! Keystore; `plain:` with the UI's warning elsewhere) - it holds the device's
//! private key and the pairing keys.

use pdfcore::error::PdfError;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use synccore::lan::{LanIdentity, PairedPeer};
use synccore::metadata;

use crate::secret;

pub(super) const STORE_FILE: &str = "lan-sync.store";
const MAX_STORE_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct LanStore {
    #[serde(default)]
    pub(super) identity: Option<LanIdentity>,
    #[serde(default)]
    pub(super) peers: Vec<PairedPeer>,
    /// `device_id` -> relative path -> SHA-256 of the copy both sides had.
    #[serde(default)]
    pub(super) bases: HashMap<String, HashMap<String, String>>,
    /// Confirmed deletions per peer, so a later plan does not resurrect or
    /// re-report them (`device_id` -> relative path -> tombstone).
    #[serde(default)]
    pub(super) tombstones: HashMap<String, HashMap<String, Tombstone>>,
}

/// A deletion the user confirmed. Kept for 30 days: the trash restores the
/// bytes, the tombstone keeps the plan honest in the meantime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Tombstone {
    pub(super) path: String,
    pub(super) sha256: String,
    /// `local` (removed on this device) or `remote` (removed on the peer).
    pub(super) side: String,
    pub(super) deleted_at: u64,
}

pub(super) const TOMBSTONE_RETENTION_SECS: u64 = 30 * 24 * 60 * 60;

pub(super) fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed| elapsed.as_secs()).unwrap_or(0)
}

impl LanStore {
    /// Records (or refreshes) the tombstone of a confirmed deletion.
    pub(super) fn record_tombstone(&mut self, device_id: &str, path: &str, sha256: &str, side: &str, now: u64) {
        self.tombstones.entry(device_id.to_string()).or_default().insert(
            path.to_string(),
            Tombstone { path: path.to_string(), sha256: sha256.to_string(), side: side.to_string(), deleted_at: now },
        );
    }

    /// The file came back (restore): the tombstone must not suppress it.
    pub(super) fn clear_tombstone(&mut self, device_id: &str, path: &str) {
        if let Some(entries) = self.tombstones.get_mut(device_id) {
            entries.remove(path);
            if entries.is_empty() {
                self.tombstones.remove(device_id);
            }
        }
    }

    /// Drops tombstones older than the retention window and returns the live
    /// paths for one peer.
    pub(super) fn tombstone_paths(&mut self, device_id: &str, now: u64) -> HashSet<String> {
        self.prune_tombstones(now);
        self.tombstones
            .get(device_id)
            .map(|entries| entries.values().map(|entry| entry.path.clone()).collect())
            .unwrap_or_default()
    }

    pub(super) fn prune_tombstones(&mut self, now: u64) {
        self.tombstones.retain(|_, entries| {
            entries.retain(|_, tombstone| now.saturating_sub(tombstone.deleted_at) <= TOMBSTONE_RETENTION_SECS);
            !entries.is_empty()
        });
    }

    pub(super) fn clear_base(&mut self, device_id: &str, path: &str) {
        if let Some(entries) = self.bases.get_mut(device_id) {
            entries.remove(path);
            if entries.is_empty() {
                self.bases.remove(device_id);
            }
        }
    }
}

pub(super) fn store_path(config_dir: &Path) -> PathBuf {
    config_dir.join(STORE_FILE)
}

/// The store, or an empty one. A file that cannot be opened or parsed is an
/// error: silently starting over would forget every pairing.
pub(super) fn load(config_dir: &Path) -> Result<LanStore, PdfError> {
    let path = store_path(config_dir);
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(LanStore::default()),
        Err(error) => return Err(PdfError::from_io(error)),
    };
    if content.len() > MAX_STORE_BYTES {
        return Err(PdfError::Internal("the LAN sync store is unreasonably large".into()));
    }
    let bytes = secret::open_bytes(secret::platform_cipher(), &content)?;
    serde_json::from_slice(&bytes)
        .map_err(|error| PdfError::InvalidInput(format!("the LAN sync store is not valid: {error}")))
}

pub(super) fn save(config_dir: &Path, store: &LanStore) -> Result<(), PdfError> {
    let path = store_path(config_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(PdfError::from_io)?;
    }
    let json = serde_json::to_vec(store).map_err(|error| PdfError::Internal(error.to_string()))?;
    let sealed = secret::seal_bytes(secret::platform_cipher(), &json);
    secret::write_sealed(&path, sealed.as_bytes())
        .map_err(|error| PdfError::Internal(format!("the LAN sync store could not be written: {error}")))
}

/// The device identity, created on first use with the existing device id.
pub(super) fn identity(config_dir: &Path) -> Result<LanIdentity, PdfError> {
    let mut store = load(config_dir)?;
    if let Some(identity) = store.identity.clone() {
        return Ok(identity);
    }
    let device_id = metadata::load_or_create_device_id(config_dir)
        .map_err(|error| PdfError::Internal(format!("the device id could not be created: {error}")))?;
    let identity = LanIdentity::generate(&device_id, &default_name())
        .map_err(|error| PdfError::Internal(format!("the LAN identity could not be created: {error}")))?;
    store.identity = Some(identity.clone());
    save(config_dir, &store)?;
    Ok(identity)
}

/// Renames this device for the network (the identity keeps its certificate).
pub(super) fn rename(config_dir: &Path, name: &str) -> Result<LanIdentity, PdfError> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 64 {
        return Err(PdfError::InvalidInput("the device name must be 1-64 characters".into()));
    }
    let mut store = load(config_dir)?;
    let mut identity = identity(config_dir)?;
    identity.name = trimmed.to_string();
    store.identity = Some(identity.clone());
    save(config_dir, &store)?;
    Ok(identity)
}

fn default_name() -> String {
    for variable in ["COMPUTERNAME", "HOSTNAME"] {
        if let Ok(value) = std::env::var(variable) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return format!("OmniOffice ({trimmed})");
            }
        }
    }
    "OmniOffice".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_store_gets_an_identity_and_round_trips_through_the_cipher() {
        let dir = tempfile::tempdir().unwrap();
        let first = identity(dir.path()).expect("identity");
        assert_eq!(first.fingerprint.len(), 64);
        assert!(!first.private_key.is_empty());
        let restored = identity(dir.path()).expect("identity again");
        assert_eq!(restored.fingerprint, first.fingerprint, "the identity is persisted, not regenerated");

        let mut store = load(dir.path()).expect("store");
        store.peers.push(PairedPeer {
            device_id: "peer-1".into(),
            name: "Telefon".into(),
            address: "192.168.1.20:47820".into(),
            fingerprint: "ab".repeat(32),
            shared_key: "cd".repeat(32),
            paired_at: "2026-10-30T12:00:00Z".into(),
            host_side: false,
        });
        save(dir.path(), &store).expect("save");
        let loaded = load(dir.path()).expect("load");
        assert_eq!(loaded.peers.len(), 1);
        assert_eq!(loaded.peers[0].name, "Telefon");
    }

    #[test]
    fn tombstones_are_recorded_pruned_and_cleared() {
        let mut store = LanStore::default();
        let now = now_unix();
        store.record_tombstone("peer-1", "eski.docx", "abc", "local", now);
        store.record_tombstone(
            "peer-1",
            "silinen.docx",
            "def",
            "remote",
            now.saturating_sub(TOMBSTONE_RETENTION_SECS + 1),
        );
        assert_eq!(store.tombstone_paths("peer-1", now).len(), 1, "the expired tombstone is pruned");
        assert!(store.tombstone_paths("peer-1", now).contains("eski.docx"));
        assert!(store.tombstone_paths("peer-2", now).is_empty());
        store.clear_tombstone("peer-1", "eski.docx");
        assert!(store.tombstone_paths("peer-1", now).is_empty());
    }

    #[test]
    fn a_missing_store_is_empty_and_a_corrupt_one_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let empty = load(dir.path()).expect("missing store is empty");
        assert!(empty.identity.is_none());
        std::fs::write(store_path(dir.path()), "not a sealed store").unwrap();
        assert!(load(dir.path()).is_err());
    }

    #[test]
    fn renaming_keeps_the_certificate() {
        let dir = tempfile::tempdir().unwrap();
        let before = identity(dir.path()).expect("identity");
        let after = rename(dir.path(), "  Atölye  ").expect("rename");
        assert_eq!(after.name, "Atölye");
        assert_eq!(after.fingerprint, before.fingerprint);
        assert!(rename(dir.path(), "   ").is_err());
    }
}
