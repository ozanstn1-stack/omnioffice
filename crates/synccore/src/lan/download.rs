//! Resumable downloads (E6.3): both halves.
//!
//! Uploads have resumed since D3 (`lan/upload.rs`); a download was one `get`
//! and started over when the Wi-Fi dropped. This module adds the symmetric
//! pair. The wire grows by exactly two operations, `begin_get` and `get_chunk`;
//! a peer that does not know them answers "unknown operation" and
//! [`LanClient::fetch_resumable`] falls back to the whole-file `get`, so a new
//! client still works against an older host.
//!
//! The partial download lives next to the finished files as
//! `<root>/.parts/<hash of the relative path>.get` with a `<...>.get.json` note
//! carrying the hash and size of the document it belongs to. `.parts` is
//! hidden from `scan_folder` and refused by `resolve_path`, so the other side
//! can never ask for it.

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    file_modified, hex_encode, parts_dir, refusal, resolve_path, Failure, HostState, LanClient, PairedPeer,
    RemoteFile, Request, Response, MAX_CHUNK_BYTES, PART_MAX_AGE_SECS,
};
use crate::SyncError;

/// Attempts for one download, matching `push_resumable`.
const DOWNLOAD_ATTEMPTS: usize = 3;

/// The note stored beside a partial download, so a file left over from another
/// version of the document is not resumed into a corrupt one.
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct PartialNote {
    pub(super) sha256: String,
    pub(super) size: u64,
}

// ---------------------------------------------------------------------------
// Host half
// ---------------------------------------------------------------------------

/// Hashes a file without loading it, so a large document does not sit in RAM
/// just to be announced.
fn hash_file(path: &Path) -> Result<String, Failure> {
    let mut file = std::fs::File::open(path)
        .map_err(|error| refusal(format!("the document is unreadable: {error}")))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| refusal(format!("the document is unreadable: {error}")))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex_encode(&hasher.finalize()))
}

/// Announces what a download of `path` would carry: size, hash and modified
/// time. The client resumes only when the hash still matches what it has.
pub(super) fn handle_begin_get(state: &Arc<HostState>, request: &Request) -> Result<Response, Failure> {
    let target = resolve_path(&state.root, &request.path).map_err(|error| refusal(error.to_string()))?;
    let size = std::fs::metadata(&target)
        .map_err(|error| refusal(format!("the document is unreadable: {error}")))?
        .len();
    if size > crate::MAX_TRANSFER_BYTES {
        return Err(refusal("the document is larger than the transfer limit"));
    }
    Ok(Response {
        ok: true,
        size,
        sha256: hash_file(&target)?,
        modified: file_modified(&target),
        ..Response::default()
    })
}

/// Serves the bytes from `offset` to the end of the chunk window. `total` is the
/// size the client believes the document has; a mismatch means the document
/// changed under it and the download has to start again.
pub(super) fn handle_get_chunk(
    state: &Arc<HostState>,
    request: &Request,
) -> Result<(Response, Option<Vec<u8>>), Failure> {
    let target = resolve_path(&state.root, &request.path).map_err(|error| refusal(error.to_string()))?;
    let size = std::fs::metadata(&target)
        .map_err(|error| refusal(format!("the document is unreadable: {error}")))?
        .len();
    if request.total != 0 && request.total != size {
        return Err(refusal("the document changed; start the download again"));
    }
    if request.offset > size {
        return Err(refusal("the requested offset is past the end of the document"));
    }
    let window = (size - request.offset).min(MAX_CHUNK_BYTES as u64) as usize;
    let mut file = std::fs::File::open(&target)
        .map_err(|error| refusal(format!("the document is unreadable: {error}")))?;
    file.seek(SeekFrom::Start(request.offset))
        .map_err(|error| refusal(format!("the document is unreadable: {error}")))?;
    let mut buffer = vec![0u8; window];
    file.read_exact(&mut buffer)
        .map_err(|error| refusal(format!("the document is unreadable: {error}")))?;
    let response = Response {
        ok: true,
        body_len: buffer.len() as u64,
        offset: request.offset + buffer.len() as u64,
        size,
        ..Response::default()
    };
    Ok((response, Some(buffer)))
}

// ---------------------------------------------------------------------------
// Client half
// ---------------------------------------------------------------------------

/// True when the peer's answer means "I do not know that operation" rather than
/// "that request was wrong". A host from before E6.3 answers this way, and the
/// caller falls back to the whole-file `get`.
pub fn is_unknown_operation(error: &SyncError) -> bool {
    let SyncError::Network(message) = error else { return false };
    let message = message.to_ascii_lowercase();
    message.contains("unknown operation") || message.contains("unsupported operation")
}

pub(super) fn partial_paths(root: &Path, path: &str) -> (PathBuf, PathBuf) {
    let digest = hex_encode(&Sha256::digest(path.as_bytes()));
    let stem = parts_dir(root).join(format!("{}.get", &digest[..32]));
    let note = stem.with_extension("get.json");
    (stem, note)
}

