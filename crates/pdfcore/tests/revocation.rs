//! Online revocation checking (OCSP first, then the CRL).
//!
//! The fixtures in tests/fixtures/revocation are a throwaway PKI generated once
//! with the OpenSSL command line (see the README there): an RSA CA, P-256
//! signer certificates with AIA/CRLDP, a delegated OCSP responder, a
//! self-signed impostor responder, real OpenSSL OCSP responses and a CRL. No
//! network is used: the fetcher handed to the verifier serves those files, so
//! every answer is evaluated by the production code exactly as a downloaded
//! one would be.

mod common;

use der::asn1::{BitString, OctetString};
use der::{Decode, Encode};
use pdfcore::error::PdfError;
use pdfcore::revocation::{
    check_online, revocation_endpoints, RevocationFetch, RevocationInfo, RevocationSource, RevocationStatus,
    RevocationSubject, MAX_CRL_BYTES, MAX_OCSP_RESPONSE_BYTES,
};
use pdfcore::sign::{self, SignOptions};
use std::cell::RefCell;
use std::path::Path;
use x509_cert::crl::CertificateList;
use x509_ocsp::{BasicOcspResponse, OcspResponse};

/// `thisUpdate` of every OCSP response and of the CRL (2026-10-07T06:49:41Z),
/// which is also when the revoked certificate was revoked.
const FIXTURE_TIME: u64 = 1_791_355_781;
/// `nextUpdate` of every OCSP response and of the CRL (2126-09-13T06:49:41Z).
const NEXT_UPDATE: u64 = 4_944_955_781;
/// The evaluation time for the happy paths: an hour after the answers.
const NOW: u64 = FIXTURE_TIME + 3600;

const OCSP_URL: &str = "http://ocsp.omnioffice.test/";
const CRL_URL: &str = "http://crl.omnioffice.test/ca.crl";

fn fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/revocation").join(name);
    std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn signer() -> RevocationSubject {
    RevocationSubject::new(&fixture("signer.der"), &fixture("ca.der")).expect("the CA issued the signer")
}

fn revoked_signer() -> RevocationSubject {
    RevocationSubject::new(&fixture("revoked.der"), &fixture("ca.der")).expect("the CA issued the revoked signer")
}

/// Flips one bit of the signature of a DER OCSPResponse.
fn tamper_ocsp_signature(response: &[u8]) -> Vec<u8> {
    let mut outer = OcspResponse::from_der(response).expect("ocsp response");
    let bytes = outer.response_bytes.as_mut().expect("response bytes");
    let mut basic = BasicOcspResponse::from_der(bytes.response.as_bytes()).expect("basic response");
    let mut signature = basic.signature.raw_bytes().to_vec();
    let last = signature.len() - 1;
    signature[last] ^= 0x01;
    basic.signature = BitString::from_bytes(&signature).expect("bit string");
    bytes.response = OctetString::new(basic.to_der().expect("encode basic")).expect("octets");
    outer.to_der().expect("encode response")
}

/// Flips one bit of the signature of a DER CRL.
fn tamper_crl_signature(crl: &[u8]) -> Vec<u8> {
    let mut list = CertificateList::from_der(crl).expect("crl");
    let mut signature = list.signature.raw_bytes().to_vec();
    signature[10] ^= 0x01;
    list.signature = BitString::from_bytes(&signature).expect("bit string");
    list.to_der().expect("encode crl")
}

// ---------------------------------------------------------------------------
// Certificates and requests
// ---------------------------------------------------------------------------

#[test]
fn the_endpoints_come_from_aia_and_crldp_and_only_http_is_kept() {
    let endpoints = revocation_endpoints(&fixture("signer.der")).expect("endpoints");
    assert_eq!(endpoints.ocsp, vec![OCSP_URL.to_string()]);
    // The ldap:// distribution point is skipped: the app only speaks HTTP.
    assert_eq!(endpoints.crl, vec![CRL_URL.to_string()]);
    // The CA certificate names nothing.
    let ca = revocation_endpoints(&fixture("ca.der")).expect("ca endpoints");
    assert!(ca.ocsp.is_empty() && ca.crl.is_empty());
    assert!(revocation_endpoints(b"not a certificate").is_err());
}

