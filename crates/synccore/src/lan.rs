//! Same-network document sync (C14).
//!
//! Two devices on one Wi-Fi pair once, then move office documents between
//! their folders without an account, a cloud or a relay. The design rules -
//! see `docs/lan-sync.md` for the full threat model - are:
//!
//! * **Pairing is explicit and short-lived.** The host shows a six-digit code
//!   with a 120 s lifetime and five attempts; the code is consumed on the first
//!   successful pairing. A device that never pairs gets nothing.
//! * **Every later connection is pinned.** The client stores the SHA-256 of
//!   the host's self-signed certificate and refuses any other certificate;
//!   there is no CA and no trust-on-first-use after pairing.
//! * **Requests are authenticated.** Pairing establishes a 32-byte shared key;
//!   every request carries an HMAC-SHA256 over device id, timestamp, nonce,
//!   operation, path and body hash. A stale timestamp (five minutes) or a
//!   missing pairing is refused.
//! * **Transfers are bounded and path-safe.** Only known document extensions
//!   leave the folder, only relative paths are accepted (no `..`, no absolute
//!   paths), and both sides cap a single transfer at [`crate::MAX_TRANSFER_BYTES`].
//! * **Differences are never overwritten silently.** [`plan_sync`] classifies
//!   each file with the existing three-way [`crate::metadata::detect_state`]
//!   and leaves conflicts for the user; [`crate::merge::plan`] executes the
//!   explicit resolution the handler picked (`keep both`, `keep local`,
//!   `keep remote`).
//!
//! The transport is a TCP listener with a self-signed certificate per device
//! (see [`transport`]); discovery is a UDP broadcast on the local subnet,
//! because it needs no dependency and works on Android without a multicast
//! lock. mDNS is the documented follow-up.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::merge::{self, MergeAction, Resolution};
use crate::metadata::{self, BaseState, SyncState};
use crate::SyncError;

mod mdns;
mod pairing;
#[cfg(test)]
mod pairing_tests;
mod replay;
#[cfg(test)]
mod tests;
mod transport;

/// TCP port the host listens on by default.
pub const DEFAULT_PORT: u16 = 47820;
/// UDP port the discovery responder answers on.
pub const DISCOVERY_PORT: u16 = 47821;
/// Datagram a client broadcasts to find hosts.
pub const DISCOVERY_PROBE: &[u8] = b"OMNIOFFICE-DISCOVER-1";
/// How long a pairing code stays valid.
pub const PAIRING_TTL_SECS: u64 = 120;
/// Wrong codes one ticket tolerates before it is thrown away.
pub const PAIRING_MAX_ATTEMPTS: u8 = 5;
/// Clock skew a signed request may carry.
pub const REQUEST_FRESHNESS_SECS: u64 = 300;
/// Files one listing may return.
pub const MAX_LISTED_FILES: usize = 10_000;

/// Chunk size of a resumable upload: large enough to amortise the request
/// overhead, small enough that an interruption loses little.
pub const CHUNK_BYTES: usize = 512 * 1024;
/// Largest chunk the host accepts in one `put_chunk`; a misbehaving client is
/// refused instead of buffered.
const MAX_CHUNK_BYTES: usize = 4 * 1024 * 1024;
/// Connections the host handles at once, so one stalled peer cannot queue the
/// rest; a connection that arrives while all workers are busy waits in a
/// queue of [`QUEUE_DEPTH`] and is dropped beyond that.
const WORKER_THREADS: usize = 4;
/// How many accepted connections may wait for a worker.
const QUEUE_DEPTH: usize = 16;
/// Partial uploads older than this are removed when the next upload begins.
const PART_MAX_AGE_SECS: u64 = 24 * 60 * 60;

/// Extensions the LAN sync moves. Anything else stays on the machine.
const ALLOWED_EXTENSIONS: [&str; 14] =
    ["docx", "xlsx", "pptx", "odt", "ods", "odp", "pdf", "rtf", "txt", "csv", "oswk", "png", "jpg", "jpeg"];

type HmacSha256 = Hmac<Sha256>;

/// A device's TLS identity: one self-signed certificate and its private key.
/// The application layer stores it sealed (DPAPI / Android Keystore).
#[derive(Clone, Serialize, Deserialize)]
pub struct LanIdentity {
    pub device_id: String,
    pub name: String,
    /// Base64 DER of the certificate.
    pub certificate: String,
    /// Base64 DER (PKCS#8) of the private key; never logged.
    pub private_key: String,
    /// SHA-256 of the certificate DER, lowercase hex; what peers pin.
    pub fingerprint: String,
}

impl std::fmt::Debug for LanIdentity {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LanIdentity")
            .field("device_id", &self.device_id)
            .field("name", &self.name)
            .field("fingerprint", &self.fingerprint)
            .finish_non_exhaustive()
    }
}

impl LanIdentity {
    /// Generates a fresh identity. `device_id` should come from the existing
    /// `metadata::load_or_create_device_id` so sync metadata stays consistent.
    pub fn generate(device_id: &str, name: &str) -> Result<Self, SyncError> {
        use base64::Engine;
        let key_pair = rcgen::KeyPair::generate().map_err(|error| SyncError::Network(error.to_string()))?;
        let mut params = rcgen::CertificateParams::new(vec![transport::TLS_NAME.to_string()])
            .map_err(|error| SyncError::Network(error.to_string()))?;
        params.distinguished_name.push(rcgen::DnType::CommonName, name);
        let certificate = params.self_signed(&key_pair).map_err(|error| SyncError::Network(error.to_string()))?;
        let certificate_der = certificate.der().to_vec();
        let fingerprint = transport::fingerprint(certificate_der.as_slice());
        let engine = base64::engine::general_purpose::STANDARD;
        Ok(Self {
            device_id: device_id.to_string(),
            name: name.to_string(),
            certificate: engine.encode(&certificate_der),
            private_key: engine.encode(key_pair.serialize_der()),
            fingerprint,
        })
    }

