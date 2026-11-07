//! `/BS /S` border styles (bevel, inset, underline) in synthesized appearances.

#[path = "common/link_fixtures.rs"]
mod fixtures;
#[path = "common/markup_fixtures.rs"]
mod markup_fixtures;

use fixtures::{impose, page_content, rect_array, sheets, Fixture, PageSpec, TestDir};
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId};
use markup_fixtures::{blocks, xobject};
use pdfcore::nup::NupOptions;

fn page_specs(count: usize) -> Vec<PageSpec> {
    (0..count).map(|_| PageSpec::default()).collect()
}

fn numbers(values: &[f64]) -> Object {
    Object::Array(values.iter().map(|value| Object::Real(*value as f32)).collect())
}

fn annot(subtype: &str, rect: [f64; 4], style: &str) -> Dictionary {
    dictionary! {
        "Type" => "Annot",
        "Subtype" => Object::Name(subtype.as_bytes().to_vec()),
        "Rect" => rect_array(rect),
        "F" => 4i64,
        "C" => numbers(&[0.0, 0.0, 0.0]),
        "BS" => dictionary! { "W" => 2i64, "S" => Object::Name(style.as_bytes().to_vec()) },
    }
}

fn painted(doc: &Document, sheet: ObjectId) -> Vec<markup_fixtures::Block> {
    blocks(&page_content(doc, sheet)).into_iter().filter(|block| block.name.starts_with("AnX")).collect()
}

fn ops_of(doc: &Document, sheet: ObjectId, name: &str) -> String {
    let id = xobject(doc, sheet, name).expect("drawn XObject");
    let stream = doc.get_object(id).and_then(|object| object.as_stream()).expect("stream");
    let bytes = stream.decompressed_content().unwrap_or_else(|_| stream.content.clone());
    String::from_utf8_lossy(&bytes).to_string()
}

fn render(annot: Dictionary, dir: &TestDir, name: &str) -> (Document, ObjectId) {
    let mut fixture = Fixture::new(&page_specs(2));
    fixture.annot(0, annot);
    let input = dir.path(&format!("{name}.pdf"));
    fixture.write(&input);
    let out = dir.path(&format!("{name}-out.pdf"));
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    (doc, sheet)
}

#[test]
fn a_beveled_square_paints_two_tones() {
    let dir = TestDir::new();
    let (doc, sheet) = render(annot("Square", [100.0, 100.0, 200.0, 150.0], "B"), &dir, "bevel");
    let ops = ops_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert!(ops.contains("re S\n"), "the base frame is stroked: {ops:?}");
    assert_eq!(ops.matches("re f\n").count(), 4, "four bands: {ops:?}");
    assert!(ops.contains("0.25 0.25 0.25 rg"), "the light tone: {ops:?}");
    assert!(ops.contains("0 0 0 rg"), "the dark tone: {ops:?}");
}

#[test]
fn an_inset_square_swaps_the_tones() {
    let dir = TestDir::new();
    let (bevel_doc, bevel_sheet) = render(annot("Square", [100.0, 100.0, 200.0, 150.0], "B"), &dir, "b");
    let (inset_doc, inset_sheet) = render(annot("Square", [100.0, 100.0, 200.0, 150.0], "I"), &dir, "i");
    let bevel = ops_of(&bevel_doc, bevel_sheet, &painted(&bevel_doc, bevel_sheet)[0].name);
    let inset = ops_of(&inset_doc, inset_sheet, &painted(&inset_doc, inset_sheet)[0].name);
    let light = "0.25 0.25 0.25 rg";
    assert!(bevel.find(light) < bevel.find("0 0 0 rg"), "beveled starts light");
    assert!(inset.find(light) > inset.find("0 0 0 rg"), "inset starts dark");
}

