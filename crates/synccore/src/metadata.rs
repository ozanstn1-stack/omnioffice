//! Per-file sync metadata (sidecar) and the version state machine.
//!
//! ## Where the sidecar lives
//!
//! Sidecars are **not** written next to the document. Documents can live on
//! read-only media, inside Android SAF copies the app does not own, or in
//! folders the user syncs with something else - dropping `*.sync.json` files
//! there would be invasive and sometimes impossible. Instead each sidecar is
//! stored under the application data directory as
//!
//! ```text
//! <app data>/sync/<path-key>/<file name>.sync.json
//! ```
//!
//! where `<path-key>` is the first 8 hex characters of the SHA-256 of the
//! document's absolute path. The suffix keeps the spec's `<name>.sync.json`
//! naming readable, and the path key makes it collision free when two folders
//! both contain `report.oswk`.
//!
//! ## The state machine
//!
//! All decisions come from three hashes: the current local content hash, the
//! current cloud content hash, and the content hash recorded in
//! [`BaseState::cloud_sha256`] the last time the two sides were known equal.
//!
//! | local vs base | cloud vs base | result |
//! |---|---|---|
//! | (no base) | cloud absent | `LocalOnly` |
//! | (no base) | any | `Synced` if hashes match, else `Conflict` |
//! | unchanged | unchanged / cloud absent | `Synced` / `LocalOnly` |
//! | changed | unchanged | `LocalAhead` |
//! | unchanged | changed | `CloudAhead` |
//! | changed | changed, hashes differ | `Conflict` |
//!
//! A pure hash match always wins - if local and cloud bytes are identical the
//! state is `Synced` and the etag is adopted, regardless of history. A local
//! file that is missing while a cloud copy exists is `CloudAhead` (download
//! is possible); a cloud copy that disappeared while the local file is
//! unchanged is treated as `LocalOnly` because V3.1 has no delete
//! propagation - re-uploading is the only sensible non-destructive action.

use crate::{SyncError, MAX_TRANSFER_BYTES};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Last known common ancestor of the local and cloud copies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaseState {
    /// ETag the cloud copy had when the base was recorded. `None` when the
    /// server does not expose ETags; the hash is then the only version token.
    pub cloud_etag: Option<String>,
    pub cloud_sha256: String,
    /// Bumped once per successful sync (upload/download); monotonic per file.
    pub revision: u64,
}

/// Sidecar contents. Never contains credentials - the WebDAV password lives in
/// the OS-protected secret store, not here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncMeta {
    /// File name (with extension) this sidecar tracks.
    pub file: String,
    /// Absolute local path at the time of writing (diagnostics + collision
    /// detection only; the path key in the sidecar location is authoritative).
    #[serde(default)]
    pub local_path: String,
    /// Random UUID generated once per install, persisted in `device.json`.
    pub device_id: String,
    /// Local edit counter. Bumped on every successful upload or download.
    pub local_revision: u64,
    /// SHA-256 of the local file content at the time of writing.
    pub content_sha256: String,
    /// RFC 3339 timestamp of the last metadata write.
    pub updated_at: String,
    #[serde(default)]
    pub base: Option<BaseState>,
    #[serde(default)]
    pub last_synced_at: Option<String>,
}

/// Version state of one tracked file, computed by [`detect_state`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncState {
    /// Local file exists, cloud copy does not (or was deleted).
    LocalOnly,
    /// Local and cloud content hashes match.
    Synced,
    /// Local divergence since the base, cloud unchanged.
    LocalAhead,
    /// Cloud divergence since the base, local unchanged.
    CloudAhead,
    /// Both sides changed (or no common base and content differs). Never
    /// resolved automatically.
    Conflict,
}