    /// Certificate DER and private key DER, decoded for rustls.
    pub fn material(&self) -> Result<(Vec<u8>, Vec<u8>), SyncError> {
        use base64::Engine;
        let engine = base64::engine::general_purpose::STANDARD;
        let certificate = engine
            .decode(&self.certificate)
            .map_err(|error| SyncError::InvalidInput(format!("stored certificate is not valid: {error}")))?;
        let key = engine
            .decode(&self.private_key)
            .map_err(|error| SyncError::InvalidInput(format!("stored key is not valid: {error}")))?;
        Ok((certificate, key))
    }

    /// A discovery-safe view (no secret material).
    pub fn public(&self) -> LanDevice {
        LanDevice {
            device_id: self.device_id.clone(),
            name: self.name.clone(),
            fingerprint: self.fingerprint.clone(),
            address: String::new(),
        }
    }
}

/// A device seen on the network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanDevice {
    pub device_id: String,
    pub name: String,
    pub fingerprint: String,
    /// `ip:port`, filled in from the discovery reply's source address.
    pub address: String,
}

/// A paired peer. `fingerprint` pins the host certificate on the client side;
/// the host side leaves it empty because the shared key is the credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairedPeer {
    pub device_id: String,
    pub name: String,
    pub address: String,
    pub fingerprint: String,
    /// Hex HMAC key, established at pairing.
    pub shared_key: String,
    pub paired_at: String,
    /// True when this device hosts the pairing; the key works in both
    /// directions, the flag only tells the UI which list a peer belongs to.
    #[serde(default)]
    pub host_side: bool,
}

/// A six-digit one-time pairing code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingCode {
    digits: String,
}

impl PairingCode {
    /// A random code; `000000` is allowed because the value is compared, not
    /// interpreted.
    pub fn generate() -> Result<Self, SyncError> {
        let value = u32::from_be_bytes(random_bytes::<4>()?);
        Ok(Self { digits: format!("{:06}", value % 1_000_000) })
    }

    pub fn as_str(&self) -> &str {
        &self.digits
    }
}

/// The host's live pairing code: a six-digit code, its lifetime and failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingTicket {
    code: PairingCode,
    created_at: u64,
    attempts: u8,
}

impl PairingTicket {
    pub fn new() -> Result<Self, SyncError> {
        Ok(Self { code: PairingCode::generate()?, created_at: now_unix(), attempts: 0 })
    }

    pub fn code(&self) -> &str {
        self.code.as_str()
    }

    pub fn expired(&self, now: u64) -> bool {
        now.saturating_sub(self.created_at) > PAIRING_TTL_SECS
    }

    pub fn attempts_left(&self) -> u8 {
        PAIRING_MAX_ATTEMPTS.saturating_sub(self.attempts)
    }

    /// Charges one failed confirmation. The code itself is never compared
    /// (pairing is code-authenticated, see `pairing`), so the only visible
    /// event is a handshake that did not produce the expected tag.
    ///
    /// Returns `false` when the ticket is used up or expired; the caller drops
    /// it then.
    pub fn record_failure(&mut self, now: u64) -> bool {
        if self.expired(now) || self.attempts >= PAIRING_MAX_ATTEMPTS {
            return false;
        }
        self.attempts = self.attempts.saturating_add(1);
        self.attempts_left() > 0
    }
}

/// What one device offers on the network.
#[derive(Debug, Clone)]
pub struct HostOptions {
    pub identity: LanIdentity,
    pub root: PathBuf,
    /// `0` asks the OS for a free port.
    pub port: u16,
    pub pairing: Option<PairingTicket>,
    pub peers: Vec<PairedPeer>,
}

/// Discovery reply, also used in tests as the wire format.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiscoveryReply {
    protocol: u8,
    device_id: String,
    name: String,
    port: u16,
    fingerprint: String,
}

impl DiscoveryReply {
    fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    fn decode(bytes: &[u8]) -> Option<Self> {
        serde_json::from_slice(bytes).ok().filter(|reply: &Self| reply.protocol == 1)
    }
}

struct HostState {
    identity: LanIdentity,
    root: PathBuf,
    peers: Mutex<Vec<PairedPeer>>,
    ticket: Mutex<Option<PairingTicket>>,
    /// Pairing sessions waiting for their second round (E1/3).
    pairing: pairing::PendingPairings,
    /// Nonces already accepted, so a signed request cannot be replayed.
    replay: replay::ReplayGuard,
    stop: AtomicBool,
}

impl HostState {
    fn peer_key(&self, device_id: &str) -> Option<Vec<u8>> {
        let peers = self.peers.lock().ok()?;
        peers.iter().find(|peer| peer.device_id == device_id).and_then(|peer| hex_decode(&peer.shared_key))
    }
}

/// A running host: a TCP listener plus the discovery responder.
pub struct LanHost {
    state: Arc<HostState>,
    port: u16,
    listener: Option<std::thread::JoinHandle<()>>,
    responder: Option<std::thread::JoinHandle<()>>,
    /// mDNS advertisement; dropped when the host stops.
    _mdns: Option<mdns::Advertiser>,
}

