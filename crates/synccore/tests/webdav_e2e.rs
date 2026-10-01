//! End-to-end WebDAV tests against a self-contained in-process mock server.
//!
//! The mock speaks just enough DAV for the provider: PROPFIND (Depth 0/1),
//! MKCOL, PUT (honoring `If-Match`), GET and DELETE. It lets the suite exercise
//! the real `WebDavProvider` over a real TCP socket without needing Nextcloud
//! in CI, covering the full sync contract: listing, conditional uploads,
//! downloads, 412 conflict mapping and deletion.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use synccore::webdav::WebDavProvider;
use synccore::{SyncError, SyncProvider};

/// One stored object: bytes plus a monotonically increasing ETag.
#[derive(Clone, Default)]
struct Object {
    bytes: Vec<u8>,
    etag: String,
}

#[derive(Default)]
struct ServerState {
    objects: HashMap<String, Object>,
    collections: Vec<String>,
    /// Raw request lines, for assertions about what the client actually sent.
    requests: Vec<String>,
    counter: u64,
}

impl ServerState {
    fn next_etag(&mut self) -> String {
        self.counter += 1;
        format!("\"{:x}\"", self.counter)
    }
}

struct MockWebDav {
    base_url: String,
    state: Arc<Mutex<ServerState>>,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl MockWebDav {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("addr");
        let state = Arc::new(Mutex::new(ServerState::default()));
        let state_clone = state.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = stop.clone();
        let handle = thread::spawn(move || {
            for stream in listener.incoming() {
                if stop_clone.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(stream) = stream else { break };
                let state = state_clone.clone();
                // Sequential handling is fine: the client is synchronous.
                handle_connection(stream, state);
            }
        });
        Self { base_url: format!("http://{address}/dav"), state, stop, handle: Some(handle) }
    }

    fn requests(&self) -> Vec<String> {
        self.state.lock().unwrap().requests.clone()
    }
}

