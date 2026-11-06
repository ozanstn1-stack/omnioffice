//! Resumable uploads (D3): the host half.
//!
//! The partial file lives in `<root>/.parts/<hash of the relative path>.part`,
//! so a dropped connection resumes at the byte offset the host already has and
//! no user-controlled name ever reaches the filesystem. The client half
//! (`push_resumable`) stays next to the other `LanClient` methods in `lan.rs`.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use sha2::{Digest, Sha256};

use super::{
    file_modified, hex_encode, refusal, resolve_path, Failure, HostState, Request, Response, MAX_CHUNK_BYTES,
    PART_MAX_AGE_SECS,
};

// ---------------------------------------------------------------------------
// Resumable uploads (D3)
// ---------------------------------------------------------------------------

fn parts_dir(root: &Path) -> PathBuf {
    root.join(".parts")
}

/// One partial upload per relative path; the name is the hash of the path, so
/// no user-controlled component reaches the filesystem as a name.
fn part_path(root: &Path, path: &str) -> Result<PathBuf, Failure> {
    let digest = hex_encode(&Sha256::digest(path.as_bytes()));
    Ok(parts_dir(root).join(format!("{}.part", &digest[..32])))
}

/// Removes partial uploads nobody resumed within the window.
fn cleanup_parts(root: &Path) {
    let Ok(entries) = std::fs::read_dir(parts_dir(root)) else { return };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let Ok(metadata) = entry.metadata() else { continue };
        let Ok(modified) = metadata.modified() else { continue };
        let stale = now.duration_since(modified).map(|age| age.as_secs() > PART_MAX_AGE_SECS).unwrap_or(false);
        if stale {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

pub(super) fn handle_begin_put(state: &Arc<HostState>, request: &Request) -> Result<Response, Failure> {
    resolve_path(&state.root, &request.path).map_err(|error| refusal(error.to_string()))?;
    if request.total == 0 || request.total > crate::MAX_TRANSFER_BYTES {
        return Err(refusal("the announced document size is not acceptable"));
    }
    cleanup_parts(&state.root);
    std::fs::create_dir_all(parts_dir(&state.root))
        .map_err(|error| refusal(format!("the upload directory could not be created: {error}")))?;
    let part = part_path(&state.root, &request.path)?;
    let mut offset = std::fs::metadata(&part).map(|metadata| metadata.len()).unwrap_or(0);
    if offset > request.total {
        let _ = std::fs::remove_file(&part);
        offset = 0;
    }
    Ok(Response { ok: true, offset, ..Response::default() })
}

pub(super) fn handle_put_chunk(state: &Arc<HostState>, request: &Request, body: &[u8]) -> Result<Response, Failure> {
    if body.is_empty() || body.len() > MAX_CHUNK_BYTES {
        return Err(refusal("the chunk size is not acceptable"));
    }
    let part = part_path(&state.root, &request.path)?;
    let current = std::fs::metadata(&part).map(|metadata| metadata.len()).unwrap_or(0);
    if current != request.offset {
        return Err(refusal(format!("the upload is at offset {current}, not {}", request.offset)));
    }
    let total = request.total.max(current);
    if current + body.len() as u64 > total {
        return Err(refusal("the chunk exceeds the announced document size"));
    }
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&part)
        .map_err(|error| refusal(format!("the partial upload could not be opened: {error}")))?;
    file.write_all(body).map_err(|error| refusal(format!("the chunk could not be written: {error}")))?;
    Ok(Response { ok: true, offset: current + body.len() as u64, ..Response::default() })
}

pub(super) fn handle_finish_put(state: &Arc<HostState>, request: &Request) -> Result<Response, Failure> {
    let part = part_path(&state.root, &request.path)?;
    let current = std::fs::metadata(&part).map(|metadata| metadata.len()).unwrap_or(0);
    if current != request.total {
        return Err(refusal(format!("the upload is incomplete ({current} of {} bytes)", request.total)));
    }
    let mut file =
        std::fs::File::open(&part).map_err(|error| refusal(format!("the partial upload is unreadable: {error}")))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read =
            file.read(&mut buffer).map_err(|error| refusal(format!("the partial upload is unreadable: {error}")))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let digest = hex_encode(&hasher.finalize());
    if digest != request.sha256 {
        // A poisoned part must not be resumed: the client starts over.
        let _ = std::fs::remove_file(&part);
        return Err(refusal("the document hash does not match"));
    }
    let target = resolve_path(&state.root, &request.path).map_err(|error| refusal(error.to_string()))?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| refusal(format!("the folder could not be created: {error}")))?;
    }
    if target.exists() {
        std::fs::remove_file(&target)
            .map_err(|error| refusal(format!("the old document could not be replaced: {error}")))?;
    }
    std::fs::rename(&part, &target)
        .map_err(|error| refusal(format!("the document could not be moved into place: {error}")))?;
    let size = std::fs::metadata(&target).map(|metadata| metadata.len()).unwrap_or(request.total);
    Ok(Response { ok: true, size, sha256: digest, modified: file_modified(&target), ..Response::default() })
}
