//! Request signatures: version 1 (the original six fields) and version 2, which
//! binds the range a resumable transfer carries.
//!
//! Version 1 signs `device_id`, timestamp, nonce, op, path and the body hash,
//! newline separated; it cannot see `offset`, `total` or `sha256`. Version 2
//! keeps every version 1 field, adds those three, and separates the fields by
//! length. The version travels per request (`Request::sig_v`), so an older build
//! keeps working: it never writes the field, which reads as 1. Both versions are
//! HMAC-SHA256 over the canonical *bytes*, never over JSON.

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use super::{hex_decode, hex_encode, Request};
use crate::SyncError;

type HmacSha256 = Hmac<Sha256>;

/// Advertised in the pairing handshake by a device that can sign and check
/// version 2.
pub(super) const FEATURE_SIG_V2: &str = "sig-v2";

/// Domain separation: a version 2 tag never reads as a version 1 one.
const V2_PREFIX: &[u8] = b"omnioffice-lan-sig/v2\0";

/// Domain separation for the derived transfer id.
const TRANSFER_PREFIX: &[u8] = b"omnioffice-lan-transfer/v2\0";

/// A feature list arrives in a pairing message, so both ends cap it.
pub(super) const MAX_FEATURES: usize = 8;
pub(super) const MAX_FEATURE_LEN: usize = 32;

/// Which canonical byte string a request's MAC covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SigVersion {
    /// The original form, kept byte for byte: an older build still verifies it.
    V1,
    /// The range-bound form (see [`canonical_v2`]).
    V2,
}

impl SigVersion {
    /// A build that predates the field writes nothing, which reads as `0`.
    pub(super) fn to_wire(self) -> u8 {
        match self {
            SigVersion::V1 => 1,
            SigVersion::V2 => 2,
        }
    }

    /// `None` for a version this build does not know: refuse, never guess.
    pub(super) fn from_wire(byte: u8) -> Option<Self> {
        match byte {
            0 | 1 => Some(SigVersion::V1),
            2 => Some(SigVersion::V2),
            _ => None,
        }
    }

    /// The version to use with a peer: version 2 only when both ends said so
    /// (`features`, exchanged while pairing) or when a version 2 request from
    /// that peer was once accepted (`min_sig`, the pin that stops a later
    /// downgrade).
    pub(super) fn for_peer(features: &[String], min_sig: u8) -> Self {
        if min_sig >= 2 || features.iter().any(|feature| feature == FEATURE_SIG_V2) {
            SigVersion::V2
        } else {
            SigVersion::V1
        }
    }
}

/// What this build announces while pairing.
pub(super) fn advertised() -> Vec<String> {
    vec![FEATURE_SIG_V2.to_string()]
}

/// Keeps a feature list to a known shape: short ASCII words, no duplicates,
/// at most [`MAX_FEATURES`] of them, sorted so both ends derive one order. An
/// unknown feature is kept (a later build may know it) but never affects this
/// build's behaviour.
pub(super) fn sanitize_features(raw: &[String]) -> Vec<String> {
    let mut kept: Vec<String> = Vec::new();
    for feature in raw {
        let trimmed = feature.trim();
        let valid = !trimmed.is_empty()
            && trimmed.len() <= MAX_FEATURE_LEN
            && trimmed.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
        if valid && !kept.iter().any(|known| known == trimmed) && kept.len() < MAX_FEATURES {
            kept.push(trimmed.to_string());
        }
    }
    kept.sort();
    kept
}

/// The version 1 canonical string, unchanged since the first LAN cut.
pub(super) fn canonical_v1(request: &Request, body_hash: &str) -> String {
    format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        request.device_id, request.timestamp, request.nonce, request.op, request.path, body_hash
    )
}

