//! Sandboxed plugin support: plugin storage, the per-plugin data sandbox and
//! the network proxy.
//!
//! Plugin JavaScript runs in a Web Worker with no DOM and no Tauri IPC; the
//! only way out is the host RPC implemented in `src/lib/plugins.ts`. This
//! module is the backend half of that boundary:
//!
//! * manifests and sources live under `<app data>/plugins/<id>/`;
//! * per-plugin data files live under `<app data>/plugins/<id>/data/`;
//! * `plugin_http_request` is the only network path and re-checks the
//!   manifest's `network` permission. Address rules come from `netpolicy.rs`:
//!   https reaches public hosts (the connect-time resolver refuses private
//!   answers, so DNS rebinding cannot bypass it); plain http is a local
//!   network exception for localhost and private literals, never the cloud
//!   metadata address. GET/POST only, 1 MB response cap, 15 s timeout, no
//!   redirects.
//!
//! Threat model: this protects documents and user data from buggy or
//! malicious plugins through the capability API. Worker isolation is
//! same-process and same-engine - it is not an escape from the browser
//! engine itself.
//!
//! The sample plugin ships inside the binary with `include_str!`, so the
//! "reload sample" action works in packaged builds with no resource paths.

mod install;

#[cfg(test)]
use install::install_into;
use install::{delete_plugin, install_confirmed, list_plugins, InstallPlan};
// The folder install is desktop-only (Android has no folder picker), and so is
// its only call site.
#[cfg(any(desktop, test))]
use install::install_from_dir;

use crate::netpolicy::UrlPolicy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Manager};

const MAX_SOURCE_BYTES: usize = 512 * 1024;
const MAX_MANIFEST_BYTES: usize = 64 * 1024;
const MAX_FILE_BYTES: usize = 512 * 1024;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_REQUEST_BYTES: usize = 512 * 1024;
const MAX_HEADER_COUNT: usize = 32;
const MAX_PDF_TEXT_BYTES: usize = 256 * 1024;
const HTTP_TIMEOUT_SECS: u64 = 15;

const SAMPLE_MANIFEST: &str = include_str!("../../plugins/sample/manifest.json");
const SAMPLE_SOURCE: &str = include_str!("../../plugins/sample/main.js");

#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, Serialize)]
pub struct PluginErrorPayload {
    pub code: String,
    pub message: String,
}

impl PluginErrorPayload {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self { code: code.to_string(), message: message.into() }
    }

    fn invalid(message: impl Into<String>) -> Self {
        Self::new("invalid", message)
    }

    fn forbidden(message: impl Into<String>) -> Self {
        Self::new("forbidden", message)
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self::new("notFound", message)
    }

    fn too_large(message: impl Into<String>) -> Self {
        Self::new("tooLarge", message)
    }

    fn internal(message: impl Into<String>) -> Self {
        Self::new("internal", message)
    }
}

type PluginResult<T> = Result<T, PluginErrorPayload>;

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginEntry {
    /// The raw manifest as stored on disk; the frontend validates it strictly.
    #[cfg_attr(feature = "bindings", specta(type = specta_typescript::Unknown))]
    pub manifest: Value,
    #[cfg_attr(feature = "bindings", specta(type = specta_typescript::Number))]
    pub source_bytes: u64,
    /// SHA-256 of the bundle the user confirmed (see `install.rs`); the current
    /// hash when the install predates the record.
    pub sha256: String,
    /// `verified` when the files still match the record, `modified` when they
    /// changed after installation, `unrecorded` for a plugin installed before
    /// the record existed.
    pub integrity: String,
}

/// Read-only result of the `pdf` capability: page count, whether a text layer
/// exists, and the extracted text (bounded).
#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginPdfRead {
    pub page_count: u32,
    pub has_text_layer: bool,
    pub text: String,
}

#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginHttpRequest {
    pub url: String,
    pub method: Option<String>,
    pub headers: Option<std::collections::HashMap<String, String>>,
    pub body: Option<String>,
}

