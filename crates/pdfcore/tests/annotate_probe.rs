//! Where a new annotation lands, and whether the page's own list survives it.
//!
//! `annotate` writes into the user's file, so a misplaced mark stays there. The
//! module's contract: an `Annotation` carries "display-space rectangle in points
//! with the origin at the TOP-LEFT of the displayed page (matches how the UI
//! canvas reports coordinates)". The page's own coordinate system starts at its
//! `/MediaBox` origin (ISO 32000-1 7.7.3.3), which is not always `(0, 0)` - a
//! cropped or imposed page starts wherever the producer put it.
//!
//! Every case below is differential: the same display rectangle on two pages that
//! differ only in one property, so the expected difference follows from the spec
//! rather than from the code.

use std::path::{Path, PathBuf};

use lopdf::{dictionary, Dictionary, Document, Object, Stream};
use pdfcore::annotate::{annotate_editable_pdf, Annotation};
use pdfcore::docutil::OverwritePolicy;
use pdfcore::progress::CancelToken;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("omnioffice-annotate-probe");
    std::fs::create_dir_all(&dir).expect("scratch directory");
    dir.join(format!("{name}.pdf"))
}

fn rect(values: [f64; 4]) -> Object {
    Object::Array(values.iter().map(|value| Object::Real(*value as f32)).collect())
}

/// A one-page document with the given `/MediaBox` (on the page, or on the page
/// tree), optional `/Rotate`, and an optional existing link annotation.
struct Page {
    media: [f64; 4],
    inherited: bool,
    rotate: i64,
    with_link: bool,
    /// Store `/Annots` in an indirect array, as a rewriting optimizer does.
    indirect_annots: bool,
}

impl Default for Page {
    fn default() -> Self {
        Self { media: [0.0, 0.0, 595.0, 842.0], inherited: false, rotate: 0, with_link: false, indirect_annots: false }
    }
}

fn build(name: &str, page: Page) -> PathBuf {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let content = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), b"".to_vec())));
    let mut annots: Vec<Object> = Vec::new();
    if page.with_link {
        let link = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Link",
            "Rect" => rect([10.0, 10.0, 100.0, 40.0]),
            "Border" => Object::Array(vec![Object::Integer(0), Object::Integer(0), Object::Integer(0)]),
        }));
        annots.push(Object::Reference(link));
    }
    let annots_value = if page.indirect_annots {
        Object::Reference(doc.add_object(Object::Array(annots)))
    } else {
        Object::Array(annots)
    };
    let mut page_dict = dictionary! {
        "Type" => "Page",
        "Parent" => Object::Reference(pages_id),
        "Contents" => Object::Reference(content),
    };
    if !page.inherited {
        page_dict.set("MediaBox", rect(page.media));
    }
    if page.rotate != 0 {
        page_dict.set("Rotate", Object::Integer(page.rotate));
    }
    if page.with_link || page.indirect_annots {
        page_dict.set("Annots", annots_value);
    }
    let page_id = doc.add_object(Object::Dictionary(page_dict));
    let mut tree = dictionary! {
        "Type" => "Pages",
        "Kids" => Object::Array(vec![Object::Reference(page_id)]),
        "Count" => Object::Integer(1),
    };
    if page.inherited {
        tree.set("MediaBox", rect(page.media));
    }
    doc.objects.insert(pages_id, Object::Dictionary(tree));
    let catalog = doc.new_object_id();
    doc.objects.insert(
        catalog,
        Object::Dictionary(dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) }),
    );
    doc.trailer.set("Root", Object::Reference(catalog));
    let path = scratch(name);
    doc.save(&path).expect("save fixture");
    path
}

/// A mark whose display rectangle is the same on every fixture: the top-left
/// corner area of the displayed page.
fn mark(x: f64, y: f64) -> Annotation {
    Annotation {
        kind: "rect".into(),
        page: 1,
        x,
        y,
        w: 60.0,
        h: 40.0,
        text: String::new(),
        font_size_pt: 12.0,
        bold: false,
        color: "#FF0000".into(),
        opacity: 1.0,
        image_path: None,
        line_width_pt: 1.0,
        x2: None,
        y2: None,
        strokes: Vec::new(),
        image_base64: None,
    }
}

