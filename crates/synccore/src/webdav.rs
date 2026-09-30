//! WebDAV provider: PROPFIND (Depth 0/1), GET, conditional PUT, MKCOL, DELETE.
//!
//! Implementation notes:
//!
//! * **Conditional writes.** Every upload that replaces an existing cloud copy
//!   carries `If-Match: <etag>`. A server that supports it answers 412 when
//!   the remote copy changed underneath us; that maps to
//!   [`SyncError::Conflict`] and the caller must ask the user. Servers without
//!   ETag support simply ignore the header - in that case the separate hash
//!   comparison in `sync.rs` is the safety net.
//! * **Hardened XML.** The 207 Multi-Status body is parsed with a bounded
//!   depth (64), without DTD expansion, and with only the five predefined +
//!   numeric character references accepted. This mirrors the pattern used by
//!   `officecore::xml` and keeps a malicious/compromised server from blowing
//!   up the parser.
//! * **Bounded transfers.** Everything read from the wire goes through
//!   [`read_limited`]; the response `Content-Length` is checked first when the
//!   server provides it, and the streaming `Read` implementation is capped so
//!   a lying header cannot allocate unbounded memory.
//! * **No credentials in logs.** [`WebDavProvider`] implements `Debug`
//!   manually and redacts the password; the password is never part of a URL
//!   (it travels in the `Authorization` header).

use crate::{RemoteEntry, SyncError, SyncProvider, MAX_TRANSFER_BYTES};
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Largest multi-status document accepted (a folder with tens of thousands of
/// entries fits comfortably; anything larger is not a directory listing).
const MAX_XML_BYTES: u64 = 32 * 1024 * 1024;
/// Nesting cap for the DAV response parser.
const MAX_XML_DEPTH: usize = 64;
/// Individual path segment cap (most filesystems limit names to 255 bytes).
const MAX_SEGMENT_LEN: usize = 255;

const PROPFIND_BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:">
  <d:prop>
    <d:displayname/>
    <d:getcontentlength/>
    <d:getetag/>
    <d:getlastmodified/>
    <d:resourcetype/>
  </d:prop>
</d:propfind>"#;

/// Percent-encode set for one path segment: everything except unreserved
/// characters (`A-Z a-z 0-9 - . _ ~`). This keeps spaces, `#`, `?`, `%` and
/// non-ASCII characters safe inside the request URL.
const SEGMENT_ENCODE: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// Blocking WebDAV client. Construction validates the URL; network calls map
/// every HTTP/auth/parse failure to [`SyncError`].
pub struct WebDavProvider {
    base_url: String,
    username: String,
    password: String,
    client: reqwest::blocking::Client,
}

impl std::fmt::Debug for WebDavProvider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Explicitly redacted: `#[derive(Debug)]` would print the password
        // into any log line a caller ever writes.
        formatter
            .debug_struct("WebDavProvider")
            .field("base_url", &self.base_url)
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish()
    }
}

impl WebDavProvider {
    /// Strict constructor: HTTPS only. Use [`Self::new_with_options`] to opt
    /// into the loopback HTTP exception.
    pub fn new(base_url: &str, username: &str, password: &str) -> Result<Self, SyncError> {
        Self::new_with_options(base_url, username, password, false)
    }

