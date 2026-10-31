//! Same-network document sync commands (C14).
//!
//! The crypto, the protocol and the sync plan live in [`synccore::lan`]; this
//! module is the Tauri surface: the sealed store (identity, paired peers, last
//! common hashes), the running host, the discovery/pair/sync commands and the
//! execution of a merge action against the local folder.
//!
//! Two roles exist. A **host** shares one folder and shows a six-digit code; a
//! **client** pairs with it and can list, download and upload documents. A
//! pairing is one-directional: the same device id cannot be both roles at
//! once (the roadmap's follow-up is symmetric pairing). See
//! `docs/lan-sync.md` for the threat model and what is deliberately absent
//! (background service, delete propagation, mDNS).

mod store;
#[cfg(test)]
mod tests;
mod trash;

use pdfcore::error::PdfError;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use synccore::lan::{self, LanClient, LanDevice, LanHost, PairedPeer, PlanItem};
use synccore::merge::{self, MergeAction, Resolution};
use synccore::metadata::BaseState;
use synccore::SyncError;
use tauri::{AppHandle, Manager};

use store::LanStore;

/// The running host (one per app process) and the folder it shares.
static HOST: Mutex<Option<(LanHost, String)>> = Mutex::new(None);

#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanPeerView {
    pub device_id: String,
    pub name: String,
    pub address: String,
    /// Pinned certificate on the client side; empty on the host side.
    pub fingerprint: String,
    pub paired_at: String,
    /// True when this device is the host of the pairing.
    pub host_side: bool,
}

#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanHostView {
    pub port: u16,
    pub folder: String,
    /// The live pairing code, when the window is open.
    pub code: Option<String>,
    pub expires_in_secs: u64,
    pub attempts_left: u8,
}

#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanStatusView {
    pub identity: LanIdentityView,
    /// False when the platform has no cipher and the store is `plain:`.
    pub encrypted: bool,
    pub host: Option<LanHostView>,
    pub peers: Vec<LanPeerView>,
}

#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanIdentityView {
    pub device_id: String,
    pub name: String,
    pub fingerprint: String,
}

#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanDeviceView {
    pub device_id: String,
    pub name: String,
    pub address: String,
    pub fingerprint: String,
    pub paired: bool,
}

#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanPlanItemView {
    pub path: String,
    /// `synced`, `local_only`, `local_ahead`, `cloud_ahead` or `conflict`.
    pub state: String,
    /// The action that needs no question; `None` for a conflict.
    pub action: Option<String>,
    pub local_sha256: Option<String>,
    pub remote_sha256: Option<String>,
}

#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanPlanView {
    pub device_id: String,
    pub folder: String,
    pub items: Vec<LanPlanItemView>,
    /// Files one side removed since the last common state; each needs a
    /// confirmation before anything is touched.
    pub deletions: Vec<LanDeletionView>,
}

#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanDeletionView {
    pub path: String,
    /// `local` (removed here) or `remote` (removed on the peer).
    pub side: String,
}

/// What the user confirmed for one deletion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeletionAction {
    /// Delete the peer's copy (removed here).
    DeleteRemote,
    /// Download the peer's copy back (removed here).
    RestoreLocal,
    /// Delete the local file (removed on the peer).
    DeleteLocal,
    /// Upload the local file again (removed on the peer).
    RestoreRemote,
}

impl DeletionAction {
    fn parse(side: &str, resolution: &str) -> Result<Self, PdfError> {
        match (side, resolution) {
            ("local", "delete_remote") => Ok(Self::DeleteRemote),
            ("local", "restore_local") => Ok(Self::RestoreLocal),
            ("remote", "delete_local") => Ok(Self::DeleteLocal),
            ("remote", "restore_remote") => Ok(Self::RestoreRemote),
            _ => Err(PdfError::InvalidInput("unknown deletion resolution".into())),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::DeleteRemote => "delete_remote",
            Self::RestoreLocal => "restore_local",
            Self::DeleteLocal => "delete_local",
            Self::RestoreRemote => "restore_remote",
        }
    }
}

