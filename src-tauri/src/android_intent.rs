//! Android open-with bridge.
//!
//! Modern file managers hand documents to the app as `content://` URIs through
//! `ACTION_VIEW` / `ACTION_SEND` intents. The Kotlin shell (`MainActivity.kt`)
//! resolves a display name, copies each stream into `cacheDir/incoming/<id>/`
//! and appends the resulting absolute path to `cacheDir/pending-open.txt`.
//!
//! This module drains that queue for the webview. The file handling is kept in
//! a plain function so it can be unit tested on every platform.

#[cfg(any(target_os = "android", test))]
use std::path::Path;

/// Name of the queue file written by the Android shell.
#[cfg(any(target_os = "android", test))]
pub const PENDING_OPEN_FILE: &str = "pending-open.txt";

/// Reads the queue file and truncates it. Missing or unreadable files yield an
/// empty list; blank lines are skipped. Truncation happens before the caller
/// sees the paths so a document can never be imported twice.
#[cfg(any(target_os = "android", test))]
pub fn drain_pending_open(cache_dir: &Path) -> Vec<String> {
    let queue = cache_dir.join(PENDING_OPEN_FILE);
    let content = match std::fs::read_to_string(&queue) {
        Ok(content) => content,
        Err(_) => return Vec::new(),
    };
    // Best effort: a failed truncate must not hide the already copied files,
    // they are re-imported at most once more.
    let _ = std::fs::write(&queue, "");
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

/// Returns the paths of documents opened through an Android intent and clears
/// the queue. Always empty on desktop.
#[cfg(target_os = "android")]
#[tauri::command]
pub fn android_take_pending_open(app: tauri::AppHandle) -> Vec<String> {
    use tauri::Manager;
    match app.path().app_cache_dir() {
        Ok(cache) => drain_pending_open(&cache),
        Err(_) => Vec::new(),
    }
}

/// Desktop stub so `generate_handler!` stays platform independent.
#[cfg(not(target_os = "android"))]
#[tauri::command]
pub fn android_take_pending_open() -> Vec<String> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pdfsak-android-intent-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn write_queue(dir: &Path, content: &str) {
        let mut file = std::fs::File::create(dir.join(PENDING_OPEN_FILE)).expect("create queue");
        file.write_all(content.as_bytes()).expect("write queue");
    }

    #[test]
    fn drains_and_truncates_the_queue() {
        let dir = temp_dir("drain");
        write_queue(&dir, "/data/cache/incoming/a/one.docx\n/data/cache/incoming/b/two.pdf\n");

        let paths = drain_pending_open(&dir);
        assert_eq!(paths, vec!["/data/cache/incoming/a/one.docx", "/data/cache/incoming/b/two.pdf"]);
        assert_eq!(std::fs::read_to_string(dir.join(PENDING_OPEN_FILE)).unwrap(), "");

        // A second drain sees nothing.
        assert!(drain_pending_open(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_is_empty() {
        let dir = temp_dir("missing");
        assert!(drain_pending_open(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn skips_blank_and_padded_lines() {
        let dir = temp_dir("blank");
        write_queue(&dir, "\n  \n/data/cache/incoming/c/three.odt\n\n");
        assert_eq!(drain_pending_open(&dir), vec!["/data/cache/incoming/c/three.odt"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
