//! Long-term validation data (the PAdES B-LT half of the signing story).
//!
//! A signature is only as durable as the information needed to validate it:
//! the signer chain and, when a signer supplies them, the revocation records.
//! PAdES calls the archival copy of that information the DSS (Document
//! Security Store) plus one VRI entry per signature (ETSI EN 319 142-1).
//!
//! Everything here is appended as an incremental update: the signed revision -
//! and therefore every signature in it - stays byte for byte as it was, which
//! is exactly what makes the archive meaningful.

use crate::error::{PdfError, PdfResult};
use crate::sign::{der_exact_slice, to_hex_upper};
use cms::cert::CertificateChoices;
use cms::content_info::ContentInfo;
use cms::signed_data::SignedData;
use der::{Decode, Encode};
use lopdf::{Dictionary, Document, IncrementalDocument, Object, Stream};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

/// What [`add_validation_data`] wrote into the document.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LtvReport {
    /// Signatures that received a VRI entry.
    pub signatures: usize,
    /// Certificates stored in the DSS, after de-duplication.
    pub certificates: usize,
    /// The VRI keys written (uppercase hex SHA-1 of each signature /Contents).
    pub vri_keys: Vec<String>,
    /// Anything that could not be archived, reported instead of skipped.
    pub warnings: Vec<String>,
}

/// Appends the validation data of every signature in `input` to its DSS.
///
/// The VRI key is the uppercase hex SHA-1 of the signature /Contents value, as
/// the specification requires; the certificates are stored once each and shared
/// by the VRI entries that need them.
pub fn add_validation_data(input: &[u8]) -> PdfResult<(Vec<u8>, LtvReport)> {
    let doc = Document::load_mem(input).map_err(|err| PdfError::from_lopdf(err, None))?;
    if doc.is_encrypted() || doc.was_encrypted() {
        return Err(PdfError::PasswordRequired);
    }
    let catalog_id = doc
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .map_err(|_| PdfError::InvalidPdf("the document has no catalog".into()))?;

    let mut report = LtvReport::default();
    // Per signature: the VRI key and the certificates that signature needs.
    let mut entries: Vec<(String, Vec<Vec<u8>>)> = Vec::new();
    let mut certificates: Vec<Vec<u8>> = Vec::new();
    for object in doc.objects.values() {
        let Ok(dict) = object.as_dict() else { continue };
        if dict.get(b"ByteRange").is_err() || dict.get(b"Contents").is_err() {
            continue;
        }
        let Ok(Object::String(contents, _)) = dict.get(b"Contents") else {
            continue;
        };
        let key = to_hex_upper(&Sha1::digest(contents));
        let mut own: Vec<Vec<u8>> = Vec::new();
        match der_exact_slice(contents).and_then(|slice| ContentInfo::from_der(slice).ok()) {
            Some(info) => match info.content.decode_as::<SignedData>() {
                Ok(signed) => {
                    if let Some(certs) = &signed.certificates {
                        for choice in certs.0.iter() {
                            if let CertificateChoices::Certificate(cert) = choice {
                                if let Ok(der) = cert.to_der() {
                                    own.push(der);
                                }
                            }
                        }
                    }
                }
                Err(error) => report.warnings.push(format!("a signature CMS could not be decoded: {error}")),
            },
            None => report.warnings.push("a signature has no parseable CMS value".to_string()),
        }
        for der in &own {
            if !certificates.contains(der) {
                certificates.push(der.clone());
            }
        }
        entries.push((key, own));
    }
    if entries.is_empty() {
        return Err(PdfError::InvalidInput("the document has no signature to archive validation data for".into()));
    }

    // A second call must not duplicate what an earlier one stored (and the
    // existing entries may reference certificates this signature does not
    // carry, so keep them).
    for der in existing_dss_certificates(&doc) {
        if !certificates.contains(&der) {
            certificates.push(der);
        }
    }

    let mut inc = IncrementalDocument::create_from(input.to_vec(), doc);
    inc.opt_clone_object_to_new_document(catalog_id)?;

    let cert_ids: Vec<Object> = certificates
        .iter()
        .map(|der| {
            let stream = Stream::new(Dictionary::new(), der.clone());
            Object::Reference(inc.new_document.add_object(Object::Stream(stream)))
        })
        .collect();

    let mut vri = Dictionary::new();
    for (key, own) in &entries {
        let refs: Vec<Object> = own
            .iter()
            .filter_map(|der| {
                let index = certificates.iter().position(|candidate| candidate == der)?;
                Some(cert_ids[index].clone())
            })
            .collect();
        let mut entry = Dictionary::new();
        entry.set("Type", "VRI");
        entry.set("Cert", refs);
        vri.set(key.as_str(), Object::Dictionary(entry));
    }

    let mut dss = Dictionary::new();
    dss.set("Certs", cert_ids.clone());
    dss.set("VRI", Object::Dictionary(vri));
    let dss_id = inc.new_document.add_object(Object::Dictionary(dss));
    inc.new_document
        .get_object_mut(catalog_id)
        .and_then(Object::as_dict_mut)
        .map_err(|_| PdfError::InvalidPdf("the catalog is not a dictionary".into()))?
        .set("DSS", Object::Reference(dss_id));

    let mut output = Vec::new();
    inc.save_to(&mut output).map_err(|err| PdfError::ProcessingFailed(format!("could not write the PDF: {err}")))?;
    // The whole point of the exercise: every existing signature must still
    // cover exactly the bytes it signed.
    if !output.starts_with(input) {
        return Err(PdfError::Internal("the incremental update modified the signed revision".into()));
    }

    report.signatures = entries.len();
    report.certificates = certificates.len();
    report.vri_keys = entries.into_iter().map(|(key, _)| key).collect();
    Ok((output, report))
}

/// Certificates already stored in the document DSS, if any.
fn existing_dss_certificates(doc: &Document) -> Vec<Vec<u8>> {
    let Some(dss_id) = doc
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .ok()
        .and_then(|root| doc.get_object(root).ok())
        .and_then(|object| object.as_dict().ok())
        .and_then(|catalog| catalog.get(b"DSS").ok())
        .and_then(|value| value.as_reference().ok())
    else {
        return Vec::new();
    };
    let Some(entries) = doc
        .get_object(dss_id)
        .ok()
        .and_then(|object| object.as_dict().ok())
        .and_then(|dss| dss.get(b"Certs").ok())
        .and_then(|value| value.as_array().ok())
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries {
        let Ok(id) = entry.as_reference() else { continue };
        if let Ok(Object::Stream(stream)) = doc.get_object(id) {
            out.push(stream.content.clone());
        }
    }
    out
}

/// Number of signatures in `pdf`, used by the UI to decide whether archiving
/// validation data makes sense.
pub fn signature_count(pdf: &[u8]) -> usize {
    crate::sign::verify_signatures(pdf).signatures.len()
}
