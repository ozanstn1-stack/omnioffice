//! Provenance manifest (C8): a document that says what it was made from.
//!
//! The manifest is a JSON record plus an XMP packet (see [`crate::attachment`]),
//! so it travels with the file and is readable without the app. Every source
//! keeps its file name, size and SHA-256; the recorded digest is the document
//! with the manifest stripped, which is exactly what [`read_provenance`]
//! recomputes, so a document changed after the manifest was written is
//! reported as not intact.
//!
//! Embedding is refused on a signed document: rewriting the file would break
//! every signature, and the verification screen would then report the file as
//! changed after signing.

use serde::{Deserialize, Serialize};

use crate::attachment::{self, RecordKind};
use crate::certificate::{digest, FileDigest};
use crate::error::{PdfError, PdfResult};
use crate::ltv::signature_count;

/// Manifest format written and accepted by this build.
pub const FORMAT_VERSION: u32 = 1;

/// Where the manifest lives inside the document.
const KIND: RecordKind = RecordKind {
    file_name: "omnioffice-provenance.json",
    description: "OmniOffice provenance manifest",
    xmp_namespace: "https://omnioffice.app/ns/provenance/1.0/",
};

/// One document the output was made from.
#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceSource {
    pub file_name: String,
    pub sha256: String,
    pub size: u64,
    /// What the source contributed, e.g. `source`, `merge`, `attachment`.
    #[serde(default)]
    pub role: String,
}

/// The AI provider behind a generated document, when one was involved.
#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceAi {
    pub provider: String,
    pub model: String,
    /// How many verified citations the output carried, when the producer knows.
    #[serde(default)]
    pub citations: u32,
}

/// What a document was made from, when and by which module.
#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceManifest {
    pub format_version: u32,
    /// RFC 3339, UTC.
    pub created_at: String,
    pub app_version: String,
    /// The screen or pipeline that produced the output, e.g. `manual`.
    pub module: String,
    #[serde(default)]
    pub note: String,
    pub sources: Vec<ProvenanceSource>,
    #[serde(default)]
    pub ai: Option<ProvenanceAi>,
    /// SHA-256 of the document with the manifest stripped.
    #[serde(default)]
    pub document_sha256: String,
}

/// What a document reports about its own provenance.
#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceReport {
    pub present: bool,
    /// The recorded digest matches the document minus the manifest.
    pub intact: bool,
    pub manifest: Option<ProvenanceManifest>,
    /// Digest recomputed while reading; empty when no manifest is present.
    pub actual_sha256: String,
    #[serde(default)]
    pub warnings: Vec<String>,
}

/// Hashes a source file the way the manifest records it.
pub fn source_from_bytes(file_name: &str, role: &str, bytes: &[u8]) -> ProvenanceSource {
    let FileDigest { sha256, size } = digest(bytes);
    ProvenanceSource { file_name: file_name.to_string(), sha256, size, role: role.to_string() }
}

fn embed_manifest(pdf: &[u8], manifest: &ProvenanceManifest) -> PdfResult<Vec<u8>> {
    let json =
        serde_json::to_vec_pretty(manifest).map_err(|error| PdfError::Internal(format!("provenance json: {error}")))?;
    let xmp = attachment::xmp_packet(
        KIND.xmp_namespace,
        &[
            ("Module", manifest.module.clone()),
            ("CreatedAt", manifest.created_at.clone()),
            ("AppVersion", manifest.app_version.clone()),
            ("SourceCount", manifest.sources.len().to_string()),
            ("DocumentSha256", manifest.document_sha256.clone()),
        ],
    );
    attachment::embed_record(pdf, &KIND, json, xmp)
}

/// Embeds `manifest` into `pdf` and returns the written bytes plus the manifest
/// with its digest filled in.
///
/// A previous manifest is replaced rather than stacked, so the digest always
/// describes the document a reader gets. A signed document is refused: any
/// rewrite would break its signatures.
pub fn embed_provenance(pdf: &[u8], mut manifest: ProvenanceManifest) -> PdfResult<(Vec<u8>, ProvenanceManifest)> {
    if signature_count(pdf) > 0 {
        return Err(PdfError::InvalidInput(
            "the document carries a signature; adding a manifest would break it. Use an unsigned copy.".into(),
        ));
    }
    // Start from the document without any manifest of ours, so re-embedding an
    // already manifested file neither stacks records nor changes the digest.
    let base = attachment::strip_record(pdf, &KIND)?;
    manifest.format_version = FORMAT_VERSION;
    manifest.document_sha256 = String::new();
    let probe = embed_manifest(&base, &manifest)?;
    manifest.document_sha256 = digest(&attachment::strip_record(&probe, &KIND)?).sha256;
    let output = embed_manifest(&base, &manifest)?;
    Ok((output, manifest))
}

fn absent() -> ProvenanceReport {
    ProvenanceReport {
        present: false,
        intact: false,
        manifest: None,
        actual_sha256: String::new(),
        warnings: Vec::new(),
    }
}