#[test]
fn the_ocsp_request_is_byte_identical_to_openssls() {
    // `openssl ocsp -issuer ca.pem -cert signer.pem -no_nonce -reqout ...`
    assert_eq!(signer().ocsp_request().expect("request"), fixture("ocsp-request-signer.der"));
}

#[test]
fn the_issuer_must_really_have_issued_the_certificate() {
    // Right shape, wrong CA: the responder certificate did not issue the signer.
    assert!(RevocationSubject::new(&fixture("signer.der"), &fixture("responder.der")).is_err());
    assert!(RevocationSubject::new(&fixture("signer.der"), &fixture("signer.der")).is_err());
    assert!(RevocationSubject::new(b"junk", &fixture("ca.der")).is_err());
}

// ---------------------------------------------------------------------------
// OCSP responses
// ---------------------------------------------------------------------------

#[test]
fn a_good_answer_from_the_delegated_responder_is_good() {
    let info = signer().evaluate_ocsp(&fixture("ocsp-good.der"), NOW);
    assert_eq!(info.status, RevocationStatus::Good, "{info:?}");
    assert_eq!(info.source, Some(RevocationSource::Ocsp));
    assert_eq!(info.checked_at.as_deref(), Some("2026-10-07T07:49:41Z"));
    assert_eq!(info.revoked_at, None);
    assert_eq!(info.detail, None);
}

#[test]
fn a_good_answer_signed_by_the_ca_itself_is_good() {
    // Signed with the CA key and identified by key hash rather than name.
    let info = signer().evaluate_ocsp(&fixture("ocsp-good-by-ca.der"), NOW);
    assert_eq!(info.status, RevocationStatus::Good, "{info:?}");
}

#[test]
fn a_revoked_answer_reports_the_time_and_reason() {
    let info = revoked_signer().evaluate_ocsp(&fixture("ocsp-revoked.der"), NOW);
    assert_eq!(info.status, RevocationStatus::Revoked, "{info:?}");
    assert_eq!(info.revoked_at.as_deref(), Some("2026-10-07T06:49:41Z"));
    assert_eq!(info.reason.as_deref(), Some("keyCompromise"));

    // The claimed signing time qualifies the revocation but never clears it.
    let before = revoked_signer()
        .with_signing_time(Some(FIXTURE_TIME - 86_400))
        .evaluate_ocsp(&fixture("ocsp-revoked.der"), NOW);
    assert_eq!(before.status, RevocationStatus::Revoked);
    assert!(before.detail.as_deref().unwrap_or_default().contains("revoked after"), "{before:?}");
    let after =
        revoked_signer().with_signing_time(Some(FIXTURE_TIME + 60)).evaluate_ocsp(&fixture("ocsp-revoked.der"), NOW);
    assert_eq!(after.status, RevocationStatus::Revoked);
    assert!(after.detail.as_deref().unwrap_or_default().contains("already revoked"), "{after:?}");
}

#[test]
fn an_unknown_answer_is_unknown() {
    let info = signer().evaluate_ocsp(&fixture("ocsp-unknown.der"), NOW);
    assert_eq!(info.status, RevocationStatus::Unknown, "{info:?}");
}

#[test]
fn an_answer_about_another_certificate_is_rejected() {
    // Validly signed, but the CertID names the other serial number.
    let info = signer().evaluate_ocsp(&fixture("ocsp-revoked.der"), NOW);
    assert_eq!(info.status, RevocationStatus::Error);
    assert!(info.detail.as_deref().unwrap_or_default().contains("CertID"), "{info:?}");
    let info = revoked_signer().evaluate_ocsp(&fixture("ocsp-good.der"), NOW);
    assert_eq!(info.status, RevocationStatus::Error, "a good answer for another serial must not clear this one");
}

#[test]
fn a_tampered_signature_is_rejected() {
    for name in ["ocsp-good.der", "ocsp-good-by-ca.der"] {
        let tampered = tamper_ocsp_signature(&fixture(name));
        let info = signer().evaluate_ocsp(&tampered, NOW);
        assert_eq!(info.status, RevocationStatus::Error, "{name}");
        assert!(info.detail.as_deref().unwrap_or_default().contains("does not verify"), "{name}: {info:?}");
    }
}

