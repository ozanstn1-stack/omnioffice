//! OAuth cloud providers: Google Drive (v3) and Microsoft Graph (OneDrive).
//!
//! Both implement the superset of the [`SyncProvider`] surface that the Tauri
//! sync layer uses (`ensure_dir`, streaming `put_file`/`get_to_writer`,
//! `stage_download`) so the conflict rules stay identical across providers.
//!
//! Conditional writes:
//! * Microsoft Graph supports `If-Match` natively; a 412 becomes
//!   [`SyncError::Conflict`].
//! * Google Drive's v3 API compares versions server-side only for metadata
//!   updates; this client compares the stored `sha256Checksum` with the base
//!   before replacing content, which gives the same "never overwrite silently"
//!   guarantee at the cost of one metadata request.
//!
//! Access tokens are requested through [`TokenSource`] on every call, so the
//! application layer owns refresh timing and secure storage.

use crate::{RemoteEntry, SyncError, SyncProvider, MAX_TRANSFER_BYTES};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

/// Supplies a valid access token; the application layer refreshes it when
/// needed and reads it from the OS keychain.
pub trait TokenSource: Send + Sync {
    fn access_token(&self) -> Result<String, SyncError>;
}

/// A fixed token, used by tests and by one-shot operations.
pub struct StaticToken(pub String);

impl TokenSource for StaticToken {
    fn access_token(&self) -> Result<String, SyncError> {
        Ok(self.0.clone())
    }
}

fn build_client() -> Result<reqwest::blocking::Client, SyncError> {
    reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(600))
        .user_agent("PDFSwissArmyKnife/3.7 (synccore OAuth)")
        .build()
        .map_err(|error| SyncError::Internal(format!("could not build the HTTP client: {error}")))
}

/// Maps a non-success status to the shared error contract.
fn map_status(status: u16, context: &str) -> SyncError {
    match status {
        401 | 403 => SyncError::Auth(format!("{context}: HTTP {status}")),
        404 => SyncError::NotFound(context.to_string()),
        409 | 412 | 423 => SyncError::Conflict(format!("{context}: HTTP {status}")),
        413 => SyncError::TooLarge(MAX_TRANSFER_BYTES),
        429 => SyncError::Network(format!("{context}: rate limited (HTTP 429)")),
        _ => SyncError::Http { status, message: context.to_string() },
    }
}

fn network(context: &str, error: reqwest::Error) -> SyncError {
    SyncError::Network(format!("{context}: {error}"))
}

fn normalize_remote_dir(dir: &str) -> Result<Vec<String>, SyncError> {
    let mut segments = Vec::new();
    for raw in dir.split('/') {
        let segment = raw.trim();
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." || segment.contains('\\') || segment.contains('\0') {
            return Err(SyncError::InvalidInput(format!("invalid remote folder: {dir}")));
        }
        segments.push(segment.to_string());
    }
    if segments.len() > 16 {
        return Err(SyncError::InvalidInput("the remote folder is nested too deeply".into()));
    }
    Ok(segments)
}

fn normalize_file_name(name: &str) -> Result<String, SyncError> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." || trimmed.contains('/') || trimmed.contains('\\') {
        return Err(SyncError::InvalidInput(format!("invalid remote file name: {name}")));
    }
    Ok(trimmed.to_string())
}

fn hash_reader<R: Read>(reader: &mut R, writer: &mut dyn Write, context: &str) -> Result<(String, u64), SyncError> {
    let mut hasher = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer).map_err(|error| SyncError::Io(format!("{context}: {error}")))?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > MAX_TRANSFER_BYTES {
            return Err(SyncError::TooLarge(total));
        }
        hasher.update(&buffer[..read]);
        writer.write_all(&buffer[..read]).map_err(|error| SyncError::Io(format!("{context}: {error}")))?;
    }
    Ok((crate::metadata::hex_lower(&hasher.finalize()), total))
}

// ---------------------------------------------------------------------------
// Google Drive
// ---------------------------------------------------------------------------

const GOOGLE_API: &str = "https://www.googleapis.com";
const GOOGLE_FOLDER_MIME: &str = "application/vnd.google-apps.folder";

/// Google Drive v3 provider rooted at a folder path inside "My Drive".
pub struct GoogleDriveProvider {
    tokens: Box<dyn TokenSource>,
    api_base: String,
    client: reqwest::blocking::Client,
    /// Cache of folder path -> Drive folder id.
    folders: Mutex<HashMap<String, String>>,
}

impl GoogleDriveProvider {
    pub fn new(tokens: Box<dyn TokenSource>, remote_dir: &str) -> Result<Self, SyncError> {
        Self::new_with_base(tokens, remote_dir, GOOGLE_API)
    }

    pub fn new_with_base(tokens: Box<dyn TokenSource>, remote_dir: &str, api_base: &str) -> Result<Self, SyncError> {
        // The root folder is resolved per call (folder ids are not stable
        // across accounts), but the path is validated up front.
        normalize_remote_dir(remote_dir)?;
        Ok(Self {
            tokens,
            api_base: api_base.trim_end_matches('/').to_string(),
            client: build_client()?,
            folders: Mutex::new(HashMap::new()),
        })
    }

