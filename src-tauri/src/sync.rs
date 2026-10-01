//! Local-first cloud sync commands (V3.1).
//!
//! Privacy / safety model:
//!
//! * **Off by default.** `enabled: false` in `sync.json`; every network
//!   command refuses with a clear message until the user turns sync on.
//! * **Explicit actions only.** There is no background polling and no
//!   automatic upload/download in V3.1. The UI buttons are the only triggers.
//! * **Never silently overwrite.** Uploads carry `If-Match` when a base
//!   version exists; downloads refuse to replace a locally changed file
//!   unless the user explicitly chose `keep_cloud`; divergent content is
//!   surfaced as `Conflict` and resolved only from the resolve panel.
//! * **Credentials.** The WebDAV password is stored with the same secret
//!   store the AI key uses (DPAPI on Windows, plain fallback elsewhere - the
//!   existing secret.rs behaviour is reused unchanged). The password is never
//!   written to `sync.json`, never returned to the frontend and never logged.
//! * **OneDrive / Google Drive are declared but unavailable.** They return
//!   [`PdfError::Unsupported`] with an honest "requires OAuth" message; the
//!   app never pretends they work.

use crate::secret;
use pdfcore::error::{ErrorCode, PdfError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use synccore::metadata::{
    self, unknown_cloud_hash, BaseState, SyncMeta, SyncState,
};
use synccore::merge::{self, MergeAction, Resolution};
use synccore::webdav::{normalize_remote_dir, normalize_remote_name, promote_staged_download, WebDavProvider};
use synccore::{RemoteEntry, SyncError, SyncProvider, MAX_TRANSFER_BYTES};
use tauri::{AppHandle, Manager};

const CONFIG_FILE: &str = "sync.json";
/// Deliberately a separate file from the AI key: clearing one must never
/// clear the other.
const PASSWORD_FILE: &str = "sync-credentials.bin";
const DEFAULT_PROVIDER: &str = "webdav";

// ---------------------------------------------------------------------------
// Error mapping
// ---------------------------------------------------------------------------

/// Maps a [`SyncError`] to the app error type. Sync messages are written to
/// be user readable; the Sync screen displays `error.message` directly (the
/// generic `errors.*` localization would be misleading for these codes).
fn sync_error(error: SyncError) -> PdfError {
    match error {
        SyncError::Disabled => PdfError::coded(
            ErrorCode::InvalidInput,
            "Cloud sync is turned off. Enable it in the Sync settings first.",
        ),
        SyncError::Unsupported(message) => PdfError::Unsupported(message),
        SyncError::NotFound(message) => PdfError::NotFound(message),
        SyncError::Conflict(message) => PdfError::coded(ErrorCode::InvalidInput, message),
        SyncError::TooLarge(limit) => PdfError::coded(
            ErrorCode::InvalidInput,
            format!(
                "The file is larger than the {:.0} MB sync limit.",
                limit as f64 / (1024.0 * 1024.0)
            ),
        ),
        SyncError::Auth(message) => {
            PdfError::coded(ErrorCode::InvalidInput, format!("WebDAV sign-in failed: {message}"))
        }
        SyncError::Network(message) => {
            PdfError::coded(ErrorCode::InvalidInput, format!("Network problem: {message}"))
        }
        SyncError::Http { status, message } => PdfError::coded(
            ErrorCode::InvalidInput,
            format!("The WebDAV server answered HTTP {status} ({message})."),
        ),
        SyncError::Protocol(message) => PdfError::coded(
            ErrorCode::InvalidInput,
            format!("The WebDAV server sent an unexpected response: {message}"),
        ),
        SyncError::InvalidInput(message) => PdfError::coded(ErrorCode::InvalidInput, message),
        SyncError::Io(message) => PdfError::Internal(format!("sync I/O error: {message}")),
        SyncError::Internal(message) => PdfError::Internal(message),
    }
}

/// Runs blocking (network + file) work on the Tauri worker pool so the UI
/// thread never waits on a transfer.
async fn run_blocking<T, F>(work: F) -> Result<T, PdfError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, PdfError> + Send + 'static,
{
    let _permit = crate::concurrency::acquire().await;
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| PdfError::Internal(format!("sync worker failed: {error}")))?
}

