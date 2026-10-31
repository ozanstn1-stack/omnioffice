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

// ---------------------------------------------------------------------------
// DOCX manifests and the unified source chain (D7)
// ---------------------------------------------------------------------------

use std::path::{Path, PathBuf};

/// One link in a document's derivation chain (D7): a source, the
/// transformation step, or the output itself.
#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChainLink {
    /// `source`, `transform` or `output`.
    pub kind: String,
    /// What the link is, e.g. the module name, a role or a file name.
    pub label: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub role: String,
}

/// The whole derivation chain of a PDF or DOCX (D7).
#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceChainReport {
    /// `pdf`, `docx`, or empty when neither manifest container is recognised.
    pub format: String,
    pub present: bool,
    pub intact: bool,
    /// `intact`, `changed`, `absent` or `unsupported`.
    pub verdict: String,
    /// Digest recomputed from the document now.
    pub output_sha256: String,
    /// Digest the manifest recorded when it was written.
    pub recorded_sha256: String,
    pub module: String,
    pub app_version: String,
    pub created_at: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub ai: Option<ProvenanceAi>,
    pub links: Vec<ChainLink>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

enum ProvenanceFormat {
    Pdf,
    Docx,
}

fn sniff_format(bytes: &[u8]) -> Option<ProvenanceFormat> {
    if bytes.starts_with(b"%PDF") {
        Some(ProvenanceFormat::Pdf)
    } else if bytes.starts_with(b"PK\x03\x04") {
        Some(ProvenanceFormat::Docx)
    } else {
        None
    }
}

fn office_error(error: officecore::error::OfficeError) -> PdfError {
    PdfError::InvalidInput(format!("{error}"))
}

fn pdf_manifest_to_chain(
    format: &str,
    path_display: &str,
    output_bytes: u64,
    report: ProvenanceReport,
) -> SourceChainReport {
    let mut links = Vec::new();
    let mut recorded = String::new();
    let (module, app_version, created_at, note, ai) = match &report.manifest {
        Some(manifest) => {
            for source in &manifest.sources {
                links.push(ChainLink {
                    kind: "source".into(),
                    label: if source.role.is_empty() { source.file_name.clone() } else { source.role.clone() },
                    sha256: source.sha256.clone(),
                    size: Some(source.size),
                    role: source.role.clone(),
                });
            }
            links.push(ChainLink {
                kind: "transform".into(),
                label: manifest.module.clone(),
                sha256: String::new(),
                size: None,
                role: String::new(),
            });
            recorded = manifest.document_sha256.clone();
            (
                manifest.module.clone(),
                manifest.app_version.clone(),
                manifest.created_at.clone(),
                manifest.note.clone(),
                manifest.ai.clone(),
            )
        }
        None => (String::new(), String::new(), String::new(), String::new(), None),
    };
    if report.present {
        links.push(ChainLink {
            kind: "output".into(),
            label: path_display.to_string(),
            sha256: report.actual_sha256.clone(),
            size: Some(output_bytes),
            role: "output".into(),
        });
    }
    let verdict = if !report.present {
        "absent"
    } else if report.intact {
        "intact"
    } else {
        "changed"
    };
    SourceChainReport {
        format: format.into(),
        present: report.present,
        intact: report.intact,
        verdict: verdict.into(),
        output_sha256: report.actual_sha256,
        recorded_sha256: recorded,
        module,
        app_version,
        created_at,
        note,
        ai,
        links,
        warnings: report.warnings,
    }
}

/// Reads the derivation chain of a PDF or DOCX (D7): every source with its
/// SHA-256, the transformation step, and the output digest that was checked.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn provenance_read_chain(path: String) -> Result<SourceChainReport, PdfError> {
    run_blocking(move || {
        let bytes = crate::paths::read_input_file(&path, MAX_FILE_BYTES)?;
        let display = Path::new(&path).file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or(path);
        match sniff_format(&bytes) {
            Some(ProvenanceFormat::Pdf) => {
                let report = provenance::read_provenance(&bytes)?;
                Ok(pdf_manifest_to_chain("pdf", &display, bytes.len() as u64, report))
            }
            Some(ProvenanceFormat::Docx) => {
                let report = officecore::docx_provenance::read_provenance(&bytes).map_err(office_error)?;
                let mut chain = pdf_manifest_to_chain(
                    "docx",
                    &display,
                    bytes.len() as u64,
                    ProvenanceReport {
                        present: report.present,
                        intact: report.intact,
                        manifest: report.manifest.map(|manifest| ProvenanceManifest {
                            format_version: manifest.format_version,
                            created_at: manifest.created_at,
                            app_version: manifest.app_version,
                            module: manifest.module,
                            note: manifest.note,
                            sources: manifest
                                .sources
                                .into_iter()
                                .map(|source| provenance::ProvenanceSource {
                                    file_name: source.file_name,
                                    sha256: source.sha256,
                                    size: source.size,
                                    role: source.role,
                                })
                                .collect(),
                            ai: manifest.ai.map(|ai| ProvenanceAi {
                                provider: ai.provider,
                                model: ai.model,
                                citations: ai.citations,
                            }),
                            document_sha256: manifest.document_sha256,
                        }),
                        actual_sha256: report.actual_sha256,
                        warnings: report.warnings,
                    },
                );
                chain.format = "docx".into();
                Ok(chain)
            }
            None => Ok(SourceChainReport {
                format: String::new(),
                present: false,
                intact: false,
                verdict: "unsupported".into(),
                output_sha256: String::new(),
                recorded_sha256: String::new(),
                module: String::new(),
                app_version: String::new(),
                created_at: String::new(),
                note: String::new(),
                ai: None,
                links: Vec::new(),
                warnings: vec!["the file is neither a PDF nor a DOCX package".into()],
            }),
        }
    })
    .await
}

