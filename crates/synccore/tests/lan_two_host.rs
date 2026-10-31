//! Two-host LAN acceptance contract (D1).
//!
//! Two real `LanHost` listeners, two folders, two identities: the whole flow the
//! manual acceptance guide (`docs/lan-sync-acceptance.md`) walks through on a
//! real Wi-Fi, but on loopback so it runs in CI before the device pass:
//!
//! pair with the printed code -> list -> download -> upload -> symmetric call
//! back -> three-way conflict -> confirmed delete -> unpair refusal.
//!
//! The mDNS/UDP discovery rows stay manual (a CI sandbox has no multicast), and
//! so does the Android foreground service.

use std::collections::HashMap;
use std::net::TcpListener;
use std::path::Path;

use synccore::lan::{
    plan_deletions, plan_sync, scan_folder, DeletionSide, HostOptions, LanClient, LanHost, LanIdentity, PairedPeer,
    PairingTicket,
};
use synccore::metadata::{self, BaseState, SyncState};

fn folder() -> tempfile::TempDir {
    tempfile::tempdir().expect("temp dir")
}

fn write_doc(root: &Path, name: &str, body: &[u8]) {
    let path = root.join(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("parent");
    }
    std::fs::write(&path, body).expect("write document");
}

fn sha_of(root: &Path, name: &str) -> String {
    metadata::hash_file(&root.join(name)).expect("hash").0
}

/// A port the OS hands out and immediately frees; the host binds it a moment
/// later. The tiny race is acceptable in a test and keeps the announced port
/// equal to the one the host actually listens on.
fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("probe listener");
    listener.local_addr().expect("probe address").port()
}

fn peer_for(identity: &LanIdentity, address: &str, shared_key: &str, host_side: bool) -> PairedPeer {
    PairedPeer {
        device_id: identity.device_id.clone(),
        name: identity.name.clone(),
        address: address.to_string(),
        fingerprint: identity.fingerprint.clone(),
        shared_key: shared_key.to_string(),
        paired_at: "2026-10-31T00:00:00Z".to_string(),
        host_side,
    }
}

fn base_of(sha: &str) -> BaseState {
    BaseState { cloud_etag: None, cloud_sha256: sha.to_string(), revision: 1 }
}

