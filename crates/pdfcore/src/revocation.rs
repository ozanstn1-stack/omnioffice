//! Certificate revocation checks: OCSP (RFC 6960) first, the CRL (RFC 5280)
//! as the fallback.
//!
//! The network stays out of `pdfcore`, exactly as with RFC 3161 timestamps:
//! this module reads the OCSP responder and CRL distribution point URLs from
//! the certificate, builds the OCSP request DER and evaluates whatever comes
//! back (CertID, responder authorization, signature, freshness), while the
//! application performs the HTTP exchanges - and only when the user turned
//! the online check on.
//!
//! A "good" answer only means the issuing CA has not revoked the certificate.
//! It does not make the certificate trusted: there is still no trust store, so
//! [`crate::sign::SignatureInfo::trust`] stays "unknown".
//!
//! Every input here is untrusted network data. Sizes are bounded before
//! parsing, the DER decoders never panic, and every failure is reported as an
//! `error` status with an explanation instead of being skipped.

use crate::error::{PdfError, PdfResult};
use crate::sign::{certificate_issued_by, digest_of_signature_algorithm, hash_with_oid, verify_signed_data};
use const_oid::db::rfc5280::{
    ID_AD_OCSP, ID_CE_BASIC_CONSTRAINTS, ID_CE_CRL_DISTRIBUTION_POINTS, ID_CE_CRL_REASONS, ID_CE_DELTA_CRL_INDICATOR,
    ID_CE_EXT_KEY_USAGE, ID_CE_ISSUING_DISTRIBUTION_POINT, ID_CE_KEY_USAGE, ID_KP_OCSP_SIGNING,
    ID_PE_AUTHORITY_INFO_ACCESS,
};
use const_oid::db::rfc6960::ID_PKIX_OCSP_BASIC;
use const_oid::ObjectIdentifier;
use der::asn1::{Null, OctetString};
use der::{Decode, Encode, Header, Reader, SliceReader};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use x509_cert::crl::CertificateList;
use x509_cert::ext::pkix::crl::dp::IssuingDistributionPoint;
use x509_cert::ext::pkix::name::{DistributionPointName, GeneralName};
use x509_cert::ext::pkix::{
    AuthorityInfoAccessSyntax, BasicConstraints, CrlDistributionPoints, CrlReason, ExtendedKeyUsage, KeyUsage,
};
use x509_cert::spki::AlgorithmIdentifierOwned;
use x509_cert::Certificate;
use x509_ocsp::{BasicOcspResponse, CertId, CertStatus, OcspRequest, OcspResponse, OcspResponseStatus};
use x509_ocsp::{Request, ResponderId, TbsRequest};

/// Largest OCSP response accepted; a real one is a few KB.
pub const MAX_OCSP_RESPONSE_BYTES: usize = 1024 * 1024;
/// Largest CRL accepted. Large public CAs publish multi-megabyte CRLs; anything
/// beyond this is refused rather than parsed.
pub const MAX_CRL_BYTES: usize = 10 * 1024 * 1024;
/// URLs tried per method, so a hostile certificate cannot turn one
/// verification into dozens of requests.
const MAX_URLS: usize = 3;
/// Clock difference tolerated between this machine and the responder.
const CLOCK_SKEW_SECS: u64 = 5 * 60;
/// An answer without `nextUpdate` is accepted for this long after `thisUpdate`.
const MAX_AGE_WITHOUT_NEXT_UPDATE_SECS: u64 = 7 * 24 * 60 * 60;

const OID_SHA1: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.14.3.2.26");

// ---------------------------------------------------------------------------
// Result model (serde friendly for the Tauri layer)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevocationStatus {
    /// No online check ran - the default, because the check is opt-in.
    #[default]
    NotChecked,
    /// The CA says the certificate is not revoked. Not a statement of trust.
    Good,
    Revoked,
    /// The responder does not know the certificate, or the certificate names
    /// nowhere to ask.
    Unknown,
    /// Every attempt failed: unreachable server, invalid, unauthorized or
    /// stale answer.
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevocationSource {
    Ocsp,
    Crl,
}

/// Revocation status of a signer certificate.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RevocationInfo {
    pub status: RevocationStatus,
    /// Which mechanism produced the answer.
    pub source: Option<RevocationSource>,
    /// When the check ran (RFC 3339, UTC).
    pub checked_at: Option<String>,
    /// The responder or CRL URL that answered.
    pub url: Option<String>,
    /// Revocation time (RFC 3339, UTC) when the status is `revoked`.
    pub revoked_at: Option<String>,
    /// RFC 5280 CRLReason name, e.g. `keyCompromise`, when one was given.
    pub reason: Option<String>,
    /// Why the status is unknown or an error, or a caveat about a revocation.
    pub detail: Option<String>,
}