    fn token(&self) -> Result<String, SyncError> {
        self.tokens.access_token()
    }

    fn files_url(&self, path: &str) -> String {
        format!("{}/drive/v3/{}", self.api_base, path.trim_start_matches('/'))
    }

    fn upload_url(&self, path: &str) -> String {
        format!("{}/upload/drive/v3/{}", self.api_base, path.trim_start_matches('/'))
    }

    /// Runs a JSON request and returns the parsed body with the status.
    fn json_request(
        &self,
        method: reqwest::Method,
        url: &str,
        token: &str,
        body: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, SyncError> {
        let mut request = self.client.request(method.clone(), url).bearer_auth(token);
        if let Some(body) = &body {
            request = request.json(body);
        }
        let response = request.send().map_err(|error| network("Google Drive request failed", error))?;
        let status = response.status().as_u16();
        if !response.status().is_success() {
            return Err(map_status(status, "Google Drive request failed"));
        }
        if status == 204 {
            return Ok(serde_json::Value::Null);
        }
        let text = response.text().unwrap_or_default();
        if text.trim().is_empty() {
            return Ok(serde_json::Value::Null);
        }
        serde_json::from_str(&text).map_err(|error| SyncError::Protocol(format!("invalid Drive response: {error}")))
    }

    /// Finds (or creates) the folder chain and returns the deepest folder id.
    fn folder_id(&self, dir: &str) -> Result<String, SyncError> {
        let segments = normalize_remote_dir(dir)?;
        if segments.is_empty() {
            return Ok("root".to_string());
        }
        let key = segments.join("/");
        if let Some(cached) = self.folders.lock().map_err(|_| SyncError::Internal("lock".into()))?.get(&key) {
            return Ok(cached.clone());
        }
        let token = self.token()?;
        let mut parent = "root".to_string();
        let mut prefix = String::new();
        for segment in segments {
            prefix = if prefix.is_empty() { segment.clone() } else { format!("{prefix}/{segment}") };
            if let Some(cached) = self.folders.lock().map_err(|_| SyncError::Internal("lock".into()))?.get(&prefix) {
                parent = cached.clone();
                continue;
            }
            let query = format!(
                "name = '{}' and mimeType = '{GOOGLE_FOLDER_MIME}' and '{}' in parents and trashed = false",
                segment.replace('\'', "\\'"),
                parent
            );
            let url = self.files_url("files");
            let response = self
                .client
                .get(&url)
                .query(&[("q", query.as_str()), ("fields", "files(id,name)"), ("pageSize", "10"), ("spaces", "drive")])
                .bearer_auth(&token)
                .send()
                .map_err(|error| network("Google Drive folder lookup failed", error))?;
            if !response.status().is_success() {
                return Err(map_status(response.status().as_u16(), "Google Drive folder lookup failed"));
            }
            let body: serde_json::Value = response
                .json()
                .map_err(|error| SyncError::Protocol(format!("invalid Drive folder response: {error}")))?;
            let found = body
                .get("files")
                .and_then(|value| value.as_array())
                .and_then(|files| files.first())
                .and_then(|file| file.get("id"))
                .and_then(|value| value.as_str())
                .map(str::to_string);
            let id =
                match found {
                    Some(id) => id,
                    None => {
                        let created = self.json_request(
                            reqwest::Method::POST,
                            &self.files_url("files"),
                            &token,
                            Some(serde_json::json!({
                                "name": segment,
                                "mimeType": GOOGLE_FOLDER_MIME,
                                "parents": [parent],
                            })),
                        )?;
                        created.get("id").and_then(|value| value.as_str()).map(str::to_string).ok_or_else(|| {
                            SyncError::Protocol("Google Drive did not return the new folder id".into())
                        })?
                    }
                };
            self.folders.lock().map_err(|_| SyncError::Internal("lock".into()))?.insert(prefix.clone(), id.clone());
            parent = id;
        }
        Ok(parent)
    }

    /// Finds a file id inside a folder, or `None`.
    fn find_file(&self, dir: &str, name: &str) -> Result<Option<(String, Option<String>, u64)>, SyncError> {
        let name = normalize_file_name(name)?;
        let folder = self.folder_id(dir)?;
        let token = self.token()?;
        let query = format!("name = '{}' and '{}' in parents and trashed = false", name.replace('\'', "\\'"), folder);
        let response = self
            .client
            .get(self.files_url("files"))
            .query(&[
                ("q", query.as_str()),
                ("fields", "files(id,name,size,sha256Checksum,mimeType)"),
                ("pageSize", "10"),
                ("spaces", "drive"),
            ])
            .bearer_auth(&token)
            .send()
            .map_err(|error| network("Google Drive file lookup failed", error))?;
        let status = response.status().as_u16();
        if !response.status().is_success() {
            return Err(map_status(status, "Google Drive file lookup failed"));
        }
        let body: serde_json::Value =
            response.json().map_err(|error| SyncError::Protocol(format!("invalid Drive file response: {error}")))?;
        let Some(file) = body.get("files").and_then(|value| value.as_array()).and_then(|files| files.first()) else {
            return Ok(None);
        };
        let id = file.get("id").and_then(|value| value.as_str()).unwrap_or_default().to_string();
        let checksum = file.get("sha256Checksum").and_then(|value| value.as_str()).map(str::to_string);
        let size = file.get("size").and_then(|value| value.as_str()).and_then(|value| value.parse().ok()).unwrap_or(0);
        Ok(Some((id, checksum, size)))
    }

    /// Reads a remote file into memory (bounded), hashing while it streams.
    fn download(&self, path: &str) -> Result<(Vec<u8>, String, u64, Option<String>), SyncError> {
        let (dir, name) = split_remote_path(path)?;
        let (id, checksum, _) = self.find_file(&dir, &name)?.ok_or_else(|| SyncError::NotFound(path.to_string()))?;
        let token = self.token()?;
        let response = self
            .client
            .get(self.files_url(&format!("files/{id}")))
            .query(&[("alt", "media")])
            .bearer_auth(&token)
            .send()
            .map_err(|error| network("Google Drive download failed", error))?;
        let status = response.status().as_u16();
        if !response.status().is_success() {
            return Err(map_status(status, "Google Drive download failed"));
        }
        let mut bytes = Vec::new();
        let mut reader = response;
        let (sha, size) = hash_reader(&mut reader, &mut bytes, "Google Drive download")?;
        Ok((bytes, sha, size, checksum))
    }

    /// Uploads a local file with the conflict rule described in the module doc.
    fn upload(
        &self,
        path: &str,
        local: &Path,
        if_match: Option<&str>,
    ) -> Result<(String, u64, Option<String>), SyncError> {
        let (dir, name) = split_remote_path(path)?;
        let (sha256, size) = crate::metadata::hash_file(local)?;
        if size > MAX_TRANSFER_BYTES {
            return Err(SyncError::TooLarge(size));
        }
        let folder = self.folder_id(&dir)?;
        let token = self.token()?;
        let existing = self.find_file(&dir, &name)?;
        let file_id = match existing {
            Some((id, checksum, _)) => {
                if let (Some(expected), Some(current)) = (if_match, checksum.as_deref()) {
                    if expected != current {
                        return Err(SyncError::Conflict(format!("{name} changed in Google Drive since the last sync")));
                    }
                }
                id
            }
            None => {
                let created = self.json_request(
                    reqwest::Method::POST,
                    &self.files_url("files"),
                    &token,
                    Some(serde_json::json!({ "name": name, "parents": [folder] })),
                )?;
                created
                    .get("id")
                    .and_then(|value| value.as_str())
                    .map(str::to_string)
                    .ok_or_else(|| SyncError::Protocol("Google Drive did not return the new file id".into()))?
            }
        };
        let file = std::fs::File::open(local).map_err(SyncError::from)?;
        let response = self
            .client
            .patch(self.upload_url(&format!("files/{file_id}")))
            .query(&[("uploadType", "media")])
            .bearer_auth(&token)
            .header("Content-Type", "application/octet-stream")
            .header("Content-Length", size.to_string())
            .body(reqwest::blocking::Body::sized(file, size))
            .send()
            .map_err(|error| network("Google Drive upload failed", error))?;
        let status = response.status().as_u16();
        if !response.status().is_success() {
            return Err(map_status(status, "Google Drive upload failed"));
        }
        Ok((sha256, size, None))
    }

    fn remove(&self, path: &str) -> Result<(), SyncError> {
        let (dir, name) = split_remote_path(path)?;
        let (id, _, _) = self.find_file(&dir, &name)?.ok_or_else(|| SyncError::NotFound(path.to_string()))?;
        let token = self.token()?;
        let response = self
            .client
            .delete(self.files_url(&format!("files/{id}")))
            .bearer_auth(&token)
            .send()
            .map_err(|error| network("Google Drive delete failed", error))?;
        let status = response.status().as_u16();
        if !response.status().is_success() && status != 404 {
            return Err(map_status(status, "Google Drive delete failed"));
        }
        Ok(())
    }
}

fn split_remote_path(path: &str) -> Result<(String, String), SyncError> {
    let trimmed = path.trim().trim_matches('/');
    let (dir, name) = match trimmed.rsplit_once('/') {
        Some((dir, name)) => (dir.to_string(), name.to_string()),
        None => (String::new(), trimmed.to_string()),
    };
    Ok((dir, normalize_file_name(&name)?))
}

impl SyncProvider for GoogleDriveProvider {
    fn test(&self) -> Result<String, SyncError> {
        let token = self.token()?;
        let url = self.files_url("about");
        let response = self
            .client
            .get(&url)
            .query(&[("fields", "user(emailAddress,displayName),storageQuota(limit)")])
            .bearer_auth(&token)
            .send()
            .map_err(|error| network("Google Drive could not be reached", error))?;
        let status = response.status().as_u16();
        if !response.status().is_success() {
            return Err(map_status(status, "Google Drive sign-in was rejected"));
        }
        let body: serde_json::Value = response.json().unwrap_or(serde_json::Value::Null);
        let email = body
            .get("user")
            .and_then(|user| user.get("emailAddress"))
            .and_then(|value| value.as_str())
            .unwrap_or("connected");
        Ok(format!("Google Drive ({email})"))
    }

