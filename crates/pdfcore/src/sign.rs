//! Real PDF digital signatures: CMS/PKCS#7 detached, X.509, SHA-256.
//!
//! Everything here is genuine cryptography - there is no placeholder, no
//! "demo signature" and no simulated verification anywhere in this module:
//!
//! * `sign_pdf` appends a real AcroForm signature field (with an optional
//!   visible appearance stream) to the document and embeds a detached CMS
//!   `SignedData` object whose signature covers the SHA-256 digest of the
//!   whole revision except the `/Contents` placeholder itself.
//! * `verify_signatures` parses the embedded CMS with the RustCrypto `cms`
//!   parser (an independent implementation from the hand written encoder),
//!   recomputes the byte-range digest and verifies the RSA/ECDSA signature
//!   with the signer certificate's public key.
//! * `parse_pkcs12` decrypts PKCS#12 (PFX) containers with pure Rust code so
//!   the exact same path works on Windows and on Android.
//!
//! Structure of the produced signature:
//!
//! ```text
//! /Type /Sig /Filter /Adobe.PPKLite /SubFilter /adbe.pkcs7.detached
//! /ByteRange [0 <contents-start> <contents-end> <file-end>]
//! /Contents <hex DER + zero padding>
//! ```
//!
//! The document is updated with a PDF incremental update (ISO 32000-1 7.5.6):
//! the original bytes are left untouched and only the new/updated objects, a
//! cross-reference section and a trailer are appended. The signature covers
//! the original bytes plus the appended revision, so the ByteRange digest is
//! stable and the original pages and content streams are preserved byte for
//! byte.
//!
//! Honest limitations (surfaced as `trust = "unknown"`):
//! * There is no system trust store, no network access and therefore no
//!   certificate path validation or revocation checking. A valid signature
//!   proves that the private key belonging to the embedded certificate signed
//!   these bytes - nothing more. Chain linkage is checked structurally
//!   (issuer/subject plus the certificate signature) but never reported as
//!   trusted.
//! * Encrypted input PDFs are rejected; decrypt them first.
//! * PKCS#12 files protected with RC2/RC4 are rejected with a clear error
//!   (they are legacy-only); AES (PBES2) and 3DES (PKCS#12 PBE) are supported.

use crate::error::{PdfError, PdfResult};
use const_oid::ObjectIdentifier;
use der::asn1::{
    Any, AnyRef, GeneralizedTime, ObjectIdentifier as DerObjectIdentifier, OctetString, UtcTime,
};
use der::{Decode, Encode, Tagged};
use hmac::{Hmac, Mac};
use lopdf::{dictionary, Dictionary, Document, IncrementalDocument, Object, Stream, StringFormat};
use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::pkcs1v15::{Signature as RsaSignature, SigningKey as RsaSigningKey, VerifyingKey as RsaVerifyingKey};
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey, EncodePublicKey};
use rsa::signature::{SignatureEncoding, Signer, Verifier};
use rsa::{RsaPrivateKey, RsaPublicKey};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha384, Sha512};
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};
use x509_cert::Certificate;

// ---------------------------------------------------------------------------
// Object identifiers
// ---------------------------------------------------------------------------

/// `id-data` (RFC 5652): the content type of a detached signature payload.
const OID_DATA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.1");
/// `id-signedData` (RFC 5652).
const OID_SIGNED_DATA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.2");
/// `id-encryptedData` (RFC 5652), used by encrypted PKCS#12 bags.
const OID_ENCRYPTED_DATA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.6");

const OID_ATTR_CONTENT_TYPE: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.3");
const OID_ATTR_MESSAGE_DIGEST: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.4");
const OID_ATTR_SIGNING_TIME: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.5");

const OID_SHA1: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.14.3.2.26");
const OID_SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.1");
const OID_SHA384: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.2");
const OID_SHA512: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.3");

const OID_RSA_ENCRYPTION: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.1");
const OID_SHA1_WITH_RSA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.5");
const OID_SHA256_WITH_RSA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.11");
const OID_SHA384_WITH_RSA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.12");
const OID_SHA512_WITH_RSA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.13");

const OID_EC_PUBLIC_KEY: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.2.1");
const OID_EC_P256: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.3.1.7");
const OID_ECDSA_WITH_SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.2");
const OID_ECDSA_WITH_SHA384: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.3");
const OID_ECDSA_WITH_SHA512: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.4");

const OID_BASIC_CONSTRAINTS: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.29.19");
const OID_COMMON_NAME: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.4.3");

const OID_PBES2: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.5.13");
const OID_PKCS12_PBE_3DES_3KEY: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.12.1.3");
const OID_PKCS12_PBE_3DES_2KEY: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.12.1.4");
const OID_PKCS12_PBE_RC4_128: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.12.1.1");
const OID_PKCS12_PBE_RC4_40: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.12.1.2");
const OID_PKCS12_PBE_RC2_128: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.12.1.5");
const OID_PKCS12_PBE_RC2_40: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.12.1.6");

const OID_PKCS12_KEY_BAG: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.12.10.1.1");
const OID_PKCS12_SHROUDED_KEY_BAG: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.12.10.1.2");
const OID_PKCS12_CERT_BAG: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.12.10.1.3");
const OID_X509_CERTIFICATE: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.22.1");

// ---------------------------------------------------------------------------
// Options and reports (serde friendly for the Tauri layer)
// ---------------------------------------------------------------------------

/// Signature placement and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignOptions {
    /// 1-based page number the signature widget is placed on.
    #[serde(default = "default_page")]
    pub page: u32,
    /// Widget rectangle in PDF points with the origin at the BOTTOM-LEFT of
    /// the unrotated page: `[x1, y1, x2, y2]`. `None` uses a 264x74 pt box in
    /// the bottom margin of the page (`[36, 36, 300, 110]`).
    #[serde(default)]
    pub rect: Option<[f32; 4]>,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub contact: String,
    /// Draw a visible appearance (signer name, date, reason, location).
    #[serde(default = "default_true")]
    pub appearance: bool,
    /// Name written into `/Name` and the appearance. Falls back to the
    /// certificate common name when absent.
    #[serde(default)]
    pub signer_name: Option<String>,
}

fn default_page() -> u32 {
    1
}

fn default_true() -> bool {
    true
}

impl Default for SignOptions {
    fn default() -> Self {
        Self {
            page: 1,
            rect: None,
            reason: String::new(),
            location: String::new(),
            contact: String::new(),
            appearance: true,
            signer_name: None,
        }
    }
}

/// The default widget box: a small field in the bottom-left margin area of
/// the page (PDF points, bottom-left origin).
pub const DEFAULT_RECT: [f32; 4] = [36.0, 36.0, 300.0, 110.0];

/// Information about one X.509 certificate, extracted from the signature or
/// the PFX container. Never claims trust - see [`SignatureInfo::trust`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CertificateInfo {
    pub subject: String,
    pub issuer: String,
    pub serial_hex: String,
    pub not_before: String,
    pub not_after: String,
    pub expired: bool,
    pub is_ca: bool,
    /// SHA-256 fingerprint of the DER certificate, uppercase hex separated by
    /// colons (OpenSSL style).
    pub sha256_fingerprint: String,
}

/// Verification result for a single embedded signature.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureInfo {
    pub field_name: String,
    pub sub_filter: String,
    /// The ByteRange covers the whole revision except the `/Contents` value.
    pub covers_whole_document: bool,
    /// The signed bytes no longer hash to the digest in the CMS, or the
    /// ByteRange does not span the whole revision (an incremental update was
    /// appended after signing).
    pub modified_after_signing: bool,
    /// SHA-256 (or the declared digest) of the covered bytes matches the
    /// `messageDigest` signed attribute.
    pub digest_matches: bool,
    /// The RSA/ECDSA signature over the signed attributes verifies with the
    /// public key of the embedded signer certificate.
    pub signature_valid: bool,
    /// Signer certificate first, then the remaining embedded certificates in
    /// path order when the issuer links allow it.
    pub chain: Vec<CertificateInfo>,
    /// Every certificate in `chain` is issued (and signature-verified) by the
    /// next one.
    pub chain_linked: bool,
    /// The last certificate of the chain is self-signed.
    pub self_signed_chain: bool,
    pub signer: CertificateInfo,
    /// Signing time from the CMS `signingTime` attribute (RFC 3339, UTC).
    pub signing_time: Option<String>,
    pub algorithm: String,
    /// Always `"unknown"` in this offline build: no system trust store and no
    /// revocation checking are available.
    pub trust: String,
    /// Human readable notes about anything that could not be checked.
    #[serde(default)]
    pub notes: Vec<String>,
}

/// Result of [`verify_signatures`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureReport {
    pub signatures: Vec<SignatureInfo>,
}

/// Decrypted PKCS#12 identity: the signer certificate, its private key in
/// PKCS#8 form and the remaining certificates of the container.
pub struct Pkcs12Identity {
    pub cert_der: Vec<u8>,
    pub key_pkcs8_der: Vec<u8>,
    pub chain_der: Vec<Vec<u8>>,
}

// ---------------------------------------------------------------------------
// Minimal DER writer
//
// The CMS structure is encoded by hand (with the `der`/`const-oid` types for
// object identifiers and times) so the exact bytes that are signed are under
// our control. Verification uses the independent RustCrypto `cms` parser.
// ---------------------------------------------------------------------------

fn der_length(out: &mut Vec<u8>, length: usize) {
    if length < 0x80 {
        out.push(length as u8);
    } else {
        let mut bytes = Vec::new();
        let mut value = length;
        while value > 0 {
            bytes.push((value & 0xFF) as u8);
            value >>= 8;
        }
        bytes.reverse();
        out.push(0x80 | bytes.len() as u8);
        out.extend_from_slice(&bytes);
    }
}

/// One DER TLV: tag, definite length, content.
fn der_tlv(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(content.len() + 6);
    out.push(tag);
    der_length(&mut out, content.len());
    out.extend_from_slice(content);
    out
}