// ---------------------------------------------------------------------------
// Config persistence
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncConfigFile {
    /// Cloud sync is OFF unless the user turns it on. Default `false`.
    #[serde(default)]
    pub enabled: bool,
    /// "webdav" | "onedrive" | "google-drive". The latter two are stored but
    /// always rejected at use time in this build.
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub username: String,
    /// Explicit opt-in for the plain-HTTP loopback exception. Even with this
    /// set, `normalize_base_url_with_options` only accepts `http://` for
    /// loopback hosts; public HTTP endpoints are always refused.
    #[serde(default)]
    pub allow_insecure_http: bool,
    #[serde(default = "default_remote_dir")]
    pub remote_dir: String,
}

fn default_provider() -> String {
    DEFAULT_PROVIDER.to_string()
}

fn default_remote_dir() -> String {
    "/".to_string()
}

impl Default for SyncConfigFile {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: default_provider(),
            url: String::new(),
            username: String::new(),
            allow_insecure_http: false,
            remote_dir: default_remote_dir(),
        }
    }
}

/// Frontend view. Never contains the password itself.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncConfigView {
    pub enabled: bool,
    pub provider: String,
    pub url: String,
    pub username: String,
    /// True when the user explicitly allowed plain HTTP for a loopback
    /// server. Public HTTP endpoints are refused regardless.
    pub allow_insecure_http: bool,
    pub remote_dir: String,
    pub has_password: bool,
    /// "dpapi" (OS encrypted), "plain" (no OS encryption available) or "none".
    pub password_storage: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSaveInput {
    pub enabled: bool,
    pub provider: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub username: String,
    /// Explicit opt-in for plain HTTP on a loopback server only.
    #[serde(default)]
    pub allow_insecure_http: bool,
    /// `None` keeps the stored password; `Some("")` clears it.
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub remote_dir: String,
}

fn config_dir(app: &AppHandle) -> Result<PathBuf, PdfError> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|error| PdfError::Internal(format!("config dir unavailable: {error}")))?;
    std::fs::create_dir_all(&dir).map_err(PdfError::from_io)?;
    Ok(dir)
}

fn config_path(app: &AppHandle) -> Result<PathBuf, PdfError> {
    Ok(config_dir(app)?.join(CONFIG_FILE))
}

fn password_path(app: &AppHandle) -> Result<PathBuf, PdfError> {
    Ok(config_dir(app)?.join(PASSWORD_FILE))
}

fn load_config(app: &AppHandle) -> SyncConfigFile {
    config_path(app)
        .ok()
        .filter(|path| path.exists())
        .and_then(|path| std::fs::read(path).ok())
        .map(|bytes| {
            // Tolerate a UTF-8 BOM and hand-edited files.
            let text = String::from_utf8_lossy(bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes)).to_string();
            text
        })
        .and_then(|text| serde_json::from_str::<SyncConfigFile>(&text).ok())
        .unwrap_or_default()
}

fn save_config_file(app: &AppHandle, config: &SyncConfigFile) -> Result<(), PdfError> {
    let path = config_path(app)?;
    let payload = serde_json::to_vec_pretty(config)
        .map_err(|error| PdfError::Internal(format!("sync config serialize failed: {error}")))?;
    metadata::write_atomic(&path, &payload).map_err(sync_error)
}

fn password_storage(app: &AppHandle) -> String {
    password_path(app)
        .ok()
        .filter(|path| path.exists())
        .map(|path| {
            std::fs::read_to_string(&path)
                .map(|content| {
                    if content.starts_with("dpapi:") {
                        "dpapi".to_string()
                    } else {
                        "plain".to_string()
                    }
                })
                .unwrap_or_else(|_| "plain".to_string())
        })
        .unwrap_or_else(|| "none".to_string())
}

fn config_view(app: &AppHandle) -> SyncConfigView {
    let config = load_config(app);
    let has_password = password_path(app)
        .ok()
        .and_then(|path| secret::load_api_key(&path).ok())
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    SyncConfigView {
        enabled: config.enabled,
        provider: config.provider,
        url: config.url,
        username: config.username,
        allow_insecure_http: config.allow_insecure_http,
        remote_dir: config.remote_dir,
        has_password,
        password_storage: password_storage(app),
    }
}

// ---------------------------------------------------------------------------
// Provider construction
// ---------------------------------------------------------------------------

enum ProviderKind {
    WebDav,
    OneDrive,
    GoogleDrive,
}

fn provider_kind(value: &str) -> ProviderKind {
    match value.trim().to_ascii_lowercase().as_str() {
        "onedrive" | "one-drive" => ProviderKind::OneDrive,
        "google-drive" | "google_drive" | "googledrive" => ProviderKind::GoogleDrive,
        _ => ProviderKind::WebDav,
    }
}