fn manifest_for_write(
    request: &ProvenanceWriteRequest,
    sources: Vec<provenance::ProvenanceSource>,
) -> ProvenanceManifest {
    let ai = request.ai.as_ref().map(|ai| ProvenanceAi {
        provider: ai.provider.clone(),
        model: ai.model.clone(),
        citations: ai.citations,
    });
    ProvenanceManifest {
        format_version: FORMAT_VERSION,
        created_at: rfc3339_utc(time::OffsetDateTime::now_utc()),
        app_version: request.app_version.clone(),
        module: request.module.clone(),
        note: request.note.clone(),
        sources,
        ai,
        document_sha256: String::new(),
    }
}

/// Writes a copy of a PDF or DOCX with a provenance manifest embedded (D7).
/// The container is sniffed from the input bytes; a signed PDF is refused the
/// same way the PDF-only command refuses it.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn provenance_write(request: ProvenanceWriteRequest) -> Result<ProvenanceWriteResult, PdfError> {
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
        let (output_path, policy) = request.output.resolve()?;
        let path = pdfcore::docutil::resolve_output_path(&output_path, policy)?;
        let manifest = manifest_for_write(&request, sources);
        let (output_bytes, manifest, page_count) = match sniff_format(&input) {
            Some(ProvenanceFormat::Pdf) => {
                let (bytes, written) = provenance::embed_provenance(&input, manifest)?;
                let pages = lopdf::Document::load_mem(&bytes).ok().map(|doc| doc.get_pages().len() as u32);
                (bytes, written, pages)
            }
            Some(ProvenanceFormat::Docx) => {
                let docx_manifest = officecore::docx_provenance::ProvenanceManifest {
                    format_version: manifest.format_version,
                    created_at: manifest.created_at.clone(),
                    app_version: manifest.app_version.clone(),
                    module: manifest.module.clone(),
                    note: manifest.note.clone(),
                    sources: manifest
                        .sources
                        .iter()
                        .map(|source| officecore::docx_provenance::ProvenanceSource {
                            file_name: source.file_name.clone(),
                            sha256: source.sha256.clone(),
                            size: source.size,
                            role: source.role.clone(),
                        })
                        .collect(),
                    ai: manifest.ai.as_ref().map(|ai| officecore::docx_provenance::ProvenanceAi {
                        provider: ai.provider.clone(),
                        model: ai.model.clone(),
                        citations: ai.citations,
                    }),
                    document_sha256: String::new(),
                };
                let (bytes, written) =
                    officecore::docx_provenance::embed_provenance(&input, docx_manifest).map_err(office_error)?;
                let converted = ProvenanceManifest {
                    format_version: written.format_version,
                    created_at: written.created_at,
                    app_version: written.app_version,
                    module: written.module,
                    note: written.note,
                    sources: written
                        .sources
                        .into_iter()
                        .map(|source| provenance::ProvenanceSource {
                            file_name: source.file_name,
                            sha256: source.sha256,
                            size: source.size,
                            role: source.role,
                        })
                        .collect(),
                    ai: written.ai.map(|ai| ProvenanceAi {
                        provider: ai.provider,
                        model: ai.model,
                        citations: ai.citations,
                    }),
                    document_sha256: written.document_sha256,
                };
                (bytes, converted, None)
            }
            None => {
                return Err(PdfError::InvalidInput("the input is neither a PDF nor a DOCX package".into()));
            }
        };
        pdfcore::docutil::write_bytes_atomic(&path, &output_bytes)?;
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

fn source_paths(paths: &[PathBuf]) -> Vec<provenance::ProvenanceSource> {
    paths
        .iter()
        .filter_map(|path| {
            let bytes = std::fs::read(path).ok()?;
            let name = path.file_name().map(|name| name.to_string_lossy().into_owned())?;
            Some(provenance::source_from_bytes(&name, "source", &bytes))
        })
        .collect()
}

