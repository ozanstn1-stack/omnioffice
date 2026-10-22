//! Capability tokens for user files.
//!
//! The webview should not need a filesystem path to use a file the user chose:
//! the Rust side opens the dialog, registers the chosen path and hands out an
//! opaque token. Commands that operate on user documents then resolve the
//! token instead of trusting a path from the renderer. A token is random,
//! scoped (read vs read-write), expires, and never serializes the path.
//!
//! This module is the foundation for the B3 migration (#337): the dialog
//! commands below are the only places that learn a path from the user, and
//! `resolve` is the single gate document commands will move onto in the
//! follow-up slices (PDF, office, tools). Until a command is migrated it still
//! accepts a path, so nothing breaks while the slices land.

#![allow(dead_code)] // esolve is used by the migration slices (#337); tests cover it now.

use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// What a token may be used for. A read-write token also allows reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileAccess {
    Read,
    ReadWrite,
}

/// What the webview gets: an opaque token and a display name, never a path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileRef {
    pub token: String,
    pub name: String,
}

struct Entry {
    path: PathBuf,
    access: FileAccess,
    issued: Instant,
}

/// Tokens live for one session-like window; reopening the app invalidates the
/// old ones, so a stolen token cannot outlive the process.
const TOKEN_TTL: Duration = Duration::from_secs(12 * 60 * 60);

fn registry() -> &'static Mutex<HashMap<String, Entry>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Entry>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn with_registry<T>(action: impl FnOnce(&mut HashMap<String, Entry>) -> T) -> T {
    let mut registry = registry().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let now = Instant::now();
    registry.retain(|_, entry| now.duration_since(entry.issued) < TOKEN_TTL);
    action(&mut registry)
}

/// Registers `path` and returns the token the webview may use instead.
pub fn issue(path: &Path, access: FileAccess) -> FileRef {
    let token = uuid::Uuid::new_v4().simple().to_string();
    let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    with_registry(|registry| {
        registry.insert(token.clone(), Entry { path: path.to_path_buf(), access, issued: Instant::now() });
    });
    FileRef { token, name }
}

/// Resolves a token; a read-only token cannot be used for writing.
pub fn resolve(token: &str, access: FileAccess) -> Result<PathBuf, String> {
    with_registry(|registry| {
        let entry = registry.get(token).ok_or_else(|| "the file token is unknown or expired".to_string())?;
        if access == FileAccess::ReadWrite && entry.access != FileAccess::ReadWrite {
            return Err("the file token was issued for reading only".to_string());
        }
        Ok(entry.path.clone())
    })
}

/// Drops a token early (closing a document, cancelling a save target).
pub fn revoke(token: &str) -> bool {
    with_registry(|registry| registry.remove(token).is_some())
}

/// The path behind a token when `value` is one; `None` means it is a plain
/// path (the caller keeps its usual path validation), so this is the bridge
/// the command layer uses while the migration is in progress.
pub fn lookup(value: &str, access: FileAccess) -> Option<Result<PathBuf, String>> {
    with_registry(|registry| {
        let entry = registry.get(value)?;
        if access == FileAccess::ReadWrite && entry.access != FileAccess::ReadWrite {
            return Some(Err("the file token was issued for reading only".to_string()));
        }
        Some(Ok(entry.path.clone()))
    })
}

/// Opens the native file dialog on the Rust side and returns a read token.
#[tauri::command]
pub async fn file_token_pick(
    app: tauri::AppHandle,
    extensions: Option<Vec<String>>,
) -> Result<Option<FileRef>, String> {
    pick(&app, extensions, None).await
}

/// Opens the native save dialog and returns a read-write token for the target.
#[tauri::command]
pub async fn file_token_save(
    app: tauri::AppHandle,
    name: Option<String>,
    extensions: Option<Vec<String>>,
) -> Result<Option<FileRef>, String> {
    pick(&app, extensions, name).await
}

/// Forgets a token (best effort; an unknown token is not an error).
#[tauri::command]
pub fn file_token_release(token: String) -> bool {
    revoke(&token)
}

async fn pick(
    app: &tauri::AppHandle,
    extensions: Option<Vec<String>>,
    save_name: Option<String>,
) -> Result<Option<FileRef>, String> {
    use tauri_plugin_dialog::DialogExt;

    let (sender, receiver) = std::sync::mpsc::channel();
    let mut builder = app.dialog().file();
    if let Some(extensions) = extensions.as_ref().filter(|list| !list.is_empty()) {
        let filters: Vec<&str> = extensions.iter().map(String::as_str).collect();
        builder = builder.add_filter("Documents", &filters);
    }
    let saving = save_name.is_some();
    if let Some(name) = save_name {
        builder = builder.set_file_name(name);
    }
    if saving {
        builder.save_file(move |path| {
            let _ = sender.send(path);
        });
    } else {
        builder.pick_file(move |path| {
            let _ = sender.send(path);
        });
    }
    let picked = tauri::async_runtime::spawn_blocking(move || receiver.recv().ok().flatten())
        .await
        .map_err(|_| "the file dialog failed".to_string())?;
    let Some(picked) = picked else {
        return Ok(None); // the user cancelled
    };
    let path = picked.into_path().map_err(|_| "the chosen file is not a local path".to_string())?;
    let access = if saving { FileAccess::ReadWrite } else { FileAccess::Read };
    Ok(Some(issue(&path, access)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pdfsak-token-{label}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let file = dir.join(format!("{label}.pdf"));
        std::fs::write(&file, b"%PDF-1.7\n").expect("temp file");
        file
    }

    #[test]
    fn a_token_resolves_to_the_registered_path_and_keeps_only_the_name() {
        let path = temp_file("resolve");
        let issued = issue(&path, FileAccess::Read);
        assert!(issued.name.ends_with("resolve.pdf"));
        assert!(!issued.name.contains(std::path::MAIN_SEPARATOR));
        assert_eq!(resolve(&issued.token, FileAccess::Read).expect("resolve"), path);
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn a_read_token_cannot_be_used_for_writing() {
        let path = temp_file("readonly");
        let issued = issue(&path, FileAccess::Read);
        assert!(resolve(&issued.token, FileAccess::ReadWrite).is_err());
        let writable = issue(&path, FileAccess::ReadWrite);
        assert_eq!(resolve(&writable.token, FileAccess::Read).expect("read via rw"), path);
        assert_eq!(resolve(&writable.token, FileAccess::ReadWrite).expect("write"), path);
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn releasing_a_token_invalidates_it() {
        let path = temp_file("release");
        let issued = issue(&path, FileAccess::Read);
        assert!(revoke(&issued.token));
        assert!(resolve(&issued.token, FileAccess::Read).is_err());
        assert!(!revoke(&issued.token), "a second release is a no-op");
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn unknown_tokens_are_rejected_and_issued_tokens_are_unique() {
        assert!(resolve("not-a-token", FileAccess::Read).is_err());
        let path = temp_file("unique");
        let first = issue(&path, FileAccess::Read);
        let second = issue(&path, FileAccess::Read);
        assert_ne!(first.token, second.token);
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }
}
