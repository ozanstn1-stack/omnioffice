//! Adding a stamp to a document that is already signed.

mod common;

use lopdf::Document;
use pdfcore::annotate::{annotate_pdf_incremental, Annotation};
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
        params.distinguished_name.push(rcgen::DnType::CommonName, "PDF SAK Stamp Signer");
        let cert = params.self_signed(&key_pair).expect("self signed");
        (cert.der().to_vec(), key.to_pkcs8_der().expect("pkcs8 der").as_bytes().to_vec())
    })
}

fn signed_pdf() -> Vec<u8> {
    let mut doc = common::build_text_doc(1, "Stamp sample", "Stamp sample");
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
        signer_name: Some("Stamp Signer".to_string()),
    };
    sign::sign_pdf(&bytes, cert_der, key_der, &[], &options).expect("sign")
}

fn rectangle() -> Annotation {
    Annotation {
        kind: "rect".to_string(),
        page: 1,
        x: 80.0,
        y: 120.0,
        w: 200.0,
        h: 60.0,
        text: String::new(),
        font_size_pt: 14.0,
        bold: false,
        color: "#e11d48".to_string(),
        opacity: 0.35,
        image_path: None,
        line_width_pt: 2.0,
        x2: None,
        y2: None,
        strokes: Vec::new(),
        image_base64: None,
    }
}

#[test]
fn a_signed_document_survives_an_added_annotation() {
    let signed = signed_pdf();
    let updated = annotate_pdf_incremental(&signed, &[rectangle()]).expect("incremental stamp");

    // The signed revision is byte-identical; only a later revision was added.
    assert!(updated.starts_with(&signed), "the original revision must be preserved");

    let verification = sign::verify_signatures(&updated);
    assert_eq!(verification.signatures.len(), 1);
    let info = &verification.signatures[0];
    assert!(info.digest_matches, "digest: {:?}", info.notes);
    assert!(info.signature_valid, "signature: {:?}", info.notes);
    assert!(!info.modified_after_signing, "a stamp is not a modification: {:?}", info.notes);
    assert!(info.superseded_by_later_revision);

    // And the stamp really is drawn: the page content of the new revision
    // carries the rectangle operator the annotation emits.
    let reloaded = Document::load_mem(&updated).expect("reload");
    let page_id = *reloaded.get_pages().values().next().expect("page");
    let content = reloaded.get_page_content(page_id);
    assert!(
        content.windows(3).any(|window| window == b"re\n" || window == b"re "),
        "the rectangle operators must be appended to the page content"
    );
}

#[test]
fn two_stamps_can_be_appended_in_a_row() {
    let signed = signed_pdf();
    let once = annotate_pdf_incremental(&signed, &[rectangle()]).expect("first");
    let twice = annotate_pdf_incremental(&once, &[rectangle()]).expect("second");
    assert!(twice.starts_with(&once));
    let verification = sign::verify_signatures(&twice);
    assert!(verification.signatures[0].digest_matches);
    assert!(verification.signatures[0].signature_valid);
}

#[test]
fn no_annotations_is_an_error() {
    let signed = signed_pdf();
    assert!(annotate_pdf_incremental(&signed, &[]).is_err());
}
