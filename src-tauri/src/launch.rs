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

/// Existing files among command-line arguments. The first argument (the
/// executable) and anything that looks like a flag are ignored. A relative
/// path is resolved against `base`, the working directory of the process that
/// received the arguments; without a base it is taken as it is.
pub fn launch_files<I: IntoIterator<Item = String>>(args: I, base: Option<&Path>) -> Vec<String> {
    args.into_iter()
        .skip(1)
        .filter(|argument| !argument.starts_with('-'))
        .map(PathBuf::from)
        .map(|path| match base {
            Some(base) if path.is_relative() => base.join(path),
            _ => path,
        })
        .filter(|path| path.is_file())
        .map(|path| path.to_string_lossy().to_string())
        .collect()
}

/// Paths forwarded by later launches that the frontend has not picked up yet.
#[derive(Default)]
pub struct LaunchQueue(Mutex<Vec<String>>);

impl LaunchQueue {
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<String>> {
        // A poisoned lock only means a panic elsewhere; the list is still valid.
        self.0.lock().unwrap_or_else(|poison| poison.into_inner())
    }

    #[cfg(desktop)]
    pub fn push(&self, files: Vec<String>) {
        self.lock().extend(files);
    }

    /// Hands out everything queued so far, oldest first, and empties the queue.
    pub fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.lock())
    }
}

/// Drains the files forwarded by later launches (empty on mobile).
#[tauri::command]
pub fn office_take_launch_files(queue: State<'_, LaunchQueue>) -> Vec<String> {
    queue.take()
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
