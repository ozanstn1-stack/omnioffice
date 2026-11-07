//! Tests for the request signature versions over a real loopback host: what
//! pairing records on both sides, and the rule that a peer which once signed
//! with version 2 cannot go back to version 1.

use super::tests::{folder, host_at, write_doc};
use super::*;

/// A host that already holds one peer, so the version rules are checked without
/// a pairing handshake in the way. Returns the address and the fingerprint the
/// client has to pin.
fn seeded_host(root: &Path, peer: PairedPeer) -> (LanHost, String, String) {
    let identity = LanIdentity::generate("host-seeded", "Atölye PC").expect("identity");
    let fingerprint = identity.fingerprint.clone();
    let host =
        LanHost::start(HostOptions { identity, root: root.to_path_buf(), port: 0, pairing: None, peers: vec![peer] })
            .expect("host");
    let address = format!("127.0.0.1:{}", host.port());
    (host, address, fingerprint)
}

/// The host's record of the client: what the client announced while pairing.
fn host_side_peer(features: &[&str], min_sig: u8) -> PairedPeer {
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
fn client_side_peer(address: &str, fingerprint: &str, features: &[&str], min_sig: u8) -> PairedPeer {
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

#[test]
fn two_records_without_the_capability_keep_signing_version_one() {
    let root = folder();
    write_doc(root.path(), "rapor.docx", b"x");
    let (host, address, fingerprint) = seeded_host(root.path(), host_side_peer(&[], 0));
    let client = LanClient::new("client-seeded", "Telefon");
    let peer = client_side_peer(&address, &fingerprint, &[], 0);
    assert_eq!(client.list(&peer).expect("list").len(), 1, "an older pair keeps working");
    assert_eq!(host.peers()[0].min_sig, 0, "nothing is pinned without a version 2 request");
}

#[test]
fn a_peer_that_negotiated_version_two_cannot_fall_back_to_version_one() {
    let root = folder();
    write_doc(root.path(), "rapor.docx", b"x");
    // The host recorded the client's capability; the client's own record lost
    // it (an older store file), which is exactly the downgrade to refuse.
    let (_host, address, fingerprint) = seeded_host(root.path(), host_side_peer(&["sig-v2"], 0));
    let client = LanClient::new("client-seeded", "Telefon");
    let peer = client_side_peer(&address, &fingerprint, &[], 0);
    let error = client.ping(&peer).expect_err("a downgrade is refused");
    assert!(matches!(error, SyncError::SigDowngrade), "got {error:?}");
}

#[test]
fn a_pinned_peer_is_held_to_version_two_without_the_feature_list() {
    let root = folder();
    write_doc(root.path(), "rapor.docx", b"x");
    // Only the pin is left (`min_sig: 2`), no feature list: the host must still
    // demand version 2, and a client that speaks it is accepted.
    let (host, address, fingerprint) = seeded_host(root.path(), host_side_peer(&[], 2));
    let client = LanClient::new("client-seeded", "Telefon");
    let pinned = client_side_peer(&address, &fingerprint, &["sig-v2"], 0);
    assert_eq!(client.list(&pinned).expect("list").len(), 1);
    assert_eq!(host.peers()[0].min_sig, 2);
    // And a version 1 client behind the same pin is refused.
    let stale = client_side_peer(&address, &fingerprint, &[], 0);
    let error = client.ping(&stale).expect_err("a version 1 request is refused");
    assert!(matches!(error, SyncError::SigDowngrade), "got {error:?}");
}