fn der_sequence(parts: &[Vec<u8>]) -> Vec<u8> {
    let content: Vec<u8> = parts.concat();
    der_tlv(0x30, &content)
}

/// A `SET OF` value must be DER sorted (ascending by encoded bytes).
fn der_set_of(mut parts: Vec<Vec<u8>>) -> Vec<u8> {
    parts.sort();
    let content: Vec<u8> = parts.concat();
    der_tlv(0x31, &content)
}

fn der_oid(oid: ObjectIdentifier) -> Vec<u8> {
    der_tlv(0x06, oid.as_bytes())
}

fn der_octet_string(bytes: &[u8]) -> Vec<u8> {
    der_tlv(0x04, bytes)
}

fn der_integer_u64(value: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut value = value;
    if value == 0 {
        bytes.push(0);
    }
    while value > 0 {
        bytes.push((value & 0xFF) as u8);
        value >>= 8;
    }
    bytes.reverse();
    if bytes[0] & 0x80 != 0 {
        bytes.insert(0, 0);
    }
    der_tlv(0x02, &bytes)
}

fn der_null() -> Vec<u8> {
    vec![0x05, 0x00]
}

/// Wraps `inner` (a complete TLV) in a context-specific constructed tag, which
/// covers both `[n] EXPLICIT` and `[n] IMPLICIT SET OF` (the content layout is
/// identical for constructed types).
fn der_context(tag_number: u8, inner: &[u8]) -> Vec<u8> {
    der_tlv(0xA0 | tag_number, inner)
}

// ---------------------------------------------------------------------------
// Hex helpers
// ---------------------------------------------------------------------------

fn to_hex_upper(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0F) as usize] as char);
    }
    out
}

fn fingerprint_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

/// SHA-256 fingerprint of a DER certificate (OpenSSL style, colon separated).
pub fn certificate_fingerprint(cert_der: &[u8]) -> PdfResult<String> {
    let cert = Certificate::from_der(cert_der)
        .map_err(|err| PdfError::InvalidInput(format!("not a DER certificate: {err}")))?;
    Ok(fingerprint_hex(&cert.to_der().map_err(internal)?))
}

/// Returns the exact DER TLV at the start of `bytes` (ignoring any trailing
/// zero padding), or `None` when the length field is malformed.
fn der_exact_slice(bytes: &[u8]) -> Option<&[u8]> {
    if bytes.len() < 2 {
        return None;
    }
    let first = bytes[1];
    let (header_len, length) = if first < 0x80 {
        (2usize, first as usize)
    } else {
        let count = (first & 0x7F) as usize;
        if count == 0 || count > 8 || bytes.len() < 2 + count {
            return None;
        }
        let mut length = 0usize;
        for byte in &bytes[2..2 + count] {
            length = length.checked_mul(256)?.checked_add(*byte as usize)?;
        }
        (2 + count, length)
    };
    let end = header_len.checked_add(length)?;
    if end > bytes.len() {
        return None;
    }
    Some(&bytes[..end])
}

fn internal(err: impl std::fmt::Display) -> PdfError {
    PdfError::Internal(err.to_string())
}

// ---------------------------------------------------------------------------
// Signing keys
// ---------------------------------------------------------------------------

/// Private key material supported for signing. RSA uses PKCS#1 v1.5 (the
/// scheme behind `rsaEncryption` in CMS) and P-256 uses ECDSA/SHA-256.
enum KeyMaterial {
    Rsa(RsaPrivateKey),
    P256(Box<p256::ecdsa::SigningKey>),
}

impl KeyMaterial {
    /// CMS `signatureAlgorithm` identifier.
    fn algorithm_identifier(&self) -> Vec<u8> {
        match self {
            // rsaEncryption carries a NULL parameter.
            KeyMaterial::Rsa(_) => der_sequence(&[der_oid(OID_RSA_ENCRYPTION), der_null()]),
            // ecdsa-with-SHA256 has absent parameters (RFC 5758).
            KeyMaterial::P256(_) => der_sequence(&[der_oid(OID_ECDSA_WITH_SHA256)]),
        }
    }

    fn sign(&self, message: &[u8]) -> PdfResult<Vec<u8>> {
        match self {
            KeyMaterial::Rsa(key) => {
                let signing_key = RsaSigningKey::<Sha256>::new(key.clone());
                let signature: RsaSignature = signing_key.sign(message);
                Ok(signature.to_vec())
            }            KeyMaterial::P256(key) => {
                // `Signer` for ECDSA signing keys is deterministic (RFC 6979).
                let signature: p256::ecdsa::Signature = key.sign(message);
                Ok(signature.to_der().as_bytes().to_vec())
            }
        }
    }

    fn public_key_der(&self) -> PdfResult<Vec<u8>> {
        match self {
            KeyMaterial::Rsa(key) => RsaPublicKey::from(key)
                .to_public_key_der()
                .map(|doc| doc.as_bytes().to_vec())
                .map_err(internal),
            KeyMaterial::P256(key) => key
                .verifying_key()
                .to_public_key_der()
                .map(|doc| doc.as_bytes().to_vec())
                .map_err(internal),
        }
    }
}

/// Parses a private key in PKCS#8 (preferred) or PKCS#1 (RSA only) form.
fn parse_signing_key(key_der: &[u8]) -> PdfResult<KeyMaterial> {
    // PKCS#8 PrivateKeyInfo / OneAsymmetricKey. `pkcs8` also tolerates the
    // optional attributes field that Windows CNG exports include.
    let pkcs8_error = match pkcs8::PrivateKeyInfo::try_from(key_der) {
        Ok(info) => {
            if info.algorithm.oid == OID_RSA_ENCRYPTION {
                let key = RsaPrivateKey::from_pkcs8_der(key_der).map_err(|err| {
                    PdfError::InvalidInput(format!("invalid RSA private key: {err}"))
                })?;
                return Ok(KeyMaterial::Rsa(key));
            }
            if info.algorithm.oid == OID_EC_PUBLIC_KEY {
                let curve = info
                    .algorithm
                    .parameters
                    .and_then(|params| params.decode_as::<DerObjectIdentifier>().ok());
                if curve != Some(OID_EC_P256) {
                    return Err(PdfError::Unsupported(
                        "only P-256 EC keys are supported for signing".into(),
                    ));
                }
                let key = p256::ecdsa::SigningKey::from_pkcs8_der(key_der).map_err(|err| {
                    PdfError::InvalidInput(format!("invalid P-256 private key: {err}"))
                })?;
                return Ok(KeyMaterial::P256(Box::new(key)));
            }
            return Err(PdfError::Unsupported(format!(
                "unsupported private key algorithm {}",
                info.algorithm.oid
            )));
        }
        Err(err) => err.to_string(),
    };
    // Bare PKCS#1 RSAPrivateKey.
    match RsaPrivateKey::from_pkcs1_der(key_der) {
        Ok(key) => Ok(KeyMaterial::Rsa(key)),
        Err(pkcs1_error) => Err(PdfError::InvalidInput(format!(
            "the private key is neither PKCS#8 ({pkcs8_error}) nor PKCS#1 ({pkcs1_error})"
        ))),
    }
}

/// Wraps a bare PKCS#1 RSA key into a PKCS#8 PrivateKeyInfo so callers always
/// receive the documented `key_pkcs8_der` shape.
fn wrap_pkcs1_as_pkcs8(pkcs1_der: &[u8]) -> Vec<u8> {
    let algorithm = der_sequence(&[der_oid(OID_RSA_ENCRYPTION), der_null()]);
    der_sequence(&[
        der_integer_u64(0),
        algorithm,
        der_octet_string(pkcs1_der),
    ])
}

/// Normalizes any accepted private key encoding to PKCS#8.
fn normalize_key_der(key_der: &[u8]) -> Vec<u8> {
    if pkcs8::PrivateKeyInfo::try_from(key_der).is_ok() {
        key_der.to_vec()
    } else {
        wrap_pkcs1_as_pkcs8(key_der)
    }
}

/// True when the private key actually belongs to the certificate's public key.
fn key_matches_certificate(cert: &Certificate, key: &KeyMaterial) -> bool {
    let spki = match cert.tbs_certificate.subject_public_key_info.to_der() {
        Ok(spki) => spki,
        Err(_) => return false,
    };
    match key.public_key_der() {
        Ok(public) => public == spki,
        Err(_) => false,
    }
}

// ---------------------------------------------------------------------------
// Certificate inspection
// ---------------------------------------------------------------------------

fn any_to_string(value: &Any) -> Option<String> {
    use der::Tag;
    match value.tag() {
        Tag::Utf8String => value
            .decode_as::<der::asn1::Utf8StringRef>()
            .ok()
            .map(|s| s.as_str().to_string()),
        Tag::PrintableString => value
            .decode_as::<der::asn1::PrintableStringRef>()
            .ok()
            .map(|s| s.as_str().to_string()),
        Tag::Ia5String => value
            .decode_as::<der::asn1::Ia5StringRef>()
            .ok()
            .map(|s| s.as_str().to_string()),
        Tag::BmpString => value
            .decode_as::<der::asn1::BmpString>()
            .ok()
            .map(|s| s.to_string()),
        _ => Some(String::from_utf8_lossy(value.value()).to_string()),
    }
}

/// Extracts the first common name from a subject/issuer name.
fn name_common_name(name: &x509_cert::name::Name) -> Option<String> {
    for rdn in name.0.iter() {
        for attribute in rdn.0.iter() {
            if attribute.oid == OID_COMMON_NAME {
                if let Some(text) = any_to_string(&attribute.value) {
                    return Some(text);
                }
            }
        }
    }
    None
}

fn time_to_string(time: &x509_cert::time::Time) -> String {
    time.to_date_time().to_string()
}

fn time_is_expired(not_after: &x509_cert::time::Time) -> bool {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    not_after.to_unix_duration().as_secs() < now
}

