//! Office document commands: open/save/convert/export/clean plus the local
//! stores behind autosave, recovery, version history and the productivity
//! modules (Notes, Planner, Data, Draw).
//!
//! Everything here is local-first: no network access, no telemetry, and no
//! document content leaves the machine.

use officecore::cleaner::{self, CleanOptions, CleanResult};
use officecore::csvio::{self, CsvOptions};
use officecore::error::OfficeError;
use officecore::model::*;
use officecore::{docx, layout, odf, pptx, rtf, textio, xlsx};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize)]
pub struct OfficeErrorPayload {
    pub code: String,
    pub message: String,
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenDocument {
    pub kind: String,
    pub title: String,
    pub path: String,
    pub model: Value,
    pub warnings: Vec<String>,
    /// True when the file was a legacy binary format (`.doc`/`.ppt`). The
    /// frontend must not save back to it: the tab keeps an empty path so the
    /// first save asks for a modern destination.
    #[serde(default)]
    pub legacy: bool,
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
}

fn extension(path: &Path) -> String {
    path.extension().map(|value| value.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()
}

fn kind_for_extension(extension: &str) -> Option<&'static str> {
    match extension {
        "docx" | "docm" | "dotx" | "odt" | "rtf" | "txt" | "md" | "markdown" | "html" | "htm" | "oswk-writer" => {
            Some("writer")
        }
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
        "xlsx" | "xlsm" | "xls" | "ods" => {
            let read = xlsx::read_workbook_file(path).map_err(payload)?;
            ("calc".to_string(), read.workbook.title.clone(), to_value(read.workbook)?, read.warnings)
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
    Ok(OpenDocument { kind, title, path: path_text, model, warnings, legacy })
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
    let task = tauri::async_runtime::spawn_blocking(move || {
        let path = input_path(&path)?;
        open_path(&path)
    });
    task.await.map_err(|error| payload(OfficeError::internal(format!("worker thread failed: {error}"))))?
}

// ---------------------------------------------------------------------------
// Save
// ---------------------------------------------------------------------------

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
                    let bytes = layout::document_to_pdf(&document);
                    officecore::io::write_atomic(path, &bytes).map_err(payload)?;
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
                    let bytes = odf::write_odp(&deck).map_err(payload)?;
                    officecore::io::write_atomic(path, &bytes).map_err(payload)?;
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
    let bytes = match kind {
        "writer" => layout::document_to_pdf(&writer_from_value(model)?),
        "calc" => layout::workbook_to_pdf(&workbook_from_value(model)?, 20),
        "impress" => layout::deck_to_pdf(&deck_from_value(model)?),
        other => {
            return Err(payload(OfficeError::invalid(format!("Cannot export {other} to PDF."))));
        }
    };
    officecore::io::write_atomic(path, &bytes).map_err(payload)?;
    Ok(SaveDocument { path: path.to_string_lossy().to_string(), warnings: Vec::new() })
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

pub fn convert(input: &Path, output: &Path, _options: &ConvertOptions) -> Result<ConversionInfo, OfficeErrorPayload> {
    let input_extension = extension(input);
    let output_extension = extension(output);
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
                None,
                &silent,
                &cancel,
            )
            .map_err(|e| payload(OfficeError::internal(e.to_string())))?;
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
            let pages = pdfcore::render::page_geometries(input, None)
                .map_err(|e| payload(OfficeError::internal(e.to_string())))?;
            let mut text = String::new();
            for page in &pages {
                let content = pdfcore::render::extract_page_text(input, None, page.page)
                    .map_err(|e| payload(OfficeError::internal(e.to_string())))?;
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
            let pages = pdfcore::render::page_geometries(input, None)
                .map_err(|e| payload(OfficeError::internal(e.to_string())))?;
            let mut document =
                officecore::model::TextDocument { title: officecore::io::file_stem(input), ..Default::default() };
            for page in &pages {
                let content = pdfcore::render::extract_page_text(input, None, page.page)
                    .map_err(|e| payload(OfficeError::internal(e.to_string())))?;
                for line in content.lines() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        document.blocks.push(officecore::model::Block::paragraph(trimmed));
                    }
                }
            }
            if document.blocks.is_empty() {
                document.blocks.push(officecore::model::Block::paragraph(""));
            }
            officecore::docx::write_docx_file(output, &document).map_err(payload)?;
            return Ok(ConversionInfo {
                input: input.to_string_lossy().to_string(),
                output: output.to_string_lossy().to_string(),
                converted: true,
                warnings: vec!["Text content extracted from PDF into Word DOCX document.".into()],
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

#[tauri::command]
pub fn history_push(
    app: AppHandle,
    document_id: String,
    kind: String,
    title: String,
    model: Value,
) -> Result<HistoryEntry, OfficeErrorPayload> {
    let dir = history_dir(&app, &document_id)?;
    std::fs::create_dir_all(&dir).map_err(|error| payload(OfficeError::from_io(error, &dir)))?;
    let mut index = read_history_index(&dir);
    let version = index.iter().map(|entry| entry.version).max().unwrap_or(0) + 1;
    let payload_json = serde_json::json!({
        "version": version,
        "savedAt": timestamp(),
        "kind": kind,
        "title": title,
        "model": model,
    });
    let bytes = serde_json::to_vec(&payload_json)
        .map_err(|error| payload(OfficeError::internal(format!("Could not encode the version: {error}"))))?;
    let file = dir.join(format!("v{version}.json"));
    officecore::io::write_atomic(&file, &bytes).map_err(payload)?;
    let entry = HistoryEntry { version, saved_at: timestamp(), title, kind, size: bytes.len() as u64 };
    index.push(entry.clone());
    // Keep the newest 25 versions.
    index.sort_by_key(|entry| entry.version);
    while index.len() > 25 {
        let removed = index.remove(0);
        let _ = std::fs::remove_file(dir.join(format!("v{}.json", removed.version)));
    }
    let index_bytes = serde_json::to_vec(&index)
        .map_err(|error| payload(OfficeError::internal(format!("Could not encode the history index: {error}"))))?;
    officecore::io::write_atomic(&dir.join("index.json"), &index_bytes).map_err(payload)?;
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
    let path = recovery_dir(&app)?.join(format!("{id}.json"));
    let bytes = officecore::io::read_bytes(&path).map_err(payload)?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|error| payload(OfficeError::corrupt(format!("The recovery snapshot is damaged: {error}"))))?;
    Ok(value.get("model").cloned().unwrap_or(Value::Null))
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
    fn extension_detection() {
        assert_eq!(kind_for_extension("docx"), Some("writer"));
        assert_eq!(kind_for_extension("doc"), None);
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
    }

    #[test]
    fn timestamp_is_iso_like() {
        let value = timestamp();
        assert_eq!(value.len(), 20);
        assert!(value.ends_with('Z'));
    }
}

/// Files passed on the command line (Windows file associations / "Open with").
/// Only existing file paths are returned; anything else is ignored.
#[tauri::command]
pub fn office_startup_files() -> Vec<String> {
    std::env::args()
        .skip(1)
        .filter(|argument| !argument.starts_with('-'))
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_file())
        .map(|path| path.to_string_lossy().to_string())
        .collect()
}
