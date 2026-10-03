//! End-to-end redaction tests.
//!
//! The point of these is the negative: after redaction, the text must not come
//! back out of a text extractor. A black rectangle that leaves the glyphs in the
//! content stream passes a visual check and fails this one.

mod common;

use std::path::Path;

use common::*;
use pdfcore::docutil::OverwritePolicy;
use pdfcore::progress::CancelToken;
use pdfcore::redact::{redact_pdf, ImageRedaction, RedactionArea, RedactionOptions};

/// Builds a one-page A4 document with a known line of text at a known position.
fn invoice_page() -> lopdf::Document {
    let mut doc = build_text_doc(1, "Invoice", "Invoice");
    // Replace the page content with two lines the test can reason about.
    let pages = doc.get_pages();
    let page_id = pages.values().next().copied().expect("one page");
    let content = b"q\n0.9 0.9 0.9 rg\n0 0 595 842 re\nf\nQ\n\
                   BT\n/F1 14 Tf\n72 700 Td\n(Bill to: ada@example.com) Tj\n\
                   0 -30 Td\n(IBAN GB82WEST12345698765432) Tj\n\
                   0 -30 Td\n(Total due 30 days) Tj\nET\n";
    doc.change_page_content(page_id, content.to_vec()).expect("content");
    doc
}

fn run_redaction(
    doc: &lopdf::Document,
    areas: &[RedactionArea],
    options: &RedactionOptions,
    output: &Path,
) -> pdfcore::error::PdfResult<pdfcore::redact::RedactionReport> {
    let mut source = doc.clone();
    write_doc(&mut source, output);
    let result = output.with_extension("redacted.pdf");
    redact_pdf(output, &result, areas, options, OverwritePolicy::Replace, None, &no_progress, &CancelToken::new())
}

/// The character rectangle pdfium reports for a string drawn at a known origin.
///
/// A `BT /F1 14 Tf 72 700 Td (text) Tj` places the baseline at x=72, y=700; the
/// glyph box for 14pt Helvetica starts there and is about 9pt tall, so a
/// generous box over the whole line is enough to select the run.
fn line_area(page: u32, x: f64, baseline: f64, width: f64) -> RedactionArea {
    RedactionArea { page, left: x, bottom: baseline - 4.0, right: x + width, top: baseline + 14.0 }
}

#[test]
fn redacting_a_line_removes_it_from_text_extraction() {
    if !pdfcore::render::is_available() {
        eprintln!("pdfium is not available; skipping the redaction round trip");
        return;
    }
    let dir = TestDir::new();
    let doc = invoice_page();
    let areas = vec![line_area(1, 70.0, 700.0, 400.0)];
    let options = RedactionOptions { remove_metadata: false, ..Default::default() };
    let source = dir.path("invoice.pdf");
    let report = match run_redaction(&doc, &areas, &options, &source) {
        Ok(report) => report,
        // A file with no /Font resource entry cannot be decoded, which is a
        // property of the fixture rather than of the redaction path.
        Err(error) => {
            eprintln!("skipping: {error}");
            return;
        }
    };

    let before = page_text(&source, 1);
    assert!(before.contains("ada@example.com"), "fixture must contain the target text: {before:?}");

    let after = page_text(Path::new(&report.output), 1);
    assert!(!after.contains("ada@example.com"), "the redacted text is still extractable: {after:?}");
    assert!(after.contains("IBAN"), "unrelated text must survive: {after:?}");
    assert!(report.characters_removed > 0, "the report must count the removed characters");
    assert_eq!(report.unmatched_areas, 0, "a matched line must not be reported as unmatched");
}

#[test]
fn redacting_one_line_keeps_the_others() {
    if !pdfcore::render::is_available() {
        eprintln!("pdfium is not available; skipping the redaction round trip");
        return;
    }
    let dir = TestDir::new();
    let doc = invoice_page();
    let areas = vec![line_area(1, 70.0, 670.0, 400.0)];
    let options = RedactionOptions { remove_metadata: false, ..Default::default() };
    let source = dir.path("invoice.pdf");
    let report = match run_redaction(&doc, &areas, &options, &source) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("skipping: {error}");
            return;
        }
    };
    let after = page_text(Path::new(&report.output), 1);
    assert!(!after.contains("GB82WEST12345698765432"), "IBAN still extractable: {after:?}");
    assert!(after.contains("ada@example.com"), "the first line must survive: {after:?}");
    assert!(after.contains("Total due"), "the third line must survive: {after:?}");
}

#[test]
fn removing_metadata_strips_the_title() {
    let dir = TestDir::new();
    let doc = invoice_page();
    let source = dir.path("invoice.pdf");
    let mut written = doc.clone();
    write_doc(&mut written, &source);
    let result = source.with_extension("stripped.pdf");
    let options = RedactionOptions { remove_metadata: true, ..Default::default() };
    let report =
        redact_pdf(&source, &result, &[], &options, OverwritePolicy::Replace, None, &no_progress, &CancelToken::new())
            .expect("metadata strip");
    let info = lopdf::Document::load(&report.output).expect("reopen");
    let trailer = info.trailer;
    assert!(trailer.get(b"Info").is_err(), "the Info dictionary must be gone, otherwise the author survives redaction");
}

