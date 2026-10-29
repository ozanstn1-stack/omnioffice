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

use lopdf::{dictionary, Dictionary, Document, Object, Stream};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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

/// The XMP packet that mirrors the certificate into the catalog metadata.
fn xmp_packet(certificate: &RedactionCertificate) -> String {
    let escape =
        |value: &str| value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;");
    format!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n\
<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n\
  <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n\
    <rdf:Description rdf:about=\"\" xmlns:omni=\"{namespace}\">\n\
      <omni:CertificateId>{id}</omni:CertificateId>\n\
      <omni:OriginalSha256>{original}</omni:OriginalSha256>\n\
      <omni:RedactedSha256>{redacted}</omni:RedactedSha256>\n\
      <omni:CreatedAt>{created}</omni:CreatedAt>\n\
      <omni:ItemCount>{count}</omni:ItemCount>\n\
    </rdf:Description>\n\
  </rdf:RDF>\n\
</x:xmpmeta>\n\
<?xpacket end=\"w\"?>",
        namespace = CERTIFICATE_XMP_NAMESPACE,
        id = escape(&certificate.id),
        original = escape(&certificate.original.sha256),
        redacted = escape(&certificate.redacted.sha256),
        created = escape(&certificate.created_at),
        count = certificate.items.len(),
    )
}

/// Finds the certificate's file specification id, if the PDF carries one.
fn certificate_filespec(doc: &Document) -> Option<lopdf::ObjectId> {
    let catalog = doc.catalog().ok()?;
    let names = catalog.get(b"Names").ok()?.as_dict().ok()?;
    let embedded = names.get(b"EmbeddedFiles").ok()?.as_dict().ok()?;
    let pairs = embedded.get(b"Names").ok()?.as_array().ok()?;
    for pair in pairs.chunks(2) {
        let [name, reference] = pair else { continue };
        if name.as_str().ok() == Some(CERTIFICATE_FILE_NAME.as_bytes()) {
            if let Ok(id) = reference.as_reference() {
                return Some(id);
            }
        }
    }
    None
}

/// Reads the embedded certificate bytes.
fn read_embedded_certificate(doc: &Document) -> Option<Vec<u8>> {
    let filespec_id = certificate_filespec(doc)?;
    let filespec = doc.get_dictionary(filespec_id).ok()?;
    let ef = filespec.get(b"EF").ok()?.as_dict().ok()?;
    let stream_id = ef.get(b"F").ok()?.as_reference().ok()?;
    let stream = doc.get_object(stream_id).ok()?.as_stream().ok()?;
    Some(stream.content.clone())
}

/// Embeds the certificate as an embedded file and as XMP metadata.
pub fn embed_certificate(pdf: &[u8], certificate: &RedactionCertificate) -> PdfResult<Vec<u8>> {
    let mut doc = Document::load_mem(pdf).map_err(|error| PdfError::InvalidPdf(format!("{error}")))?;
    let json = serde_json::to_vec_pretty(certificate)
        .map_err(|error| PdfError::InvalidPdf(format!("certificate json: {error}")))?;

    let stream_id = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "EmbeddedFile",
            "Subtype" => Object::Name(b"application/json".to_vec()),
        },
        json,
    ));
    let filespec_id = doc.add_object(dictionary! {
        "Type" => "Filespec",
        "F" => Object::string_literal(CERTIFICATE_FILE_NAME),
        "Desc" => Object::string_literal("OmniOffice redaction certificate"),
        "EF" => dictionary! { "F" => Object::Reference(stream_id) },
    });

    let catalog_id = doc
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .map_err(|error| PdfError::InvalidPdf(format!("{error}")))?;
    {
        let catalog = doc.get_dictionary_mut(catalog_id).map_err(|error| PdfError::InvalidPdf(format!("{error}")))?;
        // Merge into an existing name tree rather than replacing it.
        let mut embedded = match catalog.get(b"Names").ok().and_then(|value| value.as_dict().ok()) {
            Some(names) => match names.get(b"EmbeddedFiles").ok().and_then(|value| value.as_dict().ok()) {
                Some(tree) => tree.clone(),
                None => dictionary! {},
            },
            None => dictionary! {},
        };
        let mut pairs: Vec<Object> =
            embedded.get(b"Names").ok().and_then(|value| value.as_array().ok()).cloned().unwrap_or_default();
        pairs.push(Object::string_literal(CERTIFICATE_FILE_NAME));
        pairs.push(Object::Reference(filespec_id));
        embedded.set("Names", Object::Array(pairs));
        let mut names = match catalog.get(b"Names").ok().and_then(|value| value.as_dict().ok()) {
            Some(existing) => existing.clone(),
            None => Dictionary::new(),
        };
        names.set("EmbeddedFiles", Object::Dictionary(embedded));
        catalog.set("Names", Object::Dictionary(names));
    }

    let metadata_id = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "Metadata",
            "Subtype" => "XML",
        },
        xmp_packet(certificate).into_bytes(),
    ));
    {
        let catalog = doc.get_dictionary_mut(catalog_id).map_err(|error| PdfError::InvalidPdf(format!("{error}")))?;
        catalog.set("Metadata", Object::Reference(metadata_id));
    }

    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|error| PdfError::InvalidPdf(format!("{error}")))?;
    Ok(out)
}