    /// Constructor with the explicit insecure-HTTP opt-in. Plain `http://` is
    /// still refused for every non-loopback host; the flag only unlocks
    /// `localhost` / loopback-IP servers.
    pub fn new_with_options(
        base_url: &str,
        username: &str,
        password: &str,
        allow_insecure_http: bool,
    ) -> Result<Self, SyncError> {
        let base_url = normalize_base_url_with_options(base_url, allow_insecure_http)?;
        let redirect = reqwest::redirect::Policy::custom(move |attempt| {
            if !is_redirect_target_allowed(attempt.url(), allow_insecure_http) {
                return attempt.error(
                    "a redirect to a non-local http:// endpoint was refused to keep the credentials and documents encrypted"
                        .to_string(),
                );
            }
            if attempt.previous().len() >= 5 {
                attempt.stop()
            } else {
                attempt.follow()
            }
        });
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(20))
            // Generous overall timeout: a 512 MB transfer over a slow uplink
            // can legitimately take minutes. The cap bounds the size.
            .timeout(Duration::from_secs(600))
            .user_agent("PDFSwissArmyKnife/3.1 (synccore)")
            .redirect(redirect)
            .build()
            .map_err(|error| SyncError::Internal(format!("HTTP client could not be created: {error}")))?;
        Ok(Self {
            base_url,
            username: username.to_string(),
            password: password.to_string(),
            client,
        })
    }

    /// Ensures a remote directory chain exists (`MKCOL` each level). A 405
    /// response means "already there" on every WebDAV server in practice.
    pub fn ensure_dir(&self, remote_dir: &str) -> Result<(), SyncError> {
        let segments = normalize_remote_dir(remote_dir)?;
        let mut prefix: Vec<String> = Vec::new();
        for segment in &segments {
            prefix.push(segment.clone());
            let url = self.url_for(&prefix);
            let response = self
                .request(reqwest::Method::from_bytes(b"MKCOL").expect("MKCOL is a valid method"), &url, &[])
                .send()
                .map_err(network_error)?;
            match response.status().as_u16() {
                // 405: collection already exists (Apache/Nextcloud/ownCloud).
                200 | 201 | 204 | 405 => {}
                401 | 403 => {
                    return Err(SyncError::Auth(
                        "the server refused to create the remote folder (permission denied)".to_string(),
                    ))
                }
                409 => {
                    return Err(SyncError::Protocol(
                        "the parent collection is missing on the server".to_string(),
                    ))
                }
                other => return Err(http_status_error(other, &format!("MKCOL {remote_dir}"))),
            }
        }
        Ok(())
    }

    /// Base URL path segments (e.g. `https://host/dav` -> `["dav"]`), combined
    /// with the request directory segments for href matching.
    fn base_segments(&self) -> Vec<String> {
        let without_scheme = match self.base_url.split_once("://") {
            Some((_, rest)) => rest,
            None => self.base_url.as_str(),
        };
        let path = match without_scheme.find('/') {
            Some(index) => &without_scheme[index..],
            None => "",
        };
        path.split('/')
            .filter(|segment| !segment.is_empty())
            .map(decode_percent)
            .collect()
    }

    fn url_for(&self, segments: &[String]) -> String {
        let mut url = self.base_url.clone();
        for segment in segments {
            url.push('/');
            url.push_str(&encode_segment(segment));
        }
        url
    }

    fn request(&self, method: reqwest::Method, url: &str, depth: &[(&str, &str)]) -> reqwest::blocking::RequestBuilder {
        let mut request = self.client.request(method, url);
        for (key, value) in depth {
            request = request.header(*key, *value);
        }
        if !self.username.trim().is_empty() {
            request = request.basic_auth(&self.username, Some(&self.password));
        }
        request
    }

    fn propfind(&self, url: &str, depth: &str) -> Result<reqwest::blocking::Response, SyncError> {
        let request = self
            .client
            .request(reqwest::Method::from_bytes(b"PROPFIND").expect("PROPFIND is a valid method"), url)
            .header("Depth", depth)
            .header("Content-Type", "application/xml; charset=utf-8")
            .body(PROPFIND_BODY);
        let request = if !self.username.trim().is_empty() {
            request.basic_auth(&self.username, Some(&self.password))
        } else {
            request
        };
        request.send().map_err(network_error)
    }

    /// Streams a local file to the server without buffering it in memory and
    /// returns `(sha256, size, etag)`. The body carries a known
    /// Content-Length (every WebDAV server accepts it) and the hash is
    /// computed while the bytes travel, so a 512 MB document never needs more
    /// than one 64 KiB chunk of memory.
    pub fn put_file(
        &self,
        path: &str,
        local: &Path,
        if_match_etag: Option<&str>,
    ) -> Result<(String, u64, Option<String>), SyncError> {
        let metadata = std::fs::metadata(local)?;
        if metadata.len() > MAX_TRANSFER_BYTES {
            return Err(SyncError::TooLarge(metadata.len()));
        }
        let segments = normalize_remote_path(path)?;
        let url = self.url_for(&segments);
        let file = File::open(local)?;
        let hasher = Arc::new(Mutex::new(Sha256::new()));
        let body = HashingReader {
            inner: file,
            hasher: hasher.clone(),
        };
        let mut request = self
            .request(reqwest::Method::PUT, &url, &[])
            .header("Content-Type", "application/octet-stream")
            .body(reqwest::blocking::Body::sized(body, metadata.len()));
        if let Some(etag) = if_match_etag {
            if !etag.trim().is_empty() {
                request = request.header("If-Match", etag.trim());
            }
        }
        let response = request.send().map_err(network_error)?;
        let status = response.status().as_u16();
        let etag = header_etag(&response);
        let etag = map_put_status(status, path, etag)?;
        let sha256 = {
            let guard = hasher.lock().map_err(|_| SyncError::Internal("upload hasher was poisoned".to_string()))?;
            hex_lower(&guard.clone().finalize())
        };
        Ok((sha256, metadata.len(), etag))
    }

    /// Streams a remote file into `writer` while hashing it. Returns
    /// `(sha256, size, etag)`. Nothing larger than the transfer cap is ever
    /// read; a lying `Content-Length` cannot overflow memory because the
    /// response is consumed in bounded chunks.
    pub fn get_to_writer(
        &self,
        path: &str,
        writer: &mut dyn Write,
    ) -> Result<(String, u64, Option<String>), SyncError> {
        let segments = normalize_remote_path(path)?;
        let url = self.url_for(&segments);
        let mut response = self.request(reqwest::Method::GET, &url, &[]).send().map_err(network_error)?;
        let status = response.status();
        if !status.is_success() {
            return Err(http_status_error(status.as_u16(), path));
        }
        if let Some(length) = response.content_length() {
            if length > MAX_TRANSFER_BYTES {
                return Err(SyncError::TooLarge(length));
            }
        }
        let etag = header_etag(&response);
        let mut hasher = Sha256::new();
        let mut total = 0u64;
        let mut chunk = [0u8; 64 * 1024];
        loop {
            let read = response.read(&mut chunk)?;
            if read == 0 {
                break;
            }
            total += read as u64;
            if total > MAX_TRANSFER_BYTES {
                return Err(SyncError::TooLarge(total));
            }
            hasher.update(&chunk[..read]);
            writer.write_all(&chunk[..read])?;
        }
        Ok((hex_lower(&hasher.finalize()), total, etag))
    }

    /// Streams a remote file into a temporary sibling of `local` (same
    /// directory, so the final rename is atomic) and returns
    /// `(temp_path, sha256, size, etag)`. The caller decides whether to
    /// promote the temporary file with [`promote_staged_download`]; on any
    /// error the partial file is removed.
    pub fn stage_download(
        &self,
        path: &str,
        local: &Path,
    ) -> Result<(PathBuf, String, u64, Option<String>), SyncError> {
        let parent = local
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .ok_or_else(|| SyncError::InvalidInput("the download destination has no folder".to_string()))?;
        let name = local
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "download".to_string());
        let temp = parent.join(format!(".{name}.{}.part", uuid::Uuid::new_v4()));
        let mut file = File::create(&temp)?;
        let streamed = self.get_to_writer(path, &mut file);
        let (sha256, size, etag) = match streamed {
            Ok(result) => result,
            Err(error) => {
                drop(file);
                let _ = std::fs::remove_file(&temp);
                return Err(error);
            }
        };
        file.sync_all()?;
        drop(file);
        Ok((temp, sha256, size, etag))
    }
}

/// Promotes a staged download into place with an atomic rename (the staged
/// file lives in the same directory as the target).
pub fn promote_staged_download(staged: &Path, target: &Path) -> Result<(), SyncError> {
    std::fs::rename(staged, target)?;
    Ok(())
}

/// A `Read` adapter that feeds every byte through SHA-256, so an upload can
/// stream and hash at the same time.
struct HashingReader<R> {
    inner: R,
    hasher: Arc<Mutex<Sha256>>,
}

impl<R: Read> Read for HashingReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buffer)?;
        if read > 0 {
            if let Ok(mut hasher) = self.hasher.lock() {
                hasher.update(&buffer[..read]);
            }
        }
        Ok(read)
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