#[test]
fn a_responder_the_ca_did_not_authorize_is_rejected() {
    // Same subject name and OCSP-signing EKU as the real responder, correctly
    // self-signed answer - but the CA never issued that certificate.
    let info = signer().evaluate_ocsp(&fixture("ocsp-rogue.der"), NOW);
    assert_eq!(info.status, RevocationStatus::Error);
    assert!(info.detail.as_deref().unwrap_or_default().contains("did not authorize"), "{info:?}");
}

#[test]
fn a_stale_or_future_answer_is_rejected() {
    // The CA-signed answer isolates the time window: the delegated responder
    // certificate shares the same validity, so its own expiry trips first.
    let expired = signer().evaluate_ocsp(&fixture("ocsp-good-by-ca.der"), NEXT_UPDATE + 3600);
    assert_eq!(expired.status, RevocationStatus::Error);
    assert!(expired.detail.as_deref().unwrap_or_default().contains("nextUpdate"), "{expired:?}");
    let early = signer().evaluate_ocsp(&fixture("ocsp-good-by-ca.der"), FIXTURE_TIME - 86_400);
    assert_eq!(early.status, RevocationStatus::Error);
    assert!(early.detail.as_deref().unwrap_or_default().contains("future"), "{early:?}");

    let delegated = signer().evaluate_ocsp(&fixture("ocsp-good.der"), NEXT_UPDATE + 3600);
    assert_eq!(delegated.status, RevocationStatus::Error);
    assert!(delegated.detail.as_deref().unwrap_or_default().contains("expired"), "{delegated:?}");
}

// ---------------------------------------------------------------------------
// CRLs
// ---------------------------------------------------------------------------

#[test]
fn a_crl_miss_is_good_and_a_hit_is_revoked() {
    let miss = signer().evaluate_crl(&fixture("ca.crl"), NOW);
    assert_eq!(miss.status, RevocationStatus::Good, "{miss:?}");
    assert_eq!(miss.source, Some(RevocationSource::Crl));

    let hit = revoked_signer().evaluate_crl(&fixture("ca.crl"), NOW);
    assert_eq!(hit.status, RevocationStatus::Revoked, "{hit:?}");
    assert_eq!(hit.source, Some(RevocationSource::Crl));
    assert_eq!(hit.revoked_at.as_deref(), Some("2026-10-07T06:49:41Z"));
    assert_eq!(hit.reason.as_deref(), Some("keyCompromise"));
}

#[test]
fn a_tampered_or_stale_crl_is_rejected() {
    let tampered = signer().evaluate_crl(&tamper_crl_signature(&fixture("ca.crl")), NOW);
    assert_eq!(tampered.status, RevocationStatus::Error);
    assert!(tampered.detail.as_deref().unwrap_or_default().contains("does not verify"), "{tampered:?}");

    let stale = revoked_signer().evaluate_crl(&fixture("ca.crl"), NEXT_UPDATE + 3600);
    assert_eq!(stale.status, RevocationStatus::Error, "a stale CRL must not be trusted either way");
    assert!(stale.detail.as_deref().unwrap_or_default().contains("nextUpdate"), "{stale:?}");

    // An OCSP response is not a CRL.
    assert_eq!(signer().evaluate_crl(&fixture("ocsp-good.der"), NOW).status, RevocationStatus::Error);
}

// ---------------------------------------------------------------------------
// Hostile input
// ---------------------------------------------------------------------------

#[test]
fn oversized_input_is_refused_before_parsing() {
    let ocsp = signer().evaluate_ocsp(&vec![0x30; MAX_OCSP_RESPONSE_BYTES + 1], NOW);
    assert_eq!(ocsp.status, RevocationStatus::Error);
    assert!(ocsp.detail.as_deref().unwrap_or_default().contains("larger"), "{ocsp:?}");
    let crl = signer().evaluate_crl(&vec![0x30; MAX_CRL_BYTES + 1], NOW);
    assert_eq!(crl.status, RevocationStatus::Error);
    assert!(crl.detail.as_deref().unwrap_or_default().contains("larger"), "{crl:?}");
}