fn certificate_is_ca(cert: &Certificate) -> bool {
    let extensions = match &cert.tbs_certificate.extensions {
        Some(extensions) => extensions,
        None => return false,
    };
    for extension in extensions {
        if extension.extn_id == OID_BASIC_CONSTRAINTS {
            if let Ok(constraints) =
                x509_cert::ext::pkix::BasicConstraints::from_der(extension.extn_value.as_bytes())
            {
                return constraints.ca;
            }
        }
    }
    false
}

/// Builds the serializable certificate summary used everywhere in the report.
fn certificate_info(cert: &Certificate, der: &[u8]) -> CertificateInfo {
    let serial_bytes = cert.tbs_certificate.serial_number.as_bytes();
    // Drop the DER sign padding (a leading zero keeps the integer positive).
    let mut trimmed = serial_bytes;
    while trimmed.len() > 1 && trimmed[0] == 0 {
        trimmed = &trimmed[1..];
    }
    CertificateInfo {
        subject: cert.tbs_certificate.subject.to_string(),
        issuer: cert.tbs_certificate.issuer.to_string(),
        serial_hex: to_hex_upper(trimmed),
        not_before: time_to_string(&cert.tbs_certificate.validity.not_before),
        not_after: time_to_string(&cert.tbs_certificate.validity.not_after),
        expired: time_is_expired(&cert.tbs_certificate.validity.not_after),
        is_ca: certificate_is_ca(cert),
        sha256_fingerprint: fingerprint_hex(der),
    }
}

/// Digest implied by a combined signature algorithm OID (`sha256WithRSA...`,
/// `ecdsa-with-SHA256`, ...). Certificate signatures always use a combined
/// OID, unlike CMS SignerInfo which splits digest and signature algorithm.
fn digest_of_signature_algorithm(oid: ObjectIdentifier) -> Option<ObjectIdentifier> {
    match oid {
        OID_SHA1_WITH_RSA => Some(OID_SHA1),
        OID_SHA256_WITH_RSA => Some(OID_SHA256),
        OID_SHA384_WITH_RSA => Some(OID_SHA384),
        OID_SHA512_WITH_RSA => Some(OID_SHA512),
        OID_ECDSA_WITH_SHA256 => Some(OID_SHA256),
        OID_ECDSA_WITH_SHA384 => Some(OID_SHA384),
        OID_ECDSA_WITH_SHA512 => Some(OID_SHA512),
        _ => None,
    }
}

/// Verifies that `child` was signed by the public key of `parent`.
fn certificate_signed_by(child: &Certificate, parent: &Certificate) -> bool {
    let tbs = match child.tbs_certificate.to_der() {
        Ok(tbs) => tbs,
        Err(_) => return false,
    };
    let spki = match parent.tbs_certificate.subject_public_key_info.to_der() {
        Ok(spki) => spki,
        Err(_) => return false,
    };
    let digest = match digest_of_signature_algorithm(child.signature_algorithm.oid) {
        Some(digest) => digest,
        None => return false,
    };
    verify_signed_data(
        child.signature_algorithm.oid,
        digest,
        &spki,
        &tbs,
        child.signature.as_bytes().unwrap_or_default(),
    )
}

fn certificate_is_self_signed(cert: &Certificate) -> bool {
    cert.tbs_certificate.issuer == cert.tbs_certificate.subject && certificate_signed_by(cert, cert)
}

/// True when the certificate was signed by the CA that issued it (issuer name
/// matches subject and the signature verifies).
fn certificate_issued_by(child: &Certificate, parent: &Certificate) -> bool {
    child.tbs_certificate.issuer == parent.tbs_certificate.subject && certificate_signed_by(child, parent)
}

// ---------------------------------------------------------------------------
// Signature verification (shared by CMS signer info and certificate chains)
// ---------------------------------------------------------------------------

/// Verifies an RSA PKCS#1 v1.5 or ECDSA signature. `signature_algorithm` is
/// the OID from the SignerInfo (or the certificate signature), `digest_oid`
/// selects the hash; for ECDSA only P-256/SHA-256 is implemented.
fn verify_signed_data(
    signature_algorithm: ObjectIdentifier,
    digest_oid: ObjectIdentifier,
    spki_der: &[u8],
    message: &[u8],
    signature: &[u8],
) -> bool {
    match signature_algorithm {
        OID_RSA_ENCRYPTION
        | OID_SHA1_WITH_RSA
        | OID_SHA256_WITH_RSA
        | OID_SHA384_WITH_RSA
        | OID_SHA512_WITH_RSA => {
            let public_key = match RsaPublicKey::from_public_key_der(spki_der) {
                Ok(key) => key,
                Err(_) => return false,
            };
            let signature = match RsaSignature::try_from(signature) {
                Ok(signature) => signature,
                Err(_) => return false,
            };
            match digest_oid {
                OID_SHA1 => RsaVerifyingKey::<Sha1>::new(public_key)
                    .verify(message, &signature)
                    .is_ok(),
                OID_SHA256 => RsaVerifyingKey::<Sha256>::new(public_key)
                    .verify(message, &signature)
                    .is_ok(),
                OID_SHA384 => RsaVerifyingKey::<Sha384>::new(public_key)
                    .verify(message, &signature)
                    .is_ok(),
                OID_SHA512 => RsaVerifyingKey::<Sha512>::new(public_key)
                    .verify(message, &signature)
                    .is_ok(),
                _ => false,
            }
        }
        OID_ECDSA_WITH_SHA256 | OID_ECDSA_WITH_SHA384 | OID_ECDSA_WITH_SHA512 => {
            // The RustCrypto P-256 verifier hashes with SHA-256; other digests
            // would need another curve implementation and are not claimed.
            if digest_oid != OID_SHA256 {
                return false;
            }
            let public_key = match p256::ecdsa::VerifyingKey::from_public_key_der(spki_der) {
                Ok(key) => key,
                Err(_) => return false,
            };
            let signature = match p256::ecdsa::Signature::from_der(signature) {
                Ok(signature) => signature,
                Err(_) => return false,
            };
            public_key.verify(message, &signature).is_ok()
        }
        _ => false,
    }
}

fn hash_with_oid(oid: ObjectIdentifier, data: &[u8]) -> Option<Vec<u8>> {
    match oid {
        OID_SHA1 => Some(Sha1::digest(data).to_vec()),
        OID_SHA256 => Some(Sha256::digest(data).to_vec()),
        OID_SHA384 => Some(Sha384::digest(data).to_vec()),
        OID_SHA512 => Some(Sha512::digest(data).to_vec()),
        _ => None,
    }
}

fn digest_name(oid: ObjectIdentifier) -> String {
    match oid {
        OID_SHA1 => "SHA-1".to_string(),
        OID_SHA256 => "SHA-256".to_string(),
        OID_SHA384 => "SHA-384".to_string(),
        OID_SHA512 => "SHA-512".to_string(),
        other => other.to_string(),
    }
}