#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginHttpResponse {
    pub status: u16,
    pub body: String,
    pub truncated: bool,
}

// ---------------------------------------------------------------------------
// Paths and names
// ---------------------------------------------------------------------------

fn plugins_root(app: &AppHandle) -> PluginResult<PathBuf> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("plugins"))
        .map_err(|error| PluginErrorPayload::internal(format!("Could not locate the app data directory: {error}")))
}

/// `[a-z0-9]` at both ends, dots/dashes/underscores inside, 1..=64 chars.
/// No separators and no `..`, so the id can never escape the plugins root.
pub fn validate_plugin_id(id: &str) -> bool {
    if id.is_empty() || id.len() > 64 || id.contains("..") {
        return false;
    }
    let mut chars = id.chars();
    let first = chars.next().unwrap_or('_');
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return false;
    }
    let mut last = first;
    for ch in chars {
        if !(ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == '_') {
            return false;
        }
        last = ch;
    }
    last.is_ascii_lowercase() || last.is_ascii_digit()
}

/// Plain data files only: no separators, no `..`, no leading dot, and no
/// trailing dot/space (which Windows silently strips).
pub fn validate_file_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 128 || name.contains("..") {
        return false;
    }
    let mut chars = name.chars();
    let first = chars.next().unwrap_or('.');
    if !first.is_ascii_alphanumeric() {
        return false;
    }
    let mut last = first;
    for ch in chars {
        if !(ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-' || ch == ' ') {
            return false;
        }
        last = ch;
    }
    last.is_ascii_alphanumeric()
}

fn plugin_dir(root: &Path, id: &str) -> PluginResult<PathBuf> {
    if !validate_plugin_id(id) {
        return Err(PluginErrorPayload::invalid("Invalid plugin id."));
    }
    Ok(root.join(id))
}

/// The per-plugin sandbox directory. Created on demand; the returned path is
/// always `<root>/<id>` itself, never a child chosen by the caller.
fn data_dir(root: &Path, id: &str) -> PluginResult<PathBuf> {
    Ok(plugin_dir(root, id)?.join("data"))
}

pub fn sandbox_file(root: &Path, id: &str, name: &str) -> PluginResult<PathBuf> {
    if !validate_file_name(name) {
        return Err(PluginErrorPayload::invalid("Invalid file name."));
    }
    let dir = data_dir(root, id)?;
    // Re-check reparse points on every access, not just at install time: a
    // local attacker who can write the app-data directory could otherwise
    // plant a symlink that redirects a read/write/delete outside the sandbox.
    if is_reparse_point(&dir) {
        return Err(PluginErrorPayload::forbidden("The plugin data directory is a link."));
    }
    let path = dir.join(name);
    if path.parent() != Some(dir.as_path()) {
        return Err(PluginErrorPayload::invalid("Invalid file name."));
    }
    if is_reparse_point(&path) {
        return Err(PluginErrorPayload::forbidden("The plugin file is a link."));
    }
    Ok(path)
}

/// True when `path` is a symlink or, on Windows, any reparse point (symlinks
/// and directory junctions are both reparse points). Missing paths are not
/// links, so creating a fresh sandbox file is unaffected.
fn is_reparse_point(path: &Path) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(path) else { return false };
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    false
}

fn read_capped(path: &Path, max: usize, label: &str) -> PluginResult<Vec<u8>> {
    let metadata =
        std::fs::metadata(path).map_err(|_| PluginErrorPayload::not_found(format!("{label} was not found.")))?;
    if metadata.len() > max as u64 {
        return Err(PluginErrorPayload::too_large(format!("{label} is larger than {} KB.", max / 1024)));
    }
    std::fs::read(path).map_err(|_| PluginErrorPayload::internal(format!("Could not read {label}.")))
}

fn read_text(path: &Path, max: usize, label: &str) -> PluginResult<String> {
    let bytes = read_capped(path, max, label)?;
    String::from_utf8(bytes).map_err(|_| PluginErrorPayload::invalid(format!("{label} is not valid UTF-8 text.")))
}

