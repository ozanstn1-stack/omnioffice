//! The version 2 signature proves *which* fields the MAC covers: this suite
//! signs one request, changes one field at a time, and expects the host to
//! refuse every version of it. The canonical form is unit-tested in
//! `sig_tests.rs`; here the path is end to end - a real pinned TLS connection,
//! a real host, and the refusal code the caller sees.

use sha2::{Digest, Sha256};

use super::sig_negotiation_tests::{client_side_peer, host_side_peer, seeded_host, send};
use super::tests::{folder, write_doc};
use super::*;

/// A document with enough bytes that every range field means something.
const BODY: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";

fn digest_of(bytes: &[u8]) -> String {
    hex_encode(&Sha256::digest(bytes))
}

/// A peer that already pinned version 2, so the host checks the range-bound
/// form and nothing else.
fn pinned_peer(address: &str, fingerprint: &str) -> PairedPeer {
    client_side_peer(address, fingerprint, &["sig-v2"], 0)
}

/// One `get_chunk` request, signed the way `LanClient` signs it.
fn signed(peer: &PairedPeer, version: sig::SigVersion) -> Request {
    let mut request = Request {
        op: "get_chunk".into(),
        device_id: "client-seeded".into(),
        timestamp: now_unix(),
        nonce: hex_encode(&random_bytes::<16>().expect("nonce")),
        path: "rapor.docx".into(),
        offset: 0,
        body_len: 0,
        total: BODY.len() as u64,
        sha256: digest_of(BODY),
        ..Request::default()
    };
    request.sig_v = version.to_wire();
    let body_hash = digest_of(b"");
    request.mac = sig::sign(&peer.shared_key, &request, &body_hash, &peer.device_id, version).expect("signature");
    request
}

/// The host's answer to one request over its own pinned connection.
fn answer(address: &str, fingerprint: &str, request: &Request) -> Response {
    send(address, fingerprint, request)
}

fn refused_with_code(response: &Response, code: &str, what: &str) {
    assert!(!response.ok, "{what}: the request was accepted");
    assert_eq!(response.code, code, "{what}: {} ", response.error);
}

#[test]
fn a_changed_range_field_breaks_the_signature() {
    let root = folder();
    write_doc(root.path(), "rapor.docx", BODY);
    let (_host, address, fingerprint) = seeded_host(root.path(), host_side_peer(&["sig-v2"], 0));
    let peer = pinned_peer(&address, &fingerprint);
    let good = signed(&peer, sig::SigVersion::V2);
    assert!(answer(&address, &fingerprint, &good).ok, "the signed request itself is accepted");

    // Each mutation is one field of the canonical form: offset, total, the
    // document hash and the operation. The host refuses on the MAC, before it
    // ever looks at the range.
    let mutations: [(&str, fn(&mut Request)); 4] = [
        ("offset", |request| request.offset += 1),
        ("total", |request| request.total += 1),
        ("sha256", |request| request.sha256 = "0".repeat(63) + "1"),
        ("op", |request| request.op = "get".into()),
    ];
    for (what, mutate) in mutations {
        let mut request = good.clone();
        mutate(&mut request);
        refused_with_code(&answer(&address, &fingerprint, &request), "bad-signature", what);
    }

    // The path is bound through the derived transfer id as well as literally.
    let mut other_path = good.clone();
    other_path.path = "diger.docx".into();
    refused_with_code(&answer(&address, &fingerprint, &other_path), "bad-signature", "path");
}

#[test]
fn a_changed_identity_or_nonce_breaks_the_signature() {
    let root = folder();
    write_doc(root.path(), "rapor.docx", BODY);
    let (_host, address, fingerprint) = seeded_host(root.path(), host_side_peer(&["sig-v2"], 0));
    let peer = pinned_peer(&address, &fingerprint);
    let good = signed(&peer, sig::SigVersion::V2);

    // A tag made for another receiver: the host's own device id is in the
    // message, so a request signed for a different device does not verify even
    // with the right key.
    let mut elsewhere = good.clone();
    elsewhere.mac = sig::sign(&peer.shared_key, &elsewhere, &digest_of(b""), "host-elsewhere", sig::SigVersion::V2)
        .expect("signature");
    refused_with_code(&answer(&address, &fingerprint, &elsewhere), "bad-signature", "receiver");

    // A stale nonce swap and a clock that moved are both covered fields.
    let mut other_nonce = good.clone();
    other_nonce.nonce = "f".repeat(32);
    refused_with_code(&answer(&address, &fingerprint, &other_nonce), "bad-signature", "nonce");

    let mut moved = good.clone();
    moved.timestamp += 30;
    refused_with_code(&answer(&address, &fingerprint, &moved), "bad-signature", "timestamp");

    // An unknown sender has no key at all, so it is refused before the MAC.
    let mut stranger = good;
    stranger.device_id = "someone-else".into();
    let refused = answer(&address, &fingerprint, &stranger);
    assert!(!refused.ok, "an unpaired sender is refused");
    assert!(refused.error.contains("not paired"), "{}", refused.error);
}

