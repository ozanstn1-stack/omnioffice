//! Office document commands: open/save/convert/export/clean plus the local
//! stores behind autosave, recovery, version history and the productivity
//! modules (Notes, Planner, Data, Draw).
//!
//! Everything here is local-first: no network access, no telemetry, and no
//! document content leaves the machine.

use officecore::cleaner::{self, CleanOptions, CleanResult};
use officecore::csvio::{self, CsvOptions};
use officecore::error::{OfficeError, OfficeResult};
use officecore::model::*;
use officecore::{docx, layout, odf, pptx, rtf, textio, ubl, udf, xlsx};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize)]
pub struct OfficeErrorPayload {
    pub code: String,
    pub message: String,
}

impl std::fmt::Display for OfficeErrorPayload {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl From<OfficeError> for OfficeErrorPayload {
    fn from(error: OfficeError) -> Self {
        Self { code: error.code, message: error.message }
    }
}

fn payload(error: OfficeError) -> OfficeErrorPayload {
    error.into()
}

/// Validates an input file coming from the frontend through the shared path
/// policy, surfacing a normal office error on rejection.
fn input_path(raw: &str) -> Result<PathBuf, OfficeErrorPayload> {
    crate::paths::input_file(raw)
        .map(|path| path.into_path_buf())
        .map_err(|error| payload(OfficeError::invalid(error.to_string())))
}

/// Validates an output file coming from the frontend through the shared path
/// policy.
fn output_path(raw: &str) -> Result<PathBuf, OfficeErrorPayload> {
    crate::paths::output_file(raw)
        .map(|path| path.into_path_buf())
        .map_err(|error| payload(OfficeError::invalid(error.to_string())))
}

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenDocument {
    pub kind: String,
    pub title: String,
    pub path: String,
    pub model: Value,
    pub warnings: Vec<String>,
    /// True when the file was a legacy binary format (`.doc`/`.xls`/`.ppt`). The
    /// frontend must not save back to it: the tab keeps an empty path so the
    /// first save asks for a modern destination.
    #[serde(default)]
    pub legacy: bool,
    /// Write token for an in-place save (issued in the app process, not the
    /// worker: the token registry is per process).
    #[serde(default)]
    pub token: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveDocument {
    pub path: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ConvertOptions {
    pub delimiter: Option<String>,
    pub has_header: Option<bool>,
    pub encoding: Option<String>,
    pub pdf_mode: Option<String>,
    pub image_format: Option<String>,
    pub dpi: Option<f64>,
    pub jpeg_quality: Option<u8>,
    pub grayscale: Option<bool>,
    pub page_size: Option<String>,
    pub orientation: Option<String>,
    pub margin_pt: Option<f64>,
    /// Password for an encrypted PDF input. `office_convert` keeps its
    /// `{ input, output, options }` request shape; the frontend carries this
    /// inside the options bag. It is only ever passed to pdfcore and never
    /// persisted or logged.
    #[serde(default)]
    pub password: Option<String>,
}

fn extension(path: &Path) -> String {
    path.extension().map(|value| value.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()
}

fn kind_for_extension(extension: &str) -> Option<&'static str> {
    match extension {
        "docx" | "docm" | "dotx" | "odt" | "udf" | "rtf" | "txt" | "md" | "markdown" | "html" | "htm"
        | "oswk-writer" => Some("writer"),
        "xlsx" | "xlsm" | "xls" | "ods" | "csv" | "tsv" | "oswk-calc" => Some("calc"),
        "pptx" | "pptm" | "odp" | "oswk-impress" => Some("impress"),
        _ => None,
    }
}

fn is_native(path: &Path) -> bool {
    extension(path) == "oswk"
}

fn to_value<T: Serialize>(model: T) -> Result<Value, OfficeErrorPayload> {
    serde_json::to_value(model)
        .map_err(|error| payload(OfficeError::internal(format!("Could not serialize the document model: {error}"))))
}

fn writer_from_value(model: Value) -> Result<TextDocument, OfficeErrorPayload> {
    serde_json::from_value(model)
        .map_err(|error| payload(OfficeError::invalid(format!("The document model is not valid: {error}"))))
}

fn workbook_from_value(model: Value) -> Result<Workbook, OfficeErrorPayload> {
    serde_json::from_value(model)
        .map_err(|error| payload(OfficeError::invalid(format!("The spreadsheet model is not valid: {error}"))))
}

fn deck_from_value(model: Value) -> Result<Deck, OfficeErrorPayload> {
    serde_json::from_value(model)
        .map_err(|error| payload(OfficeError::invalid(format!("The presentation model is not valid: {error}"))))
}

// ---------------------------------------------------------------------------
// Open
// ---------------------------------------------------------------------------

pub fn open_path(path: &Path) -> Result<OpenDocument, OfficeErrorPayload> {
    let extension = extension(path);
    let path_text = path.to_string_lossy().to_string();
    let mut legacy = false;
    let (kind, title, model, warnings): (String, String, Value, Vec<String>) = match extension.as_str() {
        "docx" | "docm" | "dotx" => {
            let read = docx::read_docx_file(path).map_err(payload)?;
            ("writer".to_string(), read.document.title.clone(), to_value(read.document)?, read.warnings)
        }
        // Word 97-2003: prefer a locally installed LibreOffice for fidelity,
        // fall back to the built-in CFB text importer. Never edited in place.
        "doc" | "dot" => {
            legacy = true;
            if let Some(converted) = try_libreoffice(path, "docx") {
                let read = docx::read_docx_file(&converted.path).map_err(payload)?;
                let mut warnings = vec![
                    "Converted with the installed LibreOffice. The original .doc file is unchanged; save as .docx or .oswk to keep your edits."
                        .to_string(),
                ];
                warnings.extend(read.warnings);
                let title = if read.document.title.trim().is_empty() {
                    officecore::io::file_stem(path)
                } else {
                    read.document.title.clone()
                };
                ("writer".to_string(), title, to_value(read.document)?, warnings)
            } else {
                let read = officecore::legacy::read_doc_file(path).map_err(payload)?;
                ("writer".to_string(), officecore::io::file_stem(path), to_value(read.document)?, read.warnings)
            }
        }
        // PowerPoint 97-2003: same strategy as Word.
        "ppt" => {
            legacy = true;
            if let Some(converted) = try_libreoffice(path, "pptx") {
                let read = pptx::read_pptx_file(&converted.path).map_err(payload)?;
                let mut warnings = vec![
                    "Converted with the installed LibreOffice. The original .ppt file is unchanged; save as .pptx or .oswk to keep your edits."
                        .to_string(),
                ];
                warnings.extend(read.warnings);
                let title = if read.deck.title.trim().is_empty() {
                    officecore::io::file_stem(path)
                } else {
                    read.deck.title.clone()
                };
                ("impress".to_string(), title, to_value(read.deck)?, warnings)
            } else {
                let read = officecore::legacy::read_ppt_file(path).map_err(payload)?;
                ("impress".to_string(), officecore::io::file_stem(path), to_value(read.document)?, read.warnings)
            }
        }
        "odt" => {
            let read = odf::read_odt_file(path).map_err(payload)?;
            ("writer".to_string(), read.document.title.clone(), to_value(read.document)?, read.warnings)
        }
        // UYAP `.udf`: a ZIP whose `content.xml` carries the text. Closed
        // format; see `officecore::udf` for what is modelled.
        "udf" => {
            let read = udf::read_udf_file(path).map_err(payload)?;
            ("writer".to_string(), read.document.title.clone(), to_value(read.document)?, read.warnings)
        }
        "rtf" => {
            let read = rtf::read_rtf_file(path).map_err(payload)?;
            ("writer".to_string(), read.document.title.clone(), to_value(read.document)?, read.warnings)
        }
        "txt" | "md" | "markdown" | "html" | "htm" => {
            let bytes = officecore::io::read_bytes(path).map_err(payload)?;
            let text = officecore::zip::decode_utf8(&bytes, "text").map_err(payload)?;
            let title = officecore::io::file_stem(path);
            let document = textio::text_to_document(&text, &title);
            let mut warnings = Vec::new();
            if extension == "html" || extension == "htm" {
                warnings.push("HTML files are imported as plain text; formatting is not preserved.".into());
            }
            ("writer".to_string(), title, to_value(document)?, warnings)
        }
        "xlsx" | "xlsm" | "ods" => {
            let read = xlsx::read_workbook_file(path).map_err(payload)?;
            ("calc".to_string(), read.workbook.title.clone(), to_value(read.workbook)?, read.warnings)
        }
        // Excel 97-2003: prefer a locally installed LibreOffice for fidelity,
        // fall back to the calamine values-only import. Never edited in place.
        "xls" => {
            legacy = true;
            if let Some(converted) = try_libreoffice(path, "xlsx") {
                let read = xlsx::read_workbook_file(&converted.path).map_err(payload)?;
                let mut warnings = vec![
                    "Converted with the installed LibreOffice. The original .xls file is unchanged; save as .xlsx or .oswk to keep your edits."
                        .to_string(),
                ];
                warnings.extend(read.warnings);
                let title = if read.workbook.title.trim().is_empty() {
                    officecore::io::file_stem(path)
                } else {
                    read.workbook.title.clone()
                };
                ("calc".to_string(), title, to_value(read.workbook)?, warnings)
            } else {
                let read = xlsx::read_workbook_file(path).map_err(payload)?;
                let mut warnings = read.warnings;
                warnings.push(
                    "Legacy Excel (.xls) files import the cached values only; the original is unchanged - save as .xlsx or .oswk to keep your edits."
                        .into(),
                );
                ("calc".to_string(), officecore::io::file_stem(path), to_value(read.workbook)?, warnings)
            }
        }
        "csv" | "tsv" => {
            let bytes = officecore::io::read_bytes(path).map_err(payload)?;
            let mut options = CsvOptions::default();
            if extension == "tsv" {
                options.delimiter = "tab".into();
            }
            let read = csvio::parse_csv(&bytes, &options).map_err(payload)?;
            let mut workbook = read.workbook;
            workbook.title = officecore::io::file_stem(path);
            ("calc".to_string(), workbook.title.clone(), to_value(workbook)?, read.warnings)
        }
        "pptx" | "pptm" => {
            let read = pptx::read_pptx_file(path).map_err(payload)?;
            ("impress".to_string(), read.deck.title.clone(), to_value(read.deck)?, read.warnings)
        }
        "odp" => {
            let read = odf::read_odp_file(path).map_err(payload)?;
            ("impress".to_string(), read.deck.title.clone(), to_value(read.deck)?, read.warnings)
        }
        "oswk" => {
            let bytes = officecore::io::read_bytes(path).map_err(payload)?;
            let mut raw: Value = serde_json::from_slice(&bytes)
                .map_err(|error| payload(OfficeError::corrupt(format!("The unit file is not valid: {error}"))))?;
            // Corrupt-file detection: verify the content checksum before the
            // migration touches anything. A mismatch means damage or an
            // out-of-app edit, not a file we should silently reinterpret.
            officecore::unit::verify_checksum(&raw).map_err(payload)?;
            // Schema migration: older files gain the V3 fields with defaults,
            // newer files are refused instead of being misread.
            let migration = officecore::schema::migrate_unit(&mut raw).map_err(payload)?;
            let mut unit: NativeUnit = serde_json::from_value(raw)
                .map_err(|error| payload(OfficeError::corrupt(format!("The unit file is not valid: {error}"))))?;
            let title = unit.title.clone();
            let kind = unit.kind.clone();
            let mut warnings = unit.warnings;
            if migration.migrated {
                warnings.push(format!(
                    "This document was migrated from schema {} to {}{}",
                    migration.from_version,
                    migration.to_version,
                    if migration.notes.is_empty() {
                        ".".to_string()
                    } else {
                        format!(" ({} change(s)).", migration.notes.len())
                    }
                ));
                warnings.extend(migration.notes);
            }
            // Unknown envelope fields survive: hand the extensions map back so a
            // later save can merge it in. Stored on the model value under a
            // reserved key the frontend ignores and returns unchanged.
            if !unit.extensions.is_empty() {
                if let Value::Object(ref mut model) = unit.model {
                    model.insert("__oswkExtensions".into(), Value::Object(unit.extensions));
                }
            }
            (kind, title, unit.model, warnings)
        }
        "pdf" => {
            return Err(payload(OfficeError::unsupported(
                "PDF files open in the PDF module, not in Writer, Calc or Impress.",
            )));
        }
        other => {
            return Err(payload(OfficeError::unsupported(format!("Opening .{other} files is not supported yet."))));
        }
    };
    Ok(OpenDocument { kind, title, path: path_text, model, warnings, legacy, token: String::new() })
}

// ---------------------------------------------------------------------------
// Legacy binary formats and the optional LibreOffice bridge
// ---------------------------------------------------------------------------

/// A LibreOffice conversion result that removes its temporary directory when
/// dropped.
struct LibreOfficeConversion {
    path: PathBuf,
    dir: PathBuf,
}

impl Drop for LibreOfficeConversion {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Finds the LibreOffice CLI: the `PDFSAK_SOFFICE` override, `soffice` on
/// PATH, or the standard install locations.
fn soffice_path() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os("PDFSAK_SOFFICE") {
        let candidate = PathBuf::from(explicit);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            for name in ["soffice.exe", "soffice"] {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    #[cfg(windows)]
    {
        for base in [
            r"C:\Program Files\LibreOffice\program\soffice.exe",
            r"C:\Program Files (x86)\LibreOffice\program\soffice.exe",
        ] {
            let candidate = PathBuf::from(base);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Converts a legacy binary document with LibreOffice, when one is installed.
/// Hardened invocation: fixed arguments, no shell, no stdin, a 120 s timeout
/// and a private temporary directory that is removed afterwards.
fn try_libreoffice(path: &Path, target: &str) -> Option<LibreOfficeConversion> {
    let exe = soffice_path()?;
    let dir = std::env::temp_dir().join(format!("osak-legacy-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).ok()?;
    let mut command = std::process::Command::new(exe);
    command
        .arg("--headless")
        .arg("--norestore")
        .arg("--convert-to")
        .arg(target)
        .arg("--outdir")
        .arg(&dir)
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = command.spawn().ok()?;
    let started = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if started.elapsed() > std::time::Duration::from_secs(120) {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = std::fs::remove_dir_all(&dir);
                    return None;
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            Err(_) => {
                let _ = std::fs::remove_dir_all(&dir);
                return None;
            }
        }
    };
    let produced = dir.join(format!("{}.{}", officecore::io::file_stem(path), target));
    if !status.success() || !produced.is_file() {
        let _ = std::fs::remove_dir_all(&dir);
        return None;
    }
    Some(LibreOfficeConversion { path: produced, dir })
}

#[tauri::command]
pub async fn office_open_document(path: String) -> Result<OpenDocument, OfficeErrorPayload> {
    let _permit = crate::concurrency::acquire().await;
    let task = tauri::async_runtime::spawn_blocking(move || -> Result<OpenDocument, OfficeErrorPayload> {
        let path = input_path(&path)?;
        // Untrusted parse: it runs in a worker process, so a malformed file
        // can kill the parser, never the app (see `crate::worker`).
        let json =
            crate::worker::run_json_or_local(&crate::worker::Job::OfficeOpen { path: path.as_path().to_path_buf() })
                .map_err(|error| payload(OfficeError::invalid(error)))?;
        let document: OpenDocument = serde_json::from_str(&json)
            .map_err(|error| payload(OfficeError::internal(format!("the parser result is not valid: {error}"))))?;
        Ok(document)
    });
    let mut document =
        task.await.map_err(|error| payload(OfficeError::internal(format!("worker thread failed: {error}"))))??;
    if !document.legacy {
        // The registry lives in this process; the worker's JSON cannot carry a
        // usable token, so it is issued here for an in-place save.
        document.token =
            crate::files::issue(std::path::Path::new(&document.path), crate::files::FileAccess::ReadWrite).token;
    }
    Ok(document)
}

// ---------------------------------------------------------------------------
// Save
// ---------------------------------------------------------------------------

/// The pdfcore watermark options that mirror a Writer watermark model.
fn watermark_options(watermark: &Watermark) -> pdfcore::watermark::WatermarkOptions {
    pdfcore::watermark::WatermarkOptions {
        kind: "text".into(),
        text: watermark.text.clone(),
        font_size_pt: watermark.font_pt,
        bold: watermark.bold,
        color: watermark.color.clone().unwrap_or_else(|| "#94a3b8".into()),
        opacity: watermark.opacity,
        rotation_deg: watermark.rotation,
        position: "center".into(),
        margin_pt: 24.0,
        tile: false,
        image_path: None,
        image_scale: 0.35,
        pages: Vec::new(),
    }
}

/// Applies a Writer watermark to freshly exported PDF bytes as an incremental
/// revision (so a signed PDF keeps its signatures). Returns the original bytes
/// and a warning when the watermark cannot be drawn.
fn watermark_pdf(bytes: Vec<u8>, watermark: Option<&Watermark>) -> (Vec<u8>, Option<String>) {
    let Some(watermark) = watermark.filter(|watermark| !watermark.text.trim().is_empty()) else {
        return (bytes, None);
    };
    match pdfcore::watermark::watermark_pdf_incremental(&bytes, &watermark_options(watermark)) {
        Ok(marked) => (marked, None),
        Err(error) => (bytes, Some(format!("The watermark could not be added to the PDF: {error}"))),
    }
}

pub fn save_model(kind: &str, model: Value, path: &Path) -> Result<SaveDocument, OfficeErrorPayload> {
    let extension = extension(path);
    let mut warnings = Vec::new();
    match kind {
        "writer" => {
            let document = writer_from_value(model)?;
            match extension.as_str() {
                "docx" => {
                    let bytes = docx::write_docx(&document).map_err(payload)?;
                    officecore::io::write_atomic(path, &bytes).map_err(payload)?;
                }
                "odt" => {
                    let bytes = odf::write_odt(&document).map_err(payload)?;
                    officecore::io::write_atomic(path, &bytes).map_err(payload)?;
                }
                "udf" => {
                    let bytes = udf::write_udf(&document).map_err(payload)?;
                    officecore::io::write_atomic(path, &bytes).map_err(payload)?;
                }
                "rtf" => {
                    rtf::write_rtf_file(path, &document).map_err(payload)?;
                }
                "txt" => {
                    officecore::io::write_atomic(path, textio::document_to_text(&document).as_bytes())
                        .map_err(payload)?;
                }
                "md" | "markdown" => {
                    officecore::io::write_atomic(path, textio::document_to_markdown(&document).as_bytes())
                        .map_err(payload)?;
                }
                "html" | "htm" => {
                    officecore::io::write_atomic(path, textio::document_to_html(&document).as_bytes())
                        .map_err(payload)?;
                }
                "pdf" => {
                    let (bytes, watermark_warning) =
                        watermark_pdf(layout::document_to_pdf(&document), document.watermark.as_ref());
                    officecore::io::write_atomic(path, &bytes).map_err(payload)?;
                    if let Some(warning) = watermark_warning {
                        warnings.push(warning);
                    }
                }
                other => {
                    return Err(payload(OfficeError::unsupported(format!(
                        "Saving Writer documents as .{other} is not supported."
                    ))));
                }
            }
            if document.comments.iter().any(|comment| !comment.resolved) {
                warnings.push("RTF/TXT exports cannot carry comments; DOCX and ODT keep them.".into());
            }
        }
        "calc" => {
            let workbook = workbook_from_value(model)?;
            match extension.as_str() {
                "xlsx" => {
                    let result = xlsx::write_xlsx_package(&workbook).map_err(payload)?;
                    warnings.extend(result.warnings);
                    officecore::io::write_atomic(path, &result.bytes).map_err(payload)?;
                }
                "ods" => {
                    let bytes = odf::write_ods(&workbook).map_err(payload)?;
                    officecore::io::write_atomic(path, &bytes).map_err(payload)?;
                }
                "csv" | "tsv" => {
                    let mut options = CsvOptions::default();
                    if extension == "tsv" {
                        options.delimiter = "tab".into();
                    }
                    let bytes = csvio::write_csv(&workbook, workbook.active_sheet, &options).map_err(payload)?;
                    officecore::io::write_atomic(path, &bytes).map_err(payload)?;
                }
                "pdf" => {
                    let bytes = layout::workbook_to_pdf(&workbook, 20);
                    officecore::io::write_atomic(path, &bytes).map_err(payload)?;
                    warnings.push(
                        "Charts are rendered in the app; the PDF export draws their data ranges as labelled boxes."
                            .into(),
                    );
                }
                other => {
                    return Err(payload(OfficeError::unsupported(format!(
                        "Saving spreadsheets as .{other} is not supported."
                    ))));
                }
            }
        }
        "impress" => {
            let deck = deck_from_value(model)?;
            match extension.as_str() {
                "pptx" => {
                    let result = pptx::write_pptx_package(&deck).map_err(payload)?;
                    warnings.extend(result.warnings);
                    officecore::io::write_atomic(path, &result.bytes).map_err(payload)?;
                }
                "odp" => {
                    let result = odf::write_odp_package(&deck).map_err(payload)?;
                    warnings.extend(result.warnings);
                    officecore::io::write_atomic(path, &result.bytes).map_err(payload)?;
                }
                "pdf" => {
                    let bytes = layout::deck_to_pdf(&deck);
                    officecore::io::write_atomic(path, &bytes).map_err(payload)?;
                }
                other => {
                    return Err(payload(OfficeError::unsupported(format!(
                        "Saving presentations as .{other} is not supported."
                    ))));
                }
            }
        }
        other => {
            return Err(payload(OfficeError::invalid(format!("Unknown document type {other}."))));
        }
    }
    Ok(SaveDocument { path: path.to_string_lossy().to_string(), warnings })
}

#[tauri::command]
pub async fn office_save_document(
    kind: String,
    model: Value,
    path: String,
) -> Result<SaveDocument, OfficeErrorPayload> {
    let _permit = crate::concurrency::acquire().await;
    let task = tauri::async_runtime::spawn_blocking(move || {
        let path = output_path(&path)?;
        save_model(&kind, model, &path)
    });
    task.await.map_err(|error| payload(OfficeError::internal(format!("worker thread failed: {error}"))))?
}

/// The canonical `.oswk` envelope.
///
/// `documentType` is the model family (`writer`/`calc`/`impress`), `kind` is
/// kept as an alias for older readers, `applicationVersion` records the build
/// that wrote the file, `checksum` is a SHA-256 of the serialized `model`, and
/// `extensions` is a free-form bag for future metadata that older builds must
/// preserve rather than drop. Unknown keys are round-tripped untouched.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeUnit {
    pub format: String,
    /// Schema version of the model (see `officecore::schema`).
    pub version: u32,
    #[serde(default)]
    pub schema_version: Option<u32>,
    /// Canonical document family. Older files only carry `kind`.
    #[serde(default)]
    pub document_type: Option<String>,
    pub kind: String,
    #[serde(default)]
    pub application_version: Option<String>,
    pub title: String,
    pub saved_at: String,
    /// SHA-256 hex of the canonical serialization of `model`.
    #[serde(default)]
    pub checksum: Option<String>,
    /// Feature names the model actually uses, so a reader knows what matters.
    #[serde(default)]
    pub feature_manifest: Vec<String>,
    /// Free-form extension bag; never pruned by a save.
    #[serde(default)]
    pub extensions: serde_json::Map<String, Value>,
    #[serde(default)]
    pub warnings: Vec<String>,
    pub model: Value,
}

/// Saves the native unit format (`.oswk`): the complete model as JSON so no
/// information the suite understands is ever lost.
pub fn save_native(kind: &str, title: &str, model: Value, path: &Path) -> Result<SaveDocument, OfficeErrorPayload> {
    save_native_with_metadata(kind, title, model, path, serde_json::Map::new())
}

/// Like [`save_native`] but merges preserved `extensions` from a previously
/// read unit so unknown keys survive a round trip.
pub fn save_native_with_metadata(
    kind: &str,
    title: &str,
    model: Value,
    path: &Path,
    extensions: serde_json::Map<String, Value>,
) -> Result<SaveDocument, OfficeErrorPayload> {
    let checksum = officecore::unit::checksum_of(&model);
    let features = officecore::unit::feature_manifest(kind, &model);
    let unit = NativeUnit {
        format: "office-swiss-army-knife".into(),
        version: officecore::schema::SCHEMA_VERSION,
        schema_version: Some(officecore::schema::SCHEMA_VERSION),
        document_type: Some(kind.to_string()),
        kind: kind.to_string(),
        application_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        title: title.to_string(),
        saved_at: timestamp(),
        checksum: Some(checksum),
        feature_manifest: features,
        extensions,
        warnings: Vec::new(),
        model,
    };
    let bytes = serde_json::to_vec_pretty(&unit)
        .map_err(|error| payload(OfficeError::internal(format!("Could not encode the unit file: {error}"))))?;
    officecore::io::write_atomic(path, &bytes).map_err(payload)?;
    Ok(SaveDocument { path: path.to_string_lossy().to_string(), warnings: Vec::new() })
}

#[tauri::command]
pub async fn office_save_unit(
    kind: String,
    title: String,
    model: Value,
    path: String,
) -> Result<SaveDocument, OfficeErrorPayload> {
    let _permit = crate::concurrency::acquire().await;
    let task = tauri::async_runtime::spawn_blocking(move || {
        let path = output_path(&path)?;
        save_native(&kind, &title, model, &path)
    });
    task.await.map_err(|error| payload(OfficeError::internal(format!("worker thread failed: {error}"))))?
}

/// The capability matrix for one file extension (Compatibility Center).
#[tauri::command]
pub fn office_capabilities(extension: String) -> officecore::compat::FormatCapabilities {
    officecore::compat::format_capabilities(&extension)
}

/// What the document *model* supports for a kind, independent of the format.
#[tauri::command]
pub fn office_model_capabilities(kind: String) -> officecore::compat::DocumentCapabilities {
    officecore::compat::model_capabilities(&kind)
}

/// Lists every extension this build can open.
#[tauri::command]
pub fn office_supported_extensions() -> Vec<String> {
    officecore::compat::supported_open_extensions().iter().map(|value| value.to_string()).collect()
}

/// Reports what `kind`'s model would lose if it were saved as `target`.
#[tauri::command]
pub fn office_compatibility(
    kind: String,
    model: Value,
    target: String,
) -> Result<officecore::compat::CompatibilityReport, OfficeErrorPayload> {
    let report = match kind.as_str() {
        "writer" => officecore::compat::document_feature_report(&writer_from_value(model)?, &target),
        "calc" => officecore::compat::workbook_feature_report(&workbook_from_value(model)?, &target),
        "impress" => officecore::compat::deck_feature_report(&deck_from_value(model)?, &target),
        other => return Err(payload(OfficeError::invalid(format!("Unknown document kind '{other}'.")))),
    };
    Ok(report)
}

pub fn timestamp() -> String {
    let now =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|value| value.as_secs()).unwrap_or(0);
    let days = now / 86_400;
    let seconds = now % 86_400;
    let (year, month, day) = civil_from_days(days as i64 + 719_468);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", seconds / 3600, (seconds % 3600) / 60, seconds % 60)
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    // `saturating_sub` keeps the floor-division branch well-defined for
    // extreme negative input instead of overflowing in debug builds.
    let era = if z >= 0 { z } else { z.saturating_sub(146_096) } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

// ---------------------------------------------------------------------------
// PDF export
// ---------------------------------------------------------------------------

pub fn export_pdf(kind: &str, model: Value, path: &Path) -> Result<SaveDocument, OfficeErrorPayload> {
    let mut warnings = Vec::new();
    let bytes = match kind {
        "writer" => {
            let document = writer_from_value(model)?;
            let (bytes, watermark_warning) =
                watermark_pdf(layout::document_to_pdf(&document), document.watermark.as_ref());
            if let Some(warning) = watermark_warning {
                warnings.push(warning);
            }
            bytes
        }
        "calc" => layout::workbook_to_pdf(&workbook_from_value(model)?, 20),
        "impress" => layout::deck_to_pdf(&deck_from_value(model)?),
        other => {
            return Err(payload(OfficeError::invalid(format!("Cannot export {other} to PDF."))));
        }
    };
    officecore::io::write_atomic(path, &bytes).map_err(payload)?;
    Ok(SaveDocument { path: path.to_string_lossy().to_string(), warnings })
}

#[tauri::command]
pub async fn office_export_pdf(kind: String, model: Value, path: String) -> Result<SaveDocument, OfficeErrorPayload> {
    let _permit = crate::concurrency::acquire().await;
    let task = tauri::async_runtime::spawn_blocking(move || {
        let path = output_path(&path)?;
        export_pdf(&kind, model, &path)
    });
    task.await.map_err(|error| payload(OfficeError::internal(format!("worker thread failed: {error}"))))?
}

// ---------------------------------------------------------------------------
// Universal conversion
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionInfo {
    pub input: String,
    pub output: String,
    pub converted: bool,
    pub warnings: Vec<String>,
}

/// Maps a pdfcore failure onto the office payload, keeping the stable
/// password codes the frontend already translates.
fn pdfcore_payload(error: pdfcore::error::PdfError) -> OfficeErrorPayload {
    let code = match &error {
        pdfcore::error::PdfError::PasswordRequired => Some("password_required"),
        pdfcore::error::PdfError::WrongPassword => Some("wrong_password"),
        _ => None,
    };
    match code {
        Some(code) => OfficeErrorPayload { code: code.to_string(), message: error.to_string() },
        None => payload(OfficeError::internal(error.to_string())),
    }
}

/// Maps the layout pdfcore recovered onto the Writer model: headings use the
/// Heading1-3 styles, list items carry their list level, tables become real
/// Word tables, and every page after the first starts with a page break.
fn pdf_layout_to_document(title: &str, recovered: &pdfcore::pdf2doc::RecoveredDocument) -> TextDocument {
    use pdfcore::pdf2doc::{BlockKind, ListKind};

    let mut document = TextDocument { title: title.to_string(), ..Default::default() };
    if let Some(first) = recovered.pages.iter().find(|page| page.width > 0.0 && page.height > 0.0) {
        document.page.width_pt = first.width;
        document.page.height_pt = first.height;
        let (short, long) = (first.width.min(first.height), first.width.max(first.height));
        document.page.size = match (short.round() as i64, long.round() as i64) {
            (595, 842) => "a4",
            (612, 792) => "letter",
            _ => "custom",
        }
        .into();
        document.page.orientation = if first.width > first.height { "landscape" } else { "portrait" }.into();
    }
    document.blocks.clear();
    let mut page_break = false;
    for page in &recovered.pages {
        for (index, block) in page.blocks.iter().enumerate() {
            let starts_page = page_break && index == 0;
            if let BlockKind::Table(layout) = &block.kind {
                if starts_page {
                    // A table cannot carry a page break, so the break becomes
                    // its own block just before the table.
                    document.blocks.push(Block::PageBreak);
                }
                page_break = false;
                document.blocks.push(Block::Table { table: pdf_table_to_model(layout, page.width) });
                continue;
            }
            let mut props = ParaProps::default();
            match &block.kind {
                BlockKind::Paragraph => {}
                BlockKind::Heading(level) => props.style = format!("Heading{}", (*level).clamp(1, 3)),
                BlockKind::ListItem(item) => {
                    let kind = if item.kind == ListKind::Bullet { "bullet" } else { "number" };
                    props.space_after_pt = 2.0;
                    props.list = Some(ListInfo {
                        kind: kind.into(),
                        level: item.level.min(8),
                        start: item.start,
                        marker: item.marker.clone(),
                    });
                }
                // Tables are mapped above.
                BlockKind::Table(_) => {}
            }
            // The break goes on the first block of the page, not into a
            // paragraph of its own that could spill onto an extra page.
            props.page_break_before = starts_page;
            page_break = false;
            let runs = block
                .spans
                .iter()
                .map(|span| Run { text: span.text.clone(), bold: span.bold, italic: span.italic, ..Default::default() })
                .collect();
            document.blocks.push(Block::Paragraph { props, runs });
        }
        // A blank page is skipped, but the next page still starts on a new one.
        page_break = !document.blocks.is_empty();
    }
    if document.blocks.is_empty() {
        document.blocks.push(Block::paragraph(""));
    }
    document
}

/// A recovered table as a Writer table: one paragraph per cell with the
/// recovered styling. Column widths follow the cells' left edges; the last
/// column reaches the page's right margin.
fn pdf_table_to_model(layout: &pdfcore::pdf2doc::TableLayout, page_width: f64) -> TableData {
    let rows = layout
        .rows
        .iter()
        .map(|row| TableRow {
            cells: row
                .iter()
                .map(|cell| TableCell {
                    blocks: vec![Block::Paragraph {
                        props: ParaProps::default(),
                        runs: cell
                            .spans
                            .iter()
                            .map(|span| Run {
                                text: span.text.clone(),
                                bold: span.bold,
                                italic: span.italic,
                                ..Default::default()
                            })
                            .collect(),
                    }],
                    colspan: 1,
                    rowspan: 1,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        })
        .collect();
    TableData {
        rows,
        column_widths_pt: pdf_column_widths(layout, page_width),
        borders: true,
        border_color: "#94a3b8".into(),
        align: "left".into(),
    }
}

/// Column widths from the first row's cell starts: each column runs to the
/// next cell's left edge, and the last one to the page's right margin. A
/// minimum keeps a degenerate table legible.
fn pdf_column_widths(layout: &pdfcore::pdf2doc::TableLayout, page_width: f64) -> Vec<f64> {
    let Some(first) = layout.rows.first() else { return Vec::new() };
    let left = first.first().map_or(0.0, |cell| cell.x);
    first
        .iter()
        .enumerate()
        .map(|(index, cell)| {
            let width = match first.get(index + 1) {
                Some(next) => next.x - cell.x,
                None => page_width - left - cell.x,
            };
            width.max(12.0)
        })
        .collect()
}

/// The earlier conversion: one paragraph per text line pdfium extracts.
fn pdf_plain_text_document(
    input: &Path,
    title: &str,
    password: Option<&str>,
) -> Result<TextDocument, OfficeErrorPayload> {
    let pages = pdfcore::render::page_geometries(input, password).map_err(pdfcore_payload)?;
    let mut document = TextDocument { title: title.to_string(), ..Default::default() };
    for page in &pages {
        let content = pdfcore::render::extract_page_text(input, password, page.page).map_err(pdfcore_payload)?;
        for line in content.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                document.blocks.push(Block::paragraph(trimmed));
            }
        }
    }
    if document.blocks.is_empty() {
        document.blocks.push(Block::paragraph(""));
    }
    Ok(document)
}

pub fn convert(input: &Path, output: &Path, options: &ConvertOptions) -> Result<ConversionInfo, OfficeErrorPayload> {
    let input_extension = extension(input);
    let output_extension = extension(output);
    let password = options.password.as_deref();
    let mut warnings = Vec::new();

    // 1. PDF input conversions (images, text, Word document)
    if input_extension == "pdf" {
        if matches!(output_extension.as_str(), "jpg" | "jpeg" | "png") {
            let format = match output_extension.as_str() {
                "png" => pdfcore::images::ImageFormat::Png,
                _ => pdfcore::images::ImageFormat::Jpeg,
            };
            let out_dir = output.parent().unwrap_or_else(|| Path::new("."));
            let prefix = officecore::io::file_stem(output);
            let silent = |_| ();
            let cancel = pdfcore::CancelToken::new();
            let result = pdfcore::convert::pdf_to_images(
                input,
                out_dir,
                format,
                150,
                85,
                false,
                &prefix,
                &[],
                pdfcore::docutil::OverwritePolicy::Replace,
                password,
                &silent,
                &cancel,
            )
            .map_err(pdfcore_payload)?;
            return Ok(ConversionInfo {
                input: input.to_string_lossy().to_string(),
                output: result
                    .files
                    .first()
                    .map(|f| f.path.clone())
                    .unwrap_or_else(|| output.to_string_lossy().to_string()),
                converted: true,
                warnings: vec![format!("Extracted {} page(s) to images.", result.files.len())],
            });
        } else if output_extension == "txt" {
            let pages = pdfcore::render::page_geometries(input, password).map_err(pdfcore_payload)?;
            let mut text = String::new();
            for page in &pages {
                let content =
                    pdfcore::render::extract_page_text(input, password, page.page).map_err(pdfcore_payload)?;
                text.push_str(&content);
                text.push_str("\n\n");
            }
            officecore::io::write_atomic(output, text.as_bytes()).map_err(payload)?;
            return Ok(ConversionInfo {
                input: input.to_string_lossy().to_string(),
                output: output.to_string_lossy().to_string(),
                converted: true,
                warnings: vec![],
            });
        } else if output_extension == "docx" {
            // Layout recovery keeps paragraphs, headings, lists, tables and
            // pages; a file it cannot read (or one without a text layer) falls
            // back to one paragraph per extracted line. A missing or wrong
            // password, however, must surface as the real error instead of a
            // silent empty document.
            let title = officecore::io::file_stem(input);
            let recovered = match pdfcore::pdf2doc::recover_file(input, password) {
                Ok(found) if found.has_text() => Some(found),
                Ok(_) => None,
                Err(error) => {
                    if matches!(
                        error,
                        pdfcore::error::PdfError::PasswordRequired | pdfcore::error::PdfError::WrongPassword
                    ) {
                        return Err(pdfcore_payload(error));
                    }
                    None
                }
            };
            let (document, message) = match recovered {
                Some(found) => (
                    pdf_layout_to_document(&title, &found),
                    "Paragraphs, headings, lists, tables and page breaks recovered from the PDF into a Word DOCX document.",
                ),
                None => (
                    pdf_plain_text_document(input, &title, password)?,
                    "Text content extracted from PDF into Word DOCX document.",
                ),
            };
            officecore::docx::write_docx_file(output, &document).map_err(payload)?;
            return Ok(ConversionInfo {
                input: input.to_string_lossy().to_string(),
                output: output.to_string_lossy().to_string(),
                converted: true,
                warnings: vec![message.into()],
            });
        }
    }

    // 2. Image -> PDF conversion
    if matches!(input_extension.as_str(), "jpg" | "jpeg" | "png" | "bmp" | "gif" | "webp" | "tiff" | "tif")
        && output_extension == "pdf"
    {
        let items = vec![pdfcore::images::ImageItem { path: input.to_string_lossy().to_string(), rotation_delta: 0 }];
        let silent = |_| ();
        let cancel = pdfcore::CancelToken::new();
        pdfcore::images::images_to_pdf(
            &items,
            &Default::default(),
            output,
            pdfcore::docutil::OverwritePolicy::Replace,
            &silent,
            &cancel,
        )
        .map_err(|e| payload(OfficeError::internal(e.to_string())))?;
        return Ok(ConversionInfo {
            input: input.to_string_lossy().to_string(),
            output: output.to_string_lossy().to_string(),
            converted: true,
            warnings: vec![],
        });
    }

    let kind = kind_for_extension(&input_extension).ok_or_else(|| {
        payload(OfficeError::unsupported(format!("Converting .{input_extension} files is not supported.")))
    })?;

    let model = if is_native(input) {
        let bytes = officecore::io::read_bytes(input).map_err(payload)?;
        let unit: NativeUnit = serde_json::from_slice(&bytes)
            .map_err(|error| payload(OfficeError::corrupt(format!("The unit file is not valid: {error}"))))?;
        unit.model
    } else {
        open_path(input)?.model
    };

    if output_extension == "pdf" {
        let saved = export_pdf(kind, model, output)?;
        warnings.extend(saved.warnings);
    } else if output_extension == "oswk" {
        let title = officecore::io::file_stem(input);
        save_native(kind, &title, model, output)?;
    } else {
        let saved = save_model(kind, model, output)?;
        warnings.extend(saved.warnings);
    }
    Ok(ConversionInfo {
        input: input.to_string_lossy().to_string(),
        output: output.to_string_lossy().to_string(),
        converted: true,
        warnings,
    })
}

#[tauri::command]
pub async fn office_convert(
    input: String,
    output: String,
    options: Option<ConvertOptions>,
) -> Result<ConversionInfo, OfficeErrorPayload> {
    let options = options.unwrap_or_default();
    let _permit = crate::concurrency::acquire().await;
    let task = tauri::async_runtime::spawn_blocking(move || {
        let input = input_path(&input)?;
        let output = output_path(&output)?;
        convert(&input, &output, &options)
    });
    task.await.map_err(|error| payload(OfficeError::internal(format!("worker thread failed: {error}"))))?
}

/// Lists the conversions the converter can perform for a given extension.
///
/// UYAP `.udf` is experimental (no real UYAP file has verified the round trip
/// yet, RELEASE_READINESS.md), so it is deliberately absent as both a source
/// and a target; the engine still reads and writes it behind the
/// `experimentalUdf` flag.
#[tauri::command]
pub fn office_conversion_targets(extension: String) -> Vec<String> {
    match extension.trim_start_matches('.').to_ascii_lowercase().as_str() {
        "docx" | "docm" | "dotx" | "doc" | "dot" => {
            vec!["pdf".into(), "docx".into(), "odt".into(), "rtf".into(), "txt".into(), "html".into(), "oswk".into()]
        }
        "odt" | "rtf" | "txt" | "md" => {
            vec!["pdf".into(), "docx".into(), "odt".into(), "rtf".into(), "txt".into(), "html".into(), "oswk".into()]
        }
        "xlsx" | "ods" | "csv" | "tsv" | "xls" => {
            vec!["pdf".into(), "xlsx".into(), "ods".into(), "csv".into(), "oswk".into()]
        }
        "pptx" | "odp" => vec!["pdf".into(), "pptx".into(), "odp".into(), "oswk".into()],
        "oswk" => {
            vec!["pdf".into(), "docx".into(), "xlsx".into(), "pptx".into(), "odt".into(), "ods".into(), "odp".into()]
        }
        "pdf" => vec!["jpg".into(), "png".into(), "txt".into(), "docx".into()],
        "jpg" | "jpeg" | "png" | "bmp" | "webp" => vec!["pdf".into()],
        _ => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// e-Fatura / e-Arşiv (UBL-TR)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceDocument {
    pub kind: String,
    pub title: String,
    /// The Writer model rendered with the app's own deterministic template.
    pub model: Value,
    pub invoice: ubl::UblInvoice,
    pub warnings: Vec<String>,
    pub issues: Vec<ubl::UblIssue>,
}

/// Parses a `.xml`/`.zip` UBL-TR invoice into the app's document shape.
pub fn invoice_document(path: &Path) -> Result<InvoiceDocument, OfficeErrorPayload> {
    let read = ubl::read_ubl_file(path).map_err(payload)?;
    let document = ubl::invoice_to_document(&read.invoice);
    Ok(InvoiceDocument {
        kind: "writer".into(),
        title: document.title.clone(),
        model: to_value(document)?,
        invoice: read.invoice,
        warnings: read.warnings,
        issues: read.issues,
    })
}

/// Reads every invoice in one `.xml`/`.zip` file.
pub fn invoice_package(path: &Path) -> Result<Vec<InvoiceDocument>, OfficeErrorPayload> {
    let reads = ubl::read_ubl_package_file(path).map_err(payload)?;
    let mut documents = Vec::with_capacity(reads.len());
    for read in reads {
        let document = ubl::invoice_to_document(&read.invoice);
        documents.push(InvoiceDocument {
            kind: "writer".into(),
            title: document.title.clone(),
            model: to_value(document)?,
            invoice: read.invoice,
            warnings: read.warnings,
            issues: read.issues,
        });
    }
    Ok(documents)
}

/// Reads an `.eml` message and extracts its attachments into the cache.
pub fn eml_message(path: &Path) -> Result<officecore::eml::EmlMessage, OfficeErrorPayload> {
    officecore::eml::read_eml_file(path, &officecore::eml::default_cache_dir()).map_err(payload)
}

/// Opens a `.xml`/`.zip` UBL-TR invoice and returns both the parsed fields and
/// the Writer model the app renders (never the invoice's own XSLT).
#[tauri::command]
pub async fn office_open_invoice(path: String) -> Result<InvoiceDocument, OfficeErrorPayload> {
    let _permit = crate::concurrency::acquire().await;
    let task = tauri::async_runtime::spawn_blocking(move || {
        let path = input_path(&path)?;
        let json = crate::worker::run_json_or_local(&crate::worker::Job::InvoiceOpen { path })
            .map_err(|error| payload(OfficeError::invalid(error)))?;
        serde_json::from_str(&json)
            .map_err(|error| payload(OfficeError::internal(format!("the parser result is not valid: {error}"))))
    });
    task.await.map_err(|error| payload(OfficeError::internal(format!("worker thread failed: {error}"))))?
}

#[derive(Debug, Clone, Serialize)]
pub struct InvoiceSummary {
    /// A Calc workbook model for the VAT summary; save it as `.xlsx`/`.ods`.
    pub workbook: Value,
    pub warnings: Vec<String>,
    pub count: usize,
}

/// Reads every invoice in one `.xml`/`.zip` file. A zip can hold several
/// invoices; the single-invoice command returns only the first.
#[tauri::command]
pub async fn office_open_invoice_package(path: String) -> Result<Vec<InvoiceDocument>, OfficeErrorPayload> {
    let _permit = crate::concurrency::acquire().await;
    let task = tauri::async_runtime::spawn_blocking(move || {
        let path = input_path(&path)?;
        let json = crate::worker::run_json_or_local(&crate::worker::Job::InvoicePackage { path })
            .map_err(|error| payload(OfficeError::invalid(error)))?;
        serde_json::from_str(&json)
            .map_err(|error| payload(OfficeError::internal(format!("the parser result is not valid: {error}"))))
    });
    task.await.map_err(|error| payload(OfficeError::internal(format!("worker thread failed: {error}"))))?
}

/// Reads every invoice in the selection and builds the VAT summary workbook.
#[tauri::command]
pub async fn office_invoice_summary(paths: Vec<String>) -> Result<InvoiceSummary, OfficeErrorPayload> {
    let _permit = crate::concurrency::acquire().await;
    let task = tauri::async_runtime::spawn_blocking(move || {
        let mut invoices = Vec::new();
        let mut warnings = Vec::new();
        for path in &paths {
            let source = input_path(path)?;
            match ubl::read_ubl_package_file(&source) {
                Ok(reads) => {
                    for read in reads {
                        warnings.extend(read.warnings);
                        warnings.extend(read.issues.iter().map(|issue| issue.message.clone()));
                        invoices.push(read.invoice);
                    }
                }
                Err(error) => warnings.push(format!("{path}: {error}")),
            }
        }
        if invoices.is_empty() {
            return Err(payload(OfficeError::invalid("No UBL invoice could be read from the selection.")));
        }
        let count = invoices.len();
        let workbook = ubl::vat_summary_workbook(&invoices);
        Ok::<_, OfficeErrorPayload>(InvoiceSummary { workbook: to_value(workbook)?, warnings, count })
    });
    task.await.map_err(|error| payload(OfficeError::internal(format!("worker thread failed: {error}"))))?
}

/// The dated reference VAT-rate table shown next to the validation results.
#[tauri::command]
pub fn office_vat_rates() -> Vec<ubl::VatRate> {
    ubl::vat_rates()
}

// ---------------------------------------------------------------------------
// Address mail merge
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MailMergeRequest {
    pub template: String,
    pub data: String,
    pub output_dir: String,
    /// "pdf" or "docx".
    pub format: String,
    /// A data field used as the output file name; otherwise `<template>-<n>`.
    #[serde(default)]
    pub name_field: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MailMergeError {
    pub row: usize,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MailMergeReport {
    pub produced: Vec<String>,
    pub errors: Vec<MailMergeError>,
}

fn safe_file_stem(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if matches!(ch, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || ch.is_control() {
                '_'
            } else {
                ch
            }
        })
        .collect::<String>()
        .trim()
        .trim_matches('.')
        .chars()
        .take(64)
        .collect()
}

/// Fills a Writer template's `{{alan}}` placeholders from a CSV file and writes
/// one document per row. A row that misses a placeholder is reported, not
/// written.
#[tauri::command]
pub async fn office_mail_merge(request: MailMergeRequest) -> Result<MailMergeReport, OfficeErrorPayload> {
    let _permit = crate::concurrency::acquire().await;
    let task = tauri::async_runtime::spawn_blocking(move || mail_merge(request));
    task.await.map_err(|error| payload(OfficeError::internal(format!("worker thread failed: {error}"))))?
}

fn mail_merge(request: MailMergeRequest) -> Result<MailMergeReport, OfficeErrorPayload> {
    let format = request.format.trim().to_ascii_lowercase();
    if format != "pdf" && format != "docx" {
        return Err(payload(OfficeError::invalid("The output format must be pdf or docx.")));
    }
    let template_path = input_path(&request.template)?;
    let opened = open_path(&template_path)?;
    let template: officecore::model::TextDocument = serde_json::from_value(opened.model)
        .map_err(|error| payload(OfficeError::invalid(format!("The template is not a Writer document: {error}"))))?;
    let data_path = input_path(&request.data)?;
    let data_bytes = std::fs::read(data_path.as_path())
        .map_err(|error| payload(OfficeError::from_io(error, data_path.as_path())))?;
    let rows = officecore::mailmerge::parse_rows(&data_bytes);
    if rows.is_empty() {
        return Err(payload(OfficeError::invalid("The data file has no data rows.")));
    }
    let out_dir = output_path(&request.output_dir)?;
    std::fs::create_dir_all(out_dir.as_path())
        .map_err(|error| payload(OfficeError::from_io(error, out_dir.as_path())))?;
    let stem = officecore::io::file_stem(&template_path);
    let mut produced = Vec::new();
    let mut errors = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let missing = officecore::mailmerge::missing_fields(&template, row);
        if !missing.is_empty() {
            errors
                .push(MailMergeError { row: index + 1, message: format!("missing field(s): {}", missing.join(", ")) });
            continue;
        }
        let document = officecore::mailmerge::merge_document(&template, row);
        let name = request
            .name_field
            .as_deref()
            .and_then(|field| row.get(field))
            .map(|value| safe_file_stem(value))
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| format!("{stem}-{}", index + 1));
        let path = out_dir.as_path().join(format!("{name}.{format}"));
        let written = if format == "pdf" {
            officecore::io::write_atomic(&path, &officecore::layout::document_to_pdf(&document))
        } else {
            officecore::docx::write_docx(&document).and_then(|bytes| officecore::io::write_atomic(&path, &bytes))
        };
        match written {
            Ok(()) => produced.push(path.to_string_lossy().to_string()),
            Err(error) => errors.push(MailMergeError { row: index + 1, message: error.to_string() }),
        }
    }
    Ok(MailMergeReport { produced, errors })
}

// ---------------------------------------------------------------------------
// Document scanner (Android camera path)
// ---------------------------------------------------------------------------

/// One captured page: an image file plus the four page corners the user tapped
/// (in source pixels). Without corners the image is used as it is.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanPage {
    pub path: String,
    #[serde(default)]
    pub corners: Option<Vec<[f32; 2]>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanRequest {
    pub pages: Vec<ScanPage>,
    pub output: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub output: String,
    pub pages: usize,
    pub corrected: usize,
    pub warnings: Vec<String>,
}

/// The corner picker's view of one page photo (see `pdfcore::scan::preview`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanPreview {
    /// A small upright JPEG as a `data:` URL.
    pub data_url: String,
    /// Upright full-size width and height: the corners are sent in these pixels.
    pub width: u32,
    pub height: u32,
}

/// Longest side of the corner picker's preview.
const SCAN_PREVIEW_SIDE: u32 = 1280;

/// A small upright preview of a page photo for the corner picker. The photo
/// itself can be 12 MP and carry its rotation in EXIF; the preview is turned
/// the same way the page will be, so the picked corners line up.
#[tauri::command]
pub async fn office_scan_preview(path: String) -> Result<ScanPreview, OfficeErrorPayload> {
    let task = tauri::async_runtime::spawn_blocking(move || scan_preview(&path));
    task.await.map_err(|error| payload(OfficeError::internal(format!("worker thread failed: {error}"))))?
}

fn scan_preview(path: &str) -> Result<ScanPreview, OfficeErrorPayload> {
    use base64::Engine;
    let source = input_path(path)?;
    let preview = pdfcore::scan::preview(&source, SCAN_PREVIEW_SIDE)
        .map_err(|error| payload(OfficeError::invalid(error.to_string())))?;
    let data_url =
        format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(&preview.jpeg));
    Ok(ScanPreview { data_url, width: preview.width, height: preview.height })
}

/// Turns each captured page upright, corrects its perspective when it has
/// corners and assembles a multi-page PDF through the existing image-to-PDF
/// writer. The photos come from the system camera (Android) or a file pick.
#[tauri::command]
pub async fn office_scan_pages(request: ScanRequest) -> Result<ScanResult, OfficeErrorPayload> {
    let _permit = crate::concurrency::acquire().await;
    let task = tauri::async_runtime::spawn_blocking(move || scan_pages(request));
    task.await.map_err(|error| payload(OfficeError::internal(format!("worker thread failed: {error}"))))?
}

fn scan_pages(request: ScanRequest) -> Result<ScanResult, OfficeErrorPayload> {
    if request.pages.is_empty() {
        return Err(payload(OfficeError::invalid("Select at least one page image.")));
    }
    let output = output_path(&request.output)?;
    let temp = tempfile::tempdir().map_err(|error| payload(OfficeError::internal(error.to_string())))?;
    let mut items = Vec::with_capacity(request.pages.len());
    let mut warnings = Vec::new();
    let mut corrected = 0usize;
    for (index, page) in request.pages.iter().enumerate() {
        let source = input_path(&page.path)?;
        let quad = page.corners.as_ref().filter(|corners| corners.len() == 4).map(|corners| {
            [
                [corners[0][0], corners[0][1]],
                [corners[1][0], corners[1][1]],
                [corners[2][0], corners[2][1]],
                [corners[3][0], corners[3][1]],
            ]
        });
        // Every page is turned upright first (camera photos carry their
        // rotation in EXIF), then flattened when it has corners.
        let path = temp.path().join(format!("page-{index}.jpg"));
        let target = match pdfcore::scan::prepare_page(&source, quad, &path) {
            Ok(true) => {
                if quad.is_some() {
                    corrected += 1;
                }
                path
            }
            Ok(false) => source.clone(),
            Err(error) => {
                warnings.push(format!("Page {} was used as it is: {error}", index + 1));
                source.clone()
            }
        };
        items.push(pdfcore::images::ImageItem { path: target.to_string_lossy().to_string(), rotation_delta: 0 });
    }
    let silent = |_| ();
    let cancel = pdfcore::CancelToken::new();
    pdfcore::images::images_to_pdf(
        &items,
        &Default::default(),
        &output,
        pdfcore::docutil::OverwritePolicy::Replace,
        &silent,
        &cancel,
    )
    .map_err(|error| payload(OfficeError::internal(error.to_string())))?;
    Ok(ScanResult { output: output.to_string_lossy().to_string(), pages: items.len(), corrected, warnings })
}

/// Opens a KEP `.eml` message (read-only): headers, body and attachments. Each
/// attachment is written to a temp directory so the UI can open it; the message
/// itself is never modified and nothing is sent anywhere.
#[tauri::command]
pub async fn office_open_eml(path: String) -> Result<officecore::eml::EmlMessage, OfficeErrorPayload> {
    let _permit = crate::concurrency::acquire().await;
    let task = tauri::async_runtime::spawn_blocking(move || {
        let path = input_path(&path)?;
        let json = crate::worker::run_json_or_local(&crate::worker::Job::EmlOpen { path })
            .map_err(|error| payload(OfficeError::invalid(error)))?;
        serde_json::from_str(&json)
            .map_err(|error| payload(OfficeError::internal(format!("the parser result is not valid: {error}"))))
    });
    task.await.map_err(|error| payload(OfficeError::internal(format!("worker thread failed: {error}"))))?
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceQr {
    pub payload: pdfcore::gibqr::GibQr,
    pub raw: String,
    pub page: u32,
    /// Whether the payload's sender VKN/TCKN passes its checksum.
    pub tax_id_valid: bool,
    pub warnings: Vec<String>,
}

/// Reads the GİB karekod (e-Fatura QR) from a PDF's first pages (or a given
/// page). Returns `None` when no QR is found, so the UI can say so plainly.
#[tauri::command]
pub async fn office_read_invoice_qr(path: String, page: Option<u32>) -> Result<Option<InvoiceQr>, OfficeErrorPayload> {
    let _permit = crate::concurrency::acquire().await;
    let task = tauri::async_runtime::spawn_blocking(move || {
        let source = input_path(&path)?;
        let found = match page {
            Some(page) => pdfcore::gibqr::decode_page(&source, None, page)
                .map_err(pdfcore_payload)?
                .map(|(qr, raw)| (qr, raw, page)),
            None => pdfcore::gibqr::decode_document(&source, None, 5).map_err(pdfcore_payload)?,
        };
        Ok::<_, OfficeErrorPayload>(found.map(|(payload, raw, page)| {
            let tax_id_valid = pdfcore::gibqr::tax_id_valid(&payload.vkn_tckn);
            let mut warnings = Vec::new();
            if payload.no.is_empty() {
                warnings.push("The QR payload carries no invoice number.".into());
            }
            if !payload.vkn_tckn.is_empty() && !tax_id_valid {
                warnings.push("The QR sender VKN/TCKN does not pass its checksum.".into());
            }
            InvoiceQr { payload, raw, page, tax_id_valid, warnings }
        }))
    });
    task.await.map_err(|error| payload(OfficeError::internal(format!("worker thread failed: {error}"))))?
}

// ---------------------------------------------------------------------------
// Cleaner
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn office_clean(path: String, options: CleanOptions) -> Result<CleanResult, OfficeErrorPayload> {
    let _permit = crate::concurrency::acquire().await;
    let task = tauri::async_runtime::spawn_blocking(move || {
        let source = input_path(&path)?;
        cleaner::ensure_supported(&source).map_err(payload)?;
        cleaner::clean_package(&source, &options).map_err(payload)
    });
    task.await.map_err(|error| payload(OfficeError::internal(format!("worker thread failed: {error}"))))?
}

#[tauri::command]
pub async fn office_image_footprint(path: String) -> Result<u64, OfficeErrorPayload> {
    let _permit = crate::concurrency::acquire().await;
    let task = tauri::async_runtime::spawn_blocking(move || {
        let path = input_path(&path)?;
        cleaner::image_footprint(&path).map_err(payload)
    });
    task.await.map_err(|error| payload(OfficeError::internal(format!("worker thread failed: {error}"))))?
}

// ---------------------------------------------------------------------------
// Stores (notes, planner, data, draw, autosave, recovery, history, favourites)
// ---------------------------------------------------------------------------

fn sanitize_key(key: &str) -> Result<String, OfficeErrorPayload> {
    let cleaned: String =
        key.chars().filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_').take(64).collect();
    if cleaned.is_empty() || cleaned != key {
        return Err(payload(OfficeError::invalid("Invalid store key.")));
    }
    Ok(cleaned)
}

fn config_dir(app: &AppHandle) -> Result<PathBuf, OfficeErrorPayload> {
    app.path()
        .app_config_dir()
        .map_err(|error| payload(OfficeError::internal(format!("Could not locate the app data directory: {error}"))))
}

fn store_path(app: &AppHandle, key: &str) -> Result<PathBuf, OfficeErrorPayload> {
    Ok(config_dir(app)?.join(format!("{key}.json")))
}

#[tauri::command]
pub fn store_load(app: AppHandle, key: String) -> Result<Value, OfficeErrorPayload> {
    let key = sanitize_key(&key)?;
    let path = store_path(&app, &key)?;
    if !path.exists() {
        return Ok(Value::Null);
    }
    let bytes = officecore::io::read_bytes(&path).map_err(payload)?;
    serde_json::from_slice(&bytes)
        .map_err(|error| payload(OfficeError::corrupt(format!("Stored data is damaged: {error}"))))
}

#[tauri::command]
pub fn store_save(app: AppHandle, key: String, value: Value) -> Result<(), OfficeErrorPayload> {
    let key = sanitize_key(&key)?;
    let path = store_path(&app, &key)?;
    let bytes = serde_json::to_vec(&value)
        .map_err(|error| payload(OfficeError::internal(format!("Could not encode data: {error}"))))?;
    officecore::io::write_atomic(&path, &bytes).map_err(payload)
}

#[tauri::command]
pub fn store_clear(app: AppHandle, key: String) -> Result<(), OfficeErrorPayload> {
    let key = sanitize_key(&key)?;
    let path = store_path(&app, &key)?;
    if path.exists() {
        std::fs::remove_file(&path).map_err(|error| payload(OfficeError::from_io(error, &path)))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Version history
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub version: u32,
    pub saved_at: String,
    pub title: String,
    pub kind: String,
    pub size: u64,
}

fn history_dir(app: &AppHandle, document_id: &str) -> Result<PathBuf, OfficeErrorPayload> {
    let id = sanitize_key(document_id)?;
    Ok(config_dir(app)?.join("versions").join(id))
}

/// How many versions a document keeps.
const HISTORY_LIMIT: usize = 25;

#[tauri::command]
pub fn history_push(
    app: AppHandle,
    document_id: String,
    kind: String,
    title: String,
    model: Value,
) -> Result<HistoryEntry, OfficeErrorPayload> {
    let dir = history_dir(&app, &document_id)?;
    push_version(&dir, kind, title, &model, HISTORY_LIMIT, officecore::io::write_atomic).map_err(payload)
}

/// Writes a new version file and updates the index, keeping the newest
/// `limit` versions. `write` is the atomic file writer (injected so a test can
/// make the index write fail).
fn push_version(
    dir: &Path,
    kind: String,
    title: String,
    model: &Value,
    limit: usize,
    write: impl Fn(&Path, &[u8]) -> OfficeResult<()>,
) -> OfficeResult<HistoryEntry> {
    std::fs::create_dir_all(dir).map_err(|error| OfficeError::from_io(error, dir))?;
    let mut index = read_history_index(dir);
    let version = index.iter().map(|entry| entry.version).max().unwrap_or(0) + 1;
    let payload_json = serde_json::json!({
        "version": version,
        "savedAt": timestamp(),
        "kind": kind,
        "title": title,
        "model": model,
    });
    let bytes = serde_json::to_vec(&payload_json)
        .map_err(|error| OfficeError::internal(format!("Could not encode the version: {error}")))?;
    write(&dir.join(format!("v{version}.json")), &bytes)?;
    let entry = HistoryEntry { version, saved_at: timestamp(), title, kind, size: bytes.len() as u64 };
    index.push(entry.clone());
    // Keep the newest versions.
    index.sort_by_key(|entry| entry.version);
    let mut pruned = Vec::new();
    while index.len() > limit {
        pruned.push(index.remove(0).version);
    }
    let index_bytes = serde_json::to_vec(&index)
        .map_err(|error| OfficeError::internal(format!("Could not encode the history index: {error}")))?;
    // The new index goes first: a crash before it leaves the old index, whose
    // files all still exist; a crash after it leaves at most an orphaned file.
    write(&dir.join("index.json"), &index_bytes)?;
    for version in &pruned {
        let _ = std::fs::remove_file(dir.join(format!("v{version}.json")));
    }
    Ok(entry)
}

fn read_history_index(dir: &Path) -> Vec<HistoryEntry> {
    let path = dir.join("index.json");
    if !path.exists() {
        return Vec::new();
    }
    std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Vec<HistoryEntry>>(&bytes).ok())
        .unwrap_or_default()
}

#[tauri::command]
pub fn history_list(app: AppHandle, document_id: String) -> Result<Vec<HistoryEntry>, OfficeErrorPayload> {
    let dir = history_dir(&app, &document_id)?;
    let mut index = read_history_index(&dir);
    index.sort_by_key(|entry| std::cmp::Reverse(entry.version));
    Ok(index)
}

#[tauri::command]
pub fn history_load(app: AppHandle, document_id: String, version: u32) -> Result<Value, OfficeErrorPayload> {
    let dir = history_dir(&app, &document_id)?;
    let path = dir.join(format!("v{version}.json"));
    let bytes = officecore::io::read_bytes(&path).map_err(payload)?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|error| payload(OfficeError::corrupt(format!("The saved version is damaged: {error}"))))?;
    Ok(value.get("model").cloned().unwrap_or(Value::Null))
}

#[tauri::command]
pub fn history_clear(app: AppHandle, document_id: String) -> Result<(), OfficeErrorPayload> {
    let dir = history_dir(&app, &document_id)?;
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|error| payload(OfficeError::from_io(error, &dir)))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Recovery (autosave snapshots)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryEntry {
    pub document_id: String,
    pub kind: String,
    pub title: String,
    pub path: Option<String>,
    pub saved_at: String,
    pub size: u64,
}

fn recovery_dir(app: &AppHandle) -> Result<PathBuf, OfficeErrorPayload> {
    Ok(config_dir(app)?.join("recovery"))
}

#[tauri::command]
pub fn recovery_save(
    app: AppHandle,
    document_id: String,
    kind: String,
    title: String,
    path: Option<String>,
    model: Value,
) -> Result<(), OfficeErrorPayload> {
    let id = sanitize_key(&document_id)?;
    let dir = recovery_dir(&app)?;
    std::fs::create_dir_all(&dir).map_err(|error| payload(OfficeError::from_io(error, &dir)))?;
    let payload_json = serde_json::json!({
        "documentId": id,
        "kind": kind,
        "title": title,
        "path": path,
        "savedAt": timestamp(),
        "model": model,
    });
    let bytes = serde_json::to_vec(&payload_json)
        .map_err(|error| payload(OfficeError::internal(format!("Could not encode the recovery snapshot: {error}"))))?;
    officecore::io::write_atomic(&dir.join(format!("{id}.json")), &bytes).map_err(payload)
}

#[tauri::command]
pub fn recovery_list(app: AppHandle) -> Result<Vec<RecoveryEntry>, OfficeErrorPayload> {
    let dir = recovery_dir(&app)?;
    let mut entries = Vec::new();
    let Ok(read_dir) = std::fs::read_dir(&dir) else { return Ok(entries) };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if extension(&path) != "json" {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let Ok(value) = serde_json::from_slice::<Value>(&bytes) else { continue };
        entries.push(RecoveryEntry {
            document_id: value.get("documentId").and_then(Value::as_str).unwrap_or_default().to_string(),
            kind: value.get("kind").and_then(Value::as_str).unwrap_or_default().to_string(),
            title: value.get("title").and_then(Value::as_str).unwrap_or_default().to_string(),
            path: value.get("path").and_then(Value::as_str).map(str::to_string),
            saved_at: value.get("savedAt").and_then(Value::as_str).unwrap_or_default().to_string(),
            size: bytes.len() as u64,
        });
    }
    entries.sort_by(|a, b| b.saved_at.cmp(&a.saved_at));
    Ok(entries)
}

#[tauri::command]
pub fn recovery_load(app: AppHandle, document_id: String) -> Result<Value, OfficeErrorPayload> {
    let id = sanitize_key(&document_id)?;
    let dir = recovery_dir(&app)?;
    let path = dir.join(format!("{id}.json"));
    let bytes = officecore::io::read_bytes(&path).map_err(payload)?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|error| payload(OfficeError::corrupt(format!("The recovery snapshot is damaged: {error}"))))?;
    let mut model = value.get("model").cloned().unwrap_or(Value::Null);
    // Images live in the content-addressed blob store; put them back so callers
    // always receive a complete model.
    rehydrate_blobs(&mut model, &dir.join("blobs")).map_err(|error| payload(OfficeError::corrupt(error)))?;
    Ok(model)
}

#[tauri::command]
pub fn recovery_discard(app: AppHandle, document_id: String) -> Result<(), OfficeErrorPayload> {
    let id = sanitize_key(&document_id)?;
    let path = recovery_dir(&app)?.join(format!("{id}.json"));
    if path.exists() {
        std::fs::remove_file(&path).map_err(|error| payload(OfficeError::from_io(error, &path)))?;
    }
    Ok(())
}

#[tauri::command]
pub fn recovery_discard_all(app: AppHandle) -> Result<(), OfficeErrorPayload> {
    let dir = recovery_dir(&app)?;
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|error| payload(OfficeError::from_io(error, &dir)))?;
    }
    Ok(())
}

/// A blob reference is a hex digest; anything else must never reach a join.
fn sanitize_blob_hash(hash: &str) -> Result<String, OfficeErrorPayload> {
    if hash.is_empty() || hash.len() > 64 || !hash.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(payload(OfficeError::invalid("Invalid image blob reference.")));
    }
    Ok(hash.to_ascii_lowercase())
}

/// Autosave that stores images separately. `model` already has each image's
/// `dataBase64` replaced by a `blobRef` digest; `blobs` carries only the blobs
/// the session has not sent yet, so an autosave of a document with unchanged
/// images moves no image data over IPC at all.
#[tauri::command]
pub fn recovery_save_blobs(
    app: AppHandle,
    document_id: String,
    kind: String,
    title: String,
    path: Option<String>,
    model: Value,
    blobs: std::collections::HashMap<String, String>,
) -> Result<(), OfficeErrorPayload> {
    let id = sanitize_key(&document_id)?;
    let dir = recovery_dir(&app)?;
    std::fs::create_dir_all(&dir).map_err(|error| payload(OfficeError::from_io(error, &dir)))?;
    if !blobs.is_empty() {
        let blob_dir = dir.join("blobs");
        std::fs::create_dir_all(&blob_dir).map_err(|error| payload(OfficeError::from_io(error, &blob_dir)))?;
        for (hash, data) in &blobs {
            let name = sanitize_blob_hash(hash)?;
            let target = blob_dir.join(&name);
            // Content-addressed: an existing blob is identical, so skip the write.
            if !target.exists() {
                std::fs::write(&target, data.as_bytes())
                    .map_err(|error| payload(OfficeError::from_io(error, &target)))?;
            }
        }
    }
    let payload_json = serde_json::json!({
        "documentId": id,
        "kind": kind,
        "title": title,
        "path": path,
        "savedAt": timestamp(),
        "model": model,
    });
    let bytes = serde_json::to_vec(&payload_json)
        .map_err(|error| payload(OfficeError::internal(format!("Could not encode the recovery snapshot: {error}"))))?;
    officecore::io::write_atomic(&dir.join(format!("{id}.json")), &bytes).map_err(payload)
}

/// Restores `dataBase64` from `blobRef` in place. A missing blob is a hard
/// error: dropping an image silently would be data loss.
fn rehydrate_blobs(value: &mut Value, blobs: &Path) -> Result<(), String> {
    match value {
        Value::Array(items) => {
            for item in items {
                rehydrate_blobs(item, blobs)?;
            }
        }
        Value::Object(map) => {
            let reference = map.get("blobRef").and_then(Value::as_str).map(str::to_string);
            for (_, child) in map.iter_mut() {
                rehydrate_blobs(child, blobs)?;
            }
            if let Some(reference) = reference {
                let name =
                    sanitize_blob_hash(&reference).map_err(|_| format!("Invalid blob reference: {reference}"))?;
                let path = blobs.join(&name);
                let bytes = std::fs::read(&path).map_err(|_| {
                    format!("The recovery snapshot references an image that is no longer stored ({reference}).")
                })?;
                let data =
                    String::from_utf8(bytes).map_err(|_| format!("The stored image {reference} is not valid text."))?;
                map.remove("blobRef");
                map.insert("dataBase64".to_string(), Value::String(data));
            }
        }
        _ => {}
    }
    Ok(())
}

fn collect_blob_refs(value: &Value, out: &mut std::collections::HashSet<String>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_blob_refs(item, out);
            }
        }
        Value::Object(map) => {
            if let Some(reference) = map.get("blobRef").and_then(Value::as_str) {
                out.insert(reference.to_ascii_lowercase());
            }
            for (_, child) in map {
                collect_blob_refs(child, out);
            }
        }
        _ => {}
    }
}