impl RevocationInfo {
    fn with_status(status: RevocationStatus, now: u64) -> Self {
        Self { status, checked_at: format_time(now), ..Self::default() }
    }

    /// `unknown` with an explanation (for example: nowhere to ask).
    pub fn unknown(detail: impl Into<String>, now: u64) -> Self {
        Self { detail: Some(detail.into()), ..Self::with_status(RevocationStatus::Unknown, now) }
    }

    /// `error` with an explanation.
    pub fn error(detail: impl Into<String>, now: u64) -> Self {
        Self { detail: Some(detail.into()), ..Self::with_status(RevocationStatus::Error, now) }
    }

    /// Adds the caveat about the signature's *claimed* signing time to a
    /// `revoked` answer that has none yet. The network answer is cached per
    /// signer certificate; this per-signature interpretation is applied
    /// afterwards, so signatures that differ only in the (unverified) time
    /// they claim never cause another request.
    pub fn qualified_by_signing_time(mut self, claimed: Option<u64>) -> Self {
        if self.status == RevocationStatus::Revoked && self.detail.is_none() {
            let revoked_at = self
                .revoked_at
                .as_deref()
                .and_then(|text| text.parse::<der::DateTime>().ok())
                .map(|time| time.unix_duration().as_secs());
            if let (Some(revoked_at), Some(claimed)) = (revoked_at, claimed) {
                self.detail = Some(signing_time_note(revoked_at, claimed));
            }
        }
        self
    }
}

fn signing_time_note(revoked_at: u64, claimed_signing_time: u64) -> String {
    if revoked_at > claimed_signing_time {
        "The certificate was revoked after the time the signature claims it was made, but that time comes from the \
         signer and is not verified here, so the signature cannot be relied on."
            .to_string()
    } else {
        "The certificate was already revoked when the signature claims it was made.".to_string()
    }
}

/// Seconds since the Unix epoch, for the `now` arguments below.
pub fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_secs()).unwrap_or(0)
}

fn format_time(unix: u64) -> Option<String> {
    der::DateTime::from_unix_duration(Duration::from_secs(unix)).ok().map(|time| time.to_string())
}

fn reason_name(reason: CrlReason) -> &'static str {
    match reason {
        CrlReason::Unspecified => "unspecified",
        CrlReason::KeyCompromise => "keyCompromise",
        CrlReason::CaCompromise => "cACompromise",
        CrlReason::AffiliationChanged => "affiliationChanged",
        CrlReason::Superseded => "superseded",
        CrlReason::CessationOfOperation => "cessationOfOperation",
        CrlReason::CertificateHold => "certificateHold",
        CrlReason::RemoveFromCRL => "removeFromCRL",
        CrlReason::PrivilegeWithdrawn => "privilegeWithdrawn",
        CrlReason::AaCompromise => "aACompromise",
    }
}

// ---------------------------------------------------------------------------
// Endpoints
// ---------------------------------------------------------------------------

/// Where a certificate says its revocation status is published. Only http://
/// and https:// URLs are kept (ldap:// and friends are skipped), at most three
/// of each.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RevocationEndpoints {
    /// OCSP responders from the Authority Information Access extension.
    pub ocsp: Vec<String>,
    /// CRL distribution points.
    pub crl: Vec<String>,
}

fn http_uri(name: &GeneralName) -> Option<String> {
    let GeneralName::UniformResourceIdentifier(uri) = name else {
        return None;
    };
    let text = uri.as_str().trim();
    let lower = text.to_ascii_lowercase();
    (lower.starts_with("http://") || lower.starts_with("https://")).then(|| text.to_string())
}

fn push_url(list: &mut Vec<String>, url: String) {
    if list.len() < MAX_URLS && !list.contains(&url) {
        list.push(url);
    }
}

fn endpoints_of(cert: &Certificate) -> RevocationEndpoints {
    let mut endpoints = RevocationEndpoints::default();
    for extension in cert.tbs_certificate.extensions.iter().flatten() {
        if extension.extn_id == ID_PE_AUTHORITY_INFO_ACCESS {
            let Ok(access) = AuthorityInfoAccessSyntax::from_der(extension.extn_value.as_bytes()) else {
                continue;
            };
            for description in &access.0 {
                if description.access_method == ID_AD_OCSP {
                    if let Some(url) = http_uri(&description.access_location) {
                        push_url(&mut endpoints.ocsp, url);
                    }
                }
            }
        } else if extension.extn_id == ID_CE_CRL_DISTRIBUTION_POINTS {
            let Ok(points) = CrlDistributionPoints::from_der(extension.extn_value.as_bytes()) else {
                continue;
            };
            for point in &points.0 {
                // A point limited to some reasons, or published by another
                // issuer, cannot answer "is this certificate revoked" alone.
                if point.reasons.is_some() || point.crl_issuer.is_some() {
                    continue;
                }
                if let Some(DistributionPointName::FullName(names)) = &point.distribution_point {
                    for name in names {
                        if let Some(url) = http_uri(name) {
                            push_url(&mut endpoints.crl, url);
                        }
                    }
                }
            }
        }
    }
    endpoints
}

