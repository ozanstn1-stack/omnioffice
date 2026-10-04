//! Comparison and inspection against the real sample documents, so the reports
//! are known to be meaningful for files a user would actually open.

mod common;

use common::*;
use pdfcore::compare::{compare_pdfs, CompareOptions};
use pdfcore::inspect::{inspect_document, Severity};
use pdfcore::progress::CancelToken;

#[test]
fn comparing_two_real_samples_reports_the_differences() {
    let left = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/sample-1.pdf");
    let right = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/sample-2.pdf");
    let report = compare_pdfs(&left, &right, None, None, &CompareOptions::default(), &no_progress, &CancelToken::new())
        .expect("compare the real samples");
    // sample-1 has five pages, sample-2 three.
    assert_eq!(report.left_pages, 5);
    assert_eq!(report.right_pages, 3);
    assert!(!report.identical);
    assert!(report.removed_pages.contains(&4) && report.removed_pages.contains(&5), "{report:?}");
    // The shared pages carry different text, so they must be reported as
    // changed rather than silently treated as equal.
    assert!(report.text_differences.iter().any(|entry| entry.kind == "changed"), "{report:?}");
    for entry in &report.text_differences {
        assert!(entry.page <= 3, "a page that only exists on the left is not a text change: {entry:?}");
        assert!(entry.left != entry.right, "a 'changed' entry with identical text is a false positive: {entry:?}");
    }
}

#[test]
fn a_real_sample_inspects_without_missing_anything() {
    for sample in ["sample-1.pdf", "sample-2.pdf", "sample-images.pdf", "sample-ocr.pdf"] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples").join(sample);
        let report = inspect_document(&path, None).unwrap_or_else(|error| panic!("{sample}: {error}"));
        assert_eq!(report.file_size_bytes, std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0), "{sample}");
        assert!(report.page_count > 0, "{sample}");
        assert!(!report.pdf_version.is_empty(), "{sample}");
        assert!(!report.findings.is_empty(), "{sample}: a synthetic sample always has something wrong");
        // The UI indexes into these, so an empty one where there should be
        // content would mean the screen renders a blank tab.
        if sample == "sample-1.pdf" {
            assert_eq!(report.fonts.len(), 1, "{sample}");
            assert!(!report.fonts[0].embedded, "the base-14 font is not embedded");
        }
        if sample == "sample-images.pdf" {
            assert_eq!(report.images.len(), 1, "{sample}");
            assert!(report.total_image_pixels > 0, "{sample}");
            assert!(report.page_count == 2, "{sample}");
        }
        // Untagged synthetic documents must fail, not quietly pass.
        assert_eq!(report.accessibility_conformance, "fails", "{sample}");
        assert!(
            report.findings.iter().any(|finding| finding.severity == Severity::Error),
            "{sample}: an untagged document has blocking problems"
        );
    }
}