/// One document in the local trash ("Recently deleted", 30 days).
#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanTrashEntryView {
    pub id: String,
    pub path: String,
    pub peer_device_id: String,
    pub peer_name: String,
    /// `local` (removed here) or `remote` (removed on the peer).
    pub side: String,
    pub sha256: String,
    #[cfg_attr(feature = "bindings", specta(type = specta_typescript::Number))]
    pub size: u64,
    #[cfg_attr(feature = "bindings", specta(type = specta_typescript::Number))]
    pub deleted_at: u64,
}

fn trash_view(entry: trash::TrashEntry) -> LanTrashEntryView {
    LanTrashEntryView {
        id: entry.id,
        path: entry.path,
        peer_device_id: entry.peer_device_id,
        peer_name: entry.peer_name,
        side: entry.side,
        sha256: entry.sha256,
        size: entry.size,
        deleted_at: entry.deleted_at,
    }
}

#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanDeletionResultView {
    pub path: String,
    pub applied: bool,
    pub action: String,
}

#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanApplyView {
    pub path: String,
    pub applied: bool,
    pub action: String,
    pub warning: Option<String>,
}

// ---------------------------------------------------------------------------
// Store helpers
// ---------------------------------------------------------------------------

fn config_dir(app: &AppHandle) -> Result<PathBuf, PdfError> {
    app.path().app_config_dir().map_err(|error| PdfError::Internal(format!("config dir unavailable: {error}")))
}

fn sync_error(error: SyncError) -> PdfError {
    match error {
        SyncError::InvalidInput(message) => PdfError::InvalidInput(message),
        SyncError::Auth(message) => PdfError::PermissionDenied(message),
        SyncError::Network(message) | SyncError::Protocol(message) => PdfError::ProcessingFailed(message),
        SyncError::TooLarge(limit) => PdfError::InvalidInput(format!("the document is larger than {limit} bytes")),
        SyncError::NotFound(name) => PdfError::NotFound(name),
        other => PdfError::Internal(other.to_string()),
    }
}

fn peer_view(peer: &PairedPeer) -> LanPeerView {
    LanPeerView {
        device_id: peer.device_id.clone(),
        name: peer.name.clone(),
        address: peer.address.clone(),
        fingerprint: peer.fingerprint.clone(),
        paired_at: peer.paired_at.clone(),
        host_side: peer.host_side || peer.fingerprint.is_empty(),
    }
}

fn status_of(app: &AppHandle) -> Result<LanStatusView, PdfError> {
    let directory = config_dir(app)?;
    let identity = store::identity(&directory)?;
    let mut store = store::load(&directory)?;
    let mut peers: Vec<LanPeerView> = store.peers.iter().map(peer_view).collect();
    // Copy what the status needs out of the host before the lock is released.
    let host_data = HOST.lock().ok().and_then(|guard| {
        guard.as_ref().map(|(host, folder)| {
            (host.port(), host.pairing_code(), host.pairing_attempts_left(), host.peers(), folder.clone())
        })
    });
    let host_view = host_data.as_ref().map(|(port, code, attempts_left, _peers, folder)| LanHostView {
        port: *port,
        folder: folder.clone(),
        code: code.clone(),
        expires_in_secs: if code.is_some() { lan::PAIRING_TTL_SECS } else { 0 },
        attempts_left: *attempts_left,
    });
    if let Some((_, _, _, host_peers, _)) = host_data.as_ref() {
        // A pairing that happened since the last call is persisted with it.
        let mut changed = false;
        for peer in host_peers {
            match store.peers.iter_mut().find(|known| known.device_id == peer.device_id) {
                Some(known) => *known = peer.clone(),
                None => {
                    store.peers.push(peer.clone());
                    changed = true;
                }
            }
        }
        if changed {
            store::save(&directory, &store)?;
            peers = store.peers.iter().map(peer_view).collect();
        }
    }
    Ok(LanStatusView {
        identity: LanIdentityView {
            device_id: identity.device_id,
            name: identity.name,
            fingerprint: identity.fingerprint,
        },
        encrypted: crate::secret::platform_cipher().is_some(),
        host: host_view,
        peers,
    })
}