#[test]
fn an_underline_keeps_only_the_bottom_edge() {
    let dir = TestDir::new();
    let mut square = annot("Square", [100.0, 100.0, 200.0, 150.0], "U");
    square.set("IC", numbers(&[0.0, 1.0, 0.0]));
    let (doc, sheet) = render(square, &dir, "underline");
    let ops = ops_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert!(!ops.contains("re S"), "no frame: {ops:?}");
    assert!(ops.contains("101 101 m\n199 101 l\n"), "the bottom edge only: {ops:?}");
    assert!(ops.contains("re f\n"), "the interior is still filled: {ops:?}");
}

#[test]
fn a_beveled_circle_splits_into_arcs() {
    let dir = TestDir::new();
    let (doc, sheet) = render(annot("Circle", [100.0, 100.0, 200.0, 160.0], "B"), &dir, "circle");
    let ops = ops_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert_eq!(ops.matches("S\n").count(), 3, "the outline and two bands: {ops:?}");
    assert!(ops.contains("0.25 0.25 0.25 RG"), "the light band: {ops:?}");
}

#[test]
fn a_beveled_polygon_colours_each_edge() {
    let dir = TestDir::new();
    let mut polygon = annot("Polygon", [100.0, 100.0, 200.0, 200.0], "B");
    polygon.set("Vertices", numbers(&[100.0, 100.0, 200.0, 100.0, 200.0, 200.0, 100.0, 200.0]));
    let (doc, sheet) = render(polygon, &dir, "polygon");
    let ops = ops_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert!(ops.contains("2 J"), "edges meet at the corners: {ops:?}");
    assert!(ops.contains("0.25 0.25 0.25 RG") && ops.contains("0 0 0 RG"), "both tones: {ops:?}");
    assert_eq!(ops.matches(" l\n").count(), 4, "one line per edge: {ops:?}");
}

#[test]
fn a_small_shape_falls_back_to_the_plain_frame() {
    let dir = TestDir::new();
    let (doc, sheet) = render(annot("Square", [100.0, 100.0, 106.0, 200.0], "B"), &dir, "tiny");
    let ops = ops_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert_eq!(ops.matches("re f\n").count(), 0, "no band fits: {ops:?}");
    assert!(ops.contains("re S\n"), "the plain frame is drawn: {ops:?}");
}

#[test]
fn a_plain_style_keeps_its_output() {
    let dir = TestDir::new();
    let (doc, sheet) = render(annot("Square", [100.0, 100.0, 200.0, 150.0], "S"), &dir, "plain");
    let ops = ops_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert_eq!(ops, "0 0 0 RG\n2 w\n101 101 98 48 re\nS\n", "the plain frame is unchanged: {ops:?}");
}

#[test]
fn a_styled_free_text_keeps_its_text() {
    let dir = TestDir::new();
    let mut note = annot("FreeText", [100.0, 100.0, 300.0, 200.0], "B");
    note.set("Contents", Object::string_literal("styled"));
    note.set("DA", Object::string_literal("/Helv 12 Tf 0 g"));
    let (doc, sheet) = render(note, &dir, "note");
    let ops = ops_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert!(ops.contains("BT\n") && ops.contains(" Tj\n"), "the text survives: {ops:?}");
    assert_eq!(ops.matches("re f\n").count(), 5, "a background plus four bands: {ops:?}");
}

#[test]
fn a_polyline_and_a_line_ignore_the_style() {
    let dir = TestDir::new();
    let mut line = annot("Line", [50.0, 50.0, 250.0, 120.0], "B");
    line.set("L", numbers(&[50.0, 50.0, 250.0, 120.0]));
    let (doc, sheet) = render(line, &dir, "line");
    let ops = ops_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert!(!ops.contains("re f"), "a line has no frame to bevel: {ops:?}");
    assert!(ops.contains("50 50 m\n250 120 l\n"), "{ops:?}");

    let mut polyline = annot("PolyLine", [100.0, 100.0, 300.0, 200.0], "B");
    polyline.set("Vertices", numbers(&[100.0, 100.0, 200.0, 200.0, 300.0, 100.0]));
    let (doc, sheet) = render(polyline, &dir, "polyline");
    let ops = ops_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert!(!ops.contains("re f"), "an open path has no inside: {ops:?}");
}
