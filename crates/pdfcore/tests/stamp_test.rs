//! Header/footer and Bates stamps (v4.6.0).

mod common;

use common::*;
use lopdf::Document;
use pdfcore::docutil::OverwritePolicy;
use pdfcore::progress::CancelToken;
use pdfcore::sign::{self, SignOptions};
use pdfcore::stamp::{stamp_pdf, stamp_pdf_incremental, BatesOptions, HeaderFooterOptions};
use rsa::pkcs8::{EncodePrivateKey, LineEnding};
use rsa::RsaPrivateKey;
use std::sync::OnceLock;

fn sample_bytes(pages: u32) -> Vec<u8> {
    let mut doc = build_text_doc(pages, "STAMP", "Stamp sample");
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save");
    bytes
}

fn page_content(pdf: &[u8], page: u32) -> String {
    let doc = Document::load_mem(pdf).expect("reload");
    let page_id = *doc.get_pages().get(&page).expect("page exists");
    String::from_utf8_lossy(&doc.get_page_content(page_id)).to_string()
}

fn identity() -> &'static (Vec<u8>, Vec<u8>) {
    static IDENTITY: OnceLock<(Vec<u8>, Vec<u8>)> = OnceLock::new();
    IDENTITY.get_or_init(|| {
        let key = RsaPrivateKey::new(&mut rand::rngs::OsRng, 2048).expect("rsa key");
        let pem = key.to_pkcs8_pem(LineEnding::LF).expect("pkcs8 pem");
        let key_pair = rcgen::KeyPair::from_pkcs8_pem_and_sign_algo(&pem, &rcgen::PKCS_RSA_SHA256)
            .expect("ring accepts the RSA key");
        let mut params = rcgen::CertificateParams::new(Vec::<String>::new()).expect("params");
        params.distinguished_name.push(rcgen::DnType::CommonName, "PDF SAK Stamp Signer");
        let cert = params.self_signed(&key_pair).expect("self signed");
        (cert.der().to_vec(), key.to_pkcs8_der().expect("pkcs8 der").as_bytes().to_vec())
    })
}

#[test]
fn header_footer_text_is_drawn_and_placeholders_are_substituted() {
    let bytes = sample_bytes(3);
    let options = HeaderFooterOptions { footer_center: "Sayfa {page} / {pages}".into(), ..Default::default() };
    let stamped = stamp_pdf_incremental(&bytes, Some(&options), None).expect("stamp");

    assert!(stamped.starts_with(&bytes), "the original revision must be preserved");
    for page in 1..=3u32 {
        let content = page_content(&stamped, page);
        assert!(content.contains(&format!("Sayfa {page} / 3")), "page {page} content: {content}");
        assert!(content.contains("/Helv"), "the Helvetica resource must be wired in");
    }
}

#[test]
fn count_from_start_offsets_the_visible_number() {
    let bytes = sample_bytes(3);
    let options = HeaderFooterOptions {
        footer_right: "{page} / {pages}".into(),
        pages: vec![2, 3],
        start_number: 10,
        count_from_start: false,
        ..Default::default()
    };
    let stamped = stamp_pdf_incremental(&bytes, Some(&options), None).expect("stamp");
    // Counting from the document, not the selection: page 2 shows "11 / 3".
    assert!(page_content(&stamped, 2).contains("11 / 3"));
    assert!(page_content(&stamped, 3).contains("12 / 3"));

    let selection_counted = HeaderFooterOptions {
        footer_right: "{page} / {pages}".into(),
        pages: vec![2, 3],
        start_number: 10,
        count_from_start: true,
        ..Default::default()
    };
    let stamped = stamp_pdf_incremental(&bytes, Some(&selection_counted), None).expect("stamp");
    // Counting from the start: first selected page shows "10 / 2".
    assert!(page_content(&stamped, 2).contains("10 / 2"));
    assert!(page_content(&stamped, 3).contains("11 / 2"));
}