fn client_peer(store: &LanStore, device_id: &str) -> Result<PairedPeer, PdfError> {
    store
        .peers
        .iter()
        .find(|peer| peer.device_id == device_id && !peer.fingerprint.is_empty() && !peer.shared_key.is_empty())
        .cloned()
        .ok_or_else(|| PdfError::InvalidInput("this device is not paired with that peer".into()))
}

/// A folder the webview picked: a capability token resolved here (or a raw
/// path in debug builds), never a path the UI invented.
fn folder_path(folder: &str) -> Result<PathBuf, PdfError> {
    let validated = crate::paths::directory(folder)?;
    validated.as_path().canonicalize().map_err(PdfError::from_io)
}

fn bases_for(store: &LanStore, device_id: &str) -> HashMap<String, BaseState> {
    store
        .bases
        .get(device_id)
        .map(|entries| {
            entries
                .iter()
                .map(|(path, sha256)| {
                    (path.clone(), BaseState { cloud_etag: None, cloud_sha256: sha256.clone(), revision: 1 })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn state_name(item: &PlanItem) -> String {
    serde_json::to_value(item.state)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

fn action_name(action: MergeAction) -> &'static str {
    match action {
        MergeAction::Nothing => "nothing",
        MergeAction::UploadLocal => "upload",
        MergeAction::DownloadCloud => "download",
        MergeAction::DownloadCloudCopy => "download_copy",
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn lan_status(app: AppHandle) -> Result<LanStatusView, PdfError> {
    status_of(&app)
}

/// Renames this device on the network (the certificate stays the same).
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn lan_rename(app: AppHandle, name: String) -> Result<LanStatusView, PdfError> {
    store::rename(&config_dir(&app)?, &name)?;
    status_of(&app)
}

/// Shares `folder` and opens a fresh pairing window.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn lan_host_start(app: AppHandle, folder: String) -> Result<LanStatusView, PdfError> {
    let root = folder_path(&folder)?;
    let directory = config_dir(&app)?;
    let identity = store::identity(&directory)?;
    let store = store::load(&directory)?;
    let ticket = lan::PairingTicket::new().map_err(sync_error)?;
    let host = LanHost::start(lan::HostOptions {
        identity,
        root,
        port: lan::DEFAULT_PORT,
        pairing: Some(ticket),
        peers: store.peers.clone(),
    })
    .map_err(sync_error)?;
    if let Ok(mut guard) = HOST.lock() {
        if let Some((previous, _)) = guard.replace((host, folder)) {
            previous.stop();
        }
    }
    // Android keeps the process (and the mDNS multicast lock) alive through a
    // foreground service while a folder is shared; a no-op elsewhere.
    crate::android_lan::set_active(true);
    status_of(&app)
}

/// Opens a new pairing window on the running host (a new code).
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn lan_host_open_pairing(app: AppHandle) -> Result<LanStatusView, PdfError> {
    if let Ok(guard) = HOST.lock() {
        if let Some((host, _)) = guard.as_ref() {
            host.open_pairing().map_err(sync_error)?;
        }
    }
    status_of(&app)
}

/// Stops sharing.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn lan_host_stop(app: AppHandle) -> Result<LanStatusView, PdfError> {
    if let Ok(mut guard) = HOST.lock() {
        if let Some((host, _)) = guard.take() {
            host.stop();
        }
    }
    crate::android_lan::set_active(false);
    status_of(&app)
}

/// Revokes a host-side pairing.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn lan_host_unpair(app: AppHandle, device_id: String) -> Result<LanStatusView, PdfError> {
    {
        let guard = HOST.lock().map_err(|_| PdfError::Internal("the LAN host is unavailable".into()))?;
        if let Some((host, _)) = guard.as_ref() {
            host.unpair(&device_id);
        }
    }
    let directory = config_dir(&app)?;
    let mut store = store::load(&directory)?;
    store.peers.retain(|peer| peer.device_id != device_id);
    store.bases.remove(&device_id);
    store::save(&directory, &store)?;
    status_of(&app)
}

/// Broadcasts a discovery probe and returns the hosts that answered.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn lan_discover(app: AppHandle, timeout_ms: u32) -> Result<Vec<LanDeviceView>, PdfError> {
    let timeout = std::time::Duration::from_millis(u64::from(timeout_ms.clamp(200, 5_000)));
    let directory = config_dir(&app)?;
    let paired: Vec<String> = store::load(&directory)?.peers.iter().map(|peer| peer.device_id.clone()).collect();
    let devices: Vec<LanDevice> = tauri::async_runtime::spawn_blocking(move || lan::discover(timeout))
        .await
        .map_err(|error| PdfError::Internal(format!("worker thread failed: {error}")))?
        .map_err(sync_error)?;
    Ok(devices
        .into_iter()
        .map(|device| LanDeviceView {
            paired: paired.contains(&device.device_id),
            device_id: device.device_id,
            name: device.name,
            address: device.address,
            fingerprint: device.fingerprint,
        })
        .collect())
}

/// Pairs with a host using its six-digit code and stores the pin + key.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn lan_pair(app: AppHandle, address: String, code: String) -> Result<LanStatusView, PdfError> {
    let directory = config_dir(&app)?;
    let identity = store::identity(&directory)?;
    let hosting_port = HOST.lock().ok().and_then(|guard| guard.as_ref().map(|(host, _)| host.port())).unwrap_or(0);
    let client = LanClient::new(identity.device_id.clone(), identity.name.clone())
        .with_identity(identity.fingerprint.clone(), hosting_port);
    let peer = tauri::async_runtime::spawn_blocking(move || client.pair(&address, &code))
        .await
        .map_err(|error| PdfError::Internal(format!("worker thread failed: {error}")))?
        .map_err(sync_error)?;
    let mut store = store::load(&directory)?;
    store.peers.retain(|known| known.device_id != peer.device_id);
    store.peers.push(peer);
    store::save(&directory, &store)?;
    status_of(&app)
}

/// Forgets a client-side pairing.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn lan_unpair(app: AppHandle, device_id: String) -> Result<LanStatusView, PdfError> {
    let directory = config_dir(&app)?;
    let mut store = store::load(&directory)?;
    store.peers.retain(|peer| peer.device_id != device_id);
    store.bases.remove(&device_id);
    store::save(&directory, &store)?;
    status_of(&app)
}

/// Compares the local folder with the peer's folder over the LAN.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn lan_plan(app: AppHandle, device_id: String, folder: String) -> Result<LanPlanView, PdfError> {
    let root = folder_path(&folder)?;
    let directory = config_dir(&app)?;
    let mut store = store::load(&directory)?;
    let peer = client_peer(&store, &device_id)?;
    let bases = bases_for(&store, &device_id);
    let identity = store::identity(&directory)?;
    let device_name = identity.name.clone();
    let plan =
        tauri::async_runtime::spawn_blocking(move || plan_for(&peer, &root, &bases, &identity.device_id, &device_name))
            .await
            .map_err(|error| PdfError::Internal(format!("worker thread failed: {error}")))??;
    // A confirmed deletion keeps its tombstone for 30 days: do not report it
    // again, and drop base entries for paths gone on both sides.
    let tombstoned = store.tombstone_paths(&device_id, store::now_unix());
    let stale = plan.stale_bases.len();
    for path in &plan.stale_bases {
        store.clear_base(&device_id, path);
    }
    if stale > 0 {
        store::save(&directory, &store)?;
    }
    Ok(LanPlanView {
        device_id,
        folder,
        items: plan
            .items
            .into_iter()
            .map(|item| LanPlanItemView {
                state: state_name(&item),
                path: item.path,
                action: item.action.map(action_name).map(str::to_string),
                local_sha256: item.local_sha256,
                remote_sha256: item.remote_sha256,
            })
            .collect(),
        deletions: plan
            .deletions
            .into_iter()
            .filter(|deletion| !tombstoned.contains(&deletion.path))
            .map(|deletion| LanDeletionView {
                path: deletion.path,
                side: match deletion.side {
                    lan::DeletionSide::Local => "local".to_string(),
                    lan::DeletionSide::Remote => "remote".to_string(),
                },
            })
            .collect(),
    })
}

