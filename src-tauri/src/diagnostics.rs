//! "Export diagnostics": a plain-text report the user can attach to a GitHub
//! issue. It is built locally and only written where the user chooses -
//! nothing is sent anywhere.
//!
//! Contents: app/core version and platform, engine availability, the last
//! jobs (kind, status, time, error), the tail of the frontend log and the tail
//! of the Rust panic log (`crash.log`, already masked when it is written). Job
//! titles and payloads are left out, and every path-like token is reduced to
//! `<path>` plus its extension, so document names and folders do not end up
//! in a public issue.

use crate::jobs::{JobRecord, JobStore};
use std::fmt::Write as _;
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};

const MAX_JOBS: usize = 20;
const MAX_LOG_LINES: usize = 150;
const MAX_CRASH_LINES: usize = 120;

/// True for tokens that look like a filesystem path (absolute, home-relative
/// or containing a separator). URLs are kept: they carry no local data.
fn is_path_like(token: &str) -> bool {
    if token.contains("://") {
        return false;
    }
    let bytes = token.as_bytes();
    let drive =
        bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && matches!(bytes[2], b'\\' | b'/');
    drive
        || token.starts_with('/')
        || token.starts_with("\\\\")
        || token.starts_with("~/")
        || token.contains('\\')
        || token.matches('/').count() >= 2
}

/// `<path>` plus the extension of `path` when it is a short alphanumeric one.
fn masked(path: &str) -> String {
    let mut out = String::from("<path>");
    if let Some((_, extension)) = path.rsplit_once('.') {
        if !extension.is_empty() && extension.len() <= 5 && extension.chars().all(|c| c.is_ascii_alphanumeric()) {
            out.push('.');
            out.push_str(extension);
        }
    }
    out
}

/// Replaces paths with `<path>` (keeping a short extension so the file type is
/// still visible). Double-quoted paths are masked whole, so names with spaces
/// do not leak their tail; unquoted path tokens are masked one by one. Quotes
/// and trailing punctuation are preserved.
pub fn redact_paths(text: &str) -> String {
    let mut quoted_pass = String::with_capacity(text.len());
    let mut parts = text.split('"');
    if let Some(first) = parts.next() {
        quoted_pass.push_str(first);
    }
    for (index, part) in parts.enumerate() {
        quoted_pass.push('"');
        // Odd segments (0-based index even here) are inside quotes.
        if index % 2 == 0 && is_path_like(part.split(' ').next().unwrap_or("")) {
            quoted_pass.push_str(&masked(part));
        } else {
            quoted_pass.push_str(part);
        }
    }
    redact_tokens(&quoted_pass)
}

fn redact_tokens(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for (index, raw) in text.split(' ').enumerate() {
        if index > 0 {
            out.push(' ');
        }
        let start = raw.find(|c: char| !matches!(c, '"' | '\'' | '(' | '[' | '{')).unwrap_or(raw.len());
        let end = raw
            .rfind(|c: char| !matches!(c, '"' | '\'' | ')' | ']' | '}' | ',' | ';' | ':' | '.'))
            .map(|i| i + 1)
            .unwrap_or(start);
        let (prefix, rest) = raw.split_at(start);
        let (core, suffix) = rest.split_at(end.saturating_sub(start).min(rest.len()));
        if !core.is_empty() && core != "<path>" && !core.starts_with("<path>.") && is_path_like(core) {
            out.push_str(prefix);
            out.push_str(&masked(core));
            out.push_str(suffix);
        } else {
            out.push_str(raw);
        }
    }
    out
}

fn status_name(record: &JobRecord) -> String {
    serde_json::to_value(record.status).ok().and_then(|value| value.as_str().map(str::to_string)).unwrap_or_default()
}

/// Builds the report from already collected parts (pure, unit-tested).
pub fn build_report(
    header: &[(&str, String)],
    engines: &serde_json::Value,
    jobs: &[JobRecord],
    log_tail: &str,
    crash_tail: &str,
) -> String {
    let mut report = String::from("OmniOffice diagnostics\n======================\n");
    for (key, value) in header {
        let _ = writeln!(report, "{key}: {value}");
    }
    let _ = writeln!(report, "\nEngines\n-------");
    for key in ["pdfium", "qpdf", "tesseract", "tesseract_version", "ocr_languages"] {
        if let Some(value) = engines.get(key) {
            let _ = writeln!(report, "{key}: {value}");
        }
    }
    let _ = writeln!(report, "\nRecent jobs (newest first, titles and inputs omitted)\n-----------");
    let mut recent: Vec<&JobRecord> = jobs.iter().collect();
    recent.sort_by_key(|job| std::cmp::Reverse(job.updated_at));
    if recent.is_empty() {
        let _ = writeln!(report, "(none)");
    }
    for job in recent.into_iter().take(MAX_JOBS) {
        let error = job.error.as_deref().map(redact_paths).unwrap_or_default();
        let _ = writeln!(report, "{} {} {} {}", job.updated_at, job.kind, status_name(job), error.trim());
    }
    let _ = writeln!(report, "\nFrontend log (last {MAX_LOG_LINES} lines)\n------------");
    let lines: Vec<&str> = log_tail.lines().collect();
    let skip = lines.len().saturating_sub(MAX_LOG_LINES);
    for line in &lines[skip..] {
        let _ = writeln!(report, "{}", redact_paths(line));
    }
    let _ = writeln!(report, "\nCrash log (last {MAX_CRASH_LINES} lines)\n---------");
    let crash_lines: Vec<&str> = crash_tail.lines().collect();
    if crash_lines.is_empty() {
        let _ = writeln!(report, "(none)");
    }
    // Written masked by crash_log.rs; running the path redaction over it again
    // would also mangle the tidied source locations.
    for line in &crash_lines[crash_lines.len().saturating_sub(MAX_CRASH_LINES)..] {
        let _ = writeln!(report, "{line}");
    }
    report
}

