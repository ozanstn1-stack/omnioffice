//! Persistent stores for AI results and the operation log.
//!
//! * `ai-library.json` keeps an index of every saved AI result (summary,
//!   translation, Q&A answer, cleaned text) next to a Markdown file on disk.
//! * `operations.json` keeps a log of finished operations (merge, split,
//!   compress, OCR, watermark, convert, AI ...) with paths and size deltas -
//!   never document content.
//!
//! Both files live in the app config folder; the Markdown files live in the
//! AI library folder chosen in Settings (Documents by default).

use pdfcore::error::PdfError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const MAX_AI_ENTRIES: usize = 300;
const MAX_OPERATION_ENTRIES: usize = 500;
const PREVIEW_CHARS: usize = 400;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiLibraryEntry {
    pub id: String,
    /// Unix seconds.
    pub created_at: u64,
    /// summary | translate | ask | cleanup | metadata
    pub kind: String,
    pub source_path: String,
    pub source_name: String,
    pub model: String,
    pub pages: u32,
    pub characters: u64,
    /// Human readable summary of the options used.
    pub options: String,
    /// Markdown file written to disk.
    pub file_path: String,
    /// First characters of the result, for the list view.
    pub preview: String,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationEntry {
    pub id: String,
    pub created_at: u64,
    /// merge | split | compress | ocr | watermark | protect | convert | ai ...
    pub operation: String,
    pub input_path: String,
    pub output_path: String,
    pub page_count: Option<u32>,
    pub input_bytes: Option<u64>,
    pub output_bytes: Option<u64>,
    /// true when the user cancelled or the operation failed (kept for context).
    pub ok: bool,
    pub detail: Option<String>,
}

fn now_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn read_json<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> T {
    if !path.exists() {
        return T::default();
    }
    std::fs::read(path)
        .ok()
        .map(|bytes| {
            let text = String::from_utf8_lossy(bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes)).to_string();
            serde_json::from_str::<T>(&text).unwrap_or_default()
        })
        .unwrap_or_default()
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), PdfError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(PdfError::from_io)?;
    }
    let text = serde_json::to_string_pretty(value)
        .map_err(|error| PdfError::Internal(format!("store serialize failed: {error}")))?;
    // Crash-safe: a torn write here used to silently drop the whole library or
    // operation log because the readers fall back to `Default`.
    crate::commands::write_atomic(path, text.as_bytes())
}

fn slug(value: &str, max: usize) -> String {
    let mut out = String::new();
    for character in value.chars() {
        if character.is_alphanumeric() {
            out.push(character.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
        if out.len() >= max {
            break;
        }
    }
    out.trim_matches('-').to_string()
}

/// Builds the Markdown file name for a new library entry.
pub fn library_file_name(kind: &str, source_name: &str, created_at: u64) -> String {
    let stem = Path::new(source_name)
        .file_stem()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "document".into());
    // yyyy-MM-dd_HHmmss from the unix timestamp (UTC is fine for ordering).
    let seconds = created_at;
    let days = seconds / 86_400;
    let time = seconds % 86_400;
    let (year, month, day) = civil_from_days(days as i64);
    format!(
        "{year:04}-{month:02}-{day:02}_{:02}{:02}{:02}_{}_{}.md",
        time / 3600,
        (time % 3600) / 60,
        time % 60,
        kind,
        slug(&stem, 40)
    )
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// Saves a result: writes the Markdown file and appends the index entry.
#[allow(clippy::too_many_arguments)]
pub fn save_ai_entry(
    index_path: &Path,
    library_dir: &Path,
    kind: &str,
    source_path: &str,
    source_name: &str,
    model: &str,
    pages: u32,
    characters: u64,
    options: &str,
    text: &str,
    elapsed_ms: u64,
) -> Result<AiLibraryEntry, PdfError> {
    if text.trim().is_empty() {
        return Err(PdfError::InvalidInput("nothing to save".into()));
    }
    std::fs::create_dir_all(library_dir).map_err(PdfError::from_io)?;
    let created_at = now_seconds();
    let file_name = library_file_name(kind, source_name, created_at);
    let file_path = unique_path(library_dir, &file_name);
    crate::commands::write_atomic(&file_path, text.as_bytes())?;

    let entry = AiLibraryEntry {
        id: format!("{created_at}-{}", slug(kind, 12)),
        created_at,
        kind: kind.to_string(),
        source_path: source_path.to_string(),
        source_name: source_name.to_string(),
        model: model.to_string(),
        pages,
        characters,
        options: options.to_string(),
        file_path: file_path.display().to_string(),
        preview: text.chars().take(PREVIEW_CHARS).collect(),
        elapsed_ms,
    };
    let mut entries: Vec<AiLibraryEntry> = read_json(index_path);
    entries.insert(0, entry.clone());
    entries.truncate(MAX_AI_ENTRIES);
    write_json(index_path, &entries)?;
    Ok(entry)
}

fn unique_path(dir: &Path, file_name: &str) -> PathBuf {
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }
    let stem = Path::new(file_name).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let extension = Path::new(file_name).extension().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "md".into());
    for index in 1..1000 {
        let next = dir.join(format!("{stem}-{index}.{extension}"));
        if !next.exists() {
            return next;
        }
    }
    // Never fall back to the taken candidate: with an atomic (replacing) write
    // that would silently overwrite an existing library file.
    dir.join(format!("{stem}-{}.{extension}", uuid::Uuid::new_v4().simple()))
}

pub fn list_ai_entries(index_path: &Path) -> Vec<AiLibraryEntry> {
    let entries: Vec<AiLibraryEntry> = read_json(index_path);
    // Drop entries whose Markdown file disappeared.
    entries.into_iter().filter(|entry| Path::new(&entry.file_path).exists()).collect()
}