/// Applies one plan item. A conflict needs an explicit `resolution`
/// (`keep_local`, `keep_cloud` or `keep_both`); anything else refuses.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn lan_apply(
    app: AppHandle,
    device_id: String,
    folder: String,
    path: String,
    resolution: Option<String>,
) -> Result<LanApplyView, PdfError> {
    let root = folder_path(&folder)?;
    let directory = config_dir(&app)?;
    let mut store = store::load(&directory)?;
    let peer = client_peer(&store, &device_id)?;
    let bases = bases_for(&store, &device_id);
    let resolution = resolution.as_deref().map(|value| {
        Resolution::parse(value).ok_or_else(|| PdfError::InvalidInput("unknown conflict resolution".into()))
    });
    let resolution = match resolution {
        Some(Ok(value)) => Some(value),
        Some(Err(error)) => return Err(error),
        None => None,
    };
    let path_for_work = path.clone();
    let identity = store::identity(&directory)?;
    let device_name = identity.name.clone();
    let (view, base) = tauri::async_runtime::spawn_blocking(move || {
        apply_one(&peer, &root, &path_for_work, resolution, &bases, &identity.device_id, &device_name)
    })
    .await
    .map_err(|error| PdfError::Internal(format!("worker thread failed: {error}")))??;
    if let Some(base) = base {
        store.bases.entry(device_id.clone()).or_default().insert(path, base);
        store::save(&directory, &store)?;
    }
    Ok(view)
}

