//! Long-term validation data: the DSS is appended as an incremental update so
//! the signature keeps covering exactly what it signed, and a second call does
//! not duplicate what the first one stored.

mod common;

use lopdf::{Document, Object};
use pdfcore::ltv::add_validation_data;
use pdfcore::sign::{self, SignOptions};
use rsa::pkcs8::{EncodePrivateKey, LineEnding};
use rsa::RsaPrivateKey;
use sha1::{Digest, Sha1};
use std::sync::OnceLock;

/// A throwaway self-signed identity, generated once per test binary.
fn identity() -> &'static (Vec<u8>, Vec<u8>) {
    static IDENTITY: OnceLock<(Vec<u8>, Vec<u8>)> = OnceLock::new();
    IDENTITY.get_or_init(|| {
        let key = RsaPrivateKey::new(&mut rand::rngs::OsRng, 2048).expect("rsa key");
        let pem = key.to_pkcs8_pem(LineEnding::LF).expect("pkcs8 pem");
        let key_pair = rcgen::KeyPair::from_pkcs8_pem_and_sign_algo(&pem, &rcgen::PKCS_RSA_SHA256)
            .expect("ring accepts the RSA key");
        let mut params = rcgen::CertificateParams::new(Vec::<String>::new()).expect("params");
        params.distinguished_name.push(rcgen::DnType::CommonName, "PDF SAK LTV Test Signer");
        let cert = params.self_signed(&key_pair).expect("self signed");
        (cert.der().to_vec(), key.to_pkcs8_der().expect("pkcs8 der").as_bytes().to_vec())
    })
}

fn options() -> SignOptions {
    SignOptions {
        page: 1,
        rect: None,
        reason: "Approval".to_string(),
        location: "Istanbul".to_string(),
        contact: String::new(),
        appearance: true,
        signer_name: Some("LTV Test Signer".to_string()),
    }
}

fn signed_pdf() -> Vec<u8> {
    let mut doc = common::build_text_doc(1, "LTV sample", "LTV sample document");
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save");
    let (cert_der, key_der) = identity();
    sign::sign_pdf(&bytes, cert_der, key_der, &[], &options()).expect("sign")
}

/// The catalog DSS dictionary of `pdf`.
fn dss(pdf: &[u8]) -> lopdf::Dictionary {
    let doc = Document::load_mem(pdf).expect("load");
    let root = doc.trailer.get(b"Root").and_then(Object::as_reference).expect("root");
    let catalog = doc.get_object(root).and_then(Object::as_dict).expect("catalog");
    let dss_id =
        catalog.get(b"DSS").and_then(Object::as_reference).expect("the catalog must carry a /DSS after archiving");
    doc.get_object(dss_id).and_then(Object::as_dict).expect("dss").clone()
}

#[test]
fn the_dss_is_appended_and_the_signature_survives() {
    let signed = signed_pdf();
    let (with_dss, report) = add_validation_data(&signed).expect("archive validation data");

    // The signed revision is untouched, which is what keeps the signature valid.
    assert!(with_dss.starts_with(&signed), "the appended update must not rewrite the original bytes");
    assert_eq!(report.signatures, 1);
    assert!(report.certificates >= 1, "the signer certificate must be archived");
    assert_eq!(report.vri_keys.len(), 1);

    let verification = sign::verify_signatures(&with_dss);
    assert_eq!(verification.signatures.len(), 1);
    let info = &verification.signatures[0];
    assert!(info.digest_matches, "digest: {:?}", info.notes);
    assert!(info.signature_valid, "signature: {:?}", info.notes);
    assert!(!info.modified_after_signing, "archiving is not a modification: {:?}", info.notes);
    assert!(info.superseded_by_later_revision, "the DSS revision comes after the signature");

    // Structure: /Certs holds one stream per certificate and /VRI is keyed by
    // the uppercase hex SHA-1 of the signature /Contents value.
    let dss = dss(&with_dss);
    let certs = dss.get(b"Certs").and_then(Object::as_array).expect("certs").clone();
    assert_eq!(certs.len(), report.certificates);
    let vri = dss.get(b"VRI").and_then(Object::as_dict).expect("vri");
    assert_eq!(vri.len(), 1);

    let doc = Document::load_mem(&with_dss).expect("load");
    let contents = doc
        .objects
        .values()
        .filter_map(|object| object.as_dict().ok())
        .filter_map(|dict| dict.get(b"Contents").ok())
        .find_map(|value| match value {
            Object::String(bytes, _) => Some(bytes.clone()),
            _ => None,
        })
        .expect("signature contents");
    let expected_key = Sha1::digest(&contents).iter().map(|byte| format!("{byte:02X}")).collect::<String>();
    assert_eq!(report.vri_keys[0], expected_key);
    assert!(vri.get(expected_key.as_bytes()).is_ok(), "the VRI key must be the contents hash");
}

#[test]
fn archiving_twice_does_not_duplicate_certificates() {
    let signed = signed_pdf();
    let (once, first) = add_validation_data(&signed).expect("first archive");
    let (twice, second) = add_validation_data(&once).expect("second archive");
    assert_eq!(second.certificates, first.certificates, "certificates must not pile up");
    assert!(twice.starts_with(&once));
    let dss = dss(&twice);
    let certs = dss.get(b"Certs").and_then(Object::as_array).expect("certs");
    assert_eq!(certs.len(), first.certificates);
    // The signature is still valid after two updates.
    let verification = sign::verify_signatures(&twice);
    assert!(verification.signatures[0].digest_matches);
    assert!(verification.signatures[0].signature_valid);
}

#[test]
fn a_document_without_signatures_has_nothing_to_archive() {
    let mut doc = common::build_text_doc(1, "Unsigned", "Unsigned document");
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save");
    let error = add_validation_data(&bytes).expect_err("there is nothing to archive");
    assert!(error.to_string().contains("no signature"), "unexpected error: {error}");
}