    fn list(&self, remote_dir: &str) -> Result<Vec<RemoteEntry>, SyncError> {
        let folder = self.folder_id(remote_dir)?;
        let token = self.token()?;
        let query = format!("'{folder}' in parents and trashed = false");
        let response = self
            .client
            .get(self.files_url("files"))
            .query(&[
                ("q", query.as_str()),
                ("fields", "files(id,name,size,sha256Checksum,mimeType,modifiedTime)"),
                ("pageSize", "1000"),
                ("spaces", "drive"),
            ])
            .bearer_auth(&token)
            .send()
            .map_err(|error| network("Google Drive list failed", error))?;
        let status = response.status().as_u16();
        if !response.status().is_success() {
            return Err(map_status(status, "Google Drive list failed"));
        }
        let body: serde_json::Value =
            response.json().map_err(|error| SyncError::Protocol(format!("invalid Drive list response: {error}")))?;
        let mut entries = Vec::new();
        if let Some(files) = body.get("files").and_then(|value| value.as_array()) {
            for file in files {
                let name = file.get("name").and_then(|value| value.as_str()).unwrap_or_default().to_string();
                if name.is_empty() {
                    continue;
                }
                let mime = file.get("mimeType").and_then(|value| value.as_str()).unwrap_or_default();
                let size =
                    file.get("size").and_then(|value| value.as_str()).and_then(|value| value.parse().ok()).unwrap_or(0);
                entries.push(RemoteEntry {
                    name,
                    size,
                    etag: file.get("sha256Checksum").and_then(|value| value.as_str()).map(str::to_string),
                    modified: file.get("modifiedTime").and_then(|value| value.as_str()).map(str::to_string),
                    is_dir: mime == GOOGLE_FOLDER_MIME,
                });
            }
        }
        Ok(entries)
    }