/// Runs the annotation and returns the output bytes.
fn run(input: &Path, output: &Path, annotation: Annotation) -> Vec<u8> {
    // The editable path: the annotation is a real `/Annots` dictionary, which is
    // what the annotation screen writes.
    annotate_editable_pdf(input, output, &[annotation], OverwritePolicy::Replace, None, &|_| {}, &CancelToken::new())
        .expect("annotate");
    std::fs::read(output).expect("read the output")
}

/// The `/Rect` of the last annotation on page one: the one this test added.
fn last_rect(bytes: &[u8]) -> [f64; 4] {
    let doc = Document::load_mem(bytes).expect("reload the output");
    let page_id = *doc.get_pages().get(&1).expect("page one");
    let page = doc.get_dictionary(page_id).expect("page");
    let annots = page.get(b"Annots").expect("annots").as_array().expect("array");
    let annot = annots.last().expect("an annotation").as_reference().expect("reference");
    let dict = doc.get_dictionary(annot).expect("annot");
    let values: Vec<f64> = dict
        .get(b"Rect")
        .expect("Rect")
        .as_array()
        .expect("array")
        .iter()
        .map(|value| value.as_float().unwrap_or(0.0) as f64)
        .collect();
    [values[0], values[1], values[2], values[3]]
}

fn annot_count(bytes: &[u8]) -> usize {
    let doc = Document::load_mem(bytes).expect("reload");
    let page_id = *doc.get_pages().get(&1).expect("page");
    match doc.get_dictionary(page_id).and_then(|page| page.get(b"Annots")) {
        Ok(Object::Array(items)) => items.len(),
        _ => 0,
    }
}

#[test]
fn a_mark_lands_where_the_page_starts_at_the_origin() {
    // The baseline: the same display rectangle on a page whose box starts at (0,0).
    let input = build("origin-zero", Page::default());
    let output = scratch("origin-zero-out");
    let rect = last_rect(&run(&input, &output, mark(100.0, 120.0)));
    // Display (100, 120) from the top-left of a 595x842 page is page (100, 842-120-40).
    assert!((rect[0] - 100.0).abs() < 1.0, "x follows the display rectangle: {rect:?}");
    assert!((rect[1] - 682.0).abs() < 1.0, "y is measured from the bottom: {rect:?}");
}

#[test]
fn a_mark_follows_the_page_box_origin() {
    // The same page shifted by (100, 200): the displayed page looks identical, so
    // the mark has to land in the same place on screen - which means the page
    // coordinates move by the box origin (ISO 32000-1 7.7.3.3).
    let input = build("origin-shifted", Page { media: [100.0, 200.0, 695.0, 1042.0], ..Page::default() });
    let output = scratch("origin-shifted-out");
    let rect = last_rect(&run(&input, &output, mark(100.0, 120.0)));
    assert!(
        (rect[0] - 200.0).abs() < 1.0 && (rect[1] - 882.0).abs() < 1.0,
        "the mark moves with the box origin: {rect:?}"
    );
}

#[test]
fn an_inherited_box_lands_like_a_direct_one() {
    let direct = build("direct", Page::default());
    let inherited = build("inherited", Page { inherited: true, ..Page::default() });
    let direct_rect = last_rect(&run(&direct, &scratch("direct-out"), mark(100.0, 120.0)));
    let inherited_rect = last_rect(&run(&inherited, &scratch("inherited-out"), mark(100.0, 120.0)));
    assert_eq!(direct_rect, inherited_rect, "an inherited /MediaBox is not a different page");
}

#[test]
fn an_existing_link_survives_and_an_indirect_list_stays_a_list() {
    let input = build("with-link", Page { with_link: true, indirect_annots: true, ..Page::default() });
    let output = scratch("with-link-out");
    let bytes = run(&input, &output, mark(100.0, 120.0));
    assert_eq!(annot_count(&bytes), 2, "the link is kept and the mark is added");
}

#[test]
fn a_reversed_box_lands_like_a_normal_one() {
    let direct = build("normal-box", Page::default());
    let reversed = build("reversed-box", Page { media: [595.0, 842.0, 0.0, 0.0], ..Page::default() });
    let normal_rect = last_rect(&run(&direct, &scratch("normal-box-out"), mark(100.0, 120.0)));
    let reversed_rect = last_rect(&run(&reversed, &scratch("reversed-box-out"), mark(100.0, 120.0)));
    assert_eq!(normal_rect, reversed_rect, "a reversed /MediaBox describes the same page (7.9.5)");
}
