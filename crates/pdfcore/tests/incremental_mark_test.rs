//! Watermarks and page numbers on a document that is already signed.

mod common;

use lopdf::Document;
use pdfcore::numbering::{numbering_pdf_incremental, NumberingOptions};
use pdfcore::sign::{self, SignOptions};
use pdfcore::watermark::{watermark_pdf_incremental, WatermarkOptions};
use rsa::pkcs8::{EncodePrivateKey, LineEnding};
use rsa::RsaPrivateKey;
use std::sync::OnceLock;

fn identity() -> &'static (Vec<u8>, Vec<u8>) {
    static IDENTITY: OnceLock<(Vec<u8>, Vec<u8>)> = OnceLock::new();
    IDENTITY.get_or_init(|| {
        let key = RsaPrivateKey::new(&mut rand::rngs::OsRng, 2048).expect("rsa key");
        let pem = key.to_pkcs8_pem(LineEnding::LF).expect("pkcs8 pem");
        let key_pair = rcgen::KeyPair::from_pkcs8_pem_and_sign_algo(&pem, &rcgen::PKCS_RSA_SHA256)
            .expect("ring accepts the RSA key");
        let mut params = rcgen::CertificateParams::new(Vec::<String>::new()).expect("params");
        params.distinguished_name.push(rcgen::DnType::CommonName, "PDF SAK Mark Signer");
        let cert = params.self_signed(&key_pair).expect("self signed");
        (cert.der().to_vec(), key.to_pkcs8_der().expect("pkcs8 der").as_bytes().to_vec())
    })
}

fn signed_pdf(pages: u32) -> Vec<u8> {
    let mut doc = common::build_text_doc(pages, "Mark sample", "Mark sample");
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save");
    let (cert_der, key_der) = identity();
    let options = SignOptions {
        page: 1,
        rect: None,
        reason: "Approval".to_string(),
        location: "Istanbul".to_string(),
        contact: String::new(),
        appearance: true,
        signer_name: Some("Mark Signer".to_string()),
    };
    sign::sign_pdf(&bytes, cert_der, key_der, &[], &options).expect("sign")
}

fn watermark_options() -> WatermarkOptions {
    serde_json::from_str(r#"{"kind":"text","text":"CONFIDENTIAL","opacity":0.3}"#).expect("options")
}

fn numbering_options() -> NumberingOptions {
    serde_json::from_str(r#"{"position":"bottom_center","format":"n_of_total"}"#).expect("options")
}

/// The signature still verifies over its own revision and is reported as
/// superseded rather than modified.
fn assert_signature_survived(updated: &[u8], original: &[u8]) {
    assert!(updated.starts_with(original), "the original revision must be preserved");
    let verification = sign::verify_signatures(updated);
    assert_eq!(verification.signatures.len(), 1);
    let info = &verification.signatures[0];
    assert!(info.digest_matches, "digest: {:?}", info.notes);
    assert!(info.signature_valid, "signature: {:?}", info.notes);
    assert!(!info.modified_after_signing, "an appended revision is not a modification: {:?}", info.notes);
    assert!(info.superseded_by_later_revision);
}

fn first_page_content(pdf: &[u8]) -> Vec<u8> {
    let doc = Document::load_mem(pdf).expect("reload");
    let page_id = *doc.get_pages().values().next().expect("page");
    doc.get_page_content(page_id)
}

#[test]
fn a_signed_document_survives_a_watermark() {
    let signed = signed_pdf(2);
    let updated = watermark_pdf_incremental(&signed, &watermark_options()).expect("watermark");
    assert_signature_survived(&updated, &signed);

    // The watermark really is drawn: the page content of the new revision
    // carries the XObject paint operator.
    let content = first_page_content(&updated);
    assert!(content.windows(3).any(|window| window == b" Do"), "the watermark XObject must be painted on the page");
}

#[test]
fn a_signed_document_survives_page_numbers() {
    let signed = signed_pdf(1);
    let updated = numbering_pdf_incremental(&signed, &numbering_options()).expect("numbering");
    assert_signature_survived(&updated, &signed);

    // The label is written as a real text operator, so it can be asserted.
    let content = first_page_content(&updated);
    let text = String::from_utf8_lossy(&content);
    assert!(text.contains("1 / 1"), "the page label must be in the content: {text}");
}

#[test]
fn both_marks_can_be_appended_in_a_row() {
    let signed = signed_pdf(1);
    let watermarked = watermark_pdf_incremental(&signed, &watermark_options()).expect("watermark");
    let numbered = numbering_pdf_incremental(&watermarked, &numbering_options()).expect("numbering");
    assert!(numbered.starts_with(&watermarked));
    assert!(numbered.starts_with(&signed));
    let verification = sign::verify_signatures(&numbered);
    assert!(verification.signatures[0].digest_matches);
    assert!(verification.signatures[0].signature_valid);
}
