//! qpdf-backed document repair and linearization.
//!
//! qpdf is the bundled desktop engine (`engines::qpdf_path`); it is not shipped
//! on Android, where this module reports the engine as missing. Both operations
//! rewrite the file through qpdf and then re-open the result so the report
//! describes the file that was actually produced, not the intent.

use crate::engines;
use crate::error::{PdfError, PdfResult};
use crate::progress::{CancelToken, ProgressCallback, ProgressEvent, ProgressReporter};
use serde::Serialize;
use std::path::Path;
use std::process::Stdio;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairReport {
    /// Where the rewritten file was written.
    pub output: String,
    /// Pages in the produced file (0 when it could not be re-opened).
    pub pages: u32,
    /// qpdf diagnostics, capped; these are the repair's warnings.
    pub warnings: Vec<String>,
}

/// The qpdf exit code that means "warnings only" (errors are 2).
const QPDF_WARNING_EXIT: i32 = 3;

fn qpdf_command() -> PdfResult<std::process::Command> {
    let exe = engines::qpdf_path()
        .ok_or_else(|| PdfError::EngineMissing("qpdf is only bundled with the desktop build".into()))?;
    let mut cmd = std::process::Command::new(exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    Ok(cmd)
}

/// Runs qpdf, tolerating its "warnings" exit code, and returns the diagnostics.
///
/// Cancellation kills the child and removes the partially written output, so a
/// cancelled repair never leaves a half-written file next to the input.
fn run_qpdf(args: &[String], output: Option<&Path>, cancel: &CancelToken) -> PdfResult<Vec<String>> {
    let mut cmd = qpdf_command()?;
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    let mut child =
        cmd.spawn().map_err(|error| PdfError::ProcessingFailed(format!("could not start qpdf: {error}")))?;
    let mut stderr = child.stderr.take();
    let collected = std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(pipe) = stderr.as_mut() {
            let _ = std::io::Read::read_to_string(pipe, &mut text);
        }
        text
    });
    let status = loop {
        if cancel.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            if let Some(path) = output {
                let _ = std::fs::remove_file(path);
            }
            return Err(PdfError::Cancelled);
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(40)),
            Err(error) => return Err(PdfError::from_io(error)),
        }
    };
    let text = collected.join().unwrap_or_default();
    let diagnostics: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.chars().take(200).collect())
        .collect();
    let code = status.code().unwrap_or(-1);
    // Exit 0 is a clean rewrite; exit 3 means qpdf recovered the file but wants
    // the warnings seen. Anything else is a failure, and the diagnostics say
    // why.
    if code == 0 || code == QPDF_WARNING_EXIT {
        return Ok(diagnostics);
    }
    if let Some(path) = output {
        let _ = std::fs::remove_file(path);
    }
    let detail = diagnostics.last().cloned().unwrap_or_default();
    Err(PdfError::ProcessingFailed(format!(
        "qpdf exited with code {code}{}",
        if detail.is_empty() { String::new() } else { format!(": {detail}") }
    )))
}

fn reopen_pages(path: &Path) -> Option<u32> {
    let doc = lopdf::Document::load(path).ok()?;
    let pages = doc.get_pages().len();
    u32::try_from(pages).ok()
}

/// Rewrites a damaged PDF through qpdf, recovering what the simple lopdf
/// parser cannot (broken xref chains, dangling objects, bad stream lengths).
pub fn repair_pdf(
    input: &Path,
    output: &Path,
    progress: &ProgressCallback,
    cancel: &CancelToken,
) -> PdfResult<RepairReport> {
    cancel.check()?;
    let reporter = ProgressReporter::new(progress);
    reporter.emit(ProgressEvent::new("repair.read", 0, 2));
    let args = vec![
        "--warning-exit-0".to_string(),
        "--".to_string(),
        input.to_string_lossy().to_string(),
        output.to_string_lossy().to_string(),
    ];
    let warnings = run_qpdf(&args, Some(output), cancel)?;
    cancel.check()?;
    reporter.emit(ProgressEvent::new("repair.verify", 1, 2));
    if !output.is_file() {
        return Err(PdfError::ProcessingFailed("qpdf reported success but produced no output file".into()));
    }
    let pages = reopen_pages(output).unwrap_or(0);
    let mut warnings = warnings;
    if pages == 0 {
        warnings.push(
            "The rewritten file could not be re-opened by the built-in parser; it may still open elsewhere.".into(),
        );
    }
    reporter.emit(ProgressEvent::new("repair.done", 2, 2));
    Ok(RepairReport { output: output.display().to_string(), pages, warnings })
}

/// Rewrites a PDF with qpdf's linearization (Fast Web View layout).
pub fn linearize_pdf(
    input: &Path,
    output: &Path,
    progress: &ProgressCallback,
    cancel: &CancelToken,
) -> PdfResult<RepairReport> {
    cancel.check()?;
    let reporter = ProgressReporter::new(progress);
    reporter.emit(ProgressEvent::new("linearize.read", 0, 2));
    let args = vec![
        "--warning-exit-0".to_string(),
        "--linearize".to_string(),
        "--".to_string(),
        input.to_string_lossy().to_string(),
        output.to_string_lossy().to_string(),
    ];
    let warnings = run_qpdf(&args, Some(output), cancel)?;
    cancel.check()?;
    reporter.emit(ProgressEvent::new("linearize.verify", 1, 2));
    if !output.is_file() {
        return Err(PdfError::ProcessingFailed("qpdf reported success but produced no output file".into()));
    }
    let pages = reopen_pages(output).unwrap_or(0);
    reporter.emit(ProgressEvent::new("linearize.done", 2, 2));
    Ok(RepairReport { output: output.display().to_string(), pages, warnings })
}