impl LanHost {
    /// Starts listening. The caller keeps the returned handle and calls
    /// [`LanHost::stop`] (or drops it) to shut the host down.
    pub fn start(options: HostOptions) -> Result<Self, SyncError> {
        let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, options.port))
            .map_err(|error| SyncError::Network(format!("the LAN port is not available: {error}")))?;
        listener.set_nonblocking(true).ok();
        let port = listener.local_addr().map_err(|error| SyncError::Network(error.to_string()))?.port();
        let state = Arc::new(HostState {
            identity: options.identity,
            root: options.root,
            peers: Mutex::new(options.peers),
            ticket: Mutex::new(options.pairing),
            pairing: pairing::PendingPairings::new(),
            replay: replay::ReplayGuard::new(REQUEST_FRESHNESS_SECS, replay::REPLAY_CAPACITY),
            stop: AtomicBool::new(false),
        });
        let config = {
            let (certificate, key) = state.identity.material()?;
            transport::server_config(&certificate, &key)?
        };
        // mDNS is best effort: a network without multicast still has the
        // broadcast responder below and manual pairing.
        let advertised = mdns::Advertiser::start(&state.identity, port, &[]).ok();
        let worker = state.clone();
        let listener_thread = std::thread::Builder::new()
            .name("omnioffice-lan-host".into())
            .spawn(move || serve(worker, listener, config))
            .map_err(|error| SyncError::Network(format!("the LAN host could not start: {error}")))?;
        let discovery_state = state.clone();
        let responder_thread = std::thread::Builder::new()
            .name("omnioffice-lan-discovery".into())
            .spawn(move || respond_to_discovery(discovery_state, port))
            .ok();
        Ok(Self { state, port, listener: Some(listener_thread), responder: responder_thread, _mdns: advertised })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn fingerprint(&self) -> String {
        self.state.identity.fingerprint.clone()
    }

    /// The live pairing code, when pairing is open.
    pub fn pairing_code(&self) -> Option<String> {
        let now = now_unix();
        let mut ticket = self.state.ticket.lock().ok()?;
        match ticket.as_ref() {
            Some(current) if current.expired(now) => {
                *ticket = None;
                None
            }
            Some(current) => Some(current.code().to_string()),
            None => None,
        }
    }

    pub fn pairing_attempts_left(&self) -> u8 {
        self.state
            .ticket
            .lock()
            .ok()
            .and_then(|ticket| ticket.as_ref().map(|current| current.attempts_left()))
            .unwrap_or(0)
    }

    /// Opens a new pairing window (a fresh code).
    pub fn open_pairing(&self) -> Result<PairingTicket, SyncError> {
        let ticket = PairingTicket::new()?;
        if let Ok(mut current) = self.state.ticket.lock() {
            *current = Some(ticket.clone());
        }
        Ok(ticket)
    }

    pub fn close_pairing(&self) {
        if let Ok(mut ticket) = self.state.ticket.lock() {
            *ticket = None;
        }
    }

    pub fn peers(&self) -> Vec<PairedPeer> {
        self.state.peers.lock().map(|peers| peers.clone()).unwrap_or_default()
    }

    /// Removes a pairing; its key stops working on the next request.
    pub fn unpair(&self, device_id: &str) -> bool {
        self.state
            .peers
            .lock()
            .map(|mut peers| {
                let before = peers.len();
                peers.retain(|peer| peer.device_id != device_id);
                peers.len() != before
            })
            .unwrap_or(false)
    }

    /// Stops the listener and the discovery responder.
    pub fn stop(mut self) {
        self.state.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.listener.take() {
            let _ = thread.join();
        }
        if let Some(thread) = self.responder.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for LanHost {
    fn drop(&mut self) {
        self.state.stop.store(true, Ordering::SeqCst);
    }
}

/// The client half: generates requests for a paired peer.
#[derive(Debug, Clone)]
pub struct LanClient {
    pub device_id: String,
    pub name: String,
    /// This device's certificate fingerprint, sent at pairing so the host can
    /// call back (symmetric pairing).
    pub fingerprint: String,
    /// This device's listening port, `0` when it is not sharing a folder.
    pub port: u16,
}

/// A remote document entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
    pub modified: String,
}

/// A local document entry with the same shape as [`RemoteFile`].
pub type LocalFile = RemoteFile;

impl LanClient {
    pub fn new(device_id: impl Into<String>, name: impl Into<String>) -> Self {
        Self { device_id: device_id.into(), name: name.into(), fingerprint: String::new(), port: 0 }
    }

    /// Announces this device's certificate and listening port at pairing.
    pub fn with_identity(mut self, fingerprint: impl Into<String>, port: u16) -> Self {
        self.fingerprint = fingerprint.into();
        self.port = port;
        self
    }

    /// Pairs with the host at `address` using its six-digit code: a SPAKE2
    /// handshake over the code, bound to both certificates (see `pairing`).
    pub fn pair(&self, address: &str, code: &str) -> Result<PairedPeer, SyncError> {
        pairing::pair(self, address, code)
    }

    /// Cheap authenticated round trip.
    pub fn ping(&self, peer: &PairedPeer) -> Result<(), SyncError> {
        self.call(peer, "ping", "", None).map(|_| ())
    }

    /// Lists the host's shared folder.
    pub fn list(&self, peer: &PairedPeer) -> Result<Vec<RemoteFile>, SyncError> {
        let (response, _) = self.call(peer, "list", "", None)?;
        Ok(response.files)
    }

    /// Downloads one document.
    pub fn fetch(&self, peer: &PairedPeer, path: &str) -> Result<(RemoteFile, Vec<u8>), SyncError> {
        let (response, body) = self.call(peer, "get", path, None)?;
        Ok((
            RemoteFile {
                path: path.to_string(),
                size: response.size,
                sha256: response.sha256,
                modified: response.modified,
            },
            body,
        ))
    }

    /// Deletes one document on the peer (only after the user confirmed the
    /// deletion in the plan; the protocol has no deleted-by-default path).
    pub fn remove(&self, peer: &PairedPeer, path: &str) -> Result<(), SyncError> {
        self.call(peer, "delete", path, None).map(|_| ())
    }

    /// Uploads one document and returns the host's hash of it.
    pub fn push(&self, peer: &PairedPeer, path: &str, bytes: &[u8]) -> Result<RemoteFile, SyncError> {
        let (response, _) = self.call(peer, "put", path, Some(bytes))?;
        Ok(RemoteFile {
            path: path.to_string(),
            size: response.size,
            sha256: response.sha256,
            modified: response.modified,
        })
    }