/// Deletes blobs no snapshot references any more (one blob can back many
/// snapshots, so this is a mark-and-sweep, never a per-snapshot delete).
/// Returns how many files were removed.
#[tauri::command]
pub fn recovery_gc_blobs(app: AppHandle) -> Result<u64, OfficeErrorPayload> {
    let dir = recovery_dir(&app)?;
    let blob_dir = dir.join("blobs");
    if !blob_dir.exists() {
        return Ok(0);
    }
    let mut referenced = std::collections::HashSet::new();
    if let Ok(read_dir) = std::fs::read_dir(&dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if extension(&path) != "json" {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else { continue };
            let Ok(value) = serde_json::from_slice::<Value>(&bytes) else { continue };
            if let Some(model) = value.get("model") {
                collect_blob_refs(model, &mut referenced);
            }
        }
    }
    let mut removed = 0u64;
    if let Ok(read_dir) = std::fs::read_dir(&blob_dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default().to_string();
            if !referenced.contains(&name) && std::fs::remove_file(&path).is_ok() {
                removed += 1;
            }
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_rejects_traversal() {
        assert!(sanitize_key("../secret").is_err());
        assert!(sanitize_key("notes").is_ok());
        assert!(sanitize_key("").is_err());
        assert!(sanitize_key("a/b").is_err());
    }

    #[test]
    fn blob_reference_validation() {
        assert!(sanitize_blob_hash("../x").is_err());
        assert!(sanitize_blob_hash("").is_err());
        assert!(sanitize_blob_hash("zz").is_err());
        assert_eq!(sanitize_blob_hash("A1B2").unwrap(), "a1b2");
    }

    #[test]
    fn invoice_qr_serializes_camel_case() {
        // The Invoices screen reads `qr.taxIdValid`; a snake_case key arrived as
        // undefined and left the tax-id badge red for every valid document.
        let qr = InvoiceQr {
            payload: pdfcore::gibqr::GibQr::default(),
            raw: String::new(),
            page: 1,
            tax_id_valid: true,
            warnings: Vec::new(),
        };
        let json = serde_json::to_value(&qr).expect("serialize");
        assert!(json.get("taxIdValid").is_some(), "the UI reads camelCase: {json}");
        assert!(json.get("tax_id_valid").is_none(), "snake_case must not leak: {json}");
    }

    #[test]
    fn blob_refs_are_collected_for_gc() {
        let model = serde_json::json!({
            "blocks": [
                { "image": { "blobRef": "AABB" } },
                { "image": { "blobRef": "ccdd" } },
                { "note": "no blob here" }
            ]
        });
        let mut refs = std::collections::HashSet::new();
        collect_blob_refs(&model, &mut refs);
        // Lower-cased, and only real references are kept.
        assert!(refs.contains("aabb"));
        assert!(refs.contains("ccdd"));
        assert_eq!(refs.len(), 2);
    }

    #[test]
    fn rehydrate_restores_images_and_fails_loudly_on_a_missing_blob() {
        let dir = std::env::temp_dir().join(format!("omnioffice-blob-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("abcd"), "IMAGEDATA").unwrap();

        let mut model = serde_json::json!({ "blocks": [ { "blobRef": "abcd", "name": "a.png" } ] });
        rehydrate_blobs(&mut model, &dir).unwrap();
        assert_eq!(model["blocks"][0]["dataBase64"], "IMAGEDATA");
        assert!(model["blocks"][0].get("blobRef").is_none());

        let mut missing = serde_json::json!({ "blobRef": "deadbeef" });
        assert!(rehydrate_blobs(&mut missing, &dir).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn extension_detection() {
        assert_eq!(kind_for_extension("docx"), Some("writer"));
        assert_eq!(kind_for_extension("doc"), None);
        assert_eq!(kind_for_extension("udf"), Some("writer"));
        assert_eq!(kind_for_extension("xlsx"), Some("calc"));
        assert_eq!(kind_for_extension("pptx"), Some("impress"));
        assert_eq!(kind_for_extension("pdf"), None);
    }

    #[test]
    fn conversion_targets_include_new_formats() {
        let pdf_targets = office_conversion_targets("pdf".into());
        assert!(pdf_targets.contains(&"jpg".into()));
        assert!(pdf_targets.contains(&"png".into()));
        assert!(pdf_targets.contains(&"txt".into()));
        assert!(pdf_targets.contains(&"docx".into()));

        let doc_targets = office_conversion_targets("doc".into());
        assert!(doc_targets.contains(&"pdf".into()));
        assert!(doc_targets.contains(&"docx".into()));

        // UYAP `.udf` is experimental: never listed as a source or a target
        // until real UYAP files verify the round trip (RELEASE_READINESS.md).
        assert!(office_conversion_targets("udf".into()).is_empty());
        assert!(!office_conversion_targets("docx".into()).contains(&"udf".into()));
        assert!(!office_conversion_targets("odt".into()).contains(&"udf".into()));
    }

    #[test]
    fn udf_opens_saves_and_converts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("karar.udf");
        let mut document = officecore::model::TextDocument::new_blank("Karar");
        document.blocks = vec![
            officecore::model::Block::Paragraph {
                props: officecore::model::ParaProps { align: "center".into(), ..Default::default() },
                runs: vec![officecore::model::Run {
                    text: "Türkçe karar metni".into(),
                    bold: true,
                    ..Default::default()
                }],
            },
            officecore::model::Block::Paragraph {
                props: Default::default(),
                runs: vec![officecore::model::Run { text: "Gerekçe".into(), ..Default::default() }],
            },
        ];
        let model = serde_json::to_value(&document).unwrap();
        save_model("writer", model, &path).unwrap();
        assert!(std::fs::read(&path).unwrap().starts_with(b"PK"), "the UDF is a ZIP");

        let opened = open_path(&path).unwrap();
        assert_eq!(opened.kind, "writer");
        assert_eq!(opened.title, "Karar");
        assert!(opened.warnings.is_empty(), "warnings: {:?}", opened.warnings);
        let reopened: officecore::model::TextDocument = serde_json::from_value(opened.model).unwrap();
        assert!(reopened.plain_text().contains("Türkçe karar metni"));

        // Both conversions the format promises must work.
        let docx = dir.path().join("karar.docx");
        convert(&path, &docx, &Default::default()).unwrap();
        let back = officecore::docx::read_docx_file(&docx).unwrap();
        assert!(back.document.plain_text().contains("Türkçe karar metni"));

        let pdf = dir.path().join("karar.pdf");
        convert(&path, &pdf, &Default::default()).unwrap();
        assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF"));
    }

    #[test]
    fn invoice_opens_and_builds_a_vat_summary() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fatura.xml");
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Invoice xmlns:cac="urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2" xmlns:cbc="urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2">
  <cbc:ID>GIB2026000000009</cbc:ID>
  <cbc:UUID>ettn-9</cbc:UUID>
  <cbc:IssueDate>2026-05-06</cbc:IssueDate>
  <cbc:DocumentCurrencyCode>TRY</cbc:DocumentCurrencyCode>
  <cac:AccountingSupplierParty><cac:Party><cac:PartyName><cbc:Name>Satıcı A.Ş.</cbc:Name></cac:PartyName></cac:Party></cac:AccountingSupplierParty>
  <cac:TaxTotal><cbc:TaxAmount>200.00</cbc:TaxAmount>
    <cac:TaxSubtotal><cbc:TaxableAmount>1000.00</cbc:TaxableAmount><cbc:TaxAmount>200.00</cbc:TaxAmount><cbc:Percent>20.00</cbc:Percent></cac:TaxSubtotal>
  </cac:TaxTotal>
  <cac:LegalMonetaryTotal>
    <cbc:LineExtensionAmount>1000.00</cbc:LineExtensionAmount>
    <cbc:TaxExclusiveAmount>1000.00</cbc:TaxExclusiveAmount>
    <cbc:TaxInclusiveAmount>1200.00</cbc:TaxInclusiveAmount>
    <cbc:PayableAmount>1200.00</cbc:PayableAmount>
  </cac:LegalMonetaryTotal>
  <cac:InvoiceLine>
    <cbc:ID>1</cbc:ID>
    <cbc:InvoicedQuantity>10</cbc:InvoicedQuantity>
    <cbc:LineExtensionAmount>1000.00</cbc:LineExtensionAmount>
    <cac:TaxTotal><cbc:TaxAmount>200.00</cbc:TaxAmount><cac:TaxSubtotal><cbc:TaxableAmount>1000.00</cbc:TaxableAmount><cbc:TaxAmount>200.00</cbc:TaxAmount><cbc:Percent>20.00</cbc:Percent></cac:TaxSubtotal></cac:TaxTotal>
    <cac:Item><cbc:Name>Hizmet</cbc:Name></cac:Item>
    <cac:Price><cbc:PriceAmount>100.00</cbc:PriceAmount></cac:Price>
  </cac:InvoiceLine>
</Invoice>"#;
        std::fs::write(&path, xml).unwrap();

        // The reader returns the fields and a Writer model that exports to PDF.
        let read = officecore::ubl::read_ubl_file(&path).unwrap();
        assert_eq!(read.invoice.number, "GIB2026000000009");
        assert!(read.issues.is_empty(), "issues: {:?}", read.issues);
        let document = officecore::ubl::invoice_to_document(&read.invoice);
        assert!(officecore::layout::document_to_pdf(&document).starts_with(b"%PDF"));

        // The VAT summary aggregates the invoice into a Calc workbook.
        let workbook = officecore::ubl::vat_summary_workbook(&[read.invoice]);
        assert_eq!(workbook.sheets[0].name, "KDV Özeti");
        let package = officecore::xlsx::write_xlsx_package(&workbook).unwrap();
        assert!(package.bytes.starts_with(b"PK"));
    }

    /// A two-page PDF: heading, paragraph and bullets, then a heading and a
    /// paragraph, with a page number in each footer.
    fn write_sample_pdf(path: &Path) {
        use lopdf::{dictionary, Document, Object, Stream};
        let mut doc = Document::new();
        let regular = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
        }));
        let bold = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica-Bold", "Encoding" => "WinAnsiEncoding",
        }));
        let resources = doc.add_object(Object::Dictionary(dictionary! {
            "Font" => dictionary! { "F1" => regular, "F2" => bold },
        }));
        let pages: [&[(&str, f64, f64, &str)]; 2] = [
            &[
                ("F2", 24.0, 760.0, "Quarterly Notes"),
                ("F1", 11.0, 716.0, "The first quarter went well and the team shipped every feature that was planned"),
                ("F1", 11.0, 702.0, "for the release, including the new importer."),
                ("F1", 11.0, 678.0, "- Faster start up"),
                ("F1", 11.0, 664.0, "- Smaller installers"),
                ("F1", 10.0, 40.0, "1"),
            ],
            &[
                ("F2", 24.0, 760.0, "Next Steps"),
                ("F1", 11.0, 716.0, "Planning for the second quarter starts next week with a review of the backlog."),
                ("F1", 10.0, 40.0, "2"),
            ],
        ];
        let mut kids = Vec::new();
        for rows in pages {
            let mut content = String::new();
            for (font, size, y, text) in rows {
                let x = if *y < 100.0 { 296.0 } else { 72.0 };
                content.push_str(&format!("BT\n/{font} {size} Tf\n{x} {y} Td\n({text}) Tj\nET\n"));
            }
            let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content.into_bytes())));
            kids.push(doc.add_object(Object::Dictionary(dictionary! {
                "Type" => "Page",
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
                "Resources" => resources,
                "Contents" => content_id,
            })));
        }
        let pages_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids.iter().map(|id| Object::Reference(*id)).collect::<Vec<Object>>(),
            "Count" => kids.len() as i64,
        }));
        for id in &kids {
            doc.get_object_mut(*id).unwrap().as_dict_mut().unwrap().set("Parent", Object::Reference(pages_id));
        }
        let catalog = doc.add_object(Object::Dictionary(dictionary! { "Type" => "Catalog", "Pages" => pages_id }));
        doc.trailer.set("Root", Object::Reference(catalog));
        doc.save(path).expect("save pdf");
    }

    #[test]
    fn pdf_to_docx_keeps_headings_lists_and_pages() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("notes.pdf"), dir.path().join("notes.docx"));
        write_sample_pdf(&input);
        convert(&input, &output, &ConvertOptions::default()).expect("convert");

        let read = docx::read_docx_file(&output).expect("read docx");
        let paragraphs: Vec<(&ParaProps, String)> = read
            .document
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::Paragraph { props, runs } => Some((props, runs.iter().map(|run| run.text.as_str()).collect())),
                _ => None,
            })
            .collect();
        let texts: Vec<&str> = paragraphs.iter().map(|(_, text)| text.as_str()).collect();
        assert_eq!(
            texts,
            [
                "Quarterly Notes",
                "The first quarter went well and the team shipped every feature that was planned for the release, including the new importer.",
                "Faster start up",
                "Smaller installers",
                "Next Steps",
                "Planning for the second quarter starts next week with a review of the backlog.",
            ]
        );
        assert_eq!(paragraphs[0].0.style, "Heading1");
        assert_eq!(paragraphs[1].0.style, "Normal");
        assert!(paragraphs[2].0.list.as_ref().is_some_and(|list| list.kind == "bullet"));
        assert!(paragraphs[3].0.list.is_some());
        assert!(paragraphs[4].0.page_break_before);
        assert!(!paragraphs[0].0.page_break_before);
        assert!(!paragraphs[5].0.page_break_before);
    }

    /// A one-page PDF with a two-column, three-row table and a short line
    /// above it; every cell is its own text-positioning run.
    fn write_table_pdf(path: &Path) {
        use lopdf::{dictionary, Document, Object, Stream};
        let mut doc = Document::new();
        let regular = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
        }));
        let resources = doc.add_object(Object::Dictionary(dictionary! { "Font" => dictionary! { "F1" => regular } }));
        let rows = [(640.0, "Station", "Flow rate"), (626.0, "North bridge", "12"), (612.0, "Old mill", "9")];
        let mut content = String::from("BT\n/F1 11 Tf\n72 700 Td\n(Measurements) Tj\nET\n");
        for (y, left, right) in rows {
            content.push_str(&format!(
                "BT\n/F1 11 Tf\n72 {y} Td\n({left}) Tj\nET\nBT\n/F1 11 Tf\n320 {y} Td\n({right}) Tj\nET\n"
            ));
        }
        let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content.into_bytes())));
        let page_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Resources" => resources,
            "Contents" => content_id,
        }));
        let pages_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1,
        }));
        doc.get_object_mut(page_id).unwrap().as_dict_mut().unwrap().set("Parent", Object::Reference(pages_id));
        let catalog = doc.add_object(Object::Dictionary(dictionary! { "Type" => "Catalog", "Pages" => pages_id }));
        doc.trailer.set("Root", Object::Reference(catalog));
        doc.save(path).expect("save pdf");
    }

    #[test]
    fn pdf_to_docx_recovers_simple_tables() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("table.pdf"), dir.path().join("table.docx"));
        write_table_pdf(&input);
        convert(&input, &output, &ConvertOptions::default()).expect("convert");

        let read = docx::read_docx_file(&output).expect("read docx");
        let table = read
            .document
            .blocks
            .iter()
            .find_map(|block| match block {
                Block::Table { table } => Some(table),
                _ => None,
            })
            .expect("the recovered document must contain a table");
        assert_eq!(table.rows.len(), 3);
        assert_eq!(table.rows[0].cells.len(), 2);
        assert_eq!(table.rows[0].cells[0].blocks[0].plain_text().trim(), "Station");
        assert_eq!(table.rows[0].cells[1].blocks[0].plain_text().trim(), "Flow rate");
        assert!(table.rows[1].cells[0].blocks[0].plain_text().contains("North bridge"));
        assert!(table.rows[2].cells[1].blocks[0].plain_text().contains("9"));
        assert!(table.borders);
        assert!(table.column_widths_pt.iter().all(|width| *width > 0.0));
        assert!(read.document.blocks.iter().any(|block| matches!(block, Block::Paragraph { .. })));
    }

    #[test]
    fn protected_pdf_needs_its_password_to_convert() {
        use pdfcore::docutil::OverwritePolicy;
        use pdfcore::security::{protect_pdf, ProtectOptions};

        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("notes.pdf");
        let protected = dir.path().join("notes-locked.pdf");
        let output = dir.path().join("notes.docx");
        write_sample_pdf(&input);
        let options = ProtectOptions {
            user_password: "s3cret".into(),
            owner_password: "s3cret".into(),
            allow_printing: true,
            allow_copying: true,
            allow_editing: true,
            allow_commenting: true,
        };
        protect_pdf(&input, &protected, &options, OverwritePolicy::Replace, None).expect("protect");

        // No password: the real PasswordRequired error, not an empty document.
        let error = convert(&protected, &output, &ConvertOptions::default()).expect_err("password required");
        assert_eq!((error.code.as_str(), output.exists()), ("password_required", false));

        // A wrong password is reported as such.
        let wrong = ConvertOptions { password: Some("guess".into()), ..Default::default() };
        let error = convert(&protected, &output, &wrong).expect_err("wrong password");
        assert_eq!(error.code, "wrong_password");

        // The right password converts.
        let right = ConvertOptions { password: Some("s3cret".into()), ..Default::default() };
        convert(&protected, &output, &right).expect("convert with password");
        let read = docx::read_docx_file(&output).expect("read docx");
        assert!(read.document.plain_text().contains("Quarterly Notes"));
    }

    #[test]
    fn timestamp_is_iso_like() {
        let value = timestamp();
        assert_eq!(value.len(), 20);
        assert!(value.ends_with('Z'));
    }

    #[test]
    fn writer_pdf_watermark_is_applied_as_a_new_revision() {
        let mut document = TextDocument::new_blank("Watermark");
        document.blocks = vec![Block::paragraph("Body")];
        let plain = layout::document_to_pdf(&document);

        // No watermark: the bytes pass through untouched.
        let (same, warning) = watermark_pdf(plain.clone(), None);
        assert_eq!(plain, same);
        assert!(warning.is_none());

        document.watermark = Some(Watermark::default());
        let (marked, warning) = watermark_pdf(plain.clone(), document.watermark.as_ref());
        assert!(warning.is_none(), "warning: {warning:?}");
        assert_ne!(plain, marked, "the watermark must change the produced PDF");
        assert!(marked.starts_with(b"%PDF"));
    }

    #[test]
    fn odp_save_surfaces_package_warnings() {
        let mut deck = Deck::new_blank("Deck");
        let mut chart = SlideObject::new("chart", 10.0, 10.0, 300.0, 200.0);
        // Radar charts cannot be written to ODP, so the package must warn.
        chart.chart = Some(ChartData {
            kind: "radar".into(),
            series: vec![ChartSeries { name: "S".into(), range: "A1:A3".into(), color: None }],
            ..Default::default()
        });
        deck.slides[0].objects.push(chart);
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("deck.odp");
        let saved = save_model("impress", serde_json::to_value(&deck).unwrap(), &path).expect("save odp");
        assert!(!saved.warnings.is_empty(), "the ODP package warnings must be surfaced");
        assert!(path.exists());
    }

    fn push(dir: &Path, text: &str, limit: usize) -> OfficeResult<HistoryEntry> {
        push_version(dir, "writer".into(), "Doc".into(), &serde_json::json!({ "text": text }), limit, |path, bytes| {
            officecore::io::write_atomic(path, bytes)
        })
    }

    fn version_files(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn history_keeps_the_newest_versions_and_nothing_else() {
        let dir = tempfile::tempdir().expect("tempdir");
        for round in 1..=5 {
            let entry = push(dir.path(), &format!("text {round}"), 3).unwrap();
            assert_eq!(entry.version, round);
        }
        let index = read_history_index(dir.path());
        assert_eq!(index.iter().map(|entry| entry.version).collect::<Vec<_>>(), vec![3, 4, 5]);
        // Pruned files are gone and no temp file is left behind.
        assert_eq!(version_files(dir.path()), vec!["index.json", "v3.json", "v4.json", "v5.json"]);
        let newest: Value = serde_json::from_slice(&std::fs::read(dir.path().join("v5.json")).unwrap()).unwrap();
        assert_eq!(newest["model"]["text"], "text 5");
    }

    #[test]
    fn history_never_deletes_a_version_before_the_new_index_is_written() {
        let dir = tempfile::tempdir().expect("tempdir");
        for round in 1..=3 {
            push(dir.path(), &format!("text {round}"), 3).unwrap();
        }
        let before = std::fs::read(dir.path().join("index.json")).unwrap();
        // The fourth push would prune v1, but the index write fails.
        let failed =
            push_version(dir.path(), "writer".into(), "Doc".into(), &serde_json::json!({}), 3, |path, bytes| {
                if path.file_name().is_some_and(|name| name == "index.json") {
                    return Err(OfficeError::internal("disk full"));
                }
                officecore::io::write_atomic(path, bytes)
            });
        assert!(failed.is_err());
        // The old index is untouched and every row it lists still has its file.
        assert_eq!(std::fs::read(dir.path().join("index.json")).unwrap(), before);
        for entry in read_history_index(dir.path()) {
            assert!(dir.path().join(format!("v{}.json", entry.version)).exists(), "v{} is missing", entry.version);
        }
        // The next successful push reuses the number of the failed one and
        // prunes down to the limit.
        let retry = push(dir.path(), "text 4", 3).unwrap();
        assert_eq!(retry.version, 4);
        let versions: Vec<u32> = read_history_index(dir.path()).iter().map(|entry| entry.version).collect();
        assert_eq!(versions, vec![2, 3, 4]);
        assert!(!dir.path().join("v1.json").exists());
    }
}