/// Removes partial downloads nobody resumed within the window. Uploads clean
/// theirs in `upload::cleanup_parts`; both live in `.parts`.
fn cleanup_partial_downloads(root: &Path) {
    let Ok(entries) = std::fs::read_dir(parts_dir(root)) else { return };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.contains(".get") {
            continue;
        }
        let Ok(metadata) = entry.metadata() else { continue };
        let Ok(modified) = metadata.modified() else { continue };
        let stale = now.duration_since(modified).map(|age| age.as_secs() > PART_MAX_AGE_SECS).unwrap_or(false);
        if stale {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Reads the note of a partial download, if there is one.
fn read_note(path: &Path) -> Option<PartialNote> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

impl LanClient {
    /// Downloads a document in chunks, resuming from the partial file after a
    /// dropped connection and verifying the whole-file hash before returning.
    ///
    /// A host from before E6.3 does not know `begin_get`; the call falls back to
    /// the single-request [`LanClient::fetch`] so mixed versions keep working.
    pub fn fetch_resumable(
        &self,
        peer: &PairedPeer,
        path: &str,
        root: &Path,
        mut on_progress: impl FnMut(u64, u64),
    ) -> Result<(RemoteFile, Vec<u8>), SyncError> {
        match self.fetch_resumable_once(peer, path, root, &mut on_progress) {
            Err(error) if is_unknown_operation(&error) => {
                let (file, bytes) = self.fetch(peer, path)?;
                on_progress(bytes.len() as u64, bytes.len() as u64);
                Ok((file, bytes))
            }
            other => other,
        }
    }

    fn fetch_resumable_once(
        &self,
        peer: &PairedPeer,
        path: &str,
        root: &Path,
        on_progress: &mut impl FnMut(u64, u64),
    ) -> Result<(RemoteFile, Vec<u8>), SyncError> {
        cleanup_partial_downloads(root);
        let (partial, note_path) = partial_paths(root, path);
        let mut attempt = 0;
        loop {
            attempt += 1;
            match self.download_attempt(peer, path, root, &partial, &note_path, on_progress) {
                Ok(result) => return Ok(result),
                Err(error) if attempt < DOWNLOAD_ATTEMPTS => {
                    let _ = error;
                    let _ = std::fs::remove_file(&partial);
                    let _ = std::fs::remove_file(&note_path);
                }
                Err(error) => return Err(error),
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn download_attempt(
        &self,
        peer: &PairedPeer,
        path: &str,
        root: &Path,
        partial: &Path,
        note_path: &Path,
        on_progress: &mut impl FnMut(u64, u64),
    ) -> Result<(RemoteFile, Vec<u8>), SyncError> {
        let (announce, _) = self.request(peer, "begin_get", path, None, 0, 0, "")?;
        let total = announce.size;
        if total > crate::MAX_TRANSFER_BYTES {
            return Err(SyncError::TooLarge(crate::MAX_TRANSFER_BYTES));
        }
        std::fs::create_dir_all(parts_dir(root)).map_err(|error| SyncError::Network(error.to_string()))?;

        // Resume only when the partial file belongs to the document we are
        // about to fetch; anything else starts from zero.
        let mut have = std::fs::metadata(partial).map(|metadata| metadata.len()).unwrap_or(0);
        let note = read_note(note_path);
        let belongs = note.as_ref().is_some_and(|entry| entry.sha256 == announce.sha256 && entry.size == total);
        if !belongs || have > total {
            let _ = std::fs::remove_file(partial);
            have = 0;
        }
        if !belongs {
            let note = PartialNote { sha256: announce.sha256.clone(), size: total };
            let text = serde_json::to_string(&note).unwrap_or_default();
            std::fs::write(note_path, text).map_err(|error| SyncError::Network(error.to_string()))?;
        }

        {
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(partial)
                .map_err(|error| SyncError::Network(error.to_string()))?;
            while have < total {
                let (response, body) = self.request(peer, "get_chunk", path, None, have, total, "")?;
                if body.is_empty() {
                    return Err(SyncError::Network("the peer sent an empty chunk".into()));
                }
                if body.len() > MAX_CHUNK_BYTES {
                    return Err(SyncError::Network("the peer sent a chunk that is too large".into()));
                }
                let written = response.offset.saturating_sub(have);
                if written != body.len() as u64 {
                    return Err(SyncError::Network("the peer reported a different chunk length".into()));
                }
                file.write_all(&body).map_err(|error| SyncError::Network(error.to_string()))?;
                have += body.len() as u64;
                on_progress(have, total);
            }
        }

        let bytes = std::fs::read(partial).map_err(|error| SyncError::Network(error.to_string()))?;
        if bytes.len() as u64 != total || hex_encode(&Sha256::digest(&bytes)) != announce.sha256 {
            let _ = std::fs::remove_file(partial);
            let _ = std::fs::remove_file(note_path);
            return Err(SyncError::Network("the downloaded document does not match its hash".into()));
        }
        let _ = std::fs::remove_file(partial);
        let _ = std::fs::remove_file(note_path);
        let file = RemoteFile {
            path: path.to_string(),
            size: total,
            sha256: announce.sha256,
            modified: announce.modified,
        };
        Ok((file, bytes))
    }
}
