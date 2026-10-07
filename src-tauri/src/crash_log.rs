//! Panic record for release builds.
//!
//! The release profile is `panic = "abort"`: a Rust panic ends the process
//! immediately and, without this hook, leaves nothing behind but "the app
//! closed". The hook appends one block per panic to `crash.log` in the app log
//! directory (next to `frontend.log`) and then chains to the previous hook, so
//! the default stderr report and the abort behave exactly as before.
//!
//! A block holds the time, app version, thread, message, source location and a
//! backtrace. Release builds are stripped, so the backtrace there is mostly
//! addresses; the message and location are what identify the failure. Paths are
//! masked like the diagnostics export does: a panic message can quote a
//! document path (`<path>.pdf`), and source locations are reduced to their last
//! components so a developer's home directory never ends up in a bug report.
//! The Settings diagnostics export appends the tail of this file.
//!
//! The hook is installed first thing in `run()`; the log directory only exists
//! once the app is built, so a panic before then is not recorded (it still goes
//! to stderr through the previous hook).

use crate::diagnostics::redact_paths;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// File name inside the app log directory.
pub const CRASH_LOG: &str = "crash.log";

/// When the file grows past this it is moved to `crash.log.old` (replacing an
/// older one), so a crash loop cannot fill the disk.
const MAX_LOG_BYTES: u64 = 256 * 1024;

/// Path components kept from an absolute source path.
const LOCATION_COMPONENTS: usize = 4;

static LOG_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Tells the hook where `crash.log` lives; called once the app log directory
/// is known. Later calls are ignored.
pub fn set_log_dir(dir: PathBuf) {
    let _ = LOG_DIR.set(dir);
}

/// Installs the panic hook and chains to whatever hook was set before.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        record_panic(info);
        previous(info);
    }));
}

fn record_panic(info: &std::panic::PanicHookInfo<'_>) {
    // Nothing below may panic: a panic inside the hook aborts without a record.
    let Some(dir) = LOG_DIR.get() else { return };
    let location = info.location().map(|place| format!("{}:{}:{}", place.file(), place.line(), place.column()));
    let thread = std::thread::current();
    let report = format_report(
        env!("CARGO_PKG_VERSION"),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
        thread.name(),
        &panic_message(info.payload()),
        location.as_deref(),
        &std::backtrace::Backtrace::force_capture().to_string(),
    );
    let _ = append_report(&dir.join(CRASH_LOG), &report);
}

/// The text of a panic payload (`panic!("...")` carries a `&str` or `String`).
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_string()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "<non-string panic payload>".to_string()
    }
}

/// Splits a trailing `:line` / `:line:column` off a source location. A drive
/// letter colon (`C:\...`) is never mistaken for one because its tail is not
/// numeric.
fn split_line_column(text: &str) -> (&str, &str) {
    let mut end = text.len();
    for _ in 0..2 {
        match text[..end].rfind(':') {
            Some(index) if is_number(&text[index + 1..end]) => end = index,
            _ => break,
        }
    }
    text.split_at(end)
}

