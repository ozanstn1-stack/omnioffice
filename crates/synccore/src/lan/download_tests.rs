//! Tests for resumable downloads (E6.3), against a real loopback host.

use super::*;
use crate::lan::download::is_unknown_operation;
use sha2::{Digest, Sha256};

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
    let identity = LanIdentity::generate("host-get", "Atölye PC").expect("identity");
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

fn paired(root: &Path) -> (LanHost, LanClient, PairedPeer) {
    let (host, ticket, address) = host_at(root);
    let client = LanClient::new("client-get", "Telefon");
    let peer = client.pair(&address, ticket.code()).expect("pairing");
    (host, client, peer)
}

#[test]
fn a_chunked_download_resumes_from_the_partial_file() {
    let root = folder();
    // ~1.3 MiB: several chunks of 512 KiB.
    let body: Vec<u8> = (0..(CHUNK_BYTES * 2 + 12_345)).map(|index| (index % 251) as u8).collect();
    write_doc(root.path(), "buyuk.pdf", &body);
    let (_host, client, peer) = paired(root.path());

    // A previous run got through the first chunk and then died.
    let (partial, note) = download::partial_paths(root.path(), "buyuk.pdf");
    std::fs::create_dir_all(partial.parent().expect("parts")).expect("parts dir");
    std::fs::write(&partial, &body[..CHUNK_BYTES]).expect("partial");
    let note_value = download::PartialNote { sha256: hex_encode(&Sha256::digest(&body)), size: body.len() as u64 };
    std::fs::write(&note, serde_json::to_string(&note_value).expect("json")).expect("note");

    let mut seen = Vec::new();
    let (file, bytes) = client
        .fetch_resumable(&peer, "buyuk.pdf", root.path(), |done, total| seen.push((done, total)))
        .expect("resumable download");

    assert_eq!(bytes, body, "the bytes match the document");
    assert_eq!(file.size, body.len() as u64);
    assert_eq!(file.sha256, hex_encode(&Sha256::digest(&body)));
    // Resumed, not restarted: the first chunk it fetched already starts at the
    // offset the partial file had, so the first progress report is past it.
    assert!(seen.first().copied().unwrap_or((0, 0)).0 > CHUNK_BYTES as u64, "seen {seen:?}");
    assert!(!partial.exists(), "the partial file is consumed");
    assert!(!note.exists(), "the note is consumed");
}

#[test]
fn a_download_restarts_when_the_host_file_changed() {
    let root = folder();
    let original: Vec<u8> = (0..(CHUNK_BYTES + 4_096)).map(|index| (index % 97) as u8).collect();
    write_doc(root.path(), "rapor.docx", &original);
    let (_host, client, peer) = paired(root.path());

    // A partial from a *different* document of the same length: only the hash
    // tells them apart, which is what the note is for.
    let (partial, note) = download::partial_paths(root.path(), "rapor.docx");
    std::fs::create_dir_all(partial.parent().expect("parts")).expect("parts dir");
    std::fs::write(&partial, vec![0u8; CHUNK_BYTES]).expect("partial");
    let stale = download::PartialNote { sha256: "0".repeat(64), size: original.len() as u64 };
    std::fs::write(&note, serde_json::to_string(&stale).expect("json")).expect("note");

    let (_, bytes) = client.fetch_resumable(&peer, "rapor.docx", root.path(), |_, _| {}).expect("download");
    assert_eq!(bytes, original, "the stale partial is discarded, not resumed");
}

#[test]
fn a_corrupted_partial_is_discarded_after_the_hash_check() {
    let root = folder();
    let body: Vec<u8> = (0..(CHUNK_BYTES + 7_777)).map(|index| (index % 61) as u8).collect();
    write_doc(root.path(), "bozuk.pdf", &body);
    let (_host, client, peer) = paired(root.path());

    // Right length, right note, wrong bytes: only the final hash catches it.
    let (partial, note) = download::partial_paths(root.path(), "bozuk.pdf");
    std::fs::create_dir_all(partial.parent().expect("parts")).expect("parts dir");
    std::fs::write(&partial, vec![0xABu8; CHUNK_BYTES]).expect("partial");
    let value = download::PartialNote { sha256: hex_encode(&Sha256::digest(&body)), size: body.len() as u64 };
    std::fs::write(&note, serde_json::to_string(&value).expect("json")).expect("note");

    let (_, bytes) = client.fetch_resumable(&peer, "bozuk.pdf", root.path(), |_, _| {}).expect("download");
    assert_eq!(bytes, body, "a corrupted partial never reaches the caller");
}

#[test]
fn a_download_chunk_checks_offsets_sizes_and_paths() {
    let root = folder();
    let body = vec![7u8; 4_096];
    write_doc(root.path(), "kucuk.pdf", &body);
    let (_host, client, peer) = paired(root.path());

    let size = body.len() as u64;
    // An offset past the end is refused instead of returning an empty body.
    assert!(client.request(&peer, "get_chunk", "kucuk.pdf", None, size + 1, size, "").is_err());
    // A size the client believes but the host does not share is refused.
    assert!(client.request(&peer, "get_chunk", "kucuk.pdf", None, 0, size + 10, "").is_err());
    // The path rules still apply to the new operations.
    assert!(client.request(&peer, "begin_get", "../gizli.pdf", None, 0, 0, "").is_err());
    assert!(client.request(&peer, "begin_get", ".parts/x.get", None, 0, 0, "").is_err());
    assert!(client.request(&peer, "begin_get", "kurulum.exe", None, 0, 0, "").is_err());
}

#[test]
fn an_empty_document_downloads_without_chunks() {
    let root = folder();
    write_doc(root.path(), "bos.txt", b"");
    let (_host, client, peer) = paired(root.path());

    let mut calls = 0;
    let (file, bytes) = client.fetch_resumable(&peer, "bos.txt", root.path(), |_, _| calls += 1).expect("download");
    assert!(bytes.is_empty());
    assert_eq!(file.size, 0);
    assert_eq!(calls, 0, "no chunk is requested for an empty document");
}

#[test]
fn an_unknown_operation_is_recognised_for_the_whole_file_fallback() {
    let root = folder();
    write_doc(root.path(), "rapor.docx", b"x");
    let (_host, client, peer) = paired(root.path());

    // The host answers an operation it never heard of with "unknown operation";
    // that message is what turns a resumable attempt into the whole-file path.
    let error = client.request(&peer, "begin_transfer", "rapor.docx", None, 0, 0, "").expect_err("refused");
    assert!(is_unknown_operation(&error), "got {error:?}");
    // A genuinely wrong request is not mistaken for an old host.
    let wrong = client.request(&peer, "begin_get", "../x.pdf", None, 0, 0, "").expect_err("refused");
    assert!(!is_unknown_operation(&wrong), "got {wrong:?}");
}
