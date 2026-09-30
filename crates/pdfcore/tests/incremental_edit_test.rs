//! Incremental editing of an already-signed document.
//!
//! The point of appending a revision is that the signed bytes stay exactly as
//! they were: the signature is still valid, the change is simply a later
//! revision. These tests drive the real signing path and the real metadata
//! writer rather than hand-built objects.

mod common;

use lopdf::Document;
use pdfcore::metadata::{edit_metadata_incremental, read_metadata, PdfMetadata};
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
        params.distinguished_name.push(rcgen::DnType::CommonName, "PDF SAK Incremental Signer");
        let cert = params.self_signed(&key_pair).expect("self signed");
        (cert.der().to_vec(), key.to_pkcs8_der().expect("pkcs8 der").as_bytes().to_vec())
    })
}

fn signed_pdf() -> Vec<u8> {
    let mut doc = common::build_text_doc(1, "Incremental sample", "Incremental sample");
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
        signer_name: Some("Incremental Signer".to_string()),
    };
    sign::sign_pdf(&bytes, cert_der, key_der, &[], &options).expect("sign")
}

#[test]
fn a_signed_document_survives_a_metadata_edit() {
    let signed = signed_pdf();
    let meta = PdfMetadata {
        title: "Changed after signing".to_string(),
        author: "Second thoughts".to_string(),
        ..Default::default()
    };

    let updated = edit_metadata_incremental(&signed, &meta).expect("incremental metadata edit");

    // 1. The signed revision is byte-identical, which is what keeps the
    //    signature meaningful.
    assert!(updated.starts_with(&signed), "the original revision must be preserved");

    // 2. The signature still verifies over its own revision and is reported as
    //    superseded rather than modified.
    let verification = sign::verify_signatures(&updated);
    assert_eq!(verification.signatures.len(), 1);
    let info = &verification.signatures[0];
    assert!(info.digest_matches, "digest: {:?}", info.notes);
    assert!(info.signature_valid, "signature: {:?}", info.notes);
    assert!(!info.modified_after_signing, "an appended revision is not a modification: {:?}", info.notes);
    assert!(info.superseded_by_later_revision);

    // 3. A reader sees the new values.
    let reloaded = Document::load_mem(&updated).expect("reload");
    let metadata = read_metadata(&reloaded);
    assert_eq!(metadata.title, "Changed after signing");
    assert_eq!(metadata.author, "Second thoughts");
}

#[test]
fn the_incremental_edit_is_reported_by_the_cheap_scan() {
    let signed = signed_pdf();
    assert!(pdfcore::incremental::has_signatures(&signed));
    assert_eq!(pdfcore::incremental::signature_count(&signed), 1);
    let updated =
        edit_metadata_incremental(&signed, &PdfMetadata { title: "T".into(), ..Default::default() }).expect("edit");
    // The signature dictionaries are still there after the update.
    assert!(pdfcore::incremental::has_signatures(&updated));
}

#[test]
fn an_unsigned_document_still_round_trips() {
    let mut doc = common::build_text_doc(1, "Plain", "Plain");
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save");
    let updated = edit_metadata_incremental(&bytes, &PdfMetadata { title: "New title".into(), ..Default::default() })
        .expect("edit");
    assert!(updated.starts_with(&bytes));
    let reloaded = Document::load_mem(&updated).expect("reload");
    assert_eq!(read_metadata(&reloaded).title, "New title");
    assert_eq!(reloaded.get_pages().len(), 1);
}