/// Reads the OCSP and CRL URLs of a DER certificate.
pub fn revocation_endpoints(cert_der: &[u8]) -> PdfResult<RevocationEndpoints> {
    let cert = Certificate::from_der(cert_der)
        .map_err(|err| PdfError::InvalidInput(format!("not a DER certificate: {err}")))?;
    Ok(endpoints_of(&cert))
}

/// Every URI of the certificate's CRL distribution points, whatever the
/// scheme (an IssuingDistributionPoint may name the ldap:// one).
fn crldp_uris(cert: &Certificate) -> Vec<String> {
    let mut uris = Vec::new();
    let Some(value) = extension_value(cert, ID_CE_CRL_DISTRIBUTION_POINTS) else {
        return uris;
    };
    let Ok(points) = CrlDistributionPoints::from_der(value) else {
        return uris;
    };
    for point in &points.0 {
        if let Some(DistributionPointName::FullName(names)) = &point.distribution_point {
            for name in names {
                if let GeneralName::UniformResourceIdentifier(uri) = name {
                    uris.push(uri.as_str().trim().to_string());
                }
            }
        }
    }
    uris
}

/// True when a CRL's IssuingDistributionPoint name covers the certificate:
/// it names the URL the CRL was fetched from or one of the certificate's own
/// CRL distribution points (RFC 5280 6.3.3 (b)(2)(i)). A name relative to the
/// CRL issuer cannot be matched to a URL and never covers.
fn idp_covers(name: &DistributionPointName, fetched_from: Option<&str>, cert_points: &[String]) -> bool {
    let DistributionPointName::FullName(names) = name else {
        return false;
    };
    names.iter().any(|name| {
        let GeneralName::UniformResourceIdentifier(uri) = name else {
            return false;
        };
        let uri = uri.as_str().trim();
        fetched_from.is_some_and(|url| url.trim().eq_ignore_ascii_case(uri))
            || cert_points.iter().any(|point| point.eq_ignore_ascii_case(uri))
    })
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

/// The exact bytes of the first element inside a DER SEQUENCE - the signed
/// `tbsResponseData` / `tbsCertList`, verified as received rather than
/// re-encoded.
fn first_element(der: &[u8]) -> Option<&[u8]> {
    let mut reader = SliceReader::new(der).ok()?;
    Header::decode(&mut reader).ok()?;
    reader.tlv_bytes().ok()
}

fn extension_value(cert: &Certificate, oid: ObjectIdentifier) -> Option<&[u8]> {
    cert.tbs_certificate
        .extensions
        .iter()
        .flatten()
        .find(|extension| extension.extn_id == oid)
        .map(|extension| extension.extn_value.as_bytes())
}

fn valid_at(cert: &Certificate, now: u64) -> bool {
    let validity = &cert.tbs_certificate.validity;
    let not_before = validity.not_before.to_unix_duration().as_secs();
    let not_after = validity.not_after.to_unix_duration().as_secs();
    not_before <= now.saturating_add(CLOCK_SKEW_SECS) && now <= not_after.saturating_add(CLOCK_SKEW_SECS)
}

/// `thisUpdate` must not be in the future and the answer must not be past its
/// `nextUpdate` (or, without one, older than a week).
fn check_freshness(what: &str, this_update: u64, next_update: Option<u64>, now: u64) -> Result<(), String> {
    let show = |unix: u64| format_time(unix).unwrap_or_else(|| unix.to_string());
    if this_update > now.saturating_add(CLOCK_SKEW_SECS) {
        return Err(format!("the {what} is dated in the future ({})", show(this_update)));
    }
    match next_update {
        Some(next) if next.saturating_add(CLOCK_SKEW_SECS) < now => {
            Err(format!("the {what} is out of date: its nextUpdate was {}", show(next)))
        }
        None if now.saturating_sub(this_update) > MAX_AGE_WITHOUT_NEXT_UPDATE_SECS => {
            Err(format!("the {what} from {} names no nextUpdate and is too old to rely on", show(this_update)))
        }
        _ => Ok(()),
    }
}

/// The CA certificate whose key authenticates an OCSP response or a CRL must
/// be able to act as a CA: `basicConstraints` (when present) says `cA`, and
/// `keyUsage` (when present) allows `keyCertSign` and, for a CRL, `cRLSign`.
/// Absent extensions are tolerated, as RFC 5280 allows for old CA certificates.
fn check_issuer_may_sign(issuer: &Certificate, crl: bool) -> Result<(), String> {
    if let Some(value) = extension_value(issuer, ID_CE_BASIC_CONSTRAINTS) {
        let is_ca = BasicConstraints::from_der(value).map(|constraints| constraints.ca).unwrap_or(false);
        if !is_ca {
            return Err(
                "the issuing certificate is not a certificate authority (basicConstraints cA is not set)".into()
            );
        }
    }
    if let Some(value) = extension_value(issuer, ID_CE_KEY_USAGE) {
        let usage = KeyUsage::from_der(value).map_err(|_| "the issuing certificate's key usage is invalid")?;
        if !usage.key_cert_sign() {
            return Err("the issuing certificate is not allowed to sign certificates (keyCertSign)".into());
        }
        if crl && !usage.crl_sign() {
            return Err("the issuing certificate is not allowed to sign CRLs".into());
        }
    }
    Ok(())
}

fn verify_with(signer: &Certificate, algorithm: ObjectIdentifier, message: &[u8], signature: &[u8]) -> bool {
    let Ok(spki) = signer.tbs_certificate.subject_public_key_info.to_der() else {
        return false;
    };
    let Some(digest) = digest_of_signature_algorithm(algorithm) else {
        return false;
    };
    verify_signed_data(algorithm, digest, &spki, message, signature)
}

// ---------------------------------------------------------------------------
// The certificate under check
// ---------------------------------------------------------------------------

/// A certificate whose revocation status is checked, plus the CA certificate
/// that issued it (needed for the OCSP CertID and to verify every answer).
pub struct RevocationSubject {
    cert: Certificate,
    issuer: Certificate,
    endpoints: RevocationEndpoints,
    claimed_signing_time: Option<u64>,
}

impl RevocationSubject {
    /// Fails unless `issuer_der` really issued `cert_der` (name and signature).
    pub fn new(cert_der: &[u8], issuer_der: &[u8]) -> PdfResult<Self> {
        let cert = Certificate::from_der(cert_der)
            .map_err(|err| PdfError::InvalidInput(format!("not a DER certificate: {err}")))?;
        let issuer = Certificate::from_der(issuer_der)
            .map_err(|err| PdfError::InvalidInput(format!("the issuer is not a DER certificate: {err}")))?;
        if !certificate_issued_by(&cert, &issuer) {
            return Err(PdfError::InvalidInput("the issuer certificate did not issue this certificate".into()));
        }
        let endpoints = endpoints_of(&cert);
        Ok(Self { cert, issuer, endpoints, claimed_signing_time: None })
    }

    /// When the signature claims it was made (Unix seconds). Used only to
    /// qualify a revocation in the detail text: the time is not verified.
    pub fn with_signing_time(mut self, unix: Option<u64>) -> Self {
        self.claimed_signing_time = unix;
        self
    }

    pub fn endpoints(&self) -> &RevocationEndpoints {
        &self.endpoints
    }

    fn cert_id(&self, hash: ObjectIdentifier) -> Result<CertId, String> {
        let name = self.issuer.tbs_certificate.subject.to_der().map_err(|err| err.to_string())?;
        let key = self.issuer.tbs_certificate.subject_public_key_info.subject_public_key.raw_bytes();
        let unsupported = || format!("unsupported CertID hash algorithm {hash}");
        let name_hash = hash_with_oid(hash, &name).ok_or_else(unsupported)?;
        let key_hash = hash_with_oid(hash, key).ok_or_else(unsupported)?;
        Ok(CertId {
            hash_algorithm: AlgorithmIdentifierOwned { oid: hash, parameters: Some(Null.into()) },
            issuer_name_hash: OctetString::new(name_hash).map_err(|err| err.to_string())?,
            issuer_key_hash: OctetString::new(key_hash).map_err(|err| err.to_string())?,
            serial_number: self.cert.tbs_certificate.serial_number.clone(),
        })
    }

    /// True when `id` names this certificate (any supported hash algorithm;
    /// the algorithm parameters are not compared).
    fn matches(&self, id: &CertId) -> bool {
        if id.serial_number != self.cert.tbs_certificate.serial_number {
            return false;
        }
        match self.cert_id(id.hash_algorithm.oid) {
            Ok(expected) => {
                expected.issuer_name_hash == id.issuer_name_hash && expected.issuer_key_hash == id.issuer_key_hash
            }
            Err(_) => false,
        }
    }

    /// DER `OCSPRequest` for this certificate: one SHA-1 CertID (the hash
    /// every responder supports, RFC 5019), unsigned and without a nonce -
    /// most CA responders serve pre-signed answers and ignore nonces, so
    /// freshness is enforced through thisUpdate/nextUpdate instead.
    pub fn ocsp_request(&self) -> PdfResult<Vec<u8>> {
        let cert_id = self.cert_id(OID_SHA1).map_err(PdfError::Internal)?;
        let request = OcspRequest {
            tbs_request: TbsRequest {
                request_list: vec![Request { req_cert: cert_id, single_request_extensions: None }],
                ..TbsRequest::default()
            },
            optional_signature: None,
        };
        request.to_der().map_err(|err| PdfError::Internal(err.to_string()))
    }

    /// Evaluates an `OCSPResponse` received for [`Self::ocsp_request`] at Unix
    /// time `now`. Anything not provably correct is an `error`.
    pub fn evaluate_ocsp(&self, response: &[u8], now: u64) -> RevocationInfo {
        let mut info = self.read_ocsp(response, now).unwrap_or_else(|detail| RevocationInfo::error(detail, now));
        info.source = Some(RevocationSource::Ocsp);
        info
    }

    fn read_ocsp(&self, response: &[u8], now: u64) -> Result<RevocationInfo, String> {
        if response.len() > MAX_OCSP_RESPONSE_BYTES {
            return Err("the OCSP response is larger than 1 MB".into());
        }
        let response =
            OcspResponse::from_der(response).map_err(|err| format!("the OCSP response is not valid DER ({err})"))?;
        if response.response_status != OcspResponseStatus::Successful {
            return Err(format!("the OCSP responder refused the request ({:?})", response.response_status));
        }
        let bytes = response.response_bytes.ok_or("the OCSP response carries no answer")?;
        if bytes.response_type != ID_PKIX_OCSP_BASIC {
            return Err(format!("unsupported OCSP response type {}", bytes.response_type));
        }
        check_issuer_may_sign(&self.issuer, false)?;
        let basic_der = bytes.response.as_bytes();
        let basic = BasicOcspResponse::from_der(basic_der)
            .map_err(|err| format!("the basic OCSP response is not valid DER ({err})"))?;
        let signed = first_element(basic_der).ok_or("the basic OCSP response has no response data")?;

        // Authentic first: signed by the CA or by a responder it authorized.
        let signer = self.ocsp_signer(&basic, now)?;
        let signature = basic.signature.as_bytes().ok_or("the OCSP signature is not a whole number of bytes")?;
        if !verify_with(signer, basic.signature_algorithm.oid, signed, signature) {
            return Err(format!(
                "the OCSP response signature does not verify (algorithm {})",
                basic.signature_algorithm.oid
            ));
        }

        // Then about this certificate.
        let single = basic
            .tbs_response_data
            .responses
            .iter()
            .find(|single| self.matches(&single.cert_id))
            .ok_or("the OCSP response is about a different certificate (CertID mismatch)")?;

        // And current.
        check_freshness(
            "OCSP response",
            single.this_update.0.to_unix_duration().as_secs(),
            single.next_update.map(|next| next.0.to_unix_duration().as_secs()),
            now,
        )?;

        Ok(match single.cert_status {
            CertStatus::Good(_) => RevocationInfo::with_status(RevocationStatus::Good, now),
            CertStatus::Revoked(revoked) => {
                self.revoked(revoked.revocation_time.0.to_unix_duration().as_secs(), revoked.revocation_reason, now)
            }
            CertStatus::Unknown(_) => RevocationInfo::unknown("the OCSP responder does not know this certificate", now),
        })
    }

    /// The certificate that signed an OCSP response: the issuing CA itself,
    /// or a delegated responder certificate issued directly by that CA with
    /// the OCSP-signing extended key usage (RFC 6960 4.2.2.2).
    fn ocsp_signer<'a>(&'a self, basic: &'a BasicOcspResponse, now: u64) -> Result<&'a Certificate, String> {
        let identifies = |cert: &Certificate| match &basic.tbs_response_data.responder_id {
            ResponderId::ByName(name) => cert.tbs_certificate.subject == *name,
            ResponderId::ByKey(hash) => {
                let key = cert.tbs_certificate.subject_public_key_info.subject_public_key.raw_bytes();
                Sha1::digest(key).as_slice() == hash.as_bytes()
            }
        };
        if identifies(&self.issuer) {
            return Ok(&self.issuer);
        }
        let mut named = false;
        for candidate in basic.certs.iter().flatten() {
            if !identifies(candidate) {
                continue;
            }
            named = true;
            if !certificate_issued_by(candidate, &self.issuer) {
                continue;
            }
            let ocsp_signing = extension_value(candidate, ID_CE_EXT_KEY_USAGE)
                .and_then(|value| ExtendedKeyUsage::from_der(value).ok())
                .map(|usage| usage.0.contains(&ID_KP_OCSP_SIGNING))
                .unwrap_or(false);
            if !ocsp_signing {
                return Err("the OCSP responder certificate lacks the OCSP-signing extended key usage".into());
            }
            if !valid_at(candidate, now) {
                return Err("the OCSP responder certificate is expired or not yet valid".into());
            }
            return Ok(candidate);
        }
        Err(if named {
            "the OCSP response is signed by a responder the certificate authority did not authorize".into()
        } else {
            "the OCSP response does not identify a responder certificate it includes".into()
        })
    }

    /// Evaluates a DER CRL at Unix time `now`. A CRL limited to one
    /// distribution point is accepted only when that point is one of the
    /// certificate's own; use [`Self::evaluate_crl_from`] when the URL is known.
    pub fn evaluate_crl(&self, crl: &[u8], now: u64) -> RevocationInfo {
        self.evaluate_crl_from(crl, None, now)
    }

    /// Evaluates a DER CRL downloaded from `url` at Unix time `now`. A CRL
    /// whose IssuingDistributionPoint names a distribution point that is
    /// neither `url` nor one of the certificate's own does not cover this
    /// certificate: the result is `unknown`, never `good`.
    pub fn evaluate_crl_from(&self, crl: &[u8], url: Option<&str>, now: u64) -> RevocationInfo {
        let mut info = self.read_crl(crl, url, now).unwrap_or_else(|detail| RevocationInfo::error(detail, now));
        info.source = Some(RevocationSource::Crl);
        info
    }

    fn read_crl(&self, crl: &[u8], url: Option<&str>, now: u64) -> Result<RevocationInfo, String> {
        if crl.len() > MAX_CRL_BYTES {
            return Err("the CRL is larger than 10 MB".into());
        }
        let list = CertificateList::from_der(crl).map_err(|err| format!("the CRL is not valid DER ({err})"))?;
        let signed = first_element(crl).ok_or("the CRL has no tbsCertList")?;
        let tbs = &list.tbs_cert_list;
        if tbs.issuer != self.issuer.tbs_certificate.subject {
            return Err("the CRL was published by a different certificate authority".into());
        }
        if tbs.signature.oid != list.signature_algorithm.oid {
            return Err("the CRL's two signature algorithm fields disagree".into());
        }
        check_issuer_may_sign(&self.issuer, true)?;
        let signature = list.signature.as_bytes().ok_or("the CRL signature is not a whole number of bytes")?;
        if !verify_with(&self.issuer, list.signature_algorithm.oid, signed, signature) {
            return Err(format!(
                "the CRL signature does not verify with the issuer's key (algorithm {})",
                list.signature_algorithm.oid
            ));
        }
        // A CRL extension that cannot be honoured makes the CRL unusable
        // (RFC 5280 5.2): delta and partial CRLs cannot prove "not revoked".
        for extension in tbs.crl_extensions.iter().flatten() {
            if extension.extn_id == ID_CE_DELTA_CRL_INDICATOR {
                return Err("delta CRLs are not supported".into());
            }
            if extension.extn_id == ID_CE_ISSUING_DISTRIBUTION_POINT {
                let point = IssuingDistributionPoint::from_der(extension.extn_value.as_bytes())
                    .map_err(|err| format!("the CRL's issuing distribution point is invalid ({err})"))?;
                if point.only_contains_ca_certs
                    || point.only_contains_attribute_certs
                    || point.indirect_crl
                    || point.only_some_reasons.is_some()
                {
                    return Err("the CRL covers only part of the certificates or reasons".into());
                }
                // A CRL partitioned by distribution point only speaks for the
                // certificates that name that point.
                if let Some(name) = &point.distribution_point {
                    if !idp_covers(name, url, &crldp_uris(&self.cert)) {
                        return Ok(RevocationInfo::unknown(
                            "the CRL is limited to a distribution point this certificate does not name, so it cannot \
                             show that the certificate is not revoked",
                            now,
                        ));
                    }
                }
            } else if extension.critical {
                return Err(format!("the CRL has an unsupported critical extension {}", extension.extn_id));
            }
        }
        check_freshness(
            "CRL",
            tbs.this_update.to_unix_duration().as_secs(),
            tbs.next_update.map(|next| next.to_unix_duration().as_secs()),
            now,
        )?;

        let serial = &self.cert.tbs_certificate.serial_number;
        let entry = tbs.revoked_certificates.iter().flatten().find(|entry| entry.serial_number == *serial);
        let Some(entry) = entry else {
            return Ok(RevocationInfo::with_status(RevocationStatus::Good, now));
        };
        let reason = entry
            .crl_entry_extensions
            .iter()
            .flatten()
            .find(|extension| extension.extn_id == ID_CE_CRL_REASONS)
            .and_then(|extension| CrlReason::from_der(extension.extn_value.as_bytes()).ok());
        if reason == Some(CrlReason::RemoveFromCRL) {
            return Ok(RevocationInfo::with_status(RevocationStatus::Good, now));
        }
        Ok(self.revoked(entry.revocation_date.to_unix_duration().as_secs(), reason, now))
    }

    fn revoked(&self, revoked_at: u64, reason: Option<CrlReason>, now: u64) -> RevocationInfo {
        let detail = self.claimed_signing_time.map(|signed| signing_time_note(revoked_at, signed));
        RevocationInfo {
            revoked_at: format_time(revoked_at),
            reason: reason.map(|reason| reason_name(reason).to_string()),
            detail,
            ..RevocationInfo::with_status(RevocationStatus::Revoked, now)
        }
    }
}

// ---------------------------------------------------------------------------
// Online check driver
// ---------------------------------------------------------------------------

/// One HTTP exchange the application performs on behalf of [`check_online`].
#[derive(Debug, Clone, Copy)]
pub enum RevocationFetch<'a> {
    /// POST `body` with `Content-Type: application/ocsp-request`.
    Ocsp { url: &'a str, body: &'a [u8] },
    /// GET a DER encoded CRL.
    Crl { url: &'a str },
}

/// Performs a [`RevocationFetch`] and returns the response body. The
/// application owns the transport: schemes, timeouts and size limits.
pub type RevocationFetcher<'a> = dyn Fn(RevocationFetch<'_>) -> PdfResult<Vec<u8>> + 'a;