/// Applies every item that needs no question; conflicts are left for the user.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn lan_sync_safe(app: AppHandle, device_id: String, folder: String) -> Result<Vec<LanApplyView>, PdfError> {
    let root = folder_path(&folder)?;
    let directory = config_dir(&app)?;
    let mut store = store::load(&directory)?;
    let peer = client_peer(&store, &device_id)?;
    let bases = bases_for(&store, &device_id);
    let identity = store::identity(&directory)?;
    let device_name = identity.name.clone();
    let results = tauri::async_runtime::spawn_blocking(move || {
        apply_safe(&peer, &root, &bases, &identity.device_id, &device_name)
    })
    .await
    .map_err(|error| PdfError::Internal(format!("worker thread failed: {error}")))??;
    for (path, base) in &results.bases {
        store.bases.entry(device_id.clone()).or_default().insert(path.clone(), base.clone());
    }
    store::save(&directory, &store)?;
    Ok(results.views)
}

/// Confirms one deletion the plan reported. `side` is `local` (removed here)
/// or `remote` (removed on the peer); `resolution` is `delete_remote`,
/// `restore_local`, `delete_local` or `restore_remote`. Nothing is deleted
/// without this call.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn lan_resolve_deletion(
    app: AppHandle,
    device_id: String,
    folder: String,
    path: String,
    side: String,
    resolution: String,
) -> Result<LanDeletionResultView, PdfError> {
    let root = folder_path(&folder)?;
    let action = DeletionAction::parse(&side, &resolution)?;
    let directory = config_dir(&app)?;
    let mut store = store::load(&directory)?;
    let peer = client_peer(&store, &device_id)?;
    let identity = store::identity(&directory)?;
    let name = identity.name.clone();
    let device = device_id.clone();
    let path_for_work = path.clone();
    let (view, base, candidate) = tauri::async_runtime::spawn_blocking(move || {
        resolve_deletion(&peer, &root, &path_for_work, action, &device, &name)
    })
    .await
    .map_err(|error| PdfError::Internal(format!("worker thread failed: {error}")))??;
    match base {
        Some(sha) => {
            store.bases.entry(device_id.clone()).or_default().insert(path.clone(), sha);
        }
        None => store.clear_base(&device_id, &path),
    }
    match action {
        DeletionAction::DeleteRemote | DeletionAction::DeleteLocal => {
            if let Some(candidate) = candidate {
                let side = candidate.side.clone();
                let sha256 = candidate.sha256.clone();
                trash::push(&directory, candidate)?;
                store.record_tombstone(&device_id, &path, &sha256, &side, store::now_unix());
            }
        }
        DeletionAction::RestoreLocal | DeletionAction::RestoreRemote => {
            store.clear_tombstone(&device_id, &path);
        }
    }
    store::save(&directory, &store)?;
    Ok(view)
}