    /// Begins (or resumes) a chunked upload; returns the offset the peer
    /// already has for this document.
    pub fn begin_upload(&self, peer: &PairedPeer, path: &str, total: u64, sha256: &str) -> Result<u64, SyncError> {
        let (response, _) = self.request(peer, "begin_put", path, None, 0, total, sha256)?;
        Ok(response.offset)
    }

    /// Sends one chunk and returns the peer's new offset.
    pub fn upload_chunk(
        &self,
        peer: &PairedPeer,
        path: &str,
        offset: u64,
        chunk: &[u8],
        total: u64,
    ) -> Result<u64, SyncError> {
        let (response, _) = self.request(peer, "put_chunk", path, Some(chunk), offset, total, "")?;
        Ok(response.offset)
    }

    /// Verifies the assembled document and moves it into place.
    pub fn finish_upload(
        &self,
        peer: &PairedPeer,
        path: &str,
        total: u64,
        sha256: &str,
    ) -> Result<RemoteFile, SyncError> {
        let (response, _) = self.request(peer, "finish_put", path, None, total, total, sha256)?;
        Ok(RemoteFile {
            path: path.to_string(),
            size: response.size,
            sha256: response.sha256,
            modified: response.modified,
        })
    }

    /// Uploads a document in chunks, resuming at the peer's offset after a
    /// dropped connection (up to three attempts) and reporting progress.
    /// A completed attempt that fails the hash check restarts from zero.
    pub fn push_resumable(
        &self,
        peer: &PairedPeer,
        path: &str,
        bytes: &[u8],
        mut on_progress: impl FnMut(u64, u64),
    ) -> Result<RemoteFile, SyncError> {
        let total = bytes.len() as u64;
        if total > crate::MAX_TRANSFER_BYTES {
            return Err(SyncError::TooLarge(crate::MAX_TRANSFER_BYTES));
        }
        let sha256 = hex_encode(&Sha256::digest(bytes));
        let mut last_error: Option<SyncError> = None;
        for _attempt in 0..3 {
            let mut offset = match self.begin_upload(peer, path, total, &sha256) {
                Ok(offset) => offset.min(total),
                Err(error) => {
                    last_error = Some(error);
                    continue;
                }
            };
            while offset < total {
                let end = (offset as usize + CHUNK_BYTES).min(bytes.len());
                match self.upload_chunk(peer, path, offset, &bytes[offset as usize..end], total) {
                    Ok(next) if next > offset => {
                        offset = next.min(total);
                        on_progress(offset, total);
                    }
                    Ok(_) => {
                        last_error = Some(SyncError::Protocol("the peer did not advance the upload".into()));
                        break;
                    }
                    Err(error) => {
                        last_error = Some(error);
                        break;
                    }
                }
            }
            if offset == total {
                match self.finish_upload(peer, path, total, &sha256) {
                    Ok(file) => {
                        on_progress(total, total);
                        return Ok(file);
                    }
                    Err(error) => last_error = Some(error),
                }
            }
        }
        Err(last_error.unwrap_or_else(|| SyncError::Network("the upload did not finish".into())))
    }

    fn call(
        &self,
        peer: &PairedPeer,
        op: &str,
        path: &str,
        body: Option<&[u8]>,
    ) -> Result<(Response, Vec<u8>), SyncError> {
        self.request(peer, op, path, body, 0, 0, "")
    }

    #[allow(clippy::too_many_arguments)]
    fn request(
        &self,
        peer: &PairedPeer,
        op: &str,
        path: &str,
        body: Option<&[u8]>,
        offset: u64,
        total: u64,
        sha256: &str,
    ) -> Result<(Response, Vec<u8>), SyncError> {
        if peer.shared_key.is_empty() || peer.fingerprint.is_empty() {
            return Err(SyncError::Auth("this peer has not completed pairing".into()));
        }
        let mut connection = transport::Client::connect(peer_address(peer)?, Some(&peer.fingerprint))?;
        let mut request = Request {
            op: op.to_string(),
            body_len: body.map(|bytes| bytes.len() as u64).unwrap_or(0),
            device_id: self.device_id.clone(),
            path: path.to_string(),
            timestamp: now_unix(),
            nonce: hex_encode(&random_bytes::<16>()?),
            offset,
            total,
            sha256: sha256.to_string(),
            ..Request::default()
        };
        let body_hash = hex_encode(&Sha256::digest(body.unwrap_or(&[])));
        request.mac = sign(&peer.shared_key, &request, &body_hash)?;
        let (response, body) = connection.request::<_, Response>(&request, body)?;
        if !response.ok {
            return Err(SyncError::Network(non_empty(&response.error, "the request was refused")));
        }
        Ok((response, body))
    }
}

fn peer_address(peer: &PairedPeer) -> Result<&str, SyncError> {
    if peer.address.is_empty() {
        return Err(SyncError::InvalidInput("this peer has no address yet; discover it again".into()));
    }
    Ok(&peer.address)
}

// ---------------------------------------------------------------------------
// Wire protocol
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Request {
    op: String,
    #[serde(default)]
    body_len: u64,
    #[serde(default)]
    device_id: String,
    #[serde(default)]
    timestamp: u64,
    #[serde(default)]
    nonce: String,
    #[serde(default)]
    mac: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    name: String,
    /// The client's certificate fingerprint, so the host can call it back.
    #[serde(default)]
    fingerprint: String,
    /// The client's listening port, `0` when it is not sharing.
    #[serde(default)]
    port: u16,
    /// Resumable upload: byte offset of this chunk.
    #[serde(default)]
    offset: u64,
    /// Resumable upload: total document size.
    #[serde(default)]
    total: u64,
    /// Resumable upload: SHA-256 of the whole document.
    #[serde(default)]
    sha256: String,
    /// Pairing: the client's one-shot session id.
    #[serde(default)]
    session: String,
    /// Pairing: the client's SPAKE2 message, hex.
    #[serde(default)]
    spake: String,
    /// Pairing: the client's key-confirmation tag, hex.
    #[serde(default)]
    confirm: String,
}

