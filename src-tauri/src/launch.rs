//! Files the OS hands to the app: the command line of the first launch
//! (Windows file associations / "Open with") and, on desktop, the command lines
//! of later launches, which the single-instance plugin forwards to the running
//! app instead of starting a second one.
//!
//! Forwarded files go through a queue plus a bare `launch:files-queued` event
//! instead of an event that carries the paths. Selecting several documents in
//! Explorer and pressing Enter starts one process per file at (almost) the same
//! moment, long before the first instance's webview has registered its
//! listener, so a payload event would simply be lost. The frontend drains the
//! queue when the event arrives and once more right after it starts listening.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::State;

/// What a command line yielded: the documents to open and, for the log and
/// the "could not open" notice, the arguments that looked like documents but
/// were not (file name and reason only, never a full path).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct LaunchScan {
    pub files: Vec<String>,
    pub skipped: Vec<String>,
}

/// One argument as a path: surrounding quotes removed and a `file:` URI
/// (some apps hand those to "Open with") turned into a local path.
fn argument_path(argument: &str) -> PathBuf {
    let trimmed = argument.trim().trim_matches('"');
    if trimmed.len() > 5 && trimmed.get(..5).is_some_and(|prefix| prefix.eq_ignore_ascii_case("file:")) {
        if let Some(path) = url::Url::parse(trimmed).ok().and_then(|url| url.to_file_path().ok()) {
            return path;
        }
    }
    PathBuf::from(trimmed)
}

fn resolve(path: PathBuf, base: Option<&Path>) -> PathBuf {
    match base {
        Some(base) if path.is_relative() => base.join(path),
        _ => path,
    }
}

fn display_name(path: &Path) -> String {
    path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| "?".into())
}

/// Existing files among command-line arguments. The first argument (the
/// executable) and anything that looks like a flag are ignored. A relative
/// path is resolved against `base`, the working directory of the process that
/// received the arguments; without a base it is taken as it is.
///
/// An association registered without quotes around `%1` splits a path that
/// contains spaces ("C:\Users\Ozan\My Documents\a b.pdf") into several
/// arguments; consecutive arguments are joined back until they name a file.
pub fn scan_launch_args<I: IntoIterator<Item = String>>(args: I, base: Option<&Path>) -> LaunchScan {
    let arguments: Vec<String> = args.into_iter().skip(1).collect();
    let mut scan = LaunchScan::default();
    let mut index = 0;
    while index < arguments.len() {
        let argument = &arguments[index];
        index += 1;
        if argument.starts_with('-') || argument.trim().is_empty() {
            continue;
        }
        let path = resolve(argument_path(argument), base);
        if path.is_file() {
            scan.files.push(path.to_string_lossy().to_string());
            continue;
        }
        // Rejoin a path the shell split on spaces.
        let mut joined = argument.clone();
        let mut found = None;
        for (offset, next) in arguments[index..].iter().enumerate() {
            if next.starts_with('-') {
                break;
            }
            joined.push(' ');
            joined.push_str(next);
            let candidate = resolve(argument_path(&joined), base);
            if candidate.is_file() {
                found = Some((candidate, offset + 1));
                break;
            }
        }
        if let Some((candidate, consumed)) = found {
            scan.files.push(candidate.to_string_lossy().to_string());
            index += consumed;
            continue;
        }
        let reason = if path.is_dir() { "a folder" } else { "not found" };
        scan.skipped.push(format!("{} ({reason})", display_name(&path)));
    }
    scan
}

/// The documents among command-line arguments (see `scan_launch_args`).
#[cfg(test)]
pub fn launch_files<I: IntoIterator<Item = String>>(args: I, base: Option<&Path>) -> Vec<String> {
    scan_launch_args(args, base).files
}

/// Appends one line to `launch.log` in the app log folder: how many arguments
/// arrived, how many became documents and which were skipped (names only).
pub fn log_scan(app: &tauri::AppHandle, origin: &str, argument_count: usize, scan: &LaunchScan) {
    use tauri::Manager;
    let Ok(dir) = app.path().app_log_dir() else {
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    let timestamp =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let line = format!(
        "[{timestamp}] {origin}: arguments={argument_count} documents={} skipped={:?}\n",
        scan.files.len(),
        scan.skipped
    );
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("launch.log"))
        .and_then(|mut file| std::io::Write::write_all(&mut file, line.as_bytes()));
}

/// A later launch's command line (single instance): queue its documents and
/// skipped arguments, log the scan and wake the frontend.
#[cfg(desktop)]
pub fn forward(app: &tauri::AppHandle, argv: Vec<String>, cwd: &str) {
    use tauri::{Emitter, Manager};
    let count = argv.len().saturating_sub(1);
    let scan = scan_launch_args(argv, Some(Path::new(cwd)));
    if count > 0 {
        log_scan(app, "forwarded", count, &scan);
    }
    if scan.files.is_empty() && scan.skipped.is_empty() {
        return;
    }
    if let Some(queue) = app.try_state::<LaunchQueue>() {
        queue.push(scan.files);
        queue.push_skipped(scan.skipped);
    }
    let _ = app.emit("launch:files-queued", ());
}

/// Paths forwarded by later launches that the frontend has not picked up yet,
/// plus the names of arguments that could not be opened (for the notice).
#[derive(Default)]
pub struct LaunchQueue(Mutex<Vec<String>>, Mutex<Vec<String>>);

impl LaunchQueue {
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<String>> {
        // A poisoned lock only means a panic elsewhere; the list is still valid.
        self.0.lock().unwrap_or_else(|poison| poison.into_inner())
    }

    #[cfg(desktop)]
    pub fn push(&self, files: Vec<String>) {
        self.lock().extend(files);
    }

