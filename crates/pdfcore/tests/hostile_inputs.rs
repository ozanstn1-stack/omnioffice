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

// ---------------------------------------------------------------------------
// content: a ToUnicode map that inflates far beyond its size
// ---------------------------------------------------------------------------

/// A Type0 Identity-H font whose `/ToUnicode` stream is a valid CMap followed
/// by `padding` bytes of blanks, Flate-compressed, shown `strings` times.
fn save_type0_page(path: &Path, padding: usize, strings: usize) {
    let mut cmap = b"/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
        /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
        /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
        1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
        1 beginbfrange\n<0041> <0041> <0058>\nendbfrange\n\
        endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n"
        .to_vec();
    cmap.resize(cmap.len() + padding, b' ');
    let mut doc = Document::with_version("1.7");
    let mut stream = Stream::new(dictionary! {}, cmap);
    stream.compress().expect("compress");
    assert!(stream.content.len() < 100_000, "the bomb must stay small on disk");
    let cmap_id = doc.add_object(Object::Stream(stream));
    let descendant = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "CIDFontType2", "BaseFont" => "Test",
        "CIDSystemInfo" => dictionary! {
            "Registry" => Object::string_literal("Adobe"),
            "Ordering" => Object::string_literal("Identity"),
            "Supplement" => 0,
        },
    });
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type0", "BaseFont" => "Test", "Encoding" => "Identity-H",
        "DescendantFonts" => vec![Object::Reference(descendant)], "ToUnicode" => cmap_id,
    });
    let mut content = String::from("BT /F1 12 Tf 72 780 Td 12 TL\n");
    for _ in 0..strings {
        content.push_str("[<0041> -300 <0041>] TJ T*\n");
    }
    content.push_str("ET\n");
    let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content.into_bytes())));
    let pages_id = doc.new_object_id();
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        "Contents" => content_id,
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![Object::Reference(page_id)], "Count" => 1 }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", Object::Reference(catalog));
    doc.save(path).expect("save pdf");
}

#[test]
fn an_inflating_to_unicode_map_is_bounded_and_decoded_once() {
    let dir = TestDir::new();
    let path = dir.path("cmap-bomb.pdf");
    save_type0_page(&path, 20 * 1024 * 1024, 40);

    let started = Instant::now();
    let runs = pdfcore::content::list_text_runs_in_file(&path, None).expect("list runs");
    let listed = started.elapsed();
    assert_eq!(runs.len(), 40);
    // The oversized map is skipped, so the codes are read as raw bytes.
    assert_eq!(runs[0].text, "\0A \0A");

    let started = Instant::now();
    let pages = pdfcore::pdf2doc::content_stream_pages(&path, None).expect("pages");
    let converted = started.elapsed();
    assert_eq!(pages.len(), 1);
    assert!(listed < Duration::from_secs(2), "listing took {listed:?}");
    assert!(converted < Duration::from_secs(2), "conversion took {converted:?}");
}

#[test]
fn a_reasonable_to_unicode_map_still_decodes() {
    let dir = TestDir::new();
    let path = dir.path("cmap-ok.pdf");
    save_type0_page(&path, 1024 * 1024, 40);
    let runs = pdfcore::content::list_text_runs_in_file(&path, None).expect("list runs");
    assert_eq!(runs.len(), 40);
    assert_eq!(runs[0].text, "X X");
}

// ---------------------------------------------------------------------------
// rebuild: inherited attributes are shared, not copied onto every page
// ---------------------------------------------------------------------------

#[test]
fn rebuilding_a_page_tree_does_not_multiply_inherited_resources() {
    use pdfcore::progress::CancelToken;
    use pdfcore::rebuild::rebuild_pdf;

    const PAGES: usize = 300;
    const FONTS: usize = 20_000;
    let mut doc = Document::with_version("1.7");
    let font = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" });
    let mut fonts = lopdf::Dictionary::new();
    for index in 0..FONTS {
        fonts.set(format!("F{index}"), Object::Reference(font));
    }
    let pages_id = doc.new_object_id();
    let mut kids = Vec::new();
    for _ in 0..PAGES {
        let content = doc.add_object(Object::Stream(Stream::new(dictionary! {}, b"BT /F1 12 Tf (x) Tj ET".to_vec())));
        let page = doc.add_object(dictionary! { "Type" => "Page", "Parent" => pages_id, "Contents" => content });
        kids.push(Object::Reference(page));
    }
    // No /Count: the tree counts as damaged and is flattened.
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
            "Resources" => dictionary! { "Font" => fonts },
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut input = Vec::new();
    doc.save_to(&mut input).expect("save");

    let started = Instant::now();
    let rebuilt = rebuild_pdf(&input, &CancelToken::default()).expect("rebuild");
    assert!(started.elapsed() < Duration::from_secs(10), "took {:?}", started.elapsed());
    assert_eq!(rebuilt.pages as usize, PAGES);
    assert!(rebuilt.bytes.len() <= input.len() * 2, "{} bytes in, {} bytes out", input.len(), rebuilt.bytes.len());

    let output = Document::load_mem(&rebuilt.bytes).expect("reload");
    let pages = output.get_pages();
    assert_eq!(pages.len(), PAGES);
    for page_id in pages.values().step_by(100) {
        let fonts = output.get_page_fonts(*page_id).expect("fonts");
        assert_eq!(fonts.len(), FONTS);
        assert!(fonts.contains_key(b"F1".as_slice()));
    }
}
