//! Tests for the Tauri-side LAN helpers. The network itself is covered by the
//! `synccore::lan` loopback test; here the store, the folder validation, the
//! plan mapping and the refusal paths are checked without a socket.

use super::*;
use std::collections::HashMap;

fn peer(fingerprint: &str, shared_key: &str) -> PairedPeer {
    PairedPeer {
        device_id: "peer-1".into(),
        name: "Telefon".into(),
        address: "192.168.1.20:47820".into(),
        fingerprint: fingerprint.to_string(),
        shared_key: shared_key.to_string(),
        paired_at: "2026-10-30T12:00:00Z".into(),
        host_side: fingerprint.is_empty(),
    }
}

#[test]
fn the_folder_must_exist_and_is_canonicalised() {
    let dir = tempfile::tempdir().unwrap();
    let resolved = folder_path(&dir.path().to_string_lossy()).expect("folder");
    assert!(resolved.is_dir());
    assert!(folder_path("/definitely/not/here").is_err());
}

#[test]
fn a_host_side_peer_cannot_be_used_as_a_sync_target() {
    let store = LanStore { peers: vec![peer("", "ab")], ..LanStore::default() };
    assert!(client_peer(&store, "peer-1").is_err(), "no pin means no client use");
    let mut store = LanStore { peers: vec![peer(&"ab".repeat(32), &"cd".repeat(32))], ..LanStore::default() };
    assert!(client_peer(&store, "peer-1").is_ok());
    store.peers.clear();
    assert!(client_peer(&store, "peer-1").is_err());
}

#[test]
fn the_plan_maps_states_and_actions_for_the_view() {
    let item = PlanItem {
        path: "rapor.docx".into(),
        state: synccore::metadata::SyncState::Conflict,
        action: None,
        local_sha256: Some("aa".into()),
        remote_sha256: Some("bb".into()),
    };
    assert_eq!(state_name(&item), "conflict");
    assert!(item.action.is_none());
    let synced = PlanItem { state: synccore::metadata::SyncState::Synced, action: Some(MergeAction::Nothing), ..item };
    assert_eq!(state_name(&synced), "synced");
    assert_eq!(synced.action.map(action_name), Some("nothing"));
    assert_eq!(action_name(MergeAction::UploadLocal), "upload");
    assert_eq!(action_name(MergeAction::DownloadCloud), "download");
    assert_eq!(action_name(MergeAction::DownloadCloudCopy), "download_copy");
}

#[test]
fn stored_hashes_become_the_three_way_base() {
    let mut store = LanStore::default();
    store.bases.entry("peer-1".into()).or_default().insert("rapor.docx".into(), "abc".into());
    let bases = bases_for(&store, "peer-1");
    assert_eq!(bases.get("rapor.docx").expect("base").cloud_sha256, "abc");
    assert!(bases_for(&store, "peer-2").is_empty());
}

#[test]
fn peer_views_tell_the_two_roles_apart() {
    let host_side = peer_view(&peer("", "ab"));
    assert!(host_side.host_side);
    let client_side = peer_view(&peer(&"ab".repeat(32), &"cd".repeat(32)));
    assert!(!client_side.host_side);
    assert_eq!(client_side.name, "Telefon");
}

#[test]
fn a_sync_error_maps_to_a_pdf_error_the_ui_can_report() {
    let invalid = sync_error(synccore::SyncError::InvalidInput("bad".into()));
    assert!(matches!(invalid, PdfError::InvalidInput(_)));
    let auth = sync_error(synccore::SyncError::Auth("no".into()));
    assert!(matches!(auth, PdfError::PermissionDenied(_)));
    let network = sync_error(synccore::SyncError::Network("down".into()));
    assert!(matches!(network, PdfError::ProcessingFailed(_)));
    let _: HashMap<String, String> = HashMap::new();
}
