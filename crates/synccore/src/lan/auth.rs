//! Host-side request authentication: which peer is asking, which signature
//! version it may use, and what a refusal says.
//!
//! A request is authenticated twice over: the TLS connection is pinned to the
//! peer's certificate (see [`super::transport`]), and the request itself
//! carries an HMAC under the key pairing derived. This module is the second
//! half - the fresh timestamp, the body length, the signature version the two
//! ends negotiated, the MAC, and the replay window, in that order, so a
//! cheap check comes before an expensive one and a MAC that never verified
//! never reaches the replay guard.

use std::sync::Arc;

use sha2::{Digest, Sha256};

use super::sig;
use super::{hex_decode, hex_encode, now_unix, refusal, Failure, HostState, Request, REQUEST_FRESHNESS_SECS};
use crate::SyncError;

/// Refusal kinds that travel in `Response::code`, so the caller can act on the
/// reason instead of matching an English sentence. Only these are ever sent;
/// an older host sends none, and an older client ignores the field.
pub(super) const CODE_BAD_SIGNATURE: &str = "bad-signature";
pub(super) const CODE_SIG_DOWNGRADE: &str = "sig-downgrade";
pub(super) const CODE_RANGE_MISMATCH: &str = "range-mismatch";

/// A refusal the caller can recognise. The message stays human-readable and
/// never says which field failed: a forger learns nothing from it.
pub(super) fn refuse(code: &str, message: impl Into<String>) -> Failure {
    Box::new(super::Response { ok: false, error: message.into(), code: code.to_string(), ..Default::default() })
}

/// Maps a refused answer to the error the caller sees. An empty `code` is a
/// host from before this field, so the message is all there is.
pub(super) fn refusal_error(response: &super::Response) -> SyncError {
    match response.code.as_str() {
        CODE_BAD_SIGNATURE => SyncError::BadSignature,
        CODE_SIG_DOWNGRADE => SyncError::SigDowngrade,
        CODE_RANGE_MISMATCH => SyncError::RangeMismatch,
        _ => SyncError::Network(super::non_empty(&response.error, "the request was refused")),
    }
}

impl HostState {
    /// What the host needs about one peer to check a request: its key and the
    /// signature version the two ends negotiated.
    fn peer_auth(&self, device_id: &str) -> Option<PeerAuth> {
        let peers = self.peers.lock().ok()?;
        let peer = peers.iter().find(|peer| peer.device_id == device_id)?;
        Some(PeerAuth {
            key: hex_decode(&peer.shared_key)?,
            negotiated: sig::SigVersion::for_peer(&peer.features, peer.min_sig),
        })
    }

    /// Records that a peer signed with version 2. The peer list is persisted by
    /// the application layer, so the pin survives a restart.
    fn pin_min_sig(&self, device_id: &str) {
        let Ok(mut peers) = self.peers.lock() else { return };
        if let Some(peer) = peers.iter_mut().find(|peer| peer.device_id == device_id) {
            peer.min_sig = 2;
        }
    }
}

/// One peer's authentication state for a single request.
struct PeerAuth {
    key: Vec<u8>,
    /// The version this peer must sign with; a version 2 request is also
    /// accepted from a peer that never announced it (and pins it then).
    negotiated: sig::SigVersion,
}

pub(super) fn authenticated(state: &Arc<HostState>, request: &Request, body: &[u8]) -> Result<(), Failure> {
    if request.device_id.is_empty() {
        return Err(refusal("not paired"));
    }
    let Some(peer) = state.peer_auth(&request.device_id) else {
        return Err(refusal("this device is not paired"));
    };
    let now = now_unix();
    if now.abs_diff(request.timestamp) > REQUEST_FRESHNESS_SECS {
        return Err(refusal("the request timestamp is stale"));
    }
    if request.body_len != body.len() as u64 {
        return Err(refusal("the request body length does not match"));
    }
    // A version this build does not know is refused instead of guessed at, and
    // a peer that negotiated version 2 may not send version 1: that is the
    // downgrade the pin exists to stop.
    let Some(version) = sig::SigVersion::from_wire(request.sig_v) else {
        return Err(refuse(CODE_BAD_SIGNATURE, "the request signature is not valid"));
    };
    if version == sig::SigVersion::V1 && peer.negotiated == sig::SigVersion::V2 {
        return Err(refuse(CODE_SIG_DOWNGRADE, "this peer must sign its requests with version 2"));
    }
    let body_hash = hex_encode(&Sha256::digest(body));
    if !sig::verify(&peer.key, request, &body_hash, &state.identity.device_id, version) {
        return Err(refuse(CODE_BAD_SIGNATURE, "the request signature is not valid"));
    }
    // Trust on first use: a version 2 request is accepted even when the feature
    // list is missing (an older store file), and the pin then keeps it 2.
    if version == sig::SigVersion::V2 {
        state.pin_min_sig(&request.device_id);
    }
    // A valid MAC is not enough: the same request must not be accepted twice.
    if !state.replay.accept(&request.device_id, &request.nonce, now) {
        return Err(refusal("the request was already used"));
    }
    Ok(())
}