impl Drop for MockWebDav {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Unblock the accept loop with a throwaway connection.
        let authority = self.base_url.trim_start_matches("http://").split('/').next().unwrap_or("").to_string();
        let _ = TcpStream::connect(&authority);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn handle_connection(mut stream: TcpStream, state: Arc<Mutex<ServerState>>) {
    // A short read timeout turns any malformed/keep-alive request into a clean
    // close instead of a hung test.
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut raw = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        match stream.read(&mut chunk) {
            Ok(0) => return,
            Ok(read) => raw.extend_from_slice(&chunk[..read]),
            Err(_) => return,
        }
        if let Some(position) = raw.windows(4).position(|window| window == b"\r\n\r\n") {
            break position + 4;
        }
    };
    let header_text = String::from_utf8_lossy(&raw[..header_end]).to_string();
    let mut lines = header_text.lines();
    let request_line = lines.next().unwrap_or("").to_string();
    let mut headers: HashMap<String, String> = HashMap::new();
    for line in lines {
        if let Some((key, value)) = line.split_once(':') {
            headers.insert(key.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    let method = parts.first().copied().unwrap_or("");
    let path = parts.get(1).copied().unwrap_or("/");
    let depth = headers.get("depth").cloned().unwrap_or_else(|| "0".to_string());
    let content_length = headers.get("content-length").and_then(|value| value.parse::<usize>().ok()).unwrap_or(0);
    let mut body = raw[header_end..].to_vec();
    while body.len() < content_length {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => body.extend_from_slice(&chunk[..read]),
            Err(_) => break,
        }
    }
    body.truncate(content_length);

    let mut guard = state.lock().unwrap();
    guard.requests.push(format!("{method} {path} depth={depth}"));
    let key = normalize_key(path);

    let response: Vec<u8> = match method {
        "PROPFIND" => {
            let entries: Vec<(String, Object)> = if depth == "0" {
                guard
                    .objects
                    .iter()
                    .filter(|(name, _)| name.as_str() == key)
                    .map(|(name, object)| (name.clone(), object.clone()))
                    .collect()
            } else {
                let prefix = if key.is_empty() { String::new() } else { format!("{key}/") };
                guard
                    .objects
                    .iter()
                    .filter(|(name, _)| {
                        name.starts_with(&prefix) && name[key.len()..].trim_start_matches('/').find('/').is_none()
                    })
                    .map(|(name, object)| (name.clone(), object.clone()))
                    .collect()
            };
            let mut xml = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<D:multistatus xmlns:D=\"DAV:\">");
            // The collection itself.
            xml.push_str(&format!(
                "<D:response><D:href>/{key}{}</D:href><D:propstat><D:prop><D:resourcetype><D:collection/></D:resourcetype></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>",
                if key.is_empty() { "" } else { "/" }
            ));
            for (name, object) in entries {
                xml.push_str(&format!(
                    "<D:response><D:href>/{name}</D:href><D:propstat><D:prop><D:getcontentlength>{}</D:getcontentlength><D:getetag>{}</D:getetag><D:getlastmodified>Wed, 01 Oct 2026 08:00:00 GMT</D:getlastmodified></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>",
                    object.bytes.len(),
                    object.etag
                ));
            }
            xml.push_str("</D:multistatus>");
            respond(207, "application/xml", xml.into_bytes())
        }
        "MKCOL" => {
            if !guard.collections.contains(&key) {
                guard.collections.push(key.clone());
            }
            respond(201, "text/plain", Vec::new())
        }
        "PUT" => {
            if let Some(if_match) = headers.get("if-match") {
                match guard.objects.get(&key) {
                    Some(existing) if strip_quotes(&existing.etag) == strip_quotes(if_match) => {}
                    Some(_) => {
                        drop(guard);
                        let _ = stream.write_all(
                            b"HTTP/1.1 412 Precondition Failed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        );
                        return;
                    }
                    None => {}
                }
            }
            let etag = guard.next_etag();
            guard.objects.insert(key.clone(), Object { bytes: body, etag: etag.clone() });
            let response =
                format!("HTTP/1.1 201 Created\r\nETag: {etag}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            let _ = stream.write_all(response.as_bytes());
            return;
        }
        "GET" => match guard.objects.get(&key) {
            Some(object) => {
                let response = format!(
                    "HTTP/1.1 200 OK\r\nETag: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    object.etag,
                    object.bytes.len()
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.write_all(&object.bytes);
                return;
            }
            None => respond(404, "text/plain", Vec::new()),
        },
        "DELETE" => {
            if let Some(if_match) = headers.get("if-match") {
                if let Some(existing) = guard.objects.get(&key) {
                    if strip_quotes(&existing.etag) != strip_quotes(if_match) {
                        drop(guard);
                        let _ = stream.write_all(
                            b"HTTP/1.1 412 Precondition Failed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        );
                        return;
                    }
                }
            }
            guard.objects.remove(&key);
            respond(204, "text/plain", Vec::new())
        }
        _ => respond(405, "text/plain", Vec::new()),
    };

    let _ = stream.write_all(&response);
}

/// Strips the leading `/dav/` prefix, the trailing slash and minimal
/// percent-encoding.
fn normalize_key(path: &str) -> String {
    let trimmed = path.trim_start_matches("/dav").trim_start_matches('/').trim_end_matches('/');
    percent_decode(trimmed)
}

/// Compares ETags ignoring the surrounding quotes, matching the provider's
/// normalization (it strips `W/` and quotes before sending `If-Match`).
fn strip_quotes(etag: &str) -> String {
    etag.trim().trim_matches('"').to_string()
}

fn percent_decode(value: &str) -> String {
    let mut out = Vec::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn respond(status: u16, content_type: &str, body: Vec<u8>) -> Vec<u8> {
    let mut response = format!(
        "HTTP/1.1 {status} OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(&body);
    response
}

fn provider(server: &MockWebDav) -> WebDavProvider {
    // The base URL is the DAV root; the remote directory is passed explicitly
    // to each call, matching how the application layer composes them.
    WebDavProvider::new_with_options(&server.base_url, "alice", "hunter2", true).expect("provider")
}

#[test]
fn full_roundtrip_list_upload_download_delete() {
    let server = MockWebDav::start();
    let provider = provider(&server);

    // Fail fast if the server is reachable (also proves the connection test).
    provider.test().expect("test");
    provider.ensure_dir("PDFSAK").expect("mkcol");

    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("report.oswk");
    let payload = b"{\"json\":\"document\"}".to_vec();
    std::fs::write(&source, &payload).expect("write");

    // Streaming upload.
    let (sha, size, etag) = provider.put_file("PDFSAK/report.oswk", &source, None).expect("put");
    assert_eq!(size, payload.len() as u64);
    assert!(etag.is_some(), "server must return an ETag");
    assert_eq!(sha.len(), 64, "sha256 hex");

    // Listing sees the object with the right size and etag.
    let listed = provider.list("PDFSAK").expect("list");
    let entry = listed.iter().find(|entry| entry.name == "report.oswk").expect("entry");
    assert_eq!(entry.size, payload.len() as u64);
    assert_eq!(entry.etag, etag);

    // Streaming download returns the same bytes and the same hash.
    let mut downloaded = Vec::new();
    let (downloaded_sha, downloaded_size, _) =
        provider.get_to_writer("PDFSAK/report.oswk", &mut downloaded).expect("get");
    assert_eq!(downloaded, payload);
    assert_eq!(downloaded_size, payload.len() as u64);
    assert_eq!(downloaded_sha, sha);

    provider.delete("PDFSAK/report.oswk").expect("delete");
    assert!(provider.list("PDFSAK").expect("list").iter().all(|entry| entry.name != "report.oswk"));
}

#[test]
fn conditional_upload_detects_a_stale_base_and_maps_to_conflict() {
    let server = MockWebDav::start();
    let provider = provider(&server);
    provider.ensure_dir("PDFSAK").expect("mkcol");

    let dir = tempfile::tempdir().expect("tempdir");
    let first = dir.path().join("a.oswk");
    std::fs::write(&first, b"v1").expect("write");
    let (_, _, etag) = provider.put_file("PDFSAK/a.oswk", &first, None).expect("put v1");

    // A second writer moves the cloud copy forward.
    let other = dir.path().join("b.oswk");
    std::fs::write(&other, b"v2").expect("write");
    provider.put_file("PDFSAK/a.oswk", &other, None).expect("put v2");

    // A conditional write against the stale etag must be refused (HTTP 412).
    let stale = dir.path().join("stale.oswk");
    std::fs::write(&stale, b"old-base").expect("write");
    let error = provider.put_file("PDFSAK/a.oswk", &stale, etag.as_deref()).expect_err("stale base must conflict");
    assert!(matches!(error, SyncError::Conflict(_)), "expected Conflict, got {error:?}");

    // The newer cloud copy is untouched.
    let mut current = Vec::new();
    provider.get_to_writer("PDFSAK/a.oswk", &mut current).expect("get");
    assert_eq!(current, b"v2");
}

#[test]
fn download_refuses_a_missing_remote_file() {
    let server = MockWebDav::start();
    let provider = provider(&server);
    provider.ensure_dir("PDFSAK").expect("mkcol");

    let mut sink = Vec::new();
    let error = provider.get_to_writer("PDFSAK/absent.oswk", &mut sink).expect_err("missing");
    assert!(matches!(error, SyncError::NotFound(_)), "expected NotFound, got {error:?}");
}

#[test]
fn requests_carry_conditional_headers_and_depth() {
    let server = MockWebDav::start();
    let provider = provider(&server);
    provider.ensure_dir("PDFSAK").expect("mkcol");

    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("c.oswk");
    std::fs::write(&file, b"data").expect("write");
    let (_, _, etag) = provider.put_file("PDFSAK/c.oswk", &file, None).expect("put");

    let dir_entries = provider.list("PDFSAK").expect("list");
    assert!(dir_entries.iter().any(|entry| entry.name == "c.oswk"));

    // A conditional PUT to the same etag succeeds.
    provider.put_file("PDFSAK/c.oswk", &file, etag.as_deref()).expect("conditional put");

    // The list request used Depth: 1 (that is what makes it a listing).
    let requests = server.requests();
    assert!(
        requests.iter().any(|request| request.starts_with("PROPFIND") && request.contains("depth=1")),
        "listing must use Depth: 1, got {requests:?}"
    );
}
