//! Local document vault: an opt-in, fully local index of user-chosen folders
//! with full-text search and preview locations.
//!
//! Design rules:
//! * Only folders the user saved in `vault-config.json` are ever walked.
//!   A folder that is not part of the saved configuration is never scanned.
//! * Nothing leaves the machine: no network access, no telemetry, and file
//!   contents never appear in warnings, errors or logs.
//! * `vault/index.json` is written through `officecore::io::write_atomic`
//!   (temp sibling + rename), so a crash mid-write keeps the previous index.
//!   A damaged index is reported as a warning and reset to empty instead of
//!   aborting the app.
//! * Extracted text is cached under `vault/docs/<id>.txt` for previews; the
//!   index itself holds bounded excerpts plus per-location excerpts.
//!
//! `TextLocation.label` semantics:
//! * writer formats (docx/docm/dotx/odt/rtf/txt/md/html): `paragraph N`,
//!   `table N`, `figure N`, `block N` (1-based ordinals in the body);
//! * spreadsheets (xlsx/xlsm/xls/ods/csv/tsv): `Sheet1!A1:B7`, the first and
//!   last non-empty cell of the sheet;
//! * presentations (pptx/pptm/odp): `slide N` (1-based);
//! * PDFs: `page N` (1-based);
//! * the whole-document excerpt is always searchable as label `document`.

use officecore::error::OfficeError;
use officecore::model::{Block, CellValue, TextDocument, Workbook};
use officecore::{csvio, docx, odf, pptx, rtf, textio, xlsx};
use pdfcore::error::PdfError;
use pdfcore::progress::{CancelToken, ProgressEvent};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager, State};

use crate::jobs::{emit_progress, JobRegistry};

// ---------------------------------------------------------------------------
// Limits
// ---------------------------------------------------------------------------

const VAULT_DIR: &str = "vault";
const INDEX_FILE: &str = "index.json";
const CONFIG_FILE: &str = "vault-config.json";
const STATUS_FILE: &str = "status.json";
const DOCS_DIR: &str = "docs";

const INDEX_VERSION: u32 = 1;
const DEFAULT_MAX_FILE_MB: u64 = 25;
const DEFAULT_MAX_FILES: usize = 20_000;
const MAX_DOCUMENTS: usize = 20_000;
/// Full extracted text kept per document (cache + document text cap).
const MAX_EXTRACTED_CHARS: usize = 2_000_000;
/// Excerpt of the extracted text stored in the index itself.
const EXCERPT_CHARS: usize = 200_000;
/// Excerpt of each location stored in the index.
const LOCATION_EXCERPT_CHARS: usize = 2_000;
const MAX_LOCATIONS: usize = 400;
const MAX_HEADINGS: usize = 200;
const MAX_HEADING_CHARS: usize = 300;
const MAX_WARNINGS: usize = 200;
const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 200;
const SNIPPET_CHARS: usize = 160;
const FUZZY_MIN_WORD: usize = 4;
const FUZZY_MAX_WORD: usize = 64;

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct VaultConfig {
    /// Folders the user explicitly selected for indexing.
    pub folders: Vec<String>,
    pub include_pdf: bool,
    pub include_office: bool,
    pub max_file_mb: u64,
    pub updated_at: String,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            folders: Vec::new(),
            include_pdf: true,
            include_office: true,
            max_file_mb: DEFAULT_MAX_FILE_MB,
            updated_at: String::new(),
        }
    }
}

/// One labelled chunk of a document: "page 3", "paragraph 12", "Sheet1!A1",
/// "slide 4" ... `text` is a bounded excerpt, the full text lives in the cache.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct TextLocation {
    pub label: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct VaultDocument {
    /// Stable hash of the normalized path; also the cache file name.
    pub id: String,
    pub path: String,
    pub file_name: String,
    pub extension: String,
    pub size: u64,
    /// ISO-8601 UTC string read from the file's modification time.
    pub modified: String,
    pub title: String,
    pub headings: Vec<String>,
    /// Bounded excerpt of the extracted text (first `EXCERPT_CHARS`).
    pub text: String,
    pub locations: Vec<TextLocation>,
    /// Reserved for user tags; the indexer never fills it in.
    pub tags: Vec<String>,
    pub indexed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct VaultIndex {
    pub version: u32,
    pub documents: Vec<VaultDocument>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct VaultStatus {
    pub indexed: usize,
    pub folders: usize,
    pub index_bytes: u64,
    pub last_scan: Option<String>,
    pub scanning: bool,
    /// Recovery/cap notes from the last load or scan (never file contents).
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct VaultScanRequest {
    /// When present these user-selected folders are added to the saved config
    /// before scanning; entries that are not in the saved config are never
    /// walked.
    pub folders: Option<Vec<String>>,
    /// Re-extract every file, even unchanged ones (full rescan).
    pub rescan: bool,
    /// Upper bound for discovered files; 0 uses the default cap.
    pub max_files: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct VaultSearchRequest {
    pub query: String,
    pub exact: bool,
    pub fuzzy: bool,
    pub phrase: bool,
    pub extensions: Vec<String>,
    pub modified_after: Option<String>,
    pub modified_before: Option<String>,
    pub folder: Option<String>,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultSearchHit {
    pub document_id: String,
    pub path: String,
    pub file_name: String,
    pub extension: String,
    pub size: u64,
    pub modified: String,
    pub score: f64,
    pub match_label: String,
    pub snippet: String,
    pub matched_terms: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultSearchResponse {
    pub hits: Vec<VaultSearchHit>,
    pub total: usize,
    pub took_ms: u64,
    /// True when there is no usable index yet; the UI should offer a scan.
    pub index_missing: bool,
}

/// Result of a pure folder scan. `index` is written to disk by `scan_folders`
/// unless the scan was cancelled (a partial index is never persisted).
#[derive(Debug, Clone)]
pub struct ScanOutcome {
    pub index: VaultIndex,
    pub warnings: Vec<String>,
    /// Supported files discovered in the selected folders.
    pub scanned: usize,
    /// Documents extracted (or re-extracted) in this scan.
    pub indexed: usize,
    /// Unchanged documents kept from the previous index.
    pub skipped: usize,
    /// Index entries removed (file gone / no longer supported).
    pub removed: usize,
    pub cancelled: bool,
    /// The walk hit the file cap; the index may be incomplete.
    pub truncated: bool,
}

/// Small status file next to the index (`vault/status.json`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct StoredStatus {
    scanning: bool,
    last_scan: Option<String>,
}

// ---------------------------------------------------------------------------
// Storage paths and tolerant JSON helpers
// ---------------------------------------------------------------------------

fn config_path(root: &Path) -> PathBuf {
    root.join(CONFIG_FILE)
}

fn index_path(root: &Path) -> PathBuf {
    root.join(VAULT_DIR).join(INDEX_FILE)
}

fn docs_dir(root: &Path) -> PathBuf {
    root.join(VAULT_DIR).join(DOCS_DIR)
}

fn stored_status_path(root: &Path) -> PathBuf {
    root.join(VAULT_DIR).join(STATUS_FILE)
}

fn office_to_pdf(error: OfficeError) -> PdfError {
    match error.code.as_str() {
        "not_found" => PdfError::NotFound(error.message),
        "permission_denied" => PdfError::PermissionDenied(error.message),
        "cancelled" => PdfError::Cancelled,
        "unsupported_format" => PdfError::Unsupported(error.message),
        "too_large" | "corrupt_document" | "encoding_error" | "zip_bomb" => PdfError::InvalidInput(error.message),
        _ => PdfError::Internal(error.message),
    }
}

fn write_json_atomic(path: &Path, value: &impl Serialize) -> Result<u64, PdfError> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| PdfError::Internal(format!("store serialize failed: {error}")))?;
    officecore::io::write_atomic(path, &bytes).map_err(office_to_pdf)?;
    Ok(bytes.len() as u64)
}

/// Loads the vault configuration; a missing or damaged file falls back to the
/// defaults (never fails).
pub fn load_config(root: &Path) -> VaultConfig {
    let path = config_path(root);
    if !path.exists() {
        return VaultConfig::default();
    }
    std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<VaultConfig>(&bytes).ok())
        .unwrap_or_default()
}

/// Saves the configuration and stamps `updated_at`.
pub fn save_config(root: &Path, config: &VaultConfig) -> Result<VaultConfig, PdfError> {
    let mut stored = config.clone();
    stored.updated_at = timestamp();
    write_json_atomic(&config_path(root), &stored)?;
    Ok(stored)
}

/// Loads the index. Returns `(empty index, warning)` when the file is damaged
/// instead of failing, so the app can recover by rescanning.
pub fn load_index(root: &Path) -> (VaultIndex, Vec<String>) {
    let path = index_path(root);
    if !path.exists() {
        return (VaultIndex { version: INDEX_VERSION, ..Default::default() }, Vec::new());
    }
    match std::fs::read(&path) {
        Ok(bytes) => match serde_json::from_slice::<VaultIndex>(&bytes) {
            Ok(mut index) => {
                if index.version == 0 {
                    index.version = INDEX_VERSION;
                }
                (index, Vec::new())
            }
            Err(_) => (
                VaultIndex { version: INDEX_VERSION, ..Default::default() },
                vec!["The document index is damaged and was reset. Run a scan to rebuild it.".into()],
            ),
        },
        Err(_) => (
            VaultIndex { version: INDEX_VERSION, ..Default::default() },
            vec!["The document index could not be read and was reset. Run a scan to rebuild it.".into()],
        ),
    }
}

/// Writes the index atomically and returns the serialized size in bytes.
pub fn save_index(root: &Path, index: &VaultIndex) -> Result<u64, PdfError> {
    write_json_atomic(&index_path(root), index)
}

fn read_stored_status(root: &Path) -> StoredStatus {
    let path = stored_status_path(root);
    if !path.exists() {
        return StoredStatus::default();
    }
    std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<StoredStatus>(&bytes).ok())
        .unwrap_or_default()
}

fn write_stored_status(root: &Path, status: &StoredStatus) -> Result<(), PdfError> {
    write_json_atomic(&stored_status_path(root), status).map(|_| ())
}

fn sanitize_id(id: &str) -> Result<String, PdfError> {
    let cleaned: String = id.chars().take(32).collect();
    if cleaned.is_empty() || cleaned.len() != id.len() || !cleaned.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(PdfError::InvalidInput("invalid vault document id".into()));
    }
    Ok(cleaned.to_ascii_lowercase())
}