#[test]
fn two_hosts_pair_transfer_conflict_delete_and_revoke_over_loopback() {
    let root_a = folder();
    let root_b = folder();
    write_doc(root_a.path(), "ortak.docx", b"version one");
    write_doc(root_a.path(), "alt/örnek belge.pdf", b"pdf bytes");

    let identity_a = LanIdentity::generate("device-a", "Atölye PC").expect("identity a");
    let identity_b = LanIdentity::generate("device-b", "Telefon").expect("identity b");
    let ticket = PairingTicket::new().expect("ticket");

    // Host A shares; host B shares too (started after pairing so its peer list
    // holds the key A will need for the symmetric call).
    let host_a = LanHost::start(HostOptions {
        identity: identity_a.clone(),
        root: root_a.path().to_path_buf(),
        port: 0,
        pairing: Some(ticket.clone()),
        peers: Vec::new(),
    })
    .expect("host a");
    let address_a = format!("127.0.0.1:{}", host_a.port());
    let port_b = free_port();

    // Phone (B) pairs with A, announcing its own certificate and future port.
    let client_b = LanClient::new(identity_b.device_id.clone(), identity_b.name.clone())
        .with_identity(identity_b.fingerprint.clone(), port_b);
    let peer_b_of_a = client_b.pair(&address_a, ticket.code()).expect("pairing");
    assert_eq!(peer_b_of_a.fingerprint, identity_a.fingerprint);

    // A's host-side record for B carries B's pin and announced port, and B's
    // host records the same key for A: both directions can call now.
    let peers_of_a = host_a.peers();
    assert_eq!(peers_of_a.len(), 1);
    let peer_a_of_b = peer_for(&identity_a, &address_a, &peer_b_of_a.shared_key, true);
    let host_b = LanHost::start(HostOptions {
        identity: identity_b.clone(),
        root: root_b.path().to_path_buf(),
        port: port_b,
        pairing: None,
        peers: vec![peer_a_of_b],
    })
    .expect("host b");

    // Discovery rows are manual; the listing is the contract here.
    let remote = client_b.list(&peer_b_of_a).expect("list");
    let names: Vec<&str> = remote.iter().map(|file| file.path.as_str()).collect();
    assert!(names.contains(&"ortak.docx"));
    assert!(names.contains(&"alt/örnek belge.pdf"), "unicode path: {names:?}");

    // Download into B's folder, then upload a new document to A.
    let (remote_file, bytes) = client_b.fetch(&peer_b_of_a, "ortak.docx").expect("fetch");
    write_doc(root_b.path(), "ortak.docx", &bytes);
    assert_eq!(sha_of(root_b.path(), "ortak.docx"), remote_file.sha256);
    client_b.push(&peer_b_of_a, "telefon.docx", b"phone note").expect("push");
    assert_eq!(std::fs::read(root_a.path().join("telefon.docx")).expect("pushed"), b"phone note");

    // Symmetric call: A can list B's shared folder as well.
    let client_a = LanClient::new(identity_a.device_id.clone(), identity_a.name.clone())
        .with_identity(identity_a.fingerprint.clone(), host_a.port());
    let listing_b = client_a.list(&peers_of_a[0]).expect("symmetric list");
    assert!(listing_b.iter().any(|file| file.path == "ortak.docx"), "A reads B's folder");

    // Three-way conflict: both sides edited the same file since the common base.
    write_doc(root_a.path(), "ortak.docx", b"version two (PC)");
    write_doc(root_b.path(), "ortak.docx", b"version two (phone)");
    let local_b = scan_folder(root_b.path()).expect("scan local");
    let remote_a = client_b.list(&peer_b_of_a).expect("list after edits");
    let mut bases = HashMap::new();
    bases.insert("ortak.docx".to_string(), base_of(&remote_file.sha256));
    let plan = plan_sync(&local_b, &remote_a, &bases);
    let conflict = plan.iter().find(|item| item.path == "ortak.docx").expect("plan item");
    assert_eq!(conflict.state, SyncState::Conflict);
    assert_eq!(conflict.action, None, "a conflict is never resolved automatically");
    // Keep both: the peer copy is fetched and written next to the local file.
    let (_, peer_copy) = client_b.fetch(&peer_b_of_a, "ortak.docx").expect("peer copy");
    write_doc(root_b.path(), "ortak (peer copy).docx", &peer_copy);
    assert!(root_b.path().join("ortak (peer copy).docx").exists());

    // Confirmed deletion: B had downloaded the nested document, removes its own
    // copy and confirms "delete there".
    let (nested_remote, nested_bytes) = client_b.fetch(&peer_b_of_a, "alt/örnek belge.pdf").expect("fetch nested");
    write_doc(root_b.path(), "alt/örnek belge.pdf", &nested_bytes);
    std::fs::remove_file(root_b.path().join("alt/örnek belge.pdf")).expect("local delete");
    let remote_after = client_b.list(&peer_b_of_a).expect("list before delete");
    let local_after = scan_folder(root_b.path()).expect("scan before delete");
    let mut deletion_bases = HashMap::new();
    deletion_bases.insert("alt/örnek belge.pdf".to_string(), base_of(&nested_remote.sha256));
    let deletions = plan_deletions(&local_after, &remote_after, &deletion_bases);
    assert_eq!(deletions.len(), 1);
    assert_eq!(deletions[0].side, DeletionSide::Local);
    client_b.remove(&peer_b_of_a, "alt/örnek belge.pdf").expect("confirmed delete");
    assert!(!root_a.path().join("alt/örnek belge.pdf").exists(), "peer copy removed");

    // Unpair revokes the key immediately.
    assert!(host_a.unpair(&identity_b.device_id));
    assert!(client_b.list(&peer_b_of_a).is_err(), "an unpaired device must be refused");

    host_a.stop();
    host_b.stop();
}