/// The local trash, newest first (expired entries are pruned on the way).
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn lan_trash_list(app: AppHandle) -> Result<Vec<LanTrashEntryView>, PdfError> {
    let directory = config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        trash::list(&directory).map(|entries| entries.into_iter().map(trash_view).collect())
    })
    .await
    .map_err(|error| PdfError::Internal(format!("worker thread failed: {error}")))?
}

/// Writes a trashed document back into the shared folder (no upload; the next
/// plan sees it as a local change). Consumes the trash entry and clears its
/// tombstone.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn lan_trash_restore(app: AppHandle, folder: String, id: String) -> Result<LanStatusView, PdfError> {
    let root = folder_path(&folder)?;
    let directory = config_dir(&app)?;
    let mut store = store::load(&directory)?;
    let directory_for_take = directory.clone();
    let taken = tauri::async_runtime::spawn_blocking(move || trash::take(&directory_for_take, &id))
        .await
        .map_err(|error| PdfError::Internal(format!("worker thread failed: {error}")))??;
    let Some((entry, bytes)) = taken else {
        return Err(PdfError::NotFound("this document is no longer in the trash".into()));
    };
    let target = lan::resolve_path(&root, &entry.path).map_err(sync_error)?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(PdfError::from_io)?;
    }
    synccore::metadata::write_atomic(&target, &bytes).map_err(|error| PdfError::Internal(error.to_string()))?;
    store.clear_tombstone(&entry.peer_device_id, &entry.path);
    store.clear_base(&entry.peer_device_id, &entry.path);
    store::save(&directory, &store)?;
    status_of(&app)
}

/// Drops expired trash entries (older than the 30-day retention window).
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn lan_trash_purge(app: AppHandle) -> Result<usize, PdfError> {
    let directory = config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || trash::purge_expired(&directory))
        .await
        .map_err(|error| PdfError::Internal(format!("worker thread failed: {error}")))?
}

// ---------------------------------------------------------------------------
// The actual work (blocking)
// ---------------------------------------------------------------------------

struct FolderPlan {
    items: Vec<PlanItem>,
    deletions: Vec<lan::LanDeletion>,
    /// Paths in the base that exist on neither side anymore; their base entry
    /// is stale and would turn a later re-appearance into a fake conflict.
    stale_bases: Vec<String>,
}

