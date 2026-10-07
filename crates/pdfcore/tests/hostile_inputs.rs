//! Hostile-input regressions: small crafted PDFs that used to abort the
//! process, exhaust memory or take seconds. Each must now finish quickly with
//! the normal content intact.

use lopdf::{dictionary, Document, Object, Stream};
use std::path::Path;
use std::time::{Duration, Instant};

mod common;
use common::TestDir;

/// Saves a one-page PDF with Helvetica as `/F1` and the given content stream.
fn save_text_page(path: &Path, content: &str) {
    let mut doc = Document::new();
    doc.version = "1.7".to_string();
    let font = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
    }));
    let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content.as_bytes().to_vec())));
    let pages_id = doc.new_object_id();
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        "Contents" => content_id,
    }));
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![Object::Reference(page_id)], "Count" => 1,
        }),
    );
    let catalog = doc.add_object(Object::Dictionary(dictionary! { "Type" => "Catalog", "Pages" => pages_id }));
    doc.trailer.set("Root", Object::Reference(catalog));
    doc.save(path).expect("save pdf");
}

const NORMAL_LINES: [&str; 8] = [
    "The first line of ordinary body text sits near the top of the page",
    "The second line of ordinary body text follows right below the first",
    "The third line of ordinary body text keeps the same left margin here",
    "The fourth line of ordinary body text is still part of one paragraph",
    "The fifth line of ordinary body text continues the same paragraph on",
    "The sixth line of ordinary body text is almost the end of the block",
    "The seventh line of ordinary body text closes in on the last line of",
    "The eighth line of ordinary body text ends the paragraph at last now",
];

// ---------------------------------------------------------------------------
// pdf2doc: fragments far outside the page
// ---------------------------------------------------------------------------

#[test]
fn text_far_outside_the_page_is_ignored_by_layout_recovery() {
    let mut content = String::new();
    for (index, line) in NORMAL_LINES.iter().enumerate() {
        content.push_str(&format!("BT /F1 11 Tf 72 {} Td ({line}) Tj ET\n", 760 - index * 14));
    }
    // One fragment at 1e10 used to allocate ~80 GB, one at 3e38 overflowed.
    // (PDF has no exponent notation, so the numbers are written out.)
    for (offset, (x, text)) in [(1e10, "one"), (3e38, "two"), (2e8, "three"), (-1e10, "four")].iter().enumerate() {
        content.push_str(&format!("BT /F1 11 Tf {x:.1} {} Td (far away {text}) Tj ET\n", 400 - offset * 20));
    }
    let dir = TestDir::new();
    let path = dir.path("far.pdf");
    save_text_page(&path, &content);

    let started = Instant::now();
    let recovered = pdfcore::pdf2doc::recover_file(&path, None).expect("recover");
    assert!(started.elapsed() < Duration::from_secs(5), "took {:?}", started.elapsed());
    let text: String = recovered.pages.iter().flat_map(|page| page.blocks.iter()).map(|block| block.text()).collect();
    assert!(text.contains("The first line of ordinary body text"), "{text}");
    assert!(text.contains("ends the paragraph at last now"), "{text}");
    assert!(!text.contains("far away"), "{text}");
}

#[test]
fn a_gutter_search_over_a_huge_text_range_gives_up() {
    use pdfcore::pdf2doc::{recover_pages, PageText, TextFragment};
    // Even a page box that is itself absurd must not make the layout allocate.
    let fragment = |text: &str, x: f64, y: f64| TextFragment {
        text: text.to_string(),
        x,
        width: 300.0,
        y,
        size: 11.0,
        bold: false,
        italic: false,
    };
    let mut fragments: Vec<TextFragment> =
        (0..8).map(|i| fragment("ordinary text here", 72.0, 700.0 - 14.0 * i as f64)).collect();
    fragments.push(fragment("wide", 1e10, 300.0));
    fragments.push(fragment("wider", 3e38, 280.0));
    let page = PageText { page: 1, width: 3e38, height: 842.0, fragments };
    let started = Instant::now();
    let pages = recover_pages(&[page]);
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(pages.len(), 1);
}