impl transport::HasBodyLen for Request {
    fn body_len(&self) -> u64 {
        self.body_len
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Response {
    ok: bool,
    #[serde(default)]
    error: String,
    #[serde(default)]
    body_len: u64,
    #[serde(default)]
    device_id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    fingerprint: String,
    /// Pairing: the host's SPAKE2 message, hex.
    #[serde(default)]
    spake: String,
    /// Pairing: the host's key-confirmation tag, hex.
    #[serde(default)]
    confirm: String,
    #[serde(default)]
    files: Vec<RemoteFile>,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    sha256: String,
    #[serde(default)]
    modified: String,
    /// Resumable upload: the host's current offset after a begin/chunk.
    #[serde(default)]
    offset: u64,
}

impl transport::HasBodyLen for Response {
    fn body_len(&self) -> u64 {
        self.body_len
    }
}

/// A refused handler result: a `Response` with `ok: false`. Boxed so the
/// `Result` stays small, as clippy asks.
type Failure = Box<Response>;

fn refusal(message: impl Into<String>) -> Failure {
    Box::new(Response { ok: false, error: message.into(), ..Response::default() })
}

// ---------------------------------------------------------------------------
// The host loop
// ---------------------------------------------------------------------------

fn serve(state: Arc<HostState>, listener: TcpListener, config: Arc<rustls::ServerConfig>) {
    // A stalled peer must not queue every other device: the accept loop hands
    // each connection to one of a few workers, and a full queue drops new
    // connections instead of growing without bound.
    let (queue, pending) = std::sync::mpsc::sync_channel::<TcpStream>(QUEUE_DEPTH);
    let pending = Arc::new(Mutex::new(pending));
    for index in 0..WORKER_THREADS {
        let worker_state = state.clone();
        let worker_config = config.clone();
        let worker_queue = pending.clone();
        let spawned = std::thread::Builder::new()
            .name(format!("omnioffice-lan-worker-{index}"))
            .spawn(move || work(worker_state, worker_config, worker_queue));
        if spawned.is_err() {
            break;
        }
    }
    while !state.stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((socket, _)) => {
                let _ = queue.try_send(socket);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(_) => std::thread::sleep(Duration::from_millis(100)),
        }
    }
    // The listener is closed here and the queue is dropped, so every worker
    // wakes up and exits after the request it is already handling. They are not
    // joined: a peer that opens a connection and sends nothing would otherwise
    // hold "stop sharing" for the whole socket timeout.
    drop(queue);
}

/// One connection at a time, until the queue closes.
fn work(
    state: Arc<HostState>,
    config: Arc<rustls::ServerConfig>,
    queue: Arc<Mutex<std::sync::mpsc::Receiver<TcpStream>>>,
) {
    loop {
        let next = queue.lock().ok().and_then(|queue| queue.recv().ok());
        let Some(socket) = next else { return };
        if let Ok(mut peer) = transport::ServerPeer::accept_stream(socket, &config) {
            handle_request(&state, &mut peer);
        }
    }
}

fn handle_request(state: &Arc<HostState>, peer: &mut transport::ServerPeer) {
    let Ok((request, body)) = peer.read_request::<Request>() else { return };
    let client_address = peer.peer_addr();
    let outcome = match request.op.as_str() {
        "pair_begin" => pairing::handle_begin(state, &request).map(|response| (response, None)),
        "pair_finish" => pairing::handle_finish(state, &request, client_address).map(|response| (response, None)),
        // The pre-v7.2 handshake sent the code itself; it is refused instead of
        // being honoured quietly, so an old device is told to update.
        "pair" => Err(refusal("this host needs the updated pairing handshake; update both devices")),
        "ping" => authenticated(state, &request, &body).map(|_| (Response { ok: true, ..Response::default() }, None)),
        "list" => {
            authenticated(state, &request, &body).and_then(|_| handle_list(state)).map(|response| (response, None))
        }
        "get" => authenticated(state, &request, &body).and_then(|_| handle_get(state, &request.path)),
        "put" => authenticated(state, &request, &body)
            .and_then(|_| handle_put(state, &request.path, &body))
            .map(|response| (response, None)),
        "delete" => authenticated(state, &request, &body)
            .and_then(|_| handle_delete(state, &request.path))
            .map(|response| (response, None)),
        "begin_put" => authenticated(state, &request, &body)
            .and_then(|_| handle_begin_put(state, &request))
            .map(|response| (response, None)),
        "put_chunk" => authenticated(state, &request, &body)
            .and_then(|_| handle_put_chunk(state, &request, &body))
            .map(|response| (response, None)),
        "finish_put" => authenticated(state, &request, &body)
            .and_then(|_| handle_finish_put(state, &request))
            .map(|response| (response, None)),
        _ => Err(refusal("unknown operation")),
    };
    let (response, document) = outcome.unwrap_or_else(|failure| (*failure, None));
    let _ = peer.respond(&response, document.as_deref());
}

fn authenticated(state: &Arc<HostState>, request: &Request, body: &[u8]) -> Result<(), Failure> {
    if request.device_id.is_empty() {
        return Err(refusal("not paired"));
    }
    let Some(key) = state.peer_key(&request.device_id) else {
        return Err(refusal("this device is not paired"));
    };
    let now = now_unix();
    if now.abs_diff(request.timestamp) > REQUEST_FRESHNESS_SECS {
        return Err(refusal("the request timestamp is stale"));
    }
    if request.body_len != body.len() as u64 {
        return Err(refusal("the request body length does not match"));
    }
    let body_hash = hex_encode(&Sha256::digest(body));
    verify_mac(&key, request, &body_hash).map_err(|_| refusal("the request signature is not valid"))?;
    // A valid MAC is not enough: the same request must not be accepted twice.
    if !state.replay.accept(&request.device_id, &request.nonce, now) {
        return Err(refusal("the request was already used"));
    }
    Ok(())
}

fn handle_list(state: &Arc<HostState>) -> Result<Response, Failure> {
    let files = scan_folder(&state.root).map_err(|error| refusal(error.to_string()))?;
    if files.len() > MAX_LISTED_FILES {
        return Err(refusal("the shared folder holds too many documents"));
    }
    Ok(Response { ok: true, files, ..Response::default() })
}

fn handle_get(state: &Arc<HostState>, path: &str) -> Result<(Response, Option<Vec<u8>>), Failure> {
    let target = resolve_path(&state.root, path).map_err(|error| refusal(error.to_string()))?;
    let (bytes, sha256) = metadata::read_capped(&target).map_err(|error| refusal(error.to_string()))?;
    let modified = file_modified(&target);
    let response = Response {
        ok: true,
        body_len: bytes.len() as u64,
        size: bytes.len() as u64,
        sha256,
        modified,
        ..Response::default()
    };
    Ok((response, Some(bytes)))
}

fn handle_put(state: &Arc<HostState>, path: &str, body: &[u8]) -> Result<Response, Failure> {
    let target = resolve_path(&state.root, path).map_err(|error| refusal(error.to_string()))?;
    if body.len() as u64 > crate::MAX_TRANSFER_BYTES {
        return Err(refusal("the document is larger than the transfer limit"));
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| refusal(format!("the folder could not be created: {error}")))?;
    }
    metadata::write_atomic(&target, body).map_err(|error| refusal(error.to_string()))?;
    Ok(Response {
        ok: true,
        size: body.len() as u64,
        sha256: hex_encode(&Sha256::digest(body)),
        modified: file_modified(&target),
        ..Response::default()
    })
}