fn write_text(path: &Path, text: &str, label: &str) -> PluginResult<()> {
    officecore::io::write_atomic(path, text.as_bytes())
        .map_err(|error| PluginErrorPayload::internal(format!("Could not write {label} ({}).", error.code)))
}

fn require_permission(dir: &Path, permission: &str) -> PluginResult<()> {
    let text = read_text(&dir.join("manifest.json"), MAX_MANIFEST_BYTES, "manifest.json")?;
    let manifest: Value =
        serde_json::from_str(&text).map_err(|_| PluginErrorPayload::invalid("manifest.json is not valid JSON."))?;
    let allowed = manifest
        .get("permissions")
        .and_then(Value::as_array)
        .map(|items| items.iter().any(|item| item.as_str() == Some(permission)))
        .unwrap_or(false);
    if !allowed {
        return Err(PluginErrorPayload::forbidden(format!("The plugin lacks the {permission} permission.")));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Network guard
// ---------------------------------------------------------------------------

/// True for the cloud instance-metadata endpoint every major provider uses
/// (AWS/GCP/Azure/OpenStack). It is link-local, so the generic class check would
/// otherwise wave it through; block it explicitly on every scheme.
/// The address policy for a plugin request: https to public hosts, plain http
/// to the local network (the documented LAN exception), never metadata.
fn plugin_policy(url: &reqwest::Url) -> UrlPolicy {
    if url.scheme() == "http" {
        UrlPolicy::PrivateHttp
    } else {
        UrlPolicy::PublicHttps
    }
}

/// Validates a plugin URL against the shared network policy.
///
/// Policy: https is allowed to any public host; plain http is allowed only to
/// localhost and private/link-local literals (and still never to the cloud
/// metadata endpoint). Credentials in the URL are refused. Host names over
/// `https` are re-checked at connect time by the policy's DNS resolver.
pub fn validate_http_url(raw: &str) -> PluginResult<reqwest::Url> {
    let url = reqwest::Url::parse(raw).map_err(|_| PluginErrorPayload::invalid("The URL is not valid."))?;
    plugin_policy(&url).check_url(&url).map_err(PluginErrorPayload::forbidden)?;
    Ok(url)
}

fn forbidden_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "host" | "connection" | "content-length" | "transfer-encoding" | "upgrade" | "proxy-authorization" | "te"
    )
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub fn plugin_list(app: AppHandle) -> PluginResult<Vec<PluginEntry>> {
    Ok(list_plugins(&plugins_root(&app)?))
}

#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub fn plugin_read_source(app: AppHandle, id: String) -> PluginResult<String> {
    let root = plugins_root(&app)?;
    let dir = plugin_dir(&root, &id)?;
    read_text(&dir.join("main.js"), MAX_SOURCE_BYTES, "main.js")
}

/// Installs the manifest/source pair the frontend holds, after the human
/// confirms the plan in a native dialog the *Rust* side opens (E1/5): the
/// webview cannot accept on its own behalf, and a declined dialog writes
/// nothing.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn plugin_install(app: AppHandle, manifest_json: String, source: String) -> PluginResult<PluginEntry> {
    let root = plugins_root(&app)?;
    let dialog_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        install_confirmed(&root, &manifest_json, &source, |plan| confirm_install(&dialog_app, plan))
    })
    .await
    .map_err(|_| PluginErrorPayload::internal("The plugin install task failed."))?
}

/// Asks the human to approve an install. The dialog blocks this worker thread,
/// never the UI thread, and a platform that never answers counts as "no".
fn confirm_install(app: &AppHandle, plan: &InstallPlan) -> bool {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
    let (sender, receiver) = std::sync::mpsc::channel();
    app.dialog()
        .message(plan.summary())
        .title("Install plugin")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom("Install".into(), "Cancel".into()))
        .show(move |confirmed| {
            let _ = sender.send(confirmed);
        });
    receiver.recv_timeout(std::time::Duration::from_secs(300)).unwrap_or(false)
}

