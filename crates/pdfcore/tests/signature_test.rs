//! Real digital signature tests.
//!
//! Every test here exercises the production code paths: certificates are
//! generated at runtime, PKCS#12 containers are built and then parsed back,
//! signatures are produced and verified, and the CMS bytes are parsed with the
//! independent RustCrypto `cms` parser as well as by the library itself.
//! Nothing is mocked and no signature is simulated.

mod common;

use cbc::cipher::{block_padding::Pkcs7, BlockModeEncrypt, KeyIvInit};
use der::{Decode, Encode};
use hmac::{Hmac, Mac};
use lopdf::{dictionary, Document, Object, StringFormat};
use pdfcore::sign::{self, SignOptions};
use rand::rngs::OsRng;
use rsa::pkcs8::{EncodePrivateKey, LineEnding};
use rsa::RsaPrivateKey;
use sha1::Sha1;
use sha2::Sha256;
use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// Test identities
// ---------------------------------------------------------------------------

struct Identity {
    cert_der: Vec<u8>,
    key_pkcs8_der: Vec<u8>,
    common_name: String,
}

struct ChainIdentity {
    leaf: Identity,
    ca: Identity,
}

fn ecdsa_identity() -> &'static Identity {
    static IDENTITY: OnceLock<Identity> = OnceLock::new();
    IDENTITY.get_or_init(|| {
        let key_pair = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).expect("ec key");
        make_identity(&key_pair, "PDF SAK ECDSA Test Signer", rcgen::IsCa::ExplicitNoCa)
    })
}

fn rsa_identity() -> &'static Identity {
    static IDENTITY: OnceLock<Identity> = OnceLock::new();
    IDENTITY.get_or_init(|| {
        // A 2048-bit key keeps the test honest while staying tolerable in a
        // debug build; generation happens once per test binary.
        let key = RsaPrivateKey::new(&mut OsRng, 2048).expect("rsa key");
        let pem = key.to_pkcs8_pem(LineEnding::LF).expect("pkcs8 pem");
        let key_pair = rcgen::KeyPair::from_pkcs8_pem_and_sign_algo(&pem, &rcgen::PKCS_RSA_SHA256)
            .expect("ring accepts the RSA key");
        make_identity(&key_pair, "PDF SAK RSA Test Signer", rcgen::IsCa::ExplicitNoCa)
    })
}

fn chained_identity() -> &'static ChainIdentity {
    static IDENTITY: OnceLock<ChainIdentity> = OnceLock::new();
    IDENTITY.get_or_init(|| {
        let ca_key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).expect("ca key");
        let mut ca_params = rcgen::CertificateParams::new(Vec::<String>::new()).expect("ca params");
        ca_params.distinguished_name.push(rcgen::DnType::CommonName, "PDF SAK Test Root CA");
        ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        let ca_cert = ca_params.self_signed(&ca_key).expect("ca cert");
        let ca = Identity {
            cert_der: ca_cert.der().to_vec(),
            key_pkcs8_der: ca_key.serialize_der(),
            common_name: "PDF SAK Test Root CA".to_string(),
        };

        let leaf_key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).expect("leaf key");
        let mut leaf_params = rcgen::CertificateParams::new(Vec::<String>::new()).expect("leaf params");
        leaf_params.distinguished_name.push(rcgen::DnType::CommonName, "PDF SAK Chained Test Signer");
        leaf_params.is_ca = rcgen::IsCa::ExplicitNoCa;
        let leaf_cert = leaf_params.signed_by(&leaf_key, &ca_cert, &ca_key).expect("leaf cert");
        let leaf = Identity {
            cert_der: leaf_cert.der().to_vec(),
            key_pkcs8_der: leaf_key.serialize_der(),
            common_name: "PDF SAK Chained Test Signer".to_string(),
        };
        ChainIdentity { leaf, ca }
    })
}