fn is_number(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

/// Reduces a source path to something safe and still meaningful: relative
/// paths stay as they are, registry crates keep `<crate>-<version>/src/...`,
/// the standard library keeps `library/...` and any other absolute path keeps
/// its last [`LOCATION_COMPONENTS`] components.
fn tidy_source_path(file: &str) -> String {
    let normalized = file.replace('\\', "/");
    let absolute = normalized.starts_with('/') || normalized.as_bytes().get(1) == Some(&b':');
    if !absolute {
        return normalized;
    }
    if let Some((_, rest)) = normalized.split_once("/registry/src/") {
        // Skip the `index.crates.io-<hash>` directory.
        if let Some((_, crate_path)) = rest.split_once('/') {
            return crate_path.to_string();
        }
    }
    if let Some((_, rest)) = normalized.split_once("/library/") {
        return format!("library/{rest}");
    }
    let parts: Vec<&str> = normalized.split('/').filter(|part| !part.is_empty()).collect();
    parts[parts.len().saturating_sub(LOCATION_COMPONENTS)..].join("/")
}

/// `path:line:column` with the path tidied.
fn tidy_location(location: &str) -> String {
    let (file, line_column) = split_line_column(location);
    format!("{}{}", tidy_source_path(file), line_column)
}

/// Tidies the `at <file>:<line>` lines of a backtrace; frame lines are kept.
fn tidy_backtrace(backtrace: &str) -> String {
    let mut out = String::with_capacity(backtrace.len());
    for line in backtrace.lines() {
        let trimmed = line.trim_start();
        match trimmed.strip_prefix("at ") {
            Some(location) => {
                let indent = &line[..line.len() - trimmed.len()];
                let _ = writeln!(out, "{indent}at {}", tidy_location(location));
            }
            None => {
                let _ = writeln!(out, "{line}");
            }
        }
    }
    out
}

/// Builds one crash-log block (pure; see the unit tests for the exact shape).
fn format_report(
    version: &str,
    timestamp: u64,
    thread: Option<&str>,
    message: &str,
    location: Option<&str>,
    backtrace: &str,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "[{timestamp}] panic in OmniOffice {version}, thread '{}'", thread.unwrap_or("<unnamed>"));
    let mut lines = message.lines();
    let _ = writeln!(out, "  message: {}", redact_paths(lines.next().unwrap_or("")));
    for line in lines {
        let _ = writeln!(out, "    {}", redact_paths(line));
    }
    if let Some(location) = location {
        let _ = writeln!(out, "  at {}", tidy_location(location));
    }
    let _ = writeln!(out, "  backtrace:");
    for line in tidy_backtrace(backtrace).lines() {
        let _ = writeln!(out, "    {line}");
    }
    out.push('\n');
    out
}

