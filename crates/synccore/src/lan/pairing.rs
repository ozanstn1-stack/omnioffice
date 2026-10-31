//! Code-authenticated pairing for the LAN sync (E1/3).
//!
//! The six-digit code is a weak password. Sending it, or letting one pairing
//! transcript be tested against it offline, would hand an active attacker on
//! the same network the key every later request is signed with. Pairing
//! therefore runs SPAKE2 over the code and binds both certificates into the key
//! it derives:
//!
//! * **The code never crosses the wire.** It is only the SPAKE2 password. A
//!   device that does not know it cannot compute the key from either handshake
//!   message, so a wrong guess costs one online attempt and nothing else.
//! * **Both certificate fingerprints are hashed into the key.** A device that
//!   relays the two handshakes - presenting its own certificate to one side -
//!   derives two different keys and can confirm neither side. That is what
//!   closes the "relay the pairing, pin the attacker" attack the pinless
//!   pairing connection used to allow.
//! * **A guess costs an attempt.** SPAKE2 does not reject a wrong password on
//!   its own, so the host only learns that a guess was wrong from the
//!   confirmation tag on the second round, where it uses up one of the five
//!   attempts.
//!
//! Three messages over two connections:
//!
//! ```text
//! client -> host  pair_begin   session, spake, device_id, name, fingerprint, port
//! host -> client  pair_begin   spake, fingerprint, confirm (host tag)
//! client -> host  pair_finish  session, device_id, confirm (client tag)
//! host -> client  pair_finish  ok, device_id, name, fingerprint
//! ```
//!
//! The second connection is pinned to the certificate the host answered with
//! in the first round, so the confirmation cannot be redirected either.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use hmac::{Hmac, Mac};
use sha2::Sha256;
use spake2::{Ed25519Group, Identity, Password, Spake2};

use super::transport;
use super::{
    hex_decode, hex_encode, now_rfc3339_utc, now_unix, random_bytes, refusal, Failure, HostState, LanClient,
    PairedPeer, Request, Response, DEFAULT_PORT, PAIRING_TTL_SECS,
};
use crate::SyncError;

type HmacSha256 = Hmac<Sha256>;

/// Version mixed into the password and the key derivation. A device on the
/// previous pairing protocol (which sent the code in the clear) cannot
/// complete a handshake with this one.
pub(super) const PROTOCOL: &str = "omnioffice-lan-pair-v2";

/// SPAKE2 identity strings. Both sides pass the same pair in the same order,
/// so the two instances cannot be confused about their roles.
pub(super) const ID_CLIENT: &[u8] = b"omnioffice-lan-client";
pub(super) const ID_HOST: &[u8] = b"omnioffice-lan-host";

/// Tag labels, so a host tag cannot be replayed as a client tag.
const LABEL_HOST: &str = "host";
const LABEL_CLIENT: &str = "client";

/// Pairing sessions that may wait for their second round at once. A peer that
/// opens first rounds without finishing them cannot exhaust memory; one device
/// only ever needs one (a retry replaces its earlier session).
const MAX_PENDING: usize = 8;
/// A SPAKE2 Ed25519 message is one side byte plus a 32-byte group element, so
/// its hex form is fixed; a bigger field is refused before it is decoded.
const SPAKE_MESSAGE_HEX: usize = 66;
/// Longest identity field a pairing request may carry (`device_id`, `name`,
/// the client's fingerprint): short values, so the in-flight map stays small.
const MAX_IDENTITY_FIELD: usize = 128;

/// The SPAKE2 password for a code: domain-separated, so this exchange cannot be
/// replayed into any other use of the same six digits.
fn password(code: &str) -> Password {
    Password::new(format!("{PROTOCOL}:{}", code.trim()).as_bytes())
}