fn plan_for(
    peer: &PairedPeer,
    root: &Path,
    bases: &HashMap<String, BaseState>,
    device_id: &str,
    name: &str,
) -> Result<FolderPlan, PdfError> {
    let client = LanClient::new(device_id, name);
    let remote = client.list(peer).map_err(sync_error)?;
    let local = lan::scan_folder(root).map_err(sync_error)?;
    let items = lan::plan_sync(&local, &remote, bases);
    let deletions = lan::plan_deletions(&local, &remote, bases);
    let existing: std::collections::HashSet<String> =
        local.iter().map(|file| file.path.clone()).chain(remote.iter().map(|file| file.path.clone())).collect();
    let stale_bases = bases.keys().filter(|path| !existing.contains(*path)).cloned().collect();
    Ok(FolderPlan { items, deletions, stale_bases })
}

fn apply_one(
    peer: &PairedPeer,
    root: &Path,
    path: &str,
    resolution: Option<Resolution>,
    bases: &HashMap<String, BaseState>,
    device_id: &str,
    name: &str,
) -> Result<(LanApplyView, Option<String>), PdfError> {
    let plan = plan_for(peer, root, bases, device_id, name)?;
    let item = plan
        .items
        .iter()
        .find(|item| item.path == path)
        .ok_or_else(|| PdfError::NotFound(format!("{path} is not in the plan")))?;
    let action = match item.action {
        Some(action) => action,
        None => {
            let resolution = resolution
                .ok_or_else(|| PdfError::InvalidInput("this file changed on both sides; pick a resolution".into()))?;
            lan::resolve_action(item.state, resolution).map_err(sync_error)?
        }
    };
    let client = LanClient::new(device_id, name);
    let base = execute(&client, peer, root, path, action)?;
    Ok((
        LanApplyView {
            path: path.to_string(),
            applied: action != MergeAction::Nothing,
            action: action_name(action).into(),
            warning: None,
        },
        base,
    ))
}

fn apply_safe(
    peer: &PairedPeer,
    root: &Path,
    bases: &HashMap<String, BaseState>,
    device_id: &str,
    name: &str,
) -> Result<SafeReport, PdfError> {
    let plan = plan_for(peer, root, bases, device_id, name)?;
    let client = LanClient::new(device_id, name);
    let mut views = Vec::new();
    let mut updated = Vec::new();
    for item in plan.items {
        let Some(action) = item.action else {
            views.push(LanApplyView {
                path: item.path.clone(),
                applied: false,
                action: "conflict".into(),
                warning: Some("changed on both sides; keep both, keep local or keep remote".into()),
            });
            continue;
        };
        match execute(&client, peer, root, &item.path, action) {
            Ok(base) => {
                if let Some(base) = base {
                    updated.push((item.path.clone(), base));
                }
                views.push(LanApplyView {
                    path: item.path.clone(),
                    applied: action != MergeAction::Nothing,
                    action: action_name(action).into(),
                    warning: None,
                });
            }
            Err(error) => views.push(LanApplyView {
                path: item.path.clone(),
                applied: false,
                action: action_name(action).into(),
                warning: Some(error.to_string()),
            }),
        }
    }
    Ok(SafeReport { views, bases: updated })
}