/// Cache path of a document's full extracted text.
pub fn document_text_path(root: &Path, id: &str) -> Result<PathBuf, PdfError> {
    let id = sanitize_id(id)?;
    Ok(docs_dir(root).join(format!("{id}.txt")))
}

/// Reads the cached full extracted text of one document.
pub fn read_document_text(root: &Path, id: &str) -> Result<String, PdfError> {
    let path = document_text_path(root, id)?;
    std::fs::read_to_string(&path).map_err(PdfError::from_io)
}

/// Current vault status (index size, folder count, scan markers).
pub fn vault_status_at(root: &Path) -> VaultStatus {
    let (index, warnings) = load_index(root);
    let stored = read_stored_status(root);
    let index_bytes = std::fs::metadata(index_path(root)).map(|metadata| metadata.len()).unwrap_or(0);
    VaultStatus {
        indexed: index.documents.len(),
        folders: load_config(root).folders.len(),
        index_bytes,
        last_scan: stored.last_scan,
        scanning: stored.scanning,
        warnings,
    }
}

/// Deletes the index and the extraction cache (the folder selection stays).
pub fn clear_vault_at(root: &Path) -> Result<(), PdfError> {
    let index = index_path(root);
    if index.exists() {
        std::fs::remove_file(&index).map_err(PdfError::from_io)?;
    }
    let docs = docs_dir(root);
    if docs.exists() {
        std::fs::remove_dir_all(&docs).map_err(PdfError::from_io)?;
    }
    let status = stored_status_path(root);
    if status.exists() {
        std::fs::remove_file(&status).map_err(PdfError::from_io)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Time helpers
// ---------------------------------------------------------------------------

fn timestamp() -> String {
    iso_from_system_time(SystemTime::now())
}

fn iso_from_system_time(time: SystemTime) -> String {
    let seconds = time.duration_since(UNIX_EPOCH).map(|value| value.as_secs()).unwrap_or(0);
    let days = (seconds / 86_400) as i64;
    let time = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", time / 3600, (time % 3600) / 60, time % 60)
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

// ---------------------------------------------------------------------------
// Scanning
// ---------------------------------------------------------------------------

/// Capped warning list; file contents are never put into a message.
struct WarningSink {
    items: Vec<String>,
    omitted: usize,
}

impl WarningSink {
    fn new(items: Vec<String>) -> Self {
        Self { items, omitted: 0 }
    }

    fn push(&mut self, message: String) {
        if self.items.len() < MAX_WARNINGS {
            self.items.push(message);
        } else {
            self.omitted += 1;
        }
    }

    fn finish(mut self) -> Vec<String> {
        if self.omitted > 0 {
            self.items.push(format!("{} additional warnings were omitted.", self.omitted));
        }
        self.items
    }
}

struct ExtractedDocument {
    title: String,
    headings: Vec<String>,
    text: String,
    locations: Vec<TextLocation>,
}

fn metadata_only(path: &Path) -> ExtractedDocument {
    ExtractedDocument {
        title: officecore::io::file_stem(path),
        headings: Vec::new(),
        text: String::new(),
        locations: Vec::new(),
    }
}

fn truncate_chars(text: &str, max: usize) -> (String, bool) {
    let mut chars = text.chars();
    let mut out = String::new();
    for _ in 0..max {
        match chars.next() {
            Some(ch) => out.push(ch),
            None => return (out, false),
        }
    }
    let truncated = chars.next().is_some();
    (out, truncated)
}

fn push_location(locations: &mut Vec<TextLocation>, label: String, text: &str, path: &Path, warnings: &mut WarningSink) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    if locations.len() >= MAX_LOCATIONS {
        if locations.len() == MAX_LOCATIONS {
            warnings.push(format!("{}: only the first {MAX_LOCATIONS} locations are indexed.", path.display()));
        }
        return;
    }
    let (excerpt, _) = truncate_chars(trimmed, LOCATION_EXCERPT_CHARS);
    locations.push(TextLocation { label, text: excerpt });
}

fn report_reader_warnings(read_warnings: &[String], path: &Path, warnings: &mut WarningSink) {
    for message in read_warnings.iter().take(5) {
        warnings.push(format!("{}: {message}", path.display()));
    }
}

fn writer_extract(document: &TextDocument, read_warnings: &[String], path: &Path, warnings: &mut WarningSink) -> ExtractedDocument {
    report_reader_warnings(read_warnings, path, warnings);
    let mut locations = Vec::new();
    let mut headings = Vec::new();
    let mut paragraphs = 0usize;
    let mut tables = 0usize;
    let mut figures = 0usize;
    let mut blocks = 0usize;
    for block in &document.blocks {
        blocks += 1;
        let text = block.plain_text();
        match block {
            Block::Paragraph { props, .. } => {
                paragraphs += 1;
                if props.style.starts_with("Heading") {
                    let heading = text.trim();
                    if !heading.is_empty() && headings.len() < MAX_HEADINGS {
                        headings.push(truncate_chars(heading, MAX_HEADING_CHARS).0);
                    }
                }
                push_location(&mut locations, format!("paragraph {paragraphs}"), &text, path, warnings);
            }
            Block::Table { .. } => {
                tables += 1;
                push_location(&mut locations, format!("table {tables}"), &text, path, warnings);
            }
            Block::Image { .. } => {
                figures += 1;
                push_location(&mut locations, format!("figure {figures}"), &text, path, warnings);
            }
            _ => {
                push_location(&mut locations, format!("block {blocks}"), &text, path, warnings);
            }
        }
    }
    ExtractedDocument {
        title: document.title.clone(),
        headings,
        text: document.plain_text(),
        locations,
    }
}

fn cell_value_text(value: &CellValue) -> Option<String> {
    match value {
        CellValue::Empty => None,
        CellValue::Text(text) => Some(text.clone()),
        CellValue::Number(number) => Some(if number.fract() == 0.0 && number.is_finite() && number.abs() < 1e15 {
            format!("{}", *number as i64)
        } else {
            number.to_string()
        }),
        CellValue::Bool(value) => Some(if *value { "TRUE".into() } else { "FALSE".into() }),
        CellValue::Error(error) => Some(error.clone()),
    }
}

fn workbook_extract(workbook: &Workbook, read_warnings: &[String], path: &Path, warnings: &mut WarningSink) -> ExtractedDocument {
    report_reader_warnings(read_warnings, path, warnings);
    let mut locations = Vec::new();
    let mut all_text = String::new();
    for sheet in &workbook.sheets {
        let mut lines = Vec::new();
        let mut min_row = u32::MAX;
        let mut max_row = 0u32;
        let mut min_col = u32::MAX;
        let mut max_col = 0u32;
        for (address, cell) in &sheet.cells {
            let Some(value) = cell_value_text(&cell.value) else { continue };
            if value.trim().is_empty() {
                continue;
            }
            if let Some((row, column)) = officecore::address::parse(address) {
                min_row = min_row.min(row);
                max_row = max_row.max(row);
                min_col = min_col.min(column);
                max_col = max_col.max(column);
            }
            lines.push(format!("{address}: {value}"));
        }
        if lines.is_empty() {
            continue;
        }
        let label = if min_row == u32::MAX {
            sheet.name.clone()
        } else {
            let first = officecore::address::format(min_row, min_col);
            let last = officecore::address::format(max_row, max_col);
            if first == last {
                format!("{}!{}", sheet.name, first)
            } else {
                format!("{}!{}:{}", sheet.name, first, last)
            }
        };
        let text = lines.join("\n");
        all_text.push_str(&label);
        all_text.push('\n');
        all_text.push_str(&text);
        all_text.push('\n');
        push_location(&mut locations, label, &text, path, warnings);
    }
    ExtractedDocument {
        title: workbook.title.clone(),
        headings: Vec::new(),
        text: all_text,
        locations,
    }
}

fn deck_extract(deck: &officecore::model::Deck, read_warnings: &[String], path: &Path, warnings: &mut WarningSink) -> ExtractedDocument {
    report_reader_warnings(read_warnings, path, warnings);
    let mut locations = Vec::new();
    let mut all_text = String::new();
    for (index, slide) in deck.slides.iter().enumerate() {
        let mut parts = Vec::new();
        for object in &slide.objects {
            if let Some(frame) = &object.text {
                let text = frame.plain();
                if !text.trim().is_empty() {
                    parts.push(text);
                }
            }
            if let Some(table) = &object.table {
                let text = Block::Table { table: table.clone() }.plain_text();
                if !text.trim().is_empty() {
                    parts.push(text);
                }
            }
        }
        if !slide.notes.trim().is_empty() {
            parts.push(slide.notes.clone());
        }
        if parts.is_empty() {
            continue;
        }
        let label = format!("slide {}", index + 1);
        let text = parts.join("\n");
        all_text.push_str(&label);
        all_text.push('\n');
        all_text.push_str(&text);
        all_text.push('\n');
        push_location(&mut locations, label, &text, path, warnings);
    }
    ExtractedDocument {
        title: deck.title.clone(),
        headings: Vec::new(),
        text: all_text,
        locations,
    }
}

/// Best-effort HTML to text: drops `<...>` tags. Script/style bodies stay (the
/// index is about searchable words, not rendering).
fn strip_html_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_tag = false;
    for ch in text.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            other if !in_tag => out.push(other),
            _ => {}
        }
    }
    out
}

