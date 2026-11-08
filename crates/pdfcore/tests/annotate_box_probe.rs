//! Where a mark lands when the page's visible box is not its `/MediaBox`.
//!
//! `annotate` takes "display space": points with the origin at the TOP-LEFT of
//! the page as the screen draws it. The screen draws what the renderer reports,
//! and a PDF renderer shows the **crop box** - the region the page is clipped to
//! for display (ISO 32000-1 14.11.2), which is the intersection of `/CropBox`
//! and `/MediaBox`. Neither box has to start at `(0, 0)`: a cropped scan or an
//! imposed sheet starts wherever the producer put it.
//!
//! Two places translate display space into the page: the editable path (a real
//! `/Annots` dictionary) and the bake path (`annotate_pdf`, which draws the mark
//! into the content stream). Every case below pins the same display rectangle
//! and compares the page coordinates it has to end up at, so the expectation
//! follows from the spec and not from the code.

use std::path::{Path, PathBuf};

use lopdf::{dictionary, Dictionary, Document, Object, Stream};
use pdfcore::annotate::{annotate_editable_pdf, annotate_pdf, Annotation};
use pdfcore::docutil::OverwritePolicy;
use pdfcore::progress::CancelToken;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("omnioffice-annotate-box-probe");
    std::fs::create_dir_all(&dir).expect("scratch directory");
    dir.join(format!("{name}.pdf"))
}

fn rect(values: [f64; 4]) -> Object {
    Object::Array(values.iter().map(|value| Object::Real(*value as f32)).collect())
}

/// A one-page document with a `/MediaBox`, an optional `/CropBox` (on the page
/// or inherited from the page tree, as a rewriting optimizer may leave it) and
/// an optional `/Rotate`.
struct Page {
    media: [f64; 4],
    crop: Option<[f64; 4]>,
    crop_inherited: bool,
    rotate: i64,
}

impl Default for Page {
    fn default() -> Self {
        Self { media: [0.0, 0.0, 600.0, 800.0], crop: None, crop_inherited: false, rotate: 0 }
    }
}

fn build(name: &str, page: Page) -> PathBuf {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let content = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), b"".to_vec())));
    let mut page_dict = dictionary! {
        "Type" => "Page",
        "Parent" => Object::Reference(pages_id),
        "Contents" => Object::Reference(content),
        "MediaBox" => rect(page.media),
    };
    if let Some(crop) = page.crop {
        if !page.crop_inherited {
            page_dict.set("CropBox", rect(crop));
        }
    }
    if page.rotate != 0 {
        page_dict.set("Rotate", Object::Integer(page.rotate));
    }
    let page_id = doc.add_object(Object::Dictionary(page_dict));
    let mut tree = dictionary! {
        "Type" => "Pages",
        "Kids" => Object::Array(vec![Object::Reference(page_id)]),
        "Count" => Object::Integer(1),
    };
    if let (Some(crop), true) = (page.crop, page.crop_inherited) {
        tree.set("CropBox", rect(crop));
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

/// The same display rectangle on every fixture: 100 points in from the left and
/// 50 points down from the top, 60 by 40 points.
const MARK_X: f64 = 100.0;
const MARK_Y: f64 = 50.0;
const MARK_W: f64 = 60.0;
const MARK_H: f64 = 40.0;

fn mark() -> Annotation {
    Annotation {
        kind: "rect".into(),
        page: 1,
        x: MARK_X,
        y: MARK_Y,
        w: MARK_W,
        h: MARK_H,
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

fn run_editable(input: &Path, output: &Path) -> Vec<u8> {
    annotate_editable_pdf(input, output, &[mark()], OverwritePolicy::Replace, None, &|_| {}, &CancelToken::new())
        .expect("annotate");
    std::fs::read(output).expect("read the output")
}

fn run_baked(input: &Path, output: &Path) -> Vec<u8> {
    annotate_pdf(input, output, &[mark()], OverwritePolicy::Replace, None, &|_| {}, &CancelToken::new()).expect("bake");
    std::fs::read(output).expect("read the output")
}

/// The leading numeric operands of a content-stream line and the operator that
/// follows them, so `1 0 0 1 0 0 cm` reads as six numbers and `cm`.
fn operands(line: &str) -> Option<(Vec<f64>, &str)> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let mut values = Vec::new();
    let mut operator = "";
    for token in tokens {
        match token.parse::<f64>() {
            Ok(value) => values.push(value),
            Err(_) => {
                operator = token;
                break;
            }
        }
    }
    if values.is_empty() {
        None
    } else {
        Some((values, operator))
    }
}

/// The `/Rect` of the last annotation on page one.
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

/// The baked rectangle, worked out in page space: the content stream draws in
/// the space its `cm` puts it in, so the numbers alone are not the answer.
fn baked_rect(bytes: &[u8]) -> [f64; 4] {
    let doc = Document::load_mem(bytes).expect("reload the output");
    let page_id = *doc.get_pages().get(&1).expect("page one");
    let content = String::from_utf8_lossy(&doc.get_page_content(page_id)).into_owned();
    let mut matrix: Option<Vec<f64>> = None;
    let mut box_values: Option<Vec<f64>> = None;
    for line in content.lines() {
        if let Some((values, operator)) = operands(line) {
            match (values.len(), operator) {
                (6, "cm") => matrix = Some(values),
                (4, "re") => box_values = Some(values),
                _ => {}
            }
        }
    }
    let m = matrix.expect("a cm matrix in the baked content");
    let b = box_values.expect("a re rectangle in the baked content");
    let apply = |x: f64, y: f64| (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]);
    let (x0, y0) = apply(b[0], b[1]);
    let (x1, y1) = apply(b[0] + b[2], b[1] + b[3]);
    [x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)]
}