impl SyncProvider for WebDavProvider {
    fn test(&self) -> Result<String, SyncError> {
        let url = self.url_for(&[]);
        let response = self.propfind(&url, "0")?;
        let status = response.status().as_u16();
        match status {
            200 | 207 => {}
            401 | 403 | 407 => {
                return Err(SyncError::Auth(
                    "the server rejected these credentials (check the username and password)".to_string(),
                ))
            }
            404 => return Err(SyncError::NotFound(format!("WebDAV endpoint not found: {}", self.base_url))),
            other => return Err(http_status_error(other, "PROPFIND (connection test)")),
        }
        let server = response
            .headers()
            .get(reqwest::header::SERVER)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.to_string())
            .unwrap_or_else(|| "WebDAV server".to_string());
        Ok(server)
    }

    fn list(&self, remote_dir: &str) -> Result<Vec<RemoteEntry>, SyncError> {
        let dir_segments = normalize_remote_dir(remote_dir)?;
        let mut request_segments = self.base_segments();
        request_segments.extend(dir_segments.iter().cloned());
        let mut url = self.url_for(&dir_segments);
        url.push('/');
        let response = self.propfind(&url, "1")?;
        let status = response.status().as_u16();
        match status {
            200 | 207 => {}
            401 | 403 | 407 => {
                return Err(SyncError::Auth(
                    "the server rejected these credentials while listing the remote folder".to_string(),
                ))
            }
            404 => return Err(SyncError::NotFound(remote_dir.to_string())),
            other => return Err(http_status_error(other, &format!("PROPFIND {remote_dir}"))),
        }
        let body = read_limited(response, MAX_XML_BYTES)?;
        let text = String::from_utf8_lossy(&body);
        parse_multistatus(&text, &request_segments)
    }

    fn get(&self, path: &str) -> Result<(Vec<u8>, Option<String>), SyncError> {
        let segments = normalize_remote_path(path)?;
        let url = self.url_for(&segments);
        let request = self.request(reqwest::Method::GET, &url, &[]);
        let response = request.send().map_err(network_error)?;
        let status = response.status();
        if !status.is_success() {
            return Err(http_status_error(status.as_u16(), path));
        }
        if let Some(length) = response.content_length() {
            if length > MAX_TRANSFER_BYTES {
                return Err(SyncError::TooLarge(length));
            }
        }
        let etag = header_etag(&response);
        let bytes = read_limited(response, MAX_TRANSFER_BYTES)?;
        Ok((bytes, etag))
    }

    fn put(
        &self,
        path: &str,
        bytes: &[u8],
        if_match_etag: Option<&str>,
    ) -> Result<Option<String>, SyncError> {
        if bytes.len() as u64 > MAX_TRANSFER_BYTES {
            return Err(SyncError::TooLarge(bytes.len() as u64));
        }
        let segments = normalize_remote_path(path)?;
        let url = self.url_for(&segments);
        let mut request = self
            .request(reqwest::Method::PUT, &url, &[])
            .header("Content-Type", "application/octet-stream")
            // `Vec<u8>` body: reqwest needs an owned/`'static` body for the
            // blocking client; the buffer is already capped at 512 MB above.
            .body(bytes.to_vec());
        if let Some(etag) = if_match_etag {
            if !etag.trim().is_empty() {
                request = request.header("If-Match", etag.trim());
            }
        }
        let response = request.send().map_err(network_error)?;
        let status = response.status().as_u16();
        let etag = header_etag(&response);
        map_put_status(status, path, etag)
    }

    fn delete(&self, path: &str) -> Result<(), SyncError> {
        let segments = normalize_remote_path(path)?;
        let url = self.url_for(&segments);
        let response = self
            .request(reqwest::Method::DELETE, &url, &[])
            .send()
            .map_err(network_error)?;
        match response.status().as_u16() {
            200 | 202 | 204 => Ok(()),
            401 | 403 | 407 => Err(SyncError::Auth("the server refused the delete".to_string())),
            404 => Err(SyncError::NotFound(path.to_string())),
            423 => Err(SyncError::Conflict("the remote file is locked by another client".to_string())),
            other => Err(http_status_error(other, &format!("DELETE {path}"))),
        }
    }
}

// ---------------------------------------------------------------------------
// Status mapping (pure, unit tested)
// ---------------------------------------------------------------------------

/// Maps a completed PUT response. 412 is the important one: the precondition
/// (`If-Match`) failed, which means somebody else stored a newer version.
pub fn map_put_status(
    status: u16,
    path: &str,
    etag: Option<String>,
) -> Result<Option<String>, SyncError> {
    match status {
        200 | 201 | 204 => Ok(etag),
        412 => Err(SyncError::Conflict(format!(
            "the cloud copy of {path} changed since it was last synced (HTTP 412); resolve the conflict and try again"
        ))),
        409 => Err(SyncError::Conflict(format!(
            "the server reported a state conflict for {path} (HTTP 409)"
        ))),
        423 => Err(SyncError::Conflict(format!(
            "the cloud copy of {path} is locked by another client (HTTP 423)"
        ))),
        401 | 403 | 407 => Err(SyncError::Auth(format!(
            "the server rejected the upload of {path} (HTTP {status})"
        ))),
        404 => Err(SyncError::NotFound(path.to_string())),
        507 => Err(SyncError::Http {
            status,
            message: "the WebDAV account has no free space".to_string(),
        }),
        other => Err(http_status_error(other, &format!("PUT {path}"))),
    }
}

fn http_status_error(status: u16, context: &str) -> SyncError {
    match status {
        401 | 403 | 407 => SyncError::Auth(format!("authentication/permission failure on {context}")),
        404 => SyncError::NotFound(context.to_string()),
        423 => SyncError::Conflict(format!("{context} is locked by another client")),
        _ => SyncError::Http {
            status,
            message: context.to_string(),
        },
    }
}

fn network_error(error: reqwest::Error) -> SyncError {
    // The password is never part of the URL, so the error text is safe to
    // surface - but it still stays inside the SyncError, never a log call.
    if error.is_timeout() {
        SyncError::Network(format!("the request timed out ({error})"))
    } else if error.is_connect() {
        SyncError::Network(format!("could not connect to the WebDAV server ({error})"))
    } else {
        SyncError::Network(error.to_string())
    }
}

fn header_etag(response: &reqwest::blocking::Response) -> Option<String> {
    response
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|value| value.to_str().ok())
        .and_then(normalize_etag)
}

// ---------------------------------------------------------------------------
// Path handling
// ---------------------------------------------------------------------------

/// True when `host` points at the local machine: `localhost`, any name under
/// `.localhost`, or a loopback IP literal (`127.0.0.0/8`, `::1`).
pub fn is_loopback_host(host: &str) -> bool {
    let trimmed = host.trim().trim_start_matches('[').trim_end_matches(']');
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.eq_ignore_ascii_case("localhost") {
        return true;
    }
    // IP literals arrive without the authority brackets once the URL parser
    // has split them out; strip defensively in case a caller passes a raw host.
    trimmed
        .parse::<std::net::IpAddr>()
        .map(|address| address.is_loopback())
        .unwrap_or(false)
}