#[tauri::command]
pub fn diagnostics_report(app: AppHandle, store: State<'_, Arc<JobStore>>) -> String {
    let now = time::OffsetDateTime::now_utc();
    let header = [
        ("Version", env!("CARGO_PKG_VERSION").to_string()),
        ("Core", pdfcore::VERSION.to_string()),
        ("Platform", format!("{} ({}, {})", std::env::consts::OS, std::env::consts::ARCH, std::env::consts::FAMILY)),
        ("Created (UTC)", format!("{now}")),
    ];
    let engines = serde_json::to_value(pdfcore::engines::engine_status()).unwrap_or_default();
    let log = app
        .path()
        .app_log_dir()
        .ok()
        .and_then(|dir| std::fs::read_to_string(dir.join("frontend.log")).ok())
        .unwrap_or_default();
    let crash = app
        .path()
        .app_log_dir()
        .ok()
        .and_then(|dir| std::fs::read_to_string(dir.join(crate::crash_log::CRASH_LOG)).ok())
        .unwrap_or_default();
    build_report(&header, &engines, &store.records(), &log, &crash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::JobStatus;
    use serde_json::json;

    #[test]
    fn paths_are_reduced_to_their_extension() {
        assert_eq!(
            redact_paths(r#"cannot open "C:\Users\Ayse\Documents\maas 2026.pdf" now"#),
            r#"cannot open "<path>.pdf" now"#
        );
        assert_eq!(redact_paths("saved /home/ali/Belgeler/rapor.docx."), "saved <path>.docx.");
        assert_eq!(redact_paths("see https://github.com/x/y/issues"), "see https://github.com/x/y/issues");
        assert_eq!(redact_paths("engines ok, 3/4 done"), "engines ok, 3/4 done");
        assert_eq!(redact_paths(r"\\server\share\a.xlsx"), "<path>.xlsx");
    }

    #[test]
    fn report_omits_titles_and_payloads() {
        let job = JobRecord {
            id: "j1".into(),
            kind: "merge".into(),
            title: "Merge Salary.pdf".into(),
            status: JobStatus::Failed,
            progress: 0.0,
            detail: None,
            payload: Some(json!({ "inputs": ["/home/u/Salary.pdf"] })),
            error: Some("cannot read /home/u/Salary.pdf".into()),
            created_at: 1,
            updated_at: 2,
        };
        let report = build_report(
            &[("Version", "3.9.0".into())],
            &json!({ "pdfium": true, "pdfium_path": "/home/u/engines/pdfium.dll" }),
            &[job],
            "[1] [info] opened /home/u/Salary.pdf\n",
            "",
        );
        assert!(report.contains("Version: 3.9.0"));
        assert!(report.contains("2 merge failed cannot read <path>.pdf"));
        assert!(report.contains("pdfium: true"));
        assert!(!report.contains("Salary"));
        assert!(!report.contains("/home/u"));
    }

    #[test]
    fn report_includes_the_tail_of_the_crash_log() {
        let crash: String = (1..=MAX_CRASH_LINES + 30).map(|n| format!("crash line {n}\n")).collect();
        let report = build_report(&[], &json!({}), &[], "", &crash);
        assert!(report.contains("Crash log (last 120 lines)"));
        // Only the newest lines survive; the oldest are dropped.
        assert!(report.contains(&format!("crash line {}", MAX_CRASH_LINES + 30)));
        assert!(report.contains("crash line 31\n"));
        assert!(!report.contains("crash line 30\n"));
        // The crash log is masked when it is written, so its tidied source
        // locations pass through untouched.
        let located = build_report(&[], &json!({}), &[], "", "  at crates/pdfcore/src/merge.rs:88:14\n");
        assert!(located.contains("  at crates/pdfcore/src/merge.rs:88:14"));
    }

    #[test]
    fn report_says_so_when_there_is_no_crash_log() {
        let report = build_report(&[], &json!({}), &[], "", "");
        let section = report.split("Crash log").nth(1).expect("crash section");
        assert!(section.contains("(none)"));
    }
}
