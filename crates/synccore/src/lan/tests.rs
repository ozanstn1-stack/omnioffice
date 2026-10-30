//! Tests for the LAN sync: identity pinning, pairing, the plan matrix and a
//! real loopback host/client exchange over pinned TLS.

use super::*;
use sha2::{Digest, Sha256};
use std::time::Duration;

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

fn host_at(root: &Path) -> (LanHost, PairingTicket, String) {
    let identity = LanIdentity::generate("host-1", "Atölye PC").expect("identity");
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

#[test]
fn an_identity_pins_the_certificate_it_generated() {
    let identity = LanIdentity::generate("device-7", "Dizüstü").expect("identity");
    let (certificate, key) = identity.material().expect("material");
    assert!(!certificate.is_empty());
    assert!(!key.is_empty());
    assert_eq!(identity.fingerprint, hex_encode(&Sha256::digest(&certificate)));
    assert_eq!(identity.fingerprint.len(), 64);
    // The JSON round trip keeps the secret but never prints it in Debug.
    let json = serde_json::to_string(&identity).expect("json");
    let restored: LanIdentity = serde_json::from_str(&json).expect("restore");
    assert_eq!(restored.fingerprint, identity.fingerprint);
    assert_eq!(restored.material().expect("material"), (certificate, key));
    let debug = format!("{identity:?}");
    assert!(!debug.contains(&identity.private_key));
}

#[test]
fn pairing_codes_are_six_digits_and_only_the_exact_code_matches() {
    let code = PairingCode::generate().expect("code");
    assert_eq!(code.as_str().len(), 6);
    assert!(code.as_str().bytes().all(|byte| byte.is_ascii_digit()));
    assert!(code.matches(code.as_str()));
    assert!(code.matches(&format!(" {} ", code.as_str())));
    assert!(!code.matches("12345"));
    assert!(!code.matches("1234567"));
    let other = if code.as_str() == "000000" { "000001" } else { "000000" };
    assert!(!code.matches(other));
}

#[test]
fn a_ticket_is_one_time_short_lived_and_runs_out_of_attempts() {
    let mut ticket = PairingTicket::new().expect("ticket");
    assert_eq!(ticket.attempts_left(), PAIRING_MAX_ATTEMPTS);
    assert!(!ticket.attempt("999999x", now_unix()));
    assert_eq!(ticket.attempts_left(), PAIRING_MAX_ATTEMPTS - 1);
    let right = ticket.code().to_string();
    assert!(ticket.attempt(&right, now_unix()));

    // Five wrong codes exhaust a fresh ticket, and the right code no longer helps.
    let mut fresh = PairingTicket::new().expect("ticket");
    let wrong = if fresh.code() == "000000" { "000001" } else { "000000" };
    for _ in 0..PAIRING_MAX_ATTEMPTS {
        assert!(!fresh.attempt(wrong, now_unix()));
    }
    assert_eq!(fresh.attempts_left(), 0);
    let fresh_code = fresh.code().to_string();
    assert!(!fresh.attempt(&fresh_code, now_unix()));

    // A ticket created before the TTL window is dead.
    let mut stale = PairingTicket::new().expect("ticket");
    stale.created_at = now_unix().saturating_sub(PAIRING_TTL_SECS + 1);
    assert!(stale.expired(now_unix()));
    let stale_code = stale.code().to_string();
    assert!(!stale.attempt(&stale_code, now_unix()));
}

#[test]
fn the_plan_follows_the_three_way_state_and_never_overwrites_a_conflict() {
    let base = |sha: &str| BaseState { cloud_etag: None, cloud_sha256: sha.to_string(), revision: 1 };
    let local = |path: &str, sha: &str| LocalFile {
        path: path.into(),
        size: sha.len() as u64,
        sha256: sha.into(),
        modified: String::new(),
    };
    let remote = |path: &str, sha: &str| RemoteFile {
        path: path.into(),
        size: sha.len() as u64,
        sha256: sha.into(),
        modified: String::new(),
    };
    let mut bases = HashMap::new();
    bases.insert("ahead.docx".to_string(), base("old"));

    let plan = plan_sync(
        &[
            local("synced.docx", "same"),
            local("local-only.docx", "new"),
            local("ahead.docx", "changed"),
            local("conflict.docx", "left"),
        ],
        &[
            remote("synced.docx", "same"),
            remote("ahead.docx", "old"),
            remote("conflict.docx", "right"),
            remote("remote-only.docx", "cloud"),
        ],
        &bases,
    );
    let item = |path: &str| plan.iter().find(|entry| entry.path == path).expect(path);
    assert_eq!(item("synced.docx").state, SyncState::Synced);
    assert_eq!(item("synced.docx").action, Some(MergeAction::Nothing));
    assert_eq!(item("local-only.docx").state, SyncState::LocalOnly);
    assert_eq!(item("local-only.docx").action, Some(MergeAction::UploadLocal));
    assert_eq!(item("ahead.docx").state, SyncState::LocalAhead);
    assert_eq!(item("ahead.docx").action, Some(MergeAction::UploadLocal));
    assert_eq!(item("remote-only.docx").state, SyncState::CloudAhead);
    assert_eq!(item("remote-only.docx").action, Some(MergeAction::DownloadCloud));
    let conflict = item("conflict.docx");
    assert_eq!(conflict.state, SyncState::Conflict);
    assert_eq!(conflict.action, None);
    assert!(conflict.needs_resolution());
    assert_eq!(
        resolve_action(SyncState::Conflict, Resolution::KeepBoth).expect("plan"),
        MergeAction::DownloadCloudCopy
    );
    assert_eq!(resolve_action(SyncState::Conflict, Resolution::KeepLocal).expect("plan"), MergeAction::UploadLocal);
    assert!(resolve_action(SyncState::LocalOnly, Resolution::KeepCloud).is_err());
}

#[test]
fn only_relative_document_paths_are_accepted() {
    let root = Path::new("/shared");
    assert_eq!(
        resolve_path(root, "sub/rapor.docx").expect("relative document"),
        PathBuf::from("/shared/sub/rapor.docx")
    );
    for rejected in ["", "..", "../secret.docx", "/etc/passwd", "a/../../b.docx", "run.exe", ".hidden/a.docx"] {
        assert!(resolve_path(root, rejected).is_err(), "{rejected:?} must be refused");
    }
}

#[test]
fn a_loopback_host_pairs_lists_and_transfers() {
    let root = folder();
    write_doc(root.path(), "rapor.docx", b"first document");
    let (host, ticket, address) = host_at(root.path());
    let client = LanClient::new("client-1", "Telefon");

    // A wrong code is refused and does not burn the ticket.
    let wrong = if ticket.code() == "000000" { "000001" } else { "000000" };
    assert!(client.pair(&address, wrong).is_err());
    let peer = client.pair(&address, ticket.code()).expect("pairing");
    assert_eq!(peer.fingerprint, host.fingerprint(), "the host certificate is pinned");
    assert_eq!(peer.shared_key.len(), 64);
    // The code is one-time.
    assert!(client.pair(&address, ticket.code()).is_err());

    // List, fetch and upload.
    let files = client.list(&peer).expect("list");
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "rapor.docx");
    let (file, bytes) = client.fetch(&peer, "rapor.docx").expect("fetch");
    assert_eq!(bytes, b"first document");
    assert_eq!(file.sha256, files[0].sha256);
    let remote = client.push(&peer, "alt/yeni.docx", b"second document").expect("push");
    assert_eq!(remote.size, 15);
    assert_eq!(std::fs::read(root.path().join("alt/yeni.docx")).expect("written"), b"second document");
    assert_eq!(client.list(&peer).expect("list").len(), 2);

    // Unpairing revokes the key immediately.
    assert!(host.unpair("client-1"));
    assert!(client.ping(&peer).is_err(), "an unpaired device must be refused");
    host.stop();
}