#[track_caller]
fn assert_rect(actual: [f64; 4], expected: [f64; 4], what: &str) {
    let close = actual.iter().zip(expected.iter()).all(|(a, e)| (a - e).abs() < 0.01);
    assert!(close, "{what}: expected {expected:?}, got {actual:?}");
}

/// The page box starts at `(0, 0)` here, so this is the reference rectangle
/// every other case is stated against.
const PLAIN: [f64; 4] = [100.0, 710.0, 160.0, 750.0];

#[test]
fn a_plain_page_puts_the_mark_where_it_was_drawn() {
    // Display (100, 50) on a 600x800 page is page (100, 800 - 50 - 40).
    let input = build("plain", Page::default());
    assert_rect(last_rect(&run_editable(&input, &scratch("plain-edit-out"))), PLAIN, "editable");
    assert_rect(baked_rect(&run_baked(&input, &scratch("plain-bake-out"))), PLAIN, "baked");
}

#[test]
fn a_baked_mark_follows_the_box_origin() {
    // The editable path already does this. The bake path draws into the content
    // stream, and the page's own coordinates start at its box origin (7.7.3.3),
    // so the same displayed mark has to move with the box.
    let shifted = build("bake-shifted", Page { media: [100.0, 200.0, 700.0, 1000.0], ..Page::default() });
    let expected = [PLAIN[0] + 100.0, PLAIN[1] + 200.0, PLAIN[2] + 100.0, PLAIN[3] + 200.0];
    assert_rect(baked_rect(&run_baked(&shifted, &scratch("bake-shifted-out"))), expected, "baked, shifted");
}

#[test]
fn a_baked_mark_survives_a_reversed_box() {
    // The corners may be written in either order and describe the same page
    // (7.9.5); a negative width would put the mark outside the page.
    let reversed = build("bake-reversed", Page { media: [600.0, 800.0, 0.0, 0.0], ..Page::default() });
    assert_rect(baked_rect(&run_baked(&reversed, &scratch("bake-reversed-out"))), PLAIN, "baked, reversed");
}

#[test]
fn a_mark_lands_where_the_crop_box_shows_the_page() {
    // The screen draws the crop box, so display space starts at the crop box's
    // own corner: the same mark is 50 and 100 points further into the page.
    let cropped = build("crop", Page { crop: Some([50.0, 100.0, 550.0, 700.0]), ..Page::default() });
    let expected = [150.0, 610.0, 210.0, 650.0];
    assert_rect(last_rect(&run_editable(&cropped, &scratch("crop-edit-out"))), expected, "editable, cropped");
}

#[test]
fn a_baked_mark_lands_where_the_crop_box_shows_the_page() {
    let cropped = build("crop-bake", Page { crop: Some([50.0, 100.0, 550.0, 700.0]), ..Page::default() });
    let expected = [150.0, 610.0, 210.0, 650.0];
    assert_rect(baked_rect(&run_baked(&cropped, &scratch("crop-bake-out"))), expected, "baked, cropped");
}

#[test]
fn a_crop_box_that_matches_the_media_box_changes_nothing() {
    let same = build("crop-same", Page { crop: Some([0.0, 0.0, 600.0, 800.0]), ..Page::default() });
    assert_rect(last_rect(&run_editable(&same, &scratch("crop-same-out"))), PLAIN, "crop box == media box");
}

#[test]
fn a_crop_box_is_clipped_to_the_media_box() {
    // The visible region is the intersection, so a crop box hanging over the
    // left and bottom edges starts at the media box's corner (14.11.2.1).
    let overhang = build("crop-overhang", Page { crop: Some([-50.0, -50.0, 300.0, 400.0]), ..Page::default() });
    let expected = [100.0, 310.0, 160.0, 350.0];
    assert_rect(last_rect(&run_editable(&overhang, &scratch("crop-overhang-out"))), expected, "clipped crop");
}

#[test]
fn an_inherited_crop_box_lands_like_a_direct_one() {
    let direct = build("crop-direct", Page { crop: Some([50.0, 100.0, 550.0, 700.0]), ..Page::default() });
    let inherited = build(
        "crop-inherited",
        Page { crop: Some([50.0, 100.0, 550.0, 700.0]), crop_inherited: true, ..Page::default() },
    );
    let expected = [150.0, 610.0, 210.0, 650.0];
    assert_rect(last_rect(&run_editable(&direct, &scratch("crop-direct-out"))), expected, "direct");
    assert_rect(last_rect(&run_editable(&inherited, &scratch("crop-inherited-out"))), expected, "inherited");
}

#[test]
fn a_rotated_crop_page_puts_the_mark_where_it_was_drawn() {
    // /Rotate 90 turns the 500x600 crop box into a 600x500 view. Display (100, 50)
    // is 100 points from the view's left edge, which runs along the page's own
    // y axis, and 50 points down from its top edge, which runs along the page's
    // own x axis; both are then offset by the crop box corner. The mark's sides
    // swap too, because a rectangle turned 90 degrees is 40 wide and 60 tall.
    let rotated =
        build("crop-rotated", Page { crop: Some([50.0, 100.0, 550.0, 700.0]), rotate: 90, ..Page::default() });
    let expected = [100.0, 200.0, 140.0, 260.0];
    assert_rect(last_rect(&run_editable(&rotated, &scratch("crop-rotated-out"))), expected, "rotated");
}