fn signature_algorithm_name(oid: ObjectIdentifier) -> String {
    match oid {
        OID_RSA_ENCRYPTION => "RSA PKCS#1 v1.5".to_string(),
        OID_SHA1_WITH_RSA => "RSA PKCS#1 v1.5 (SHA-1)".to_string(),
        OID_SHA256_WITH_RSA => "RSA PKCS#1 v1.5 (SHA-256)".to_string(),
        OID_SHA384_WITH_RSA => "RSA PKCS#1 v1.5 (SHA-384)".to_string(),
        OID_SHA512_WITH_RSA => "RSA PKCS#1 v1.5 (SHA-512)".to_string(),
        OID_ECDSA_WITH_SHA256 => "ECDSA P-256 (SHA-256)".to_string(),
        OID_ECDSA_WITH_SHA384 => "ECDSA (SHA-384)".to_string(),
        OID_ECDSA_WITH_SHA512 => "ECDSA (SHA-512)".to_string(),
        other => other.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Detached CMS SignedData (hand encoded)
// ---------------------------------------------------------------------------

struct SigningMoment {
    /// DER encoded UTCTime/GeneralizedTime for the signed attribute.
    der_time: Vec<u8>,
    /// `D:YYYYMMDDHHMMSSZ` for the PDF `/M` entry.
    pdf_date: String,
    /// Human readable UTC timestamp for the visible appearance.
    display: String,
}

fn signing_moment_now() -> PdfResult<SigningMoment> {
    let now = time::OffsetDateTime::now_utc();
    let datetime = der::DateTime::new(
        now.year() as u16,
        now.month() as u8,
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
    )
    .map_err(internal)?;
    // RFC 5652: dates through 2049 use UTCTime, later dates GeneralizedTime.
    let der_time = if (1950..=2049).contains(&now.year()) {
        UtcTime::from_date_time(datetime)
            .map_err(internal)?
            .to_der()
            .map_err(internal)?
    } else {
        GeneralizedTime::from_date_time(datetime)
            .to_der()
            .map_err(internal)?
    };
    Ok(SigningMoment {
        der_time,
        pdf_date: format!(
            "D:{:04}{:02}{:02}{:02}{:02}{:02}Z",
            now.year(),
            now.month() as u8,
            now.day(),
            now.hour(),
            now.minute(),
            now.second()
        ),
        display: format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
            now.year(),
            now.month() as u8,
            now.day(),
            now.hour(),
            now.minute(),
            now.second()
        ),
    })
}

/// Builds a detached CMS `SignedData`:
/// * `encapContentInfo` is `id-data` with NO eContent (the PDF bytes are
///   covered through the ByteRange digest, not embedded);
/// * signed attributes are contentType, signingTime and messageDigest;
/// * the signature is computed over the DER `SET OF Attribute` (RFC 5652
///   requires the tag to be `SET OF`, not the `[0] IMPLICIT` form used inside
///   SignerInfo);
/// * the signer certificate and the provided chain are embedded.
fn build_detached_cms(
    message_digest: &[u8],
    moment: &SigningMoment,
    signer_der: &[u8],
    signer_cert: &Certificate,
    chain_der: &[Vec<u8>],
    key: &KeyMaterial,
) -> PdfResult<Vec<u8>> {
    let digest_algorithm = der_sequence(&[der_oid(OID_SHA256)]);

    let attr_content_type = der_sequence(&[
        der_oid(OID_ATTR_CONTENT_TYPE),
        der_set_of(vec![der_oid(OID_DATA)]),
    ]);
    let attr_signing_time = der_sequence(&[
        der_oid(OID_ATTR_SIGNING_TIME),
        der_set_of(vec![moment.der_time.clone()]),
    ]);
    let attr_message_digest = der_sequence(&[
        der_oid(OID_ATTR_MESSAGE_DIGEST),
        der_set_of(vec![der_octet_string(message_digest)]),
    ]);

    // DER SET OF ordering: ascending by encoded bytes.
    let mut attributes = vec![attr_content_type, attr_signing_time, attr_message_digest];
    attributes.sort();
    let signed_attrs_content: Vec<u8> = attributes.concat();

    // The bytes that are actually signed: the SET OF encoding.
    let signature_input = der_tlv(0x31, &signed_attrs_content);
    let signature = key.sign(&signature_input)?;

    // IssuerAndSerialNumber: copy the issuer name and serial from the cert.
    let issuer_der = signer_cert
        .tbs_certificate
        .issuer
        .to_der()
        .map_err(internal)?;
    let serial_der = der_tlv(0x02, signer_cert.tbs_certificate.serial_number.as_bytes());
    let sid = der_sequence(&[issuer_der, serial_der]);

    let signer_info = der_sequence(&[
        der_integer_u64(1),
        sid,
        digest_algorithm.clone(),
        der_context(0, &signed_attrs_content),
        key.algorithm_identifier(),
        der_octet_string(&signature),
    ]);

    // Certificates: SET OF CertificateChoices; the DER ordering applies.
    let mut certificates: Vec<Vec<u8>> = Vec::with_capacity(chain_der.len() + 1);
    certificates.push(signer_der.to_vec());
    certificates.extend(chain_der.iter().cloned());
    certificates.sort();
    certificates.dedup();
    let certificate_set: Vec<u8> = certificates.concat();

    let signed_data = der_sequence(&[
        der_integer_u64(1),
        der_set_of(vec![digest_algorithm]),
        // EncapsulatedContentInfo { eContentType id-data } - no eContent.
        der_sequence(&[der_oid(OID_DATA)]),
        der_context(0, &certificate_set),
        der_set_of(vec![signer_info]),
    ]);

    Ok(der_sequence(&[der_oid(OID_SIGNED_DATA), der_context(0, &signed_data)]))
}

// ---------------------------------------------------------------------------
// PKCS#12 (PFX) parsing
// ---------------------------------------------------------------------------

/// EncryptedPrivateKeyInfo: SEQUENCE { AlgorithmIdentifier, OCTET STRING }.
#[derive(der::Sequence)]
struct EncryptedPrivateKeyInfoLite {
    algorithm: x509_cert::spki::AlgorithmIdentifierOwned,
    encrypted_data: OctetString,
}

/// Decrypts a PKCS#12 PBE (PBES1, SHA-1 KDF) protected payload with 3DES.
/// `key_len` is 24 for 3-key 3DES and 16 for the legacy 2-key variant.
fn pkcs12_pbe_3des_decrypt(
    parameters: &Any,
    ciphertext: &[u8],
    password: &str,
    key_len: usize,
) -> PdfResult<Vec<u8>> {
    use cbc::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};

    let (salt, iterations) = parse_pbe_params(parameters)?;
    let key = pkcs12::kdf::derive_key_utf8::<Sha1>(
        password,
        &salt,
        pkcs12::kdf::Pkcs12KeyType::EncryptionKey,
        iterations,
        key_len,
    )
    .map_err(|err| PdfError::ProcessingFailed(format!("PKCS#12 key derivation failed: {err}")))?;
    let iv = pkcs12::kdf::derive_key_utf8::<Sha1>(
        password,
        &salt,
        pkcs12::kdf::Pkcs12KeyType::Iv,
        iterations,
        8,
    )
    .map_err(|err| PdfError::ProcessingFailed(format!("PKCS#12 IV derivation failed: {err}")))?;

    // The 2-key variant expands K1||K2 to K1||K2||K1.
    let mut key24 = [0u8; 24];
    key24[..key_len].copy_from_slice(&key[..key_len]);
    if key_len == 16 {
        key24[16..].copy_from_slice(&key[..8]);
    }

    let mut buffer = ciphertext.to_vec();
    let plain = cbc::Decryptor::<des::TdesEde3>::new_from_slices(&key24, &iv)
        .map_err(|err| PdfError::ProcessingFailed(format!("3DES setup failed: {err}")))?
        .decrypt_padded_mut::<Pkcs7>(&mut buffer)
        .map_err(|_| PdfError::WrongPassword)?;
    Ok(plain.to_vec())
}

/// Reads the `pkcs-12PbeParams ::= SEQUENCE { salt OCTET STRING, iterations
/// INTEGER }` structure.
fn parse_pbe_params(parameters: &Any) -> PdfResult<(Vec<u8>, i32)> {
    #[derive(der::Sequence)]
    struct Pkcs12PbeParams {
        salt: OctetString,
        iterations: i32,
    }
    // `Any::value()` is only the SEQUENCE content; re-encode the complete TLV.
    let der = parameters.to_der().map_err(internal)?;
    let params = Pkcs12PbeParams::from_der(&der)
        .map_err(|err| PdfError::InvalidInput(format!("invalid PKCS#12 PBE parameters: {err}")))?;
    if params.iterations <= 0 {
        return Err(PdfError::InvalidInput(
            "invalid PKCS#12 PBE iteration count".into(),
        ));
    }
    Ok((params.salt.as_bytes().to_vec(), params.iterations))
}

/// Decrypts a PKCS#12 encrypted bag with the declared password-based
/// encryption scheme. Supports PBES2 (AES/3DES/DES) and the legacy PKCS#12
/// PBE schemes with 3DES; RC2/RC4 are rejected honestly.
fn decrypt_pbe(
    algorithm: &x509_cert::spki::AlgorithmIdentifierOwned,
    ciphertext: &[u8],
    password: &str,
) -> PdfResult<Vec<u8>> {
    if algorithm.oid == OID_PBES2 {
        let der = algorithm.to_der().map_err(internal)?;
        let scheme = pkcs5::EncryptionScheme::from_der(&der)
            .map_err(|err| PdfError::InvalidInput(format!("invalid PBES2 parameters: {err}")))?;
        return scheme
            .decrypt(password.as_bytes(), ciphertext)
            .map_err(|err| PdfError::ProcessingFailed(format!("PKCS#12 decryption failed: {err}")));
    }
    let parameters = algorithm.parameters.as_ref().ok_or_else(|| {
        PdfError::InvalidInput("PKCS#12 encryption algorithm has no parameters".into())
    })?;
    match algorithm.oid {
        OID_PKCS12_PBE_3DES_3KEY => pkcs12_pbe_3des_decrypt(parameters, ciphertext, password, 24),
        OID_PKCS12_PBE_3DES_2KEY => pkcs12_pbe_3des_decrypt(parameters, ciphertext, password, 16),
        OID_PKCS12_PBE_RC4_128 | OID_PKCS12_PBE_RC4_40 => Err(PdfError::Unsupported(
            "this PKCS#12 file uses RC4 encryption, which is not supported; re-export it with AES or 3DES"
                .into(),
        )),
        OID_PKCS12_PBE_RC2_128 | OID_PKCS12_PBE_RC2_40 => Err(PdfError::Unsupported(
            "this PKCS#12 file uses RC2 encryption, which is not supported; re-export it with AES or 3DES"
                .into(),
        )),
        other => Err(PdfError::Unsupported(format!(
            "unsupported PKCS#12 encryption algorithm {other}"
        ))),
    }
}

/// Verifies the PKCS#12 integrity MAC (RFC 7292 section 4). A mismatch is the
/// authoritative wrong-password signal.
fn verify_pkcs12_mac(
    auth_safe_content: &Any,
    mac_data: &pkcs12::mac_data::MacData,
    password: &str,
) -> PdfResult<()> {
    let octets = auth_safe_content
        .decode_as::<OctetString>()
        .map_err(|err| PdfError::InvalidInput(format!("invalid PKCS#12 authSafe: {err}")))?;
    let data = octets.as_bytes();

    let algorithm = mac_data.mac.algorithm.oid;
    let digest = mac_data.mac.digest.as_bytes();
    let iterations = mac_data.iterations.max(1);
    let salt = mac_data.mac_salt.as_bytes();

    macro_rules! verify_with {
        ($hash:ty, $key_len:expr) => {{
            let key = pkcs12::kdf::derive_key_utf8::<$hash>(
                password,
                salt,
                pkcs12::kdf::Pkcs12KeyType::Mac,
                iterations,
                $key_len,
            )
            .map_err(|err| PdfError::ProcessingFailed(format!("PKCS#12 MAC key failed: {err}")))?;
            let mut mac = <Hmac<$hash>>::new_from_slice(&key)
                .map_err(|err| PdfError::ProcessingFailed(format!("HMAC setup failed: {err}")))?;
            mac.update(data);
            let computed = mac.finalize().into_bytes();
            computed.as_slice() == digest
        }};
    }

    let ok = match algorithm {
        OID_SHA1 => verify_with!(Sha1, 20),
        OID_SHA256 => verify_with!(Sha256, 32),
        OID_SHA384 => verify_with!(Sha384, 48),
        OID_SHA512 => verify_with!(Sha512, 64),
        // Unknown MAC algorithm: cannot verify here. Decryption of the key
        // bags below still fails for a wrong password.
        _ => return Ok(()),
    };
    if ok {
        Ok(())
    } else {
        Err(PdfError::WrongPassword)
    }
}

/// Unwraps the `[0] EXPLICIT` bagValue wrapper and returns the inner TLV.
fn bag_value_inner(bag_value: &[u8]) -> PdfResult<Vec<u8>> {
    let any = AnyRef::from_der(bag_value)
        .map_err(|err| PdfError::InvalidInput(format!("invalid PKCS#12 bag value: {err}")))?;
    Ok(any.value().to_vec())
}

