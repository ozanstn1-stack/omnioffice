//! Provenance manifest commands (C8): write and read the record that says what
//! a generated PDF was made from. The manifest itself is
//! `pdfcore::provenance`; this module only resolves capability tokens, hashes
//! sources and writes the output atomically.

use pdfcore::provenance::{self, ProvenanceAi, ProvenanceManifest, ProvenanceReport, FORMAT_VERSION};
use pdfcore::PdfError;
use serde::{Deserialize, Serialize};

use crate::commands::{OpResult, OutputSpec};

/// Safety cap for the document and each source read.
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

async fn run_blocking<T, F>(work: F) -> Result<T, PdfError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, PdfError> + Send + 'static,
{
    let _permit = crate::concurrency::acquire().await;
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| PdfError::Internal(format!("worker thread failed: {e}")))?
}

/// One source document: a path or file token, its display name and role.
#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceSourceInput {
    pub path: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
}

#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceAiInput {
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub citations: u32,
}

#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceWriteRequest {
    /// The document to add a manifest to (a path or file token).
    pub input: String,
    pub output: OutputSpec,
    /// The screen or pipeline that produced the output, e.g. `manual`.
    pub module: String,
    #[serde(default)]
    pub note: String,
    /// The running app version, shown in the manifest.
    pub app_version: String,
    #[serde(default)]
    pub sources: Vec<ProvenanceSourceInput>,
    #[serde(default)]
    pub ai: Option<ProvenanceAiInput>,
}

#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceWriteResult {
    pub result: OpResult,
    pub manifest: ProvenanceManifest,
}

fn display_name(raw: &str, name: Option<&str>, resolved: &std::path::Path) -> String {
    name.map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .or_else(|| resolved.file_name().map(|name| name.to_string_lossy().into_owned()))
        .unwrap_or_else(|| raw.to_string())
}

fn rfc3339_utc(now: time::OffsetDateTime) -> String {
    now.format(&time::format_description::well_known::Rfc3339).unwrap_or_else(|_| String::new())
}

/// Writes a copy of the input with the manifest embedded and reports the digest
/// it recorded.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn pdf_provenance_write(request: ProvenanceWriteRequest) -> Result<ProvenanceWriteResult, PdfError> {
    run_blocking(move || {
        let input = crate::paths::read_input_file(&request.input, MAX_FILE_BYTES)?;
        let mut sources = Vec::with_capacity(request.sources.len());
        for source in &request.sources {
            let resolved = crate::paths::input_file(&source.path)?.into_path_buf();
            let bytes = crate::paths::read_input_file(&source.path, MAX_FILE_BYTES)?;
            let role = source.role.as_deref().map(str::trim).filter(|role| !role.is_empty()).unwrap_or("source");
            let name = display_name(&source.path, source.name.as_deref(), &resolved);
            sources.push(provenance::source_from_bytes(&name, role, &bytes));
        }
        let ai = request.ai.map(|ai| ProvenanceAi { provider: ai.provider, model: ai.model, citations: ai.citations });
        let manifest = ProvenanceManifest {
            format_version: FORMAT_VERSION,
            created_at: rfc3339_utc(time::OffsetDateTime::now_utc()),
            app_version: request.app_version,
            module: request.module,
            note: request.note,
            sources,
            ai,
            document_sha256: String::new(),
        };
        let (output_bytes, manifest) = provenance::embed_provenance(&input, manifest)?;
        let (output_path, policy) = request.output.resolve()?;
        let path = pdfcore::docutil::resolve_output_path(&output_path, policy)?;
        pdfcore::docutil::write_bytes_atomic(&path, &output_bytes)?;
        let page_count = lopdf::Document::load_mem(&output_bytes).ok().map(|doc| doc.get_pages().len() as u32);
        Ok(ProvenanceWriteResult {
            result: OpResult {
                path: path.display().to_string(),
                page_count,
                original_bytes: Some(input.len() as u64),
                output_bytes: Some(output_bytes.len() as u64),
                reduction: None,
                message: Some(manifest.module.clone()),
            },
            manifest,
        })
    })
    .await
}

/// Reads the manifest of a document, if it carries one, and reports whether the
/// document still matches the digest recorded then.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn pdf_provenance_read(path: String) -> Result<ProvenanceReport, PdfError> {
    run_blocking(move || {
        let bytes = crate::paths::read_input_file(&path, MAX_FILE_BYTES)?;
        provenance::read_provenance(&bytes)
    })
    .await
}