/// The version 2 canonical bytes:
///
/// ```text
/// "omnioffice-lan-sig/v2\0"
/// op sender_id receiver_id transfer_id path offset length body_hash
/// total sha256 nonce timestamp
/// ```
///
/// Every field is written as `u32be` length plus bytes, so a boundary is never
/// ambiguous (`("ab", "c")` and `("a", "bc")` sign differently); the numbers are
/// their 8-byte big-endian form. `transfer_id` is derived from `path`, `total`
/// and `sha256`; `total` is `u64::MAX` when the size is unknown and `sha256` is
/// 32 zero bytes when the hash is. `length` is the request body length, which is
/// the chunk length of an upload.
pub(super) fn canonical_v2(request: &Request, body_hash: &str, receiver: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(320);
    out.extend_from_slice(V2_PREFIX);
    push_field(&mut out, request.op.as_bytes());
    push_field(&mut out, request.device_id.as_bytes());
    push_field(&mut out, receiver.as_bytes());
    push_field(&mut out, &transfer_id(request));
    push_field(&mut out, request.path.as_bytes());
    push_field(&mut out, &request.offset.to_be_bytes());
    push_field(&mut out, &request.body_len.to_be_bytes());
    push_field(&mut out, &digest_bytes(body_hash));
    push_field(&mut out, &known_total(request.total).to_be_bytes());
    push_field(&mut out, &digest_bytes(&request.sha256));
    push_field(&mut out, request.nonce.as_bytes());
    push_field(&mut out, &(request.timestamp as i64).to_be_bytes());
    out
}

/// One length-prefixed field.
fn push_field(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(bytes);
}

/// `total` means "unknown" when it is zero, which the canonical form writes as
/// the reserved `u64::MAX` so a real size can never collide with it.
fn known_total(total: u64) -> u64 {
    if total == 0 {
        u64::MAX
    } else {
        total
    }
}

/// A hex digest as its 32 bytes; anything that is not a digest (empty, wrong
/// length, not hex) is the reserved all-zero value, so a request that does not
/// know the hash still signs deterministically.
fn digest_bytes(hex: &str) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    if let Some(decoded) = hex_decode(hex) {
        if decoded.len() == 32 {
            bytes.copy_from_slice(&decoded);
        }
    }
    bytes
}

/// The transfer a request belongs to: the path plus the document version the
/// caller announced. Both ends compute it from fields already on the wire.
pub(super) fn transfer_id(request: &Request) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(TRANSFER_PREFIX);
    hasher.update(request.path.as_bytes());
    hasher.update([0u8]);
    hasher.update(known_total(request.total).to_be_bytes());
    hasher.update(digest_bytes(&request.sha256));
    hasher.finalize().into()
}

fn message(request: &Request, body_hash: &str, receiver: &str, version: SigVersion) -> Vec<u8> {
    match version {
        SigVersion::V1 => canonical_v1(request, body_hash).into_bytes(),
        SigVersion::V2 => canonical_v2(request, body_hash, receiver),
    }
}

/// Signs a request with the hex pairing key and returns the hex MAC.
pub(super) fn sign(
    key_hex: &str,
    request: &Request,
    body_hash: &str,
    receiver: &str,
    version: SigVersion,
) -> Result<String, SyncError> {
    let key = hex_decode(key_hex).ok_or_else(|| SyncError::Auth("the pairing key is not valid".into()))?;
    let mut mac = HmacSha256::new_from_slice(&key).map_err(|error| SyncError::Auth(error.to_string()))?;
    mac.update(&message(request, body_hash, receiver, version));
    Ok(hex_encode(&mac.finalize().into_bytes()))
}

/// Signs a request the way `LanClient` does, for the tests that build one by
/// hand: the version the two ends negotiated, addressed to the peer's device
/// id.
#[cfg(test)]
pub(super) fn sign_for_test(peer: &super::PairedPeer, request: &mut Request) {
    let body_hash = hex_encode(&Sha256::digest(b""));
    let version = SigVersion::for_peer(&peer.features, peer.min_sig);
    request.sig_v = version.to_wire();
    request.mac = sign(&peer.shared_key, request, &body_hash, &peer.device_id, version).expect("signature");
}

/// Checks the MAC a request carries. A malformed or missing MAC is simply a
/// wrong one: the caller refuses without saying which part failed.
pub(super) fn verify(key: &[u8], request: &Request, body_hash: &str, receiver: &str, version: SigVersion) -> bool {
    verify_tag(key, request, body_hash, receiver, version, &request.mac)
}

/// Checks one tag of a request; the caller says which field it came from.
pub(super) fn verify_tag(
    key: &[u8],
    request: &Request,
    body_hash: &str,
    receiver: &str,
    version: SigVersion,
    claimed: &str,
) -> bool {
    let Some(signature) = hex_decode(claimed) else { return false };
    let Ok(mut mac) = HmacSha256::new_from_slice(key) else { return false };
    mac.update(&message(request, body_hash, receiver, version));
    mac.verify_slice(&signature).is_ok()
}