/// Collects keys and certificates from a SafeContents list of bags.
fn collect_safe_bags(
    bags: &[pkcs12::safe_bag::SafeBag],
    keys: &mut Vec<Vec<u8>>,
    certs: &mut Vec<Vec<u8>>,
) -> PdfResult<()> {
    for bag in bags {
        if bag.bag_id == OID_PKCS12_KEY_BAG {
            keys.push(bag_value_inner(&bag.bag_value)?);
        } else if bag.bag_id == OID_PKCS12_SHROUDED_KEY_BAG {
            let inner = bag_value_inner(&bag.bag_value)?;
            let encrypted = EncryptedPrivateKeyInfoLite::from_der(&inner).map_err(|err| {
                PdfError::InvalidInput(format!("invalid shrouded key bag: {err}"))
            })?;
            // Decryption happens later so a MAC failure is reported first.
            keys.push(encrypted_key_marker(&encrypted)?);
        } else if bag.bag_id == OID_PKCS12_CERT_BAG {
            let inner = bag_value_inner(&bag.bag_value)?;
            let cert_bag = pkcs12::cert_type::CertBag::from_der(&inner)
                .map_err(|err| PdfError::InvalidInput(format!("invalid certificate bag: {err}")))?;
            if cert_bag.cert_id == OID_X509_CERTIFICATE {
                certs.push(cert_bag.cert_value.as_bytes().to_vec());
            }
        }
    }
    Ok(())
}

/// Placeholder that keeps the encrypted key bag DER around; replaced by the
/// decrypted PKCS#8 when the container password is available.
fn encrypted_key_marker(encrypted: &EncryptedPrivateKeyInfoLite) -> PdfResult<Vec<u8>> {
    Ok(encrypted.to_der().map_err(internal)?)
}

/// Real PKCS#12 parsing and decryption. Returns the signer certificate, its
/// PKCS#8 private key and the remaining certificates. A wrong password is an
/// error - there is no unencrypted fallback.
pub fn parse_pkcs12(pfx_der: &[u8], password: &str) -> PdfResult<Pkcs12Identity> {
    let pfx = pkcs12::pfx::Pfx::from_der(pfx_der)
        .map_err(|err| PdfError::InvalidInput(format!("not a PKCS#12 file: {err}")))?;

    // 1. Verify the integrity MAC when present.
    let mac_present = pfx.mac_data.is_some();
    if let Some(mac_data) = &pfx.mac_data {
        verify_pkcs12_mac(&pfx.auth_safe.content, mac_data, password)?;
    }

    // 2. The authSafe is a `data` ContentInfo whose OCTET STRING carries the
    //    DER encoded AuthenticatedSafe (SEQUENCE OF ContentInfo).
    if pfx.auth_safe.content_type != OID_DATA {
        return Err(PdfError::InvalidInput(
            "unexpected PKCS#12 authSafe content type".into(),
        ));
    }
    let auth_safe_octets = pfx
        .auth_safe
        .content
        .decode_as::<OctetString>()
        .map_err(|err| PdfError::InvalidInput(format!("invalid PKCS#12 authSafe: {err}")))?;
    let authenticated = Vec::<cms::content_info::ContentInfo>::from_der(
        auth_safe_octets.as_bytes(),
    )
    .map_err(|err| PdfError::InvalidInput(format!("invalid AuthenticatedSafe: {err}")))?;

    let mut keys: Vec<Vec<u8>> = Vec::new();
    let mut certs: Vec<Vec<u8>> = Vec::new();

    for content in &authenticated {
        if content.content_type == OID_DATA {
            let octets = content
                .content
                .decode_as::<OctetString>()
                .map_err(|err| PdfError::InvalidInput(format!("invalid safe contents: {err}")))?;
            let bags = Vec::<pkcs12::safe_bag::SafeBag>::from_der(octets.as_bytes())
                .map_err(|err| PdfError::InvalidInput(format!("invalid safe bag list: {err}")))?;
            collect_safe_bags(&bags, &mut keys, &mut certs)?;
        } else if content.content_type == OID_ENCRYPTED_DATA {
            let encrypted = content
                .content
                .decode_as::<cms::encrypted_data::EncryptedData>()
                .map_err(|err| PdfError::InvalidInput(format!("invalid EncryptedData: {err}")))?;
            let ciphertext = encrypted
                .enc_content_info
                .encrypted_content
                .as_ref()
                .ok_or_else(|| PdfError::InvalidInput("encrypted bag has no payload".into()))?;
            let plain = match decrypt_pbe(
                &encrypted.enc_content_info.content_enc_alg,
                ciphertext.as_bytes(),
                password,
            ) {
                Ok(plain) => plain,
                Err(PdfError::ProcessingFailed(_)) if !mac_present => {
                    // No MAC to prove it, but a failed decryption without one
                    // almost always means the password was wrong.
                    return Err(PdfError::WrongPassword);
                }
                Err(err) => return Err(err),
            };
            let bags = Vec::<pkcs12::safe_bag::SafeBag>::from_der(&plain)
                .map_err(|_| PdfError::WrongPassword)?;
            collect_safe_bags(&bags, &mut keys, &mut certs)?;
        }
        // EnvelopedData (public-key encrypted bags) is not applicable to PFX
        // files produced by Windows/OpenSSL and is skipped.
    }

    // 3. Decrypt the shrouded key bags that were collected as raw DER.
    let mut decrypted_keys: Vec<Vec<u8>> = Vec::new();
    for key in &keys {
        if let Ok(encrypted) = EncryptedPrivateKeyInfoLite::from_der(key) {
            match decrypt_pbe(&encrypted.algorithm, encrypted.encrypted_data.as_bytes(), password) {
                Ok(plain) => decrypted_keys.push(normalize_key_der(&plain)),
                Err(PdfError::ProcessingFailed(_)) if !mac_present => {
                    return Err(PdfError::WrongPassword)
                }
                Err(err) => return Err(err),
            }
        } else {
            decrypted_keys.push(normalize_key_der(key));
        }
    }
    if decrypted_keys.is_empty() {
        return Err(PdfError::InvalidInput(
            "the PKCS#12 file contains no private key".into(),
        ));
    }
    if certs.is_empty() {
        return Err(PdfError::InvalidInput(
            "the PKCS#12 file contains no certificate".into(),
        ));
    }

    // 4. Pair the first key that matches a certificate. Windows/OpenSSL
    //    containers normally hold exactly one identity; additional
    //    certificates become the chain.
    let mut signer_cert: Option<Vec<u8>> = None;
    let mut signer_key: Option<Vec<u8>> = None;
    'outer: for key in &decrypted_keys {
        if let Ok(material) = parse_signing_key(key) {
            for cert_der in &certs {
                if let Ok(cert) = Certificate::from_der(cert_der) {
                    if key_matches_certificate(&cert, &material) {
                        signer_cert = Some(cert_der.clone());
                        signer_key = Some(key.clone());
                        break 'outer;
                    }
                }
            }
        }
    }
    let cert_der = signer_cert.unwrap_or_else(|| certs[0].clone());
    let key_pkcs8_der = signer_key.unwrap_or_else(|| decrypted_keys[0].clone());
    let chain_der = certs
        .iter()
        .filter(|candidate| **candidate != cert_der)
        .cloned()
        .collect();

    Ok(Pkcs12Identity {
        cert_der,
        key_pkcs8_der,
        chain_der,
    })
}

// ---------------------------------------------------------------------------
// PDF signing
// ---------------------------------------------------------------------------

/// Size of the `/Contents` placeholder in DER bytes (16384 hex characters).
const CONTENTS_DER_CAPACITY: usize = 8192;
/// Payload length of the `/ByteRange` placeholder literal string. The final
/// array is space padded to the same length so every offset stays stable.
const BYTE_RANGE_SLOT: usize = 64;

/// Builds the text shown inside the widget appearance stream. Only Base14
/// Helvetica is used, so no font embedding is required.
fn appearance_stream_text(name: &str, moment: &SigningMoment, options: &SignOptions) -> String {
    fn winansi(text: &str) -> String {
        // Helvetica without an embedded font is limited to WinAnsi; anything
        // outside ASCII is shown as '?' rather than producing mojibake.
        text.chars()
            .map(|c| if c.is_ascii() { c } else { '?' })
            .collect()
    }
    let mut lines: Vec<String> = Vec::new();
    let signer = if name.trim().is_empty() {
        "Digitally signed".to_string()
    } else {
        format!("Digitally signed by {}", name.trim())
    };
    lines.push(signer);
    lines.push(format!("Date: {}", moment.display));
    if !options.reason.trim().is_empty() {
        lines.push(format!("Reason: {}", options.reason.trim()));
    }
    if !options.location.trim().is_empty() {
        lines.push(format!("Location: {}", options.location.trim()));
    }
    let escaped: Vec<String> = lines
        .iter()
        .map(|line| crate::docutil::escape_pdf_literal(&winansi(line)))
        .collect();
    escaped.join("\n")
}

fn appearance_content(
    name: &str,
    moment: &SigningMoment,
    options: &SignOptions,
    width: f32,
    height: f32,
) -> Vec<u8> {
    let lines = appearance_stream_text(name, moment, options);
    let mut out = String::new();
    out.push_str("q\n");
    out.push_str("0.6 0.6 0.6 RG\n0.5 w\n");
    out.push_str(&format!(
        "0.25 0.25 {:.2} {:.2} re S\n",
        (width - 0.5).max(0.0),
        (height - 0.5).max(0.0)
    ));
    out.push_str("0 0 0 rg\nBT\n/SigFont 8 Tf\n");
    let mut y = height - 12.0;
    for line in lines.split('\n') {
        if y < 4.0 {
            break;
        }
        out.push_str(&format!("1 0 0 1 8 {y:.2} Tm\n({line}) Tj\n"));
        y -= 11.0;
    }
    out.push_str("ET\nQ\n");
    out.into_bytes()
}