/// What the attempts so far produced short of a definite answer.
#[derive(Default)]
struct Attempts {
    unknown: Option<RevocationInfo>,
    failures: Vec<String>,
}

impl Attempts {
    /// Returns the answer when it is definite (good or revoked).
    fn settle(&mut self, info: RevocationInfo, url: &str, label: &str) -> Option<RevocationInfo> {
        let info = RevocationInfo { url: Some(url.to_string()), ..info };
        match info.status {
            RevocationStatus::Good | RevocationStatus::Revoked => return Some(info),
            RevocationStatus::Unknown => {
                self.unknown.get_or_insert(info);
            }
            _ => self.failures.push(format!("{label} {url}: {}", info.detail.as_deref().unwrap_or("failed"))),
        }
        None
    }
}

/// Asks the certificate's OCSP responders, then its CRL distribution points,
/// until one gives a definite good/revoked answer.
pub fn check_online(subject: &RevocationSubject, fetch: &RevocationFetcher<'_>, now: u64) -> RevocationInfo {
    let endpoints = subject.endpoints();
    if endpoints.ocsp.is_empty() && endpoints.crl.is_empty() {
        return RevocationInfo::unknown(
            "the certificate names no OCSP responder or CRL distribution point over HTTP, so there is nowhere to ask",
            now,
        );
    }
    let mut attempts = Attempts::default();
    if !endpoints.ocsp.is_empty() {
        match subject.ocsp_request() {
            Ok(body) => {
                for url in &endpoints.ocsp {
                    let info = match fetch(RevocationFetch::Ocsp { url, body: &body }) {
                        Ok(response) => subject.evaluate_ocsp(&response, now),
                        Err(error) => RevocationInfo::error(error.to_string(), now),
                    };
                    if let Some(answer) = attempts.settle(info, url, "OCSP") {
                        return answer;
                    }
                }
            }
            Err(error) => attempts.failures.push(format!("OCSP request: {error}")),
        }
    }
    for url in &endpoints.crl {
        let info = match fetch(RevocationFetch::Crl { url }) {
            Ok(crl) => subject.evaluate_crl_from(&crl, Some(url), now),
            Err(error) => RevocationInfo::error(error.to_string(), now),
        };
        if let Some(answer) = attempts.settle(info, url, "CRL") {
            return answer;
        }
    }
    let Attempts { unknown, failures } = attempts;
    unknown.unwrap_or_else(|| RevocationInfo::error(failures.join("; "), now))
}