fn resolve_deletion(
    peer: &PairedPeer,
    root: &Path,
    path: &str,
    action: DeletionAction,
    device_id: &str,
    name: &str,
) -> Result<(LanDeletionResultView, Option<String>, Option<trash::TrashCandidate>), PdfError> {
    let client = LanClient::new(device_id, name);
    let mut candidate: Option<trash::TrashCandidate> = None;
    let base = match action {
        DeletionAction::DeleteRemote => {
            // Keep the peer's bytes before they are gone; a fetch failure means
            // the file already vanished there, which is not a reason to stop.
            candidate = client.fetch(peer, path).ok().map(|(remote, bytes)| trash::TrashCandidate {
                path: path.to_string(),
                peer_device_id: peer.device_id.clone(),
                peer_name: peer.name.clone(),
                side: "local".to_string(),
                sha256: remote.sha256,
                bytes,
            });
            client.remove(peer, path).map_err(sync_error)?;
            None
        }
        DeletionAction::RestoreLocal => {
            let (remote, bytes) = client.fetch(peer, path).map_err(sync_error)?;
            let target = lan::resolve_path(root, path).map_err(sync_error)?;
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(PdfError::from_io)?;
            }
            synccore::metadata::write_atomic(&target, &bytes).map_err(|error| PdfError::Internal(error.to_string()))?;
            Some(remote.sha256)
        }
        DeletionAction::DeleteLocal => {
            let target = lan::resolve_path(root, path).map_err(sync_error)?;
            candidate = std::fs::read(&target).ok().map(|bytes| trash::TrashCandidate {
                path: path.to_string(),
                peer_device_id: peer.device_id.clone(),
                peer_name: peer.name.clone(),
                side: "remote".to_string(),
                sha256: synccore::metadata::hash_bytes(&bytes),
                bytes,
            });
            match std::fs::remove_file(&target) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(PdfError::from_io(error)),
            }
            None
        }
        DeletionAction::RestoreRemote => {
            let target = lan::resolve_path(root, path).map_err(sync_error)?;
            let bytes = std::fs::read(&target).map_err(PdfError::from_io)?;
            let remote = client.push(peer, path, &bytes).map_err(sync_error)?;
            Some(remote.sha256)
        }
    };
    Ok((
        LanDeletionResultView { path: path.to_string(), applied: true, action: action.as_str().to_string() },
        base,
        candidate,
    ))
}

struct SafeReport {
    views: Vec<LanApplyView>,
    bases: Vec<(String, String)>,
}

/// Runs one merge action and returns the new common hash, if any.
fn execute(
    client: &LanClient,
    peer: &PairedPeer,
    root: &Path,
    path: &str,
    action: MergeAction,
) -> Result<Option<String>, PdfError> {
    match action {
        MergeAction::Nothing => Ok(None),
        MergeAction::UploadLocal => {
            let local = lan::resolve_path(root, path).map_err(sync_error)?;
            let bytes = std::fs::read(&local).map_err(PdfError::from_io)?;
            if bytes.len() as u64 > synccore::MAX_TRANSFER_BYTES {
                return Err(PdfError::InvalidInput("the document is larger than the transfer limit".into()));
            }
            let remote = client.push(peer, path, &bytes).map_err(sync_error)?;
            Ok(Some(remote.sha256))
        }
        MergeAction::DownloadCloud => {
            let (remote, bytes) = client.fetch(peer, path).map_err(sync_error)?;
            let target = lan::resolve_path(root, path).map_err(sync_error)?;
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(PdfError::from_io)?;
            }
            synccore::metadata::write_atomic(&target, &bytes).map_err(|error| PdfError::Internal(error.to_string()))?;
            Ok(Some(remote.sha256))
        }
        MergeAction::DownloadCloudCopy => {
            let (_, bytes) = client.fetch(peer, path).map_err(sync_error)?;
            let stamp = synccore::metadata::stamp_for_name(std::time::SystemTime::now());
            let file_name = path.rsplit('/').next().unwrap_or(path);
            let parent = Path::new(path).parent().map(|value| value.to_string_lossy().replace('\\', "/"));
            let exists = |name: &str| {
                let candidate = match &parent {
                    Some(folder) if !folder.is_empty() => format!("{folder}/{name}"),
                    _ => name.to_string(),
                };
                lan::resolve_path(root, &candidate).map(|path| path.exists()).unwrap_or(false)
            };
            let unique = merge::unique_cloud_copy_name(file_name, &stamp, exists);
            let relative = match &parent {
                Some(folder) if !folder.is_empty() => format!("{folder}/{unique}"),
                _ => unique,
            };
            let target = lan::resolve_path(root, &relative).map_err(sync_error)?;
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(PdfError::from_io)?;
            }
            synccore::metadata::write_atomic(&target, &bytes).map_err(|error| PdfError::Internal(error.to_string()))?;
            // The copy is a new file; no common hash is recorded for it.
            Ok(None)
        }
    }
}
