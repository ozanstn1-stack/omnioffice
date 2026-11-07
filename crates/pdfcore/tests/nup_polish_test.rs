//! Line leaders (`/LL`, `/LLE`, `/LLO`) in synthesized appearances.

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

fn line_annot(rect: [f64; 4], anchors: [f64; 4]) -> Dictionary {
    dictionary! {
        "Type" => "Annot",
        "Subtype" => "Line",
        "Rect" => rect_array(rect),
        "F" => 4i64,
        "L" => numbers(&anchors),
        "C" => numbers(&[0.0, 0.0, 0.0]),
        "BS" => dictionary! { "W" => 1i64 },
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

fn bbox_of(doc: &Document, sheet: ObjectId, name: &str) -> [f64; 4] {
    let id = xobject(doc, sheet, name).expect("drawn XObject");
    let stream = doc.get_object(id).and_then(|object| object.as_stream()).expect("stream");
    markup_fixtures::read_rect(stream.dict.get(b"BBox").expect("bbox")).expect("rect")
}

fn render(annot: Dictionary) -> (Document, ObjectId) {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    fixture.annot(0, annot);
    let input = dir.path("line.pdf");
    fixture.write(&input);
    let out = dir.path("line-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    (doc, sheet)
}

#[test]
fn a_positive_leader_draws_the_line_beside_its_anchors() {
    let mut annot = line_annot([100.0, 100.0, 200.0, 100.0], [100.0, 100.0, 200.0, 100.0]);
    annot.set("LL", Object::Real(20.0));
    let (doc, sheet) = render(annot);
    let ops = ops_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert!(ops.contains("100 120 m\n200 120 l\n"), "the line moved to the left normal: {ops:?}");
    assert!(ops.contains("100 100 m\n100 120 l\n"), "a leader joins the anchor: {ops:?}");
    assert!(ops.contains("200 100 m\n200 120 l\n"), "both anchors get one: {ops:?}");
}

#[test]
fn a_negative_leader_flips_the_side() {
    let mut annot = line_annot([100.0, 100.0, 200.0, 100.0], [100.0, 100.0, 200.0, 100.0]);
    annot.set("LL", Object::Real(-20.0));
    let (doc, sheet) = render(annot);
    let ops = ops_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert!(ops.contains("100 80 m\n200 80 l\n"), "the line sits below: {ops:?}");
}

#[test]
fn a_leader_extension_runs_past_the_line() {
    let mut annot = line_annot([100.0, 100.0, 200.0, 100.0], [100.0, 100.0, 200.0, 100.0]);
    annot.set("LL", Object::Real(20.0));
    annot.set("LLE", Object::Real(5.0));
    let (doc, sheet) = render(annot);
    let ops = ops_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert!(ops.contains("100 100 m\n100 125 l\n"), "the leader reaches past the line: {ops:?}");
}

#[test]
fn a_leader_offset_leaves_a_gap_at_the_anchor() {
    let mut annot = line_annot([100.0, 100.0, 200.0, 100.0], [100.0, 100.0, 200.0, 100.0]);
    annot.set("LL", Object::Real(20.0));
    annot.set("LLO", Object::Real(3.0));
    let (doc, sheet) = render(annot);
    let ops = ops_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert!(ops.contains("100 103 m\n100 120 l\n"), "the leader starts three points out: {ops:?}");
}

#[test]
fn a_line_without_a_leader_keeps_its_output() {
    let annot = line_annot([100.0, 100.0, 200.0, 100.0], [100.0, 100.0, 200.0, 100.0]);
    let (doc, sheet) = render(annot);
    let ops = ops_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert!(ops.contains("100 100 m\n200 100 l\n"), "{ops:?}");
    assert_eq!(ops.matches(" m\n").count(), 1, "no leaders are added: {ops:?}");
}

#[test]
fn extension_and_offset_without_a_leader_are_ignored() {
    let mut annot = line_annot([100.0, 100.0, 200.0, 100.0], [100.0, 100.0, 200.0, 100.0]);
    annot.set("LLE", Object::Real(5.0));
    annot.set("LLO", Object::Real(5.0));
    let (doc, sheet) = render(annot);
    let ops = ops_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert_eq!(ops.matches(" m\n").count(), 1, "nothing to extend without /LL: {ops:?}");
}

#[test]
fn line_ends_sit_on_the_shifted_line() {
    let mut annot = line_annot([100.0, 100.0, 200.0, 100.0], [100.0, 100.0, 200.0, 100.0]);
    annot.set("LL", Object::Real(20.0));
    annot.set("LE", Object::Array(vec![Object::Name(b"OpenArrow".to_vec()), Object::Name(b"None".to_vec())]));
    let (doc, sheet) = render(annot);
    let ops = ops_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert!(ops.contains("[] 0 d\n"), "an end block is drawn: {ops:?}");
    assert!(ops.contains("100 120"), "the end uses the shifted anchor: {ops:?}");
}

#[test]
fn the_box_covers_the_shifted_line_and_leaders() {
    let mut annot = line_annot([100.0, 100.0, 200.0, 100.0], [100.0, 100.0, 200.0, 100.0]);
    annot.set("LL", Object::Real(20.0));
    annot.set("LLE", Object::Real(5.0));
    let (doc, sheet) = render(annot);
    let bbox = bbox_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert!(bbox[3] >= 125.0, "the box reaches the leader tip: {bbox:?}");
}

#[test]
fn a_collapsed_line_is_skipped() {
    let annot = line_annot([100.0, 100.0, 100.0, 100.0], [100.0, 100.0, 100.0, 100.0]);
    let (doc, sheet) = render(annot);
    assert!(painted(&doc, sheet).is_empty(), "a line without direction has no appearance");
}

#[test]
fn a_dashed_leader_is_dashed_too() {
    let mut annot = line_annot([100.0, 100.0, 200.0, 100.0], [100.0, 100.0, 200.0, 100.0]);
    annot.set("LL", Object::Real(20.0));
    annot.set("BS", dictionary! { "W" => 1i64, "S" => "D", "D" => numbers(&[3.0, 2.0]) });
    let (doc, sheet) = render(annot);
    let ops = ops_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert!(ops.contains("[3 2] 0 d\n"), "the dash is set once for the whole path: {ops:?}");
}
