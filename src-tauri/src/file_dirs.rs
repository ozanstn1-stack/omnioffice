//! Directory capability tokens: folder-writing tools (PDF to images, split)
//! get an opaque token for the folder the human picked, resolved by
//! `paths::directory`. Kept apart from `files.rs` to respect the ratchet.

use crate::files::{issue, FileAccess, FileRef};
use std::path::Path;

/// Registers a directory the human picked for output.
pub fn issue_dir(path: &Path) -> FileRef {
    let mut reference = issue(path, FileAccess::ReadWrite);
    reference.path = Some(path.display().to_string());
    reference
}

/// Opens the native folder dialog and returns a directory token.
#[tauri::command]
pub async fn file_token_pick_directory(app: tauri::AppHandle) -> Result<Option<FileRef>, String> {
    use tauri_plugin_dialog::DialogExt;
    let (sender, receiver) = std::sync::mpsc::channel();
    app.dialog().file().pick_folder(move |path| {
        let _ = sender.send(path);
    });
    let picked = tauri::async_runtime::spawn_blocking(move || receiver.recv().ok().flatten())
        .await
        .map_err(|_| "the folder dialog failed".to_string())?;
    Ok(picked.and_then(|path| path.into_path().ok()).map(|path| issue_dir(&path)))
}