/// A redirect is only followed over HTTPS, or over plain HTTP when the target
/// is the local machine and the caller explicitly opted into the loopback
/// exception. A public HTTP target (a downgrade attack, or a redirected
/// credential leak) fails the request instead of being followed.
pub fn is_redirect_target_allowed(url: &reqwest::Url, allow_insecure_http: bool) -> bool {
    match url.scheme() {
        "https" => true,
        "http" => allow_insecure_http && url.host_str().map(is_loopback_host).unwrap_or(false),
        _ => false,
    }
}

/// Validates and trims the WebDAV endpoint URL. Query strings and fragments
/// are rejected: they cannot be combined with per-folder paths safely.
///
/// This is the strict entry point: HTTPS only. Callers that want the
/// loopback-HTTP exception use [`normalize_base_url_with_options`].
pub fn normalize_base_url(url: &str) -> Result<String, SyncError> {
    normalize_base_url_with_options(url, false)
}

/// Validates and trims the WebDAV endpoint URL.
///
/// HTTPS is mandatory for every non-local host: the Basic Auth credentials
/// and the synchronized documents must never travel in cleartext. Plain
/// `http://` is only accepted for loopback hosts (`localhost`, `127.0.0.0/8`,
/// `::1`) and only when the caller explicitly opted in with
/// `allow_insecure_http`; a public `http://` endpoint is refused even then.
pub fn normalize_base_url_with_options(url: &str, allow_insecure_http: bool) -> Result<String, SyncError> {
    let trimmed = url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(SyncError::InvalidInput("the WebDAV URL is empty".to_string()));
    }
    if trimmed.chars().any(|ch| ch.is_whitespace() || ch.is_control()) {
        return Err(SyncError::InvalidInput("the WebDAV URL contains whitespace".to_string()));
    }
    let parsed = reqwest::Url::parse(trimmed).map_err(|_| {
        SyncError::InvalidInput("the WebDAV URL is not a valid absolute http(s) URL".to_string())
    })?;
    match parsed.scheme() {
        "https" => {}
        "http" => {
            let host = parsed.host_str().unwrap_or_default();
            if !is_loopback_host(host) {
                return Err(SyncError::InvalidInput(
                    "plain http:// is refused for non-local servers: use https:// so the credentials and the synced documents are encrypted in transit"
                        .to_string(),
                ));
            }
            if !allow_insecure_http {
                return Err(SyncError::InvalidInput(
                    "plain http:// only works for a local server (localhost) and must be enabled explicitly in the sync settings"
                        .to_string(),
                ));
            }
        }
        other => {
            return Err(SyncError::InvalidInput(format!(
                "the WebDAV URL must start with https:// (got {other}://)"
            )))
        }
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(SyncError::InvalidInput(
            "the WebDAV URL must not contain a query string or fragment".to_string(),
        ));
    }
    // Refuse credentials embedded in the URL: they would leak into error
    // strings, and the password belongs to the secret store.
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(SyncError::InvalidInput(
            "remove the username/password from the URL and use the dedicated fields".to_string(),
        ));
    }
    Ok(trimmed.to_string())
}

/// Splits a remote directory into validated segments. Empty/`/` means the
/// server root. `..`, `.`, backslashes and control characters are rejected so
/// a remote path can never escape the configured directory.
pub fn normalize_remote_dir(dir: &str) -> Result<Vec<String>, SyncError> {
    let trimmed = dir.trim();
    if trimmed.is_empty() || trimmed == "/" {
        return Ok(Vec::new());
    }
    let mut segments = Vec::new();
    for segment in trimmed.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        segments.push(validate_segment(segment)?);
    }
    Ok(segments)
}

/// Validates a remote file path (directory segments + final name).
pub fn normalize_remote_path(path: &str) -> Result<Vec<String>, SyncError> {
    let trimmed = path.trim();
    if trimmed.ends_with('/') {
        return Err(SyncError::InvalidInput(format!("expected a file path, got a folder: {path}")));
    }
    let segments = normalize_remote_dir(trimmed)?;
    if segments.is_empty() {
        return Err(SyncError::InvalidInput("the remote file path is empty".to_string()));
    }
    Ok(segments)
}

/// Validates a single remote file name (no separators, no traversal).
pub fn normalize_remote_name(name: &str) -> Result<String, SyncError> {
    let trimmed = name.trim();
    if trimmed.contains('/') {
        return Err(SyncError::InvalidInput(
            "a remote file name must not contain path separators".to_string(),
        ));
    }
    validate_segment(trimmed)
}

fn validate_segment(segment: &str) -> Result<String, SyncError> {
    if segment.is_empty() {
        return Err(SyncError::InvalidInput("empty path segment".to_string()));
    }
    if segment == "." || segment == ".." {
        return Err(SyncError::InvalidInput(
            "relative path segments are not allowed".to_string(),
        ));
    }
    if segment.contains('\\') {
        return Err(SyncError::InvalidInput(
            "backslashes are not allowed in remote paths".to_string(),
        ));
    }
    if segment.chars().any(|ch| ch.is_control()) {
        return Err(SyncError::InvalidInput(
            "control characters are not allowed in remote paths".to_string(),
        ));
    }
    if segment.len() > MAX_SEGMENT_LEN {
        return Err(SyncError::InvalidInput(
            "a remote path segment is longer than 255 bytes".to_string(),
        ));
    }
    Ok(segment.to_string())
}

fn encode_segment(segment: &str) -> String {
    utf8_percent_encode(segment, SEGMENT_ENCODE).to_string()
}

fn decode_percent(value: &str) -> String {
    percent_encoding::percent_decode_str(value).decode_utf8_lossy().to_string()
}

/// Normalizes an ETag: strips the weak prefix and surrounding quotes so that
/// `W/"abc"` and `"abc"` compare equal across servers.
pub fn normalize_etag(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let without_weak = trimmed.strip_prefix("W/").unwrap_or(trimmed).trim();
    let unquoted = without_weak.trim_matches('"').trim();
    if unquoted.is_empty() {
        None
    } else {
        Some(unquoted.to_string())
    }
}

// ---------------------------------------------------------------------------
// Bounded XML parsing
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct XmlNode {
    name: String,
    text: String,
    children: Vec<XmlNode>,
}

impl XmlNode {
    fn local_name(&self) -> &str {
        self.name.rsplit(':').next().unwrap_or(&self.name)
    }

    fn child(&self, name: &str) -> Option<&XmlNode> {
        self.children
            .iter()
            .find(|child| child.name == name || child.local_name() == name)
    }

    fn deep_text(&self) -> String {
        let mut out = self.text.clone();
        for child in &self.children {
            out.push_str(&child.deep_text());
        }
        out
    }

