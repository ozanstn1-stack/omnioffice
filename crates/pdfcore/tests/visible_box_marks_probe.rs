//! Where a watermark and a page-number stamp land when the page's visible box
//! is not its `/MediaBox`.
//!
//! Both features draw into the page content stream, and the numbers in that
//! stream are only the last step: a `cm` puts them somewhere. This probe walks
//! the stream - `q`/`Q` save and restore, `cm` multiplies into the current
//! transform - and then reads the mark in **page space**, which is what a
//! viewer shows. A renderer draws the crop box clipped to the media box
//! (ISO 32000-1 14.11.2), so the mark has to sit inside that region: a
//! centre watermark at its centre, a corner stamp at its corner.
//!
//! Two kinds of expectation are used, and neither copies the production math:
//! a *differential* one (two pages that differ only in where their box starts
//! must move the mark by exactly that difference) and an *invariant* one (the
//! centre of the visible box is the centre of the visible box, whatever the
//! boxes say).

use std::path::{Path, PathBuf};

use lopdf::{dictionary, Dictionary, Document, Object, Stream};
use pdfcore::docutil::OverwritePolicy;
use pdfcore::progress::CancelToken;
use pdfcore::stamp::{stamp_pdf, HeaderFooterOptions};
use pdfcore::watermark::{add_watermark, WatermarkOptions};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("omnioffice-visible-box-probe");
    std::fs::create_dir_all(&dir).expect("scratch directory");
    dir.join(format!("{name}.pdf"))
}

fn rect(values: [f64; 4]) -> Object {
    Object::Array(values.iter().map(|value| Object::Real(*value as f32)).collect())
}

/// A one-page document with the given box, optionally cropped and rotated.
fn build(name: &str, media: [f64; 4], crop: Option<[f64; 4]>, rotate: i64) -> PathBuf {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let content = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), b"".to_vec())));
    let mut page = dictionary! {
        "Type" => "Page",
        "Parent" => Object::Reference(pages_id),
        "Contents" => Object::Reference(content),
        "MediaBox" => rect(media),
    };
    if let Some(crop) = crop {
        page.set("CropBox", rect(crop));
    }
    if rotate != 0 {
        page.set("Rotate", Object::Integer(rotate));
    }
    let page_id = doc.add_object(Object::Dictionary(page));
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
            "Count" => Object::Integer(1),
        }),
    );
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

// ---------------------------------------------------------------------------
// A small content-stream walker
// ---------------------------------------------------------------------------

/// An affine transform, the way PDF writes one: `x' = a x + c y + e`,
/// `y' = b x + d y + f`.
#[derive(Clone, Copy, Debug)]
struct Affine([f64; 6]);