/// The pairing key: SPAKE2's key with the protocol and both certificate
/// fingerprints hashed in.
pub(super) fn derive(
    spake_key: &[u8],
    client_fingerprint: &str,
    host_fingerprint: &str,
) -> Result<[u8; 32], SyncError> {
    let mut mac = HmacSha256::new_from_slice(spake_key).map_err(|error| SyncError::Auth(error.to_string()))?;
    mac.update(transcript(client_fingerprint, host_fingerprint).as_bytes());
    let digest = mac.finalize().into_bytes();
    let mut key = [0u8; 32];
    key.copy_from_slice(&digest);
    Ok(key)
}

fn transcript(client_fingerprint: &str, host_fingerprint: &str) -> String {
    format!("{PROTOCOL}\nclient:{client_fingerprint}\nhost:{host_fingerprint}")
}

pub(super) fn tag(key: &[u8; 32], label: &str, client_fingerprint: &str, host_fingerprint: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts a 32-byte key");
    mac.update(format!("{PROTOCOL}:{label}\n{}", transcript(client_fingerprint, host_fingerprint)).as_bytes());
    hex_encode(&mac.finalize().into_bytes())
}

/// Compares a hex tag without an early exit on the first wrong byte.
pub(super) fn verify_tag(
    key: &[u8; 32],
    label: &str,
    client_fingerprint: &str,
    host_fingerprint: &str,
    claimed: &str,
) -> bool {
    let expected = tag(key, label, client_fingerprint, host_fingerprint);
    let (expected, claimed) = (expected.as_bytes(), claimed.trim().as_bytes());
    if expected.len() != claimed.len() {
        return false;
    }
    let mut difference = 0u8;
    for (left, right) in expected.iter().zip(claimed.iter()) {
        difference |= left ^ right;
    }
    difference == 0
}

/// One pairing session between its two rounds.
struct Pending {
    key: [u8; 32],
    device_id: String,
    name: String,
    port: u16,
    /// The client's claimed certificate fingerprint, bound into the key.
    fingerprint: String,
    created_at: u64,
}

impl Pending {
    fn fresh(&self, now: u64) -> bool {
        now.saturating_sub(self.created_at) <= PAIRING_TTL_SECS
    }
}

/// The host's in-flight pairing sessions, keyed by the client's session id.
pub(super) struct PendingPairings {
    inner: Mutex<HashMap<String, Pending>>,
}

impl PendingPairings {
    pub(super) fn new() -> Self {
        Self { inner: Mutex::new(HashMap::new()) }
    }

    /// Reserves a session, dropping expired ones and any earlier session the
    /// same device left open (a retry supersedes it). `false` when the host is
    /// already holding its limit or the session id is in flight.
    fn insert(&self, session: String, pending: Pending, now: u64) -> bool {
        let Ok(mut inner) = self.inner.lock() else { return false };
        inner.retain(|_, waiting| waiting.fresh(now) && waiting.device_id != pending.device_id);
        if inner.len() >= MAX_PENDING || inner.contains_key(&session) {
            return false;
        }
        inner.insert(session, pending);
        true
    }

    /// Takes one session out: a pairing attempt is one-shot, so a captured
    /// confirmation cannot be replayed against the same session.
    fn take(&self, session: &str, now: u64) -> Option<Pending> {
        let mut inner = self.inner.lock().ok()?;
        inner.remove(session).filter(|pending| pending.fresh(now))
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.inner.lock().map(|inner| inner.len()).unwrap_or(0)
    }
}