/// Files passed on the command line (Windows file associations / "Open with").
/// Only existing file paths are returned; anything else is ignored.
#[tauri::command]
pub fn office_startup_files() -> Vec<crate::files::FileRef> {
    crate::launch::startup_tokens(std::env::args())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpellCheckMisspelling {
    pub word: String,
    pub start: usize,
    pub end: usize,
    pub suggestions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpellCheckResponse {
    pub misspellings: Vec<SpellCheckMisspelling>,
}

/// Offline Turkish spell checker command. Async + `spawn_blocking`: a long
/// document must never block the UI thread while the analyzer walks it.
#[tauri::command]
pub async fn turkish_spellcheck(text: String, suggestions_limit: Option<usize>) -> SpellCheckResponse {
    let limit = suggestions_limit.unwrap_or(4);
    let task = tauri::async_runtime::spawn_blocking(move || {
        turkish::check_text(&text)
            .into_iter()
            .map(|m| {
                let suggestions = if limit > 0 { turkish::suggestions(&m.word, limit) } else { Vec::new() };
                SpellCheckMisspelling { word: m.word, start: m.start, end: m.end, suggestions }
            })
            .collect::<Vec<_>>()
    });
    match task.await {
        Ok(misspellings) => SpellCheckResponse { misspellings },
        Err(_) => SpellCheckResponse { misspellings: Vec::new() },
    }
}

/// Suggestions for a single word.
#[tauri::command]
pub async fn turkish_suggestions(word: String, limit: Option<usize>) -> Vec<String> {
    let limit = limit.unwrap_or(5);
    tauri::async_runtime::spawn_blocking(move || turkish::suggestions(&word, limit)).await.unwrap_or_default()
}

#[cfg(test)]
mod scan_tests {
    use super::*;

    fn write_page(path: &Path, width: u32, height: u32) {
        let mut image = image::RgbaImage::new(width, height);
        for y in 0..height {
            for x in 0..width {
                let value = if (x / 8 + y / 8) % 2 == 0 { 40 } else { 220 };
                image.put_pixel(x, y, image::Rgba([value, value, value, 255]));
            }
        }
        image.save(path).expect("write page");
    }

    #[test]
    fn scan_corrects_corners_and_assembles_a_pdf() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("page1.png");
        let second = dir.path().join("page2.png");
        write_page(&first, 60, 80);
        write_page(&second, 60, 80);
        let output = dir.path().join("scan.pdf");

        // The first page is corrected from an offset quad, the second is used
        // as it is (no corners).
        let request = ScanRequest {
            pages: vec![
                ScanPage {
                    path: first.to_string_lossy().to_string(),
                    corners: Some(vec![[10.0, 10.0], [50.0, 10.0], [50.0, 70.0], [10.0, 70.0]]),
                },
                ScanPage { path: second.to_string_lossy().to_string(), corners: None },
            ],
            output: output.to_string_lossy().to_string(),
        };
        let result = scan_pages(request).expect("scan");
        assert_eq!(result.pages, 2);
        assert_eq!(result.corrected, 1, "one page had corners");
        assert!(result.warnings.is_empty(), "warnings: {:?}", result.warnings);
        let bytes = std::fs::read(&output).expect("pdf written");
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn scan_preview_is_a_small_jpeg_with_the_full_size() {
        let dir = tempfile::tempdir().unwrap();
        let page = dir.path().join("page.png");
        write_page(&page, 2560, 1280);
        let preview = scan_preview(&page.to_string_lossy()).expect("preview");
        assert!(preview.data_url.starts_with("data:image/jpeg;base64,"));
        assert_eq!((preview.width, preview.height), (2560, 1280));
        assert!(scan_preview(&dir.path().join("gone.jpg").to_string_lossy()).is_err());
    }

    #[test]
    fn scan_rejects_an_empty_page_list() {
        let dir = tempfile::tempdir().unwrap();
        let request = ScanRequest { pages: Vec::new(), output: dir.path().join("x.pdf").to_string_lossy().to_string() };
        assert!(scan_pages(request).is_err());
    }
}

#[cfg(test)]
mod mailmerge_tests {
    use super::*;

    #[test]
    fn mail_merge_produces_one_document_per_row() {
        let dir = tempfile::tempdir().unwrap();
        let mut template = officecore::model::TextDocument::new_blank("Mektup");
        template.blocks = vec![officecore::model::Block::Paragraph {
            props: Default::default(),
            runs: vec![officecore::model::Run { text: "Sayın {{ad}},".into(), ..Default::default() }],
        }];
        let template_path = dir.path().join("mektup.docx");
        officecore::docx::write_docx_file(&template_path, &template).unwrap();
        let data_path = dir.path().join("veri.csv");
        std::fs::write(&data_path, "ad\nAyşe\nMehmet\n").unwrap();
        let out_dir = dir.path().join("cikti");

        let report = mail_merge(MailMergeRequest {
            template: template_path.to_string_lossy().to_string(),
            data: data_path.to_string_lossy().to_string(),
            output_dir: out_dir.to_string_lossy().to_string(),
            format: "pdf".into(),
            name_field: Some("ad".into()),
        })
        .expect("merge");
        assert_eq!(report.produced.len(), 2, "errors: {:?}", report.errors);
        assert!(report.errors.is_empty());
        assert!(out_dir.join("Ayşe.pdf").exists());
        assert!(out_dir.join("Mehmet.pdf").exists());
    }

    #[test]
    fn mail_merge_reports_a_missing_field() {
        let dir = tempfile::tempdir().unwrap();
        let mut template = officecore::model::TextDocument::new_blank("Mektup");
        template.blocks = vec![officecore::model::Block::Paragraph {
            props: Default::default(),
            runs: vec![officecore::model::Run { text: "{{ad}} {{soyad}}".into(), ..Default::default() }],
        }];
        let template_path = dir.path().join("m.docx");
        officecore::docx::write_docx_file(&template_path, &template).unwrap();
        let data_path = dir.path().join("v.csv");
        std::fs::write(&data_path, "ad\nAyşe\n").unwrap();
        let report = mail_merge(MailMergeRequest {
            template: template_path.to_string_lossy().to_string(),
            data: data_path.to_string_lossy().to_string(),
            output_dir: dir.path().join("out").to_string_lossy().to_string(),
            format: "docx".into(),
            name_field: None,
        })
        .unwrap();
        assert!(report.produced.is_empty());
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].message.contains("soyad"), "{}", report.errors[0].message);
    }
}