/// Clones the target of an indirect reference into the new revision so it can
/// be modified without touching the original revision.
fn clone_reference(inc: &mut IncrementalDocument, object: Option<&Object>) -> PdfResult<Option<lopdf::ObjectId>> {
    match object.and_then(|value| value.as_reference().ok()) {
        Some(id) => {
            inc.opt_clone_object_to_new_document(id)?;
            Ok(Some(id))
        }
        None => Ok(None),
    }
}

/// Appends the widget to the page's `/Annots` array (cloning the array first
/// when it is indirect).
fn append_page_annotation(
    inc: &mut IncrementalDocument,
    page_id: lopdf::ObjectId,
    widget_id: lopdf::ObjectId,
) -> PdfResult<()> {
    let annots = inc
        .new_document
        .get_dictionary(page_id)?
        .get(b"Annots")
        .ok()
        .cloned();
    let array_id = clone_reference(inc, annots.as_ref())?;
    let mut array: Vec<Object> = match (&array_id, &annots) {
        (Some(id), _) => inc.new_document.get_object(*id)?.as_array()?.clone(),
        (None, Some(Object::Array(items))) => items.clone(),
        _ => Vec::new(),
    };
    array.push(Object::Reference(widget_id));
    if let Some(id) = array_id {
        *inc.new_document.get_object_mut(id)?.as_array_mut()? = array;
        inc.new_document
            .get_dictionary_mut(page_id)?
            .set("Annots", Object::Reference(id));
    } else {
        inc.new_document
            .get_dictionary_mut(page_id)?
            .set("Annots", Object::Array(array));
    }
    Ok(())
}

/// Adds the signature field to the catalog's AcroForm, creating the AcroForm
/// when the document has none. Existing fields and resources are preserved.
fn attach_signature_field(
    inc: &mut IncrementalDocument,
    catalog_id: lopdf::ObjectId,
    field_id: lopdf::ObjectId,
    font_id: lopdf::ObjectId,
) -> PdfResult<()> {
    let acroform = inc
        .new_document
        .get_dictionary(catalog_id)?
        .get(b"AcroForm")
        .ok()
        .cloned();
    let acroform_id = clone_reference(inc, acroform.as_ref())?;

    let mut form: Dictionary = match (&acroform_id, &acroform) {
        (Some(id), _) => inc.new_document.get_dictionary(*id)?.clone(),
        (None, Some(Object::Dictionary(dict))) => dict.clone(),
        _ => Dictionary::new(),
    };

    let fields = form.get(b"Fields").ok().cloned();
    let fields_id = clone_reference(inc, fields.as_ref())?;
    let mut field_array: Vec<Object> = match (&fields_id, &fields) {
        (Some(id), _) => inc.new_document.get_object(*id)?.as_array()?.clone(),
        (None, Some(Object::Array(items))) => items.clone(),
        _ => Vec::new(),
    };
    field_array.push(Object::Reference(field_id));
    if let Some(id) = fields_id {
        *inc.new_document.get_object_mut(id)?.as_array_mut()? = field_array;
        form.set("Fields", Object::Reference(id));
    } else {
        form.set("Fields", Object::Array(field_array));
    }
    // SignaturesExist | AppendOnly.
    form.set("SigFlags", 3i64);
    // Provide default resources only when the AcroForm has none; an existing
    // DR belongs to other fields and is left alone.
    if !form.has(b"DR") {
        form.set(
            "DR",
            Object::Dictionary(dictionary! {
                "Font" => Object::Dictionary(dictionary! {
                    "Helv" => Object::Reference(font_id),
                }),
            }),
        );
    }

    if let Some(id) = acroform_id {
        *inc.new_document.get_dictionary_mut(id)? = form;
    } else {
        inc.new_document
            .get_dictionary_mut(catalog_id)?
            .set("AcroForm", Object::Dictionary(form));
    }
    Ok(())
}

fn existing_field_names(doc: &Document) -> HashSet<String> {
    let mut names = HashSet::new();
    for object in doc.objects.values() {
        if let Ok(dict) = object.as_dict() {
            if let Ok(name) = dict.get(b"T") {
                if let Some(text) = crate::docutil::pdf_text_value(name) {
                    names.insert(text);
                }
            }
        }
    }
    names
}

/// Real PDF signing. The document is updated incrementally: the original
/// bytes are preserved, the signature covers the whole revision, and the CMS
/// DER is written into the fixed size `/Contents` placeholder with zero
/// padding after the DER (which the byte range excludes entirely).
pub fn sign_pdf(
    input: &[u8],
    cert_der: &[u8],
    key_pkcs8_der: &[u8],
    chain_der: &[Vec<u8>],
    options: &SignOptions,
) -> PdfResult<Vec<u8>> {
    // 1. Identity and key.
    let signer_cert = Certificate::from_der(cert_der)
        .map_err(|err| PdfError::InvalidInput(format!("invalid signing certificate: {err}")))?;
    let key = parse_signing_key(key_pkcs8_der)?;
    if !key_matches_certificate(&signer_cert, &key) {
        return Err(PdfError::InvalidInput(
            "the private key does not belong to the signing certificate".into(),
        ));
    }

    // 2. Load and validate the PDF.
    let doc = Document::load_mem(input).map_err(|err| PdfError::from_lopdf(err, None))?;
    if doc.is_encrypted() || doc.was_encrypted() {
        return Err(PdfError::PasswordRequired);
    }
    if doc.get_pages().is_empty() {
        return Err(PdfError::InvalidPdf("the document has no pages".into()));
    }
    let mut inc = IncrementalDocument::create_from(input.to_vec(), doc);
    if inc.get_prev_documents().xref_start == 0 {
        return Err(PdfError::InvalidPdf(
            "the document has no usable cross-reference table and cannot be updated".into(),
        ));
    }

    let page_id = *inc
        .get_prev_documents()
        .get_pages()
        .get(&options.page)
        .ok_or(PdfError::RangeOutOfBounds)?;
    let catalog_id = inc
        .get_prev_documents()
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .map_err(|_| PdfError::InvalidPdf("the document has no catalog".into()))?;

    // Clone the page and catalog into the appended revision.
    inc.opt_clone_object_to_new_document(page_id)?;
    inc.opt_clone_object_to_new_document(catalog_id)?;

    let rect = options.rect.unwrap_or(DEFAULT_RECT);
    if rect[2] <= rect[0] || rect[3] <= rect[1] {
        return Err(PdfError::InvalidInput(
            "the signature rectangle is empty".into(),
        ));
    }
    let moment = signing_moment_now()?;

    // 3. Font for the appearance stream (Base14 Helvetica, no embedding).
    let font_id = inc.new_document.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    }));

    // 4. Appearance stream (Form XObject) when requested.
    let appearance_id = if options.appearance {
        let width = rect[2] - rect[0];
        let height = rect[3] - rect[1];
        let signer_name = options
            .signer_name
            .clone()
            .or_else(|| name_common_name(&signer_cert.tbs_certificate.subject))
            .unwrap_or_default();
        let content = appearance_content(&signer_name, &moment, options, width, height);
        let mut stream_dict = Dictionary::new();
        stream_dict.set("Type", "XObject");
        stream_dict.set("Subtype", "Form");
        stream_dict.set("FormType", 1i64);
        stream_dict.set(
            "BBox",
            vec![
                Object::Real(0.0),
                Object::Real(0.0),
                Object::Real(width),
                Object::Real(height),
            ],
        );
        stream_dict.set(
            "Resources",
            Object::Dictionary(dictionary! {
                "Font" => Object::Dictionary(dictionary! {
                    "SigFont" => Object::Reference(font_id),
                }),
            }),
        );
        Some(inc.new_document.add_object(Object::Stream(Stream::new(
            stream_dict,
            content,
        ))))
    } else {
        None
    };

    // 5. Signature dictionary with fixed size placeholders for /ByteRange and
    //    /Contents. The placeholders are replaced in place (same length) after
    //    serialization, so all offsets stay valid.
    let field_name = {
        let taken = existing_field_names(inc.get_prev_documents());
        let mut index = 1;
        loop {
            let candidate = format!("Signature{index}");
            if !taken.contains(&candidate) {
                break candidate;
            }
            index += 1;
        }
    };
    let signer_name = options
        .signer_name
        .clone()
        .or_else(|| name_common_name(&signer_cert.tbs_certificate.subject))
        .unwrap_or_default();

    let mut signature_dict = Dictionary::new();
    signature_dict.set("Type", "Sig");
    signature_dict.set("Filter", "Adobe.PPKLite");
    signature_dict.set("SubFilter", "adbe.pkcs7.detached");
    signature_dict.set(
        "ByteRange",
        Object::String(vec![b'0'; BYTE_RANGE_SLOT], StringFormat::Literal),
    );
    signature_dict.set(
        "Contents",
        Object::String(vec![0u8; CONTENTS_DER_CAPACITY], StringFormat::Hexadecimal),
    );
    signature_dict.set(
        "M",
        Object::String(moment.pdf_date.clone().into_bytes(), StringFormat::Literal),
    );
    if !signer_name.is_empty() {
        signature_dict.set(
            "Name",
            Object::String(signer_name.clone().into_bytes(), StringFormat::Literal),
        );
    }
    if !options.reason.trim().is_empty() {
        signature_dict.set(
            "Reason",
            Object::String(options.reason.trim().as_bytes().to_vec(), StringFormat::Literal),
        );
    }
    if !options.location.trim().is_empty() {
        signature_dict.set(
            "Location",
            Object::String(
                options.location.trim().as_bytes().to_vec(),
                StringFormat::Literal,
            ),
        );
    }
    if !options.contact.trim().is_empty() {
        signature_dict.set(
            "ContactInfo",
            Object::String(options.contact.trim().as_bytes().to_vec(), StringFormat::Literal),
        );
    }
    let signature_id = inc
        .new_document
        .add_object(Object::Dictionary(signature_dict));

    // 6. Widget annotation / signature field.
    let mut field = Dictionary::new();
    field.set("Type", "Annot");
    field.set("Subtype", "Widget");
    field.set("FT", "Sig");
    field.set(
        "T",
        Object::String(field_name.clone().into_bytes(), StringFormat::Literal),
    );
    // Print (4) + Locked (128): the widget is part of the printed page and is
    // locked once signed.
    field.set("F", 132i64);
    field.set(
        "Rect",
        vec![
            Object::Real(rect[0]),
            Object::Real(rect[1]),
            Object::Real(rect[2]),
            Object::Real(rect[3]),
        ],
    );
    field.set("P", Object::Reference(page_id));
    field.set("V", Object::Reference(signature_id));
    if let Some(appearance_id) = appearance_id {
        field.set(
            "AP",
            Object::Dictionary(dictionary! {
                "N" => Object::Reference(appearance_id),
            }),
        );
    }
    let field_id = inc.new_document.add_object(Object::Dictionary(field));

    append_page_annotation(&mut inc, page_id, field_id)?;
    attach_signature_field(&mut inc, catalog_id, field_id, font_id)?;

    // 7. Serialize the incremental update and locate the placeholders.
    let mut output = Vec::new();
    inc.save_to(&mut output)
        .map_err(|err| PdfError::ProcessingFailed(format!("could not write the PDF: {err}")))?;

    let contents_token: Vec<u8> = {
        let mut token = Vec::with_capacity(CONTENTS_DER_CAPACITY * 2 + 2);
        token.push(b'<');
        token.extend(std::iter::repeat_n(b'0', CONTENTS_DER_CAPACITY * 2));
        token.push(b'>');
        token
    };
    let byte_range_token: Vec<u8> = {
        let mut token = Vec::with_capacity(BYTE_RANGE_SLOT + 2);
        token.push(b'(');
        token.extend(std::iter::repeat_n(b'0', BYTE_RANGE_SLOT));
        token.push(b')');
        token
    };
    let contents_start = find_unique(&output, &contents_token)?;
    let byte_range_start = find_unique(&output, &byte_range_token)?;
    // `<` + hex + `>` is excluded from the byte range entirely.
    let contents_end = contents_start + contents_token.len();

    let byte_range = [
        0i64,
        contents_start as i64,
        contents_end as i64,
        (output.len() - contents_end) as i64,
    ];
    // Standard layout: [offset1 length1 offset2 length2].
    let byte_range_text = format!(
        "[{} {} {} {}]",
        byte_range[0], byte_range[1], byte_range[2], byte_range[3]
    );
    if byte_range_text.len() > byte_range_token.len() {
        return Err(PdfError::Internal(
            "the document is too large for the fixed size ByteRange placeholder".into(),
        ));
    }
    let mut padded = byte_range_text.into_bytes();
    padded.resize(byte_range_token.len(), b' ');
    output[byte_range_start..byte_range_start + byte_range_token.len()].copy_from_slice(&padded);

    // 8. Hash every covered byte and build the detached CMS.
    let mut hasher = Sha256::new();
    hasher.update(&output[..contents_start]);
    hasher.update(&output[contents_end..]);
    let digest = hasher.finalize();

    let cms = build_detached_cms(
        &digest,
        &moment,
        cert_der,
        &signer_cert,
        chain_der,
        &key,
    )?;
    // The CMS must be a single well formed DER object; the parser here is the
    // same independent one used by verification.
    let cms_slice = der_exact_slice(&cms)
        .filter(|slice| slice.len() == cms.len())
        .ok_or_else(|| PdfError::Internal("the produced CMS is not valid DER".into()))?;
    let parsed = cms::content_info::ContentInfo::from_der(cms_slice)
        .map_err(|err| PdfError::Internal(format!("the produced CMS could not be parsed: {err}")))?;
    if parsed.content_type != OID_SIGNED_DATA {
        return Err(PdfError::Internal("the produced CMS is not SignedData".into()));
    }

    let hex = to_hex_upper(&cms);
    if hex.len() > CONTENTS_DER_CAPACITY * 2 {
        return Err(PdfError::InvalidInput(
            "the certificate chain is too large for the signature placeholder".into(),
        ));
    }
    let hex_bytes = hex.as_bytes();
    output[contents_start + 1..contents_start + 1 + hex_bytes.len()].copy_from_slice(hex_bytes);
    // Everything after the DER stays '0' - that is the documented zero padding
    // inside /Contents; the byte range excludes the whole token, so the
    // padding never takes part in the digest.

    Ok(output)
}

