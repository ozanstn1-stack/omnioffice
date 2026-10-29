//! Evidence bundle commands (C3): build a bundle from exhibit files, verify a
//! bundle on reopen and match original files to its exhibits. The bundle
//! itself is `pdfcore::evidence`.

use pdfcore::evidence::{EvidenceDetails, EvidenceFile, EvidenceManifest, EvidenceVerification};
use pdfcore::PdfError;
use serde::{Deserialize, Serialize};

use crate::commands::{OpResult, OutputSpec};

/// Safety cap for one exhibit or bundle read.
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

/// One exhibit: a path or file token, its label and, for a token, its name.
#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceExhibit {
    pub path: String,
    pub label: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceBundleRequest {
    pub exhibits: Vec<EvidenceExhibit>,
    pub details: EvidenceDetails,
    pub output: OutputSpec,
}

#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceBundleResult {
    pub result: OpResult,
    pub manifest: EvidenceManifest,
}

/// An original file checked against a bundle's manifest.
#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceOriginalMatch {
    pub path: String,
    pub sha256: String,
    /// The exhibit number whose digest matches, if any.
    pub exhibit: Option<u32>,
}

fn display_name(raw: &str, name: Option<&str>, resolved: &std::path::Path) -> String {
    name.map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .or_else(|| resolved.file_name().map(|name| name.to_string_lossy().into_owned()))
        .unwrap_or_else(|| raw.to_string())
}

/// Builds the bundle (cover, index, stamped exhibits, embedded manifest) and
/// writes it to the output.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn evidence_bundle_build(request: EvidenceBundleRequest) -> Result<EvidenceBundleResult, PdfError> {
    run_blocking(move || {
        let (output, policy) = request.output.resolve()?;
        let mut files = Vec::with_capacity(request.exhibits.len());
        for exhibit in &request.exhibits {
            let resolved = crate::paths::input_file(&exhibit.path)?.into_path_buf();
            files.push(EvidenceFile {
                label: exhibit.label.clone(),
                file_name: display_name(&exhibit.path, exhibit.name.as_deref(), &resolved),
                bytes: crate::paths::read_input_file(&exhibit.path, MAX_FILE_BYTES)?,
            });
        }
        let original_bytes = files.iter().map(|file| file.bytes.len() as u64).sum();
        let (bundle, manifest) = pdfcore::evidence::build_evidence_bundle(files, &request.details)?;
        let path = pdfcore::docutil::resolve_output_path(&output, policy)?;
        pdfcore::docutil::write_bytes_atomic(&path, &bundle)?;
        Ok(EvidenceBundleResult {
            result: OpResult {
                path: path.display().to_string(),
                page_count: Some(manifest.total_pages),
                original_bytes: Some(original_bytes),
                output_bytes: Some(bundle.len() as u64),
                reduction: None,
                message: Some(manifest.id.clone()),
            },
            manifest,
        })
    })
    .await
}

/// Reads a bundle's manifest and checks the bundle against it.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn evidence_bundle_verify(path: String) -> Result<EvidenceVerification, PdfError> {
    run_blocking(move || {
        let bytes = crate::paths::read_input_file(&path, MAX_FILE_BYTES)?;
        pdfcore::evidence::verify_evidence_bundle(&bytes)
    })
    .await
}

/// Hashes each original and finds the exhibit it was submitted as.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn evidence_match_originals(
    bundle: String,
    originals: Vec<String>,
) -> Result<Vec<EvidenceOriginalMatch>, PdfError> {
    run_blocking(move || {
        let bytes = crate::paths::read_input_file(&bundle, MAX_FILE_BYTES)?;
        let manifest = pdfcore::evidence::verify_evidence_bundle(&bytes)?
            .manifest
            .ok_or_else(|| PdfError::InvalidInput("the file is not an evidence bundle".into()))?;
        originals
            .into_iter()
            .map(|path| {
                let original = crate::paths::read_input_file(&path, MAX_FILE_BYTES)?;
                Ok(EvidenceOriginalMatch {
                    sha256: pdfcore::certificate::digest(&original).sha256,
                    exhibit: pdfcore::evidence::match_original(&manifest, &original),
                    path,
                })
            })
            .collect()
    })
    .await
}
