//! Document repair and qpdf-backed linearization.
//!
//! Repair uses qpdf when it is available (bundled on Windows, a system install
//! on Linux/macOS, see `engines::qpdf_path`) and otherwise - on Android, or
//! when qpdf fails - the built-in rebuild in `rebuild`. Linearization needs
//! qpdf. Every operation re-opens the result so the report describes the file
//! that was actually produced, not the intent.

use crate::error::{PdfError, PdfResult};
use crate::progress::{CancelToken, ProgressCallback, ProgressEvent, ProgressReporter};
use crate::{docutil, engines, rebuild};
use serde::Serialize;
use std::path::Path;
use std::process::Stdio;

/// The engine that rewrote a repaired file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RepairMethod {
    Qpdf,
    /// The built-in object-scanning rebuild (`rebuild::rebuild_pdf`).
    Builtin,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairReport {
    /// Where the rewritten file was written.
    pub output: String,
    /// Pages in the produced file (0 when it could not be re-opened).
    pub pages: u32,
    /// qpdf diagnostics or the rebuild's notes, capped; these are the
    /// repair's warnings.
    pub warnings: Vec<String>,
    /// Which engine repaired the file; absent for linearization.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<RepairMethod>,
}

/// The qpdf exit code that means "warnings only" (errors are 2).
const QPDF_WARNING_EXIT: i32 = 3;

fn qpdf_command() -> PdfResult<std::process::Command> {
    let exe =
        engines::qpdf_path().ok_or_else(|| PdfError::EngineMissing("qpdf is not available on this device".into()))?;
    #[cfg_attr(not(windows), allow(unused_mut))]
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

/// Rewrites a damaged PDF, recovering what the simple lopdf parser cannot
/// (broken xref chains, dangling objects, bad stream lengths). qpdf is used
/// when it is available; without it, or when it fails, the built-in rebuild
/// takes over.
pub fn repair_pdf(
    input: &Path,
    output: &Path,
    progress: &ProgressCallback,
    cancel: &CancelToken,
) -> PdfResult<RepairReport> {
    cancel.check()?;
    if engines::qpdf_path().is_none() {
        return repair_pdf_builtin(input, output, progress, cancel);
    }
    match repair_with_qpdf(input, output, progress, cancel) {
        Ok(report) => Ok(report),
        Err(PdfError::Cancelled) => Err(PdfError::Cancelled),
        Err(error) => {
            let mut report = repair_pdf_builtin(input, output, progress, cancel)?;
            report
                .warnings
                .insert(0, format!("qpdf could not repair the file ({error}); the built-in engine was used."));
            Ok(report)
        }
    }
}

fn repair_with_qpdf(
    input: &Path,
    output: &Path,
    progress: &ProgressCallback,
    cancel: &CancelToken,
) -> PdfResult<RepairReport> {
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
    Ok(RepairReport { output: output.display().to_string(), pages, warnings, method: Some(RepairMethod::Qpdf) })
}

/// Rebuilds a damaged PDF with the built-in engine (no qpdf): every object is
/// recovered by scanning the file and the result gets a fresh
/// cross-reference table. See `rebuild` for what it tolerates.
pub fn repair_pdf_builtin(
    input: &Path,
    output: &Path,
    progress: &ProgressCallback,
    cancel: &CancelToken,
) -> PdfResult<RepairReport> {
    cancel.check()?;
    let reporter = ProgressReporter::new(progress);
    reporter.emit(ProgressEvent::new("repair.read", 0, 3));
    rebuild::check_input_size(std::fs::metadata(input).map_err(PdfError::from_io)?.len())?;
    let data = std::fs::read(input).map_err(PdfError::from_io)?;
    cancel.check()?;
    reporter.emit(ProgressEvent::new("repair.rebuild", 1, 3));
    let rebuilt = rebuild::rebuild_pdf(&data, cancel)?;
    drop(data);
    cancel.check()?;
    docutil::write_bytes_atomic(output, &rebuilt.bytes)?;
    reporter.emit(ProgressEvent::new("repair.verify", 2, 3));
    let mut warnings = rebuilt.warnings;
    let pages = match reopen_pages(output).filter(|&pages| pages > 0) {
        Some(pages) => pages,
        // An encrypted result only re-opens here when it needs no password;
        // the rebuilt page tree is known either way.
        None if rebuilt.encrypted => rebuilt.pages,
        None => {
            warnings.push(
                "The rewritten file could not be re-opened by the built-in parser; it may still open elsewhere.".into(),
            );
            0
        }
    };
    reporter.emit(ProgressEvent::new("repair.done", 3, 3));
    Ok(RepairReport { output: output.display().to_string(), pages, warnings, method: Some(RepairMethod::Builtin) })
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
    Ok(RepairReport { output: output.display().to_string(), pages, warnings, method: None })
}