    fn find_all<'a>(&'a self, name: &str, out: &mut Vec<&'a XmlNode>) {
        if self.name == name || self.local_name() == name {
            out.push(self);
        }
        for child in &self.children {
            child.find_all(name, out);
        }
    }

    fn has_descendant(&self, name: &str) -> bool {
        if self.local_name() == name {
            return true;
        }
        self.children.iter().any(|child| child.has_descendant(name))
    }
}

/// Parses a Multi-Status document into remote entries.
///
/// `request_segments` is the decoded path prefix of the PROPFIND target
/// relative to the URL authority (base URL path + remote directory segments).
/// Entries are matched by that prefix so the collection itself and any deeper
/// descendants are ignored.
pub fn parse_multistatus(xml: &str, request_segments: &[String]) -> Result<Vec<RemoteEntry>, SyncError> {
    let document = parse_xml_bounded(xml)?;
    let mut responses = Vec::new();
    document.find_all("response", &mut responses);

    let mut entries = Vec::new();
    for response in responses {
        let Some(href_node) = response.child("href") else {
            continue;
        };
        let href = href_node.deep_text();
        let path = href_path(&href);
        if path.is_empty() {
            continue;
        }
        let decoded_segments: Vec<String> = path
            .split('/')
            .filter(|segment| !segment.is_empty())
            .map(decode_percent)
            .collect();
        let Some(name) = entry_name_for(&decoded_segments, request_segments) else {
            continue;
        };

        // Props may be split over several `propstat` blocks (one per status).
        // Only 2xx blocks carry usable values.
        let mut props: Vec<&XmlNode> = Vec::new();
        for propstat in &response.children {
            if propstat.local_name() != "propstat" {
                continue;
            }
            let status_ok = propstat
                .child("status")
                .map(|status| propstat_is_success(&status.deep_text()))
                .unwrap_or(true);
            if !status_ok {
                continue;
            }
            if let Some(prop) = propstat.child("prop") {
                props.push(prop);
            }
        }
        let prop = |name: &str| -> Option<&XmlNode> {
            props.iter().find_map(|prop| prop.child(name))
        };

        let size = prop("getcontentlength")
            .map(|node| node.deep_text())
            .and_then(|text| text.trim().parse::<u64>().ok())
            .unwrap_or(0);
        let etag = prop("getetag")
            .map(|node| node.deep_text())
            .and_then(|text| normalize_etag(&text));
        let modified = prop("getlastmodified")
            .map(|node| node.deep_text())
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty());
        let is_dir = prop("resourcetype")
            .map(|node| node.has_descendant("collection"))
            .unwrap_or(false);

        entries.push(RemoteEntry {
            name,
            size,
            etag,
            modified,
            is_dir,
        });
    }
    Ok(entries)
}

/// Extracts the path portion of an href, which may be absolute
/// (`https://host/dav/x`), authority-relative (`//host/dav/x`) or
/// root-relative (`/dav/x`, `dav/x`).
fn href_path(href: &str) -> String {
    let trimmed = href.trim();
    // Drop query/fragment before any other processing.
    let without_query = trimmed.split(['?', '#']).next().unwrap_or(trimmed);
    // Absolute URL: strip scheme and authority.
    let rest = match without_query.split_once("://") {
        Some((_, after_scheme)) => match after_scheme.find('/') {
            Some(index) => &after_scheme[index..],
            None => return String::new(),
        },
        None => without_query,
    };
    // Protocol-relative href (`//host/path`): strip the authority as well.
    let rest = match rest.strip_prefix("//") {
        Some(after_authority) => match after_authority.find('/') {
            Some(index) => &after_authority[index..],
            None => return String::new(),
        },
        None => rest,
    };
    rest.to_string()
}

/// Returns the file name when the href is a direct child of the requested
/// directory. `None` for the directory itself, deeper descendants, or
/// unrelated paths. A tolerant last-segment fallback covers servers that
/// return paths relative to their own alias rather than the request URL.
fn entry_name_for(href_segments: &[String], request_segments: &[String]) -> Option<String> {
    // The href path equals or is a prefix of the request path (the collection
    // itself, or one of its ancestors): never an entry.
    let prefix_matches = href_segments.len() <= request_segments.len()
        && href_segments == &request_segments[..href_segments.len()];
    if prefix_matches {
        return None;
    }
    if href_segments.len() > request_segments.len() {
        let remainder = &href_segments[request_segments.len()..];
        // Exactly one deeper segment = direct child; more = descendant.
        return (remainder.len() == 1).then(|| remainder[0].clone());
    }
    // Alias fallback: the server used a different prefix of the same depth.
    href_segments.last().cloned()
}

fn propstat_is_success(status: &str) -> bool {
    // e.g. "HTTP/1.1 200 OK" (Apache) or "HTTP/1.1 404 Not Found".
    let code = status
        .split_whitespace()
        .find_map(|token| token.parse::<u16>().ok());
    matches!(code, Some(code) if (200..300).contains(&code))
}

fn parse_xml_bounded(xml: &str) -> Result<XmlNode, SyncError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    reader.config_mut().expand_empty_elements = false;
    let mut stack: Vec<XmlNode> = vec![XmlNode { name: "#document".into(), ..Default::default() }];
    let mut depth = 0usize;
    loop {
        match reader.read_event() {
            Ok(Event::Start(start)) => {
                depth += 1;
                if depth > MAX_XML_DEPTH {
                    return Err(SyncError::Protocol("XML nesting is deeper than the safe limit".to_string()));
                }
                stack.push(node_from_start(&start)?);
            }
            Ok(Event::Empty(start)) => {
                depth += 1;
                if depth > MAX_XML_DEPTH {
                    return Err(SyncError::Protocol("XML nesting is deeper than the safe limit".to_string()));
                }
                let node = node_from_start(&start)?;
                stack.last_mut().expect("stack is never empty").children.push(node);
                depth -= 1;
            }
            Ok(Event::End(_)) => {
                if stack.len() > 1 {
                    let node = stack.pop().expect("length checked");
                    stack.last_mut().expect("stack is never empty").children.push(node);
                    depth = depth.saturating_sub(1);
                }
            }
            Ok(Event::Text(text)) => {
                let raw = text.into_inner();
                let value = quick_xml::escape::unescape(&raw)
                    .map(|value| value.into_owned())
                    .unwrap_or_else(|_| raw.into_owned());
                stack.last_mut().expect("stack is never empty").text.push_str(&value);
            }
            Ok(Event::CData(data)) => {
                let value = data.into_inner().into_owned();
                stack.last_mut().expect("stack is never empty").text.push_str(&value);
            }
            Ok(Event::GeneralRef(reference)) => {
                // Only predefined and numeric references; unknown entities are
                // dropped (and DTDs are never consulted, so no XXE).
                let name = reference.into_inner();
                let value = match name.as_ref() {
                    "amp" => "&",
                    "lt" => "<",
                    "gt" => ">",
                    "quot" => "\"",
                    "apos" => "'",
                    other => {
                        if let Some(rest) = other.strip_prefix('#') {
                            let code = if let Some(hex) = rest.strip_prefix('x').or_else(|| rest.strip_prefix('X')) {
                                u32::from_str_radix(hex, 16).ok()
                            } else {
                                rest.parse::<u32>().ok()
                            };
                            if let Some(ch) = code.and_then(char::from_u32) {
                                stack.last_mut().expect("stack is never empty").text.push(ch);
                            }
                        }
                        continue;
                    }
                };
                stack.last_mut().expect("stack is never empty").text.push_str(value);
            }
            Ok(Event::Eof) => break,
            // Decl, PI, Comment, DocType: deliberately ignored. A DTD is
            // never loaded, so external entities cannot resolve.
            Ok(_) => {}
            Err(error) => {
                return Err(SyncError::Protocol(format!("malformed DAV XML: {error}")));
            }
        }
    }
    if stack.len() > 1 {
        // A truncated document (unclosed elements) must not be silently
        // accepted: it could hide part of a directory listing.
        return Err(SyncError::Protocol(
            "malformed DAV XML: the document ends with unclosed elements".to_string(),
        ));
    }
    let mut root = stack.pop().expect("root exists");
    if root.children.len() == 1 {
        root = root.children.remove(0);
    }
    Ok(root)
}