// ---------------------------------------------------------------------------
// Limits for one verification
// ---------------------------------------------------------------------------

/// Network exchanges one verification may make in total. The URLs come from
/// certificates inside an untrusted PDF, so without a ceiling a crafted file
/// could turn one verification into an unbounded number of requests.
pub const MAX_FETCHES_PER_VERIFICATION: usize = 20;
/// Wall-clock time one verification may spend asking CAs; after it the
/// remaining signatures are reported as `unknown`. The application also clamps
/// each request's timeout to what is left of it.
pub const VERIFICATION_DEADLINE: Duration = Duration::from_secs(45);

/// Why no further request may be made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetExhausted {
    Fetches,
    Deadline,
}

impl BudgetExhausted {
    pub fn describe(self) -> String {
        match self {
            Self::Fetches => {
                format!(
                    "the limit of {MAX_FETCHES_PER_VERIFICATION} revocation requests for one verification was reached"
                )
            }
            Self::Deadline => format!(
                "the {} second time limit for the revocation check of one verification was reached",
                VERIFICATION_DEADLINE.as_secs()
            ),
        }
    }
}

/// A request counter plus a deadline. Time is passed in, so the logic is a
/// pure function of its inputs.
#[derive(Debug)]
pub struct FetchBudget {
    max_fetches: usize,
    spent: Cell<usize>,
    deadline: Instant,
}

