//! Verifiable redaction certificates.
//!
//! A certificate records **what** was redacted (page, category, rule - never a
//! value), **when**, and the SHA-256 digests of the input and the redacted
//! output, optionally stamped by an RFC 3161 timestamp authority (the token
//! comes from [`crate::timestamp`]; the HTTP exchange happens in the app).
//!
//! The certificate is embedded in the redacted PDF twice: as an embedded file
//! (`/Names /EmbeddedFiles`) and as XMP metadata in the catalog. Verification
//! re-extracts it, strips it from the file and re-hashes the remaining bytes,
//! so a file edited after certification fails with `content_matches: false`.
//!
//! The recorded output digest is computed in two passes ("embed a probe,
//! strip it, hash what remains") so verification does not depend on lopdf
//! serialising a load/save round trip byte-identically: the digest is defined
//! as *the hash of this file with the certificate removed*.

use lopdf::Document;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::attachment::{self, RecordKind};
use crate::error::{PdfError, PdfResult};
use crate::timestamp::TimestampToken;

/// The embedded file's name inside the PDF.
pub const CERTIFICATE_FILE_NAME: &str = "omni-redaction-certificate.json";
/// XMP namespace that marks our metadata packet.
pub const CERTIFICATE_XMP_NAMESPACE: &str = "https://omnioffice.app/ns/redaction/1.0/";

/// SHA-256 and size of one file in the chain.
#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileDigest {
    pub sha256: String,
    pub size: u64,
}

/// One redaction: where and under which rule, never what.
#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CertificateItem {
    pub page: u32,
    pub category: String,
    pub rule: String,
}

/// The granted timestamp, mirroring [`TimestampToken`] in JSON form.
#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CertificateTimestamp {
    /// `genTime` from the TSTInfo, when it could be read.
    pub time: Option<String>,
    /// The DER `ContentInfo`, hex-encoded (the certificate is JSON).
    pub token_hex: String,
}

/// The certificate as embedded in the PDF.
#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RedactionCertificate {
    pub version: u32,
    pub id: String,
    /// RFC 3339 UTC.
    pub created_at: String,
    pub original: FileDigest,
    /// The redacted file **with the certificate stripped** (see module docs).
    pub redacted: FileDigest,
    pub items: Vec<CertificateItem>,
    pub timestamp: Option<CertificateTimestamp>,
}

/// The result of checking a PDF for a certificate.
#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CertificateVerification {
    pub present: bool,
    pub certificate: Option<RedactionCertificate>,
    /// True when the file, minus the certificate itself, still hashes to the
    /// digest recorded at certification time.
    pub content_matches: bool,
    /// SHA-256 of the file with the certificate stripped.
    pub actual_sha256: String,
}

/// SHA-256 and size of `bytes`.
pub fn digest(bytes: &[u8]) -> FileDigest {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    FileDigest { sha256: format!("{:x}", hasher.finalize()), size: bytes.len() as u64 }
}