fn node_from_start(start: &BytesStart<'_>) -> Result<XmlNode, SyncError> {
    // Attributes are not needed by the DAV response logic; entity-laden
    // attribute values are therefore never even decoded.
    Ok(XmlNode {
        name: start.name().as_ref().to_string(),
        text: String::new(),
        children: Vec::new(),
    })
}

/// Reads a response body with a hard cap, using `std::io::Read` on the
/// blocking response so a missing/lying `Content-Length` cannot overflow.
fn read_limited<R: Read>(mut reader: R, limit: u64) -> Result<Vec<u8>, SyncError> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 64 * 1024];
    loop {
        let read = reader.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        if buffer.len() as u64 + read as u64 > limit {
            return Err(SyncError::TooLarge(limit));
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- URL / path normalization -------------------------------------------

    #[test]
    fn base_url_validation() {
        assert_eq!(
            normalize_base_url("https://cloud.example.com/dav/").unwrap(),
            "https://cloud.example.com/dav"
        );
        assert_eq!(
            normalize_base_url_with_options("http://localhost:8080/dav", true).unwrap(),
            "http://localhost:8080/dav"
        );
        // A public HTTP endpoint is refused even with the opt-in: only
        // loopback hosts may fall back to plain HTTP.
        assert!(normalize_base_url("http://example.com/dav").is_err());
        assert!(normalize_base_url_with_options("http://example.com/dav", true).is_err());
        assert!(normalize_base_url_with_options("http://192.168.1.10:8080/dav", true).is_err());
        // Loopback HTTP still requires the explicit opt-in.
        assert!(normalize_base_url("http://localhost:8080/dav").is_err());
        assert!(normalize_base_url("http://127.0.0.1:8080/dav").is_err());
        assert!(normalize_base_url_with_options("http://127.0.0.1:8080/dav", true).is_ok());
        assert!(normalize_base_url_with_options("http://[::1]:8080/dav", true).is_ok());
        assert!(normalize_base_url("").is_err());
        assert!(normalize_base_url("ftp://host/dav").is_err());
        assert!(normalize_base_url("https://user:pass@host/dav").is_err());
        assert!(normalize_base_url("https://host/dav?x=1").is_err());
        assert!(normalize_base_url("https://host/dav#frag").is_err());
    }

    #[test]
    fn loopback_detection() {
        assert!(is_loopback_host("localhost"));
        assert!(is_loopback_host("[::1]"));
        assert!(is_loopback_host("127.0.0.1"));
        assert!(is_loopback_host("127.8.8.8"));
        assert!(!is_loopback_host("example.com"));
        assert!(!is_loopback_host("192.168.1.10"));
        assert!(!is_loopback_host("10.0.0.1"));
        assert!(!is_loopback_host(""));
    }

    #[test]
    fn redirect_policy_rejects_public_http_targets() {
        let https = reqwest::Url::parse("https://cloud.example.com/x").unwrap();
        let public_http = reqwest::Url::parse("http://example.com/x").unwrap();
        let loopback_http = reqwest::Url::parse("http://127.0.0.1:8080/x").unwrap();
        let other = reqwest::Url::parse("ftp://example.com/x").unwrap();
        assert!(is_redirect_target_allowed(&https, false));
        assert!(!is_redirect_target_allowed(&public_http, false));
        assert!(!is_redirect_target_allowed(&public_http, true));
        assert!(!is_redirect_target_allowed(&loopback_http, false));
        assert!(is_redirect_target_allowed(&loopback_http, true));
        assert!(!is_redirect_target_allowed(&other, true));
    }

    #[test]
    fn provider_rejects_plain_http_for_non_local_hosts() {
        assert!(WebDavProvider::new("http://example.com/dav", "a", "b").is_err());
        assert!(WebDavProvider::new_with_options("http://example.com/dav", "a", "b", true).is_err());
        assert!(WebDavProvider::new("http://127.0.0.1:8080/dav", "a", "b").is_err());
        assert!(WebDavProvider::new_with_options("http://127.0.0.1:8080/dav", "a", "b", true).is_ok());
    }

    #[test]
    fn redirect_downgrade_is_refused() {
        use std::io::{Read, Write};
        // A loopback WebDAV endpoint that answers every request with a
        // redirect to a public http:// host. The client must refuse the
        // redirect instead of sending the request (and Basic Auth) there.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0u8; 2048];
                let _ = stream.read(&mut buffer);
                let response = "HTTP/1.1 302 Found\r\nLocation: http://example.invalid/steal\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                let _ = stream.write_all(response.as_bytes());
            }
        });
        let provider =
            WebDavProvider::new_with_options(&format!("http://127.0.0.1:{port}/dav"), "alice", "hunter2", true)
                .unwrap();
        let error = provider.test().unwrap_err();
        server.join().unwrap();
        match error {
            SyncError::Network(message) => assert!(
                message.contains("redirect"),
                "the failure must be the refused redirect, got: {message}"
            ),
            other => panic!("expected Network error for the refused redirect, got {other:?}"),
        }
    }

    #[test]
    fn streaming_put_and_get_roundtrip() {
        use std::io::{Read, Write};
        // A minimal loopback WebDAV endpoint: stores the PUT body in memory
        // and serves it back on GET (with an ETag). Verifies that the
        // streaming upload/download paths speak real HTTP.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let mut stored: Vec<u8> = Vec::new();
            for _ in 0..2 {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let mut buffer: Vec<u8> = Vec::new();
                let mut chunk = [0u8; 4096];
                let header_end = loop {
                    let read = stream.read(&mut chunk).unwrap_or(0);
                    if read == 0 {
                        return;
                    }
                    buffer.extend_from_slice(&chunk[..read]);
                    if let Some(position) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
                        break position + 4;
                    }
                };
                let headers = String::from_utf8_lossy(&buffer[..header_end]).to_string();
                let headers_lower = headers.to_lowercase();
                let content_length = headers_lower
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length: "))
                    .and_then(|value| value.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                while buffer.len() < header_end + content_length {
                    let read = stream.read(&mut chunk).unwrap_or(0);
                    if read == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..read]);
                }
                if headers.starts_with("PUT") {
                    stored = buffer[header_end..header_end + content_length].to_vec();
                    let response =
                        "HTTP/1.1 201 Created\r\nETag: \"e1\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                    let _ = stream.write_all(response.as_bytes());
                } else if headers.starts_with("GET") {
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nETag: \"e1\"\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        stored.len()
                    );
                    let _ = stream.write_all(response.as_bytes());
                    let _ = stream.write_all(&stored);
                } else {
                    let _ = stream.write_all(b"HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                }
            }
        });
        let provider =
            WebDavProvider::new_with_options(&format!("http://127.0.0.1:{port}/dav"), "", "", true).unwrap();
        let dir = std::env::temp_dir().join(format!("pdfsak-stream-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("source.oswk");
        let payload = b"streamed document bytes";
        std::fs::write(&source, payload).unwrap();

        let (sha256, size, etag) = provider.put_file("remote.oswk", &source, None).unwrap();
        assert_eq!(size, payload.len() as u64);
        assert_eq!(etag.as_deref(), Some("e1"));
        assert_eq!(sha256, hex_lower(Sha256::digest(payload).as_slice()));

        let mut downloaded = Vec::new();
        let (downloaded_sha, downloaded_size, downloaded_etag) =
            provider.get_to_writer("remote.oswk", &mut downloaded).unwrap();
        assert_eq!(downloaded, payload);
        assert_eq!(downloaded_size, payload.len() as u64);
        assert_eq!(downloaded_sha, sha256);
        assert_eq!(downloaded_etag.as_deref(), Some("e1"));

        server.join().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remote_dir_validation() {
        assert_eq!(normalize_remote_dir("").unwrap(), Vec::<String>::new());
        assert_eq!(normalize_remote_dir("/").unwrap(), Vec::<String>::new());
        assert_eq!(normalize_remote_dir("/a/b/").unwrap(), vec!["a", "b"]);
        assert_eq!(normalize_remote_dir("a//b").unwrap(), vec!["a", "b"]);
        assert!(normalize_remote_dir("..").is_err());
        assert!(normalize_remote_dir("a/../../etc").is_err());
        assert!(normalize_remote_dir("a\\b").is_err());
        assert!(normalize_remote_dir("a/\u{0}b").is_err());
        assert!(normalize_remote_dir(&"x".repeat(300)).is_err());
    }

    #[test]
    fn remote_name_and_path_validation() {
        assert_eq!(normalize_remote_name("report.oswk").unwrap(), "report.oswk");
        assert!(normalize_remote_name("a/b").is_err());
        assert!(normalize_remote_name("..").is_err());
        assert_eq!(
            normalize_remote_path("PDFSAK/report.oswk").unwrap(),
            vec!["PDFSAK", "report.oswk"]
        );
        assert!(normalize_remote_path("PDFSAK/").is_err());
        assert!(normalize_remote_path("").is_err());
    }

    #[test]
    fn url_segments_are_percent_encoded() {
        let provider = WebDavProvider::new("https://host/dav", "", "").unwrap();
        let url = provider.url_for(&["My Docs".to_string(), "Report (final) #1.oswk".to_string()]);
        assert_eq!(
            url,
            "https://host/dav/My%20Docs/Report%20%28final%29%20%231.oswk"
        );
    }

    #[test]
    fn etag_normalization() {
        assert_eq!(normalize_etag("\"abc\"").as_deref(), Some("abc"));
        assert_eq!(normalize_etag("W/\"abc\"").as_deref(), Some("abc"));
        assert_eq!(normalize_etag("  abc ").as_deref(), Some("abc"));
        assert_eq!(normalize_etag(""), None);
        assert_eq!(normalize_etag("\"\""), None);
    }

    #[test]
    fn debug_redacts_password() {
        let provider = WebDavProvider::new("https://host/dav", "alice", "hunter2").unwrap();
        let text = format!("{provider:?}");
        assert!(!text.contains("hunter2"), "password must never be printable");
        assert!(text.contains("<redacted>"));
        assert!(text.contains("alice"));
    }

    // -- Multi-Status parsing -----------------------------------------------

    const APACHE_MULTISTATUS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/dav/PDFSAK/</D:href>
    <D:propstat>
      <D:prop>
        <D:displayname>PDFSAK</D:displayname>
        <D:resourcetype><D:collection/></D:resourcetype>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/dav/PDFSAK/report.oswk</D:href>
    <D:propstat>
      <D:prop>
        <D:displayname>report.oswk</D:displayname>
        <D:getcontentlength>1234</D:getcontentlength>
        <D:getetag>"abc123"</D:getetag>
        <D:getlastmodified>Wed, 21 Oct 2026 07:28:00 GMT</D:getlastmodified>
        <D:resourcetype/>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/dav/PDFSAK/Report%20Final.oswk</D:href>
    <D:propstat>
      <D:prop>
        <D:getcontentlength>99</D:getcontentlength>
        <D:getetag>W/"weak-etag"</D:getetag>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
    <D:propstat>
      <D:prop>
        <D:getlastmodified>ignored</D:getlastmodified>
      </D:prop>
      <D:status>HTTP/1.1 404 Not Found</D:status>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/dav/PDFSAK/sub/</D:href>
    <D:propstat>
      <D:prop><D:resourcetype><D:collection/></D:resourcetype></D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

    fn segment(value: &str) -> String {
        value.to_string()
    }

    #[test]
    fn parses_apache_multistatus() {
        let entries = parse_multistatus(
            APACHE_MULTISTATUS,
            &[segment("dav"), segment("PDFSAK")],
        )
        .unwrap();
        // The collection itself is skipped; the sub-collection is returned as
        // a directory entry (callers decide whether to care).
        assert_eq!(entries.len(), 3);

        let report = &entries[0];
        assert_eq!(report.name, "report.oswk");
        assert_eq!(report.size, 1234);
        assert_eq!(report.etag.as_deref(), Some("abc123"));
        assert_eq!(report.modified.as_deref(), Some("Wed, 21 Oct 2026 07:28:00 GMT"));
        assert!(!report.is_dir);

        let encoded = &entries[1];
        assert_eq!(encoded.name, "Report Final.oswk");
        assert_eq!(encoded.etag.as_deref(), Some("weak-etag"));
        // The 404 propstat block must not contribute.
        assert_eq!(encoded.modified, None);

        let sub = &entries[2];
        assert_eq!(sub.name, "sub");
        assert!(sub.is_dir);
    }

    #[test]
    fn parses_namespace_free_and_absolute_hrefs() {
        let xml = r#"<multistatus xmlns="DAV:">
  <response>
    <href>https://host:5000/dav/PDFSAK/file%20one.oswk</href>
    <propstat><prop><getcontentlength>7</getcontentlength><getetag>E1</getetag></prop><status>HTTP/1.1 200 OK</status></propstat>
  </response>
</multistatus>"#;
        let entries = parse_multistatus(xml, &[segment("dav"), segment("PDFSAK")]).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "file one.oswk");
        assert_eq!(entries[0].size, 7);
    }

    #[test]
    fn alias_hrefs_fall_back_to_last_segment() {
        // Some servers return hrefs relative to an internal alias that does
        // not match the request prefix; the last segment is still the name.
        let xml = r#"<D:multistatus xmlns:D="DAV:">
  <D:response><D:href>/internal-alias/other.oswk</D:href>
    <D:propstat><D:prop><D:getcontentlength>5</D:getcontentlength></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat>
  </D:response>
</D:multistatus>"#;
        let entries = parse_multistatus(xml, &[segment("dav"), segment("PDFSAK")]).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "other.oswk");
    }

    #[test]
    fn malformed_xml_is_a_protocol_error() {
        let error = parse_multistatus("<D:multistatus><oops>", &[]).unwrap_err();
        assert!(matches!(error, SyncError::Protocol(_)));
    }

    #[test]
    fn rejects_deeply_nested_xml() {
        let mut xml = String::new();
        for _ in 0..200 {
            xml.push_str("<a>");
        }
        for _ in 0..200 {
            xml.push_str("</a>");
        }
        let error = parse_multistatus(&xml, &[]).unwrap_err();
        assert!(matches!(error, SyncError::Protocol(_)));
    }

    #[test]
    fn doctype_with_external_entity_is_ignored() {
        // The DTD is skipped and the entity is not resolved, so nothing from
        // the external file can ever reach the parser's output.
        let xml = r#"<?xml version="1.0"?>
<!DOCTYPE multistatus [ <!ENTITY xxe SYSTEM "file:///etc/passwd"> ]>
<D:multistatus xmlns:D="DAV:">
  <D:response><D:href>/dav/PDFSAK/%26amp%3B.oswk</D:href>
    <D:propstat><D:prop><D:getcontentlength>1</D:getcontentlength></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat>
  </D:response>
</D:multistatus>"#;
        let entries = parse_multistatus(xml, &[segment("dav"), segment("PDFSAK")]).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "&amp;.oswk");
    }

    // -- status mapping ------------------------------------------------------

    #[test]
    fn conditional_put_412_is_a_conflict() {
        let error = map_put_status(412, "PDFSAK/report.oswk", None).unwrap_err();
        match error {
            SyncError::Conflict(message) => assert!(message.contains("412")),
            other => panic!("expected Conflict, got {other:?}"),
        }
    }

    #[test]
    fn put_status_mapping() {
        assert_eq!(map_put_status(201, "a.oswk", Some("e1".into())).unwrap().as_deref(), Some("e1"));
        assert!(matches!(map_put_status(401, "a.oswk", None), Err(SyncError::Auth(_))));
        assert!(matches!(map_put_status(404, "a.oswk", None), Err(SyncError::NotFound(_))));
        assert!(matches!(map_put_status(423, "a.oswk", None), Err(SyncError::Conflict(_))));
        assert!(matches!(map_put_status(500, "a.oswk", None), Err(SyncError::Http { status: 500, .. })));
    }

    #[test]
    fn read_limit_is_enforced() {
        let data = vec![0u8; 4096];
        assert_eq!(read_limited(&data[..], 4096).unwrap().len(), 4096);
        assert!(matches!(read_limited(&data[..], 1024), Err(SyncError::TooLarge(_))));
    }

    // -- live server (opt-in) ------------------------------------------------

    /// Not run by default. To exercise it:
    /// `$env:PDFSAK_WEBDAV_URL="https://host/dav"; ... ; cargo test -p synccore -- --ignored`
    #[test]
    #[ignore = "requires a real WebDAV server and PDFSAK_WEBDAV_URL/USER/PASSWORD env vars"]
    fn live_webdav_roundtrip() {
        let Ok(url) = std::env::var("PDFSAK_WEBDAV_URL") else {
            eprintln!("skipped: PDFSAK_WEBDAV_URL is not set");
            return;
        };
        let user = std::env::var("PDFSAK_WEBDAV_USER").unwrap_or_default();
        let password = std::env::var("PDFSAK_WEBDAV_PASSWORD").unwrap_or_default();
        let provider = WebDavProvider::new_with_options(&url, &user, &password, true).unwrap();
        provider.test().unwrap();
        provider.ensure_dir("pdfsak-sync-test").unwrap();

        let path = "pdfsak-sync-test/live-test.oswk";
        let etag = provider.put(path, b"hello webdav", None).unwrap();
        let (bytes, downloaded_etag) = provider.get(path).unwrap();
        assert_eq!(bytes, b"hello webdav");
        if let (Some(put), Some(got)) = (&etag, &downloaded_etag) {
            assert_eq!(put, got);
        }
        // A conditional write against a stale etag must be refused.
        let conflict = provider
            .put(path, b"overwrite attempt", Some("definitely-not-the-current-etag"))
            .unwrap_err();
        assert!(matches!(conflict, SyncError::Conflict(_)), "expected 412 conflict");
        // The original bytes must still be there.
        let (bytes, _) = provider.get(path).unwrap();
        assert_eq!(bytes, b"hello webdav");
        provider.delete(path).unwrap();
    }
}
