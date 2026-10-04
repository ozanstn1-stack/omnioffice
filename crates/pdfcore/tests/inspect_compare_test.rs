//! Inspector and comparison against real documents.

mod common;

use common::*;
use lopdf::dictionary;
use pdfcore::compare::{compare_pdfs, CompareOptions};
use pdfcore::inspect::{inspect_document, Severity};
use pdfcore::progress::CancelToken;

#[test]
fn inspection_reports_the_facts_of_a_known_document() {
    let dir = TestDir::new();
    let mut doc = build_text_doc(3, "Report", "Quarterly Report");
    let path = dir.path("report.pdf");
    write_doc(&mut doc, &path);

    let inspection = inspect_document(&path, None).expect("inspect");
    assert_eq!(inspection.page_count, 3);
    assert_eq!(inspection.pdf_version, "1.7");
    assert!(!inspection.encrypted);
    assert_eq!(inspection.title_override, "Quarterly Report");
    assert_eq!(inspection.author_override, "Test Suite");
    assert_eq!(inspection.file_size_bytes, std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0));
    assert!(!inspection.fonts.is_empty(), "the fixture uses a Type1 font");
    assert!(inspection.fonts.iter().any(|font| font.name == "F1"));
    // The base-14 fixture font has no font program, which is exactly what the
    // PDF/A check is for.
    assert!(inspection.fonts.iter().any(|font| !font.embedded));
}

#[test]
fn inspection_of_an_image_document_counts_the_images() {
    let dir = TestDir::new();
    let mut doc = build_image_doc(2, "Scan");
    let path = dir.path("scan.pdf");
    write_doc(&mut doc, &path);
    let inspection = inspect_document(&path, None).expect("inspect");
    assert_eq!(inspection.page_count, 2);
    // Both pages embed an image with the same geometry, encoding and colour
    // space, so the report lists one image drawn twice.
    assert_eq!(inspection.images.len(), 1, "{inspection:?}");
    assert_eq!(inspection.images[0].width, 900);
    assert_eq!(inspection.images[0].height, 1300);
    assert_eq!(inspection.images[0].occurrences, 2);
    assert!(inspection.total_image_pixels > 0);
    assert!(inspection.color_spaces.iter().any(|space| space.name == "DeviceRGB"));
}

#[test]
fn inspection_finds_embedded_files_and_javascript() {
    let dir = TestDir::new();
    let mut doc = build_text_doc(1, "With extras", "Extras");
    let file_spec = lopdf::Dictionary::new();
    let embedded = doc.add_object(lopdf::Object::Stream(lopdf::Stream::new(file_spec, b"attachment body".to_vec())));
    let files = doc.add_object(lopdf::Object::Dictionary(lopdf::dictionary! {
        "Names" => vec![lopdf::Object::String(b"notes.txt".to_vec(), lopdf::StringFormat::Literal), lopdf::Object::Reference(embedded)],
    }));
    let names = doc.add_object(lopdf::Object::Dictionary(lopdf::dictionary! {
        "EmbeddedFiles" => lopdf::Object::Reference(files),
    }));
    let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
    if let Ok(lopdf::Object::Dictionary(catalog)) = doc.get_object_mut(root) {
        catalog.set("Names", lopdf::Object::Reference(names));
        catalog.set("JavaScript", lopdf::Object::Name(b"this.print".to_vec()));
    }
    let path = dir.path("extras.pdf");
    write_doc(&mut doc, &path);

    let inspection = inspect_document(&path, None).expect("inspect");
    assert!(inspection.has_javascript, "the catalog declares JavaScript");
    assert!(inspection
        .findings
        .iter()
        .any(|entry| entry.code == "security.javascript" && entry.severity == Severity::Warning));
}