/// Removes one document after the user confirmed its deletion in the plan.
fn handle_delete(state: &Arc<HostState>, path: &str) -> Result<Response, Failure> {
    let target = resolve_path(&state.root, path).map_err(|error| refusal(error.to_string()))?;
    match std::fs::remove_file(&target) {
        Ok(()) => Ok(Response { ok: true, ..Response::default() }),
        // A peer that already removed it is not an error.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Response { ok: true, ..Response::default() }),
        Err(error) => Err(refusal(format!("the document could not be deleted: {error}"))),
    }
}

// ---------------------------------------------------------------------------
// Resumable uploads (D3)
// ---------------------------------------------------------------------------

fn parts_dir(root: &Path) -> PathBuf {
    root.join(".parts")
}

/// One partial upload per relative path; the name is the hash of the path, so
/// no user-controlled component reaches the filesystem as a name.
fn part_path(root: &Path, path: &str) -> Result<PathBuf, Failure> {
    let digest = hex_encode(&Sha256::digest(path.as_bytes()));
    Ok(parts_dir(root).join(format!("{}.part", &digest[..32])))
}

/// Removes partial uploads nobody resumed within the window.
fn cleanup_parts(root: &Path) {
    let Ok(entries) = std::fs::read_dir(parts_dir(root)) else { return };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let Ok(metadata) = entry.metadata() else { continue };
        let Ok(modified) = metadata.modified() else { continue };
        let stale = now.duration_since(modified).map(|age| age.as_secs() > PART_MAX_AGE_SECS).unwrap_or(false);
        if stale {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn handle_begin_put(state: &Arc<HostState>, request: &Request) -> Result<Response, Failure> {
    resolve_path(&state.root, &request.path).map_err(|error| refusal(error.to_string()))?;
    if request.total == 0 || request.total > crate::MAX_TRANSFER_BYTES {
        return Err(refusal("the announced document size is not acceptable"));
    }
    cleanup_parts(&state.root);
    std::fs::create_dir_all(parts_dir(&state.root))
        .map_err(|error| refusal(format!("the upload directory could not be created: {error}")))?;
    let part = part_path(&state.root, &request.path)?;
    let mut offset = std::fs::metadata(&part).map(|metadata| metadata.len()).unwrap_or(0);
    if offset > request.total {
        let _ = std::fs::remove_file(&part);
        offset = 0;
    }
    Ok(Response { ok: true, offset, ..Response::default() })
}

fn handle_put_chunk(state: &Arc<HostState>, request: &Request, body: &[u8]) -> Result<Response, Failure> {
    if body.is_empty() || body.len() > MAX_CHUNK_BYTES {
        return Err(refusal("the chunk size is not acceptable"));
    }
    let part = part_path(&state.root, &request.path)?;
    let current = std::fs::metadata(&part).map(|metadata| metadata.len()).unwrap_or(0);
    if current != request.offset {
        return Err(refusal(format!("the upload is at offset {current}, not {}", request.offset)));
    }
    let total = request.total.max(current);
    if current + body.len() as u64 > total {
        return Err(refusal("the chunk exceeds the announced document size"));
    }
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&part)
        .map_err(|error| refusal(format!("the partial upload could not be opened: {error}")))?;
    file.write_all(body).map_err(|error| refusal(format!("the chunk could not be written: {error}")))?;
    Ok(Response { ok: true, offset: current + body.len() as u64, ..Response::default() })
}

fn handle_finish_put(state: &Arc<HostState>, request: &Request) -> Result<Response, Failure> {
    let part = part_path(&state.root, &request.path)?;
    let current = std::fs::metadata(&part).map(|metadata| metadata.len()).unwrap_or(0);
    if current != request.total {
        return Err(refusal(format!("the upload is incomplete ({current} of {} bytes)", request.total)));
    }
    let mut file =
        std::fs::File::open(&part).map_err(|error| refusal(format!("the partial upload is unreadable: {error}")))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read =
            file.read(&mut buffer).map_err(|error| refusal(format!("the partial upload is unreadable: {error}")))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let digest = hex_encode(&hasher.finalize());
    if digest != request.sha256 {
        // A poisoned part must not be resumed: the client starts over.
        let _ = std::fs::remove_file(&part);
        return Err(refusal("the document hash does not match"));
    }
    let target = resolve_path(&state.root, &request.path).map_err(|error| refusal(error.to_string()))?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| refusal(format!("the folder could not be created: {error}")))?;
    }
    if target.exists() {
        std::fs::remove_file(&target)
            .map_err(|error| refusal(format!("the old document could not be replaced: {error}")))?;
    }
    std::fs::rename(&part, &target)
        .map_err(|error| refusal(format!("the document could not be moved into place: {error}")))?;
    let size = std::fs::metadata(&target).map(|metadata| metadata.len()).unwrap_or(request.total);
    Ok(Response { ok: true, size, sha256: digest, modified: file_modified(&target), ..Response::default() })
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