/// Finds a byte pattern that must occur exactly once.
fn find_unique(haystack: &[u8], needle: &[u8]) -> PdfResult<usize> {
    let mut found: Option<usize> = None;
    let mut start = 0usize;
    while let Some(offset) = haystack[start..]
        .windows(needle.len())
        .position(|window| window == needle)
    {
        let index = start + offset;
        if found.is_some() {
            return Err(PdfError::Internal(
                "ambiguous signature placeholder in the produced PDF".into(),
            ));
        }
        found = Some(index);
        start = index + 1;
    }
    found.ok_or_else(|| {
        PdfError::Internal("the signature placeholder was not found in the produced PDF".into())
    })
}

// ---------------------------------------------------------------------------
// PDF signature verification
// ---------------------------------------------------------------------------

/// Maps signature dictionary object ids to their field names.
fn collect_signature_field_names(doc: &Document) -> std::collections::HashMap<lopdf::ObjectId, String> {
    let mut names = std::collections::HashMap::new();
    for (id, object) in &doc.objects {
        let dict = match object.as_dict() {
            Ok(dict) => dict,
            Err(_) => continue,
        };
        let is_signature_field = dict
            .get(b"FT")
            .ok()
            .and_then(|value| value.as_name().ok())
            .map(|name| name == b"Sig")
            .unwrap_or(false);
        if !is_signature_field {
            continue;
        }
        let field_name = dict
            .get(b"T")
            .ok()
            .and_then(crate::docutil::pdf_text_value)
            .unwrap_or_else(|| format!("Signature{}", id.0));
        if let Ok(value_id) = dict.get(b"V").and_then(Object::as_reference) {
            names.insert(value_id, field_name);
        }
    }
    names
}

/// Computes the covered ranges and the total number of signed bytes.
fn byte_range_pairs(values: &[Object]) -> Option<Vec<(usize, usize)>> {
    if values.len() < 2 || values.len() % 2 != 0 {
        return None;
    }
    let mut pairs = Vec::with_capacity(values.len() / 2);
    for chunk in values.chunks_exact(2) {
        let start = crate::docutil::object_to_f64(&chunk[0])?;
        let length = crate::docutil::object_to_f64(&chunk[1])?;
        if start < 0.0 || length < 0.0 {
            return None;
        }
        pairs.push((start as usize, length as usize));
    }
    Some(pairs)
}