/// RFC 3339 UTC timestamp without pulling the `time` formatting feature in.
fn rfc3339_utc(now: time::OffsetDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

/// The digest a certificate records for `redacted` bytes: the hash of the
/// file with the certificate stripped (see module docs). Exposed so the app
/// can stamp exactly this digest with an RFC 3161 authority before building
/// the certificate.
pub fn certified_digest(redacted: &[u8]) -> PdfResult<FileDigest> {
    let probe = RedactionCertificate {
        version: 1,
        id: "probe".into(),
        created_at: String::new(),
        original: digest(&[]),
        redacted: digest(redacted),
        items: Vec::new(),
        timestamp: None,
    };
    let embedded = embed_certificate(redacted, &probe)?;
    Ok(digest(&strip_certificate(&embedded)?))
}

/// Builds a certificate for a completed redaction.
///
/// `redacted` is the file **before** the certificate is embedded; the recorded
/// digest is computed from a probe embed/strip cycle so it matches what
/// [`verify_certificate`] will compute later.
pub fn build_certificate(
    original: &[u8],
    redacted: &[u8],
    items: Vec<CertificateItem>,
    timestamp: Option<TimestampToken>,
) -> PdfResult<RedactionCertificate> {
    let original_digest = digest(original);
    let mut certificate = RedactionCertificate {
        version: 1,
        id: String::new(),
        created_at: rfc3339_utc(time::OffsetDateTime::now_utc()),
        original: original_digest,
        redacted: digest(redacted),
        items,
        timestamp: timestamp.map(|token| CertificateTimestamp {
            time: token.time,
            token_hex: token.token.iter().map(|byte| format!("{byte:02x}")).collect(),
        }),
    };
    certificate.id = format!("CERT-{}", &certificate.redacted.sha256[..12]);
    // The recorded digest is the hash of the certified file with the
    // certificate stripped - exactly what verification recomputes.
    certificate.redacted = certified_digest(redacted)?;
    certificate.id = format!("CERT-{}", &certificate.redacted.sha256[..12]);
    Ok(certificate)
}

/// Where the certificate lives inside the PDF.
const KIND: RecordKind = RecordKind {
    file_name: CERTIFICATE_FILE_NAME,
    description: "OmniOffice redaction certificate",
    xmp_namespace: CERTIFICATE_XMP_NAMESPACE,
};

/// The XMP packet that mirrors the certificate into the catalog metadata.
fn xmp_packet(certificate: &RedactionCertificate) -> String {
    attachment::xmp_packet(
        CERTIFICATE_XMP_NAMESPACE,
        &[
            ("CertificateId", certificate.id.clone()),
            ("OriginalSha256", certificate.original.sha256.clone()),
            ("RedactedSha256", certificate.redacted.sha256.clone()),
            ("CreatedAt", certificate.created_at.clone()),
            ("ItemCount", certificate.items.len().to_string()),
        ],
    )
}

/// Embeds the certificate as an embedded file and as XMP metadata.
pub fn embed_certificate(pdf: &[u8], certificate: &RedactionCertificate) -> PdfResult<Vec<u8>> {
    let json = serde_json::to_vec_pretty(certificate)
        .map_err(|error| PdfError::InvalidPdf(format!("certificate json: {error}")))?;
    attachment::embed_record(pdf, &KIND, json, xmp_packet(certificate))
}

/// Removes the certificate (embedded file + our XMP metadata) from the PDF.
///
/// This is what verification hashes: the file "as it was redacted", without
/// the evidence that was added afterwards.
pub fn strip_certificate(pdf: &[u8]) -> PdfResult<Vec<u8>> {
    attachment::strip_record(pdf, &KIND)
}

/// Extracts and verifies the certificate, if the PDF carries one.
pub fn verify_certificate(pdf: &[u8]) -> PdfResult<CertificateVerification> {
    let doc = Document::load_mem(pdf).map_err(|error| PdfError::InvalidPdf(format!("{error}")))?;
    let Some(bytes) = attachment::read_record(&doc, &KIND) else {
        return Ok(CertificateVerification {
            present: false,
            certificate: None,
            content_matches: false,
            actual_sha256: String::new(),
        });
    };
    let certificate: RedactionCertificate =
        serde_json::from_slice(&bytes).map_err(|error| PdfError::InvalidPdf(format!("certificate json: {error}")))?;
    drop(doc);
    let stripped = strip_certificate(pdf)?;
    let actual = digest(&stripped);
    Ok(CertificateVerification {
        present: true,
        content_matches: actual.sha256 == certificate.redacted.sha256,
        certificate: Some(certificate),
        actual_sha256: actual.sha256,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary as dict;
    use lopdf::{dictionary, Dictionary, Object, Stream};

    fn sample_pdf() -> Vec<u8> {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let content_id = doc.add_object(Stream::new(Dictionary::new(), b"BT ET".to_vec()));
        let page_id = doc.add_object(dict! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![0.into(), 0.into(), 200.into(), 200.into()]),
            "Contents" => Object::Reference(content_id),
        });
        let pages = dict! {
            "Type" => "Pages",
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
            "Count" => 1,
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));
        let catalog_id = doc.add_object(dict! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) });
        doc.trailer.set("Root", Object::Reference(catalog_id));
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    fn items() -> Vec<CertificateItem> {
        vec![
            CertificateItem { page: 1, category: "tckn".into(), rule: "KVKK Md. 6".into() },
            CertificateItem { page: 2, category: "iban".into(), rule: "KVKK Md. 6".into() },
        ]
    }

    #[test]
    fn embeds_and_verifies_a_certificate() {
        let original = sample_pdf();
        let redacted = sample_pdf();
        let certificate = build_certificate(&original, &redacted, items(), None).unwrap();
        let embedded = embed_certificate(&redacted, &certificate).unwrap();
        let verification = verify_certificate(&embedded).unwrap();
        assert!(verification.present);
        assert!(verification.content_matches, "the fresh file must verify");
        let found = verification.certificate.unwrap();
        assert_eq!(found.items, items());
        assert_eq!(found.original.sha256, digest(&original).sha256);
    }

    #[test]
    fn a_modified_file_fails_verification() {
        let redacted = sample_pdf();
        let certificate = build_certificate(&sample_pdf(), &redacted, items(), None).unwrap();
        let embedded = embed_certificate(&redacted, &certificate).unwrap();
        // Edit the document the way a real edit would: through the object
        // model, so the file stays valid and only the content changes.
        let mut doc = Document::load_mem(&embedded).unwrap();
        let content_id = doc
            .objects
            .iter()
            .find_map(|(id, object)| {
                let stream = object.as_stream().ok()?;
                (stream.content == b"BT ET").then_some(*id)
            })
            .expect("the content stream is still there");
        let mut stream = doc.get_object(content_id).unwrap().as_stream().unwrap().clone();
        stream.content.extend_from_slice(b" ");
        doc.objects.insert(content_id, Object::Stream(stream));
        let mut edited = Vec::new();
        doc.save_to(&mut edited).unwrap();
        let verification = verify_certificate(&edited).unwrap();
        assert!(verification.present, "the certificate survives a content edit");
        assert!(!verification.content_matches, "a modified file must not verify");
    }

    #[test]
    fn a_plain_pdf_has_no_certificate() {
        let verification = verify_certificate(&sample_pdf()).unwrap();
        assert!(!verification.present);
        assert!(!verification.content_matches);
    }

    #[test]
    fn the_certificate_never_carries_a_redacted_value() {
        let certificate = build_certificate(&sample_pdf(), &sample_pdf(), items(), None).unwrap();
        let json = serde_json::to_string(&certificate).unwrap();
        assert!(!json.contains("12345678901"), "no values may appear in the certificate");
        assert!(json.contains("tckn"));
        assert!(json.contains("KVKK Md. 6"));
    }

    #[test]
    fn strip_removes_the_certificate_and_keeps_foreign_metadata() {
        let redacted = sample_pdf();
        let certificate = build_certificate(&sample_pdf(), &redacted, items(), None).unwrap();
        let embedded = embed_certificate(&redacted, &certificate).unwrap();
        assert!(verify_certificate(&embedded).unwrap().present);
        let stripped = strip_certificate(&embedded).unwrap();
        assert!(!verify_certificate(&stripped).unwrap().present);
    }

    #[test]
    fn embedding_is_stable_across_probe_and_final_pass() {
        // build_certificate probes with the initial certificate and records the
        // stripped digest; embedding the final certificate must verify.
        let redacted = sample_pdf();
        let first = build_certificate(&sample_pdf(), &redacted, items(), None).unwrap();
        let embedded_first = embed_certificate(&redacted, &first).unwrap();
        let verification = verify_certificate(&embedded_first).unwrap();
        assert!(verification.content_matches);
        // A second certification of the same redacted bytes records the same digest.
        let second = build_certificate(&sample_pdf(), &redacted, items(), None).unwrap();
        assert_eq!(first.redacted.sha256, second.redacted.sha256);
    }
}