fn respond_to_discovery(state: Arc<HostState>, port: u16) {
    let Ok(socket) = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, DISCOVERY_PORT)) else { return };
    socket.set_nonblocking(true).ok();
    let reply = DiscoveryReply {
        protocol: 1,
        device_id: state.identity.device_id.clone(),
        name: state.identity.name.clone(),
        port,
        fingerprint: state.identity.fingerprint.clone(),
    }
    .encode();
    let mut buffer = [0u8; 512];
    while !state.stop.load(Ordering::SeqCst) {
        match socket.recv_from(&mut buffer) {
            Ok((length, from)) if buffer[..length] == *DISCOVERY_PROBE => {
                let _ = socket.send_to(&reply, from);
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}

/// Broadcasts a probe and collects the hosts that answer within `timeout`.
/// The local subnet is addressed by the OS broadcast address; this replaces
/// mDNS for the first cut (see `docs/lan-sync.md`).
pub fn discover(timeout: Duration) -> Result<Vec<LanDevice>, SyncError> {
    let half = timeout / 2;
    let mut devices = mdns::discover(half).unwrap_or_default();
    let remaining = timeout.saturating_sub(half);
    if !remaining.is_zero() {
        match discover_broadcast(remaining) {
            Ok(more) => {
                for device in more {
                    if !devices.iter().any(|known| known.device_id == device.device_id) {
                        devices.push(device);
                    }
                }
            }
            Err(error) if devices.is_empty() => return Err(error),
            Err(_) => {}
        }
    }
    devices.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(devices)
}

/// The UDP broadcast fallback from the first cut.
fn discover_broadcast(timeout: Duration) -> Result<Vec<LanDevice>, SyncError> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        .map_err(|error| SyncError::Network(format!("discovery socket failed: {error}")))?;
    socket.set_broadcast(true).ok();
    socket.set_read_timeout(Some(Duration::from_millis(200))).ok();
    socket
        .send_to(DISCOVERY_PROBE, (Ipv4Addr::BROADCAST, DISCOVERY_PORT))
        .map_err(|error| SyncError::Network(format!("discovery broadcast failed: {error}")))?;
    let deadline = std::time::Instant::now() + timeout;
    let mut devices: Vec<LanDevice> = Vec::new();
    let mut buffer = [0u8; 512];
    while std::time::Instant::now() < deadline {
        match socket.recv_from(&mut buffer) {
            Ok((length, from)) => {
                let Some(reply) = DiscoveryReply::decode(&buffer[..length]) else { continue };
                let address = SocketAddr::new(from.ip(), reply.port).to_string();
                if devices.iter().any(|device| device.device_id == reply.device_id) {
                    continue;
                }
                devices.push(LanDevice {
                    device_id: reply.device_id,
                    name: reply.name,
                    fingerprint: reply.fingerprint,
                    address,
                });
            }
            Err(_) => continue,
        }
    }
    devices.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(devices)
}

// ---------------------------------------------------------------------------
// Folder manifest and the sync plan
// ---------------------------------------------------------------------------

/// Walks `root` and fingerprints the documents the LAN sync may move.
pub fn scan_folder(root: &Path) -> Result<Vec<LocalFile>, SyncError> {
    let mut files = Vec::new();
    walk_folder(root, root, &mut files)?;
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

fn walk_folder(root: &Path, directory: &Path, files: &mut Vec<LocalFile>) -> Result<(), SyncError> {
    if files.len() > MAX_LISTED_FILES {
        return Ok(());
    }
    let entries = std::fs::read_dir(directory).map_err(|error| SyncError::Network(error.to_string()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let file_type = entry.file_type().map_err(|error| SyncError::Network(error.to_string()))?;
        if file_type.is_dir() {
            // Hidden folders and the sync sidecars are not documents.
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            walk_folder(root, &path, files)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let relative = match path.strip_prefix(root) {
            Ok(relative) => relative.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };
        if !extension_allowed(&relative) {
            continue;
        }
        let (sha256, size) = metadata::hash_file(&path)?;
        files.push(LocalFile { path: relative, size, sha256, modified: file_modified(&path) });
    }
    Ok(())
}

/// Which side removed a file that both sides had before.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeletionSide {
    /// The file is gone from the local folder but still on the peer.
    Local,
    /// The file is gone from the peer but still in the local folder.
    Remote,
}

/// A deletion the user has to confirm: nothing is removed automatically.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanDeletion {
    pub path: String,
    pub side: DeletionSide,
}

/// Deletions relative to the last common state: a path in the base that is
/// missing on exactly one side was removed there. Paths missing on both sides
/// are not reported (the caller drops their base entries).
pub fn plan_deletions(
    local: &[LocalFile],
    remote: &[RemoteFile],
    bases: &HashMap<String, BaseState>,
) -> Vec<LanDeletion> {
    let mut deletions = Vec::new();
    for path in bases.keys() {
        let local_has = local.iter().any(|file| &file.path == path);
        let remote_has = remote.iter().any(|file| &file.path == path);
        match (local_has, remote_has) {
            (false, true) => deletions.push(LanDeletion { path: path.clone(), side: DeletionSide::Local }),
            (true, false) => deletions.push(LanDeletion { path: path.clone(), side: DeletionSide::Remote }),
            _ => {}
        }
    }
    deletions.sort_by(|left, right| left.path.cmp(&right.path));
    deletions
}

/// One file's state and the action that follows from it. `action` is `None`
/// exactly when the state is a conflict: the user picks a resolution and the
/// caller runs it through [`resolve_action`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanItem {
    pub path: String,
    pub state: SyncState,
    pub action: Option<MergeAction>,
    pub local_sha256: Option<String>,
    pub remote_sha256: Option<String>,
}

impl PlanItem {
    pub fn needs_resolution(&self) -> bool {
        self.state == SyncState::Conflict
    }
}

/// Three-way plan over the local manifest, the remote listing and the last
/// common ancestor hashes (keyed by the relative path).
pub fn plan_sync(local: &[LocalFile], remote: &[RemoteFile], bases: &HashMap<String, BaseState>) -> Vec<PlanItem> {
    let mut items: Vec<PlanItem> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for file in local {
        seen.push(&file.path);
        let remote_file = remote.iter().find(|candidate| candidate.path == file.path);
        let state = metadata::detect_state(
            Some(&file.sha256),
            remote_file.map(|candidate| candidate.sha256.as_str()),
            bases.get(&file.path),
        );
        items.push(PlanItem {
            path: file.path.clone(),
            state,
            action: default_action(state),
            local_sha256: Some(file.sha256.clone()),
            remote_sha256: remote_file.map(|candidate| candidate.sha256.clone()),
        });
    }
    for file in remote {
        if seen.contains(&file.path.as_str()) {
            continue;
        }
        let state = metadata::detect_state(None, Some(&file.sha256), bases.get(&file.path));
        items.push(PlanItem {
            path: file.path.clone(),
            state,
            action: default_action(state),
            local_sha256: None,
            remote_sha256: Some(file.sha256.clone()),
        });
    }
    items.sort_by(|left, right| left.path.cmp(&right.path));
    items
}

/// The action a state allows without asking the user.
fn default_action(state: SyncState) -> Option<MergeAction> {
    match state {
        SyncState::Synced => Some(MergeAction::Nothing),
        SyncState::LocalOnly | SyncState::LocalAhead => Some(MergeAction::UploadLocal),
        SyncState::CloudAhead => Some(MergeAction::DownloadCloud),
        // Never overwrite on a conflict: the caller asks for a resolution.
        SyncState::Conflict => None,
    }
}

/// Runs the user's explicit resolution through the existing merge policy.
pub fn resolve_action(state: SyncState, resolution: Resolution) -> Result<MergeAction, SyncError> {
    merge::plan(resolution, state)
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

fn extension_allowed(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| ALLOWED_EXTENSIONS.iter().any(|known| known.eq_ignore_ascii_case(extension)))
        .unwrap_or(false)
}

/// Validates a relative document path and turns it into a path under `root`.
pub fn resolve_path(root: &Path, relative: &str) -> Result<PathBuf, SyncError> {
    let trimmed = relative.trim().replace('\\', "/");
    if trimmed.is_empty() || trimmed.len() > 512 {
        return Err(SyncError::InvalidInput("the document path is empty or too long".into()));
    }
    if !extension_allowed(&trimmed) {
        return Err(SyncError::InvalidInput("this file type is not shared over the LAN".into()));
    }
    let mut joined = root.to_path_buf();
    for component in Path::new(&trimmed).components() {
        match component {
            Component::Normal(part) => {
                let part = part.to_string_lossy();
                if part.starts_with('.') {
                    return Err(SyncError::InvalidInput("hidden paths are not shared".into()));
                }
                joined.push(part.as_ref());
            }
            _ => return Err(SyncError::InvalidInput("only relative document paths are allowed".into())),
        }
    }
    Ok(joined)
}

fn random_bytes<const N: usize>() -> Result<[u8; N], SyncError> {
    let mut bytes = [0u8; N];
    getrandom::getrandom(&mut bytes).map_err(|error| SyncError::Network(format!("no randomness: {error}")))?;
    Ok(bytes)
}

fn hex_encode(bytes: &[u8]) -> String {
    metadata::hex_lower(bytes)
}

fn hex_decode(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len() / 2).map(|index| u8::from_str_radix(text.get(index * 2..index * 2 + 2)?, 16).ok()).collect()
}