#[test]
fn an_untagged_document_fails_accessibility_with_an_explanation() {
    let dir = TestDir::new();
    let mut doc = build_text_doc(1, "Scan", "Scanned invoice");
    let path = dir.path("plain.pdf");
    write_doc(&mut doc, &path);
    let inspection = inspect_document(&path, None).expect("inspect");
    assert!(!inspection.struct_tree);
    assert_eq!(inspection.accessibility_conformance, "fails");
    let finding =
        inspection.findings.iter().find(|entry| entry.code == "a11y.untagged").expect("untagged must be reported");
    assert_eq!(finding.severity, Severity::Error);
    assert!(!finding.detail.is_empty(), "a finding must say what to do");
}

#[test]
fn inspection_of_a_missing_file_is_an_error_not_an_empty_report() {
    let dir = TestDir::new();
    let error = inspect_document(&dir.path("nope.pdf"), None);
    assert!(error.is_err());
}

#[test]
fn comparing_a_document_with_itself_finds_nothing() {
    let dir = TestDir::new();
    let mut doc = build_text_doc(2, "Same", "Same");
    let path = dir.path("same.pdf");
    write_doc(&mut doc, &path);
    let report = compare_pdfs(&path, &path, None, None, &CompareOptions::default(), &no_progress, &CancelToken::new())
        .expect("compare");
    assert!(report.identical, "a document is identical to itself: {report:?}");
    assert!(report.text_differences.is_empty());
}

#[test]
fn comparing_different_documents_reports_the_change() {
    if !pdfcore::render::is_available() {
        eprintln!("pdfium is not available; skipping the comparison round trip");
        return;
    }
    let dir = TestDir::new();
    let mut left = build_text_doc(1, "Before", "Before");
    let mut right = build_text_doc(1, "After", "After");
    let left_path = dir.path("left.pdf");
    let right_path = dir.path("right.pdf");
    write_doc(&mut left, &left_path);
    write_doc(&mut right, &right_path);

    let report = compare_pdfs(
        &left_path,
        &right_path,
        None,
        None,
        &CompareOptions::default(),
        &no_progress,
        &CancelToken::new(),
    )
    .expect("compare");
    assert!(!report.identical);
    assert_eq!(report.left_pages, 1);
    assert_eq!(report.right_pages, 1);
    assert_eq!(report.text_differences.len(), 1, "one page changed: {report:?}");
    assert_eq!(report.text_differences[0].page, 1);
    assert_eq!(report.text_differences[0].kind, "changed");
    assert!(report.text_differences[0].left.contains("Before"));
    assert!(report.text_differences[0].right.contains("After"));
}

#[test]
fn extra_pages_are_reported_as_added_or_removed() {
    let dir = TestDir::new();
    let mut one = build_text_doc(1, "One", "One");
    let mut three = build_text_doc(3, "One", "One");
    let short = dir.path("short.pdf");
    let long = dir.path("long.pdf");
    write_doc(&mut one, &short);
    write_doc(&mut three, &long);
    let report = compare_pdfs(&short, &long, None, None, &CompareOptions::default(), &no_progress, &CancelToken::new())
        .expect("compare");
    assert_eq!(report.added_pages, vec![2, 3]);
    assert!(report.removed_pages.is_empty());
    assert!(!report.identical);
}

#[test]
fn a_missing_input_is_reported() {
    let dir = TestDir::new();
    let error = compare_pdfs(
        &dir.path("a.pdf"),
        &dir.path("b.pdf"),
        None,
        None,
        &CompareOptions::default(),
        &no_progress,
        &CancelToken::new(),
    );
    assert!(error.is_err());
}

#[test]
fn a_cancelled_comparison_reports_cancelled() {
    let dir = TestDir::new();
    let mut doc = build_text_doc(1, "Same", "Same");
    let path = dir.path("same.pdf");
    write_doc(&mut doc, &path);
    let token = CancelToken::new();
    token.cancel();
    let result = compare_pdfs(&path, &path, None, None, &CompareOptions::default(), &no_progress, &token);
    // The text pass checks the token per page, so a single-page document still
    // reports Cancelled rather than a misleading "identical".
    assert!(result.is_err(), "a cancelled comparison must not report success");
}
