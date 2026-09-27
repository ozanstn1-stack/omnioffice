//! Local-first cloud sync foundation.
//!
//! Design rules that every module here must respect:
//!
//! * **Opt-in.** `synccore` never touches the network on its own; the caller
//!   decides when to list/upload/download. The application layer keeps a
//!   config flag (`enabled: false` by default) and refuses commands while it
//!   is off.
//! * **No silent overwrite, ever.** Uploads are conditional (`If-Match`) when
//!   a base version exists, downloads are hash-checked, and any situation in
//!   which both sides changed is surfaced as [`metadata::SyncState::Conflict`]
//!   instead of being resolved automatically. The only destructive operations
//!   are `keep_cloud` (replace local) and a forced `keep_local` upload, and
//!   both require an explicit user resolution.
//! * **Bounded.** Transfers are capped at [`MAX_TRANSFER_BYTES`], hashes are
//!   streamed, multi-status XML is parsed with a depth limit and numeric
//!   entity references only (never external entities).
//!
//! The crate is pure Rust and blocking on purpose: the Tauri layer runs each
//! call in its blocking worker pool, and a blocking trait is object safe
//! (`dyn SyncProvider`) without pulling in an async runtime.

pub mod metadata;
pub mod merge;
pub mod webdav;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Hard cap for a single transfer. `.oswk` documents are JSON/zip payloads;
/// anything above half a gigabyte is almost certainly a mistake, and the cap
/// bounds the memory a single sync action can use.
pub const MAX_TRANSFER_BYTES: u64 = 512 * 1024 * 1024;

/// A file or folder returned by a remote listing. `name` is the file name
/// relative to the listed directory (not a full path).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteEntry {
    pub name: String,
    pub size: u64,
    /// Server-provided entity tag, normalized (no weak prefix, no quotes).
    pub etag: Option<String>,
    /// Raw `getlastmodified` value; display only, never parsed for logic.
    pub modified: Option<String>,
    pub is_dir: bool,
}

/// Errors shared by all sync providers. `Conflict` is the important one: it
/// means a conditional write was refused (HTTP 412) or a state check found
/// divergent content, and the caller must ask the user what to do.
#[derive(Debug, Error)]
pub enum SyncError {
    #[error("cloud sync is disabled")]
    Disabled,
    /// Provider exists in the UI but is not implemented in this build
    /// (OAuth providers). Never pretend it works.
    #[error("unsupported sync provider: {0}")]
    Unsupported(String),
    #[error("remote file not found: {0}")]
    NotFound(String),
    /// The remote file changed under us (etag mismatch / concurrent edit).
    #[error("sync conflict: {0}")]
    Conflict(String),
    #[error("file is larger than the {0} byte transfer limit")]
    TooLarge(u64),
    #[error("authentication failed: {0}")]
    Auth(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("server returned HTTP {status}: {message}")]
    Http { status: u16, message: String },
    /// Malformed multi-status XML or an unexpected DAV response shape.
    #[error("malformed server response: {0}")]
    Protocol(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("I/O error: {0}")]
    Io(String),
    #[error("internal sync error: {0}")]
    Internal(String),
}

impl From<std::io::Error> for SyncError {
    fn from(error: std::io::Error) -> Self {
        SyncError::Io(error.to_string())
    }
}

/// The provider abstraction. Implementations must:
///
/// * map HTTP 401/403 to [`SyncError::Auth`], 404 to [`SyncError::NotFound`]
///   and 412 on conditional writes to [`SyncError::Conflict`];
/// * return the ETag of the stored object from [`SyncProvider::put`] whenever
///   the server provides it (it is the cheap version token used elsewhere);
/// * never retry destructive operations automatically.
pub trait SyncProvider: Send + Sync {
    /// Cheap connectivity + credential probe. Returns a human readable server
    /// description on success.
    fn test(&self) -> Result<String, SyncError>;

    /// Returns the entries directly inside `remote_dir` (files and folders).
    fn list(&self, remote_dir: &str) -> Result<Vec<RemoteEntry>, SyncError>;

    /// Downloads a file. Returns `(bytes, etag)`; the etag is taken from the
    /// response headers and normalized.
    fn get(&self, path: &str) -> Result<(Vec<u8>, Option<String>), SyncError>;

    /// Uploads a file. When `if_match_etag` is `Some`, the request carries an
    /// `If-Match` header and the server refuses the write (412 -> conflict)
    /// if the remote version no longer matches. Returns the new etag.
    fn put(
        &self,
        path: &str,
        bytes: &[u8],
        if_match_etag: Option<&str>,
    ) -> Result<Option<String>, SyncError>;

    /// Deletes a remote file.
    fn delete(&self, path: &str) -> Result<(), SyncError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_is_stable() {
        assert_eq!(SyncError::Disabled.to_string(), "cloud sync is disabled");
        assert!(SyncError::Conflict("etag mismatch".into())
            .to_string()
            .contains("etag mismatch"));
    }
}
