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

/// Name of the durable log the Android shell writes (`OpenWithLog.kt`).
#[cfg(any(target_os = "android", test))]
pub const OPEN_WITH_LOG_FILE: &str = "open-with.log";

/// The log cap mirrors `OpenWithLog.kt`: above the maximum, the newest bytes
/// are kept so a report stays small enough to copy by hand.
#[cfg(any(target_os = "android", test))]
const OPEN_WITH_LOG_MAX_BYTES: u64 = 128 * 1024;

/// Bytes kept by a trim; matches the Kotlin shell's `keepBytes`.
#[cfg(any(target_os = "android", test))]
const OPEN_WITH_LOG_KEEP_BYTES: usize = 64 * 1024;

/// One RFC 3339 timestamp for a log line (the Kotlin side writes the device's
/// offset, both are ISO 8601 and sort the same way in a report).
#[cfg(any(target_os = "android", test))]
fn open_with_timestamp() -> String {
    time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339).unwrap_or_default()
}

/// Appends one line to the Android shell's open-with log.
///
/// The queue drain calls this once per poll so the on-device report can tell
/// "nobody polls the queue" from "the queue was already empty"; the lines are
/// read back by `android_open_with_diagnostics`. A diagnostics write must
/// never break the open-with pipeline, so every failure is ignored.
#[cfg(any(target_os = "android", test))]
pub fn append_open_with_log(cache_dir: &Path, line: &str) {
    use std::io::Write as _;
    let path = cache_dir.join(OPEN_WITH_LOG_FILE);
    let flat = line.replace(['\n', '\r'], " ");
    let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };
    if writeln!(file, "{}\t{}", open_with_timestamp(), flat).is_err() {
        return;
    }
    // Kotlin trims on its own writes, but the webview polls every 2.5 s, so
    // drains alone can grow the file; apply the same cap here.
    let Ok(metadata) = std::fs::metadata(&path) else {
        return;
    };
    if metadata.len() <= OPEN_WITH_LOG_MAX_BYTES {
        return;
    }
    let Ok(bytes) = std::fs::read(&path) else {
        return;
    };
    let mut start = bytes.len().saturating_sub(OPEN_WITH_LOG_KEEP_BYTES);
    while start < bytes.len() && bytes[start] != b'\n' {
        start += 1;
    }
    if start < bytes.len() {
        start += 1;
    }
    let _ = std::fs::write(&path, &bytes[start..]);
}

/// Renders the in-app open-with report: the Rust side's cache paths and
/// pending queue, then the Kotlin shell's log. Missing or unreadable parts are
/// named in place of their contents; the function never fails, because a
/// diagnostic report is useless if gathering it can break the pipeline.
#[cfg(any(target_os = "android", test))]
pub fn build_open_with_diagnostics(cache_dir: Option<&Path>) -> String {
    let mut report = String::from("== open-with: Rust side ==\n");
    let Some(cache_dir) = cache_dir else {
        report.push_str("cache_dir: <unavailable>\n");
        report.push_str("\n== open-with: Kotlin log ==\nlog: <unavailable>\n");
        return report;
    };
    report.push_str(&format!("cache_dir: {}\n", cache_dir.display()));
    let queue = cache_dir.join(PENDING_OPEN_FILE);
    report.push_str(&format!("pending_open_path: {}\n", queue.display()));
    report.push_str(&format!("pending_open_exists: {}\n", queue.is_file()));
    report.push_str("pending_open_contents:\n");
    match std::fs::read_to_string(&queue) {
        Ok(contents) => {
            report.push_str(&contents);
            if !contents.is_empty() && !contents.ends_with('\n') {
                report.push('\n');
            }
        }
        Err(_) => report.push_str("(missing or unreadable)\n"),
    }
    report.push_str("\n== open-with: Kotlin log ==\nlog:\n");
    match std::fs::read_to_string(cache_dir.join(OPEN_WITH_LOG_FILE)) {
        Ok(text) if !text.is_empty() => {
            report.push_str(&text);
            if !text.ends_with('\n') {
                report.push('\n');
            }
        }
        _ => report.push_str("(empty or missing)\n"),
    }
    report
}

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
    content.lines().map(str::trim).filter(|line| !line.is_empty()).map(str::to_string).collect()
}

