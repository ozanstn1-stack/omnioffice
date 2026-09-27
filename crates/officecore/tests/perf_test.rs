//! Performance guards for the office engines.
//!
//! Policy (mirrors the PDF perf suite):
//! * The always-on cases fail when an engine regresses by an order of
//!   magnitude, not when a CI machine is a little slower. Every bound below is
//!   printed together with the measured time by running the suite with
//!   `--nocapture`, and the bound is a generous multiple of the measurement
//!   on the development machine.
//! * The `#[ignore]` cases are the heavy contract: run them explicitly with
//!   `cargo test -p officecore perf -- --ignored --nocapture`. They are not
//!   part of the default loop because a 500-page layout pass costs minutes on
//!   a loaded CI box.
//!
//! These tests use only deterministic, generated documents; no network, no
//! sample files, no timing fiddling (no sleeps, no retries).

use officecore::layout;
use officecore::model::*;
use std::time::{Duration, Instant};

/// Builds a Writer document whose content deterministically paginates to
/// roughly `pages` pages: a heading plus ten body paragraphs per page, each
/// page closed by an explicit page break so the count does not depend on font
/// metrics in the layout engine.
fn writer_document(pages: usize) -> TextDocument {
    let mut document = TextDocument::new_blank("Perf writer");
    document.page = PageSetup::from_preset("a4", "portrait");
    let mut blocks = Vec::with_capacity(pages * 12);
    for page in 0..pages {
        blocks.push(Block::heading(&format!("Section {page}"), 1));
        for line in 0..10 {
            blocks.push(Block::paragraph(&format!(
                "Page {page} line {line}: the quick brown fox jumps over the lazy dog while the layout engine measures this line."
            )));
        }
        blocks.push(Block::PageBreak);
    }
    document.blocks = blocks;
    document
}

fn report(label: &str, elapsed: Duration, bound: Duration) {
    eprintln!(
        "perf: {label} took {:.3}s (bound {:.1}s)",
        elapsed.as_secs_f64(),
        bound.as_secs_f64()
    );
}

/// The Writer PDF export is the most expensive pure-Rust path in the engine
/// (layout + pdfcanvas, two or three passes to stabilize page numbers). A
/// regression here is user visible as "Export to PDF hangs".
#[test]
fn perf_writer_pdf_export_of_100_pages_under_bound() {
    let document = writer_document(100);
    let bound = Duration::from_secs(60);
    let started = Instant::now();
    let bytes = layout::document_to_pdf(&document);
    let elapsed = started.elapsed();
    report("writer layout+pdfcanvas, 100 pages", elapsed, bound);
    assert!(bytes.starts_with(b"%PDF-"), "the export must be a PDF");
    assert!(bytes.len() > 50_000, "suspiciously small export: {} bytes", bytes.len());
    assert!(elapsed < bound, "100-page Writer PDF export took {elapsed:?}, bound {bound:?}");
}

/// Heavy contract: run with `cargo test -p officecore perf -- --ignored`.
/// 500 pages is the size the product promises to export without falling over.
#[test]
#[ignore = "heavy: run with `cargo test -p officecore perf -- --ignored --nocapture`"]
fn perf_writer_pdf_export_of_500_pages_under_bound() {
    let document = writer_document(500);
    let bound = Duration::from_secs(300);
    let started = Instant::now();
    let bytes = layout::document_to_pdf(&document);
    let elapsed = started.elapsed();
    report("writer layout+pdfcanvas, 500 pages", elapsed, bound);
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(elapsed < bound, "500-page Writer PDF export took {elapsed:?}, bound {bound:?}");
}

/// A Writer document with real layout complexity (table + image + notes) must
/// not degrade into a quadratic blow-up either; this is a smaller but
/// structurally richer document than the plain 100-page one.
#[test]
fn perf_writer_pdf_export_of_50_pages_with_notes_and_tables_under_bound() {
    let mut document = writer_document(50);
    document.footnotes = vec![Footnote {
        id: "fn-perf".into(),
        runs: vec![Run { text: "Perf note".into(), ..Default::default() }],
        marker: String::new(),
    }];
    // One table per ten pages, with a 20x4 body.
    let mut enriched = Vec::new();
    for (index, block) in document.blocks.into_iter().enumerate() {
        if index % 120 == 0 {
            let mut table = TableData::simple(21, 4, 460.0);
            for row in 0..21u32 {
                for column in 0..4u32 {
                    table.rows[row as usize].cells[column as usize].blocks =
                        vec![Block::paragraph(&format!("r{row}c{column}"))];
                }
            }
            enriched.push(Block::Table { table });
        }
        enriched.push(block);
    }
    document.blocks = enriched;
    // Reference the note from the first paragraph so the note area is reserved.
    if let Some(Block::Paragraph { runs, .. }) = document.blocks.get_mut(1) {
        runs.push(Run { footnote: Some("fn-perf".into()), ..Default::default() });
    }

    let bound = Duration::from_secs(90);
    let started = Instant::now();
    let bytes = layout::document_to_pdf(&document);
    let elapsed = started.elapsed();
    report("writer layout+pdfcanvas, 50 pages with tables/notes", elapsed, bound);
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(elapsed < bound, "enriched Writer export took {elapsed:?}, bound {bound:?}");
}