/// Embeds a PDF manifest in a produced file, best effort: a producer's output
/// must never fail because its provenance record could not be written.
pub(crate) fn attach_pdf_manifest(path: &Path, module: &str, sources: &[PathBuf]) {
    let Ok(bytes) = std::fs::read(path) else { return };
    let manifest = ProvenanceManifest {
        format_version: FORMAT_VERSION,
        created_at: rfc3339_utc(time::OffsetDateTime::now_utc()),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        module: module.to_string(),
        note: String::new(),
        sources: source_paths(sources),
        ai: None,
        document_sha256: String::new(),
    };
    if let Ok((output, _)) = provenance::embed_provenance(&bytes, manifest) {
        let _ = pdfcore::docutil::write_bytes_atomic(path, &output);
    }
}

/// Embeds a DOCX manifest in a produced file, best effort (D7).
pub(crate) fn attach_docx_manifest(path: &Path, module: &str, sources: &[PathBuf]) {
    let Ok(bytes) = std::fs::read(path) else { return };
    let manifest = officecore::docx_provenance::ProvenanceManifest {
        format_version: officecore::docx_provenance::FORMAT_VERSION,
        created_at: rfc3339_utc(time::OffsetDateTime::now_utc()),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        module: module.to_string(),
        note: String::new(),
        sources: source_paths(sources)
            .into_iter()
            .map(|source| officecore::docx_provenance::ProvenanceSource {
                file_name: source.file_name,
                sha256: source.sha256,
                size: source.size,
                role: source.role,
            })
            .collect(),
        ai: None,
        document_sha256: String::new(),
    };
    if let Ok((output, _)) = officecore::docx_provenance::embed_provenance(&bytes, manifest) {
        let _ = pdfcore::docutil::write_bytes_atomic(path, &output);
    }
}

#[cfg(test)]
mod chain_tests {
    use super::*;

    fn manifest() -> ProvenanceManifest {
        ProvenanceManifest {
            format_version: FORMAT_VERSION,
            created_at: "2026-10-31T09:00:00Z".into(),
            app_version: "7.0.0".into(),
            module: "merge".into(),
            note: String::new(),
            sources: vec![
                provenance::source_from_bytes("a.pdf", "source", b"aaa"),
                provenance::source_from_bytes("b.pdf", "source", b"bbb"),
            ],
            ai: None,
            document_sha256: "recorded".into(),
        }
    }

    #[test]
    fn the_sniffer_separates_pdf_docx_and_unknown_files() {
        assert!(matches!(sniff_format(b"%PDF-1.7"), Some(ProvenanceFormat::Pdf)));
        assert!(matches!(sniff_format(b"PK\x03\x04rest"), Some(ProvenanceFormat::Docx)));
        assert!(sniff_format(b"plain text").is_none());
    }

    #[test]
    fn the_chain_lists_sources_then_the_transformation_then_the_output() {
        let report = ProvenanceReport {
            present: true,
            intact: true,
            manifest: Some(manifest()),
            actual_sha256: "recorded".into(),
            warnings: Vec::new(),
        };
        let chain = pdf_manifest_to_chain("pdf", "merged.pdf", 42, report);
        assert_eq!(chain.verdict, "intact");
        assert_eq!(chain.format, "pdf");
        assert_eq!(chain.links.len(), 4, "two sources, the step and the output");
        let kinds: Vec<&str> = chain.links.iter().map(|link| link.kind.as_str()).collect();
        assert_eq!(kinds, ["source", "source", "transform", "output"]);
        assert_eq!(chain.links[0].label, "a.pdf");
        assert_eq!(chain.links[2].label, "merge");
        assert_eq!(chain.links[3].sha256, "recorded");
        assert_eq!(chain.links[3].size, Some(42));
        assert_eq!(chain.recorded_sha256, "recorded");
        assert_eq!(chain.module, "merge");
    }

    #[test]
    fn a_changed_document_says_changed_and_an_absent_manifest_says_absent() {
        let changed = ProvenanceReport {
            present: true,
            intact: false,
            manifest: Some(manifest()),
            actual_sha256: "recomputed".into(),
            warnings: Vec::new(),
        };
        let chain = pdf_manifest_to_chain("docx", "out.docx", 9, changed);
        assert_eq!(chain.verdict, "changed");
        assert!(!chain.intact);
        assert_eq!(chain.output_sha256, "recomputed");
        assert_eq!(chain.recorded_sha256, "recorded");

        let absent = pdf_manifest_to_chain(
            "pdf",
            "plain.pdf",
            7,
            ProvenanceReport {
                present: false,
                intact: false,
                manifest: None,
                actual_sha256: String::new(),
                warnings: Vec::new(),
            },
        );
        assert_eq!(absent.verdict, "absent");
        assert!(absent.links.is_empty());
    }
}