/// Set once the first drain of this process has been logged.
#[cfg(target_os = "android")]
static FIRST_DRAIN_LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether a drain is worth a log line: the first one of the process (it shows
/// that the webview polls at all) and every drain that returned something.
///
/// Why: the webview polls every 2.5 s, and logging every empty drain filled
/// the 64 KB log within minutes, pushing out the intent and copy lines a
/// report needs (seen on a device with v6.2.9).
#[cfg(any(target_os = "android", test))]
pub fn should_log_drain(first_logged: &std::sync::atomic::AtomicBool, count: usize) -> bool {
    let first = !first_logged.swap(true, std::sync::atomic::Ordering::Relaxed);
    first || count > 0
}

/// Returns the paths of documents opened through an Android intent and clears
/// the queue. Always empty on desktop.
///
/// `async` is load-bearing, not cosmetic: Tauri runs synchronous commands on
/// the main thread, and on Android `app.path()` resolves the cache directory
/// through a mobile-plugin round trip (`run_mobile_plugin("getCacheDir")`) that
/// dispatches the request to the Android main thread and blocks until it
/// answers. A synchronous command would wait for the very thread it is
/// occupying, so the IPC call never resolves and the webview never receives a
/// path - the reported "share opens the app but stays on Home". Running on the
/// async runtime keeps that wait off the main thread.
#[cfg(target_os = "android")]
#[tauri::command]
pub async fn android_take_pending_open(app: tauri::AppHandle) -> Vec<String> {
    use tauri::Manager;
    match app.path().app_cache_dir() {
        Ok(cache) => {
            let paths = drain_pending_open(&cache);
            if should_log_drain(&FIRST_DRAIN_LOGGED, paths.len()) {
                append_open_with_log(&cache, &format!("drain count={} cache={}", paths.len(), cache.display()));
            }
            paths
        }
        Err(_) => Vec::new(),
    }
}

/// Desktop stub so `generate_handler!` stays platform independent; the typed
/// bindings (exported on desktop) take their signature from it.
#[cfg(not(target_os = "android"))]
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub fn android_take_pending_open() -> Vec<String> {
    Vec::new()
}

/// Human-readable snapshot of the open-with pipeline for the Settings screen.
///
/// Why: the "sharing opens the app but the document never arrives" bug was
/// reported from a phone without adb, so the usual logcat workflow is not
/// available. The user reproduces once, opens Settings -> Diagnostics and
/// copies this report, which carries the Rust queue state and the Kotlin log.
///
/// `async` for the same reason as `android_take_pending_open`: `app.path()` is
/// a blocking mobile-plugin round trip to the Android main thread, so a
/// synchronous command would freeze the UI it needs and the report could never
/// be copied.
#[cfg(target_os = "android")]
#[tauri::command]
pub async fn android_open_with_diagnostics(app: tauri::AppHandle) -> String {
    use tauri::Manager;
    build_open_with_diagnostics(app.path().app_cache_dir().ok().as_deref())
}

/// Desktop stub so `generate_handler!` stays platform independent; the typed
/// bindings (exported on desktop) take their signature from it.
#[cfg(not(target_os = "android"))]
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub fn android_open_with_diagnostics() -> String {
    String::new()
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
    fn only_the_first_and_non_empty_drains_are_logged() {
        let first = std::sync::atomic::AtomicBool::new(false);
        assert!(should_log_drain(&first, 0), "the first drain shows the webview polls");
        assert!(!should_log_drain(&first, 0), "later empty drains must not flood the log");
        assert!(!should_log_drain(&first, 0));
        assert!(should_log_drain(&first, 1), "a drain that delivered a document is always logged");
        assert!(!should_log_drain(&first, 0));
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

    #[test]
    fn report_lists_cache_paths_queue_and_log() {
        let dir = temp_dir("report");
        write_queue(&dir, "/data/cache/incoming/a/one.docx\n");
        std::fs::write(
            dir.join(OPEN_WITH_LOG_FILE),
            "2026-01-01T00:00:00.000Z\tintent action=android.intent.action.VIEW\n",
        )
        .expect("write log");

        let report = build_open_with_diagnostics(Some(&dir));
        assert!(report.contains("== open-with: Rust side =="));
        assert!(report.contains(&format!("cache_dir: {}", dir.display())));
        assert!(report.contains("pending_open_exists: true"));
        assert!(report.contains("/data/cache/incoming/a/one.docx"));
        assert!(report.contains("== open-with: Kotlin log =="));
        assert!(report.contains("intent action=android.intent.action.VIEW"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn report_names_missing_parts_instead_of_failing() {
        let dir = temp_dir("report-missing");
        let report = build_open_with_diagnostics(Some(&dir));
        assert!(report.contains("pending_open_exists: false"));
        assert!(report.contains("(missing or unreadable)"));
        assert!(report.contains("(empty or missing)"));
        assert!(build_open_with_diagnostics(None).contains("cache_dir: <unavailable>"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn drain_lines_append_to_the_open_with_log() {
        let dir = temp_dir("log");
        append_open_with_log(&dir, "drain count=0 cache=/tmp/cache");
        append_open_with_log(&dir, "drain count=2 cache=/tmp/cache\nsecond line");
        let text = std::fs::read_to_string(dir.join(OPEN_WITH_LOG_FILE)).expect("read log");
        assert_eq!(text.lines().count(), 2);
        assert!(text.contains("drain count=0 cache=/tmp/cache"));
        // A caller-supplied newline never breaks the one-entry-per-line format.
        assert!(text.contains("drain count=2 cache=/tmp/cache second line"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