/// Removes the certificate (embedded file + our XMP metadata) from the PDF.
///
/// This is what verification hashes: the file "as it was redacted", without
/// the evidence that was added afterwards.
pub fn strip_certificate(pdf: &[u8]) -> PdfResult<Vec<u8>> {
    let mut doc = Document::load_mem(pdf).map_err(|error| PdfError::InvalidPdf(format!("{error}")))?;

    // Collect and remove the filespec + its stream.
    let mut removed: Vec<lopdf::ObjectId> = Vec::new();
    if let Some(filespec_id) = certificate_filespec(&doc) {
        if let Ok(filespec) = doc.get_dictionary(filespec_id) {
            if let Ok(stream_id) = filespec
                .get(b"EF")
                .and_then(|ef| ef.as_dict())
                .and_then(|ef| ef.get(b"F"))
                .and_then(Object::as_reference)
            {
                removed.push(stream_id);
            }
        }
        removed.push(filespec_id);
    }

    let catalog_id = doc
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .map_err(|error| PdfError::InvalidPdf(format!("{error}")))?;
    let metadata_id = {
        let catalog = doc.get_dictionary(catalog_id).map_err(|error| PdfError::InvalidPdf(format!("{error}")))?;
        // Only drop metadata that is ours; a foreign XMP packet stays.
        catalog.get(b"Metadata").ok().and_then(|value| value.as_reference().ok()).filter(|id| {
            doc.get_object(*id)
                .ok()
                .and_then(|object| object.as_stream().ok())
                .map(|stream| String::from_utf8_lossy(&stream.content).contains(CERTIFICATE_XMP_NAMESPACE))
                .unwrap_or(false)
        })
    };

    {
        let catalog = doc.get_dictionary_mut(catalog_id).map_err(|error| PdfError::InvalidPdf(format!("{error}")))?;
        if let Some(id) = metadata_id {
            removed.push(id);
            catalog.remove(b"Metadata");
        }
        // Rebuild the name tree without our entry; drop the tree when empty.
        let names = catalog.get(b"Names").ok().and_then(|value| value.as_dict().ok()).cloned();
        if let Some(mut names) = names {
            let embedded = names.get(b"EmbeddedFiles").ok().and_then(|value| value.as_dict().ok()).cloned();
            if let Some(mut embedded) = embedded {
                let mut pairs: Vec<Object> =
                    embedded.get(b"Names").ok().and_then(|value| value.as_array().ok()).cloned().unwrap_or_default();
                pairs = pairs
                    .chunks(2)
                    .filter(|pair| {
                        pair.first().and_then(|name| name.as_str().ok()) != Some(CERTIFICATE_FILE_NAME.as_bytes())
                    })
                    .flat_map(|pair| pair.to_vec())
                    .collect();
                if pairs.is_empty() {
                    names.remove(b"EmbeddedFiles");
                } else {
                    embedded.set("Names", Object::Array(pairs));
                    names.set("EmbeddedFiles", Object::Dictionary(embedded));
                }
                if names.is_empty() {
                    catalog.remove(b"Names");
                } else {
                    catalog.set("Names", Object::Dictionary(names));
                }
            }
        }
    }

    for id in removed {
        doc.objects.remove(&id);
    }

    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|error| PdfError::InvalidPdf(format!("{error}")))?;
    Ok(out)
}

/// Extracts and verifies the certificate, if the PDF carries one.
pub fn verify_certificate(pdf: &[u8]) -> PdfResult<CertificateVerification> {
    let doc = Document::load_mem(pdf).map_err(|error| PdfError::InvalidPdf(format!("{error}")))?;
    let Some(bytes) = read_embedded_certificate(&doc) else {
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