/// Appends a block, creating the directory and rotating an oversized file.
fn append_report(path: &Path, report: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if std::fs::metadata(path).map(|meta| meta.len() > MAX_LOG_BYTES).unwrap_or(false) {
        let _ = std::fs::rename(path, path.with_extension("log.old"));
    }
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(report.as_bytes())?;
    file.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("pdfsak-crash-{tag}-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn payloads_of_every_common_shape_become_text() {
        assert_eq!(panic_message(&"static message"), "static message");
        assert_eq!(panic_message(&String::from("owned message")), "owned message");
        assert_eq!(panic_message(&42_u32), "<non-string panic payload>");
    }

    #[test]
    fn message_paths_are_masked_like_the_diagnostics_export() {
        let report = format_report(
            "4.2.0",
            1700000000,
            Some("tokio-runtime-worker"),
            r#"cannot open "C:\Users\Ayse\Documents\maas 2026.pdf" and /home/ali/Belgeler/rapor.docx"#,
            Some("src/commands.rs:42:5"),
            "",
        );
        assert!(report.contains(r#"message: cannot open "<path>.pdf" and <path>.docx"#), "{report}");
        for leaked in ["Ayse", "maas", "/home/ali", "Belgeler", "rapor"] {
            assert!(!report.contains(leaked), "{leaked} leaked into: {report}");
        }
    }

    #[test]
    fn report_has_the_expected_shape() {
        let report = format_report(
            "4.2.0",
            1700000000,
            Some("main"),
            "index out of bounds\nsecond line",
            Some("src/jobs.rs:10:3"),
            "   0: pdf_sak_lib::jobs::boom\n   1: std::rt::lang_start\n",
        );
        let lines: Vec<&str> = report.lines().collect();
        assert_eq!(lines[0], "[1700000000] panic in OmniOffice 4.2.0, thread 'main'");
        assert_eq!(lines[1], "  message: index out of bounds");
        assert_eq!(lines[2], "    second line");
        assert_eq!(lines[3], "  at src/jobs.rs:10:3");
        assert_eq!(lines[4], "  backtrace:");
        assert_eq!(lines[5], "       0: pdf_sak_lib::jobs::boom");
        assert!(report.ends_with("\n\n"), "blocks are separated by a blank line");
    }

    #[test]
    fn unnamed_thread_and_missing_location_are_handled() {
        let report = format_report("4.2.0", 1, None, "boom", None, "");
        assert!(report.contains("thread '<unnamed>'"));
        assert!(!report.contains("\n  at "));
    }

    #[test]
    fn source_paths_lose_the_build_machine_prefix() {
        assert_eq!(tidy_location("src/commands.rs:7:9"), "src/commands.rs:7:9");
        assert_eq!(
            tidy_location(
                "/home/runner/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/lopdf-0.45.0/src/object.rs:120:9"
            ),
            "lopdf-0.45.0/src/object.rs:120:9"
        );
        assert_eq!(
            tidy_location(
                r"C:\Users\ci\.cargo\registry\src\index.crates.io-6f17d22bba15001f\tauri-2.12.0\src\app.rs:5:1"
            ),
            "tauri-2.12.0/src/app.rs:5:1"
        );
        assert_eq!(
            tidy_location("/rustc/0123abcd/library/core/src/option.rs:1930:5"),
            "library/core/src/option.rs:1930:5"
        );
        assert_eq!(
            tidy_location("/home/dev/work/omnioffice/crates/pdfcore/src/merge.rs:88:14"),
            "crates/pdfcore/src/merge.rs:88:14"
        );
        // Without a line/column the whole text is the path.
        assert_eq!(tidy_location("/a/b/c/d/e/f.rs"), "c/d/e/f.rs");
    }

    #[test]
    fn backtrace_locations_are_tidied_and_frames_kept() {
        let raw = "   0: pdf_sak_lib::boom\n             at /home/dev/work/omnioffice/src-tauri/src/lib.rs:10:5\n   1: <unknown>\n";
        let tidy = tidy_backtrace(raw);
        assert!(tidy.contains("   0: pdf_sak_lib::boom"));
        assert!(tidy.contains("             at omnioffice/src-tauri/src/lib.rs:10:5"), "{tidy}");
        assert!(!tidy.contains("/home/dev"));
        assert!(tidy.contains("   1: <unknown>"));
    }

    #[test]
    fn append_creates_the_directory_and_keeps_earlier_blocks() {
        let dir = temp_dir("append");
        let path = dir.join("logs").join(CRASH_LOG);
        append_report(&path, "first\n\n").expect("first block");
        append_report(&path, "second\n\n").expect("second block");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "first\n\nsecond\n\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_oversized_log_is_rotated_not_grown() {
        let dir = temp_dir("rotate");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(CRASH_LOG);
        std::fs::write(&path, vec![b'x'; MAX_LOG_BYTES as usize + 1]).unwrap();
        append_report(&path, "fresh\n\n").expect("append after rotation");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "fresh\n\n");
        assert_eq!(std::fs::metadata(dir.join("crash.log.old")).unwrap().len(), MAX_LOG_BYTES + 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// End to end through the real hook: the only test that touches the
    /// process-wide hook and log directory (both are set once).
    #[test]
    fn an_installed_hook_records_a_panic_and_chains_on() {
        let dir = temp_dir("hook");
        set_log_dir(dir.clone());
        install_panic_hook();
        let outcome = std::panic::catch_unwind(|| {
            panic!("could not read /home/ali/Belgeler/rapor.docx");
        });
        assert!(outcome.is_err(), "the panic still propagates (the previous hook ran and unwinding went on)");
        let log = std::fs::read_to_string(dir.join(CRASH_LOG)).expect("crash.log written by the hook");
        assert!(log.contains("panic in OmniOffice "), "{log}");
        assert!(log.contains("message: could not read <path>.docx"), "{log}");
        assert!(log.contains("src/crash_log.rs:"), "{log}");
        assert!(log.contains("backtrace:"), "{log}");
        assert!(!log.contains("/home/ali"), "{log}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