struct WebDavContext {
    config: SyncConfigFile,
    provider: WebDavProvider,
}

/// Builds the provider for the saved config, enforcing the off switch and the
/// honest OAuth refusals. This is the single gate every network command goes
/// through, so a disabled config can never reach the network by accident.
fn webdav_context(app: &AppHandle) -> Result<WebDavContext, PdfError> {
    let config = load_config(app);
    if !config.enabled {
        return Err(sync_error(SyncError::Disabled));
    }
    match provider_kind(&config.provider) {
        ProviderKind::OneDrive => Err(PdfError::Unsupported(
            "OneDrive sync requires OAuth sign-in, which is not available in this build. Choose WebDAV instead."
                .to_string(),
        )),
        ProviderKind::GoogleDrive => Err(PdfError::Unsupported(
            "Google Drive sync requires OAuth sign-in, which is not available in this build. Choose WebDAV instead."
                .to_string(),
        )),
        ProviderKind::WebDav => {
            if config.url.trim().is_empty() {
                return Err(PdfError::coded(
                    ErrorCode::InvalidInput,
                    "Add the WebDAV server URL before using sync.",
                ));
            }
            let password = password_path(app)
                .ok()
                .and_then(|path| secret::load_api_key(&path).ok())
                .unwrap_or_default();
            let provider = WebDavProvider::new_with_options(
                &config.url,
                &config.username,
                &password,
                config.allow_insecure_http,
            )
            .map_err(sync_error)?;
            Ok(WebDavContext { config, provider })
        }
    }
}

// ---------------------------------------------------------------------------
// Local path validation
// ---------------------------------------------------------------------------

fn validate_local_file(path: &Path) -> Result<String, PdfError> {
    if !path.exists() {
        return Err(PdfError::NotFound(path.display().to_string()));
    }
    if !path.is_file() {
        return Err(PdfError::coded(
            ErrorCode::InvalidInput,
            format!("{} is not a file.", path.display()),
        ));
    }
    let extension = path
        .extension()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if extension != "oswk" {
        return Err(PdfError::coded(
            ErrorCode::InvalidInput,
            "Cloud sync tracks Office Swiss Army Knife (.oswk) documents only.",
        ));
    }
    metadata::file_name_of(path).map_err(sync_error)
}

fn validate_remote_document_name(name: &str) -> Result<String, PdfError> {
    let normalized = normalize_remote_name(name).map_err(sync_error)?;
    if !normalized.to_ascii_lowercase().ends_with(".oswk") {
        return Err(PdfError::coded(
            ErrorCode::InvalidInput,
            "Cloud sync tracks .oswk documents only.",
        ));
    }
    Ok(normalized)
}

fn remote_path_for(remote_dir: &str, file_name: &str) -> String {
    let dir = remote_dir.trim().trim_matches('/');
    if dir.is_empty() {
        file_name.to_string()
    } else {
        format!("{dir}/{file_name}")
    }
}

// ---------------------------------------------------------------------------
// Status evaluation (shared by status/upload/download/resolve)
// ---------------------------------------------------------------------------

struct Evaluated {
    local_path: PathBuf,
    file_name: String,
    remote_path: String,
    local_sha256: String,
    local_size: u64,
    meta: Option<SyncMeta>,
    entry: Option<RemoteEntry>,
    /// Cloud content hash: real when computed, a marker when only "differs"
    /// is known, `None` when the cloud copy does not exist.
    cloud_sha256: Option<String>,
    cloud_etag: Option<String>,
    conflict_note: Option<String>,
}

impl Evaluated {
    fn state(&self) -> SyncState {
        let base = self.meta.as_ref().and_then(|meta| meta.base.as_ref());
        metadata::detect_state(Some(&self.local_sha256), self.cloud_sha256.as_deref(), base)
    }
}