/// Three-way state detection. See the module docs for the full matrix.
pub fn detect_state(
    local_sha256: Option<&str>,
    cloud_sha256: Option<&str>,
    base: Option<&BaseState>,
) -> SyncState {
    match (local_sha256, cloud_sha256) {
        // Nothing anywhere: report as LocalOnly; callers only deal with
        // existing local files, so this combination is not actionable.
        (None, None) => SyncState::LocalOnly,
        // Local only, or cloud copy vanished. Re-upload is non-destructive.
        (Some(_), None) => SyncState::LocalOnly,
        // Local missing but cloud present: download is possible.
        (None, Some(_)) => SyncState::CloudAhead,
        (Some(local), Some(cloud)) => {
            // Content equality always wins: identical bytes mean synced, and
            // the caller may adopt the current etag as the new base.
            if local == cloud {
                return SyncState::Synced;
            }
            match base {
                // No common ancestor and different bytes: we cannot tell which
                // side is newer, so this is a conflict, never an overwrite.
                None => SyncState::Conflict,
                Some(base) => {
                    let local_changed = local != base.cloud_sha256;
                    let cloud_changed = cloud != base.cloud_sha256;
                    match (local_changed, cloud_changed) {
                        // Cannot happen after the equality check, but classify
                        // conservatively rather than panicking.
                        (false, false) => SyncState::Synced,
                        (true, false) => SyncState::LocalAhead,
                        (false, true) => SyncState::CloudAhead,
                        (true, true) => SyncState::Conflict,
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

/// Sidecar directory inside the application data directory.
pub fn sync_dir(config_dir: &Path) -> PathBuf {
    config_dir.join("sync")
}

/// Stable 8-hex-character key for a local path, used as the sidecar folder.
pub fn path_key(local_path: &Path) -> String {
    let normalized = normalize_path_string(local_path);
    let digest = Sha256::digest(normalized.as_bytes());
    hex_lower(&digest[..4])
}

/// `<sync dir>/<path key>/<name>.sync.json`.
pub fn meta_path(config_dir: &Path, local_path: &Path) -> Result<PathBuf, SyncError> {
    let name = file_name_of(local_path)?;
    Ok(sync_dir(config_dir).join(path_key(local_path)).join(format!("{name}.sync.json")))
}

/// `device.json` lives directly inside the sync directory.
pub fn device_path(config_dir: &Path) -> PathBuf {
    sync_dir(config_dir).join("device.json")
}

/// File name of a local path as a lossless UTF-8 `String`.
pub fn file_name_of(local_path: &Path) -> Result<String, SyncError> {
    local_path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| SyncError::InvalidInput("path has no file name".to_string()))
}

fn normalize_path_string(path: &Path) -> String {
    // Best-effort canonicalization: on Windows this turns `C:\a\..\b` into
    // `C:\b`; when the path does not exist, fall back to the raw string. The
    // key only has to be stable for a given document.
    std::fs::canonicalize(path)
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|_| path.to_string_lossy().to_string())
}

// ---------------------------------------------------------------------------
// Device identity
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceFile {
    device_id: String,
}

/// Returns the persisted per-install device id, creating it on first use.
pub fn load_or_create_device_id(config_dir: &Path) -> Result<String, SyncError> {
    let path = device_path(config_dir);
    if let Ok(bytes) = std::fs::read(&path) {
        if let Ok(file) = serde_json::from_slice::<DeviceFile>(&bytes) {
            let id = file.device_id.trim().to_string();
            if !id.is_empty() {
                return Ok(id);
            }
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    let payload = serde_json::to_vec_pretty(&DeviceFile { device_id: id.clone() })
        .map_err(|error| SyncError::Internal(format!("device id serialize failed: {error}")))?;
    write_atomic(&path, &payload)?;
    Ok(id)
}

// ---------------------------------------------------------------------------
// Sidecar read/write (atomic)
// ---------------------------------------------------------------------------

/// Loads the sidecar for a document, tolerating a missing or unreadable file.
/// A malformed sidecar is treated as absent: the state machine then falls back
/// to hash comparison, which is always safe.
pub fn load_meta(config_dir: &Path, local_path: &Path) -> Option<SyncMeta> {
    let path = meta_path(config_dir, local_path).ok()?;
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Writes the sidecar atomically (temp sibling + rename).
pub fn save_meta(config_dir: &Path, local_path: &Path, meta: &SyncMeta) -> Result<(), SyncError> {
    let path = meta_path(config_dir, local_path)?;
    let payload = serde_json::to_vec_pretty(meta)
        .map_err(|error| SyncError::Internal(format!("metadata serialize failed: {error}")))?;
    write_atomic(&path, &payload)
}

/// Removes the sidecar. Used by "forget"; never touches the document itself.
pub fn delete_meta(config_dir: &Path, local_path: &Path) -> Result<(), SyncError> {
    let path = meta_path(config_dir, local_path)?;
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(SyncError::Io(error.to_string())),
    }
}

/// Atomic write: a uniquely named temp sibling is written, flushed to stable
/// storage and then renamed over the target, so a crash never leaves a
/// half-written sidecar behind.
///
/// `fs::rename` replaces an existing target atomically on Windows
/// (`MoveFileExW` + `MOVEFILE_REPLACE_EXISTING`) and Unix; the old
/// remove-then-rename sequence lost the file if the process died in between.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), SyncError> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let name = path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "sync".to_string());
    let temp = path.with_file_name(format!(".{name}.{}.tmp", uuid::Uuid::new_v4().simple()));
    {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    std::fs::rename(&temp, path).map_err(|error| {
        let _ = std::fs::remove_file(&temp);
        SyncError::Io(error.to_string())
    })?;
    sync_parent_dir(path);
    Ok(())
}

/// Best-effort parent directory fsync; a no-op where directories cannot be
/// opened for sync (Windows).
fn sync_parent_dir(path: &Path) {
    #[cfg(unix)]
    {
        if let Some(parent) = path.parent() {
            if let Ok(dir) = std::fs::File::open(parent) {
                let _ = dir.sync_all();
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

// ---------------------------------------------------------------------------
// Hashing (streamed, capped)
// ---------------------------------------------------------------------------

/// Lowercase hex of a SHA-256 digest.
pub fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Marker used when the cloud content is known to differ from the local file
/// but its hash was not computed (sizes differ, or the etag moved while the
/// local copy is unchanged). Deliberately not a valid SHA-256 hex digest, so
/// it can never collide with a real hash; [`detect_state`] only needs to know
/// "differs".
pub fn unknown_cloud_hash(size: u64, etag: Option<&str>) -> String {
    format!("unverified-size-{size}-etag-{}", etag.unwrap_or("-"))
}

/// SHA-256 of an in-memory buffer.
pub fn hash_bytes(bytes: &[u8]) -> String {
    hex_lower(&Sha256::digest(bytes))
}

/// Streams a file through SHA-256 without keeping its bytes, enforcing the
/// transfer cap first. Returns `(sha256, size)`.
pub fn hash_file(path: &Path) -> Result<(String, u64), SyncError> {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > MAX_TRANSFER_BYTES {
        return Err(SyncError::TooLarge(metadata.len()));
    }
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > MAX_TRANSFER_BYTES {
            return Err(SyncError::TooLarge(total));
        }
        hasher.update(&buffer[..read]);
    }
    Ok((hex_lower(&hasher.finalize()), total))
}

/// Reads a file once (capped) and returns `(bytes, sha256)`; the same buffer
/// is reused for the upload, so large documents are never loaded twice.
pub fn read_capped(path: &Path) -> Result<(Vec<u8>, String), SyncError> {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > MAX_TRANSFER_BYTES {
        return Err(SyncError::TooLarge(metadata.len()));
    }
    let bytes = std::fs::read(path)?;
    if bytes.len() as u64 > MAX_TRANSFER_BYTES {
        return Err(SyncError::TooLarge(bytes.len() as u64));
    }
    let sha = hash_bytes(&bytes);
    Ok((bytes, sha))
}

// ---------------------------------------------------------------------------
// Timestamps
// ---------------------------------------------------------------------------

/// RFC 3339 UTC timestamp, e.g. `2026-09-27T14:15:30Z`.
pub fn now_rfc3339() -> String {
    format_rfc3339(SystemTime::now())
}

pub fn format_rfc3339(time: SystemTime) -> String {
    let (year, month, day, hour, minute, second) = utc_parts(time);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// File-name safe timestamp for "cloud copy" documents
/// (e.g. `2026-09-27 141530`). Colons are never used: Windows forbids them.
pub fn stamp_for_name(time: SystemTime) -> String {
    let (year, month, day, hour, minute, second) = utc_parts(time);
    format!("{year:04}-{month:02}-{day:02} {hour:02}{minute:02}{second:02}")
}

fn utc_parts(time: SystemTime) -> (i64, u32, u32, u32, u32, u32) {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    let days = seconds.div_euclid(86_400);
    let remainder = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    (
        year,
        month,
        day,
        (remainder / 3600) as u32,
        ((remainder % 3600) / 60) as u32,
        (remainder % 60) as u32,
    )
}

/// Howard Hinnant's `civil_from_days`: days since 1970-01-01 -> (y, m, d).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn base(sha: &str) -> BaseState {
        BaseState {
            cloud_etag: Some("\"etag-1\"".to_string()),
            cloud_sha256: sha.to_string(),
            revision: 1,
        }
    }

    // -- state machine: the full three-way matrix ---------------------------

    #[test]
    fn no_base_no_cloud_is_local_only() {
        assert_eq!(detect_state(Some("L"), None, None), SyncState::LocalOnly);
        assert_eq!(detect_state(None, None, None), SyncState::LocalOnly);
    }

    #[test]
    fn no_base_equal_hashes_is_synced() {
        assert_eq!(detect_state(Some("X"), Some("X"), None), SyncState::Synced);
    }

    #[test]
    fn no_base_different_hashes_is_conflict() {
        assert_eq!(detect_state(Some("L"), Some("C"), None), SyncState::Conflict);
    }

    #[test]
    fn base_and_both_unchanged_is_synced() {
        let base = base("B");
        assert_eq!(detect_state(Some("B"), Some("B"), Some(&base)), SyncState::Synced);
    }

    #[test]
    fn base_local_changed_is_local_ahead() {
        let base = base("B");
        assert_eq!(detect_state(Some("L"), Some("B"), Some(&base)), SyncState::LocalAhead);
    }

    #[test]
    fn base_cloud_changed_is_cloud_ahead() {
        let base = base("B");
        assert_eq!(detect_state(Some("B"), Some("C"), Some(&base)), SyncState::CloudAhead);
    }

    #[test]
    fn base_both_changed_is_conflict() {
        let base = base("B");
        assert_eq!(detect_state(Some("L"), Some("C"), Some(&base)), SyncState::Conflict);
    }

    #[test]
    fn hash_match_wins_over_history() {
        // Both sides changed but ended up identical: still synced.
        let base = base("B");
        assert_eq!(detect_state(Some("N"), Some("N"), Some(&base)), SyncState::Synced);
        // Same without a base at all.
        assert_eq!(detect_state(Some("N"), Some("N"), None), SyncState::Synced);
    }

    #[test]
    fn local_missing_with_cloud_is_cloud_ahead() {
        assert_eq!(detect_state(None, Some("C"), None), SyncState::CloudAhead);
        assert_eq!(detect_state(None, Some("C"), Some(&base("B"))), SyncState::CloudAhead);
    }

    #[test]
    fn vanished_cloud_is_local_only_when_local_unchanged() {
        // No delete propagation in V3.1: an unchanged local file with a
        // deleted cloud copy may be re-uploaded.
        assert_eq!(detect_state(Some("B"), None, Some(&base("B"))), SyncState::LocalOnly);
        // A changed local file with a deleted cloud copy is still local-only;
        // uploading it cannot destroy anything.
        assert_eq!(detect_state(Some("L"), None, Some(&base("B"))), SyncState::LocalOnly);
    }

    // -- sidecar paths, round trips, atomicity ------------------------------

    #[test]
    fn meta_path_is_stable_and_collision_free() {
        let config = Path::new("C:\\appdata");
        let a = meta_path(config, Path::new("C:\\docs\\a\\report.oswk")).unwrap();
        let b = meta_path(config, Path::new("C:\\docs\\b\\report.oswk")).unwrap();
        assert_ne!(a, b, "same file name in different folders must not share a sidecar");
        assert!(a.to_string_lossy().ends_with("report.oswk.sync.json"));
        let again = meta_path(config, Path::new("C:\\docs\\a\\report.oswk")).unwrap();
        assert_eq!(a, again);
    }

    #[test]
    fn sidecar_round_trip_and_delete() {
        let dir = tempfile::tempdir().unwrap();
        let document = dir.path().join("report.oswk");
        std::fs::write(&document, b"hello").unwrap();
        let meta = SyncMeta {
            file: "report.oswk".to_string(),
            local_path: document.to_string_lossy().to_string(),
            device_id: "device-1".to_string(),
            local_revision: 3,
            content_sha256: hash_bytes(b"hello"),
            updated_at: now_rfc3339(),
            base: Some(base("sha-b")),
            last_synced_at: Some(now_rfc3339()),
        };
        save_meta(dir.path(), &document, &meta).unwrap();
        let loaded = load_meta(dir.path(), &document).unwrap();
        assert_eq!(loaded, meta);
        delete_meta(dir.path(), &document).unwrap();
        assert!(load_meta(dir.path(), &document).is_none());
        // Deleting twice stays a no-op.
        delete_meta(dir.path(), &document).unwrap();
    }

    #[test]
    fn malformed_sidecar_is_treated_as_absent() {
        let dir = tempfile::tempdir().unwrap();
        let document = dir.path().join("report.oswk");
        std::fs::write(&document, b"hello").unwrap();
        let path = meta_path(dir.path(), &document).unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"{not json").unwrap();
        assert!(load_meta(dir.path(), &document).is_none());
    }

    #[test]
    fn atomic_write_leaves_no_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sync").join("x.sync.json");
        write_atomic(&path, b"one").unwrap();
        write_atomic(&path, b"two").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
        let entries: Vec<_> = std::fs::read_dir(path.parent().unwrap()).unwrap().collect();
        assert_eq!(entries.len(), 1, "temp sibling must be renamed away");
    }

    #[test]
    fn device_id_is_created_once_and_persisted() {
        let dir = tempfile::tempdir().unwrap();
        let first = load_or_create_device_id(dir.path()).unwrap();
        let second = load_or_create_device_id(dir.path()).unwrap();
        assert_eq!(first, second);
        assert!(!first.trim().is_empty());
    }

    // -- hashing and timestamps ---------------------------------------------

    #[test]
    fn hash_file_matches_in_memory_hash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.bin");
        let payload = vec![7u8; 200_000];
        std::fs::write(&path, &payload).unwrap();
        let (streamed, size) = hash_file(&path).unwrap();
        assert_eq!(size, payload.len() as u64);
        assert_eq!(streamed, hash_bytes(&payload));
        let (bytes, sha) = read_capped(&path).unwrap();
        assert_eq!(bytes, payload);
        assert_eq!(sha, streamed);
    }

    #[test]
    fn timestamps_are_well_formed_utc() {
        let epoch = UNIX_EPOCH + Duration::from_secs(0);
        assert_eq!(format_rfc3339(epoch), "1970-01-01T00:00:00Z");
        assert_eq!(stamp_for_name(epoch), "1970-01-01 000000");
        // 2024-02-29 12:34:56 UTC (leap day).
        let leap = UNIX_EPOCH + Duration::from_secs(1_709_210_096);
        assert_eq!(format_rfc3339(leap), "2024-02-29T12:34:56Z");
    }

    #[test]
    fn unknown_cloud_hash_cannot_collide_with_real_digests() {
        let marker = unknown_cloud_hash(123, Some("abc"));
        assert_eq!(marker, "unverified-size-123-etag-abc");
        assert_ne!(marker.len(), 64);
        assert!(!marker.chars().all(|ch| ch.is_ascii_hexdigit()));
        // The state machine treats it as "changed on the cloud side".
        let base = base("B");
        assert_eq!(detect_state(Some("B"), Some(&marker), Some(&base)), SyncState::CloudAhead);
        assert_eq!(detect_state(Some("L"), Some(&marker), Some(&base)), SyncState::Conflict);
    }

    // -- secrets never leak into the sidecar --------------------------------

    #[test]
    fn metadata_json_never_contains_a_password() {
        let meta = SyncMeta {
            file: "report.oswk".to_string(),
            local_path: "C:\\docs\\report.oswk".to_string(),
            device_id: "device-1".to_string(),
            local_revision: 1,
            content_sha256: hash_bytes(b"x"),
            updated_at: now_rfc3339(),
            base: Some(base("sha-b")),
            last_synced_at: None,
        };
        let json = serde_json::to_string(&meta).unwrap();
        assert!(!json.to_lowercase().contains("password"));
        assert!(!json.to_lowercase().contains("secret"));
    }
}
