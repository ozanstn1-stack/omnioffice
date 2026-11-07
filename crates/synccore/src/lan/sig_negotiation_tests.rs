//! Tests for the request signature versions over a real loopback host: what
//! pairing records on both sides, and the rule that a peer which once signed
//! with version 2 cannot go back to version 1.

use super::tests::{folder, host_at, write_doc};
use super::*;

/// A host that already holds one peer, so the version rules are checked without
/// a pairing handshake in the way. Returns the address and the fingerprint the
/// client has to pin.
pub(super) fn seeded_host(root: &Path, peer: PairedPeer) -> (LanHost, String, String) {
    let identity = LanIdentity::generate("host-seeded", "Atölye PC").expect("identity");
    let fingerprint = identity.fingerprint.clone();
    let host =
        LanHost::start(HostOptions { identity, root: root.to_path_buf(), port: 0, pairing: None, peers: vec![peer] })
            .expect("host");
    let address = format!("127.0.0.1:{}", host.port());
    (host, address, fingerprint)
}

/// The host's record of the client: what the client announced while pairing.
pub(super) fn host_side_peer(features: &[&str], min_sig: u8) -> PairedPeer {
    PairedPeer {
        device_id: "client-seeded".into(),
        name: "Telefon".into(),
        address: String::new(),
        fingerprint: "cd".repeat(32),
        shared_key: "ab".repeat(32),
        paired_at: "2026-11-07T00:00:00Z".into(),
        host_side: true,
        features: features.iter().map(|feature| (*feature).to_string()).collect(),
        min_sig,
    }
}

/// The client's record of the host.
pub(super) fn client_side_peer(address: &str, fingerprint: &str, features: &[&str], min_sig: u8) -> PairedPeer {
    PairedPeer {
        device_id: "host-seeded".into(),
        name: "Atölye PC".into(),
        address: address.to_string(),
        fingerprint: fingerprint.to_string(),
        shared_key: "ab".repeat(32),
        paired_at: "2026-11-07T00:00:00Z".into(),
        host_side: false,
        features: features.iter().map(|feature| (*feature).to_string()).collect(),
        min_sig,
    }
}

// ---------------------------------------------------------------------------
// Request signature versions (E6 hardening)
// ---------------------------------------------------------------------------

#[test]
fn pairing_exchanges_the_signature_capability_both_ways() {
    let root = folder();
    write_doc(root.path(), "rapor.docx", b"x");
    let (host, ticket, address) = host_at(root.path());
    let client = LanClient::new("client-features", "Telefon");
    let peer = client.pair(&address, ticket.code()).expect("pairing");
    // The client stored what the host announced, and the host stored what the
    // client announced: both lists name the version 2 signature.
    assert_eq!(peer.features, vec![sig::FEATURE_SIG_V2.to_string()]);
    let host_side = host.peers();
    assert_eq!(host_side.len(), 1);
    assert_eq!(host_side[0].features, vec![sig::FEATURE_SIG_V2.to_string()]);
    assert_eq!(host_side[0].min_sig, 0, "pairing announces the capability, it does not pin it");
    // A paired request therefore travels as version 2, and the host pins it.
    assert_eq!(client.list(&peer).expect("list").len(), 1);
    assert_eq!(host.peers()[0].min_sig, 2, "the first accepted version 2 request pins it");
}

/// A request built by hand, the way an older build or an attacker would send
/// it: `version1_only` leaves the version 2 sidecar out entirely.
fn raw_request(peer: &PairedPeer, op: &str, version1_only: bool) -> Request {
    let mut request = Request {
        op: op.to_string(),
        device_id: "client-seeded".into(),
        timestamp: now_unix(),
        nonce: hex_encode(&random_bytes::<16>().expect("nonce")),
        path: "rapor.docx".into(),
        ..Request::default()
    };
    let body_hash = hex_encode(&Sha256::digest(b""));
    if version1_only {
        request.sig_v = 1;
        request.mac =
            sig::sign(&peer.shared_key, &request, &body_hash, &peer.device_id, sig::SigVersion::V1).expect("signature");
    } else {
        super::auth::sign_for(peer, &mut request, &body_hash).expect("signature");
    }
    request
}

/// One request over a fresh pinned connection, answered as the host answered it.
pub(super) fn send(address: &str, fingerprint: &str, request: &Request) -> Response {
    let mut connection = transport::Client::connect(address, Some(fingerprint)).expect("connect");
    connection.request::<Request, Response>(request, None).expect("send").0
}

#[test]
fn a_pair_from_before_version_two_upgrades_itself() {
    let root = folder();
    write_doc(root.path(), "rapor.docx", b"x");
    let (host, address, fingerprint) = seeded_host(root.path(), host_side_peer(&[], 0));
    let client = LanClient::new("client-seeded", "Telefon");
    let peer = client_side_peer(&address, &fingerprint, &[], 0);
    // Neither record knows about version 2, so the client sends version 1 with
    // a version 2 sidecar and the host believes the sidecar.
    assert_eq!(client.list(&peer).expect("list").len(), 1, "an older pair keeps working");
    assert_eq!(host.peers()[0].min_sig, 2, "the first valid version 2 request pins the pair");
}