/// The host half of the first round: answers with its SPAKE2 message and the
/// confirmation tag of the key both sides can now derive.
pub(super) fn handle_begin(state: &Arc<HostState>, request: &Request) -> Result<Response, Failure> {
    if request.device_id.trim().is_empty() {
        return Err(refusal("a device id is required"));
    }
    // The session is a hex token the client picks; the identity fields are
    // short on an honest device, and capping them keeps eight in-flight
    // sessions from holding megabytes of attacker-chosen text.
    let session_ok =
        (16..=64).contains(&request.session.len()) && request.session.bytes().all(|byte| byte.is_ascii_hexdigit());
    if !session_ok {
        return Err(refusal("the pairing session is not valid"));
    }
    if request.spake.len() != SPAKE_MESSAGE_HEX {
        return Err(refusal("the pairing message is not valid"));
    }
    if request.device_id.len() > MAX_IDENTITY_FIELD
        || request.name.len() > MAX_IDENTITY_FIELD
        || request.fingerprint.len() > 64
    {
        return Err(refusal("the pairing request is not valid"));
    }
    let peer_message = hex_decode(&request.spake).ok_or_else(|| refusal("the pairing message is not valid"))?;
    let code = open_code(state)?;
    let (handshake, host_message) =
        Spake2::<Ed25519Group>::start_b(&password(&code), &Identity::new(ID_CLIENT), &Identity::new(ID_HOST));
    let Ok(spake_key) = handshake.finish(&peer_message) else {
        return Err(refusal("the pairing message is not valid"));
    };
    let host_fingerprint = state.identity.fingerprint.clone();
    let key =
        derive(&spake_key, &request.fingerprint, &host_fingerprint).map_err(|error| refusal(error.to_string()))?;
    let confirm = tag(&key, LABEL_HOST, &request.fingerprint, &host_fingerprint);
    let pending = Pending {
        key,
        device_id: request.device_id.clone(),
        name: request.name.clone(),
        port: request.port,
        fingerprint: request.fingerprint.clone(),
        created_at: now_unix(),
    };
    if !state.pairing.insert(request.session.clone(), pending, now_unix()) {
        return Err(refusal("too many pairing attempts are waiting; open a new code"));
    }
    Ok(Response {
        ok: true,
        spake: hex_encode(&host_message),
        fingerprint: host_fingerprint,
        device_id: state.identity.device_id.clone(),
        name: state.identity.name.clone(),
        confirm,
        ..Response::default()
    })
}

/// The code of the open pairing window, or the refusal that explains why
/// pairing is not available.
fn open_code(state: &Arc<HostState>) -> Result<String, Failure> {
    let now = now_unix();
    let mut ticket = state.ticket.lock().map_err(|_| refusal("pairing state is unavailable"))?;
    let Some(current) = ticket.as_mut() else {
        return Err(refusal("pairing is closed; the host must show a code first"));
    };
    if current.expired(now) {
        *ticket = None;
        return Err(refusal("the pairing code expired"));
    }
    if current.attempts_left() == 0 {
        return Err(refusal("the pairing code has no attempts left; open a new one"));
    }
    Ok(current.code().to_string())
}

/// The host half of the second round: verifies the client's tag, consumes the
/// one-time code and records the peer with the derived key.
pub(super) fn handle_finish(
    state: &Arc<HostState>,
    request: &Request,
    client_address: Option<SocketAddr>,
) -> Result<Response, Failure> {
    if request.session.is_empty() {
        return Err(refusal("the pairing request is incomplete"));
    }
    let now = now_unix();
    let Some(pending) = state.pairing.take(&request.session, now) else {
        return Err(refusal("the pairing session expired; start again"));
    };
    if request.device_id != pending.device_id {
        return Err(refusal("the pairing session belongs to another device"));
    }
    let host_fingerprint = state.identity.fingerprint.clone();
    if !verify_tag(&pending.key, LABEL_CLIENT, &pending.fingerprint, &host_fingerprint, &request.confirm) {
        // The only place a wrong code becomes visible: charge the attempt and
        // let the caller answer with the same refusal a wrong code always got.
        if let Ok(mut ticket) = state.ticket.lock() {
            let spent = match ticket.as_mut() {
                Some(current) => !current.record_failure(now),
                None => true,
            };
            if spent {
                *ticket = None;
            }
        }
        return Err(refusal("the pairing code is not correct"));
    }
    // One-time code: the window closes with the first confirmed pairing.
    if let Ok(mut ticket) = state.ticket.lock() {
        *ticket = None;
    }
    let address = match (client_address, pending.port) {
        (Some(source), port) if port > 0 => format!("{}:{port}", source.ip()),
        (Some(source), _) => format!("{}:{DEFAULT_PORT}", source.ip()),
        (None, _) => String::new(),
    };
    let peer = PairedPeer {
        device_id: pending.device_id.clone(),
        name: if pending.name.trim().is_empty() { "device".into() } else { pending.name.clone() },
        address,
        fingerprint: pending.fingerprint.clone(),
        shared_key: hex_encode(&pending.key),
        paired_at: now_rfc3339_utc(),
        host_side: true,
    };
    let mut peers = state.peers.lock().map_err(|_| refusal("pairing state is unavailable"))?;
    peers.retain(|known| known.device_id != peer.device_id);
    peers.push(peer);
    Ok(Response {
        ok: true,
        device_id: state.identity.device_id.clone(),
        name: state.identity.name.clone(),
        fingerprint: host_fingerprint,
        ..Response::default()
    })
}