fn sign(shared_key: &str, request: &Request, body_hash: &str) -> Result<String, SyncError> {
    let key = hex_decode(shared_key).ok_or_else(|| SyncError::Auth("the pairing key is not valid".into()))?;
    let mut mac = HmacSha256::new_from_slice(&key).map_err(|error| SyncError::Auth(error.to_string()))?;
    mac.update(canonical(request, body_hash).as_bytes());
    Ok(hex_encode(&mac.finalize().into_bytes()))
}

fn verify_mac(shared_key: &[u8], request: &Request, body_hash: &str) -> Result<(), SyncError> {
    let mut mac = HmacSha256::new_from_slice(shared_key).map_err(|error| SyncError::Auth(error.to_string()))?;
    mac.update(canonical(request, body_hash).as_bytes());
    let signature =
        hex_decode(&request.mac).ok_or_else(|| SyncError::Auth("the request signature is not valid".into()))?;
    mac.verify_slice(&signature).map_err(|_| SyncError::Auth("the request signature is not valid".into()))
}

fn canonical(request: &Request, body_hash: &str) -> String {
    format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        request.device_id, request.timestamp, request.nonce, request.op, request.path, body_hash
    )
}

fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed| elapsed.as_secs()).unwrap_or(0)
}

fn now_rfc3339_utc() -> String {
    metadata::now_rfc3339()
}

fn file_modified(path: &Path) -> String {
    std::fs::metadata(path).and_then(|metadata| metadata.modified()).map(metadata::format_rfc3339).unwrap_or_default()
}

fn non_empty(value: &str, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_string()
    } else {
        value.to_string()
    }
}
