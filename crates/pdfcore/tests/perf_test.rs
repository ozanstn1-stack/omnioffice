//! Performance guards for the PDF engine.
//!
//! Policy:
//! * Always-on cases assert order-of-magnitude regressions, not CI jitter.
//!   The measured time is printed with `--nocapture`; bounds are generous
//!   multiples of the development-machine numbers.
//! * Rendering cases need the bundled pdfium. When it is not available (a
//!   lean CI checkout without engines) they print `skipping` and pass, exactly
//!   like the existing render tests; the pure-Rust merge/text cases still run.
//! * The `#[ignore]` cases are the heavy contract:
//!   `cargo test -p pdfcore perf -- --ignored --nocapture`.
//!
//! All inputs are generated locally by `common::build_*`; no network, no
//! sample documents, no personal data.

mod common;

use common::*;
use pdfcore::docutil::OverwritePolicy;
use pdfcore::merge::{merge_files, MergeOptions};
use pdfcore::progress::CancelToken;
use pdfcore::render::{is_available, render_page, RenderOptions};
use std::time::{Duration, Instant};

fn report(label: &str, elapsed: Duration, bound: Duration) {
    eprintln!(
        "perf: {label} took {:.3}s (bound {:.1}s)",
        elapsed.as_secs_f64(),
        bound.as_secs_f64()
    );
}

/// Merging is the single most used PDF tool; 200 one-page files is the
/// realistic upper end of a "combine my scans" batch.
#[test]
fn perf_merge_two_hundred_single_page_documents_under_bound() {
    let dir = TestDir::new();
    let mut inputs = Vec::with_capacity(200);
    for index in 0..200 {
        let path = dir.path(&format!("page-{index:03}.pdf"));
        write_doc(&mut build_text_doc(1, &format!("P{index}"), "Perf merge"), &path);
        inputs.push(path);
    }

    let bound = Duration::from_secs(60);
    let started = Instant::now();
    let (merged, pages) = merge_files(
        &inputs,
        &dir.path("merged.pdf"),
        &MergeOptions::default(),
        OverwritePolicy::Replace,
        &no_progress,
        &CancelToken::new(),
    )
    .expect("merge succeeds");
    let elapsed = started.elapsed();
    report("merge 200 one-page PDFs", elapsed, bound);

    assert_eq!(pages, 200);
    assert_eq!(page_count(&merged), 200);
    assert!(page_text(&merged, 1).contains("P0 page 1"));
    assert!(page_text(&merged, 200).contains("P199 page 1"));
    assert!(elapsed < bound, "merging 200 PDFs took {elapsed:?}, bound {bound:?}");
}

/// Text extraction over a 200-page document. This uses lopdf's own text
/// extractor (the `common::page_text` path) so it also runs without pdfium.
#[test]
fn perf_extract_text_from_two_hundred_page_document_under_bound() {
    let dir = TestDir::new();
    let path = dir.path("book.pdf");
    write_doc(&mut build_text_doc(200, "Chapter", "Perf text"), &path);

    let bound = Duration::from_secs(60);
    let started = Instant::now();
    let document = lopdf::Document::load(&path).expect("load");
    let pages: Vec<u32> = document.get_pages().keys().copied().collect();
    let text = document.extract_text(&pages).expect("extract text");
    let elapsed = started.elapsed();
    report("lopdf text extraction, 200 pages", elapsed, bound);

    assert_eq!(pages.len(), 200);
    assert!(text.contains("Chapter page 1"), "the first page text must be found");
    assert!(text.contains("Chapter page 200"), "the last page text must be found");
    assert!(elapsed < bound, "200-page text extraction took {elapsed:?}, bound {bound:?}");
}

/// Rasterizing 20 pages exercises the pdfium binding, the page tree walk and
/// the RGBA buffer copies the export tools depend on.
#[test]
fn perf_render_twenty_pages_under_bound() {
    if !is_available() {
        eprintln!("skipping: pdfium not available");
        return;
    }
    let dir = TestDir::new();
    let path = dir.path("render.pdf");
    write_doc(&mut build_text_doc(20, "Render", "Perf render"), &path);
    let options = RenderOptions { dpi: 96.0, max_width: Some(1200), max_height: Some(1600) };

    let bound = Duration::from_secs(60);
    let started = Instant::now();
    for page in 1..=20u32 {
        let rendered = render_page(&path, None, page, &options).expect("render page");
        assert!(rendered.width > 0 && rendered.height > 0);
        assert!(!rendered.rgba.is_empty());
    }
    let elapsed = started.elapsed();
    report("pdfium render, 20 pages", elapsed, bound);
    assert!(elapsed < bound, "rendering 20 pages took {elapsed:?}, bound {bound:?}");
}

/// Heavy contract: a 500-page scanned-style document is built, rendered and
/// compared against itself with the visual pass on. This is the "big scanner
/// batch" end-to-end path.
/// Run with `cargo test -p pdfcore perf -- --ignored --nocapture`.
#[test]
#[ignore = "heavy: run with `cargo test -p pdfcore perf -- --ignored --nocapture`"]
fn perf_scanned_five_hundred_pages_render_and_compare_heavy() {
    if !is_available() {
        eprintln!("skipping: pdfium not available");
        return;
    }
    let dir = TestDir::new();
    let path = dir.path("scanned.pdf");
    let build_started = Instant::now();
    write_doc(&mut build_scanned_doc(500, "Scanned invoice", "Perf scan"), &path);
    let build_elapsed = build_started.elapsed();
    // Building the synthetic scans dominates the test (raster compose +
    // deflate per page); the bound is ~3x the development-machine measurement
    // so a pathological regression is caught without CI flakiness.
    assert!(
        build_elapsed < Duration::from_secs(1500),
        "building 500 scanned pages took {build_elapsed:?}"
    );

    // Comparing the document with itself must render every page and report no
    // differences; the point is the throughput, not the diff.
    let options = pdfcore::compare::CompareOptions {
        max_pages: 500,
        tolerance: 24,
        dpi: 72,
        visual: true,
        ignore_whitespace: true,
        max_differences: 500,
    };
    let bound = Duration::from_secs(600);
    let started = Instant::now();
    let report_result = pdfcore::compare::compare_pdfs(&path, &path, None, None, &options, &no_progress, &CancelToken::new())
        .expect("compare succeeds");
    let elapsed = started.elapsed();
    eprintln!(
        "perf: scanned build (500 pages) took {:.3}s; compare/render took {:.3}s (bound {:.1}s)",
        build_elapsed.as_secs_f64(),
        elapsed.as_secs_f64(),
        bound.as_secs_f64()
    );

    assert_eq!(report_result.left_pages, 500);
    assert_eq!(report_result.right_pages, 500);
    assert!(report_result.identical, "a document must compare identical with itself");
    assert!(!report_result.visual_truncated, "all 500 pages must be rendered");
    assert!(
        elapsed < bound,
        "500-page scanned render/compare took {elapsed:?}, bound {bound:?}"
    );
}