    fn get(&self, path: &str) -> Result<(Vec<u8>, Option<String>), SyncError> {
        let (bytes, _, _, etag) = self.download(path)?;
        Ok((bytes, etag))
    }

    fn put(&self, path: &str, bytes: &[u8], if_match_etag: Option<&str>) -> Result<Option<String>, SyncError> {
        if bytes.len() as u64 > MAX_TRANSFER_BYTES {
            return Err(SyncError::TooLarge(bytes.len() as u64));
        }
        // The trait takes bytes; the streaming path (`put_file`) is used by the
        // sync layer. Stage to a temporary file so the two share one code path.
        let dir = std::env::temp_dir();
        let temp = dir.join(format!("osak-put-{}.tmp", uuid::Uuid::new_v4()));
        std::fs::write(&temp, bytes).map_err(SyncError::from)?;
        let result = self.upload(path, &temp, if_match_etag);
        let _ = std::fs::remove_file(&temp);
        result.map(|(_, _, etag)| etag)
    }

    fn delete(&self, path: &str) -> Result<(), SyncError> {
        self.remove(path)
    }
}

// ---------------------------------------------------------------------------
// Microsoft Graph (OneDrive)
// ---------------------------------------------------------------------------

const GRAPH_API: &str = "https://graph.microsoft.com/v1.0";

/// OneDrive provider rooted at a folder path inside the signed-in drive.
pub struct MicrosoftGraphProvider {
    tokens: Box<dyn TokenSource>,
    remote_dir: String,
    api_base: String,
    client: reqwest::blocking::Client,
}

impl MicrosoftGraphProvider {
    pub fn new(tokens: Box<dyn TokenSource>, remote_dir: &str) -> Result<Self, SyncError> {
        Self::new_with_base(tokens, remote_dir, GRAPH_API)
    }

    pub fn new_with_base(tokens: Box<dyn TokenSource>, remote_dir: &str, api_base: &str) -> Result<Self, SyncError> {
        normalize_remote_dir(remote_dir)?;
        Ok(Self {
            tokens,
            remote_dir: remote_dir.trim().trim_matches('/').to_string(),
            api_base: api_base.trim_end_matches('/').to_string(),
            client: build_client()?,
        })
    }

    fn token(&self) -> Result<String, SyncError> {
        self.tokens.access_token()
    }

    /// Graph addressing: folders are `/me/drive/root:/<path>:` and children
    /// hang off that with `/children`; the root has no colon form.
    fn item_url(&self, path: &str, suffix: &str) -> String {
        let path = path.trim().trim_matches('/');
        if path.is_empty() {
            format!("{}/me/drive/root{suffix}", self.api_base)
        } else {
            format!("{}/me/drive/root:/{path}:{suffix}", self.api_base)
        }
    }