#[test]
fn truncated_and_corrupted_answers_are_errors_never_panics() {
    let subject = signer();
    for name in ["ocsp-good.der", "ocsp-good-by-ca.der", "ca.crl"] {
        let bytes = fixture(name);
        let evaluate = |input: &[u8]| {
            if name.ends_with(".crl") {
                subject.evaluate_crl(input, NOW)
            } else {
                subject.evaluate_ocsp(input, NOW)
            }
        };
        for length in 0..bytes.len() {
            let info = evaluate(&bytes[..length]);
            assert_eq!(info.status, RevocationStatus::Error, "{name} truncated to {length} bytes");
        }
        // Every single-byte corruption either fails or is still a correctly
        // signed answer about this certificate; none may panic.
        for index in 0..bytes.len() {
            let mut corrupted = bytes.clone();
            corrupted[index] ^= 0xFF;
            let info = evaluate(&corrupted);
            assert!(info.status != RevocationStatus::Revoked, "{name} corrupted at {index}: {info:?}");
        }
    }
    assert_eq!(subject.evaluate_ocsp(b"", NOW).status, RevocationStatus::Error);
    assert_eq!(subject.evaluate_crl(b"\x30\x80\x00\x00", NOW).status, RevocationStatus::Error);
}

// ---------------------------------------------------------------------------
// The OCSP-then-CRL driver
// ---------------------------------------------------------------------------

/// What a fake transport returns for each kind of request.
struct FakeTransport {
    ocsp: Option<Vec<u8>>,
    crl: Option<Vec<u8>>,
    log: RefCell<Vec<String>>,
}

impl FakeTransport {
    fn new(ocsp: Option<Vec<u8>>, crl: Option<Vec<u8>>) -> Self {
        Self { ocsp, crl, log: RefCell::new(Vec::new()) }
    }

    fn fetch(&self, request: RevocationFetch<'_>) -> Result<Vec<u8>, PdfError> {
        match request {
            RevocationFetch::Ocsp { url, body } => {
                assert_eq!(body, fixture("ocsp-request-signer.der").as_slice(), "the request goes out unchanged");
                self.log.borrow_mut().push(format!("POST {url}"));
                self.ocsp.clone().ok_or_else(|| PdfError::ProcessingFailed("connection refused".into()))
            }
            RevocationFetch::Crl { url } => {
                self.log.borrow_mut().push(format!("GET {url}"));
                self.crl.clone().ok_or_else(|| PdfError::ProcessingFailed("HTTP 404".into()))
            }
        }
    }

    fn check(&self) -> RevocationInfo {
        check_online(&signer(), &|request| self.fetch(request), NOW)
    }
}

#[test]
fn a_definite_ocsp_answer_stops_there() {
    let transport = FakeTransport::new(Some(fixture("ocsp-good.der")), Some(fixture("ca.crl")));
    let info = transport.check();
    assert_eq!(info.status, RevocationStatus::Good);
    assert_eq!(info.source, Some(RevocationSource::Ocsp));
    assert_eq!(info.url.as_deref(), Some(OCSP_URL));
    assert_eq!(*transport.log.borrow(), vec![format!("POST {OCSP_URL}")]);
}

#[test]
fn an_ocsp_failure_falls_back_to_the_crl() {
    let transport = FakeTransport::new(None, Some(fixture("ca.crl")));
    let info = transport.check();
    assert_eq!(info.status, RevocationStatus::Good, "{info:?}");
    assert_eq!(info.source, Some(RevocationSource::Crl));
    assert_eq!(info.url.as_deref(), Some(CRL_URL));
    // The ldap:// distribution point is never requested.
    assert_eq!(*transport.log.borrow(), vec![format!("POST {OCSP_URL}"), format!("GET {CRL_URL}")]);

    // A bad OCSP answer is a failure too, not a verdict.
    let transport = FakeTransport::new(Some(fixture("ocsp-rogue.der")), Some(fixture("ca.crl")));
    assert_eq!(transport.check().source, Some(RevocationSource::Crl));
}

