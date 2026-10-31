//! DOCX provenance manifests (D7): a Word document that says what it was made
//! from.
//!
//! The C8 manifest for PDF lives in `pdfcore::provenance` and travels as an
//! embedded attachment plus an XMP packet. DOCX is a ZIP package, so the same
//! manifest is stored as one extra entry (`omnioffice/provenance.json`) and the
//! recorded document digest is computed over the package's entries in a
//! canonical order — name length, name, content length, content — with the
//! manifest entry left out. That digest does not depend on ZIP timestamps,
//! compression or entry order, so a document rewritten byte-for-byte by a ZIP
//! tool still verifies, while a changed word/document.xml does not.
//!
//! The manifest JSON is wire-compatible with the PDF manifest of C8
//! (`formatVersion`, `createdAt`, `appVersion`, `module`, `note`, `sources`,
//! `ai`, `documentSha256`), so the verification screen can present both
//! formats as one source chain.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{OfficeError, OfficeResult};
use crate::zip::{ZipLimits, ZipReader, ZipWriter};

/// Manifest format written and accepted by this build.
pub const FORMAT_VERSION: u32 = 1;

/// Where the manifest lives inside the DOCX package.
pub const MANIFEST_ENTRY: &str = "omnioffice/provenance.json";

/// One document the output was made from.
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceAi {
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub citations: u32,
}

/// What a document was made from, when and by which module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceManifest {
    pub format_version: u32,
    /// RFC 3339, UTC.
    pub created_at: String,
    pub app_version: String,
    /// The screen or pipeline that produced the output, e.g. `convert`.
    pub module: String,
    #[serde(default)]
    pub note: String,
    pub sources: Vec<ProvenanceSource>,
    #[serde(default)]
    pub ai: Option<ProvenanceAi>,
    /// SHA-256 of the package's entries with the manifest entry stripped.
    #[serde(default)]
    pub document_sha256: String,
}

/// What a DOCX reports about its own provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceReport {
    pub present: bool,
    /// The recorded digest matches the package minus the manifest entry.
    pub intact: bool,
    pub manifest: Option<ProvenanceManifest>,
    /// Digest recomputed while reading; empty when no manifest is present.
    pub actual_sha256: String,
    #[serde(default)]
    pub warnings: Vec<String>,
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn read_entries(bytes: &[u8]) -> OfficeResult<Vec<(String, Vec<u8>)>> {
    ZipReader::open(bytes.to_vec()).and_then(|zip| zip.read_all(ZipLimits::default()))
}

/// The canonical digest of a DOCX package: entries sorted by name, each
/// contributing its name length, name, content length and content.
///
/// The manifest entry is left out, so embedding or re-embedding never changes
/// the digest it records.
pub fn document_digest(entries: &[(String, Vec<u8>)]) -> String {
    let mut sorted: Vec<&(String, Vec<u8>)> = entries.iter().filter(|(name, _)| name != MANIFEST_ENTRY).collect();
    sorted.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    let mut hasher = Sha256::new();
    for (name, data) in sorted {
        hasher.update((name.len() as u64).to_le_bytes());
        hasher.update(name.as_bytes());
        hasher.update((data.len() as u64).to_le_bytes());
        hasher.update(data);
    }
    hex(&hasher.finalize())
}

/// Describes one source file the way a manifest records it.
pub fn source_from_bytes(file_name: &str, role: &str, bytes: &[u8]) -> ProvenanceSource {
    ProvenanceSource {
        file_name: file_name.to_string(),
        sha256: hex(&Sha256::digest(bytes)),
        size: bytes.len() as u64,
        role: role.to_string(),
    }
}

/// Reads and verifies the manifest, if the package carries one.
pub fn read_provenance(bytes: &[u8]) -> OfficeResult<ProvenanceReport> {
    let entries = read_entries(bytes)?;
    let Some((_, manifest_bytes)) = entries.iter().find(|(name, _)| name == MANIFEST_ENTRY) else {
        return Ok(ProvenanceReport {
            present: false,
            intact: false,
            manifest: None,
            actual_sha256: String::new(),
            warnings: Vec::new(),
        });
    };
    let manifest: ProvenanceManifest = serde_json::from_slice(manifest_bytes)
        .map_err(|error| OfficeError::corrupt(format!("DOCX provenance manifest JSON: {error}")))?;
    let actual_sha256 = document_digest(&entries);
    let mut warnings = Vec::new();
    if manifest.format_version > FORMAT_VERSION {
        warnings.push(format!(
            "the manifest was written by a newer format (v{}); this build reads v{FORMAT_VERSION}",
            manifest.format_version
        ));
    }
    if manifest.sources.is_empty() {
        warnings.push("the manifest records no source documents".to_string());
    }
    let intact = !manifest.document_sha256.is_empty() && manifest.document_sha256 == actual_sha256;
    Ok(ProvenanceReport { present: true, intact, manifest: Some(manifest), actual_sha256, warnings })
}

