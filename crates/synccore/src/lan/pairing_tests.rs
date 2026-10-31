//! Regression tests for the code-authenticated pairing (E1/3).
//!
//! The pairing connection is the one TLS session whose certificate is not
//! pinned yet, so these tests are the acceptance test for the attack the old
//! handshake allowed: a device that sits between the two and relays the
//! handshake. A TCP relay is not an attack (the client would still pin the real
//! certificate), so the attacker here terminates TLS with its own certificate
//! and substitutes the fingerprint it forwards - exactly what the old check
//! accepted.

use std::net::TcpListener;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};
use spake2::{Ed25519Group, Identity, Password, Spake2};

use super::transport;
use super::*;

fn folder() -> tempfile::TempDir {
    tempfile::tempdir().expect("temp dir")
}

fn host_at(root: &Path) -> (LanHost, PairingTicket, String) {
    let identity = LanIdentity::generate("host-e13", "Atölye PC").expect("identity");
    let ticket = PairingTicket::new().expect("ticket");
    let host = LanHost::start(HostOptions {
        identity,
        root: root.to_path_buf(),
        port: 0,
        pairing: Some(ticket.clone()),
        peers: Vec::new(),
    })
    .expect("host");
    let address = format!("127.0.0.1:{}", host.port());
    (host, ticket, address)
}

/// A client-side SPAKE2 message for a password of the caller's choosing; this
/// is what a device that does not know the code can still produce.
fn begin(session: &str, device_id: &str, password: &str) -> Request {
    let (_handshake, message) = Spake2::<Ed25519Group>::start_a(
        &Password::new(password),
        &Identity::new(pairing::ID_CLIENT),
        &Identity::new(pairing::ID_HOST),
    );
    Request {
        op: "pair_begin".into(),
        session: session.into(),
        spake: hex_encode(&message),
        device_id: device_id.into(),
        name: "Telefon".into(),
        fingerprint: "ee".repeat(32),
        ..Request::default()
    }
}

#[test]
fn a_relaying_mitm_cannot_turn_a_pairing_into_a_pin() {
    let root = folder();
    let (host, ticket, host_address) = host_at(root.path());
    let attacker = LanIdentity::generate("attacker-1", "Attacker").expect("attacker identity");
    let (certificate, key) = attacker.material().expect("material");
    let config = transport::server_config(&certificate, &key).expect("attacker config");
    let attacker_fingerprint = attacker.fingerprint.clone();
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("attacker listener");
    let attacker_address = format!("127.0.0.1:{}", listener.local_addr().expect("addr").port());
    let seen = std::sync::Arc::new(Mutex::new(Vec::new()));

    let relay_fingerprints = seen.clone();
    let upstream_address = host_address.clone();
    let relay = std::thread::spawn(move || {
        let (socket, _) = listener.accept().expect("the client reaches the attacker");
        let mut inbound = transport::ServerPeer::accept_stream(socket, &config).expect("attacker handshake");
        let (request, _) = inbound.read_request::<Request>().expect("pair_begin");
        assert_eq!(request.op, "pair_begin", "the client starts with the pairing round");
        // The attacker has no code and no key: it can only relay the handshake
        // to the real host and put its own fingerprint in the answer, which the
        // old check compared against the certificate it had just recorded.
        let mut upstream = transport::Client::connect(&upstream_address, None).expect("upstream");
        let (mut response, _) = upstream.request::<Request, Response>(&request, None).expect("relayed answer");
        assert!(response.ok, "the real host answers a first round");
        if let Ok(mut seen) = relay_fingerprints.lock() {
            seen.push(response.fingerprint.clone());
        }
        response.fingerprint = attacker_fingerprint;
        inbound.respond(&response, None).expect("attacker answers");
    });

    let client = LanClient::new("client-mitm", "Telefon").with_identity("cd".repeat(32), 0);
    let error = client.pair(&attacker_address, ticket.code()).expect_err("the relay must not pin");
    let message = error.to_string();
    assert!(message.contains("does not match this host"), "unexpected refusal: {message}");
    relay.join().expect("relay thread");
    assert_eq!(*seen.lock().expect("seen"), vec![host.fingerprint()], "the relay did reach the real host");
    assert!(host.peers().is_empty(), "the host recorded no peer for the relayed handshake");
    assert_eq!(host.state.pairing.len(), 1, "the relayed first round left one pending session");
    host.stop();
}

