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
//!   manifest's `network` permission (https only; plain http only for
//!   localhost and private addresses; GET/POST; 1 MB response cap; 15 s
//!   timeout; no redirects).
//!
//! Threat model: this protects documents and user data from buggy or
//! malicious plugins through the capability API. Worker isolation is
//! same-process and same-engine - it is not an escape from the browser
//! engine itself.
//!
//! The sample plugin ships inside the binary with `include_str!`, so the
//! "reload sample" action works in packaged builds with no resource paths.

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
const HTTP_TIMEOUT_SECS: u64 = 15;

const SAMPLE_MANIFEST: &str = include_str!("../../plugins/sample/manifest.json");
const SAMPLE_SOURCE: &str = include_str!("../../plugins/sample/main.js");

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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginEntry {
    /// The raw manifest as stored on disk; the frontend validates it strictly.
    pub manifest: Value,
    pub source_bytes: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginHttpRequest {
    pub url: String,
    pub method: Option<String>,
    pub headers: Option<std::collections::HashMap<String, String>>,
    pub body: Option<String>,
}

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
    let path = dir.join(name);
    if path.parent() != Some(dir.as_path()) {
        return Err(PluginErrorPayload::invalid("Invalid file name."));
    }
    Ok(path)
}

fn read_capped(path: &Path, max: usize, label: &str) -> PluginResult<Vec<u8>> {
    let metadata = std::fs::metadata(path).map_err(|_| PluginErrorPayload::not_found(format!("{label} was not found.")))?;
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
    officecore::io::write_atomic(path, text.as_bytes()).map_err(|error| {
        PluginErrorPayload::internal(format!("Could not write {label} ({}).", error.code))
    })
}

fn require_permission(dir: &Path, permission: &str) -> PluginResult<()> {
    let text = read_text(&dir.join("manifest.json"), MAX_MANIFEST_BYTES, "manifest.json")?;
    let manifest: Value = serde_json::from_str(&text).map_err(|_| PluginErrorPayload::invalid("manifest.json is not valid JSON."))?;
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
// Install / list / delete (all testable without a Tauri handle)
// ---------------------------------------------------------------------------

pub fn install_into(root: &Path, manifest_json: &str, source: &str) -> PluginResult<PluginEntry> {
    if manifest_json.len() > MAX_MANIFEST_BYTES {
        return Err(PluginErrorPayload::too_large(format!("The manifest is larger than {} KB.", MAX_MANIFEST_BYTES / 1024)));
    }
    if source.trim().is_empty() {
        return Err(PluginErrorPayload::invalid("main.js is empty."));
    }
    if source.len() > MAX_SOURCE_BYTES {
        return Err(PluginErrorPayload::too_large(format!("main.js is larger than {} KB.", MAX_SOURCE_BYTES / 1024)));
    }
    let manifest: Value = serde_json::from_str(manifest_json).map_err(|_| PluginErrorPayload::invalid("The manifest is not valid JSON."))?;
    let id = manifest
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| PluginErrorPayload::invalid("The manifest has no id."))?;
    if !validate_plugin_id(id) {
        return Err(PluginErrorPayload::invalid("Invalid plugin id."));
    }
    let dir = root.join(id);
    std::fs::create_dir_all(&dir).map_err(|_| PluginErrorPayload::internal("Could not create the plugin directory."))?;
    write_text(&dir.join("manifest.json"), manifest_json, "manifest.json")?;
    write_text(&dir.join("main.js"), source, "main.js")?;
    Ok(PluginEntry { manifest, source_bytes: source.len() as u64 })
}

pub fn install_from_dir(root: &Path, source_dir: &Path) -> PluginResult<PluginEntry> {
    if !source_dir.is_dir() {
        return Err(PluginErrorPayload::invalid("Choose a folder that contains manifest.json and main.js."));
    }
    let manifest_json = read_text(&source_dir.join("manifest.json"), MAX_MANIFEST_BYTES, "manifest.json")?;
    let source = read_text(&source_dir.join("main.js"), MAX_SOURCE_BYTES, "main.js")?;
    install_into(root, &manifest_json, &source)
}

pub fn list_plugins(root: &Path) -> Vec<PluginEntry> {
    let mut entries = Vec::new();
    let Ok(dir) = std::fs::read_dir(root) else {
        return entries;
    };
    for item in dir.flatten() {
        let path = item.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if !validate_plugin_id(name) {
            continue;
        }
        let Ok(manifest_json) = read_text(&path.join("manifest.json"), MAX_MANIFEST_BYTES, "manifest.json") else {
            continue;
        };
        let Ok(manifest) = serde_json::from_str::<Value>(&manifest_json) else {
            continue;
        };
        // The directory name and the manifest id must agree, otherwise two
        // plugins could claim the same identity.
        if manifest.get("id").and_then(Value::as_str) != Some(name) {
            continue;
        }
        let source_bytes = std::fs::metadata(path.join("main.js")).map(|meta| meta.len()).unwrap_or(0);
        entries.push(PluginEntry { manifest, source_bytes });
    }
    entries.sort_by(|a, b| {
        let left = a.manifest.get("id").and_then(Value::as_str).unwrap_or_default();
        let right = b.manifest.get("id").and_then(Value::as_str).unwrap_or_default();
        left.cmp(right)
    });
    entries
}

pub fn delete_plugin(root: &Path, id: &str) -> PluginResult<()> {
    let dir = plugin_dir(root, id)?;
    if !dir.is_dir() {
        return Err(PluginErrorPayload::not_found("The plugin is not installed."));
    }
    std::fs::remove_dir_all(&dir).map_err(|_| PluginErrorPayload::internal("Could not remove the plugin directory."))
}

