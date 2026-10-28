//! Recent files: stored as real paths in `<config>/recent.json`; the webview
//! only receives capability tokens and display names, so a recent entry can be
//! reopened without ever exposing a path and survives a restart (the token is
//! re-issued on load).

use crate::commands::{config_dir, read_config_text, write_atomic};
use crate::files::{self, FileAccess};
use pdfcore::PdfError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::AppHandle;

/// The stored shape (a real path and its metadata).
#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RecentEntry {
    pub path: String,
    pub file_name: String,
    pub tool: String,
    pub timestamp: u64,
}

/// What the webview receives: an opaque token and the display name.
#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentRef {
    pub token: String,
    pub file_name: String,
    pub tool: String,
    pub timestamp: u64,
}

/// Turns stored entries into token refs, dropping files that no longer exist.
pub fn refs(entries: Vec<RecentEntry>) -> Vec<RecentRef> {
    entries
        .into_iter()
        .filter(|entry| Path::new(&entry.path).exists())
        .map(|entry| {
            let token = files::issue(Path::new(&entry.path), FileAccess::Read).token;
            RecentRef { token, file_name: entry.file_name, tool: entry.tool, timestamp: entry.timestamp }
        })
        .collect()
}

#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub fn load_recent(app: AppHandle) -> Result<Vec<RecentRef>, PdfError> {
    let path = config_dir(&app)?.join("recent.json");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = read_config_text(&path)?;
    let mut entries: Vec<RecentEntry> = serde_json::from_str(&text).unwrap_or_default();
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.timestamp));
    entries.truncate(30);
    Ok(refs(entries))
}

#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub fn add_recent(app: AppHandle, entry: RecentEntry) -> Result<(), PdfError> {
    let path = config_dir(&app)?.join("recent.json");
    let mut entries: Vec<RecentEntry> = if path.exists() {
        read_config_text(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    } else {
        Vec::new()
    };
    // The caller may pass a token (dialog-picked file) or a real output path;
    // the store always keeps the real path.
    let real = files::lookup(&entry.path, FileAccess::Read)
        .and_then(|resolved| resolved.ok())
        .unwrap_or_else(|| PathBuf::from(&entry.path));
    let mut entry = entry;
    entry.path = real.display().to_string();
    if entry.file_name.trim().is_empty() {
        entry.file_name = real.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    }
    entries.retain(|existing| existing.path != entry.path);
    entries.insert(0, entry);
    entries.truncate(30);
    let text =
        serde_json::to_string(&entries).map_err(|e| PdfError::Internal(format!("recent serialize error: {e}")))?;
    write_atomic(&path, text.as_bytes())
}

#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub fn clear_recent(app: AppHandle) -> Result<(), PdfError> {
    let path = config_dir(&app)?.join("recent.json");
    if path.exists() {
        std::fs::remove_file(&path).map_err(PdfError::from_io)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "recent_tests.rs"]
mod tests;