pub fn delete_ai_entry(index_path: &Path, id: &str, delete_file: bool) -> Result<(), PdfError> {
    let mut entries: Vec<AiLibraryEntry> = read_json(index_path);
    if let Some(position) = entries.iter().position(|entry| entry.id == id) {
        let removed = entries.remove(position);
        if delete_file {
            let _ = std::fs::remove_file(&removed.file_path);
        }
        write_json(index_path, &entries)?;
    }
    Ok(())
}

pub fn clear_ai_entries(index_path: &Path, delete_files: bool) -> Result<(), PdfError> {
    let entries: Vec<AiLibraryEntry> = read_json(index_path);
    if delete_files {
        for entry in &entries {
            let _ = std::fs::remove_file(&entry.file_path);
        }
    }
    write_json(index_path, &Vec::<AiLibraryEntry>::new())
}

pub fn read_ai_entry_text(index_path: &Path, id: &str) -> Result<String, PdfError> {
    let entries: Vec<AiLibraryEntry> = read_json(index_path);
    let entry = entries
        .into_iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| PdfError::NotFound(format!("library entry {id}")))?;
    std::fs::read_to_string(&entry.file_path).map_err(PdfError::from_io)
}

// ---------------------------------------------------------------------------
// Operation log
// ---------------------------------------------------------------------------

pub fn append_operation(log_path: &Path, mut entry: OperationEntry) -> Result<(), PdfError> {
    if entry.created_at == 0 {
        entry.created_at = now_seconds();
    }
    if entry.id.is_empty() {
        entry.id = format!("{}-{}", entry.created_at, slug(&entry.operation, 16));
    }
    let mut entries: Vec<OperationEntry> = read_json(log_path);
    entries.insert(0, entry);
    entries.truncate(MAX_OPERATION_ENTRIES);
    write_json(log_path, &entries)
}

pub fn list_operations(log_path: &Path) -> Vec<OperationEntry> {
    read_json(log_path)
}

pub fn clear_operations(log_path: &Path) -> Result<(), PdfError> {
    write_json(log_path, &Vec::<OperationEntry>::new())
}

impl AiLibraryEntry {
    /// Used by the library tests; the UI receives the full path instead.
    #[allow(dead_code)]
    fn file_name(&self) -> String {
        Path::new(&self.file_path)
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pdfsak-library-test-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn ai_entries_round_trip_and_files_are_written() {
        let dir = temp_dir("ai");
        let index = dir.join("ai-library.json");
        let library = dir.join("library");
        let entry = save_ai_entry(
            &index,
            &library,
            "summary",
            "C:/docs/report.pdf",
            "report.pdf",
            "deepseek-flash",
            12,
            4200,
            "medium / paragraph / Turkish",
            "# Summary\n\nThe report covers 2026 results.",
            1500,
        )
        .expect("save");
        assert!(Path::new(&entry.file_path).exists(), "markdown file must exist");
        assert!(entry.file_name().starts_with("20"));
        assert!(entry.file_path.ends_with(".md"));
        assert!(entry.preview.contains("Summary"));

        let listed = list_ai_entries(&index);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].kind, "summary");
        assert_eq!(listed[0].pages, 12);

        // A second entry is prepended.
        save_ai_entry(
            &index,
            &library,
            "translate",
            "C:/docs/report.pdf",
            "report.pdf",
            "deepseek-flash",
            12,
            5000,
            "tr",
            "## Page 1\n\nÇeviri",
            900,
        )
        .expect("save 2");
        let listed = list_ai_entries(&index);
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].kind, "translate");

        let text = read_ai_entry_text(&index, &listed[1].id).expect("read");
        assert!(text.contains("2026 results"));

        delete_ai_entry(&index, &listed[0].id, true).expect("delete");
        let listed = list_ai_entries(&index);
        assert_eq!(listed.len(), 1);

        clear_ai_entries(&index, true).expect("clear");
        assert!(list_ai_entries(&index).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_files_are_dropped_from_the_index() {
        let dir = temp_dir("ai-missing");
        let index = dir.join("ai-library.json");
        let library = dir.join("library");
        let entry = save_ai_entry(
            &index, &library, "ask", "C:/a.pdf", "a.pdf", "m", 1, 10, "", "answer text", 10,
        )
        .expect("save");
        std::fs::remove_file(&entry.file_path).expect("remove file");
        assert!(list_ai_entries(&index).is_empty(), "entries without files are filtered");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn operation_log_keeps_newest_first_and_can_be_cleared() {
        let dir = temp_dir("ops");
        let log = dir.join("operations.json");
        for index in 0..3 {
            append_operation(
                &log,
                OperationEntry {
                    id: String::new(),
                    created_at: 1_700_000_000 + index,
                    operation: "compress".into(),
                    input_path: format!("C:/in{index}.pdf"),
                    output_path: format!("C:/out{index}.pdf"),
                    page_count: Some(3),
                    input_bytes: Some(1000),
                    output_bytes: Some(400),
                    ok: true,
                    detail: Some("raster 150dpi".into()),
                },
            )
            .expect("append");
        }
        let entries = list_operations(&log);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].created_at, 1_700_000_002, "newest first");
        assert!(entries[0].id.starts_with("1700000002"));
        clear_operations(&log).expect("clear");
        assert!(list_operations(&log).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_names_are_slugified_and_timestamped() {
        let name = library_file_name("summary", "Rapor Şğü 2026.pdf", 1_700_000_000);
        assert!(name.starts_with("2023-11-14_"), "unix 1700000000 is 2023-11-14: {name}");
        assert!(name.contains("summary"));
        assert!(name.ends_with(".md"));
        assert!(!name.contains(' '));
        assert!(!name.contains('ş') || name.contains('-'), "non ascii characters are transliterated away");
    }
}