/// Lists the remote directory and computes the local hash. The cloud hash is
/// only downloaded when it cannot be inferred from the etag or the size:
///
/// * base etag == current etag -> the cloud copy is byte-identical to the
///   base, reuse the stored hash (no download);
/// * size differs -> the content differs, no download needed;
/// * local unchanged and the etag moved -> cloud changed, no download needed;
/// * otherwise -> download once and hash (never written anywhere).
fn evaluate(app: &AppHandle, local_path: &Path) -> Result<Evaluated, PdfError> {
    let context = webdav_context(app)?;
    let file_name = validate_local_file(local_path)?;
    let (local_sha256, local_size) = metadata::hash_file(local_path).map_err(sync_error)?;
    let meta = metadata::load_meta(&config_dir(app)?, local_path);

    let entries = context
        .provider
        .list(&context.config.remote_dir)
        .map_err(sync_error)?;
    let entry = entries.into_iter().find(|entry| entry.name == file_name);
    let remote_path = remote_path_for(&context.config.remote_dir, &file_name);

    let mut conflict_note = None;
    let mut cloud_sha256 = None;
    let mut cloud_etag = None;

    if let Some(entry) = &entry {
        cloud_etag = entry.etag.clone();
        if entry.is_dir {
            conflict_note = Some(format!(
                "A folder named {file_name} already exists on the server; rename one side before syncing."
            ));
            // State stays Conflict because no content comparison is possible.
            cloud_sha256 = Some(String::from("remote-is-a-folder"));
        } else {
            let base_etag = meta
                .as_ref()
                .and_then(|meta| meta.base.as_ref())
                .and_then(|base| base.cloud_etag.as_deref());
            let etag_shortcut = match (base_etag, entry.etag.as_deref()) {
                (Some(stored), Some(current)) if stored == current => meta
                    .as_ref()
                    .and_then(|meta| meta.base.as_ref())
                    .map(|base| base.cloud_sha256.clone()),
                _ => None,
            };
            cloud_sha256 = etag_shortcut;

            if cloud_sha256.is_none() {
                let local_unchanged = meta
                    .as_ref()
                    .and_then(|meta| meta.base.as_ref())
                    .map(|base| base.cloud_sha256 == local_sha256)
                    .unwrap_or(false);
                if entry.size != local_size {
                    // Different size: the content cannot be equal.
                    cloud_sha256 = Some(unknown_cloud_hash(entry.size, entry.etag.as_deref()));
                } else if local_unchanged && entry.etag.is_some() {
                    // The local copy is the base version and the etag moved,
                    // so the cloud copy changed - no need to fetch it.
                    cloud_sha256 = Some(unknown_cloud_hash(entry.size, entry.etag.as_deref()));
                } else {
                    // Hash the cloud copy without keeping it in memory.
                    let (hash, _, downloaded_etag) = context
                        .provider
                        .get_to_writer(&remote_path, &mut std::io::sink())
                        .map_err(sync_error)?;
                    cloud_sha256 = Some(hash);
                    if downloaded_etag.is_some() {
                        cloud_etag = downloaded_etag;
                    }
                }
            }
        }
    }

    Ok(Evaluated {
        local_path: local_path.to_path_buf(),
        file_name,
        remote_path,
        local_sha256,
        local_size,
        meta,
        entry,
        cloud_sha256,
        cloud_etag,
        conflict_note,
    })
}

/// Status view sent to the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatusView {
    pub file: String,
    pub local_path: String,
    pub remote_path: String,
    pub state: SyncState,
    pub tracked: bool,
    pub local_size: u64,
    pub remote_size: Option<u64>,
    pub local_sha256: String,
    pub cloud_sha256: Option<String>,
    pub remote_etag: Option<String>,
    pub base_etag: Option<String>,
    pub base_sha256: Option<String>,
    pub local_revision: u64,
    pub last_synced_at: Option<String>,
    pub updated_at: Option<String>,
    pub note: Option<String>,
}

fn status_view(evaluated: &Evaluated, state: SyncState, note: Option<String>) -> SyncStatusView {
    SyncStatusView {
        file: evaluated.file_name.clone(),
        local_path: evaluated.local_path.to_string_lossy().to_string(),
        remote_path: evaluated.remote_path.clone(),
        state,
        tracked: evaluated.meta.is_some(),
        local_size: evaluated.local_size,
        remote_size: evaluated.entry.as_ref().map(|entry| entry.size),
        local_sha256: evaluated.local_sha256.clone(),
        cloud_sha256: evaluated.cloud_sha256.clone(),
        remote_etag: evaluated.cloud_etag.clone(),
        base_etag: evaluated
            .meta
            .as_ref()
            .and_then(|meta| meta.base.as_ref())
            .and_then(|base| base.cloud_etag.clone()),
        base_sha256: evaluated
            .meta
            .as_ref()
            .and_then(|meta| meta.base.as_ref())
            .map(|base| base.cloud_sha256.clone()),
        local_revision: evaluated.meta.as_ref().map(|meta| meta.local_revision).unwrap_or(0),
        last_synced_at: evaluated.meta.as_ref().and_then(|meta| meta.last_synced_at.clone()),
        updated_at: evaluated.meta.as_ref().map(|meta| meta.updated_at.clone()),
        note,
    }
}