    fn children(&self, path: &str) -> Result<Vec<serde_json::Value>, SyncError> {
        let token = self.token()?;
        let url = self.item_url(path, "/children");
        let response = self
            .client
            .get(&url)
            .query(&[("$select", "id,name,size,eTag,file,folder,lastModifiedDateTime")])
            .bearer_auth(&token)
            .send()
            .map_err(|error| network("OneDrive list failed", error))?;
        let status = response.status().as_u16();
        if status == 404 {
            return Ok(Vec::new());
        }
        if !response.status().is_success() {
            return Err(map_status(status, "OneDrive list failed"));
        }
        let body: serde_json::Value =
            response.json().map_err(|error| SyncError::Protocol(format!("invalid Graph response: {error}")))?;
        Ok(body.get("value").and_then(|value| value.as_array()).cloned().unwrap_or_default())
    }

    fn download(&self, path: &str) -> Result<(Vec<u8>, String, u64, Option<String>), SyncError> {
        let token = self.token()?;
        let url = self.item_url(path, "/content");
        let response = self
            .client
            .get(&url)
            .bearer_auth(&token)
            .send()
            .map_err(|error| network("OneDrive download failed", error))?;
        let status = response.status().as_u16();
        if !response.status().is_success() {
            return Err(map_status(status, "OneDrive download failed"));
        }
        let etag = response.headers().get("etag").and_then(|value| value.to_str().ok()).map(normalize_etag);
        let mut bytes = Vec::new();
        let mut reader = response;
        let (sha, size) = hash_reader(&mut reader, &mut bytes, "OneDrive download")?;
        Ok((bytes, sha, size, etag))
    }

    fn upload(
        &self,
        path: &str,
        local: &Path,
        if_match: Option<&str>,
    ) -> Result<(String, u64, Option<String>), SyncError> {
        let (sha256, size) = crate::metadata::hash_file(local)?;
        // Graph's simple upload handles up to 250 MB; above that an upload
        // session would be needed and the transfer cap already refuses it.
        const SIMPLE_UPLOAD_LIMIT: u64 = 250 * 1024 * 1024;
        if size > SIMPLE_UPLOAD_LIMIT {
            return Err(SyncError::TooLarge(size));
        }
        self.ensure_dir(&self.remote_dir)?;
        let token = self.token()?;
        let url = self.item_url(path, "/content");
        let file = std::fs::File::open(local).map_err(SyncError::from)?;
        let mut request = self
            .client
            .put(&url)
            .bearer_auth(&token)
            .header("Content-Type", "application/octet-stream")
            .header("Content-Length", size.to_string())
            .body(reqwest::blocking::Body::sized(file, size));
        if let Some(etag) = if_match {
            request = request.header("If-Match", quote_etag(etag));
        }
        let response = request.send().map_err(|error| network("OneDrive upload failed", error))?;
        let status = response.status().as_u16();
        if !response.status().is_success() {
            return Err(map_status(status, "OneDrive upload failed"));
        }
        let etag = response.headers().get("etag").and_then(|value| value.to_str().ok()).map(normalize_etag);
        Ok((sha256, size, etag))
    }
}

fn normalize_etag(raw: &str) -> String {
    raw.trim().trim_start_matches("W/").trim_matches('"').to_string()
}

fn quote_etag(etag: &str) -> String {
    format!("\"{}\"", etag.trim_matches('"'))
}

impl SyncProvider for MicrosoftGraphProvider {
    fn test(&self) -> Result<String, SyncError> {
        let token = self.token()?;
        let response = self
            .client
            .get(format!("{}/me", self.api_base))
            .query(&[("$select", "displayName,userPrincipalName")])
            .bearer_auth(&token)
            .send()
            .map_err(|error| network("OneDrive could not be reached", error))?;
        let status = response.status().as_u16();
        if !response.status().is_success() {
            return Err(map_status(status, "OneDrive sign-in was rejected"));
        }
        let body: serde_json::Value = response.json().unwrap_or(serde_json::Value::Null);
        let name = body
            .get("displayName")
            .or_else(|| body.get("userPrincipalName"))
            .and_then(|value| value.as_str())
            .unwrap_or("connected");
        Ok(format!("OneDrive ({name})"))
    }

    fn list(&self, remote_dir: &str) -> Result<Vec<RemoteEntry>, SyncError> {
        let mut entries = Vec::new();
        for item in self.children(remote_dir)? {
            let name = item.get("name").and_then(|value| value.as_str()).unwrap_or_default().to_string();
            if name.is_empty() {
                continue;
            }
            let is_dir = item.get("folder").is_some();
            let size = item.get("size").and_then(|value| value.as_u64()).unwrap_or(0);
            entries.push(RemoteEntry {
                name,
                size,
                etag: item.get("eTag").and_then(|value| value.as_str()).map(normalize_etag),
                modified: item.get("lastModifiedDateTime").and_then(|value| value.as_str()).map(str::to_string),
                is_dir,
            });
        }
        Ok(entries)
    }