/// Inspects one signature dictionary. `pdf` is the complete raw file.
fn inspect_signature(
    pdf: &[u8],
    dict: &Dictionary,
    field_name: String,
) -> SignatureInfo {
    let mut notes: Vec<String> = Vec::new();
    let sub_filter = dict
        .get(b"SubFilter")
        .ok()
        .and_then(|value| value.as_name().ok())
        .map(|name| String::from_utf8_lossy(name).to_string())
        .unwrap_or_default();

    let contents: Vec<u8> = match dict.get(b"Contents") {
        Ok(Object::String(bytes, _)) => bytes.clone(),
        _ => Vec::new(),
    };
    let ranges = dict
        .get(b"ByteRange")
        .ok()
        .and_then(|value| value.as_array().ok())
        .and_then(|values| byte_range_pairs(values));

    let mut covers_whole_document = false;
    let mut digest_matches = false;

    // The CMS occupies a prefix of /Contents; zero padding follows it. Decode
    // exactly the DER TLV and keep the rest for the coverage check.
    let cms_slice = der_exact_slice(&contents);
    let padding_ok = match cms_slice {
        Some(slice) => contents[slice.len()..].iter().all(|byte| *byte == 0),
        None => false,
    };
    if cms_slice.is_some() && !padding_ok {
        notes.push("the /Contents padding contains non-zero bytes".into());
    }

    // Coverage: every byte except the `<...>` (or `(...)`) token of /Contents
    // must be signed. Hex tokens are 2 characters per byte plus the delimiters.
    if let Some(ranges) = &ranges {
        let covered: usize = ranges.iter().map(|(_, length)| *length).sum();
        let mut intervals: Vec<(usize, usize)> = ranges
            .iter()
            .map(|(start, length)| (*start, start + length))
            .collect();
        intervals.sort();
        let mut gaps: Vec<(usize, usize)> = Vec::new();
        let mut cursor = 0usize;
        for (start, end) in &intervals {
            if *start > cursor {
                gaps.push((cursor, *start));
            }
            cursor = cursor.max(*end);
        }
        if cursor < pdf.len() {
            gaps.push((cursor, pdf.len()));
        }
        let token_length = contents.len() * 2 + 2;
        let whole = ranges.len() >= 2
            && intervals.first().map(|(start, _)| *start == 0).unwrap_or(false)
            && cursor == pdf.len()
            && gaps.len() == 1
            && (gaps[0].1 - gaps[0].0 == token_length
                || gaps[0].1 - gaps[0].0 == token_length.saturating_sub(1));
        covers_whole_document = whole;
        if covered > pdf.len() {
            notes.push("the byte range extends beyond the end of the file".into());
        }
    } else {
        notes.push("the signature has no usable /ByteRange".into());
    }

    // CMS parsing and cryptographic verification.
    let mut signer = CertificateInfo::default();
    let mut chain: Vec<CertificateInfo> = Vec::new();
    let mut chain_linked = false;
    let mut self_signed_chain = false;
    let mut signature_valid = false;
    let mut signing_time: Option<String> = None;
    let mut algorithm = String::from("unknown");

    match cms_slice.and_then(|slice| cms::content_info::ContentInfo::from_der(slice).ok()) {
        Some(content) => match content
            .content
            .decode_as::<cms::signed_data::SignedData>()
        {
            Ok(signed_data) => {
                let signer_info = signed_data.signer_infos.0.iter().next();
                match signer_info {
                    Some(info) => {
                        algorithm = format!(
                            "{} / {}",
                            signature_algorithm_name(info.signature_algorithm.oid),
                            digest_name(info.digest_alg.oid)
                        );
                        // The signed bytes must hash to the messageDigest
                        // attribute.
                        let mut covered_bytes: Vec<u8> = Vec::new();
                        if let Some(ranges) = &ranges {
                            for (start, length) in ranges {
                                let end = (start + length).min(pdf.len());
                                if *start <= end {
                                    covered_bytes.extend_from_slice(&pdf[*start..end]);
                                }
                            }
                        }
                        let expected_digest = info.signed_attrs.as_ref().and_then(|attrs| {
                            attrs.iter().find_map(|attribute| {
                                if attribute.oid != OID_ATTR_MESSAGE_DIGEST {
                                    return None;
                                }
                                attribute
                                    .values
                                    .iter()
                                    .next()
                                    .and_then(|value| value.decode_as::<OctetString>().ok())
                                    .map(|octets| octets.as_bytes().to_vec())
                            })
                        });
                        if let (Some(expected), Some(computed)) = (
                            expected_digest,
                            hash_with_oid(info.digest_alg.oid, &covered_bytes),
                        ) {
                            digest_matches = expected == computed;
                        }
                        signing_time = info.signed_attrs.as_ref().and_then(|attrs| {
                            attrs.iter().find_map(|attribute| {
                                if attribute.oid != OID_ATTR_SIGNING_TIME {
                                    return None;
                                }
                                attribute.values.iter().next().and_then(|value| {
                                    // Time is a CHOICE; pick the branch by tag
                                    // (x509-cert's Time type does not implement
                                    // DecodeValue for `decode_as`).
                                    match value.tag() {
                                        der::Tag::UtcTime => value
                                            .decode_as::<UtcTime>()
                                            .ok()
                                            .map(|time| time.to_date_time().to_string()),
                                        der::Tag::GeneralizedTime => value
                                            .decode_as::<GeneralizedTime>()
                                            .ok()
                                            .map(|time| time.to_date_time().to_string()),
                                        _ => None,
                                    }
                                })
                            })
                        });

                        // Embedded certificates, ordered as a path starting at
                        // the signer.
                        let mut embedded: Vec<(Vec<u8>, Certificate)> = Vec::new();
                        if let Some(certificates) = &signed_data.certificates {
                            for choice in certificates.0.iter() {
                                if let cms::cert::CertificateChoices::Certificate(cert) = choice {
                                    if let Ok(der) = cert.to_der() {
                                        embedded.push((der, cert.clone()));
                                    }
                                }
                            }
                        }
                        let signer_cert = signer_info.and_then(|info| {
                            select_signer_certificate(&embedded, &info.sid)
                        });
                        if let Some((signer_der, signer_cert)) = signer_cert {
                            signer = certificate_info(&signer_cert, &signer_der);
                            // Verify the signature over the signed attributes.
                            if let (Some(attrs), Ok(spki)) = (
                                info.signed_attrs.as_ref(),
                                signer_cert
                                    .tbs_certificate
                                    .subject_public_key_info
                                    .to_der(),
                            ) {
                                if let Ok(attrs_der) = attrs.to_der() {
                                    signature_valid = verify_signed_data(
                                        info.signature_algorithm.oid,
                                        info.digest_alg.oid,
                                        &spki,
                                        &attrs_der,
                                        info.signature.as_bytes(),
                                    );
                                }
                            }
                            // Build the chain: signer first, then any issuer
                            // links, then the remaining certificates.
                            chain = order_chain(&embedded, &signer_der, &signer_cert);
                            chain_linked = chain_is_linked(&embedded, &chain);
                            // The chain is "self signed" when it links all the
                            // way to a self-signed root certificate.
                            self_signed_chain = chain_linked
                                && chain
                                    .last()
                                    .and_then(|last| {
                                        embedded.iter().find(|(der, _)| {
                                            last.sha256_fingerprint == fingerprint_hex(der)
                                        })
                                    })
                                    .map(|(_, cert)| certificate_is_self_signed(cert))
                                    .unwrap_or(false);
                        } else {
                            notes.push(
                                "the signer certificate is not embedded in the signature".into(),
                            );
                        }
                    }
                    None => notes.push("the CMS contains no signer information".into()),
                }
            }
            Err(err) => notes.push(format!("the CMS SignedData could not be decoded: {err}")),
        },
        None => notes.push("the /Contents value is not a parseable CMS object".into()),
    }

    // Some signers omit the signingTime attribute; the /M dictionary entry is
    // then the only available timestamp.
    if signing_time.is_none() {
        signing_time = dict
            .get(b"M")
            .ok()
            .and_then(crate::docutil::pdf_text_value)
            .filter(|text| !text.trim().is_empty());
    }

    // "Modified after signing" means the signed bytes no longer hash to the
    // signed digest, or the byte range does not span the whole revision
    // (for example an incremental update was appended after signing).
    let modified_after_signing = !digest_matches || !covers_whole_document;

    SignatureInfo {
        field_name,
        sub_filter,
        covers_whole_document,
        modified_after_signing,
        digest_matches,
        signature_valid,
        chain,
        chain_linked,
        self_signed_chain,
        signer,
        signing_time,
        algorithm,
        trust: "unknown".to_string(),
        notes,
    }
}

/// Finds the certificate referenced by the SignerInfo identifier.
fn select_signer_certificate(
    embedded: &[(Vec<u8>, Certificate)],
    sid: &cms::signed_data::SignerIdentifier,
) -> Option<(Vec<u8>, Certificate)> {
    match sid {
        cms::signed_data::SignerIdentifier::IssuerAndSerialNumber(identifier) => {
            for (der, cert) in embedded {
                if cert.tbs_certificate.serial_number == identifier.serial_number
                    && cert.tbs_certificate.issuer == identifier.issuer
                {
                    return Some((der.clone(), cert.clone()));
                }
            }
            None
        }
        cms::signed_data::SignerIdentifier::SubjectKeyIdentifier(ski) => {
            for (der, cert) in embedded {
                let matches = cert
                    .tbs_certificate
                    .extensions
                    .as_ref()
                    .map(|extensions| {
                        extensions.iter().any(|extension| {
                            extension.extn_id
                                == const_oid::db::rfc5280::ID_CE_SUBJECT_KEY_IDENTIFIER
                                && extension.extn_value.as_bytes() == ski.0.as_bytes()
                        })
                    })
                    .unwrap_or(false);
                if matches {
                    return Some((der.clone(), cert.clone()));
                }
            }
            None
        }
    }
}

/// Orders the embedded certificates as a path: signer, then each issuer, then
/// whatever is left.
fn order_chain(
    embedded: &[(Vec<u8>, Certificate)],
    signer_der: &[u8],
    signer_cert: &Certificate,
) -> Vec<CertificateInfo> {
    let mut ordered: Vec<CertificateInfo> = vec![certificate_info(signer_cert, signer_der)];
    let mut current = signer_cert.clone();
    let mut used: HashSet<Vec<u8>> = HashSet::new();
    used.insert(signer_der.to_vec());
    loop {
        let next = embedded.iter().find(|(der, cert)| {
            !used.contains(der) && certificate_issued_by(&current, cert)
        });
        match next {
            Some((der, cert)) => {
                ordered.push(certificate_info(cert, der));
                used.insert(der.clone());
                current = cert.clone();
            }
            None => break,
        }
    }
    // Append anything that did not fit the path (unrelated or out of order).
    for (der, cert) in embedded {
        if !used.contains(der) {
            ordered.push(certificate_info(cert, der));
        }
    }
    ordered
}

/// Every adjacent pair in `chain` must link (issuer name plus a valid
/// certificate signature).
fn chain_is_linked(embedded: &[(Vec<u8>, Certificate)], chain: &[CertificateInfo]) -> bool {
    if chain.len() < 2 {
        return true;
    }
    for window in chain.windows(2) {
        let child = embedded
            .iter()
            .find(|(_, cert)| fingerprint_hex(&cert.to_der().unwrap_or_default()) == window[0].sha256_fingerprint);
        let parent = embedded
            .iter()
            .find(|(_, cert)| fingerprint_hex(&cert.to_der().unwrap_or_default()) == window[1].sha256_fingerprint);
        match (child, parent) {
            (Some((_, child)), Some((_, parent))) => {
                if !certificate_issued_by(child, parent) {
                    return false;
                }
            }
            _ => return false,
        }
    }
    true
}

/// Verifies every signature embedded in `pdf`. Returns an empty list when the
/// bytes are not a parseable PDF.
pub fn verify_signatures(pdf: &[u8]) -> SignatureReport {
    let mut report = SignatureReport::default();
    let doc = match Document::load_mem(pdf) {
        Ok(doc) => doc,
        Err(_) => return report,
    };
    let field_names = collect_signature_field_names(&doc);
    for (id, object) in &doc.objects {
        let dict = match object.as_dict() {
            Ok(dict) => dict,
            Err(_) => continue,
        };
        // A signature dictionary always carries /Contents and /ByteRange.
        let looks_like_signature = dict.get(b"ByteRange").is_ok() && dict.get(b"Contents").is_ok();
        if !looks_like_signature {
            continue;
        }
        let field_name = dict
            .get(b"T")
            .ok()
            .and_then(crate::docutil::pdf_text_value)
            .or_else(|| field_names.get(id).cloned())
            .unwrap_or_else(|| format!("Signature{}", id.0));
        report
            .signatures
            .push(inspect_signature(pdf, dict, field_name));
    }
    report
}

// ---------------------------------------------------------------------------
// Tests for the private helpers live next to the public tests in
// tests/signature_test.rs; nothing below this line.
// ---------------------------------------------------------------------------