impl Affine {
    const IDENTITY: Affine = Affine([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        let m = self.0;
        (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
    }

    /// `self` followed by `outer`: the inner matrix is applied first.
    fn then(&self, outer: Affine) -> Affine {
        let a = self.0;
        let b = outer.0;
        Affine([
            b[0] * a[0] + b[2] * a[1],
            b[1] * a[0] + b[3] * a[1],
            b[0] * a[2] + b[2] * a[3],
            b[1] * a[2] + b[3] * a[3],
            b[0] * a[4] + b[2] * a[5] + b[4],
            b[1] * a[4] + b[3] * a[5] + b[5],
        ])
    }

    fn of(values: &[f64]) -> Affine {
        Affine([values[0], values[1], values[2], values[3], values[4], values[5]])
    }
}

/// What a page's content stream says, in page space.
struct Drawn {
    /// Every `Do` of an image, as the rectangle its unit square covers.
    unit_squares: Vec<[f64; 4]>,
    /// Every text anchor (`Td` position) that follows the current transform.
    text_anchors: Vec<(f64, f64)>,
}

fn walk(content: &[u8]) -> Drawn {
    let text = String::from_utf8_lossy(content);
    let mut stack: Vec<Affine> = Vec::new();
    let mut ctm = Affine::IDENTITY;
    let mut out = Drawn { unit_squares: Vec::new(), text_anchors: Vec::new() };
    for line in text.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let Some((operator, operands)) = tokens.split_last() else { continue };
        match *operator {
            "q" => stack.push(ctm),
            "Q" => ctm = stack.pop().unwrap_or(Affine::IDENTITY),
            "cm" => {
                let values: Vec<f64> = operands.iter().filter_map(|token| token.parse().ok()).collect();
                if values.len() == 6 {
                    ctm = Affine::of(&values).then(ctm);
                }
            }
            "Do" => {
                let corners = [ctm.apply(0.0, 0.0), ctm.apply(1.0, 0.0), ctm.apply(0.0, 1.0), ctm.apply(1.0, 1.0)];
                let xs: Vec<f64> = corners.iter().map(|point| point.0).collect();
                let ys: Vec<f64> = corners.iter().map(|point| point.1).collect();
                let min_x = xs.iter().cloned().fold(f64::MAX, f64::min);
                let max_x = xs.iter().cloned().fold(f64::MIN, f64::max);
                let min_y = ys.iter().cloned().fold(f64::MAX, f64::min);
                let max_y = ys.iter().cloned().fold(f64::MIN, f64::max);
                out.unit_squares.push([min_x, min_y, max_x, max_y]);
            }
            "Td" => {
                let values: Vec<f64> = operands.iter().filter_map(|token| token.parse().ok()).collect();
                if values.len() == 2 {
                    out.text_anchors.push(ctm.apply(values[0], values[1]));
                }
            }
            _ => {}
        }
    }
    out
}

fn page_stream(bytes: &[u8]) -> Vec<u8> {
    let doc = Document::load_mem(bytes).expect("reload");
    let page_id = *doc.get_pages().get(&1).expect("page one");
    doc.get_page_content(page_id)
}

// ---------------------------------------------------------------------------
// Watermark
// ---------------------------------------------------------------------------

fn watermark_options(position: &str) -> WatermarkOptions {
    WatermarkOptions {
        kind: "text".into(),
        text: "DRAFT".into(),
        font_size_pt: 48.0,
        bold: false,
        color: "#FF0000".into(),
        opacity: 1.0,
        rotation_deg: 0.0,
        position: position.into(),
        margin_pt: 0.0,
        tile: false,
        image_path: None,
        image_scale: 0.5,
        pages: Vec::new(),
    }
}

fn watermark(input: &Path, name: &str, position: &str) -> Drawn {
    let output = scratch(name);
    add_watermark(
        input,
        &output,
        &watermark_options(position),
        OverwritePolicy::Replace,
        None,
        &|_| {},
        &CancelToken::new(),
    )
    .expect("watermark");
    walk(&page_stream(&std::fs::read(&output).expect("read the output")))
}

fn centre(rect: [f64; 4]) -> (f64, f64) {
    ((rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0)
}

#[track_caller]
fn assert_near(actual: (f64, f64), expected: (f64, f64), tolerance: f64, what: &str) {
    let close = (actual.0 - expected.0).abs() <= tolerance && (actual.1 - expected.1).abs() <= tolerance;
    assert!(close, "{what}: expected {expected:?}, got {actual:?}");
}

#[test]
fn a_centre_watermark_sits_at_the_centre_of_a_plain_page() {
    let input = build("wm-plain", [0.0, 0.0, 600.0, 800.0], None, 0);
    let drawn = watermark(&input, "wm-plain-out", "center");
    assert_eq!(drawn.unit_squares.len(), 1, "one watermark placement");
    assert_near(centre(drawn.unit_squares[0]), (300.0, 400.0), 0.01, "plain page centre");
}

#[test]
fn a_watermark_follows_the_page_box_origin() {
    // The same page, moved: what the viewer shows is identical, so the mark has
    // to move with the box (ISO 32000-1 7.7.3.3).
    let shifted = build("wm-shifted", [100.0, 200.0, 700.0, 1000.0], None, 0);
    let drawn = watermark(&shifted, "wm-shifted-out", "center");
    assert_near(centre(drawn.unit_squares[0]), (400.0, 600.0), 0.01, "shifted page centre");
}

#[test]
fn a_reversed_box_watermark_lands_like_a_normal_one() {
    let reversed = build("wm-reversed", [600.0, 800.0, 0.0, 0.0], None, 0);
    let drawn = watermark(&reversed, "wm-reversed-out", "center");
    assert_near(centre(drawn.unit_squares[0]), (300.0, 400.0), 0.01, "reversed box centre");
}

#[test]
fn a_centre_watermark_sits_at_the_centre_of_the_crop_box() {
    // The viewer shows the crop box, so a centred watermark belongs at its
    // centre - not at the centre of the larger media box (14.11.2).
    let cropped = build("wm-cropped", [0.0, 0.0, 600.0, 800.0], Some([50.0, 100.0, 350.0, 500.0]), 0);
    let drawn = watermark(&cropped, "wm-cropped-out", "center");
    assert_near(centre(drawn.unit_squares[0]), (200.0, 300.0), 0.01, "crop box centre");
}

#[test]
fn a_corner_watermark_lands_on_the_crop_box_corner() {
    let cropped = build("wm-corner", [0.0, 0.0, 600.0, 800.0], Some([50.0, 100.0, 350.0, 500.0]), 0);
    let drawn = watermark(&cropped, "wm-corner-out", "top_left");
    let rect = drawn.unit_squares[0];
    assert_near((rect[0], rect[3]), (50.0, 500.0), 0.01, "top-left corner of the visible box");
}

#[test]
fn a_watermark_on_a_rotated_cropped_page_stays_inside_the_box() {
    // Rotation moves the visible region, never the mark's place in it: the
    // centre of the crop box is its centre at every rotation.
    let rotated = build("wm-rotated", [100.0, 200.0, 700.0, 1000.0], Some([150.0, 300.0, 450.0, 700.0]), 90);
    let drawn = watermark(&rotated, "wm-rotated-out", "center");
    assert_near(centre(drawn.unit_squares[0]), (300.0, 500.0), 0.01, "rotated crop centre");
}

// ---------------------------------------------------------------------------
// Page-number stamp
// ---------------------------------------------------------------------------

fn footer_options() -> HeaderFooterOptions {
    HeaderFooterOptions {
        footer_right: "Page 1".into(),
        margin_pt: 24.0,
        font_size_pt: 9.0,
        ..HeaderFooterOptions::default()
    }
}

fn stamp(input: &Path, name: &str) -> Drawn {
    let output = scratch(name);
    stamp_pdf(
        input,
        &output,
        Some(&footer_options()),
        None,
        OverwritePolicy::Replace,
        None,
        &|_| {},
        &CancelToken::new(),
    )
    .expect("stamp");
    walk(&page_stream(&std::fs::read(&output).expect("read the output")))
}

#[test]
fn a_footer_follows_the_page_box_origin() {
    let plain = build("st-plain", [0.0, 0.0, 600.0, 800.0], None, 0);
    let shifted = build("st-shifted", [100.0, 200.0, 700.0, 1000.0], None, 0);
    let a = stamp(&plain, "st-plain-out").text_anchors;
    let b = stamp(&shifted, "st-shifted-out").text_anchors;
    assert_eq!(a.len(), 1, "one footer line");
    assert_eq!(b.len(), 1, "one footer line");
    assert_near((b[0].0 - a[0].0, b[0].1 - a[0].1), (100.0, 200.0), 0.01, "the footer moves with the box origin");
}

#[test]
fn a_footer_sits_on_the_crop_box_corner() {
    // Right-aligned at `width - margin`, so the difference from the uncropped
    // page is the box origin plus the width the viewer shows.
    let plain = build("st-plain2", [0.0, 0.0, 600.0, 800.0], None, 0);
    let cropped = build("st-cropped", [0.0, 0.0, 600.0, 800.0], Some([50.0, 100.0, 350.0, 500.0]), 0);
    let a = stamp(&plain, "st-plain2-out").text_anchors;
    let b = stamp(&cropped, "st-cropped-out").text_anchors;
    assert_near(
        (b[0].0 - a[0].0, b[0].1 - a[0].1),
        (50.0 + 300.0 - 600.0, 100.0 + 0.0),
        0.01,
        "the footer follows the visible box",
    );
}