impl FetchBudget {
    pub fn new(max_fetches: usize, started: Instant, allowed: Duration) -> Self {
        Self { max_fetches, spent: Cell::new(0), deadline: started + allowed }
    }

    /// The default limits, starting at `started`.
    pub fn standard(started: Instant) -> Self {
        Self::new(MAX_FETCHES_PER_VERIFICATION, started, VERIFICATION_DEADLINE)
    }

    /// Why the next request would be refused, if it would.
    pub fn exhausted(&self, now: Instant) -> Option<BudgetExhausted> {
        if now >= self.deadline {
            Some(BudgetExhausted::Deadline)
        } else if self.spent.get() >= self.max_fetches {
            Some(BudgetExhausted::Fetches)
        } else {
            None
        }
    }

    /// Counts one request, or says why it must not be made.
    pub fn try_spend(&self, now: Instant) -> Result<(), BudgetExhausted> {
        match self.exhausted(now) {
            Some(reason) => Err(reason),
            None => {
                self.spent.set(self.spent.get() + 1);
                Ok(())
            }
        }
    }

    pub fn spent(&self) -> usize {
        self.spent.get()
    }
}

/// Identity of an exchange for de-duplication: the same URL (and, for OCSP,
/// the same request body) is asked once per verification.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum FetchKey {
    Ocsp(String, Vec<u8>),
    Crl(String),
}