/// Installs a plugin from a folder the user picks in a native dialog that the
/// *Rust* side opens. The webview cannot supply the path: it only triggers this
/// command, and the folder is chosen by the human. This removes the previous
/// design where a compromised renderer could pass any directory on disk.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn plugin_install_from_dialog(app: AppHandle) -> PluginResult<Option<PluginEntry>> {
    #[cfg(any(desktop, target_os = "windows", target_os = "linux", target_os = "macos"))]
    {
        use tauri_plugin_dialog::DialogExt;
        let (tx, rx) = std::sync::mpsc::channel();
        app.dialog().file().set_title("Choose a plugin folder (manifest.json + main.js)").pick_folder(move |folder| {
            let _ = tx.send(folder);
        });
        let picked = tauri::async_runtime::spawn_blocking(move || rx.recv().ok().flatten())
            .await
            .map_err(|_| PluginErrorPayload::internal("The plugin dialog failed."))?;
        let Some(picked) = picked else {
            return Ok(None); // user cancelled
        };
        let source_dir =
            picked.into_path().map_err(|_| PluginErrorPayload::internal("The chosen folder is not a local path."))?;
        let entry = install_from_dir(&plugins_root(&app)?, &source_dir)?;
        Ok(Some(entry))
    }
    #[cfg(not(any(desktop, target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        let _ = app;
        Err(PluginErrorPayload::forbidden("Plugin folder installation is only supported on desktop."))
    }
}

#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub fn plugin_install_sample(app: AppHandle) -> PluginResult<PluginEntry> {
    // Bundled content: the manifest and source ship inside the binary, so
    // nothing here was chosen by the webview.
    install_confirmed(&plugins_root(&app)?, SAMPLE_MANIFEST, SAMPLE_SOURCE, |_| true)
}

#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub fn plugin_delete(app: AppHandle, id: String) -> PluginResult<()> {
    delete_plugin(&plugins_root(&app)?, &id)
}

#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub fn plugin_file_read(app: AppHandle, plugin_id: String, name: String) -> PluginResult<String> {
    let root = plugins_root(&app)?;
    let dir = plugin_dir(&root, &plugin_id)?;
    require_permission(&dir, "read_files")?;
    let path = sandbox_file(&root, &plugin_id, &name)?;
    let bytes = read_capped(&path, MAX_FILE_BYTES, "The file")?;
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub fn plugin_file_write(app: AppHandle, plugin_id: String, name: String, text: String) -> PluginResult<()> {
    let root = plugins_root(&app)?;
    let dir = plugin_dir(&root, &plugin_id)?;
    require_permission(&dir, "write_files")?;
    let path = sandbox_file(&root, &plugin_id, &name)?;
    if text.len() > MAX_FILE_BYTES {
        return Err(PluginErrorPayload::too_large(format!("The file is larger than {} KB.", MAX_FILE_BYTES / 1024)));
    }
    write_text(&path, &text, "The file")
}

/// Read-only PDF capability. The file name is resolved inside the plugin's own
/// sandbox directory (the same guard as the file commands), and the manifest
/// must declare the `read_pdf` permission. The extracted text is bounded so a
/// huge PDF cannot exhaust memory in the worker.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub fn plugin_pdf_read(app: AppHandle, plugin_id: String, name: String) -> PluginResult<PluginPdfRead> {
    let root = plugins_root(&app)?;
    let dir = plugin_dir(&root, &plugin_id)?;
    require_permission(&dir, "read_pdf")?;
    let path = sandbox_file(&root, &plugin_id, &name)?;
    let info = pdfcore::info::pdf_info(&path, None)
        .map_err(|error| PluginErrorPayload::invalid(format!("The PDF could not be read: {error}")))?;
    let runs = pdfcore::content::list_text_runs_in_file(&path, None)
        .map_err(|error| PluginErrorPayload::invalid(format!("The PDF text could not be read: {error}")))?;
    let mut text = runs.into_iter().map(|run| run.text).collect::<Vec<_>>().join(" ");
    truncate_at_char_boundary(&mut text, MAX_PDF_TEXT_BYTES);
    Ok(PluginPdfRead { page_count: info.page_count, has_text_layer: info.has_text_layer, text })
}