#[test]
fn unknown_and_failures_are_reported_as_such() {
    let transport = FakeTransport::new(Some(fixture("ocsp-unknown.der")), None);
    let info = transport.check();
    assert_eq!(info.status, RevocationStatus::Unknown, "{info:?}");

    let transport = FakeTransport::new(None, None);
    let info = transport.check();
    assert_eq!(info.status, RevocationStatus::Error, "{info:?}");
    let detail = info.detail.unwrap_or_default();
    assert!(detail.contains("connection refused") && detail.contains("HTTP 404"), "{detail}");
    assert!(info.checked_at.is_some());
}

// ---------------------------------------------------------------------------
// End to end: a signed PDF
// ---------------------------------------------------------------------------

fn signed_pdf(cert: &str, key: &str) -> Vec<u8> {
    let mut doc = common::build_text_doc(1, "Revocation test", "Revocation test document");
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save test pdf");
    sign::sign_pdf(&bytes, &fixture(cert), &fixture(key), &[fixture("ca.der")], &SignOptions::default())
        .expect("sign with the fixture identity")
}

#[test]
fn offline_verification_never_checks_revocation() {
    let report = sign::verify_signatures(&signed_pdf("signer.der", "signer-key.der"));
    let info = report.signatures.first().expect("one signature");
    assert!(info.signature_valid && info.chain_linked, "{info:?}");
    assert_eq!(info.revocation.status, RevocationStatus::NotChecked);
    let json = serde_json::to_value(info).expect("json");
    assert_eq!(json["revocation"]["status"], "not_checked");
    assert_eq!(json["trust"], "unknown");
}

#[test]
fn online_verification_reports_good_and_revoked_but_never_trust() {
    let pdf = signed_pdf("signer.der", "signer-key.der");
    let transport = FakeTransport::new(Some(fixture("ocsp-good.der")), None);
    let report = sign::verify_signatures_online_at(&pdf, &|request| transport.fetch(request), NOW);
    let info = report.signatures.first().expect("one signature");
    assert!(info.signature_valid);
    assert_eq!(info.revocation.status, RevocationStatus::Good, "{:?}", info.revocation);
    assert_eq!(info.trust, "unknown", "not revoked is not trusted");

    let pdf = signed_pdf("revoked.der", "revoked-key.der");
    let report = sign::verify_signatures_online_at(
        &pdf,
        &|request| match request {
            RevocationFetch::Ocsp { .. } => Ok(fixture("ocsp-revoked.der")),
            RevocationFetch::Crl { .. } => Ok(fixture("ca.crl")),
        },
        NOW,
    );
    let info = report.signatures.first().expect("one signature");
    assert!(info.signature_valid, "the signature itself is still mathematically valid");
    assert_eq!(info.revocation.status, RevocationStatus::Revoked, "{:?}", info.revocation);
    assert_eq!(info.revocation.reason.as_deref(), Some("keyCompromise"));
    assert_eq!(info.trust, "unknown");
}

#[test]
fn a_self_signed_signer_is_unknown_without_any_request() {
    let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).expect("key");
    let mut params = rcgen::CertificateParams::new(Vec::<String>::new()).expect("params");
    params.distinguished_name.push(rcgen::DnType::CommonName, "Revocation Self Signed");
    let cert = params.self_signed(&key).expect("cert");
    let mut doc = common::build_text_doc(1, "Self signed", "Self signed");
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save");
    let pdf = sign::sign_pdf(&bytes, cert.der(), &key.serialize_der(), &[], &SignOptions::default()).expect("sign");

    let report = sign::verify_signatures_online_at(
        &pdf,
        &|_| -> Result<Vec<u8>, PdfError> { panic!("a self-signed certificate has nobody to ask") },
        NOW,
    );
    let info = report.signatures.first().expect("one signature");
    assert_eq!(info.revocation.status, RevocationStatus::Unknown);
    assert!(info.revocation.detail.as_deref().unwrap_or_default().contains("self-signed"));
}

#[test]
fn an_unparseable_pdf_still_returns_a_report() {
    let report = sign::verify_signatures_online_at(b"%PDF-1.7 garbage", &|_| Ok(Vec::new()), NOW);
    assert!(report.signatures.is_empty());
    assert_eq!(report.warnings.len(), 1);
}
