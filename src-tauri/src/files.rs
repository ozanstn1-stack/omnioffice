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
#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileRef {
    pub token: String,
    pub name: String,
    /// Only set for a save target: the human just picked it in the dialog, and
    /// the UI shows it. Pick tokens never carry a path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
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
    FileRef { token, name, path: None }
}

/// Resolves a token; a read-only token cannot be used for writing.
/// The migration gate the remaining command slices will move onto (#337). It is
/// kept (and exercised by the tests) even though the current command set still
/// resolves tokens through `lookup`; the targeted allowance keeps the file's
/// other warnings visible.
#[allow(dead_code)]
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
        // A write target the human picked is trusted for the follow-up
        // fingerprint/read commands (the path the command returns to the UI).
        if access == FileAccess::ReadWrite {
            crate::paths_policy::trust_path(&entry.path);
        }
        Some(Ok(entry.path.clone()))
    })
}

/// Opens the native file dialog on the Rust side and returns read tokens.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn file_token_pick(
    app: tauri::AppHandle,
    extensions: Option<Vec<String>>,
    multiple: Option<bool>,
) -> Result<Vec<FileRef>, String> {
    pick(&app, extensions, PickMode::Open { multiple: multiple.unwrap_or(false) }).await
}

/// Opens the native save dialog and returns a read-write token for the target.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn file_token_save(
    app: tauri::AppHandle,
    name: Option<String>,
    extensions: Option<Vec<String>>,
) -> Result<Vec<FileRef>, String> {
    pick(&app, extensions, PickMode::Save { name }).await
}

/// Forgets a token (best effort; an unknown token is not an error).
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub fn file_token_release(token: String) -> bool {
    revoke(&token)
}

/// Display names for values that may be tokens or plain paths. The webview
/// only ever needs the name, never the path behind a token.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub fn file_names(values: Vec<String>) -> Vec<Option<String>> {
    values
        .iter()
        .map(|value| {
            let path = lookup(value, FileAccess::Read)
                .and_then(|resolved| resolved.ok())
                .unwrap_or_else(|| PathBuf::from(value));
            path.file_name().map(|name| name.to_string_lossy().into_owned())
        })
        .collect()
}

/// Registers the paths of a window drop and emits only the tokens. Called from
/// the window event handler so the app code never receives the paths.
pub fn emit_drop_tokens<R: tauri::Runtime>(window: &tauri::Window<R>, paths: &[PathBuf]) {
    let refs: Vec<FileRef> =
        paths.iter().filter(|path| path.is_file()).map(|path| issue(path, FileAccess::Read)).collect();
    if !refs.is_empty() {
        let _ = tauri::Emitter::emit(window, "file-drop", refs);
    }
}

enum PickMode {
    Open { multiple: bool },
    Save { name: Option<String> },
}

async fn pick(app: &tauri::AppHandle, extensions: Option<Vec<String>>, mode: PickMode) -> Result<Vec<FileRef>, String> {
    use tauri_plugin_dialog::DialogExt;

    let mut builder = app.dialog().file();
    if let Some(extensions) = extensions.as_ref().filter(|list| !list.is_empty()) {
        let filters: Vec<&str> = extensions.iter().map(String::as_str).collect();
        builder = builder.add_filter("Documents", &filters);
    }
    match mode {
        PickMode::Save { name } => {
            let (sender, receiver) = std::sync::mpsc::channel();
            if let Some(name) = name {
                builder = builder.set_file_name(name);
            }
            builder.save_file(move |path| {
                let _ = sender.send(path);
            });
            let picked = await_path(receiver).await?;
            Ok(picked
                .and_then(|path| path.into_path().ok())
                .map(|path| {
                    let mut reference = issue(&path, FileAccess::ReadWrite);
                    reference.path = Some(path.display().to_string());
                    reference
                })
                .into_iter()
                .collect())
        }
        PickMode::Open { multiple } => {
            if multiple {
                let (sender, receiver) = std::sync::mpsc::channel();
                builder.pick_files(move |paths| {
                    let _ = sender.send(paths);
                });
                let picked = await_paths(receiver).await?;
                Ok(picked
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|path| path.into_path().ok())
                    .map(|path| issue(&path, FileAccess::Read))
                    .collect())
            } else {
                let (sender, receiver) = std::sync::mpsc::channel();
                builder.pick_file(move |path| {
                    let _ = sender.send(path);
                });
                let picked = await_path(receiver).await?;
                Ok(picked
                    .and_then(|path| path.into_path().ok())
                    .map(|path| issue(&path, FileAccess::Read))
                    .into_iter()
                    .collect())
            }
        }
    }
}

async fn await_path(
    receiver: std::sync::mpsc::Receiver<Option<tauri_plugin_dialog::FilePath>>,
) -> Result<Option<tauri_plugin_dialog::FilePath>, String> {
    tauri::async_runtime::spawn_blocking(move || receiver.recv().ok().flatten())
        .await
        .map_err(|_| "the file dialog failed".to_string())
}

async fn await_paths(
    receiver: std::sync::mpsc::Receiver<Option<Vec<tauri_plugin_dialog::FilePath>>>,
) -> Result<Option<Vec<tauri_plugin_dialog::FilePath>>, String> {
    tauri::async_runtime::spawn_blocking(move || receiver.recv().ok().flatten())
        .await
        .map_err(|_| "the file dialog failed".to_string())
}

#[cfg(test)]
#[path = "files_tests.rs"]
mod tests;