/// Truncates `text` to at most `max` bytes without splitting a character.
/// `String::truncate` panics when the index lands inside a multi-byte
/// character, which a PDF's text layer (Turkish, emoji, CJK) can easily do.
fn truncate_at_char_boundary(text: &mut String, max: usize) {
    if text.len() <= max {
        return;
    }
    let mut end = max;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
}

#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn plugin_http_request(
    app: AppHandle,
    plugin_id: String,
    request: PluginHttpRequest,
) -> PluginResult<PluginHttpResponse> {
    let root = plugins_root(&app)?;
    let dir = plugin_dir(&root, &plugin_id)?;
    require_permission(&dir, "network")?;

    let url = validate_http_url(&request.url)?;
    // The same policy supplies the connect-time resolver: an https host name
    // that resolves to a private address is filtered when the connection is
    // made, not just when the URL was parsed (DNS rebinding).
    let policy = plugin_policy(&url);
    let method = match request.method.as_deref().unwrap_or("GET").to_ascii_uppercase().as_str() {
        "GET" => reqwest::Method::GET,
        "POST" => reqwest::Method::POST,
        _ => return Err(PluginErrorPayload::invalid("Only GET and POST requests are allowed.")),
    };
    let headers = request.headers.unwrap_or_default();
    if headers.len() > MAX_HEADER_COUNT {
        return Err(PluginErrorPayload::invalid("Too many request headers."));
    }
    let body = request.body.unwrap_or_default();
    if body.len() > MAX_REQUEST_BYTES {
        return Err(PluginErrorPayload::too_large(format!(
            "The request body is larger than {} KB.",
            MAX_REQUEST_BYTES / 1024
        )));
    }

    let client = policy
        .apply_async(
            reqwest::Client::builder()
                .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
                .connect_timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none())
                .user_agent(concat!("OfficeSwissArmyKnife/", env!("CARGO_PKG_VERSION"))),
        )
        .build()
        .map_err(|_| PluginErrorPayload::internal("Could not create the HTTP client."))?;

    let mut builder = client.request(method, url);
    for (name, value) in headers {
        if forbidden_header(&name) {
            return Err(PluginErrorPayload::invalid(format!("The {name} header is not allowed.")));
        }
        let header_name = reqwest::header::HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| PluginErrorPayload::invalid("Invalid request header name."))?;
        let header_value = reqwest::header::HeaderValue::from_str(&value)
            .map_err(|_| PluginErrorPayload::invalid("Invalid request header value."))?;
        builder = builder.header(header_name, header_value);
    }
    if !body.is_empty() {
        builder = builder.body(body);
    }

    let mut response =
        builder.send().await.map_err(|_| PluginErrorPayload::new("network", "The request could not be completed."))?;
    let status = response.status().as_u16();
    let mut buffer: Vec<u8> = Vec::new();
    let mut truncated = false;
    while let Some(chunk) =
        response.chunk().await.map_err(|_| PluginErrorPayload::new("network", "The response could not be read."))?
    {
        let remaining = MAX_RESPONSE_BYTES.saturating_sub(buffer.len());
        if chunk.len() >= remaining {
            buffer.extend_from_slice(&chunk[..remaining]);
            truncated = true;
            break;
        }
        buffer.extend_from_slice(&chunk);
    }
    Ok(PluginHttpResponse { status, body: String::from_utf8_lossy(&buffer).to_string(), truncated })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("psak-plugin-test-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn valid_manifest() -> String {
        serde_json::json!({
            "id": "test.tool",
            "name": "Test Tool",
            "version": "1.0.0",
            "apiVersion": 1,
            "compatibility": { "app": ">=3.1.0" },
            "permissions": ["read_files", "write_files", "network"],
            "capabilities": ["command", "files", "network"],
            "commands": [{ "id": "run", "title": "Run" }]
        })
        .to_string()
    }

    #[test]
    fn plugin_install_writes_manifest_and_source() {
        let root = temp_root();
        let entry = install_into(&root, &valid_manifest(), "self.onPluginMessage = () => 1;").expect("install");
        assert_eq!(entry.manifest.get("id").and_then(Value::as_str), Some("test.tool"));
        assert!(root.join("test.tool").join("manifest.json").is_file());
        assert!(root.join("test.tool").join("main.js").is_file());
        let listed = list_plugins(&root);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].manifest.get("id").and_then(Value::as_str), Some("test.tool"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn plugin_install_rejects_bad_ids_and_traversal() {
        let root = temp_root();
        let ids =
            vec!["../evil".to_string(), "Test.Tool".to_string(), "a/b".to_string(), String::new(), "x".repeat(65)];
        for id in &ids {
            let manifest = valid_manifest().replace("test.tool", id);
            let result = install_into(&root, &manifest, "x");
            assert!(result.is_err(), "id {id} must be rejected");
        }
        assert!(!root.join("..").join("evil").exists());
        assert!(list_plugins(&root).is_empty());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn plugin_install_rejects_oversized_source_and_empty_manifests() {
        let root = temp_root();
        let big = "x".repeat(MAX_SOURCE_BYTES + 1);
        assert!(install_into(&root, &valid_manifest(), &big).is_err());
        assert!(install_into(&root, "not json", "x").is_err());
        assert!(install_into(&root, &valid_manifest(), "   ").is_err());
        assert!(install_into(&root, r#"{"name":"No id"}"#, "x").is_err());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn plugin_install_from_dir_requires_both_files() {
        let root = temp_root();
        let source = temp_root();
        assert!(install_from_dir(&root, &source).is_err());
        std::fs::write(source.join("manifest.json"), valid_manifest()).expect("manifest");
        assert!(install_from_dir(&root, &source).is_err());
        std::fs::write(source.join("main.js"), "x").expect("source");
        assert!(install_from_dir(&root, &source).is_ok());
        std::fs::remove_dir_all(&root).ok();
        std::fs::remove_dir_all(&source).ok();
    }

    #[test]
    fn plugin_list_skips_directories_whose_manifest_disagrees() {
        let root = temp_root();
        install_into(&root, &valid_manifest(), "x").expect("install");
        std::fs::create_dir_all(root.join("orphan")).expect("dir");
        std::fs::write(root.join("orphan").join("manifest.json"), valid_manifest()).expect("manifest");
        assert_eq!(list_plugins(&root).len(), 1);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn plugin_file_sandbox_rejects_traversal() {
        let root = temp_root();
        for name in [
            "../secret.txt",
            "..",
            "sub/dir.txt",
            "sub\\dir.txt",
            "C:\\Windows\\win.ini",
            "/etc/passwd",
            ".hidden",
            "name.",
        ] {
            assert!(sandbox_file(&root, "test.tool", name).is_err(), "{name} must be rejected");
        }
        let path = sandbox_file(&root, "test.tool", "notes.txt").expect("plain name");
        assert_eq!(path, root.join("test.tool").join("data").join("notes.txt"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn reparse_point_detection() {
        // A missing path is not a link, so a fresh sandbox file is unaffected.
        assert!(!is_reparse_point(Path::new("definitely-missing-omnioffice")));
        let dir = std::env::temp_dir().join(format!("omnioffice-reparse-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("plain.txt");
        std::fs::write(&file, "x").expect("write");
        assert!(!is_reparse_point(&file));
        #[cfg(unix)]
        {
            let link = dir.join("link.txt");
            let _ = std::fs::remove_file(&link);
            if std::os::unix::fs::symlink(&file, &link).is_ok() {
                assert!(is_reparse_point(&link), "a symlink must be flagged");
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn plugin_file_commands_require_permissions() {
        let root = temp_root();
        let manifest_without_files =
            valid_manifest().replace(r#""read_files","write_files","network""#, r#""network""#);
        install_into(&root, &manifest_without_files, "x").expect("install");
        let dir = root.join("test.tool");
        assert!(require_permission(&dir, "read_files").is_err());
        assert!(require_permission(&dir, "network").is_ok());
        let with_files = valid_manifest();
        install_into(&root, &with_files, "x").expect("reinstall");
        assert!(require_permission(&dir, "read_files").is_ok());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn plugin_delete_removes_only_the_plugin_dir() {
        let root = temp_root();
        install_into(&root, &valid_manifest(), "x").expect("install");
        delete_plugin(&root, "test.tool").expect("delete");
        assert!(!root.join("test.tool").exists());
        assert!(delete_plugin(&root, "test.tool").is_err());
        assert!(delete_plugin(&root, "../escape").is_err());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn plugin_sample_is_installable_and_read_only() {
        let root = temp_root();
        let entry = install_into(&root, SAMPLE_MANIFEST, SAMPLE_SOURCE).expect("sample install");
        assert_eq!(entry.manifest.get("id").and_then(Value::as_str), Some("sample.word-counter"));
        let permissions = entry.manifest.get("permissions").and_then(Value::as_array).cloned().unwrap_or_default();
        assert_eq!(permissions.len(), 1);
        assert_eq!(permissions[0].as_str(), Some("read_document"));
        assert!(SAMPLE_SOURCE.contains("self.host.doc.getText"));
        assert!(root.join("sample.word-counter").join("main.js").is_file());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn plugin_http_url_rules() {
        assert!(validate_http_url("https://example.com/api").is_ok());
        assert!(validate_http_url("http://example.com").is_err());
        assert!(validate_http_url("ftp://example.com").is_err());
        assert!(validate_http_url("https://user:pass@example.com").is_err());
        assert!(validate_http_url("http://localhost:8080/health").is_ok());
        assert!(validate_http_url("http://127.0.0.1:9/").is_ok());
        assert!(validate_http_url("http://192.168.1.5/").is_ok());
        assert!(validate_http_url("http://10.1.2.3/").is_ok());
        assert!(validate_http_url("http://172.16.0.1/").is_ok());
        assert!(validate_http_url("http://172.32.0.1/").is_err());
        // The plugin host may use a non-web port for a public https API.
        assert!(validate_http_url("https://example.com:8443/api").is_ok());
    }

    #[test]
    fn https_private_literal_destinations_are_refused() {
        for raw in [
            "https://127.0.0.1/",
            "https://[::1]/",
            "https://10.0.0.5/",
            "https://192.168.1.1/",
            "https://172.16.0.1/",
            "https://[fd00::1]/",
            "https://100.64.0.1/",
            "https://[::ffff:127.0.0.1]/",
            "https://169.254.169.254/latest/meta-data/",
        ] {
            assert!(validate_http_url(raw).is_err(), "{raw} must be refused over https");
        }
        // A public literal is still reachable over https.
        assert!(validate_http_url("https://8.8.8.8/").is_ok());
    }

    #[test]
    fn plain_http_private_exception_stays_available_but_metadata_is_not() {
        assert!(validate_http_url("http://127.0.0.1:8080/health").is_ok());
        for raw in
            ["http://169.254.169.254/latest/meta-data/", "http://[::ffff:169.254.169.254]/", "https://169.254.169.254/"]
        {
            assert!(validate_http_url(raw).is_err(), "{raw} must be refused");
        }
    }

    #[test]
    fn truncating_to_the_byte_limit_never_splits_a_character() {
        const MAX: usize = 256 * 1024;
        let mut text = "a".repeat(MAX - 1);
        text.push('ğ'); // two bytes straddling the limit
        text.push_str("tail");
        truncate_at_char_boundary(&mut text, MAX);
        assert!(text.len() <= MAX);
        assert!(text.is_char_boundary(text.len()));
        // The truncation stops before the character that would be split.
        assert!(text.chars().all(|ch| ch == 'a'));
    }
}