#[test]
fn the_version_one_half_of_a_dual_request_is_still_readable() {
    // What a receiver from before version 2 does with the same request: it
    // ignores the sidecar and checks the version 1 tag, so mixed builds work.
    let root = folder();
    let (_host, address, fingerprint) = seeded_host(root.path(), host_side_peer(&[], 0));
    let peer = client_side_peer(&address, &fingerprint, &[], 0);
    let request = raw_request(&peer, "ping", false);
    assert_eq!(request.sig_v, 1, "the compatibility shape carries version 1 in the main slot");
    let key = hex_decode(&peer.shared_key).expect("key");
    let body_hash = hex_encode(&Sha256::digest(b""));
    assert!(sig::verify(&key, &request, &body_hash, &peer.device_id, sig::SigVersion::V1));
    assert!(sig::verify_tag(&key, &request, &body_hash, &peer.device_id, sig::SigVersion::V2, &request.sig_v2));
    assert!(!request.sig_v2.is_empty(), "the sidecar carries the range-bound tag");
}

#[test]
fn a_stale_version_one_request_is_refused_once_the_pair_upgraded() {
    let root = folder();
    write_doc(root.path(), "rapor.docx", b"x");
    // The host already pinned the pair; a request without the sidecar is the
    // downgrade the pin exists to stop - whether or not it claims a version.
    let (_host, address, fingerprint) = seeded_host(root.path(), host_side_peer(&[], 2));
    let peer = client_side_peer(&address, &fingerprint, &[], 0);
    let refused = send(&address, &fingerprint, &raw_request(&peer, "list", true));
    assert!(!refused.ok, "a version 1 request is refused");
    assert_eq!(refused.code, "sig-downgrade", "{}", refused.error);
}

#[test]
fn a_pinned_peer_is_accepted_when_it_signs_version_two() {
    let root = folder();
    write_doc(root.path(), "rapor.docx", b"x");
    let (host, address, fingerprint) = seeded_host(root.path(), host_side_peer(&[], 2));
    let client = LanClient::new("client-seeded", "Telefon");
    let pinned = client_side_peer(&address, &fingerprint, &["sig-v2"], 0);
    assert_eq!(client.list(&pinned).expect("list").len(), 1);
    assert_eq!(host.peers()[0].min_sig, 2);
}

#[test]
fn a_broken_sidecar_is_not_rescued_by_the_version_one_tag() {
    let root = folder();
    write_doc(root.path(), "rapor.docx", b"x");
    let (_host, address, fingerprint) = seeded_host(root.path(), host_side_peer(&[], 0));
    let peer = client_side_peer(&address, &fingerprint, &[], 0);
    let mut request = raw_request(&peer, "list", false);
    // A valid version 1 tag next to a broken version 2 one is the tampered-range
    // case: the sidecar is the tag that counts, and it does not fall back.
    request.sig_v2 = "0".repeat(64);
    let refused = send(&address, &fingerprint, &request);
    assert!(!refused.ok, "the request is refused");
    assert_eq!(refused.code, "bad-signature", "{}", refused.error);
}

#[test]
fn a_version_one_only_pair_still_works_against_a_new_host() {
    let root = folder();
    write_doc(root.path(), "rapor.docx", b"x");
    // A device on a build from before version 2 sends no sidecar at all.
    let (host, address, fingerprint) = seeded_host(root.path(), host_side_peer(&[], 0));
    let peer = client_side_peer(&address, &fingerprint, &[], 0);
    let accepted = send(&address, &fingerprint, &raw_request(&peer, "list", true));
    assert!(accepted.ok, "an older build keeps working: {}", accepted.error);
    assert_eq!(accepted.files.len(), 1);
    assert_eq!(host.peers()[0].min_sig, 0, "a version 1 request never upgrades the pair");
}

#[test]
fn an_upgraded_pair_signs_version_two_alone() {
    let root = folder();
    let (_host, address, fingerprint) = seeded_host(root.path(), host_side_peer(&["sig-v2"], 0));
    let peer = client_side_peer(&address, &fingerprint, &["sig-v2"], 0);
    let request = raw_request(&peer, "ping", false);
    assert_eq!(request.sig_v, 2);
    assert!(request.sig_v2.is_empty(), "no sidecar once the pair is on version 2");
    let key = hex_decode(&peer.shared_key).expect("key");
    let body_hash = hex_encode(&Sha256::digest(b""));
    assert!(sig::verify(&key, &request, &body_hash, &peer.device_id, sig::SigVersion::V2));
}