    fn get(&self, path: &str) -> Result<(Vec<u8>, Option<String>), SyncError> {
        let (bytes, _, _, etag) = self.download(path)?;
        Ok((bytes, etag))
    }

    fn put(&self, path: &str, bytes: &[u8], if_match_etag: Option<&str>) -> Result<Option<String>, SyncError> {
        let dir = std::env::temp_dir();
        let temp = dir.join(format!("osak-put-{}.tmp", uuid::Uuid::new_v4()));
        std::fs::write(&temp, bytes).map_err(SyncError::from)?;
        let result = self.upload(path, &temp, if_match_etag);
        let _ = std::fs::remove_file(&temp);
        result.map(|(_, _, etag)| etag)
    }

    fn delete(&self, path: &str) -> Result<(), SyncError> {
        let token = self.token()?;
        let response = self
            .client
            .delete(self.item_url(path, ""))
            .bearer_auth(&token)
            .send()
            .map_err(|error| network("OneDrive delete failed", error))?;
        let status = response.status().as_u16();
        if !response.status().is_success() && status != 404 {
            return Err(map_status(status, "OneDrive delete failed"));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Superset surface used by the Tauri sync layer (matching WebDavProvider)
// ---------------------------------------------------------------------------

/// The provider operations the sync layer needs beyond the trait, shared by
/// the OAuth providers so `sync.rs` can treat them like WebDAV.
pub trait CloudProviderExt: SyncProvider {
    fn ensure_dir(&self, remote_dir: &str) -> Result<(), SyncError>;
    fn get_to_writer(&self, path: &str, writer: &mut dyn Write) -> Result<(String, u64, Option<String>), SyncError>;
    fn stage_download(&self, path: &str, local: &Path) -> Result<(PathBuf, String, u64, Option<String>), SyncError>;
    fn put_file(
        &self,
        path: &str,
        local: &Path,
        if_match_etag: Option<&str>,
    ) -> Result<(String, u64, Option<String>), SyncError>;
}

impl CloudProviderExt for GoogleDriveProvider {
    fn ensure_dir(&self, remote_dir: &str) -> Result<(), SyncError> {
        self.folder_id(remote_dir).map(|_| ())
    }

    fn get_to_writer(&self, path: &str, writer: &mut dyn Write) -> Result<(String, u64, Option<String>), SyncError> {
        let (bytes, sha, size, etag) = self.download(path)?;
        writer.write_all(&bytes).map_err(|error| SyncError::Io(error.to_string()))?;
        Ok((sha, size, etag))
    }

    fn stage_download(&self, path: &str, local: &Path) -> Result<(PathBuf, String, u64, Option<String>), SyncError> {
        let (_, name) = split_remote_path(path)?;
        let parent = local.parent().unwrap_or_else(|| Path::new("."));
        let staged = parent.join(format!(".{}.{}.part", name, uuid::Uuid::new_v4()));
        let file = std::fs::File::create(&staged).map_err(SyncError::from)?;
        let mut writer = std::io::BufWriter::new(file);
        let (sha, size, etag) = match self.get_to_writer(path, &mut writer) {
            Ok(result) => result,
            Err(error) => {
                let _ = std::fs::remove_file(&staged);
                return Err(error);
            }
        };
        writer.flush().map_err(SyncError::from)?;
        drop(writer);
        Ok((staged, sha, size, etag))
    }

    fn put_file(
        &self,
        path: &str,
        local: &Path,
        if_match_etag: Option<&str>,
    ) -> Result<(String, u64, Option<String>), SyncError> {
        self.upload(path, local, if_match_etag)
    }
}

impl CloudProviderExt for MicrosoftGraphProvider {
    /// Creates every folder segment with `conflictBehavior: fail` and tolerates
    /// 409 (already exists), which is one request per segment and avoids the
    /// "empty folder vs missing folder" ambiguity of a listing probe.
    fn ensure_dir(&self, remote_dir: &str) -> Result<(), SyncError> {
        let segments = normalize_remote_dir(remote_dir)?;
        let mut prefix = String::new();
        for segment in segments {
            prefix = if prefix.is_empty() { segment.clone() } else { format!("{prefix}/{segment}") };
            let token = self.token()?;
            let parent = prefix.rsplit_once('/').map(|(parent, _)| parent.to_string()).unwrap_or_default();
            let url = self.item_url(&parent, "/children");
            let response = self
                .client
                .post(&url)
                .bearer_auth(&token)
                .json(&serde_json::json!({
                    "name": segment,
                    "folder": {},
                    "@microsoft.graph.conflictBehavior": "fail",
                }))
                .send()
                .map_err(|error| network("OneDrive folder creation failed", error))?;
            let status = response.status().as_u16();
            // 409 = already exists, which is fine for ensure_dir.
            if !response.status().is_success() && status != 409 {
                return Err(map_status(status, "OneDrive folder creation failed"));
            }
        }
        Ok(())
    }

    fn get_to_writer(&self, path: &str, writer: &mut dyn Write) -> Result<(String, u64, Option<String>), SyncError> {
        let (bytes, sha, size, etag) = self.download(path)?;
        writer.write_all(&bytes).map_err(|error| SyncError::Io(error.to_string()))?;
        Ok((sha, size, etag))
    }

    fn stage_download(&self, path: &str, local: &Path) -> Result<(PathBuf, String, u64, Option<String>), SyncError> {
        let (_, name) = split_remote_path(path)?;
        let parent = local.parent().unwrap_or_else(|| Path::new("."));
        let staged = parent.join(format!(".{}.{}.part", name, uuid::Uuid::new_v4()));
        let (bytes, sha, size, etag) = self.download(path)?;
        if let Err(error) = std::fs::write(&staged, &bytes) {
            let _ = std::fs::remove_file(&staged);
            return Err(SyncError::from(error));
        }
        Ok((staged, sha, size, etag))
    }

    fn put_file(
        &self,
        path: &str,
        local: &Path,
        if_match_etag: Option<&str>,
    ) -> Result<(String, u64, Option<String>), SyncError> {
        self.upload(path, local, if_match_etag)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread::JoinHandle;

    /// One canned HTTP response for the mock server.
    struct MockResponse {
        status: u16,
        body: String,
        headers: Vec<(&'static str, &'static str)>,
    }

    fn response(status: u16, body: &str) -> MockResponse {
        MockResponse { status, body: body.to_string(), headers: Vec::new() }
    }

    /// Serves the responses in order, one connection each, and returns the
    /// request lines it saw. Enough for the blocking reqwest client.
    fn start_mock(responses: Vec<MockResponse>) -> (String, JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("mock bind");
        let address = format!("http://127.0.0.1:{}", listener.local_addr().expect("addr").port());
        let handle = std::thread::spawn(move || {
            let mut records = Vec::new();
            for response in responses {
                let (mut stream, _) = listener.accept().expect("mock accept");
                let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                let mut buffer = [0u8; 8192];
                let read = stream.read(&mut buffer).unwrap_or(0);
                let text = String::from_utf8_lossy(&buffer[..read]).to_string();
                let line = text.lines().next().unwrap_or_default().to_string();
                // Drain a request body (PUT/POST) so the client can finish.
                let mut content_length = 0usize;
                for header in text.lines() {
                    if let Some((key, value)) = header.split_once(':') {
                        if key.eq_ignore_ascii_case("content-length") {
                            content_length = value.trim().parse().unwrap_or(0);
                        }
                    }
                }
                let header_end = text.find("\r\n\r\n").map(|index| index + 4).unwrap_or(read);
                let mut remaining = content_length.saturating_sub(read.saturating_sub(header_end));
                let mut trash = [0u8; 64 * 1024];
                while remaining > 0 {
                    let count = stream.read(&mut trash).unwrap_or(0);
                    if count == 0 {
                        break;
                    }
                    remaining = remaining.saturating_sub(count);
                }
                records.push(line);
                let mut raw = format!(
                    "HTTP/1.1 {} MOCK\r\nContent-Length: {}\r\nConnection: close\r\n",
                    response.status,
                    response.body.len()
                );
                for (key, value) in &response.headers {
                    raw.push_str(&format!("{key}: {value}\r\n"));
                }
                raw.push_str("\r\n");
                raw.push_str(&response.body);
                let _ = stream.write_all(raw.as_bytes());
            }
            records
        });
        (address, handle)
    }

    fn temp_file(text: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("a.oswk");
        std::fs::write(&path, text).expect("write");
        (dir, path)
    }

    #[test]
    fn microsoft_graph_roundtrip_against_a_mock_drive() {
        let (base, server) = start_mock(vec![
            // test()
            response(200, r#"{"displayName":"Ada"}"#),
            // ensure_dir("Sync") -> POST /me/drive/root/children
            response(201, r#"{"id":"folder-1"}"#),
            // PUT content
            MockResponse { status: 201, body: r#"{"id":"file-1"}"#.into(), headers: vec![("ETag", "\"etag-1\"")] },
            // list()
            response(
                200,
                r#"{"value":[{"name":"a.oswk","size":4,"eTag":"etag-1","file":{"mimeType":"application/octet-stream"}}]}"#,
            ),
            // get()
            MockResponse { status: 200, body: "data".into(), headers: vec![("ETag", "\"etag-1\"")] },
            // delete()
            response(204, ""),
        ]);
        let provider = MicrosoftGraphProvider::new_with_base(Box::new(StaticToken("token-1".into())), "Sync", &base)
            .expect("provider");

        assert!(provider.test().expect("test").starts_with("OneDrive (Ada)"));
        let (_dir, file) = temp_file("data");
        let (sha, size, etag) = provider.put_file("Sync/a.oswk", &file, None).expect("put");
        assert_eq!(size, 4);
        assert!(!sha.is_empty());
        assert_eq!(etag.as_deref(), Some("etag-1"));

        let entries = provider.list("Sync").expect("list");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "a.oswk");
        assert_eq!(entries[0].etag.as_deref(), Some("etag-1"));
        assert!(!entries[0].is_dir);

        let (bytes, etag) = provider.get("Sync/a.oswk").expect("get");
        assert_eq!(bytes, b"data");
        assert_eq!(etag.as_deref(), Some("etag-1"));
        provider.delete("Sync/a.oswk").expect("delete");

        let requests = server.join().expect("server");
        assert!(requests[0].starts_with("GET /me"), "{}", requests[0]);
        assert!(requests[1].starts_with("POST /me/drive/root/children"), "{}", requests[1]);
        assert!(requests[2].starts_with("PUT /me/drive/root:/Sync/a.oswk:/content"), "{}", requests[2]);
        assert!(requests[3].starts_with("GET /me/drive/root:/Sync:/children"), "{}", requests[3]);
        assert!(requests[4].starts_with("GET /me/drive/root:/Sync/a.oswk:/content"), "{}", requests[4]);
        assert!(requests[5].starts_with("DELETE /me/drive/root:/Sync/a.oswk"), "{}", requests[5]);
    }

    #[test]
    fn microsoft_graph_conflict_is_reported_from_if_match() {
        let (base, _server) = start_mock(vec![
            // ensure_dir("Sync")
            response(201, r#"{"id":"folder-1"}"#),
            // PUT refused with 412
            response(412, r#"{"error":{"code":"preconditionFailed"}}"#),
        ]);
        let provider = MicrosoftGraphProvider::new_with_base(Box::new(StaticToken("token-1".into())), "Sync", &base)
            .expect("provider");
        let (_dir, file) = temp_file("data");
        let error = provider.put_file("Sync/a.oswk", &file, Some("etag-old")).expect_err("must conflict");
        assert!(matches!(error, SyncError::Conflict(_)), "{error}");
    }

    #[test]
    fn google_drive_roundtrip_against_a_mock_drive() {
        let (base, server) = start_mock(vec![
            // test()
            response(200, r#"{"user":{"emailAddress":"ada@example.com"}}"#),
            // list(): folder lookup
            response(200, r#"{"files":[{"id":"folder-1","name":"Sync"}]}"#),
            // list(): children
            response(
                200,
                r#"{"files":[{"name":"a.oswk","size":"4","sha256Checksum":"abc123","mimeType":"application/octet-stream"}]}"#,
            ),
            // get(): file lookup
            response(200, r#"{"files":[{"id":"file-1","name":"a.oswk","size":"4","sha256Checksum":"abc123"}]}"#),
            // get(): media
            response(200, "data"),
            // delete(): file lookup
            response(200, r#"{"files":[{"id":"file-1","name":"a.oswk"}]}"#),
            // delete()
            response(204, ""),
        ]);
        let provider = GoogleDriveProvider::new_with_base(Box::new(StaticToken("token-1".into())), "Sync", &base)
            .expect("provider");

        assert!(provider.test().expect("test").contains("ada@example.com"));
        let entries = provider.list("Sync").expect("list");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].etag.as_deref(), Some("abc123"));
        let (bytes, etag) = provider.get("Sync/a.oswk").expect("get");
        assert_eq!(bytes, b"data");
        assert_eq!(etag.as_deref(), Some("abc123"));
        provider.delete("Sync/a.oswk").expect("delete");

        let requests = server.join().expect("server");
        assert!(requests[0].starts_with("GET /drive/v3/about"), "{}", requests[0]);
        assert!(requests[2].contains("/drive/v3/files?"), "{}", requests[2]);
        assert!(requests[4].starts_with("GET /drive/v3/files/file-1"), "{}", requests[4]);
        assert!(requests[6].starts_with("DELETE /drive/v3/files/file-1"), "{}", requests[6]);
    }

    #[test]
    fn google_drive_refuses_a_stale_if_match_without_uploading() {
        let (base, server) = start_mock(vec![
            // folder lookup
            response(200, r#"{"files":[{"id":"folder-1","name":"Sync"}]}"#),
            // file lookup with the current checksum
            response(200, r#"{"files":[{"id":"file-1","name":"a.oswk","sha256Checksum":"new-checksum"}]}"#),
        ]);
        let provider = GoogleDriveProvider::new_with_base(Box::new(StaticToken("token-1".into())), "Sync", &base)
            .expect("provider");
        let (_dir, file) = temp_file("data");
        let error = provider.put_file("Sync/a.oswk", &file, Some("old-checksum")).expect_err("must conflict");
        assert!(matches!(error, SyncError::Conflict(_)), "{error}");
        let requests = server.join().expect("server");
        assert_eq!(requests.len(), 2, "no upload request may be sent after a conflict");
    }

    #[test]
    fn invalid_remote_paths_are_refused_locally() {
        let token = Box::new(StaticToken("t".into()));
        assert!(GoogleDriveProvider::new(token, "../escape").is_err());
        let token = Box::new(StaticToken("t".into()));
        assert!(MicrosoftGraphProvider::new(token, "ok/../../bad").is_err());
    }
}