/// Writes a sidecar after a successful sync step. `base_revision` continues
/// the monotonic base counter (1 for a brand-new base).
fn commit_meta(
    app: &AppHandle,
    local_path: &Path,
    previous: Option<&SyncMeta>,
    content_sha256: &str,
    cloud_etag: Option<String>,
    base_revision: u64,
) -> Result<(), PdfError> {
    let config_dir = config_dir(app)?;
    let device_id = metadata::load_or_create_device_id(&config_dir).map_err(sync_error)?;
    let now = metadata::now_rfc3339();
    let meta = SyncMeta {
        file: metadata::file_name_of(local_path).map_err(sync_error)?,
        local_path: local_path.to_string_lossy().to_string(),
        device_id,
        local_revision: previous.map(|meta| meta.local_revision + 1).unwrap_or(1),
        content_sha256: content_sha256.to_string(),
        updated_at: now.clone(),
        base: Some(BaseState {
            cloud_etag,
            cloud_sha256: content_sha256.to_string(),
            revision: base_revision,
        }),
        last_synced_at: Some(now),
    };
    metadata::save_meta(&config_dir, local_path, &meta).map_err(sync_error)
}

fn base_revision(previous: Option<&SyncMeta>) -> u64 {
    previous
        .and_then(|meta| meta.base.as_ref())
        .map(|base| base.revision + 1)
        .unwrap_or(1)
}

// ---------------------------------------------------------------------------
// Upload / download cores (shared by the explicit commands and resolve)
// ---------------------------------------------------------------------------

/// Uploads the local file. `allow_diverged` is true only for an explicit
/// `keep_local` resolution: without it, CloudAhead/Conflict states are
/// refused so a plain "upload" button can never clobber a newer cloud copy.
fn upload_core(
    app: &AppHandle,
    local_path: &Path,
    allow_diverged: bool,
) -> Result<SyncStatusView, PdfError> {
    let context = webdav_context(app)?;
    let evaluated = evaluate(app, local_path)?;
    let state = evaluated.state();

    if state == SyncState::Synced {
        return Ok(status_view(
            &evaluated,
            state,
            Some("Already in sync; nothing was uploaded.".to_string()),
        ));
    }
    if matches!(state, SyncState::CloudAhead | SyncState::Conflict) && !allow_diverged {
        return Err(sync_error(SyncError::Conflict(format!(
            "{} has unsynced changes on both sides. Resolve the conflict (Keep local / Keep cloud / Keep both) instead of a plain upload.",
            evaluated.file_name
        ))));
    }

    // Create the remote folder chain on first use (no-op when it exists).
    context
        .provider
        .ensure_dir(&context.config.remote_dir)
        .map_err(sync_error)?;

    // Stream the local file to the server: the hash is computed while the
    // bytes travel, so a 512 MB document never sits in memory.
    let (sha256, uploaded_size, new_etag) = context
        .provider
        .put_file(&evaluated.remote_path, local_path, evaluated.cloud_etag.as_deref())
        .map_err(sync_error)?;

    // Conditional write: when the server still has the version we saw, the
    // write succeeds; otherwise 412 -> SyncError::Conflict.
    let previous = evaluated.meta.as_ref();
    let final_etag = new_etag.or(evaluated.cloud_etag.clone());

    commit_meta(
        app,
        local_path,
        previous,
        &sha256,
        final_etag.clone(),
        base_revision(previous),
    )?;

    let mut view = status_view(&evaluated, SyncState::Synced, Some("Uploaded to the cloud.".to_string()));
    view.local_sha256 = sha256.clone();
    view.cloud_sha256 = Some(sha256);
    view.remote_size = Some(uploaded_size);
    view.local_size = uploaded_size;
    view.remote_etag = final_etag.clone();
    view.base_etag = final_etag;
    view.base_sha256 = view.cloud_sha256.clone();
    view.tracked = true;
    Ok(view)
}