    /// Remembers arguments that could not be opened, for the next notice.
    pub fn push_skipped(&self, skipped: Vec<String>) {
        self.1.lock().unwrap_or_else(|poison| poison.into_inner()).extend(skipped);
    }

    /// Hands out the skipped arguments once.
    pub fn take_skipped(&self) -> Vec<String> {
        std::mem::take(&mut *self.1.lock().unwrap_or_else(|poison| poison.into_inner()))
    }

    /// Hands out everything queued so far, oldest first, and empties the queue.
    pub fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.lock())
    }
}

/// Drains the files forwarded by later launches (empty on mobile). Internal:
/// it is called by `startup_tokens`, never by the webview, so it carries no
/// command attribute (an unregistered command attribute only inflates the
/// counted command surface).
fn issue_tokens(paths: Vec<String>) -> Vec<crate::files::FileRef> {
    paths
        .into_iter()
        .map(|path| crate::files::issue(std::path::Path::new(&path), crate::files::FileAccess::Read))
        .collect()
}

/// The files of this launch as capability tokens (never paths). Also logs the
/// scan and keeps the skipped arguments for the "could not open" notice.
pub fn startup_tokens_logged(app: &tauri::AppHandle, args: Vec<String>) -> Vec<crate::files::FileRef> {
    use tauri::Manager;
    let count = args.len().saturating_sub(1);
    let scan = scan_launch_args(args, None);
    if count > 0 {
        log_scan(app, "startup", count, &scan);
    }
    if let Some(queue) = app.try_state::<LaunchQueue>() {
        queue.push_skipped(scan.skipped.clone());
    }
    issue_tokens(scan.files)
}

#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub fn office_take_launch_files(queue: State<'_, LaunchQueue>) -> Vec<crate::files::FileRef> {
    issue_tokens(queue.take())
}

/// Names (with the reason) of launch arguments that could not be opened since
/// the last call, so the app can say so instead of staying silently on Home.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub fn office_take_launch_skipped(queue: State<'_, LaunchQueue>) -> Vec<String> {
    queue.take_skipped()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn only_existing_files_survive_and_the_executable_and_flags_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("report.docx");
        std::fs::write(&file, b"x").unwrap();
        let file_text = file.to_string_lossy().to_string();
        let found = launch_files(
            args(&[
                &file_text, // argv[0] is the executable, never a document
                &file_text,
                "--flag",
                "-x",
                &dir.path().join("missing.docx").to_string_lossy(),
                &dir.path().to_string_lossy(), // a directory is not a file
            ]),
            None,
        );
        assert_eq!(found, vec![file_text]);
    }

    #[test]
    fn relative_paths_resolve_against_the_forwarding_processes_directory() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("budget.xlsx"), b"x").unwrap();
        let found = launch_files(args(&["omnioffice.exe", "budget.xlsx", "nothing.xlsx"]), Some(dir.path()));
        assert_eq!(found, vec![dir.path().join("budget.xlsx").to_string_lossy().to_string()]);
    }

    #[test]
    fn an_absolute_path_ignores_the_base() {
        let dir = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let file = dir.path().join("deck.pptx");
        std::fs::write(&file, b"x").unwrap();
        let file_text = file.to_string_lossy().to_string();
        assert_eq!(launch_files(args(&["omnioffice.exe", &file_text]), Some(other.path())), vec![file_text]);
    }

    #[test]
    fn a_path_split_on_spaces_is_joined_back() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("My Documents");
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("yıllık rapor.pdf");
        std::fs::write(&file, b"x").unwrap();
        let text = file.to_string_lossy().to_string();
        let parts: Vec<&str> = text.split(' ').collect();
        let mut argv = vec!["omnioffice.exe"];
        argv.extend(parts.iter().copied());
        let scan = scan_launch_args(args(&argv), None);
        assert_eq!(scan.files, vec![text]);
        assert!(scan.skipped.is_empty());
    }

    #[test]
    fn quotes_and_file_uris_are_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("teklif mektubu.docx");
        std::fs::write(&file, b"x").unwrap();
        let text = file.to_string_lossy().to_string();
        let uri = url::Url::from_file_path(&file).unwrap().to_string();
        assert_eq!(launch_files(args(&["omnioffice.exe", &format!("\"{text}\"")]), None), vec![text.clone()]);
        assert_eq!(launch_files(args(&["omnioffice.exe", &uri]), None), vec![text]);
    }

    #[test]
    fn missing_documents_are_reported_by_name_only() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("gizli").join("kayıp.pdf");
        let scan = scan_launch_args(args(&["omnioffice.exe", &missing.to_string_lossy(), "--flag"]), None);
        assert!(scan.files.is_empty());
        assert_eq!(scan.skipped, vec!["kayıp.pdf (not found)".to_string()]);
    }

    #[test]
    fn the_skipped_queue_is_drained_once() {
        let queue = LaunchQueue::default();
        queue.push_skipped(vec!["a.pdf (not found)".into()]);
        assert_eq!(queue.take_skipped(), vec!["a.pdf (not found)".to_string()]);
        assert!(queue.take_skipped().is_empty());
    }

    #[test]
    fn no_arguments_means_no_files() {
        assert!(launch_files(Vec::<String>::new(), None).is_empty());
        assert!(launch_files(args(&["omnioffice.exe"]), None).is_empty());
    }

    #[test]
    fn the_queue_is_drained_once_and_keeps_arrival_order() {
        let queue = LaunchQueue::default();
        assert!(queue.take().is_empty());
        queue.push(args(&["a.docx", "b.docx"]));
        queue.push(args(&["c.pdf"]));
        assert_eq!(queue.take(), args(&["a.docx", "b.docx", "c.pdf"]));
        assert!(queue.take().is_empty(), "a second drain must not repeat files");
    }
}