/// The client half: runs both rounds and returns the peer to store.
pub(super) fn pair(client: &LanClient, address: &str, code: &str) -> Result<PairedPeer, SyncError> {
    let session = hex_encode(&random_bytes::<16>()?);
    let (handshake, message) =
        Spake2::<Ed25519Group>::start_a(&password(code), &Identity::new(ID_CLIENT), &Identity::new(ID_HOST));
    // First round: the certificate is not known yet, so it is recorded and
    // checked against the fingerprint the host claims in its answer.
    let mut connection = transport::Client::connect(address, None)?;
    let observed = connection.server_fingerprint.clone();
    let request = Request {
        op: "pair_begin".into(),
        session: session.clone(),
        spake: hex_encode(&message),
        device_id: client.device_id.clone(),
        name: client.name.clone(),
        fingerprint: client.fingerprint.clone(),
        port: client.port,
        ..Request::default()
    };
    let (response, _) = connection.request::<_, Response>(&request, None)?;
    if !response.ok {
        return Err(SyncError::Auth(super::non_empty(&response.error, "pairing was refused")));
    }
    if response.fingerprint.is_empty() || response.fingerprint != observed {
        return Err(SyncError::Protocol("the pairing answer is not from the printed code's host".into()));
    }
    let peer_message = hex_decode(&response.spake)
        .ok_or_else(|| SyncError::Protocol("the host did not answer the pairing handshake".into()))?;
    let spake_key = handshake
        .finish(&peer_message)
        .map_err(|_| SyncError::Protocol("the host's pairing message is not valid".into()))?;
    let key = derive(&spake_key, &client.fingerprint, &observed)?;
    // The tag is bound to this certificate pair: a relay that showed the client
    // a different certificate cannot produce it, and nothing is pinned.
    if !verify_tag(&key, LABEL_HOST, &client.fingerprint, &observed, &response.confirm) {
        return Err(SyncError::Auth("the code does not match this host; check the six digits".into()));
    }
    // Second round, on a connection pinned to the certificate the key was bound
    // to: the peer now holds exactly the pin this device will use later.
    let mut confirmation = transport::Client::connect(address, Some(&observed))?;
    let request = Request {
        op: "pair_finish".into(),
        session,
        device_id: client.device_id.clone(),
        confirm: tag(&key, LABEL_CLIENT, &client.fingerprint, &observed),
        ..Request::default()
    };
    let (response, _) = confirmation.request::<_, Response>(&request, None)?;
    if !response.ok {
        return Err(SyncError::Auth(super::non_empty(&response.error, "pairing was refused")));
    }
    Ok(PairedPeer {
        device_id: response.device_id,
        name: response.name,
        address: address.to_string(),
        fingerprint: observed,
        shared_key: hex_encode(&key),
        paired_at: now_rfc3339_utc(),
        host_side: false,
    })
}
