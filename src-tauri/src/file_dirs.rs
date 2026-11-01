//! Directory capability tokens: folder-writing tools (PDF to images, split)
//! get an opaque token for the folder the human picked, resolved by
//! `paths::directory`. Kept apart from `files.rs` to respect the ratchet.

use crate::files::FileRef;
#[cfg(any(desktop, test))]
use crate::files::{issue, FileAccess};
#[cfg(any(desktop, test))]
use std::path::Path;
use tauri::Manager;

/// Registers a directory the human picked for output (the desktop folder
/// dialog; Android picks folders through the SAF picker).
///
/// The pick is also persisted as a grant (`paths_policy::persist_grant`), which
/// is what lets the configured default output folder keep working after a
/// restart without a live token (E1).
#[cfg(any(desktop, test))]
pub fn issue_dir(config_dir: Option<&Path>, path: &Path) -> FileRef {
    if let Some(config_dir) = config_dir {
        let _ = crate::paths_policy::persist_grant(config_dir, path);
    }
    let mut reference = issue(path, FileAccess::ReadWrite);
    reference.path = Some(path.display().to_string());
    reference
}

/// Opens the native folder dialog and returns a directory token. Android uses
/// the SAF picker instead, so the native folder dialog is desktop-only.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn file_token_pick_directory(app: tauri::AppHandle) -> Result<Option<FileRef>, pdfcore::error::PdfError> {
    #[cfg(desktop)]
    {
        use tauri_plugin_dialog::DialogExt;
        let (sender, receiver) = std::sync::mpsc::channel();
        app.dialog().file().pick_folder(move |path| {
            let _ = sender.send(path);
        });
        let picked = tauri::async_runtime::spawn_blocking(move || receiver.recv().ok().flatten())
            .await
            .map_err(|_| pdfcore::error::PdfError::Internal("the folder dialog failed".to_string()))?;
        let config_dir = app.path().app_config_dir().ok();
        Ok(picked.and_then(|path| path.into_path().ok()).map(|path| issue_dir(config_dir.as_deref(), &path)))
    }
    #[cfg(not(desktop))]
    {
        let _ = app;
        Err(pdfcore::error::PdfError::Unsupported("the folder picker is only available on desktop".to_string()))
    }
}