#[test]
fn a_replay_of_a_valid_request_is_refused() {
    let root = folder();
    write_doc(root.path(), "rapor.docx", BODY);
    let (_host, address, fingerprint) = seeded_host(root.path(), host_side_peer(&["sig-v2"], 0));
    let peer = pinned_peer(&address, &fingerprint);
    let request = signed(&peer, sig::SigVersion::V2);
    assert!(answer(&address, &fingerprint, &request).ok, "first delivery");
    let replay = answer(&address, &fingerprint, &request);
    assert!(!replay.ok, "the same request is not accepted twice");
    assert!(replay.error.contains("already used"), "{}", replay.error);
}

#[test]
fn a_malformed_sidecar_is_refused_in_every_shape() {
    let root = folder();
    write_doc(root.path(), "rapor.docx", BODY);
    let (_host, address, fingerprint) = seeded_host(root.path(), host_side_peer(&[], 0));
    // The pair is not upgraded, so the client sends version 1 plus a sidecar.
    let peer = client_side_peer(&address, &fingerprint, &[], 0);
    let base = {
        let mut request = signed(&peer, sig::SigVersion::V1);
        request.sig_v2 = sig::sign(&peer.shared_key, &request, &digest_of(b""), &peer.device_id, sig::SigVersion::V2)
            .expect("signature");
        request
    };
    assert!(answer(&address, &fingerprint, &base).ok, "the dual request itself is accepted");

    let shapes: [(&str, String); 5] = [
        ("not hex", "z".repeat(64)),
        ("too short", "ab".repeat(16)),
        ("too long", "ab".repeat(48)),
        ("all zero", "0".repeat(64)),
        ("blank", " ".to_string()),
    ];
    for (what, sidecar) in shapes {
        let mut request = base.clone();
        request.nonce = hex_encode(&random_bytes::<16>().expect("nonce"));
        request.timestamp = now_unix();
        // Re-sign the version 1 half so only the sidecar is wrong.
        request.mac = sig::sign(&peer.shared_key, &request, &digest_of(b""), &peer.device_id, sig::SigVersion::V1)
            .expect("signature");
        request.sig_v2 = sidecar;
        // Even a blank sidecar counts as one: it is not the empty "absent"
        // state the version 1 tag stands on, so the request is refused rather
        // than quietly falling back.
        refused_with_code(&answer(&address, &fingerprint, &request), "bad-signature", what);
    }
}

#[test]
fn a_sidecar_signed_with_another_key_is_refused() {
    let root = folder();
    write_doc(root.path(), "rapor.docx", BODY);
    let (_host, address, fingerprint) = seeded_host(root.path(), host_side_peer(&[], 0));
    let peer = client_side_peer(&address, &fingerprint, &[], 0);
    let mut request = signed(&peer, sig::SigVersion::V1);
    request.sig_v2 = sig::sign(&"cd".repeat(32), &request, &digest_of(b""), &peer.device_id, sig::SigVersion::V2)
        .expect("signature");
    refused_with_code(&answer(&address, &fingerprint, &request), "bad-signature", "foreign key");
}

#[test]
fn the_version_two_shape_is_refused_when_the_record_demands_it_and_it_is_wrong() {
    let root = folder();
    write_doc(root.path(), "rapor.docx", BODY);
    // The peer is pinned, so a version 1 request is a downgrade, and a version 2
    // request with a changed field is a forgery: both are refused, with
    // different codes, so the caller can tell them apart.
    let (_host, address, fingerprint) = seeded_host(root.path(), host_side_peer(&[], 2));
    let peer = pinned_peer(&address, &fingerprint);

    let mut downgraded = signed(&peer, sig::SigVersion::V2);
    downgraded.sig_v = 1;
    refused_with_code(&answer(&address, &fingerprint, &downgraded), "sig-downgrade", "downgrade");

    let mut forged = signed(&peer, sig::SigVersion::V2);
    forged.offset = forged.total;
    refused_with_code(&answer(&address, &fingerprint, &forged), "bad-signature", "forgery");
}

#[test]
fn a_refused_request_changes_nothing_on_the_host() {
    let root = folder();
    write_doc(root.path(), "rapor.docx", BODY);
    let (host, address, fingerprint) = seeded_host(root.path(), host_side_peer(&[], 0));
    let peer = client_side_peer(&address, &fingerprint, &[], 0);

    // A dual request whose sidecar does not verify must not upgrade the pair,
    // even though the version 1 half is valid.
    let mut request = signed(&peer, sig::SigVersion::V1);
    request.sig_v2 = "0".repeat(64);
    refused_with_code(&answer(&address, &fingerprint, &request), "bad-signature", "broken sidecar");
    assert_eq!(host.peers()[0].min_sig, 0, "a refused request never pins the peer");

    // A valid one does, and the pin is the only thing that changes.
    let mut accepted = signed(&peer, sig::SigVersion::V1);
    accepted.sig_v2 = sig::sign(&peer.shared_key, &accepted, &digest_of(b""), &peer.device_id, sig::SigVersion::V2)
        .expect("signature");
    assert!(answer(&address, &fingerprint, &accepted).ok);
    assert_eq!(host.peers()[0].min_sig, 2);
    assert_eq!(host.peers()[0].shared_key, peer.shared_key, "the key is untouched");
}