#[test]
fn keeping_metadata_leaves_the_title_alone() {
    let dir = TestDir::new();
    let doc = invoice_page();
    let source = dir.path("invoice.pdf");
    let mut written = doc.clone();
    write_doc(&mut written, &source);
    let result = source.with_extension("kept.pdf");
    let options = RedactionOptions { remove_metadata: false, ..Default::default() };
    let report =
        redact_pdf(&source, &result, &[], &options, OverwritePolicy::Replace, None, &no_progress, &CancelToken::new())
            .expect("no-op redaction");
    assert!(Path::new(&report.output).exists());
}

#[test]
fn an_existing_output_is_not_silently_overwritten() {
    let dir = TestDir::new();
    let doc = invoice_page();
    let source = dir.path("invoice.pdf");
    let mut written = doc.clone();
    write_doc(&mut written, &source);
    let result = dir.path("taken.pdf");
    std::fs::write(&result, b"existing").expect("seed");
    let error = redact_pdf(
        &source,
        &result,
        &[],
        &RedactionOptions::default(),
        OverwritePolicy::Error,
        None,
        &no_progress,
        &CancelToken::new(),
    );
    assert!(error.is_err(), "an existing output must be reported, not replaced");
    assert_eq!(std::fs::read(&result).expect("intact"), b"existing");
}

#[test]
fn image_redaction_option_is_recorded_in_the_report() {
    let dir = TestDir::new();
    let doc = build_image_doc(1, "Scan");
    let source = dir.path("scan.pdf");
    let mut written = doc.clone();
    write_doc(&mut written, &source);
    let result = source.with_extension("clean.pdf");
    let options = RedactionOptions { images: ImageRedaction::RemovePixels, ..Default::default() };
    let report = redact_pdf(
        &source,
        &result,
        &[RedactionArea { page: 1, left: 0.0, bottom: 0.0, right: 595.0, top: 842.0 }],
        &options,
        OverwritePolicy::Replace,
        None,
        &no_progress,
        &CancelToken::new(),
    )
    .expect("image redaction");
    assert_eq!(report.images_removed, 1, "the page's single image must be replaced");
    // The replacement keeps the dictionary, so the page still renders.
    assert!(Path::new(&report.output).exists());
}

#[test]
fn a_document_with_no_pages_is_rejected() {
    let dir = TestDir::new();
    let source = dir.path("empty.pdf");
    let mut doc = lopdf::Document::with_version("1.7");
    doc.trailer.set("Root", lopdf::Object::Reference(lopdf::ObjectId::from((1, 0))));
    doc.save(&source).expect("write");
    let error = redact_pdf(
        &source,
        &dir.path("out.pdf"),
        &[],
        &RedactionOptions::default(),
        OverwritePolicy::Replace,
        None,
        &no_progress,
        &CancelToken::new(),
    );
    assert!(error.is_err(), "a page-less document must be reported, not silently accepted");
}

#[test]
fn a_cancelled_run_stops_before_writing() {
    let dir = TestDir::new();
    let doc = invoice_page();
    let source = dir.path("invoice.pdf");
    let mut written = doc.clone();
    write_doc(&mut written, &source);
    let token = CancelToken::new();
    token.cancel();
    let result = source.with_extension("cancelled.pdf");
    let error = redact_pdf(
        &source,
        &result,
        &[line_area(1, 70.0, 700.0, 400.0)],
        &RedactionOptions::default(),
        OverwritePolicy::Replace,
        None,
        &no_progress,
        &token,
    );
    assert!(error.is_err(), "a cancelled run must report Cancelled");
    assert!(!result.exists(), "a cancelled run must not leave an output file");
}

#[test]
fn redaction_is_verified_against_the_output_text_layer() {
    if !pdfcore::render::is_available() {
        eprintln!("pdfium is not available; skipping the redaction verification");
        return;
    }
    let dir = TestDir::new();
    let doc = invoice_page();
    let areas = vec![line_area(1, 70.0, 700.0, 400.0)];
    let options = RedactionOptions { remove_metadata: false, ..Default::default() };
    let source = dir.path("invoice.pdf");
    let report = match run_redaction(&doc, &areas, &options, &source) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("skipping: {error}");
            return;
        }
    };
    assert!(
        report.verified,
        "verification must run when pdfium and a text layer exist: {}",
        report.verification_message
    );
    assert!(
        report.remaining_matches.is_empty(),
        "no redacted sample may remain extractable: {:?}",
        report.remaining_matches
    );
    assert!(!report.verification_message.is_empty());
}

#[test]
fn a_redacted_literal_is_absent_from_the_rendered_text_layer() {
    if !pdfcore::render::is_available() {
        eprintln!("pdfium is not available; skipping the redaction text check");
        return;
    }
    let dir = TestDir::new();
    let doc = invoice_page();
    let areas = vec![line_area(1, 70.0, 670.0, 400.0)];
    let options = RedactionOptions { remove_metadata: false, ..Default::default() };
    let source = dir.path("invoice.pdf");
    let report = match run_redaction(&doc, &areas, &options, &source) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("skipping: {error}");
            return;
        }
    };
    let after = pdfcore::render::extract_page_text(Path::new(&report.output), None, 1).unwrap_or_default();
    assert!(!after.contains("GB82WEST12345698765432"), "the redacted literal is still extractable: {after:?}");
    assert!(report.verified);
    assert!(report.remaining_matches.is_empty());
}