#[test]
fn a_page_selection_limits_the_header_footer() {
    let bytes = sample_bytes(3);
    let options = HeaderFooterOptions { header_left: "ONLY-SELECTED".into(), pages: vec![2], ..Default::default() };
    let stamped = stamp_pdf_incremental(&bytes, Some(&options), None).expect("stamp");
    assert!(!page_content(&stamped, 1).contains("ONLY-SELECTED"));
    assert!(page_content(&stamped, 2).contains("ONLY-SELECTED"));
    assert!(!page_content(&stamped, 3).contains("ONLY-SELECTED"));
}

#[test]
fn bates_numbers_pad_prefix_and_continue_over_the_selection() {
    let bytes = sample_bytes(3);
    let options = BatesOptions {
        prefix: "ACME-".into(),
        suffix: "-B".into(),
        start: 7,
        digits: 4,
        pages: vec![1, 3],
        ..Default::default()
    };
    let stamped = stamp_pdf_incremental(&bytes, None, Some(&options)).expect("stamp");
    assert!(page_content(&stamped, 1).contains("ACME-0007-B"));
    assert!(!page_content(&stamped, 2).contains("ACME-"));
    assert!(page_content(&stamped, 3).contains("ACME-0008-B"));
}

#[test]
fn stamping_without_any_text_is_rejected() {
    let bytes = sample_bytes(1);
    assert!(matches!(stamp_pdf_incremental(&bytes, None, None), Err(pdfcore::PdfError::InvalidInput(_))));
    let empty = HeaderFooterOptions::default();
    assert!(matches!(stamp_pdf_incremental(&bytes, Some(&empty), None), Err(pdfcore::PdfError::InvalidInput(_))));
}

#[test]
fn the_full_path_writes_a_stamped_file() {
    let dir = TestDir::new();
    let input = dir.path("full.pdf");
    write_doc(&mut build_text_doc(2, "FULL", "Full stamp"), &input);
    let output = dir.path("full-stamped.pdf");
    let options = HeaderFooterOptions { footer_center: "Page {page} of {pages}".into(), ..Default::default() };
    let written = stamp_pdf(
        &input,
        &output,
        Some(&options),
        None,
        OverwritePolicy::Replace,
        None,
        &no_progress,
        &CancelToken::new(),
    )
    .expect("stamp");
    assert_eq!(written, output);
    assert!(page_text(&written, 1).contains("Page 1 of 2"));
    assert!(page_text(&written, 2).contains("Page 2 of 2"));
}

#[test]
fn a_signed_document_survives_a_stamp() {
    let bytes = sample_bytes(2);
    let (cert_der, key_der) = identity();
    let sign_options = SignOptions {
        page: 1,
        rect: None,
        reason: "Approval".to_string(),
        location: "Istanbul".to_string(),
        contact: String::new(),
        appearance: true,
        signer_name: Some("Stamp Signer".to_string()),
    };
    let signed = sign::sign_pdf(&bytes, cert_der, key_der, &[], &sign_options).expect("sign");

    let header_footer = HeaderFooterOptions { header_right: "CONFIDENTIAL".into(), ..Default::default() };
    let bates = BatesOptions { prefix: "B-".into(), digits: 3, ..Default::default() };
    let updated = stamp_pdf_incremental(&signed, Some(&header_footer), Some(&bates)).expect("stamp");

    assert!(updated.starts_with(&signed), "the signed revision must be preserved");
    let verification = sign::verify_signatures(&updated);
    assert_eq!(verification.signatures.len(), 1);
    let info = &verification.signatures[0];
    assert!(info.digest_matches, "digest: {:?}", info.notes);
    assert!(info.signature_valid, "signature: {:?}", info.notes);
    assert!(!info.modified_after_signing, "an appended revision is not a modification: {:?}", info.notes);
    assert!(info.superseded_by_later_revision);
    assert!(page_content(&updated, 1).contains("CONFIDENTIAL"));
    assert!(page_content(&updated, 1).contains("B-001"));
}