fn make_identity(key_pair: &rcgen::KeyPair, common_name: &str, is_ca: rcgen::IsCa) -> Identity {
    let mut params = rcgen::CertificateParams::new(Vec::<String>::new()).expect("params");
    params.distinguished_name.push(rcgen::DnType::CommonName, common_name);
    params.is_ca = is_ca;
    let cert = params.self_signed(key_pair).expect("self signed");
    Identity {
        cert_der: cert.der().to_vec(),
        key_pkcs8_der: key_pair.serialize_der(),
        common_name: common_name.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Test documents
// ---------------------------------------------------------------------------

/// Builds a small two page PDF with uncompressed content streams so the raw
/// bytes can be compared and tampered with.
fn build_test_pdf() -> Vec<u8> {
    let mut doc = common::build_text_doc(2, "Signature test", "Signature test document");
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save test pdf");
    bytes
}

fn sign_options() -> SignOptions {
    SignOptions {
        page: 1,
        rect: None,
        reason: "Approval".to_string(),
        location: "Istanbul".to_string(),
        contact: "signer@example.com".to_string(),
        appearance: true,
        signer_name: Some("Visible Test Signer".to_string()),
    }
}

fn sign_with(identity: &Identity, chain: &[Vec<u8>], options: &SignOptions) -> Vec<u8> {
    let pdf = build_test_pdf();
    sign::sign_pdf(&pdf, &identity.cert_der, &identity.key_pkcs8_der, chain, options).expect("sign pdf")
}

/// Finds the signature dictionary in a signed document.
fn find_signature_dict(doc: &Document) -> &lopdf::Dictionary {
    doc.objects
        .values()
        .find_map(|object| {
            let dict = object.as_dict().ok()?;
            let is_sig = dict
                .get(b"Type")
                .ok()
                .and_then(|value| value.as_name().ok())
                .map(|name| name == b"Sig")
                .unwrap_or(false);
            if is_sig {
                Some(dict)
            } else {
                None
            }
        })
        .expect("signature dictionary present")
}

// ---------------------------------------------------------------------------
// DER helpers for building PKCS#12 containers in tests
// ---------------------------------------------------------------------------

fn der_tlv(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    let mut length = content.len();
    if length < 0x80 {
        out.push(length as u8);
    } else {
        let mut bytes = Vec::new();
        while length > 0 {
            bytes.push((length & 0xFF) as u8);
            length >>= 8;
        }
        bytes.reverse();
        out.push(0x80 | bytes.len() as u8);
        out.extend_from_slice(&bytes);
    }
    out.extend_from_slice(content);
    out
}

fn der_seq(parts: &[Vec<u8>]) -> Vec<u8> {
    der_tlv(0x30, &parts.concat())
}

fn der_oid(oid: &str) -> Vec<u8> {
    let oid = const_oid::ObjectIdentifier::new_unwrap(oid);
    der_tlv(0x06, oid.as_bytes())
}

fn der_octet(bytes: &[u8]) -> Vec<u8> {
    der_tlv(0x04, bytes)
}

fn der_int(value: i64) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut value = value;
    while value > 0 {
        bytes.push((value & 0xFF) as u8);
        value >>= 8;
    }
    if bytes.is_empty() {
        bytes.push(0);
    }
    bytes.reverse();
    if bytes[0] & 0x80 != 0 {
        bytes.insert(0, 0);
    }
    der_tlv(0x02, &bytes)
}

fn der_context_zero(inner: &[u8]) -> Vec<u8> {
    der_tlv(0xA0, inner)
}

// ---------------------------------------------------------------------------
// PKCS#12 construction (test side)
// ---------------------------------------------------------------------------

const PKCS12_PBE_3DES_3KEY: &str = "1.2.840.113549.1.12.1.3";
const PKCS12_KEY_BAG: &str = "1.2.840.113549.1.12.10.1.1";
const PKCS12_SHROUDED_KEY_BAG: &str = "1.2.840.113549.1.12.10.1.2";
const PKCS12_CERT_BAG: &str = "1.2.840.113549.1.12.10.1.3";
const X509_CERTIFICATE: &str = "1.2.840.113549.1.9.22.1";
const OID_DATA: &str = "1.2.840.113549.1.7.1";
const OID_ENCRYPTED_DATA: &str = "1.2.840.113549.1.7.6";

/// Builds a real password protected PKCS#12 container holding the certificate
/// and the (encrypted) private key. `pbes2` selects AES-256-CBC with
/// PBKDF2-HMAC-SHA256, otherwise the legacy PKCS#12 3DES PBE is used.
fn build_pfx(identity: &Identity, password: &str, pbes2: bool, sha256_mac: bool) -> Vec<u8> {
    let salt = b"0123456789abcdef";
    let iv = b"fedcba9876543210";
    let iterations = 2048i32;

    // Cert bag in a plain SafeContents.
    let cert_bag_inner = der_seq(&[der_oid(X509_CERTIFICATE), der_context_zero(&der_octet(&identity.cert_der))]);
    let cert_bag = der_seq(&[der_oid(PKCS12_CERT_BAG), der_context_zero(&cert_bag_inner)]);
    let cert_contents = der_seq(std::slice::from_ref(&cert_bag));

    // Two real container layouts are covered:
    // * PBES2: a keyBag inside an encryptedData ContentInfo (OpenSSL/Windows);
    // * legacy: a pkcs8ShroudedKeyBag with PKCS#12 PBE (3DES) inside a plain
    //   data ContentInfo.
    let content_infos: Vec<Vec<u8>> = if pbes2 {
        let key_bag = der_seq(&[der_oid(PKCS12_KEY_BAG), der_context_zero(&identity.key_pkcs8_der)]);
        let key_contents = der_seq(&[key_bag]);
        let params =
            pkcs5::pbes2::Parameters::pbkdf2_sha256_aes256cbc(iterations as u32, salt, iv).expect("pbes2 params");
        let scheme = pkcs5::EncryptionScheme::from(params);
        let ciphertext = scheme.encrypt(password.as_bytes(), &key_contents).expect("encrypt key");
        let algorithm = scheme.to_der().expect("alg der");
        let enc_content_info = der_seq(&[der_oid(OID_DATA), algorithm, der_tlv(0x80, &ciphertext)]);
        let encrypted_data = der_seq(&[der_int(0), enc_content_info]);
        let encrypted_ci = der_seq(&[der_oid(OID_ENCRYPTED_DATA), der_context_zero(&encrypted_data)]);
        let certs_ci = der_seq(&[der_oid(OID_DATA), der_context_zero(&der_octet(&cert_contents))]);
        vec![encrypted_ci, certs_ci]
    } else {
        let key = pkcs12::kdf::derive_key_utf8::<Sha1>(
            password,
            salt,
            pkcs12::kdf::Pkcs12KeyType::EncryptionKey,
            iterations,
            24,
        )
        .expect("derive key");
        let iv = pkcs12::kdf::derive_key_utf8::<Sha1>(password, salt, pkcs12::kdf::Pkcs12KeyType::Iv, iterations, 8)
            .expect("derive iv");
        let ciphertext = cbc::Encryptor::<des::TdesEde3>::new_from_slices(&key, &iv)
            .expect("3des setup")
            .encrypt_padded_vec::<Pkcs7>(&identity.key_pkcs8_der);
        let pbe_params = der_seq(&[der_octet(salt), der_int(iterations as i64)]);
        let algorithm = der_seq(&[der_oid(PKCS12_PBE_3DES_3KEY), pbe_params]);
        let encrypted_key = der_seq(&[algorithm, der_octet(&ciphertext)]);
        let shrouded = der_seq(&[der_oid(PKCS12_SHROUDED_KEY_BAG), der_context_zero(&encrypted_key)]);
        let contents = der_seq(&[shrouded, cert_bag]);
        vec![der_seq(&[der_oid(OID_DATA), der_context_zero(&der_octet(&contents))])]
    };

    // AuthenticatedSafe ::= SEQUENCE OF ContentInfo.
    let authenticated_safe = der_seq(&content_infos);

    // Integrity MAC over the authSafe OCTET STRING content.
    let mac_salt = b"mac-salt-0123456";
    let mac_digest = if sha256_mac {
        let key =
            pkcs12::kdf::derive_key_utf8::<Sha256>(password, mac_salt, pkcs12::kdf::Pkcs12KeyType::Mac, iterations, 32)
                .expect("mac key");
        let mut mac = Hmac::<Sha256>::new_from_slice(&key).expect("hmac");
        mac.update(&authenticated_safe);
        (const_oid::ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.1"), mac.finalize().into_bytes().to_vec())
    } else {
        let key =
            pkcs12::kdf::derive_key_utf8::<Sha1>(password, mac_salt, pkcs12::kdf::Pkcs12KeyType::Mac, iterations, 20)
                .expect("mac key");
        let mut mac = Hmac::<Sha1>::new_from_slice(&key).expect("hmac");
        mac.update(&authenticated_safe);
        (const_oid::ObjectIdentifier::new_unwrap("1.3.14.3.2.26"), mac.finalize().into_bytes().to_vec())
    };
    let digest_info = der_seq(&[
        // DigestInfo ::= SEQUENCE { digestAlgorithm AlgorithmIdentifier,
        //                           digest OCTET STRING }
        der_seq(&[der_oid(&mac_digest.0.to_string())]),
        der_octet(&mac_digest.1),
    ]);
    let mac_data = der_seq(&[digest_info, der_octet(mac_salt), der_int(iterations as i64)]);

    // PFX ::= SEQUENCE { version 3, authSafe, macData }.
    let auth_safe = der_seq(&[der_oid(OID_DATA), der_context_zero(&der_octet(&authenticated_safe))]);
    der_seq(&[der_int(3), auth_safe, mac_data])
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn signature_rsa_sign_and_verify() {
    let identity = rsa_identity();
    let signed = sign_with(identity, &[], &sign_options());

    let report = sign::verify_signatures(&signed);
    assert_eq!(report.signatures.len(), 1, "exactly one signature expected");
    let info = &report.signatures[0];

    assert!(info.signature_valid, "RSA signature must verify: {info:?}");
    assert!(info.digest_matches, "byte range digest must match");
    assert!(info.covers_whole_document, "byte range must cover the file");
    assert!(!info.modified_after_signing);
    assert_eq!(info.sub_filter, "adbe.pkcs7.detached");
    assert_eq!(info.trust, "unknown", "offline verification never claims trust");
    assert!(info.signer.subject.contains(&identity.common_name));
    assert_eq!(info.signer.issuer, info.signer.subject, "self signed");
    assert!(!info.signer.expired);
    assert_eq!(info.chain.len(), 1);
    assert!(info.chain_linked);
    assert!(info.self_signed_chain);
    assert!(info.algorithm.contains("RSA"), "algorithm: {}", info.algorithm);
    assert!(info.algorithm.contains("SHA-256"));
    assert!(info.signing_time.is_some());
    assert_eq!(info.field_name, "Signature1");

    // The document must stay a valid PDF with the signature field present.
    let doc = Document::load_mem(&signed).expect("signed pdf loads");
    assert_eq!(doc.get_pages().len(), 2);
    let sig = find_signature_dict(&doc);
    assert_eq!(sig.get(b"SubFilter").unwrap().as_name().unwrap(), b"adbe.pkcs7.detached");
    assert!(sig.get(b"Contents").is_ok());
    assert!(sig.get(b"ByteRange").is_ok());
    assert_eq!(sig.get(b"Filter").unwrap().as_name().unwrap(), b"Adobe.PPKLite");
    assert_eq!(sig.get(b"Reason").unwrap().as_str().unwrap(), b"Approval");

    // The original pages and their content streams are untouched.
    let original = Document::load_mem(&build_test_pdf()).unwrap();
    let original_pages = original.get_pages();
    let signed_pages = doc.get_pages();
    assert_eq!(original_pages.len(), signed_pages.len());
    for (number, page_id) in &original_pages {
        let signed_id = signed_pages.get(number).unwrap();
        assert_eq!(
            original.get_page_content(*page_id),
            doc.get_page_content(*signed_id),
            "page {number} content changed"
        );
    }

    // The widget exists and points at the signature value.
    let has_widget = doc.objects.values().any(|object| {
        object
            .as_dict()
            .map(|dict| {
                dict.get(b"FT").ok().and_then(|v| v.as_name().ok()) == Some(b"Sig".as_slice())
                    && dict.get(b"Subtype").ok().and_then(|v| v.as_name().ok()) == Some(b"Widget".as_slice())
            })
            .unwrap_or(false)
    });
    assert!(has_widget, "signature widget field expected");
}

#[test]
fn signature_ecdsa_p256_sign_and_verify() {
    let identity = ecdsa_identity();
    let signed = sign_with(identity, &[], &sign_options());

    let report = sign::verify_signatures(&signed);
    assert_eq!(report.signatures.len(), 1);
    let info = &report.signatures[0];
    assert!(info.signature_valid, "ECDSA signature must verify: {info:?}");
    assert!(info.digest_matches);
    assert!(info.covers_whole_document);
    assert!(!info.modified_after_signing);
    assert!(info.algorithm.contains("ECDSA"), "algorithm: {}", info.algorithm);
    assert!(info.signer.subject.contains(&identity.common_name));
    assert!(info.self_signed_chain);
    assert_eq!(info.trust, "unknown");
}

#[test]
fn signature_chain_links_to_self_signed_root() {
    let chained = chained_identity();
    let signed = sign_with(&chained.leaf, std::slice::from_ref(&chained.ca.cert_der), &sign_options());

    let report = sign::verify_signatures(&signed);
    let info = &report.signatures[0];
    assert!(info.signature_valid, "{info:?}");
    assert!(info.digest_matches);
    assert_eq!(info.chain.len(), 2, "leaf plus CA");
    assert!(info.chain[0].subject.contains(&chained.leaf.common_name), "chain starts at the signer");
    assert!(info.chain[1].subject.contains(&chained.ca.common_name));
    assert!(info.chain[1].is_ca, "root carries basicConstraints CA");
    assert!(info.chain_linked, "leaf must be issued by the CA");
    assert!(info.self_signed_chain, "chain ends in a self signed root");
    assert_eq!(info.trust, "unknown");
}

#[test]
fn signature_tamper_detection() {
    let identity = ecdsa_identity();
    let signed = sign_with(identity, &[], &sign_options());

    // Flip one byte inside the original page content stream (covered by the
    // byte range, so the digest must stop matching).
    let needle = b"Signature test page 1";
    let position = signed
        .windows(needle.len())
        .position(|window| window == needle)
        .expect("page text present in the signed bytes");
    let mut tampered = signed.clone();
    tampered[position + 5] ^= 0x01;

    let report = sign::verify_signatures(&tampered);
    let info = &report.signatures[0];
    assert!(!info.digest_matches, "tampering must break the digest");
    assert!(info.modified_after_signing, "tampering must be reported");
    assert!(info.covers_whole_document, "coverage itself is unchanged");
}

#[test]
fn signature_byte_range_and_cms_structure() {
    let identity = ecdsa_identity();
    let signed = sign_with(identity, &[], &sign_options());

    // Parse /ByteRange and /Contents directly from the raw bytes.
    let doc = Document::load_mem(&signed).unwrap();
    let sig = find_signature_dict(&doc);
    let range = sig.get(b"ByteRange").unwrap().as_array().unwrap();
    let numbers: Vec<i64> = range.iter().map(|value| value.as_i64().expect("integer byte range")).collect();
    assert_eq!(numbers.len(), 4);
    assert_eq!(numbers[0], 0);

    let contents = match sig.get(b"Contents").unwrap() {
        Object::String(bytes, format) => {
            assert_eq!(*format, StringFormat::Hexadecimal);
            bytes.clone()
        }
        other => panic!("unexpected /Contents object: {other:?}"),
    };

    // Covered bytes == file length minus the complete `<...>` token.
    let token_length = contents.len() * 2 + 2;
    let covered = (numbers[1] + numbers[3]) as usize;
    assert_eq!(covered, signed.len() - token_length);
    assert_eq!(numbers[2] as usize, numbers[1] as usize + token_length);

    // The CMS DER starts at the beginning of /Contents and parses. Zero bytes
    // may occur inside the DER, so the exact length comes from the length
    // field, not from scanning for the first zero.
    let (header, length) = if contents[1] < 0x80 {
        (2usize, contents[1] as usize)
    } else {
        let count = (contents[1] & 0x7F) as usize;
        let mut length = 0usize;
        for byte in &contents[2..2 + count] {
            length = length * 256 + *byte as usize;
        }
        (2 + count, length)
    };
    let der = contents[..header + length].to_vec();
    assert!(contents[header + length..].iter().all(|byte| *byte == 0), "the rest of /Contents is zero padding");

    // Independent parser: RustCrypto `cms`.
    let content_info = cms::content_info::ContentInfo::from_der(&der).expect("cms parses");
    let signed_data = content_info.content.decode_as::<cms::signed_data::SignedData>().expect("SignedData parses");
    assert!(signed_data.encap_content_info.econtent.is_none(), "detached");
    assert_eq!(
        signed_data.encap_content_info.econtent_type,
        const_oid::ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.1")
    );
    assert_eq!(signed_data.signer_infos.0.len(), 1);
    let signer_info = signed_data.signer_infos.0.iter().next().unwrap();
    assert!(signer_info.signed_attrs.is_some());
    assert!(!signer_info.signature.as_bytes().is_empty());
    let certificates = signed_data.certificates.as_ref().expect("certificates embedded");
    assert_eq!(certificates.0.len(), 1);
}

#[test]
fn signature_appearance_stream_is_present() {
    let identity = ecdsa_identity();
    let signed = sign_with(identity, &[], &sign_options());
    let doc = Document::load_mem(&signed).unwrap();
    let has_appearance = doc.objects.values().any(|object| {
        object
            .as_stream()
            .map(|stream| stream.dict.get(b"Subtype").ok().and_then(|v| v.as_name().ok()) == Some(b"Form".as_slice()))
            .unwrap_or(false)
    });
    assert!(has_appearance, "visible appearance stream expected");

    // Without the appearance option there must be no form XObject.
    let invisible = SignOptions { appearance: false, ..sign_options() };
    let signed = sign_with(identity, &[], &invisible);
    let doc = Document::load_mem(&signed).unwrap();
    let has_appearance = doc.objects.values().any(|object| {
        object
            .as_stream()
            .map(|stream| stream.dict.get(b"Subtype").ok().and_then(|v| v.as_name().ok()) == Some(b"Form".as_slice()))
            .unwrap_or(false)
    });
    assert!(!has_appearance);
    // The signature still verifies without an appearance.
    let report = sign::verify_signatures(&signed);
    assert!(report.signatures[0].signature_valid);
}

#[test]
fn signature_pkcs12_roundtrip_and_wrong_password() {
    let identity = ecdsa_identity();
    let pfx = build_pfx(identity, "correct horse battery staple", true, true);

    let parsed = sign::parse_pkcs12(&pfx, "correct horse battery staple").expect("parse pfx");
    assert_eq!(parsed.cert_der, identity.cert_der);
    assert_eq!(parsed.key_pkcs8_der, identity.key_pkcs8_der);
    assert!(parsed.chain_der.is_empty());

    // Wrong password is an error, never a fallback.
    match sign::parse_pkcs12(&pfx, "wrong password") {
        Err(pdfcore::error::PdfError::WrongPassword) => {}
        Err(other) => panic!("expected WrongPassword, got {other}"),
        Ok(_) => panic!("a wrong password must never be accepted"),
    }

    // The parsed identity signs a PDF that verifies.
    let pdf = build_test_pdf();
    let signed = sign::sign_pdf(&pdf, &parsed.cert_der, &parsed.key_pkcs8_der, &parsed.chain_der, &sign_options())
        .expect("sign with pfx identity");
    let report = sign::verify_signatures(&signed);
    assert!(report.signatures[0].signature_valid);
}

#[test]
fn signature_pkcs12_legacy_3des() {
    let identity = ecdsa_identity();
    let pfx = build_pfx(identity, "legacy-password", false, false);

    let parsed = sign::parse_pkcs12(&pfx, "legacy-password").expect("parse legacy pfx");
    assert_eq!(parsed.cert_der, identity.cert_der);
    assert_eq!(parsed.key_pkcs8_der, identity.key_pkcs8_der);

    match sign::parse_pkcs12(&pfx, "nope") {
        Err(pdfcore::error::PdfError::WrongPassword) => {}
        Err(other) => panic!("expected WrongPassword, got {other}"),
        Ok(_) => panic!("a wrong password must never be accepted"),
    }
}

#[test]
fn signature_pkcs12_garbage_is_an_error() {
    for garbage in [b"not a pfx".as_slice(), &[0x30, 0x03, 0x02, 0x01, 0x03][..], &[0x00, 0x01, 0x02, 0x03, 0x04][..]] {
        match sign::parse_pkcs12(garbage, "anything") {
            Err(_) => {}
            Ok(_) => panic!("garbage must not parse as PKCS#12"),
        }
    }
    // Empty input must not panic either.
    assert!(sign::parse_pkcs12(&[], "").is_err());
}

#[test]
fn signature_rejects_mismatched_key() {
    let rsa = rsa_identity();
    let ec = ecdsa_identity();
    let pdf = build_test_pdf();
    let error = sign::sign_pdf(&pdf, &rsa.cert_der, &ec.key_pkcs8_der, &[], &sign_options());
    assert!(error.is_err(), "a mismatched key must be rejected");
}

#[test]
fn signature_rejects_bad_rect_and_page() {
    let identity = ecdsa_identity();
    let pdf = build_test_pdf();
    let options = SignOptions { rect: Some([10.0, 10.0, 10.0, 20.0]), ..sign_options() };
    assert!(sign::sign_pdf(&pdf, &identity.cert_der, &identity.key_pkcs8_der, &[], &options).is_err());

    let options = SignOptions { page: 99, ..sign_options() };
    assert!(sign::sign_pdf(&pdf, &identity.cert_der, &identity.key_pkcs8_der, &[], &options).is_err());
}

#[test]
fn signature_verify_reports_nothing_for_unsigned_pdf() {
    let report = sign::verify_signatures(&build_test_pdf());
    assert!(report.signatures.is_empty());
    assert!(report.warnings.is_empty(), "a readable unsigned PDF is not a problem: {:?}", report.warnings);
}

#[test]
fn an_unparseable_file_is_not_mistaken_for_unsigned() {
    // An empty signature list used to come back for a file that could not be
    // parsed at all, which reads exactly like "this PDF has no signatures".
    let report = sign::verify_signatures(b"this is not a pdf");
    assert!(report.signatures.is_empty());
    assert!(!report.warnings.is_empty(), "unreadable and unsigned must be distinguishable");
    assert!(report.warnings[0].contains("could not be parsed"), "unexpected warning: {:?}", report.warnings);
}

#[test]
fn a_byte_range_that_cannot_be_added_up_is_reported_not_panicked_on() {
    // start + length overflows: the old arithmetic wrapped in release builds.
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    doc.add_object(dictionary! {
        "Type" => "Sig",
        "Filter" => "Adobe.PPKLite",
        "SubFilter" => "adbe.pkcs7.detached",
        // start + length overflows usize for this pair.
        "ByteRange" => vec![
            Object::Integer(i64::MAX),
            Object::Integer(i64::MAX),
            Object::Integer(0),
            Object::Integer(0),
        ],
        "Contents" => Object::String(vec![0u8; 8], StringFormat::Hexadecimal),
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => Vec::<Object>::new(), "Count" => 0 }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save");

    let report = sign::verify_signatures(&bytes);
    assert_eq!(report.signatures.len(), 1, "the signature dictionary must be found");
    let info = &report.signatures[0];
    assert!(!info.covers_whole_document);
    assert!(
        info.notes.iter().any(|note| note.contains("no usable /ByteRange")),
        "an impossible range must be reported as unusable, got: {:?}",
        info.notes
    );
}

#[test]
fn signing_an_already_signed_pdf_keeps_the_first_signature() {
    // The signing path writes an incremental update, so a second signature must
    // land in a new revision and leave the first one intact. Long-term
    // validation and counter-signatures both depend on this property.
    let first = sign_with(ecdsa_identity(), &[], &sign_options());
    let second = sign::sign_pdf(&first, &rsa_identity().cert_der, &rsa_identity().key_pkcs8_der, &[], &sign_options())
        .expect("the second signature must be written as an update");

    assert!(second.starts_with(&first), "the original revision must stay byte-identical");

    let report = sign::verify_signatures(&second);
    assert_eq!(
        report.signatures.len(),
        2,
        "both signatures must be found: {:?}",
        report.signatures.iter().map(|info| info.field_name.clone()).collect::<Vec<_>>()
    );
    for info in &report.signatures {
        assert!(info.digest_matches, "digest mismatch: {:?}", info.notes);
        assert!(info.signature_valid, "signature invalid: {:?}", info.notes);
        // An appended revision is not a modification: both signatures stay
        // valid, the first one for the revision it signed.
        assert!(!info.modified_after_signing, "reported as modified: {:?}", info.notes);
    }
    assert_eq!(
        report.signatures.iter().filter(|info| info.superseded_by_later_revision).count(),
        1,
        "exactly the first signature is superseded by the second"
    );
    assert_eq!(
        report.signatures.iter().filter(|info| info.covers_whole_document).count(),
        1,
        "only the last signature covers the whole file"
    );
}