impl FetchKey {
    fn of(request: &RevocationFetch<'_>) -> Self {
        match request {
            RevocationFetch::Ocsp { url, body } => Self::Ocsp(url.trim().to_ascii_lowercase(), body.to_vec()),
            RevocationFetch::Crl { url } => Self::Crl(url.trim().to_ascii_lowercase()),
        }
    }
}

/// Wraps the application's fetcher with a [`FetchBudget`] and an answer cache:
/// identical exchanges (failed ones too) are performed once, and nothing is
/// sent after the budget is spent.
pub struct LimitedFetcher<'a> {
    inner: &'a RevocationFetcher<'a>,
    budget: FetchBudget,
    answers: RefCell<HashMap<FetchKey, Result<Vec<u8>, String>>>,
}

impl<'a> LimitedFetcher<'a> {
    pub fn new(inner: &'a RevocationFetcher<'a>, budget: FetchBudget) -> Self {
        Self { inner, budget, answers: RefCell::new(HashMap::new()) }
    }

    pub fn budget(&self) -> &FetchBudget {
        &self.budget
    }

    pub fn fetch(&self, request: RevocationFetch<'_>) -> PdfResult<Vec<u8>> {
        let key = FetchKey::of(&request);
        if let Some(cached) = self.answers.borrow().get(&key) {
            return cached.clone().map_err(PdfError::ProcessingFailed);
        }
        self.budget.try_spend(Instant::now()).map_err(|reason| PdfError::ProcessingFailed(reason.describe()))?;
        let outcome = (self.inner)(request);
        let stored = outcome.as_ref().map(Clone::clone).map_err(ToString::to_string);
        self.answers.borrow_mut().insert(key, stored);
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freshness_rules() {
        let day = 24 * 60 * 60;
        let now = 1_800_000_000;
        assert!(check_freshness("x", now - day, Some(now + day), now).is_ok());
        // Within the clock skew either way.
        assert!(check_freshness("x", now + 60, Some(now - 60), now).is_ok());
        assert!(check_freshness("x", now + day, Some(now + 2 * day), now).unwrap_err().contains("future"));
        assert!(check_freshness("x", now - 2 * day, Some(now - day), now).unwrap_err().contains("out of date"));
        assert!(check_freshness("x", now - day, None, now).is_ok());
        assert!(check_freshness("x", now - 8 * day, None, now).unwrap_err().contains("too old"));
    }

    #[test]
    fn the_default_is_not_checked_and_serializes_snake_case() {
        let info = RevocationInfo::default();
        assert_eq!(info.status, RevocationStatus::NotChecked);
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["status"], "not_checked");
        assert!(json["source"].is_null());
        assert!(json.get("checkedAt").is_some());
        let parsed: RevocationInfo = serde_json::from_str("{}").unwrap();
        assert_eq!(parsed, info);
    }

    #[test]
    fn times_format_as_rfc3339() {
        assert_eq!(format_time(0).as_deref(), Some("1970-01-01T00:00:00Z"));
        assert!(first_element(b"").is_none());
        assert!(first_element(&[0x30, 0x05, 0x02]).is_none());
    }
}