#[test]
fn an_attacker_that_guesses_codes_spends_the_host_attempts() {
    let root = folder();
    let (host, _ticket, address) = host_at(root.path());
    for round in 0..PAIRING_MAX_ATTEMPTS {
        let session = format!("{round:032x}");
        let mut first = transport::Client::connect(&address, None).expect("connect");
        let (answer, _) = first
            .request::<Request, Response>(&begin(&session, "attacker-1", "guessed code"), None)
            .expect("pair_begin");
        assert!(answer.ok, "the first round is not itself a guess");
        assert!(!answer.confirm.is_empty(), "the host returns its confirmation tag");
        let second = Request {
            op: "pair_finish".into(),
            session,
            device_id: "attacker-1".into(),
            confirm: "00".repeat(32),
            ..Request::default()
        };
        let mut confirmation = transport::Client::connect(&address, Some(&answer.fingerprint)).expect("connect");
        let (refused, _) = confirmation.request::<Request, Response>(&second, None).expect("pair_finish");
        assert!(!refused.ok, "a wrong confirmation is refused");
        assert!(refused.error.contains("code is not correct"), "error: {}", refused.error);
        assert_eq!(
            host.pairing_attempts_left(),
            PAIRING_MAX_ATTEMPTS - round - 1,
            "one guess costs exactly one attempt"
        );
    }
    assert!(host.pairing_code().is_none(), "the window closes after the last guess");
    assert!(host.peers().is_empty(), "no guess produced a pairing");
    host.stop();
}

#[test]
fn the_pairing_key_is_bound_to_both_certificates() {
    let spake_key = b"spake2 key material";
    let key = pairing::derive(spake_key, "aa", "bb").expect("key");
    assert_eq!(key, pairing::derive(spake_key, "aa", "bb").expect("stable"), "the derivation is stable");
    assert_ne!(key, pairing::derive(spake_key, "aa", "cc").expect("bound"), "the host certificate is bound in");
    assert_ne!(key, pairing::derive(spake_key, "dd", "bb").expect("bound"), "the client certificate is bound in");
    assert_ne!(key, pairing::derive(b"other", "aa", "bb").expect("bound"), "the handshake key is used");

    let host_tag = pairing::tag(&key, "host", "aa", "bb");
    assert!(pairing::verify_tag(&key, "host", "aa", "bb", &host_tag));
    assert!(!pairing::verify_tag(&key, "host", "aa", "cc", &host_tag), "a substituted certificate makes the tag fail");
    assert!(
        !pairing::verify_tag(&key, "client", "aa", "bb", &host_tag),
        "a host tag cannot be replayed as the client's"
    );
    assert!(!pairing::verify_tag(&key, "host", "aa", "bb", ""), "an empty tag fails");
    assert!(!pairing::verify_tag(&key, "host", "aa", "bb", "zz"), "a short tag fails");
    assert!(!pairing::verify_tag(&key, "host", "aa", "bb", &host_tag.repeat(2)), "a long tag fails");
}