/// Embeds `manifest` into the DOCX package and returns the written bytes plus
/// the manifest with its digest filled in.
///
/// A previous manifest is replaced rather than stacked, so the digest always
/// describes the package a reader gets.
pub fn embed_provenance(bytes: &[u8], mut manifest: ProvenanceManifest) -> OfficeResult<(Vec<u8>, ProvenanceManifest)> {
    let mut entries = read_entries(bytes)?;
    entries.retain(|(name, _)| name != MANIFEST_ENTRY);
    manifest.format_version = FORMAT_VERSION;
    manifest.document_sha256 = document_digest(&entries);
    let json = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| OfficeError::internal(format!("DOCX provenance JSON: {error}")))?;
    entries.push((MANIFEST_ENTRY.to_string(), json));
    let mut writer = ZipWriter::new();
    for (name, data) in &entries {
        writer.add(name, data);
    }
    Ok((writer.finish(), manifest))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_docx() -> Vec<u8> {
        let mut writer = ZipWriter::new();
        writer.add_text(
            "[Content_Types].xml",
            r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#,
        );
        writer.add_text(
            "_rels/.rels",
            r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
        );
        writer.add_text(
            "word/document.xml",
            r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>OmniOffice</w:t></w:r></w:p></w:body></w:document>"#,
        );
        writer.finish()
    }

    fn manifest() -> ProvenanceManifest {
        ProvenanceManifest {
            format_version: FORMAT_VERSION,
            created_at: "2026-10-31T09:00:00Z".into(),
            app_version: "7.0.0".into(),
            module: "convert".into(),
            note: String::new(),
            sources: vec![source_from_bytes("dilekce.pdf", "source", b"pdf-bytes")],
            ai: None,
            document_sha256: String::new(),
        }
    }

    fn entries_of(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
        read_entries(bytes).unwrap()
    }

    #[test]
    fn a_package_without_a_manifest_reports_absent() {
        let report = read_provenance(&sample_docx()).unwrap();
        assert!(!report.present);
        assert!(!report.intact);
        assert!(report.manifest.is_none());
        assert_eq!(report.actual_sha256, "");
    }

    #[test]
    fn embedding_round_trips_the_manifest_and_the_digest() {
        let (bytes, written) = embed_provenance(&sample_docx(), manifest()).unwrap();
        assert!(!written.document_sha256.is_empty());
        // The manifest is a normal package entry, not a sidecar.
        assert!(entries_of(&bytes).iter().any(|(name, _)| name == MANIFEST_ENTRY));
        let report = read_provenance(&bytes).unwrap();
        assert!(report.present);
        assert!(report.intact, "the digest recorded on write must verify on read");
        assert_eq!(report.actual_sha256, written.document_sha256);
        let read = report.manifest.unwrap();
        assert_eq!(read.module, "convert");
        assert_eq!(read.app_version, "7.0.0");
        assert_eq!(read.sources.len(), 1);
        assert_eq!(read.sources[0].file_name, "dilekce.pdf");
        assert_eq!(read.sources[0].sha256, written.sources[0].sha256);
        assert!(report.warnings.is_empty(), "warnings: {:?}", report.warnings);
    }

    #[test]
    fn changing_a_part_after_the_manifest_is_detected() {
        let (bytes, written) = embed_provenance(&sample_docx(), manifest()).unwrap();
        // Rewrite the body the way an editor would, keeping our manifest entry:
        // only the digest check can catch the change.
        let mut entries = entries_of(&bytes);
        for (name, data) in entries.iter_mut() {
            if name == "word/document.xml" {
                *data = String::from_utf8_lossy(data).replace("OmniOffice", "Changed").into_bytes();
            }
        }
        let mut writer = ZipWriter::new();
        for (name, data) in &entries {
            writer.add(name, data);
        }
        let report = read_provenance(&writer.finish()).unwrap();
        assert!(report.present);
        assert!(!report.intact, "a changed body must break the digest");
        assert_ne!(report.actual_sha256, written.document_sha256);
        assert_eq!(report.manifest.unwrap().document_sha256, written.document_sha256);
    }

    #[test]
    fn re_embedding_replaces_the_manifest_instead_of_stacking_it() {
        let (once, first) = embed_provenance(&sample_docx(), manifest()).unwrap();
        let mut second = manifest();
        second.note = "ikinci".into();
        let (twice, written) = embed_provenance(&once, second).unwrap();
        let manifest_entries = entries_of(&twice).iter().filter(|(name, _)| name == MANIFEST_ENTRY).count();
        assert_eq!(manifest_entries, 1, "exactly one manifest entry");
        let report = read_provenance(&twice).unwrap();
        assert!(report.intact);
        assert_eq!(report.manifest.unwrap().note, "ikinci");
        assert_eq!(report.actual_sha256, written.document_sha256);
        assert_eq!(first.document_sha256, written.document_sha256, "the document part did not change");
    }

    #[test]
    fn the_digest_does_not_depend_on_zip_order_or_timestamps() {
        let first = sample_docx();
        let mut reversed = ZipWriter::new();
        let mut entries = entries_of(&first);
        entries.reverse();
        for (name, data) in &entries {
            reversed.add(name, data);
        }
        let reordered = reversed.finish();
        assert_eq!(document_digest(&entries_of(&first)), document_digest(&entries_of(&reordered)));
        // And the manifest still verifies after a reorder-only rewrite.
        let (bytes, _) = embed_provenance(&first, manifest()).unwrap();
        let mut body = entries_of(&bytes);
        body.reverse();
        let mut writer = ZipWriter::new();
        for (name, data) in &body {
            writer.add(name, data);
        }
        assert!(read_provenance(&writer.finish()).unwrap().intact);
    }

    #[test]
    fn a_newer_manifest_format_is_reported_as_a_warning() {
        let (bytes, _) = embed_provenance(&sample_docx(), manifest()).unwrap();
        let mut entries = entries_of(&bytes);
        for (name, data) in entries.iter_mut() {
            if name == MANIFEST_ENTRY {
                let mut value: serde_json::Value = serde_json::from_slice(data).unwrap();
                value["formatVersion"] = serde_json::json!(FORMAT_VERSION + 1);
                *data = serde_json::to_vec(&value).unwrap();
            }
        }
        let mut writer = ZipWriter::new();
        for (name, data) in &entries {
            writer.add(name, data);
        }
        let report = read_provenance(&writer.finish()).unwrap();
        assert!(report.present);
        assert!(report.warnings.iter().any(|warning| warning.contains("newer format")));
    }
}