// ---------------------------------------------------------------------------
// Network guard
// ---------------------------------------------------------------------------

fn is_private_host(host: Option<&str>) -> bool {
    let Some(host) = host else {
        return false;
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => ip.is_private() || ip.is_loopback() || ip.is_link_local(),
        Ok(std::net::IpAddr::V6(ip)) => ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local(),
        Err(_) => false,
    }
}

/// https always; http only for localhost/private addresses; no credentials in
/// the URL.
pub fn validate_http_url(raw: &str) -> PluginResult<reqwest::Url> {
    let url = reqwest::Url::parse(raw).map_err(|_| PluginErrorPayload::invalid("The URL is not valid."))?;
    match url.scheme() {
        "https" => {}
        "http" => {
            if !is_private_host(url.host_str()) {
                return Err(PluginErrorPayload::forbidden("Plain http is only allowed for localhost and private network addresses."));
            }
        }
        _ => return Err(PluginErrorPayload::forbidden("Only https (or private http) URLs are allowed.")),
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(PluginErrorPayload::invalid("Credentials in the URL are not allowed."));
    }
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
pub fn plugin_list(app: AppHandle) -> PluginResult<Vec<PluginEntry>> {
    Ok(list_plugins(&plugins_root(&app)?))
}

#[tauri::command]
pub fn plugin_read_source(app: AppHandle, id: String) -> PluginResult<String> {
    let root = plugins_root(&app)?;
    let dir = plugin_dir(&root, &id)?;
    read_text(&dir.join("main.js"), MAX_SOURCE_BYTES, "main.js")
}

#[tauri::command]
pub fn plugin_install(app: AppHandle, manifest_json: String, source: String) -> PluginResult<PluginEntry> {
    install_into(&plugins_root(&app)?, &manifest_json, &source)
}

#[tauri::command]
pub fn plugin_install_from_path(app: AppHandle, path: String) -> PluginResult<PluginEntry> {
    install_from_dir(&plugins_root(&app)?, Path::new(&path))
}

#[tauri::command]
pub fn plugin_install_sample(app: AppHandle) -> PluginResult<PluginEntry> {
    install_into(&plugins_root(&app)?, SAMPLE_MANIFEST, SAMPLE_SOURCE)
}

#[tauri::command]
pub fn plugin_delete(app: AppHandle, id: String) -> PluginResult<()> {
    delete_plugin(&plugins_root(&app)?, &id)
}

#[tauri::command]
pub fn plugin_file_read(app: AppHandle, plugin_id: String, name: String) -> PluginResult<String> {
    let root = plugins_root(&app)?;
    let dir = plugin_dir(&root, &plugin_id)?;
    require_permission(&dir, "read_files")?;
    let path = sandbox_file(&root, &plugin_id, &name)?;
    let bytes = read_capped(&path, MAX_FILE_BYTES, "The file")?;
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

#[tauri::command]
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

#[tauri::command]
pub async fn plugin_http_request(app: AppHandle, plugin_id: String, request: PluginHttpRequest) -> PluginResult<PluginHttpResponse> {
    let root = plugins_root(&app)?;
    let dir = plugin_dir(&root, &plugin_id)?;
    require_permission(&dir, "network")?;

    let url = validate_http_url(&request.url)?;
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
        return Err(PluginErrorPayload::too_large(format!("The request body is larger than {} KB.", MAX_REQUEST_BYTES / 1024)));
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("OfficeSwissArmyKnife/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| PluginErrorPayload::internal("Could not create the HTTP client."))?;

    let mut builder = client.request(method, url);
    for (name, value) in headers {
        if forbidden_header(&name) {
            return Err(PluginErrorPayload::invalid(format!("The {name} header is not allowed.")));
        }
        let header_name = reqwest::header::HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| PluginErrorPayload::invalid("Invalid request header name."))?;
        let header_value =
            reqwest::header::HeaderValue::from_str(&value).map_err(|_| PluginErrorPayload::invalid("Invalid request header value."))?;
        builder = builder.header(header_name, header_value);
    }
    if !body.is_empty() {
        builder = builder.body(body);
    }

    let mut response = builder.send().await.map_err(|_| PluginErrorPayload::new("network", "The request could not be completed."))?;
    let status = response.status().as_u16();
    let mut buffer: Vec<u8> = Vec::new();
    let mut truncated = false;
    while let Some(chunk) = response.chunk().await.map_err(|_| PluginErrorPayload::new("network", "The response could not be read."))? {
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
        let ids = vec!["../evil".to_string(), "Test.Tool".to_string(), "a/b".to_string(), String::new(), "x".repeat(65)];
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
        for name in ["../secret.txt", "..", "sub/dir.txt", "sub\\dir.txt", "C:\\Windows\\win.ini", "/etc/passwd", ".hidden", "name."] {
            assert!(sandbox_file(&root, "test.tool", name).is_err(), "{name} must be rejected");
        }
        let path = sandbox_file(&root, "test.tool", "notes.txt").expect("plain name");
        assert_eq!(path, root.join("test.tool").join("data").join("notes.txt"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn plugin_file_commands_require_permissions() {
        let root = temp_root();
        let manifest_without_files = valid_manifest().replace(r#""read_files","write_files","network""#, r#""network""#);
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
    }

    #[test]
    fn plugin_private_host_detection() {
        assert!(is_private_host(Some("localhost")));
        assert!(is_private_host(Some("[::1]")));
        assert!(is_private_host(Some("::1")));
        assert!(is_private_host(Some("127.0.0.1")));
        assert!(is_private_host(Some("192.168.0.10")));
        assert!(!is_private_host(Some("example.com")));
        assert!(!is_private_host(Some("8.8.8.8")));
        assert!(!is_private_host(None));
    }
}