#[test]
fn a_signed_request_is_accepted_once() {
    let root = folder();
    let (host, ticket, address) = host_at(root.path());
    let client = LanClient::new("client-replay", "Telefon");
    let peer = client.pair(&address, ticket.code()).expect("pairing");
    let mut request = Request {
        op: "ping".into(),
        // The host looks the key up by the client's device id, not its own.
        device_id: client.device_id.clone(),
        timestamp: now_unix(),
        nonce: "0f".repeat(16),
        ..Request::default()
    };
    let body_hash = hex_encode(&Sha256::digest(b""));
    request.mac = sign(&peer.shared_key, &request, &body_hash).expect("signature");

    let mut first = transport::Client::connect(&address, Some(&peer.fingerprint)).expect("connect");
    let (accepted, _) = first.request::<Request, Response>(&request, None).expect("first delivery");
    assert!(accepted.ok, "the first delivery is accepted: {}", accepted.error);
    let mut replay = transport::Client::connect(&address, Some(&peer.fingerprint)).expect("connect");
    let (refused, _) = replay.request::<Request, Response>(&request, None).expect("replay");
    assert!(!refused.ok, "the same request is not accepted twice");
    assert!(refused.error.contains("already used"), "error: {}", refused.error);

    // A fresh nonce still works, so the guard is not simply refusing pings.
    let mut fresh = request.clone();
    fresh.nonce = "1e".repeat(16);
    fresh.mac = sign(&peer.shared_key, &fresh, &body_hash).expect("signature");
    let mut connection = transport::Client::connect(&address, Some(&peer.fingerprint)).expect("connect");
    let (accepted, _) = connection.request::<Request, Response>(&fresh, None).expect("fresh delivery");
    assert!(accepted.ok, "a fresh nonce is accepted: {}", accepted.error);
    host.stop();
}

#[test]
fn a_stalled_peer_does_not_block_a_pairing() {
    let root = folder();
    let (host, ticket, address) = host_at(root.path());
    // A peer that connects and never sends a byte holds one worker for the
    // whole socket timeout; the old single-threaded loop queued everyone else
    // behind it.
    let stalled = std::net::TcpStream::connect(&address).expect("stalled connection");
    std::thread::sleep(Duration::from_millis(250));
    let started = Instant::now();
    let client = LanClient::new("client-live", "Telefon");
    let peer = client.pair(&address, ticket.code()).expect("the live pairing still gets through");
    client.ping(&peer).expect("ping");
    assert!(started.elapsed() < Duration::from_secs(10), "waited {:?}", started.elapsed());
    drop(stalled);
    host.stop();
}

#[test]
fn a_client_on_the_old_pairing_protocol_is_refused() {
    let root = folder();
    let (host, _ticket, address) = host_at(root.path());
    let mut connection = transport::Client::connect(&address, None).expect("connect");
    let legacy = Request { op: "pair".into(), device_id: "old-client".into(), ..Request::default() };
    let (answer, _) = connection.request::<Request, Response>(&legacy, None).expect("answer");
    assert!(!answer.ok, "the pre-v7.2 handshake is refused");
    assert!(answer.error.contains("update both devices"), "error: {}", answer.error);
    assert_eq!(host.pairing_attempts_left(), PAIRING_MAX_ATTEMPTS, "a stale opcode is not a guess");
    assert!(host.pairing_code().is_some(), "the window stays open for an updated device");
    host.stop();
}

#[test]
fn a_malformed_pairing_request_is_refused_without_side_effects() {
    let root = folder();
    let (host, _ticket, address) = host_at(root.path());
    let session = "0f".repeat(16);
    let valid = begin(&session, "client-fields", "anything");
    let cases = [
        Request { session: "not-a-hex-token".into(), ..valid.clone() },
        Request { spake: "00".into(), ..valid.clone() },
        Request { name: "n".repeat(200), ..valid.clone() },
        Request { fingerprint: "aa".repeat(64), ..valid.clone() },
    ];
    for case in cases {
        let mut connection = transport::Client::connect(&address, None).expect("connect");
        let (answer, _) = connection.request::<Request, Response>(&case, None).expect("answer");
        assert!(!answer.ok, "a malformed pairing request must be refused: {case:?}");
        assert!(answer.spake.is_empty(), "nothing of the handshake is answered: {:?}", answer.spake);
    }
    assert_eq!(host.state.pairing.len(), 0, "a refused request holds no session");
    assert!(host.pairing_code().is_some(), "the window stays open for a real client");
    host.stop();
}

#[test]
fn stopping_the_host_does_not_wait_for_a_stalled_peer() {
    let root = folder();
    let (host, _ticket, address) = host_at(root.path());
    let stalled = std::net::TcpStream::connect(&address).expect("stalled connection");
    std::thread::sleep(Duration::from_millis(250));
    let started = Instant::now();
    host.stop();
    assert!(started.elapsed() < Duration::from_secs(5), "stopping waited {:?}", started.elapsed());
    drop(stalled);
}