#[test]
fn a_wrong_pin_cannot_reach_the_host() {
    let root = folder();
    write_doc(root.path(), "rapor.docx", b"x");
    let (host, ticket, address) = host_at(root.path());
    let client = LanClient::new("client-2", "Tablet");
    let mut peer = client.pair(&address, ticket.code()).expect("pairing");
    peer.fingerprint = "00".repeat(32);
    assert!(client.ping(&peer).is_err(), "a different certificate must be refused");
    host.stop();
}

#[test]
fn discovery_replies_round_trip() {
    let reply = DiscoveryReply {
        protocol: 1,
        device_id: "device-3".into(),
        name: "Ofis".into(),
        port: 47820,
        fingerprint: "ab".repeat(32),
    };
    let decoded = DiscoveryReply::decode(&reply.encode()).expect("decode");
    assert_eq!(decoded.device_id, reply.device_id);
    assert_eq!(decoded.port, reply.port);
    assert!(DiscoveryReply::decode(discovery_probe()).is_none());
}

fn discovery_probe() -> &'static [u8] {
    DISCOVERY_PROBE
}

#[test]
fn a_scan_folder_skips_sidecars_and_unshared_types() {
    let root = folder();
    write_doc(root.path(), "rapor.docx", b"document");
    write_doc(root.path(), "muhasebe/defier.xlsx", b"sheet");
    write_doc(root.path(), "notes.md", b"not shared");
    write_doc(root.path(), ".sync/meta.json", b"{}");
    let files = scan_folder(root.path()).expect("scan");
    let paths: Vec<&str> = files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(paths, vec!["muhasebe/defier.xlsx", "rapor.docx"]);
    assert!(files.iter().all(|file| file.size > 0 && file.sha256.len() == 64));
}

#[test]
fn a_timeout_discovery_still_returns_a_list() {
    // A CI sandbox may have no broadcast route at all; discovery must then
    // report a plain error instead of hanging, and a working network returns a
    // (possibly empty) list of well-formed devices.
    let started = std::time::Instant::now();
    match discover(Duration::from_millis(250)) {
        Ok(devices) => assert!(devices.iter().all(|device| !device.device_id.is_empty())),
        Err(error) => assert!(!error.to_string().is_empty()),
    }
    assert!(started.elapsed() < Duration::from_secs(5));
}