fn pdf_extract(path: &Path, warnings: &mut WarningSink) -> ExtractedDocument {
    let mut extracted = metadata_only(path);
    if !pdfcore::render::is_available() {
        warnings.push(format!("{}: the PDF engine is not available; the file is indexed without a text layer.", path.display()));
        return extracted;
    }
    let pdfium = match pdfcore::render::pdfium_instance() {
        Ok(pdfium) => pdfium,
        Err(error) => {
            warnings.push(format!("{}: the PDF engine could not be loaded ({error}).", path.display()));
            return extracted;
        }
    };
    let document = match pdfium.load_pdf_from_file(path, None) {
        Ok(document) => document,
        Err(error) => {
            warnings.push(format!("{}: the PDF could not be opened ({error}).", path.display()));
            return extracted;
        }
    };
    let mut text = String::new();
    let mut text_chars = 0usize;
    let mut truncated = false;
    for (index, page) in document.pages().iter().enumerate() {
        let page_text = match page.text() {
            Ok(page_text) => page_text.all(),
            Err(error) => {
                warnings.push(format!("{}: page {} could not be read ({error}).", path.display(), index + 1));
                String::new()
            }
        };
        if page_text.trim().is_empty() {
            continue;
        }
        push_location(&mut extracted.locations, format!("page {}", index + 1), &page_text, path, warnings);
        if truncated {
            continue;
        }
        let page_chars = page_text.chars().count();
        if text_chars + page_chars > MAX_EXTRACTED_CHARS {
            let (head, _) = truncate_chars(&page_text, MAX_EXTRACTED_CHARS.saturating_sub(text_chars));
            text.push_str(&head);
            truncated = true;
            warnings.push(format!("{}: extracted text was truncated after page {}.", path.display(), index + 1));
        } else {
            text.push_str(&page_text);
            text.push('\n');
            text_chars += page_chars + 1;
        }
    }
    extracted.text = text;
    extracted
}

fn extract_path(path: &Path, extension: &str, warnings: &mut WarningSink) -> ExtractedDocument {
    match extension {
        "docx" | "docm" | "dotx" => match docx::read_docx_file(path) {
            Ok(read) => writer_extract(&read.document, &read.warnings, path, warnings),
            Err(error) => {
                warnings.push(format!("{}: the document could not be read ({}).", path.display(), error.message));
                metadata_only(path)
            }
        },
        "odt" => match odf::read_odt_file(path) {
            Ok(read) => writer_extract(&read.document, &read.warnings, path, warnings),
            Err(error) => {
                warnings.push(format!("{}: the document could not be read ({}).", path.display(), error.message));
                metadata_only(path)
            }
        },
        "rtf" => match rtf::read_rtf_file(path) {
            Ok(read) => writer_extract(&read.document, &read.warnings, path, warnings),
            Err(error) => {
                warnings.push(format!("{}: the document could not be read ({}).", path.display(), error.message));
                metadata_only(path)
            }
        },
        "txt" | "md" | "markdown" | "html" | "htm" => {
            let bytes = match officecore::io::read_bytes(path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    warnings.push(format!("{}: the file could not be read ({}).", path.display(), error.message));
                    return metadata_only(path);
                }
            };
            let decoded = match officecore::zip::decode_utf8(&bytes, "text") {
                Ok(decoded) => decoded,
                Err(error) => {
                    warnings.push(format!("{}: the file could not be decoded ({}).", path.display(), error.message));
                    return metadata_only(path);
                }
            };
            let text = if extension == "html" || extension == "htm" {
                strip_html_tags(&decoded)
            } else {
                decoded
            };
            let title = officecore::io::file_stem(path);
            let document = textio::text_to_document(&text, &title);
            writer_extract(&document, &[], path, warnings)
        }
        "xlsx" | "xlsm" | "xls" => match xlsx::read_workbook_file(path) {
            Ok(read) => workbook_extract(&read.workbook, &read.warnings, path, warnings),
            Err(error) => {
                warnings.push(format!("{}: the spreadsheet could not be read ({}).", path.display(), error.message));
                metadata_only(path)
            }
        },
        "ods" => match odf::read_ods_file(path) {
            Ok(read) => workbook_extract(&read.workbook, &read.warnings, path, warnings),
            Err(error) => {
                warnings.push(format!("{}: the spreadsheet could not be read ({}).", path.display(), error.message));
                metadata_only(path)
            }
        },
        "csv" | "tsv" => {
            let bytes = match officecore::io::read_bytes(path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    warnings.push(format!("{}: the file could not be read ({}).", path.display(), error.message));
                    return metadata_only(path);
                }
            };
            let mut options = csvio::CsvOptions::default();
            if extension == "tsv" {
                options.delimiter = "tab".into();
            }
            match csvio::parse_csv(&bytes, &options) {
                Ok(read) => {
                    let mut workbook = read.workbook;
                    workbook.title = officecore::io::file_stem(path);
                    workbook_extract(&workbook, &read.warnings, path, warnings)
                }
                Err(error) => {
                    warnings.push(format!("{}: the file could not be parsed ({}).", path.display(), error.message));
                    metadata_only(path)
                }
            }
        }
        "pptx" | "pptm" => match pptx::read_pptx_file(path) {
            Ok(read) => deck_extract(&read.deck, &read.warnings, path, warnings),
            Err(error) => {
                warnings.push(format!("{}: the presentation could not be read ({}).", path.display(), error.message));
                metadata_only(path)
            }
        },
        "odp" => match odf::read_odp_file(path) {
            Ok(read) => deck_extract(&read.deck, &read.warnings, path, warnings),
            Err(error) => {
                warnings.push(format!("{}: the presentation could not be read ({}).", path.display(), error.message));
                metadata_only(path)
            }
        },
        "pdf" => pdf_extract(path, warnings),
        _ => metadata_only(path),
    }
}

/// True when the extension is enabled by the configuration.
fn is_supported(extension: &str, config: &VaultConfig) -> bool {
    match extension {
        "pdf" => config.include_pdf,
        "docx" | "docm" | "dotx" | "odt" | "rtf" | "txt" | "md" | "markdown" | "html" | "htm" | "xlsx" | "xlsm" | "xls"
        | "ods" | "csv" | "tsv" | "pptx" | "pptm" | "odp" => config.include_office,
        _ => false,
    }
}

/// Normalized path key: forward slashes, no trailing slash, lowercased on
/// Windows (case-insensitive file systems). Used for incremental matching,
/// folder prefix checks and document ids.
fn normalize_path_key(path: &str) -> String {
    let mut value = path.replace('\\', "/");
    while value.ends_with('/') && value.len() > 1 {
        value.pop();
    }
    if cfg!(windows) {
        value = value.to_lowercase();
    }
    value
}

