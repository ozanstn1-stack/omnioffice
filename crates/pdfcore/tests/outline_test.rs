//! Outline (bookmark) writing as an incremental update (v4.6.0).

mod common;

use common::*;
use lopdf::Document;
use pdfcore::inspect::{read_outline, write_outline, OutlineEntry};
use pdfcore::sign::{self, SignOptions};
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
        params.distinguished_name.push(rcgen::DnType::CommonName, "PDF SAK Outline Signer");
        let cert = params.self_signed(&key_pair).expect("self signed");
        (cert.der().to_vec(), key.to_pkcs8_der().expect("pkcs8 der").as_bytes().to_vec())
    })
}

fn sample_bytes(pages: u32) -> Vec<u8> {
    let mut doc = build_text_doc(pages, "OUT", "Outline sample");
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save");
    bytes
}

fn entries() -> Vec<OutlineEntry> {
    vec![
        OutlineEntry { title: "Chapter One".into(), page: 1, depth: 0 },
        OutlineEntry { title: "Section 1.1".into(), page: 2, depth: 1 },
        OutlineEntry { title: "Section 1.2".into(), page: 3, depth: 1 },
        OutlineEntry { title: "Chapter Two".into(), page: 4, depth: 0 },
    ]
}

fn read_back(pdf: &[u8]) -> Vec<OutlineEntry> {
    let doc = Document::load_mem(pdf).expect("reload");
    let catalog = doc.catalog().expect("catalog");
    read_outline(&doc, catalog).expect("read outline")
}

#[test]
fn write_then_read_roundtrips_titles_pages_and_depths() {
    let bytes = sample_bytes(4);
    let written = write_outline(&bytes, &entries()).expect("write outline");

    assert!(written.starts_with(&bytes), "the incremental prefix must be preserved");
    let readback = read_back(&written);
    assert_eq!(readback.len(), entries().len());
    for (expected, actual) in entries().iter().zip(readback.iter()) {
        assert_eq!(actual.title, expected.title);
        assert_eq!(actual.page, expected.page);
        assert_eq!(actual.depth, expected.depth);
    }

    let doc = Document::load_mem(&written).expect("reload");
    let catalog = doc.catalog().expect("catalog");
    assert!(catalog.get(b"Outlines").is_ok(), "the catalog must point at /Outlines");
    assert_eq!(catalog.get(b"PageMode").and_then(|value| value.as_name()).unwrap_or_default(), b"UseOutlines");
}

#[test]
fn empty_entries_remove_the_outline_and_page_mode() {
    let bytes = sample_bytes(4);
    let written = write_outline(&bytes, &entries()).expect("write outline");
    let cleared = write_outline(&written, &[]).expect("clear outline");

    assert!(cleared.starts_with(&written));
    let doc = Document::load_mem(&cleared).expect("reload");
    let catalog = doc.catalog().expect("catalog");
    assert!(catalog.get(b"Outlines").is_err(), "/Outlines must be removed");
    assert!(catalog.get(b"PageMode").is_err(), "/PageMode must be removed");
    assert!(read_back(&cleared).is_empty());
}

#[test]
fn an_out_of_range_page_is_rejected() {
    let bytes = sample_bytes(2);
    let bad = vec![OutlineEntry { title: "Nowhere".into(), page: 99, depth: 0 }];
    let error = write_outline(&bytes, &bad).expect_err("out-of-range page must fail");
    assert!(matches!(error, pdfcore::PdfError::InvalidInput(_)), "unexpected error: {error}");
    assert!(write_outline(&bytes, &[OutlineEntry { title: "Zero".into(), page: 0, depth: 0 }]).is_err());
}

#[test]
fn a_signed_document_prefix_survives_an_outline_edit() {
    let bytes = sample_bytes(4);
    let (cert_der, key_der) = identity();
    let options = SignOptions {
        page: 1,
        rect: None,
        reason: "Approval".to_string(),
        location: "Istanbul".to_string(),
        contact: String::new(),
        appearance: true,
        signer_name: Some("Outline Signer".to_string()),
    };
    let signed = sign::sign_pdf(&bytes, cert_der, key_der, &[], &options).expect("sign");

    let updated = write_outline(&signed, &entries()).expect("write outline");
    assert!(updated.starts_with(&signed), "the signed revision must be preserved");

    let verification = sign::verify_signatures(&updated);
    assert_eq!(verification.signatures.len(), 1);
    let info = &verification.signatures[0];
    assert!(info.digest_matches, "digest: {:?}", info.notes);
    assert!(info.signature_valid, "signature: {:?}", info.notes);
    assert!(!info.modified_after_signing, "an appended revision is not a modification: {:?}", info.notes);
    assert_eq!(read_back(&updated).len(), entries().len());
}