/// Downloads the cloud copy. `force` is true only for an explicit
/// `keep_cloud` resolution (or a fresh download to a path without a file);
/// without it, a locally changed file is refused.
fn download_core(
    app: &AppHandle,
    remote_name: &str,
    local_path: &Path,
    force: bool,
) -> Result<SyncStatusView, PdfError> {
    let context = webdav_context(app)?;
    let file_name = validate_remote_document_name(remote_name)?;
    let remote_path = remote_path_for(&context.config.remote_dir, &file_name);

    let parent = local_path
        .parent()
        .ok_or_else(|| PdfError::coded(ErrorCode::InvalidInput, "the destination folder is invalid"))?;
    if !local_path.exists() && !parent.exists() {
        return Err(PdfError::NotFound(parent.display().to_string()));
    }

    let config_dir = config_dir(app)?;
    let previous = metadata::load_meta(&config_dir, local_path);
    let local_exists = local_path.exists();
    let local_sha256 = if local_exists {
        Some(metadata::hash_file(local_path).map_err(sync_error)?.0)
    } else {
        None
    };

    // Stream the cloud copy into a temporary sibling first: the decision to
    // replace the local file is made after hashing, and nothing is written
    // into place until that decision is final.
    let (staged, cloud_sha256, cloud_size, etag) = context
        .provider
        .stage_download(&remote_path, local_path)
        .map_err(sync_error)?;
    let discard_staged = |staged: &Path| {
        let _ = std::fs::remove_file(staged);
    };

    // Builds the post-action view without another directory listing: after a
    // successful download the local copy holds exactly the downloaded bytes.
    let downloaded_view = |note: &str| -> SyncStatusView {
        let evaluated = Evaluated {
            local_path: local_path.to_path_buf(),
            file_name: file_name.clone(),
            remote_path: remote_path.clone(),
            local_sha256: cloud_sha256.clone(),
            local_size: cloud_size,
            meta: metadata::load_meta(&config_dir, local_path),
            entry: None,
            cloud_sha256: Some(cloud_sha256.clone()),
            cloud_etag: etag.clone(),
            conflict_note: None,
        };
        let mut view = status_view(&evaluated, SyncState::Synced, Some(note.to_string()));
        view.local_size = cloud_size;
        view.remote_size = Some(cloud_size);
        view.tracked = true;
        view
    };

    if let (true, Some(local_sha256)) = (local_exists, local_sha256.as_ref()) {
        if local_sha256 == &cloud_sha256 {
            // Same bytes: adopt the etag (and hash) as the new base and report
            // success without writing. Without this, a stale base hash would
            // later be misread as "both sides changed".
            discard_staged(&staged);
            let base_current = previous
                .as_ref()
                .and_then(|meta| meta.base.as_ref())
                .map(|base| base.cloud_sha256 == cloud_sha256 && base.cloud_etag == etag)
                .unwrap_or(false);
            if !base_current {
                commit_meta(
                    app,
                    local_path,
                    previous.as_ref(),
                    &cloud_sha256,
                    etag.clone(),
                    base_revision(previous.as_ref()),
                )?;
            }
            return Ok(downloaded_view("The local file already matches the cloud copy."));
        }
        let local_changed = previous
            .as_ref()
            .map(|meta| meta.content_sha256 != *local_sha256)
            .unwrap_or(true);
        if local_changed && !force {
            discard_staged(&staged);
            return Err(sync_error(SyncError::Conflict(format!(
                "{} has local changes that a download would replace. Use the resolve panel (Keep cloud / Keep both) to decide.",
                file_name
            ))));
        }
    }

    promote_staged_download(&staged, local_path).map_err(sync_error)?;
    commit_meta(app, local_path, previous.as_ref(), &cloud_sha256, etag.clone(), base_revision(previous.as_ref()))?;
    Ok(downloaded_view("Downloaded from the cloud."))
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn sync_get_config(app: AppHandle) -> SyncConfigView {
    config_view(&app)
}

#[tauri::command]
pub fn sync_save_config(app: AppHandle, input: SyncSaveInput) -> Result<SyncConfigView, PdfError> {
    let provider = match provider_kind(&input.provider) {
        ProviderKind::OneDrive => "onedrive",
        ProviderKind::GoogleDrive => "google-drive",
        ProviderKind::WebDav => "webdav",
    }
    .to_string();
    let url = input.url.trim().to_string();
    if !url.is_empty() {
        synccore::webdav::normalize_base_url_with_options(&url, input.allow_insecure_http).map_err(sync_error)?;
    }
    if input.enabled && provider == "webdav" && url.is_empty() {
        return Err(PdfError::coded(
            ErrorCode::InvalidInput,
            "Add the WebDAV server URL before enabling cloud sync.",
        ));
    }
    let remote_dir = {
        let segments = normalize_remote_dir(&input.remote_dir).map_err(sync_error)?;
        if segments.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", segments.join("/"))
        }
    };
    let config = SyncConfigFile {
        enabled: input.enabled,
        provider,
        url,
        username: input.username.trim().to_string(),
        allow_insecure_http: input.allow_insecure_http,
        remote_dir,
    };
    save_config_file(&app, &config)?;

    // `None` keeps the stored password, `Some("")` clears it, `Some(value)`
    // replaces it. The password is written to the secret store only.
    if let Some(password) = input.password {
        let path = password_path(&app)?;
        if password.trim().is_empty() {
            secret::delete_api_key(&path)?;
        } else {
            secret::save_api_key(&path, &password)?;
        }
    }
    Ok(config_view(&app))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncTestResult {
    pub server: String,
    pub remote_dir: String,
    pub remote_dir_exists: bool,
    pub message: String,
}

#[tauri::command]
pub async fn sync_test_connection(app: AppHandle) -> Result<SyncTestResult, PdfError> {
    run_blocking(move || {
        let context = webdav_context(&app)?;
        let server = context.provider.test().map_err(sync_error)?;
        // A missing remote folder is not a failure: it is created on the
        // first upload. Probing it never writes anything.
        let (remote_dir_exists, message) = match context.provider.list(&context.config.remote_dir) {
            Ok(entries) => (
                true,
                format!("Signed in to the server. The remote folder is reachable ({} item(s)).", entries.len()),
            ),
            Err(SyncError::NotFound(_)) => (
                false,
                "Signed in to the server. The remote folder does not exist yet; it will be created on the first upload."
                    .to_string(),
            ),
            Err(error) => return Err(sync_error(error)),
        };
        Ok(SyncTestResult {
            server,
            remote_dir: context.config.remote_dir.clone(),
            remote_dir_exists,
            message,
        })
    })
    .await
}

#[tauri::command]
pub async fn sync_status(app: AppHandle, local_path: String) -> Result<SyncStatusView, PdfError> {
    // The webview is untrusted: every sync path goes through the same
    // centralized validation as the rest of the command surface (absolute,
    // regular file, no traversal, no device names).
    let path = crate::paths::input_file(&local_path)?.into_path_buf();
    run_blocking(move || {
        let evaluated = evaluate(&app, &path)?;
        let state = evaluated.state();
        // Adopting an already-synced file is not a data movement: it only
        // records the current etag/hash as the common base so later edits can
        // be detected. Divergent states are never adopted.
        if state == SyncState::Synced
            && evaluated.cloud_sha256.is_some()
            && evaluated.conflict_note.is_none()
        {
            let base_missing = evaluated
                .meta
                .as_ref()
                .and_then(|meta| meta.base.as_ref())
                .is_none();
            let etag_moved = evaluated
                .meta
                .as_ref()
                .and_then(|meta| meta.base.as_ref())
                .map(|base| base.cloud_etag != evaluated.cloud_etag)
                .unwrap_or(true);
            if base_missing || etag_moved {
                let previous = evaluated.meta.as_ref();
                let revision = if base_missing { 1 } else { base_revision(previous) };
                commit_meta(
                    &app,
                    &path,
                    previous,
                    &evaluated.local_sha256,
                    evaluated.cloud_etag.clone(),
                    revision,
                )?;
                let refreshed = evaluate(&app, &path)?;
                let refreshed_state = refreshed.state();
                let note = if refreshed_state == SyncState::Synced {
                    "Already in sync.".to_string()
                } else {
                    "The base version was updated.".to_string()
                };
                return Ok(status_view(&refreshed, refreshed_state, Some(note)));
            }
        }
        let note = evaluated.conflict_note.clone();
        Ok(status_view(&evaluated, state, note))
    })
    .await
}

#[tauri::command]
pub async fn sync_upload(app: AppHandle, local_path: String) -> Result<SyncStatusView, PdfError> {
    let path = crate::paths::input_file(&local_path)?.into_path_buf();
    run_blocking(move || upload_core(&app, &path, false)).await
}

#[tauri::command]
pub async fn sync_download(
    app: AppHandle,
    remote_name: String,
    local_path: String,
) -> Result<SyncStatusView, PdfError> {
    // The destination may not exist yet, but its parent folder must.
    let target = crate::paths::output_file(&local_path)?.into_path_buf();
    run_blocking(move || download_core(&app, &remote_name, &target, false)).await
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncListEntry {
    pub name: String,
    pub size: u64,
    pub etag: Option<String>,
    pub modified: Option<String>,
}

#[tauri::command]
pub async fn sync_list(app: AppHandle) -> Result<Vec<SyncListEntry>, PdfError> {
    run_blocking(move || {
        let context = webdav_context(&app)?;
        let entries = context
            .provider
            .list(&context.config.remote_dir)
            .map_err(sync_error)?;
        let mut files: Vec<SyncListEntry> = entries
            .into_iter()
            .filter(|entry| !entry.is_dir && entry.name.to_ascii_lowercase().ends_with(".oswk"))
            .map(|entry| SyncListEntry {
                name: entry.name,
                size: entry.size,
                etag: entry.etag,
                modified: entry.modified,
            })
            .collect();
        files.sort_by_key(|a| a.name.to_lowercase());
        Ok(files)
    })
    .await
}

#[tauri::command]
pub async fn sync_resolve(
    app: AppHandle,
    local_path: String,
    resolution: String,
) -> Result<SyncStatusView, PdfError> {
    let path = crate::paths::input_file(&local_path)?.into_path_buf();
    run_blocking(move || {
        let resolution = Resolution::parse(&resolution).ok_or_else(|| {
            PdfError::coded(
                ErrorCode::InvalidInput,
                format!("Unknown sync resolution \"{resolution}\"."),
            )
        })?;
        let evaluated = evaluate(&app, &path)?;
        let state = evaluated.state();
        let action = merge::plan(resolution, state).map_err(sync_error)?;
        match action {
            MergeAction::Nothing => Ok(status_view(
                &evaluated,
                state,
                Some("Nothing to resolve; the file is already in sync.".to_string()),
            )),
            // Explicit force: the user saw the divergence and chose a side.
            MergeAction::UploadLocal => upload_core(&app, &path, true),
            MergeAction::DownloadCloud => download_core(&app, &evaluated.file_name, &path, true),
            MergeAction::DownloadCloudCopy => {
                let context = webdav_context(&app)?;
                let (bytes, _etag) = context
                    .provider
                    .get(&evaluated.remote_path)
                    .map_err(sync_error)?;
                let stamp = metadata::stamp_for_name(SystemTime::now());
                let directory = path.parent().map(Path::to_path_buf).unwrap_or_default();
                let copy_name = merge::unique_cloud_copy_name(&evaluated.file_name, &stamp, |candidate| {
                    directory.join(candidate).exists()
                });
                let copy_path = directory.join(&copy_name);
                metadata::write_atomic(&copy_path, &bytes).map_err(sync_error)?;
                let note = format!(
                    "The cloud version was saved as \"{copy_name}\". The local file is unchanged; resolve it when you are ready."
                );
                Ok(status_view(&evaluated, state, Some(note)))
            }
        }
    })
    .await
}

/// Removes only the sidecar; the document and the cloud copy are untouched.
/// This works even while sync is turned off (local cleanup).
#[tauri::command]
pub fn sync_forget(app: AppHandle, local_path: String) -> Result<(), PdfError> {
    // Forgetting must also work for a file that no longer exists, so only the
    // path shape is validated (still absolute, no traversal/device names).
    let path = crate::paths::lexical(&local_path)?.into_path_buf();
    metadata::delete_meta(&config_dir(&app)?, &path).map_err(sync_error)
}

/// Honest capability report for the UI (size limit, provider availability).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncCapabilities {
    pub max_transfer_bytes: u64,
    pub background_sync: bool,
    pub auto_merge: bool,
    pub providers: Vec<SyncProviderInfo>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncProviderInfo {
    pub id: String,
    pub available: bool,
    pub note: String,
}

#[tauri::command]
pub fn sync_capabilities() -> SyncCapabilities {
    SyncCapabilities {
        max_transfer_bytes: MAX_TRANSFER_BYTES,
        background_sync: false,
        auto_merge: false,
        providers: vec![
            SyncProviderInfo {
                id: "webdav".to_string(),
                available: true,
                note: "Works with Nextcloud, ownCloud, Synology, mailbox.org and any WebDAV server.".to_string(),
            },
            SyncProviderInfo {
                id: "onedrive".to_string(),
                available: false,
                note: "Requires OAuth sign-in; not available in this build.".to_string(),
            },
            SyncProviderInfo {
                id: "google-drive".to_string(),
                available: false,
                note: "Requires OAuth sign-in; not available in this build.".to_string(),
            },
        ],
    }
}
