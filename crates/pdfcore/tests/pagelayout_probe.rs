//! What Resize and Crop are supposed to do to the page a user sees.
//!
//! Every expectation below follows from ISO 32000-1 (page boundaries in 14.11.2,
//! attribute inheritance in 7.7.3.4) or from the product's own contract: resize
//! scales the page content to a chosen size, crop selects a visible rectangle in
//! display space, and pages the user did not select are left alone.

use std::path::{Path, PathBuf};

use lopdf::{dictionary, Dictionary, Document, Object, Stream};
use pdfcore::docutil::OverwritePolicy;
use pdfcore::pagelayout::{crop_pages, resize_pages, CropItem, ResizeOptions};
use pdfcore::progress::CancelToken;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("omnioffice-pagelayout-probe");
    std::fs::create_dir_all(&dir).expect("scratch directory");
    dir.join(format!("{name}.pdf"))
}

fn rect(values: [f64; 4]) -> Object {
    Object::Array(values.iter().map(|value| Object::Real(*value as f32)).collect())
}

/// A one-page document: `media` box, optional `/Rotate`, one highlight
/// annotation, and a content stream with a single text run.
fn fixture(name: &str, media: [f64; 4], rotate: i64, annot: Option<[f64; 4]>) -> PathBuf {
    let mut doc = Document::with_version("1.7");
    let pages = doc.new_object_id();
    let content = doc
        .add_object(Object::Stream(Stream::new(Dictionary::new(), b"BT /F1 24 Tf 72 720 Td (Visible) Tj ET".to_vec())));
    let font = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    }));
    let mut page = dictionary! {
        "Type" => "Page",
        "Parent" => Object::Reference(pages),
        "MediaBox" => rect(media),
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => Object::Reference(font) } },
        "Contents" => Object::Reference(content),
    };
    if rotate != 0 {
        page.set("Rotate", Object::Integer(rotate));
    }
    if let Some(annot_rect) = annot {
        let annot = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Highlight",
            "Rect" => rect(annot_rect),
            "F" => 4i64,
            "Contents" => Object::string_literal("note"),
        }));
        page.set("Annots", Object::Array(vec![Object::Reference(annot)]));
    }
    let page = doc.add_object(Object::Dictionary(page));
    doc.objects.insert(
        pages,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => Object::Array(vec![Object::Reference(page)]),
            "Count" => Object::Integer(1),
        }),
    );
    let catalog = doc.new_object_id();
    doc.objects
        .insert(catalog, Object::Dictionary(dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages) }));
    doc.trailer.set("Root", Object::Reference(catalog));
    let path = scratch(name);
    doc.save(&path).expect("save fixture");
    path
}

fn resize(input: &Path, output: &Path, size: &str, mode: &str) {
    let options = ResizeOptions { page_size: size.to_string(), mode: mode.to_string(), ..ResizeOptions::default() };
    resize_pages(input, output, &options, OverwritePolicy::Replace, None, &|_| {}, &CancelToken::new())
        .expect("resize");
}

fn media_of(path: &Path) -> [f64; 4] {
    let doc = Document::load(path).expect("load");
    let page_id = *doc.get_pages().get(&1).expect("page one");
    let media = doc.get_dictionary(page_id).and_then(|page| page.get(b"MediaBox")).expect("MediaBox");
    let values: Vec<f64> =
        media.as_array().expect("array").iter().map(|v| v.as_float().unwrap_or(0.0) as f64).collect();
    [values[0], values[1], values[2], values[3]]
}

fn annot_rect(path: &Path) -> [f64; 4] {
    let doc = Document::load(path).expect("load");
    let page_id = *doc.get_pages().get(&1).expect("page one");
    let page = doc.get_dictionary(page_id).expect("page");
    let annots = page.get(b"Annots").expect("annots").as_array().expect("array");
    let annot = annots[0].as_reference().expect("reference");
    let dict = doc.get_dictionary(annot).expect("annot");
    let values: Vec<f64> = dict
        .get(b"Rect")
        .expect("Rect")
        .as_array()
        .expect("array")
        .iter()
        .map(|v| v.as_float().unwrap_or(0.0) as f64)
        .collect();
    [values[0], values[1], values[2], values[3]]
}

#[test]
fn resize_sets_the_requested_media_box() {
    let input = fixture("control", [0.0, 0.0, 595.0, 842.0], 0, None);
    let output = scratch("control-out");
    resize(&input, &output, "a5", "fit");
    let media = media_of(&output);
    assert!((media[2] - 420.0).abs() < 1.0 && (media[3] - 595.0).abs() < 1.0, "A5 portrait: {media:?}");
}

#[test]
fn a_highlight_stays_on_the_text_after_resize() {
    // The annotation covers the text baseline at y=720 on an A4 page. Resizing to
    // A5 halves the page, so the content moves with it - and so must the box that
    // marks it.
    let input = fixture("annot", [0.0, 0.0, 595.0, 842.0], 0, Some([72.0, 715.0, 240.0, 745.0]));
    let output = scratch("annot-out");
    resize(&input, &output, "a5", "fit");
    let media = media_of(&output);
    let scale = media[2] / 595.0;
    let rect = annot_rect(&output);
    assert!(
        (rect[0] - 72.0 * scale).abs() < 1.0 && (rect[1] - 715.0 * scale).abs() < 1.0,
        "the highlight should follow the content: scale {scale:.3}, got {rect:?}"
    );
}

#[test]
fn a_rotated_page_reports_the_visible_size() {
    // `/Rotate 90` makes a portrait page look landscape. Resizing it to A4
    // landscape has to produce a page whose visible size is 842 x 595, whatever
    // the MediaBox holds.
    let input = fixture("rotate", [0.0, 0.0, 595.0, 842.0], 90, None);
    let output = scratch("rotate-out");
    resize(&input, &output, "a4", "fit");
    let media = media_of(&output);
    let (w, h) = (media[2] - media[0], media[3] - media[1]);
    assert!((w - 595.0).abs() < 1.0 && (h - 842.0).abs() < 1.0, "a rotated A4 keeps its visible size: {w} x {h}");
}

#[test]
fn crop_lands_on_the_visible_region_of_an_offset_page() {
    // A MediaBox that does not start at the origin: the crop rectangle is given in
    // display space, so the CropBox has to be offset by the page's own origin.
    let input = fixture("offset", [10.0, 20.0, 605.0, 862.0], 0, None);
    let output = scratch("offset-out");
    let crops = [CropItem { page: 1, x: 0.0, y: 0.0, w: 100.0, h: 100.0 }];
    crop_pages(&input, &output, &crops, OverwritePolicy::Replace, None).expect("crop");
    let doc = Document::load(&output).expect("load");
    let page_id = *doc.get_pages().get(&1).expect("page");
    let crop = doc.get_dictionary(page_id).and_then(|page| page.get(b"CropBox")).expect("CropBox");
    let values: Vec<f64> = crop.as_array().expect("array").iter().map(|v| v.as_float().unwrap_or(0.0) as f64).collect();
    // Display top-left 100x100 on a page 595x842 from origin (10,20) is the
    // rectangle (10, 762) .. (110, 862) in page space.
    assert!(
        (values[0] - 10.0).abs() < 1.0 && (values[1] - 762.0).abs() < 1.0,
        "the crop should honour the page origin: {values:?}"
    );
}
