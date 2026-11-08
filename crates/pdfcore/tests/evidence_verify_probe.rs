//! What an evidence bundle promises when it is reopened.
//!
//! The bundle embeds its manifest as a file attachment and records the digest of
//! the document *without* that record. Verification recomputes the digest and
//! compares, so an edit anywhere in the visible document has to show up as a
//! mismatch, and a truncated file has to come back as an error rather than a
//! panic or a false "valid". Matching an original goes through the exhibit's own
//! digest, so a re-saved or re-ordered file is not the same document.
//!
//! These cases read only the public result values; no production helper decides
//! what the expectation is.

use lopdf::{dictionary, Document, Object, Stream};
use pdfcore::evidence::{build_evidence_bundle, match_original, verify_evidence_bundle, EvidenceDetails, EvidenceFile};

fn rect(values: [f64; 4]) -> Object {
    Object::Array(values.iter().map(|value| Object::Real(*value as f32)).collect())
}

/// A `pages`-page PDF whose page content carries `tag`.
fn exhibit(pages: u32, tag: &str) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let mut kids = Vec::new();
    for index in 0..pages {
        let text = format!("BT /F1 12 Tf 72 720 Td ({tag} {index}) Tj ET");
        let content_id = doc.add_object(Stream::new(dictionary! {}, text.into_bytes()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => rect([0.0, 0.0, 595.0, 842.0]),
            "Contents" => Object::Reference(content_id),
        });
        kids.push(Object::Reference(page_id));
    }
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Count" => pages as i64, "Kids" => kids }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) });
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save the exhibit");
    out
}

fn details() -> EvidenceDetails {
    EvidenceDetails {
        title: "Delil Listesi".into(),
        case_reference: "2026/123 E.".into(),
        court: String::new(),
        submitted_by: String::new(),
        language: "tr".into(),
    }
}

fn bundle(bytes: Vec<u8>) -> Vec<u8> {
    let files = vec![EvidenceFile { label: "Ek-1".into(), file_name: "ek-1.pdf".into(), bytes }];
    build_evidence_bundle(files, &details()).expect("bundle").0
}

/// Rewrites the first page's content, the way another tool would.
fn edit_first_page(bundle: &[u8]) -> Vec<u8> {
    let mut doc = Document::load_mem(bundle).expect("load the bundle");
    let page_id = *doc.get_pages().get(&1).expect("page one");
    // `/Contents` may be one stream or an array of them (ISO 32000-1 7.8.2).
    let target = match doc.get_dictionary(page_id).and_then(|page| page.get(b"Contents")).expect("content") {
        Object::Reference(id) => *id,
        Object::Array(items) => items.iter().filter_map(|item| item.as_reference().ok()).next().expect("a stream"),
        other => panic!("unexpected /Contents: {other:?}"),
    };
    if let Ok(stream) = doc.get_object_mut(target).and_then(Object::as_stream_mut) {
        stream.content.extend_from_slice(b"\n% edited");
    }
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save the edited bundle");
    out
}

/// Flips one hexadecimal digit of the digest recorded in the manifest, leaving
/// the JSON parseable: the manifest now names a document that does not exist.
fn corrupt_recorded_digest(bundle: &[u8]) -> Vec<u8> {
    // The manifest is written pretty-printed, and the bundle's own digest is the
    // last one in the file: the entries' digests come before it.
    let needle = b"\"sha256\": \"";
    let start = bundle
        .windows(needle.len())
        .rposition(|window| window == needle)
        .expect("the manifest records the bundle digest")
        + needle.len();
    let mut out = bundle.to_vec();
    let digit = out[start];
    out[start] = if digit == b'a' { b'b' } else { b'a' };
    assert!(out[start].is_ascii_hexdigit(), "the digest is hexadecimal");
    out
}

#[test]
fn a_fresh_bundle_verifies() {
    let bundle = bundle(exhibit(2, "A"));
    let report = verify_evidence_bundle(&bundle).expect("verify");
    assert!(report.present, "the bundle carries a manifest");
    assert!(report.content_matches, "an untouched bundle matches its record");
    assert!(!report.actual_sha256.is_empty(), "the digest is reported");
    assert_eq!(report.manifest.expect("manifest").entries.len(), 1);
}

#[test]
fn an_edited_bundle_does_not_verify() {
    let bundle = bundle(exhibit(2, "A"));
    let edited = edit_first_page(&bundle);
    let report = verify_evidence_bundle(&edited).expect("verify");
    assert!(report.present, "the manifest is still there");
    assert!(!report.content_matches, "a page edit must show up as a mismatch");
}

#[test]
fn a_bundle_whose_recorded_digest_was_changed_does_not_verify() {
    let bundle = bundle(exhibit(2, "A"));
    let tampered = corrupt_recorded_digest(&bundle);
    let report = verify_evidence_bundle(&tampered).expect("verify");
    assert!(!report.content_matches, "the recorded digest no longer matches the document");
}

#[test]
fn a_truncated_bundle_is_an_error_not_a_panic() {
    let bundle = bundle(exhibit(2, "A"));
    let truncated = &bundle[..bundle.len() - 200];
    let outcome = verify_evidence_bundle(truncated);
    assert!(outcome.is_err(), "a file cut in the middle of its trailer is refused, got {outcome:?}");
}

#[test]
fn a_plain_pdf_carries_no_manifest() {
    let report = verify_evidence_bundle(&exhibit(1, "plain")).expect("verify");
    assert!(!report.present, "nothing to verify");
    assert!(report.manifest.is_none());
}

#[test]
fn an_original_matches_its_exhibit() {
    let original = exhibit(3, "A");
    let bundle = bundle(original.clone());
    let (_, manifest) = {
        let files = vec![EvidenceFile { label: "Ek-1".into(), file_name: "ek-1.pdf".into(), bytes: original.clone() }];
        build_evidence_bundle(files, &details()).expect("bundle")
    };
    assert!(verify_evidence_bundle(&bundle).expect("verify").content_matches);
    assert_eq!(match_original(&manifest, &original), Some(1), "the same bytes are the same exhibit");
}

#[test]
fn a_different_document_is_not_the_same_exhibit() {
    let original = exhibit(3, "A");
    let (_, manifest) = {
        let files = vec![EvidenceFile { label: "Ek-1".into(), file_name: "ek-1.pdf".into(), bytes: original.clone() }];
        build_evidence_bundle(files, &details()).expect("bundle")
    };
    // Same first two pages, a different third: a page-level match would be
    // wrong here, because the exhibit is the document that was submitted.
    let changed = exhibit(2, "A");
    assert_eq!(match_original(&manifest, &changed), None, "a shorter document is not the exhibit");
    let reordered = exhibit(3, "B");
    assert_eq!(match_original(&manifest, &reordered), None, "different content is not the exhibit");
    assert_eq!(match_original(&manifest, b""), None, "empty input matches nothing");
}

#[test]
fn an_empty_exhibit_list_is_refused() {
    assert!(build_evidence_bundle(Vec::new(), &details()).is_err(), "a bundle needs an exhibit");
    let empty = vec![EvidenceFile { label: "Ek-1".into(), file_name: "ek-1.pdf".into(), bytes: Vec::new() }];
    assert!(build_evidence_bundle(empty, &details()).is_err(), "an empty file is not an exhibit");
}