/// Stable document id: FNV-1a over the normalized path rendered as hex. Safe
/// as a cache file name and stable across rescans.
fn document_id(path: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in normalize_path_key(path).as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn path_is_under(path_key: &str, folder_key: &str) -> bool {
    path_key == folder_key || path_key.starts_with(&format!("{folder_key}/"))
}

/// Scans the configured folders and maintains the index under `root`.
///
/// Pure with respect to Tauri: the caller provides the storage root, the
/// configuration and the cancellation token, so the whole scan is unit
/// testable. `request.folders` never adds discovery roots - only folders
/// already saved in the configuration are walked.
pub fn scan_folders(root: &Path, config: &VaultConfig, request: &VaultScanRequest, cancel: &CancelToken) -> ScanOutcome {
    let (mut index, load_warnings) = load_index(root);
    let mut warnings = WarningSink::new(load_warnings);
    let previous: HashMap<String, VaultDocument> = index
        .documents
        .drain(..)
        .map(|document| (normalize_path_key(&document.path), document))
        .collect();

    let mut roots: Vec<(String, PathBuf)> = Vec::new();
    for folder in &config.folders {
        let key = normalize_path_key(folder);
        if key.is_empty() || roots.iter().any(|(existing, _)| *existing == key) {
            continue;
        }
        let path = PathBuf::from(folder);
        if !path.is_dir() {
            warnings.push(format!("Skipped a configured folder that no longer exists: {folder}"));
            continue;
        }
        roots.push((key, path));
    }
    if let Some(requested) = &request.folders {
        for folder in requested {
            let key = normalize_path_key(folder);
            if !roots.iter().any(|(existing, _)| *existing == key) {
                warnings.push(format!("Not part of the saved vault configuration; not scanned: {folder}"));
            }
        }
    }

    let max_files = if request.max_files == 0 { DEFAULT_MAX_FILES } else { request.max_files.min(MAX_DOCUMENTS) };
    let max_file_bytes = if config.max_file_mb == 0 {
        DEFAULT_MAX_FILE_MB * 1024 * 1024
    } else {
        config.max_file_mb.saturating_mul(1024 * 1024)
    };

    // 1) Discover supported files (hidden entries and symlinks are skipped so
    //    the walk cannot leave the selected folders).
    let mut files: Vec<PathBuf> = Vec::new();
    let mut found_keys: HashSet<String> = HashSet::new();
    let mut truncated = false;
    let mut cancelled = false;
    'walk: for (_, folder) in &roots {
        if cancel.is_cancelled() {
            cancelled = true;
            break;
        }
        let mut stack = vec![folder.clone()];
        while let Some(dir) = stack.pop() {
            if cancel.is_cancelled() {
                cancelled = true;
                break 'walk;
            }
            let entries = match std::fs::read_dir(&dir) {
                Ok(entries) => entries,
                Err(error) => {
                    warnings.push(format!("Skipped an unreadable folder {}: {error}", dir.display()));
                    continue;
                }
            };
            for entry in entries {
                if cancel.is_cancelled() {
                    cancelled = true;
                    break 'walk;
                }
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(error) => {
                        warnings.push(format!("Skipped an unreadable entry in {}: {error}", dir.display()));
                        continue;
                    }
                };
                let file_type = match entry.file_type() {
                    Ok(file_type) => file_type,
                    Err(error) => {
                        warnings.push(format!("Skipped {}: {error}", entry.path().display()));
                        continue;
                    }
                };
                if file_type.is_symlink() {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                let path = entry.path();
                if file_type.is_dir() {
                    stack.push(path);
                    continue;
                }
                if !file_type.is_file() {
                    continue;
                }
                let extension = officecore::io::extension_of(&path);
                if !is_supported(&extension, config) {
                    continue;
                }
                if files.len() >= max_files {
                    truncated = true;
                    warnings.push(format!("Stopped after {max_files} files; run the scan again to continue."));
                    break 'walk;
                }
                found_keys.insert(normalize_path_key(&path.to_string_lossy()));
                files.push(path);
            }
        }
    }

    // 2) Extract (or keep) each file.
    let mut documents: Vec<VaultDocument> = Vec::new();
    let mut current_keys: HashSet<String> = HashSet::new();
    let mut indexed = 0usize;
    let mut skipped = 0usize;
    if !cancelled {
        for path in &files {
            if cancel.is_cancelled() {
                cancelled = true;
                break;
            }
            let path_text = path.to_string_lossy().to_string();
            let key = normalize_path_key(&path_text);
            let metadata = match std::fs::metadata(path) {
                Ok(metadata) => metadata,
                Err(error) => {
                    warnings.push(format!("Skipped {}: {error}", path.display()));
                    continue;
                }
            };
            let size = metadata.len();
            if size > max_file_bytes {
                warnings.push(format!("Skipped {}: larger than the {} MB limit.", path.display(), config.max_file_mb));
                continue;
            }
            let modified = metadata.modified().map(iso_from_system_time).unwrap_or_default();
            if !request.rescan && !modified.is_empty() {
                if let Some(previous_document) = previous.get(&key) {
                    if previous_document.size == size && previous_document.modified == modified {
                        documents.push(previous_document.clone());
                        current_keys.insert(key);
                        skipped += 1;
                        continue;
                    }
                }
            }
            let extension = officecore::io::extension_of(path);
            let mut extracted = extract_path(path, &extension, &mut warnings);
            if extracted.title.trim().is_empty() {
                extracted.title = officecore::io::file_stem(path);
            }
            let mut full_text = extracted.text;
            let (bounded, was_truncated) = truncate_chars(&full_text, MAX_EXTRACTED_CHARS);
            if was_truncated {
                warnings.push(format!("{}: extracted text was truncated to {MAX_EXTRACTED_CHARS} characters.", path.display()));
            }
            full_text = bounded;
            let id = document_id(&path_text);
            let cache_path = docs_dir(root).join(format!("{id}.txt"));
            if let Err(error) = officecore::io::write_atomic(&cache_path, full_text.as_bytes()) {
                warnings.push(format!("Could not cache the extracted text of {}: {error}", path.display()));
            }
            let (excerpt, _) = truncate_chars(&full_text, EXCERPT_CHARS);
            documents.push(VaultDocument {
                id,
                path: path_text,
                file_name: path.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default(),
                extension,
                size,
                modified,
                title: extracted.title.trim().to_string(),
                headings: extracted.headings,
                text: excerpt,
                locations: extracted.locations,
                tags: Vec::new(),
                indexed_at: timestamp(),
            });
            current_keys.insert(key);
            indexed += 1;
        }
    }

    // 3) Drop entries whose file is gone, lost its support, or is outside the
    //    configured folders. A cancelled or truncated walk never purges.
    let mut removed = 0usize;
    if !cancelled {
        for (key, document) in previous {
            if current_keys.contains(&key) {
                continue;
            }
            let under_root = roots.iter().any(|(folder_key, _)| path_is_under(&key, folder_key));
            let supported_now = is_supported(&document.extension, config);
            let purge = if !under_root {
                true
            } else if !supported_now {
                true
            } else if request.rescan {
                !truncated && !found_keys.contains(&key)
            } else {
                !Path::new(&document.path).exists()
            };
            if purge {
                let _ = std::fs::remove_file(docs_dir(root).join(format!("{}.txt", document.id)));
                removed += 1;
            } else {
                documents.push(document);
            }
        }
    }

    let mut documents_truncated = false;
    if documents.len() > MAX_DOCUMENTS {
        documents_truncated = true;
        documents.truncate(MAX_DOCUMENTS);
    }
    documents.sort_by(|a, b| a.path.cmp(&b.path));
    index.version = INDEX_VERSION;
    index.documents = documents;
    index.updated_at = timestamp();

    if !cancelled {
        if let Err(error) = save_index(root, &index) {
            warnings.push(format!("The index could not be saved: {error}"));
        }
        if documents_truncated {
            warnings.push(format!("The index holds at most {MAX_DOCUMENTS} documents; the rest were left out."));
        }
    } else {
        warnings.push("The scan was cancelled; the previous index was left unchanged.".into());
    }

    ScanOutcome {
        index,
        warnings: warnings.finish(),
        scanned: files.len(),
        indexed,
        skipped,
        removed,
        cancelled,
        truncated,
    }
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

fn fold(text: &str) -> Vec<char> {
    // 1:1 char mapping (first lowercase char) so snippet offsets stay aligned.
    text.chars().map(|ch| ch.to_lowercase().next().unwrap_or(ch)).collect()
}

fn find_subsequence(hay: &[char], needle: &[char]) -> Option<usize> {
    if needle.is_empty() || needle.len() > hay.len() {
        return None;
    }
    (0..=hay.len() - needle.len()).find(|&index| &hay[index..index + needle.len()] == needle)
}

fn count_subsequence(hay: &[char], needle: &[char]) -> usize {
    if needle.is_empty() || needle.len() > hay.len() {
        return 0;
    }
    let mut count = 0usize;
    let mut index = 0usize;
    while index + needle.len() <= hay.len() {
        if &hay[index..index + needle.len()] == needle {
            count += 1;
            index += needle.len();
        } else {
            index += 1;
        }
    }
    count
}

fn word_ranges(chars: &[char]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut start: Option<usize> = None;
    for (index, ch) in chars.iter().enumerate() {
        if ch.is_alphanumeric() {
            if start.is_none() {
                start = Some(index);
            }
        } else if let Some(begin) = start.take() {
            ranges.push((begin, index));
        }
    }
    if let Some(begin) = start {
        ranges.push((begin, chars.len()));
    }
    ranges
}

fn levenshtein_within(a: &[char], b: &[char], max: usize) -> bool {
    if a.len().abs_diff(b.len()) > max {
        return false;
    }
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (index, char_a) in a.iter().enumerate() {
        let mut current = Vec::with_capacity(b.len() + 1);
        current.push(index + 1);
        let mut row_min = index + 1;
        for (position, char_b) in b.iter().enumerate() {
            let cost = if char_a == char_b { 0 } else { 1 };
            let value = (previous[position + 1] + 1).min(current[position] + 1).min(previous[position] + cost);
            row_min = row_min.min(value);
            current.push(value);
        }
        if row_min > max {
            return false;
        }
        previous = current;
    }
    previous[b.len()] <= max
}

fn fuzzy_word_count(hay: &[char], term: &[char], needle: &mut Vec<char>) -> usize {
    if term.len() > FUZZY_MAX_WORD {
        return count_subsequence(hay, term);
    }
    let mut count = 0usize;
    let mut hit: Option<Vec<char>> = None;
    for (start, end) in word_ranges(hay) {
        let word = &hay[start..end];
        if word.len() > FUZZY_MAX_WORD {
            continue;
        }
        let same = word == term;
        let near = !same && word.len().abs_diff(term.len()) <= 1 && levenshtein_within(word, term, 1);
        if same || near {
            count += 1;
            if hit.is_none() {
                hit = Some(word.to_vec());
            }
        }
    }
    if let Some(word) = hit {
        *needle = word;
    }
    count
}

fn haystack_count(hay: &[char], term: &[char], fuzzy: bool, needle: &mut Vec<char>) -> usize {
    if !fuzzy || term.len() < FUZZY_MIN_WORD {
        return count_subsequence(hay, term);
    }
    fuzzy_word_count(hay, term, needle)
}

fn phrase_matches(text: &str, terms: &[Vec<char>]) -> usize {
    phrase_matches_folded(&fold(text), terms)
}

fn phrase_matches_folded(folded: &[char], terms: &[Vec<char>]) -> usize {
    if terms.is_empty() {
        return 0;
    }
    let words = word_ranges(folded);
    if words.len() < terms.len() {
        return 0;
    }
    let mut count = 0usize;
    let mut index = 0usize;
    while index + terms.len() <= words.len() {
        let mut matches = true;
        for (offset, term) in terms.iter().enumerate() {
            let (start, end) = words[index + offset];
            if end - start != term.len() || &folded[start..end] != term.as_slice() {
                matches = false;
                break;
            }
        }
        if matches {
            count += 1;
            index += terms.len();
        } else {
            index += 1;
        }
    }
    count
}

fn make_snippet(text: &str, needles: &[Vec<char>]) -> String {
    if needles.is_empty() {
        return String::new();
    }
    let folded = fold(text);
    let mut first: Option<usize> = None;
    for needle in needles {
        if let Some(position) = find_subsequence(&folded, needle) {
            if first.map(|start| position < start).unwrap_or(true) {
                first = Some(position);
            }
        }
    }
    let Some(match_start) = first else { return String::new() };
    let length = folded.len();
    let half = SNIPPET_CHARS / 2;
    let mut start = match_start.saturating_sub(half);
    let end = (start + SNIPPET_CHARS).min(length);
    if end - start < SNIPPET_CHARS {
        start = end.saturating_sub(SNIPPET_CHARS);
    }
    let window = &folded[start..end];
    let mut spans: Vec<(usize, usize)> = Vec::new();
    for needle in needles {
        if needle.is_empty() {
            continue;
        }
        let mut offset = 0usize;
        while offset + needle.len() <= window.len() {
            match find_subsequence(&window[offset..], needle) {
                Some(position) => {
                    let at = offset + position;
                    spans.push((at, at + needle.len()));
                    offset = at + needle.len().max(1);
                }
                None => break,
            }
        }
    }
    spans.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (span_start, span_end) in spans {
        if let Some(last) = merged.last_mut() {
            if span_start < last.1 {
                last.1 = last.1.max(span_end);
                continue;
            }
        }
        merged.push((span_start, span_end));
    }
    let original: Vec<char> = text.chars().collect();
    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    let mut cursor = 0usize;
    for (span_start, span_end) in merged {
        out.extend(original[start + cursor..start + span_start].iter());
        out.push_str("<<");
        out.extend(original[start + span_start..start + span_end].iter());
        out.push_str(">>");
        cursor = span_end;
    }
    out.extend(original[start + cursor..end].iter());
    if end < length {
        out.push('…');
    }
    out
}

fn document_chunks(document: &VaultDocument) -> Vec<(&str, &str)> {
    let mut chunks = Vec::new();
    if !document.text.is_empty() {
        chunks.push(("document", document.text.as_str()));
    }
    for location in &document.locations {
        chunks.push((location.label.as_str(), location.text.as_str()));
    }
    chunks
}

fn needles_match_any(hay: &[char], needles: &[Vec<char>]) -> bool {
    needles.iter().any(|needle| count_subsequence(hay, needle) > 0)
}

fn query_terms(query: &str) -> Vec<String> {
    let mut terms: Vec<String> = Vec::new();
    let mut current = String::new();
    for ch in query.chars() {
        if ch.is_alphanumeric() {
            current.push(ch);
        } else if !current.is_empty() {
            terms.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        terms.push(current);
    }
    if terms.is_empty() {
        terms = query.split_whitespace().map(str::to_string).collect();
    }
    let mut seen = HashSet::new();
    terms.retain(|term| seen.insert(term.to_lowercase()));
    terms
}

struct ScoredHit {
    document: usize,
    score: f64,
    label: String,
    snippet: String,
    matched_terms: Vec<String>,
}

fn evaluate_document(
    document: &VaultDocument,
    request: &VaultSearchRequest,
    terms_raw: &[String],
    terms_folded: &[Vec<char>],
) -> Option<ScoredHit> {
    let chunks = document_chunks(document);
    let folded_chunks: Vec<Vec<char>> = chunks.iter().map(|(_, text)| fold(text)).collect();
    let filename = fold(&document.file_name);
    let title = fold(&document.title);
    let headings: Vec<Vec<char>> = document.headings.iter().map(|heading| fold(heading)).collect();

    let mut needles: Vec<Vec<char>> = Vec::new();
    let mut matched_terms: Vec<String> = Vec::new();
    let mut occurrences = 0usize;

    if request.exact {
        let needle = fold(request.query.trim());
        if needle.is_empty() {
            return None;
        }
        let mut count = 0usize;
        for folded in &folded_chunks {
            count += count_subsequence(folded, &needle);
        }
        count += count_subsequence(&filename, &needle);
        count += count_subsequence(&title, &needle);
        for heading in &headings {
            count += count_subsequence(heading, &needle);
        }
        if count == 0 {
            return None;
        }
        matched_terms.push(request.query.trim().to_string());
        needles.push(needle);
        occurrences = count;
    } else if request.phrase {
        let mut count = 0usize;
        for folded in &folded_chunks {
            count += phrase_matches_folded(folded, terms_folded);
        }
        count += phrase_matches(&document.file_name, terms_folded);
        count += phrase_matches(&document.title, terms_folded);
        for heading in &document.headings {
            count += phrase_matches(heading, terms_folded);
        }
        if count == 0 {
            return None;
        }
        matched_terms = terms_raw.to_vec();
        needles = terms_folded.to_vec();
        occurrences = count;
    } else {
        let fuzzy = request.fuzzy;
        if terms_folded.is_empty() {
            return None;
        }
        for term in terms_folded {
            let mut needle = term.clone();
            let mut term_count = 0usize;
            for folded in &folded_chunks {
                term_count += haystack_count(folded, term, fuzzy, &mut needle);
            }
            term_count += haystack_count(&filename, term, fuzzy, &mut needle);
            term_count += haystack_count(&title, term, fuzzy, &mut needle);
            for heading in &headings {
                term_count += haystack_count(heading, term, fuzzy, &mut needle);
            }
            if term_count == 0 {
                return None;
            }
            occurrences += term_count;
            needles.push(needle);
        }
        matched_terms = terms_raw.to_vec();
    }

    let mut best_label = "document".to_string();
    let mut best_snippet = String::new();
    let mut best_index: Option<usize> = None;
    let mut best_count = 0usize;
    let document_has_text = !document.text.is_empty();
    for (index, (label, text)) in chunks.iter().enumerate() {
        let folded = &folded_chunks[index];
        let count: usize = needles.iter().map(|needle| count_subsequence(folded, needle)).sum();
        if count == 0 {
            continue;
        }
        // A labelled location is more useful than the whole-document chunk,
        // so it wins ties ("Sheet1!A1:B7" instead of "document").
        let candidate_is_document = index == 0 && document_has_text;
        let best_is_document = best_index == Some(0) && document_has_text;
        let better = best_index.is_none()
            || count > best_count
            || (count == best_count && best_is_document && !candidate_is_document);
        if better {
            best_index = Some(index);
            best_count = count;
            best_label = (*label).to_string();
            best_snippet = make_snippet(text, &needles);
        }
    }
    if best_index.is_none() {
        if needles_match_any(&filename, &needles) {
            best_label = "file name".into();
            best_snippet = make_snippet(&document.file_name, &needles);
        } else if needles_match_any(&title, &needles) {
            best_label = "title".into();
            best_snippet = make_snippet(&document.title, &needles);
        } else if headings.iter().any(|heading| needles_match_any(heading, &needles)) {
            best_label = "heading".into();
            if let Some(heading) = document
                .headings
                .iter()
                .find(|heading| needles_match_any(&fold(heading), &needles))
            {
                best_snippet = make_snippet(heading, &needles);
            }
        } else {
            best_label = "document".into();
        }
    }

    let mut score = occurrences as f64 * 10.0;
    if needles_match_any(&filename, &needles) {
        score += 25.0;
    }
    if needles_match_any(&title, &needles) {
        score += 15.0;
    }
    if headings.iter().any(|heading| needles_match_any(heading, &needles)) {
        score += 20.0;
    }

    Some(ScoredHit {
        document: 0,
        score,
        label: best_label,
        snippet: best_snippet,
        matched_terms,
    })
}

/// Pure search over an in-memory index. Modes: `exact` (whole query as a
/// case-insensitive substring), `phrase` (query words adjacent), `fuzzy`
/// (Levenshtein distance 1 per word, words of 4..64 chars) and the default
/// AND of substring terms. An empty query returns no hits.
pub fn search_index(index: &VaultIndex, request: &VaultSearchRequest) -> VaultSearchResponse {
    let started = Instant::now();
    let query = request.query.trim();
    if query.is_empty() {
        return VaultSearchResponse {
            hits: Vec::new(),
            total: 0,
            took_ms: started.elapsed().as_millis() as u64,
            index_missing: false,
        };
    }
    let terms_raw = query_terms(query);
    let terms_folded: Vec<Vec<char>> = terms_raw.iter().map(|term| fold(term)).collect();
    let extensions: HashSet<String> = request
        .extensions
        .iter()
        .map(|extension| extension.trim().trim_start_matches('.').to_ascii_lowercase())
        .filter(|extension| !extension.is_empty())
        .collect();
    let folder = request.folder.as_deref().map(normalize_path_key).filter(|folder| !folder.is_empty());
    let limit = if request.limit == 0 { DEFAULT_LIMIT } else { request.limit.min(MAX_LIMIT) };

    let mut scored: Vec<(usize, ScoredHit)> = Vec::new();
    for (position, document) in index.documents.iter().enumerate() {
        if !extensions.is_empty() && !extensions.contains(&document.extension) {
            continue;
        }
        if let Some(folder_key) = &folder {
            if !path_is_under(&normalize_path_key(&document.path), folder_key) {
                continue;
            }
        }
        if let Some(after) = &request.modified_after {
            if document.modified.as_str() < after.as_str() {
                continue;
            }
        }
        if let Some(before) = &request.modified_before {
            if document.modified.as_str() >= before.as_str() {
                continue;
            }
        }
        if let Some(mut hit) = evaluate_document(document, request, &terms_raw, &terms_folded) {
            hit.document = position;
            scored.push((position, hit));
        }
    }

    scored.sort_by(|(position_a, hit_a), (position_b, hit_b)| {
        hit_b
            .score
            .partial_cmp(&hit_a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| index.documents[*position_b].modified.cmp(&index.documents[*position_a].modified))
            .then_with(|| index.documents[*position_a].path.cmp(&index.documents[*position_b].path))
    });

    let total = scored.len();
    let hits: Vec<VaultSearchHit> = scored
        .into_iter()
        .skip(request.offset)
        .take(limit)
        .map(|(_, hit)| {
            let document = &index.documents[hit.document];
            VaultSearchHit {
                document_id: document.id.clone(),
                path: document.path.clone(),
                file_name: document.file_name.clone(),
                extension: document.extension.clone(),
                size: document.size,
                modified: document.modified.clone(),
                score: hit.score,
                match_label: hit.label,
                snippet: hit.snippet,
                matched_terms: hit.matched_terms,
            }
        })
        .collect();

    VaultSearchResponse {
        hits,
        total,
        took_ms: started.elapsed().as_millis() as u64,
        index_missing: false,
    }
}

/// Search on disk: `index_missing` is true when there is no usable index.
pub fn search_at(root: &Path, request: &VaultSearchRequest) -> VaultSearchResponse {
    if !index_path(root).exists() {
        return VaultSearchResponse { hits: Vec::new(), total: 0, took_ms: 0, index_missing: true };
    }
    let (index, warnings) = load_index(root);
    let mut response = search_index(&index, request);
    response.index_missing = index.documents.is_empty() && !warnings.is_empty();
    response
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

fn config_dir(app: &AppHandle) -> Result<PathBuf, PdfError> {
    app.path()
        .app_config_dir()
        .map_err(|error| PdfError::Internal(format!("Could not locate the app data directory: {error}")))
}

async fn run_blocking<T, F>(work: F) -> Result<T, PdfError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, PdfError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| PdfError::Internal(format!("worker thread failed: {error}")))?
}

#[tauri::command]
pub fn vault_status(app: AppHandle) -> Result<VaultStatus, PdfError> {
    let root = config_dir(&app)?;
    Ok(vault_status_at(&root))
}

/// Saves the folder selection and the indexing switches.
#[tauri::command]
pub fn vault_configure(app: AppHandle, config: VaultConfig) -> Result<VaultConfig, PdfError> {
    let root = config_dir(&app)?;
    save_config(&root, &config)
}

/// Scans the configured folders. When `request.folders` is present, those
/// user-selected folders are added to the saved configuration first; folders
/// outside the saved configuration are never walked.
#[tauri::command]
pub async fn vault_scan(app: AppHandle, registry: State<'_, JobRegistry>, request: VaultScanRequest) -> Result<VaultStatus, PdfError> {
    let job_id = "vault-scan".to_string();
    let cancel = registry.register(&job_id);
    let app_for_work = app.clone();
    let job_for_work = job_id.clone();
    let result = run_blocking(move || {
        let root = config_dir(&app_for_work)?;
        if let Some(folders) = &request.folders {
            let mut config = load_config(&root);
            for folder in folders {
                let trimmed = folder.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let key = normalize_path_key(trimmed);
                if !config.folders.iter().any(|existing| normalize_path_key(existing) == key) {
                    config.folders.push(trimmed.to_string());
                }
            }
            save_config(&root, &config)?;
        }
        let config = load_config(&root);
        let mut stored = read_stored_status(&root);
        stored.scanning = true;
        write_stored_status(&root, &stored)?;
        emit_progress(&app_for_work, &job_for_work, &ProgressEvent::new("scan", 0, 1));
        let outcome = scan_folders(&root, &config, &request, &cancel);
        stored.scanning = false;
        if !outcome.cancelled {
            stored.last_scan = Some(outcome.index.updated_at.clone());
        }
        if let Err(error) = write_stored_status(&root, &stored) {
            return Err(error);
        }
        emit_progress(&app_for_work, &job_for_work, &ProgressEvent::new("scan", 1, 1));
        if outcome.cancelled {
            return Err(PdfError::Cancelled);
        }
        Ok(VaultStatus {
            indexed: outcome.index.documents.len(),
            folders: config.folders.len(),
            index_bytes: std::fs::metadata(index_path(&root)).map(|metadata| metadata.len()).unwrap_or(0),
            last_scan: stored.last_scan,
            scanning: false,
            warnings: outcome.warnings,
        })
    })
    .await;
    registry.finish(&job_id);
    result
}

#[tauri::command]
pub async fn vault_search(app: AppHandle, request: VaultSearchRequest) -> Result<VaultSearchResponse, PdfError> {
    let root = config_dir(&app)?;
    run_blocking(move || Ok(search_at(&root, &request))).await
}

/// Full cached text of one indexed document (for preview panes).
#[tauri::command]
pub async fn vault_document_text(app: AppHandle, id: String) -> Result<String, PdfError> {
    let root = config_dir(&app)?;
    run_blocking(move || {
        let safe = sanitize_id(&id)?;
        let path = docs_dir(&root).join(format!("{safe}.txt"));
        if path.exists() {
            return std::fs::read_to_string(&path).map_err(PdfError::from_io);
        }
        let (index, _) = load_index(&root);
        index
            .documents
            .iter()
            .find(|document| document.id == safe)
            .map(|document| document.text.clone())
            .ok_or_else(|| PdfError::NotFound(format!("vault document {safe}")))
    })
    .await
}

/// Forgets the index and the extraction cache; the folder selection stays.
#[tauri::command]
pub fn vault_clear(app: AppHandle) -> Result<VaultStatus, PdfError> {
    let root = config_dir(&app)?;
    clear_vault_at(&root)?;
    Ok(vault_status_at(&root))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pdfsak-vault-{label}-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn test_config(folders: Vec<String>) -> VaultConfig {
        VaultConfig {
            folders,
            include_pdf: false,
            include_office: true,
            max_file_mb: 8,
            updated_at: String::new(),
        }
    }

    fn scan_request() -> VaultScanRequest {
        VaultScanRequest { folders: None, rescan: false, max_files: 0 }
    }

    fn document_with_extension<'a>(index: &'a VaultIndex, extension: &str) -> &'a VaultDocument {
        index
            .documents
            .iter()
            .find(|document| document.extension == extension)
            .expect("document with extension")
    }

    fn search_request(query: &str) -> VaultSearchRequest {
        VaultSearchRequest { query: query.to_string(), ..Default::default() }
    }

    #[allow(clippy::too_many_arguments)]
    fn sample_document(
        id: &str,
        path: &str,
        file_name: &str,
        extension: &str,
        size: u64,
        modified: &str,
        title: &str,
        headings: Vec<&str>,
        text: &str,
        locations: Vec<(&str, &str)>,
    ) -> VaultDocument {
        VaultDocument {
            id: id.to_string(),
            path: path.to_string(),
            file_name: file_name.to_string(),
            extension: extension.to_string(),
            size,
            modified: modified.to_string(),
            title: title.to_string(),
            headings: headings.into_iter().map(str::to_string).collect(),
            text: text.to_string(),
            locations: locations
                .into_iter()
                .map(|(label, text)| TextLocation { label: label.to_string(), text: text.to_string() })
                .collect(),
            tags: Vec::new(),
            indexed_at: "2026-04-01T00:00:00Z".into(),
        }
    }

    fn sample_index() -> VaultIndex {
        VaultIndex {
            version: INDEX_VERSION,
            updated_at: "2026-04-01T00:00:00Z".into(),
            documents: vec![
                sample_document(
                    "a1",
                    "C:/vault/alpha.txt",
                    "alpha.txt",
                    "txt",
                    120,
                    "2026-01-02T00:00:00Z",
                    "Alpha notes",
                    vec!["Quick start"],
                    "The quick brown fox jumps over the lazy dog. Color and colour.",
                    vec![("paragraph 1", "The quick brown fox jumps over the lazy dog. Color and colour.")],
                ),
                sample_document(
                    "b2",
                    "C:/vault/sub/beta.md",
                    "beta.md",
                    "md",
                    220,
                    "2026-02-02T00:00:00Z",
                    "Beta plan",
                    Vec::new(),
                    "Alpha beta gamma. Brown bear.",
                    vec![("paragraph 1", "Alpha beta gamma."), ("paragraph 2", "Brown bear.")],
                ),
                sample_document(
                    "c3",
                    "C:/other/gamma.csv",
                    "gamma.csv",
                    "csv",
                    90,
                    "2026-03-02T00:00:00Z",
                    "Gamma data",
                    Vec::new(),
                    "Sheet1!A1:B2\nname: alpha\nvalue: 42",
                    vec![("Sheet1!A1:B2", "name: alpha\nvalue: 42")],
                ),
            ],
        }
    }

    #[test]
    fn config_round_trip_and_recovery() {
        let base = temp_root("config");
        let config = VaultConfig {
            folders: vec!["C:/docs".into()],
            include_pdf: false,
            include_office: true,
            max_file_mb: 10,
            updated_at: String::new(),
        };
        let saved = save_config(&base, &config).expect("save config");
        assert!(!saved.updated_at.is_empty(), "updatedAt is stamped");
        let loaded = load_config(&base);
        assert_eq!(loaded.folders, vec!["C:/docs".to_string()]);
        assert!(!loaded.include_pdf);
        assert_eq!(loaded.max_file_mb, 10);
        // Damaged config falls back to defaults instead of failing.
        std::fs::write(config_path(&base), b"{ not json").expect("damage config");
        assert!(load_config(&base).folders.is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn scan_indexes_text_markdown_and_csv_with_locations() {
        let base = temp_root("scan");
        let root = base.join("root");
        let docs = base.join("docs");
        std::fs::create_dir_all(&docs).expect("docs dir");
        std::fs::write(docs.join("note.txt"), "Hello vault world\n\nSecond paragraph about jars").expect("txt");
        std::fs::write(docs.join("readme.md"), "# Title\n\nMarkdown body with widgets").expect("md");
        std::fs::write(docs.join("table.csv"), "name,qty\njar,3\nwidget,7").expect("csv");
        // Hidden files and folders are never indexed.
        std::fs::create_dir_all(docs.join(".hidden")).expect("hidden dir");
        std::fs::write(docs.join(".hidden").join("secret.txt"), "hidden secret").expect("hidden file");
        std::fs::write(docs.join(".hidden.txt"), "hidden secret").expect("hidden file 2");

        let config = test_config(vec![docs.display().to_string()]);
        let outcome = scan_folders(&root, &config, &scan_request(), &CancelToken::new());
        assert_eq!(outcome.scanned, 3);
        assert_eq!(outcome.indexed, 3);
        assert_eq!(outcome.skipped, 0);
        assert_eq!(outcome.index.documents.len(), 3);
        assert!(
            !outcome.warnings.iter().any(|warning| warning.contains("could not")),
            "no extraction failures: {:?}",
            outcome.warnings
        );
        assert!(index_path(&root).exists(), "index is written");

        let txt = document_with_extension(&outcome.index, "txt");
        assert_eq!(txt.file_name, "note.txt");
        assert!(txt.text.contains("Hello vault"));
        assert!(!txt.locations.is_empty());
        assert_eq!(txt.locations[0].label, "paragraph 1");
        assert!(txt.locations.iter().any(|location| location.label == "paragraph 3"));

        let csv = document_with_extension(&outcome.index, "csv");
        assert!(csv.text.contains("widget"));
        assert_eq!(csv.locations.len(), 1);
        assert_eq!(csv.locations[0].label, "Sheet1!A1:B3");

        for document in &outcome.index.documents {
            let cache = document_text_path(&root, &document.id).expect("cache path");
            assert!(cache.exists(), "cache written for {}", document.extension);
        }
        let txt_text = read_document_text(&root, &txt.id).expect("read cache");
        assert!(txt_text.contains("Second paragraph"));

        let found = search_at(&root, &search_request("widgets"));
        assert_eq!(found.total, 1);
        assert_eq!(found.hits[0].extension, "md");
        assert!(found.hits[0].snippet.contains("<<widgets>>"));

        let csv_hit = search_at(&root, &search_request("jar"));
        assert!(csv_hit.hits.iter().any(|hit| hit.extension == "csv"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn incremental_rescan_skips_unchanged_and_reindexes_changes() {
        let base = temp_root("incremental");
        let root = base.join("root");
        let docs = base.join("docs");
        std::fs::create_dir_all(&docs).expect("docs dir");
        let txt_path = docs.join("note.txt");
        std::fs::write(&txt_path, "Hello vault world\n\nSecond paragraph about jars").expect("txt");
        std::fs::write(docs.join("readme.md"), "# Title\n\nMarkdown body with widgets").expect("md");
        let config = test_config(vec![docs.display().to_string()]);

        let first = scan_folders(&root, &config, &scan_request(), &CancelToken::new());
        assert_eq!(first.indexed, 2);

        let second = scan_folders(&root, &config, &scan_request(), &CancelToken::new());
        assert_eq!(second.indexed, 0);
        assert_eq!(second.skipped, 2);
        assert_eq!(second.index.documents.len(), 2);

        // A changed file is re-extracted even without a full rescan.
        std::fs::write(&txt_path, "Hello vault world\n\nSecond paragraph about jars and a much longer tail of words.").expect("rewrite");
        let third = scan_folders(&root, &config, &scan_request(), &CancelToken::new());
        assert_eq!(third.indexed, 1);
        assert_eq!(third.skipped, 1);
        assert!(document_with_extension(&third.index, "txt").text.contains("much longer tail"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn deleted_files_are_removed_from_the_index() {
        let base = temp_root("deleted");
        let root = base.join("root");
        let docs = base.join("docs");
        std::fs::create_dir_all(&docs).expect("docs dir");
        let txt_path = docs.join("note.txt");
        let md_path = docs.join("readme.md");
        std::fs::write(&txt_path, "Keep me around").expect("txt");
        std::fs::write(&md_path, "Delete me soon").expect("md");
        let config = test_config(vec![docs.display().to_string()]);
        let first = scan_folders(&root, &config, &scan_request(), &CancelToken::new());
        assert_eq!(first.index.documents.len(), 2);

        // Incremental scan notices the missing file.
        std::fs::remove_file(&md_path).expect("delete md");
        let second = scan_folders(&root, &config, &scan_request(), &CancelToken::new());
        assert_eq!(second.removed, 1);
        assert_eq!(second.index.documents.len(), 1);
        assert_eq!(document_with_extension(&second.index, "txt").file_name, "note.txt");

        // A full rescan removes everything that is gone.
        std::fs::remove_file(&txt_path).expect("delete txt");
        let mut request = scan_request();
        request.rescan = true;
        let third = scan_folders(&root, &config, &request, &CancelToken::new());
        assert_eq!(third.removed, 1);
        assert!(third.index.documents.is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn corrupted_index_recovers_to_empty_and_rescans() {
        let base = temp_root("corrupt");
        let root = base.join("root");
        let docs = base.join("docs");
        std::fs::create_dir_all(docs_dir(&root)).expect("vault dir");
        std::fs::create_dir_all(&docs).expect("docs dir");
        std::fs::write(docs.join("note.txt"), "Recovery works").expect("txt");
        std::fs::write(index_path(&root), b"}}} not json").expect("damage index");

        let (index, warnings) = load_index(&root);
        assert!(index.documents.is_empty());
        assert!(!warnings.is_empty(), "recovery produces a warning");

        let config = test_config(vec![docs.display().to_string()]);
        let outcome = scan_folders(&root, &config, &scan_request(), &CancelToken::new());
        assert_eq!(outcome.index.documents.len(), 1);
        assert!(load_index(&root).0.documents.len() == 1, "index is usable again");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn folder_outside_the_config_is_never_scanned() {
        let base = temp_root("optin");
        let root = base.join("root");
        let selected = base.join("selected");
        let other = base.join("other");
        std::fs::create_dir_all(&selected).expect("selected");
        std::fs::create_dir_all(&other).expect("other");
        std::fs::write(selected.join("wanted.txt"), "selected folder text").expect("selected file");
        std::fs::write(other.join("unwanted.txt"), "unselected folder text").expect("other file");

        let config = test_config(vec![selected.display().to_string()]);
        let mut request = scan_request();
        request.folders = Some(vec![selected.display().to_string(), other.display().to_string()]);
        let outcome = scan_folders(&root, &config, &request, &CancelToken::new());

        assert_eq!(outcome.index.documents.len(), 1);
        let document = &outcome.index.documents[0];
        assert!(Path::new(&document.path).starts_with(&selected));
        assert!(
            outcome.warnings.iter().any(|warning| warning.contains("other")),
            "the unselected folder is reported: {:?}",
            outcome.warnings
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn broken_files_and_missing_folders_do_not_abort_the_scan() {
        let base = temp_root("resilient");
        let root = base.join("root");
        let docs = base.join("docs");
        let missing = base.join("missing");
        std::fs::create_dir_all(&docs).expect("docs dir");
        std::fs::write(docs.join("good.txt"), "Still searchable").expect("good");
        std::fs::write(docs.join("broken.docx"), b"this is not a zip package").expect("broken");

        let config = test_config(vec![docs.display().to_string(), missing.display().to_string()]);
        let outcome = scan_folders(&root, &config, &scan_request(), &CancelToken::new());
        assert!(!outcome.cancelled);
        assert_eq!(outcome.index.documents.len(), 2, "the readable file is indexed");
        assert!(outcome.index.documents.iter().any(|document| document.file_name == "good.txt"));
        assert!(outcome.warnings.iter().any(|warning| warning.contains("broken.docx")));
        assert!(outcome.warnings.iter().any(|warning| warning.contains("missing")));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn oversized_files_are_skipped_with_a_warning() {
        let base = temp_root("caps");
        let root = base.join("root");
        let docs = base.join("docs");
        std::fs::create_dir_all(&docs).expect("docs dir");
        std::fs::write(docs.join("small.txt"), "small file").expect("small");
        std::fs::write(docs.join("big.txt"), "x".repeat(1_200_000)).expect("big");
        let mut config = test_config(vec![docs.display().to_string()]);
        config.max_file_mb = 1;

        let outcome = scan_folders(&root, &config, &scan_request(), &CancelToken::new());
        assert_eq!(outcome.index.documents.len(), 1);
        assert!(outcome.warnings.iter().any(|warning| warning.contains("big.txt")));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn file_cap_truncates_the_scan_with_a_warning() {
        let base = temp_root("files-cap");
        let root = base.join("root");
        let docs = base.join("docs");
        std::fs::create_dir_all(&docs).expect("docs dir");
        std::fs::write(docs.join("one.txt"), "first file").expect("one");
        std::fs::write(docs.join("two.txt"), "second file").expect("two");
        let config = test_config(vec![docs.display().to_string()]);
        let mut request = scan_request();
        request.max_files = 1;
        let outcome = scan_folders(&root, &config, &request, &CancelToken::new());
        assert!(outcome.truncated);
        assert_eq!(outcome.index.documents.len(), 1);
        assert!(outcome.warnings.iter().any(|warning| warning.contains("Stopped after")));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn cancelled_scan_leaves_the_index_untouched() {
        let base = temp_root("cancel");
        let root = base.join("root");
        let docs = base.join("docs");
        std::fs::create_dir_all(&docs).expect("docs dir");
        std::fs::write(docs.join("note.txt"), "Cancel me").expect("txt");
        let cancel = CancelToken::new();
        cancel.cancel();
        let config = test_config(vec![docs.display().to_string()]);
        let outcome = scan_folders(&root, &config, &scan_request(), &cancel);
        assert!(outcome.cancelled);
        assert!(!index_path(&root).exists(), "a partial index is never written");
        assert!(outcome.warnings.iter().any(|warning| warning.contains("cancelled")));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn document_ids_are_safe_and_cache_paths_reject_traversal() {
        let id = document_id("C:/docs/report.pdf");
        assert_eq!(id.len(), 16);
        assert!(id.chars().all(|ch| ch.is_ascii_hexdigit()));
        let base = temp_root("traversal");
        assert!(document_text_path(&base, "../evil").is_err());
        assert!(document_text_path(&base, "not-hex").is_err());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn search_modes_exact_phrase_default_and_fuzzy() {
        let index = sample_index();

        let default_and = search_index(&index, &search_request("alpha beta"));
        assert_eq!(default_and.total, 1);
        assert_eq!(default_and.hits[0].file_name, "beta.md");
        assert!(default_and.hits[0].matched_terms.iter().any(|term| term == "alpha"));

        let exact = search_index(&index, &VaultSearchRequest { query: "rown fo".into(), exact: true, ..Default::default() });
        assert_eq!(exact.total, 1);
        assert_eq!(exact.hits[0].file_name, "alpha.txt");
        assert!(exact.hits[0].snippet.contains("<<rown fo>>"));

        let phrase = search_index(&index, &VaultSearchRequest { query: "quick brown".into(), phrase: true, ..Default::default() });
        assert_eq!(phrase.total, 1);
        assert_eq!(phrase.hits[0].file_name, "alpha.txt");

        let wrong_order = search_index(&index, &VaultSearchRequest { query: "fox brown".into(), phrase: true, ..Default::default() });
        assert_eq!(wrong_order.total, 0, "phrase requires adjacent words in order");

        let not_adjacent = search_index(&index, &VaultSearchRequest { query: "quick fox".into(), phrase: true, ..Default::default() });
        assert_eq!(not_adjacent.total, 0);

        let fuzzy = search_index(&index, &VaultSearchRequest { query: "colr".into(), fuzzy: true, ..Default::default() });
        assert_eq!(fuzzy.total, 1);
        assert_eq!(fuzzy.hits[0].file_name, "alpha.txt");
        assert!(fuzzy.hits[0].snippet.to_lowercase().contains("<<color>>"), "fuzzy highlights the matched word");

        let short_fuzzy = search_index(&index, &VaultSearchRequest { query: "cat".into(), fuzzy: true, ..Default::default() });
        assert_eq!(short_fuzzy.total, 0, "below the fuzzy word-length guard only substrings match");

        let headings = search_index(&index, &search_request("quick start"));
        assert_eq!(headings.total, 1);
        assert_eq!(headings.hits[0].file_name, "alpha.txt");

        let filename_only = search_index(&index, &search_request("gamma"));
        assert!(filename_only.total >= 1);
        assert!(filename_only
            .hits
            .iter()
            .any(|hit| hit.file_name == "gamma.csv" && hit.match_label == "file name"));

        let empty = search_index(&index, &search_request("   "));
        assert_eq!(empty.total, 0);
    }

    #[test]
    fn search_filters_snippets_and_paging() {
        let index = sample_index();

        let by_extension = search_index(&index, &VaultSearchRequest { query: "alpha".into(), extensions: vec![".MD".into()], ..Default::default() });
        assert_eq!(by_extension.total, 1);
        assert_eq!(by_extension.hits[0].extension, "md");

        let in_folder = search_index(&index, &VaultSearchRequest { query: "alpha".into(), folder: Some("C:/vault".into()), ..Default::default() });
        assert_eq!(in_folder.total, 2);
        let in_sub = search_index(&index, &VaultSearchRequest { query: "alpha".into(), folder: Some("C:/vault/sub".into()), ..Default::default() });
        assert_eq!(in_sub.total, 1);
        assert_eq!(in_sub.hits[0].file_name, "beta.md");
        let boundary = search_index(&index, &VaultSearchRequest { query: "alpha".into(), folder: Some("C:/vau".into()), ..Default::default() });
        assert_eq!(boundary.total, 0, "folder filters respect path boundaries");

        let after = search_index(&index, &VaultSearchRequest { query: "alpha".into(), modified_after: Some("2026-02-15T00:00:00Z".into()), ..Default::default() });
        assert_eq!(after.total, 1);
        assert_eq!(after.hits[0].file_name, "gamma.csv");
        let before = search_index(&index, &VaultSearchRequest { query: "alpha".into(), modified_before: Some("2026-02-15T00:00:00Z".into()), ..Default::default() });
        assert_eq!(before.total, 2);

        let paged = search_index(&index, &VaultSearchRequest { query: "alpha".into(), limit: 2, ..Default::default() });
        assert_eq!(paged.total, 3);
        assert_eq!(paged.hits.len(), 2);
        let offset = search_index(&index, &VaultSearchRequest { query: "alpha".into(), limit: 10, offset: 1, ..Default::default() });
        assert_eq!(offset.total, 3);
        assert_eq!(offset.hits.len(), 2);

        let labelled = search_index(&index, &search_request("value"));
        assert_eq!(labelled.total, 1);
        assert_eq!(labelled.hits[0].match_label, "Sheet1!A1:B2");
        assert!(labelled.hits[0].snippet.contains("<<value>>"));
    }

    #[test]
    fn clear_removes_index_and_cache_but_keeps_config() {
        let base = temp_root("clear");
        let root = base.join("root");
        let docs = base.join("docs");
        std::fs::create_dir_all(&docs).expect("docs dir");
        std::fs::write(docs.join("note.txt"), "clear me").expect("txt");
        let config = test_config(vec![docs.display().to_string()]);
        save_config(&root, &config).expect("save config");
        scan_folders(&root, &config, &scan_request(), &CancelToken::new());
        assert!(index_path(&root).exists());
        clear_vault_at(&root).expect("clear");
        assert!(!index_path(&root).exists());
        assert!(!docs_dir(&root).exists());
        assert_eq!(load_config(&root).folders.len(), 1, "folder selection is kept");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn html_tags_are_stripped_for_indexing() {
        let stripped = strip_html_tags("<h1>Hello</h1><p>World &amp; friends</p>");
        assert!(stripped.contains("Hello"));
        assert!(stripped.contains("World"));
        assert!(!stripped.contains("<h1>"));
        assert!(!stripped.contains("</p>"));
    }
}