/// Reads and verifies the manifest, if the document carries one.
pub fn read_provenance(pdf: &[u8]) -> PdfResult<ProvenanceReport> {
    let doc = lopdf::Document::load_mem(pdf).map_err(|error| PdfError::InvalidPdf(format!("{error}")))?;
    let Some(json) = attachment::read_record(&doc, &KIND) else {
        return Ok(absent());
    };
    drop(doc);
    let manifest: ProvenanceManifest = serde_json::from_slice(&json)
        .map_err(|error| PdfError::InvalidPdf(format!("provenance manifest json: {error}")))?;
    let actual_sha256 = digest(&attachment::strip_record(pdf, &KIND)?).sha256;
    let mut warnings = Vec::new();
    if manifest.format_version > FORMAT_VERSION {
        warnings.push(format!(
            "the manifest was written by a newer format (v{}); this build reads v{FORMAT_VERSION}",
            manifest.format_version
        ));
    }
    let intact = !manifest.document_sha256.is_empty() && manifest.document_sha256 == actual_sha256;
    Ok(ProvenanceReport { present: true, intact, manifest: Some(manifest), actual_sha256, warnings })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Dictionary, Document, Object, Stream};

    fn sample_pdf() -> Vec<u8> {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let content_id = doc.add_object(Stream::new(Dictionary::new(), b"BT ET".to_vec()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![0.into(), 0.into(), 200.into(), 200.into()]),
            "Contents" => Object::Reference(content_id),
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => Object::Array(vec![Object::Reference(page_id)]),
                "Count" => 1,
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        });
        doc.trailer.set("Root", Object::Reference(catalog_id));
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    fn manifest() -> ProvenanceManifest {
        ProvenanceManifest {
            format_version: FORMAT_VERSION,
            created_at: "2026-10-29T20:00:00Z".into(),
            app_version: "6.6.0".into(),
            module: "manual".into(),
            note: String::new(),
            sources: vec![source_from_bytes("dilekce.pdf", "merge", b"source-bytes")],
            ai: None,
            document_sha256: String::new(),
        }
    }

    #[test]
    fn a_document_without_a_manifest_reports_absent() {
        let report = read_provenance(&sample_pdf()).unwrap();
        assert!(!report.present);
        assert!(report.manifest.is_none());
        assert_eq!(report.actual_sha256, "");
    }

    #[test]
    fn embedding_round_trips_the_manifest_and_the_digest() {
        let (bytes, written) = embed_provenance(&sample_pdf(), manifest()).unwrap();
        assert!(!written.document_sha256.is_empty());
        let report = read_provenance(&bytes).unwrap();
        assert!(report.present);
        assert!(report.intact, "the digest recorded on write must verify on read");
        assert_eq!(report.actual_sha256, written.document_sha256);
        let read = report.manifest.unwrap();
        assert_eq!(read.module, "manual");
        assert_eq!(read.app_version, "6.6.0");
        assert_eq!(read.sources.len(), 1);
        assert_eq!(read.sources[0].file_name, "dilekce.pdf");
        assert_eq!(read.sources[0].sha256, written.sources[0].sha256);
    }

    #[test]
    fn changing_the_document_after_the_manifest_is_detected() {
        let (bytes, _) = embed_provenance(&sample_pdf(), manifest()).unwrap();
        // Change the page content the way an editor would: the record is still
        // found, so only the digest check can catch the change.
        let mut doc = Document::load_mem(&bytes).unwrap();
        let pages = doc.get_pages();
        let page_id = *pages.values().next().unwrap();
        let contents_id = doc.get_dictionary(page_id).unwrap().get(b"Contents").unwrap().as_reference().unwrap();
        doc.get_object_mut(contents_id).unwrap().as_stream_mut().unwrap().content = b"BT (changed) ET".to_vec();
        let mut changed = Vec::new();
        doc.save_to(&mut changed).unwrap();
        let report = read_provenance(&changed).unwrap();
        assert!(report.present);
        assert!(!report.intact);
        assert_ne!(report.actual_sha256, report.manifest.unwrap().document_sha256);
    }

    #[test]
    fn re_embedding_replaces_the_manifest_instead_of_stacking_it() {
        let (once, _) = embed_provenance(&sample_pdf(), manifest()).unwrap();
        let mut second = manifest();
        second.note = "ikinci".into();
        let (twice, _) = embed_provenance(&once, second).unwrap();
        let report = read_provenance(&twice).unwrap();
        assert!(report.intact);
        assert_eq!(report.manifest.unwrap().note, "ikinci");
    }

    #[test]
    fn an_ai_source_and_citations_survive_the_round_trip() {
        let mut input = manifest();
        input.ai = Some(ProvenanceAi { provider: "deepseek".into(), model: "deepseek-flash".into(), citations: 3 });
        let (bytes, _) = embed_provenance(&sample_pdf(), input).unwrap();
        let read = read_provenance(&bytes).unwrap().manifest.unwrap();
        let ai = read.ai.unwrap();
        assert_eq!(ai.provider, "deepseek");
        assert_eq!(ai.model, "deepseek-flash");
        assert_eq!(ai.citations, 3);
    }

    #[test]
    fn a_newer_manifest_format_is_read_with_a_warning() {
        // Simulate a future writer: write the record by hand with a version
        // this build does not know, still recording the correct digest.
        let base = attachment::strip_record(&sample_pdf(), &KIND).unwrap();
        let mut future = manifest();
        future.format_version = FORMAT_VERSION + 1;
        future.document_sha256 = String::new();
        let probe = embed_manifest(&base, &future).unwrap();
        future.document_sha256 = digest(&attachment::strip_record(&probe, &KIND).unwrap()).sha256;
        let bytes = embed_manifest(&base, &future).unwrap();
        let report = read_provenance(&bytes).unwrap();
        assert_eq!(report.warnings.len(), 1);
        assert!(report.warnings[0].contains("newer format"));
        assert!(report.intact);
    }
}
