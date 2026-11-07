//! Line endings (`/LE`) on synthesized lines, polylines and callouts.

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

fn annot(subtype: &str, rect: [f64; 4]) -> Dictionary {
    dictionary! {
        "Type" => "Annot",
        "Subtype" => Object::Name(subtype.as_bytes().to_vec()),
        "Rect" => rect_array(rect),
        "F" => 4i64,
    }
}

fn numbers(values: &[f64]) -> Object {
    Object::Array(values.iter().map(|value| Object::Real(*value as f32)).collect())
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

/// A line from (100,100) to (130,140), the 3-4-5 case the expectations use.
fn diagonal_line(ends: Option<[&str; 2]>) -> Dictionary {
    let mut line = annot("Line", [100.0, 100.0, 130.0, 140.0]);
    line.set("L", numbers(&[100.0, 100.0, 130.0, 140.0]));
    line.set("C", numbers(&[0.0, 0.0, 0.0]));
    line.set("BS", dictionary! { "W" => 2i64 });
    if let Some(ends) = ends {
        line.set("LE", Object::Array(ends.iter().map(|name| Object::Name(name.as_bytes().to_vec())).collect()));
    }
    line
}

#[test]
fn closed_arrows_sit_at_both_ends_of_a_diagonal_line() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    fixture.annot(0, diagonal_line(Some(["ClosedArrow", "ClosedArrow"])));
    let input = dir.path("arrow.pdf");
    fixture.write(&input);

    let out = dir.path("arrow-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let ops = ops_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    // Bitiş ucu: tepe (130,140), kanatlar (124,137) ve (128.8,133.4).
    assert!(ops.contains("130 140 m"), "{ops:?}");
    assert!(ops.contains("124 137 l"), "{ops:?}");
    assert!(ops.contains("128.8 133.4 l"), "{ops:?}");
    // Başlangıç ucu ters yönde.
    assert!(ops.contains("100 100 m"), "{ops:?}");
    assert_eq!(ops.matches("q\n[] 0 d\n").count(), 2, "one block per end: {ops:?}");
}

#[test]
fn an_open_arrow_is_a_polyline_not_a_fill() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    fixture.annot(0, diagonal_line(Some(["OpenArrow", "None"])));
    let input = dir.path("open.pdf");
    fixture.write(&input);

    let out = dir.path("open-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let ops = ops_of(&doc, sheets(&doc)[0].1, &painted(&doc, sheets(&doc)[0].1)[0].name);
    let end_blocks: Vec<&str> = ops.split("q\n[] 0 d\n").skip(1).collect();
    assert_eq!(end_blocks.len(), 1, "only the start has an end: {ops:?}");
    assert!(end_blocks[0].ends_with("S\nQ\n"), "an open arrow is stroked: {end_blocks:?}");
}

#[test]
fn the_closed_kinds_fill_when_the_line_has_an_interior_colour() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    for (index, kind) in ["Square", "Circle", "Diamond"].into_iter().enumerate() {
        let mut line = diagonal_line(Some([kind, "None"]));
        line.set("IC", numbers(&[0.0, 1.0, 0.0]));
        line.set("Rect", rect_array([100.0 + index as f64 * 200.0, 100.0, 130.0, 140.0]));
        line.set("L", numbers(&[100.0 + index as f64 * 200.0, 100.0, 130.0, 140.0]));
        fixture.annot(0, line);
    }
    let input = dir.path("kinds.pdf");
    fixture.write(&input);

    let out = dir.path("kinds-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let drawn = painted(&doc, sheet);
    assert_eq!(drawn.len(), 3);
    let square = ops_of(&doc, sheet, &drawn[0].name);
    assert!(square.contains("0 1 0 rg"), "the fill colour is used: {square:?}");
    let circle = ops_of(&doc, sheet, &drawn[1].name);
    assert_eq!(circle.matches(" c\n").count(), 4, "a circle end is four curves: {circle:?}");
    let diamond = ops_of(&doc, sheet, &drawn[2].name);
    assert!(diamond.contains("h\nb\nQ\n"), "{diamond:?}");
}

#[test]
fn butt_and_slash_are_single_strokes() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    fixture.annot(0, diagonal_line(Some(["Butt", "Slash"])));
    let input = dir.path("plain.pdf");
    fixture.write(&input);

    let out = dir.path("plain-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let ops = ops_of(&doc, sheets(&doc)[0].1, &painted(&doc, sheets(&doc)[0].1)[0].name);
    let end_blocks: Vec<&str> = ops.split("q\n[] 0 d\n").skip(1).collect();
    assert_eq!(end_blocks.len(), 2);
    for block in end_blocks {
        assert_eq!(block.matches(" m\n").count(), 1, "single segment: {block:?}");
        assert_eq!(block.matches(" l\n").count(), 1, "single segment: {block:?}");
    }
}

#[test]
fn a_reversed_arrow_points_past_the_tip() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    fixture.annot(0, diagonal_line(Some(["None", "RClosedArrow"])));
    let input = dir.path("reversed.pdf");
    fixture.write(&input);

    let out = dir.path("reversed-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let ops = ops_of(&doc, sheets(&doc)[0].1, &painted(&doc, sheets(&doc)[0].1)[0].name);
    assert!(ops.contains("136 143 l"), "a wing sits past the tip: {ops:?}");
}

#[test]
fn no_ends_and_unknown_names_draw_nothing() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    fixture.annot(0, diagonal_line(None));
    fixture.annot(0, diagonal_line(Some(["None", "None"])));
    fixture.annot(0, diagonal_line(Some(["SomethingElse", "None"])));
    let input = dir.path("none.pdf");
    fixture.write(&input);

    let out = dir.path("none-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    for block in painted(&doc, sheet) {
        let ops = ops_of(&doc, sheet, &block.name);
        assert!(!ops.contains("[] 0 d\n\n"), "no end block: {ops:?}");
        assert_eq!(ops.matches("q\n[] 0 d\n").count(), 0, "{ops:?}");
    }
}

#[test]
fn a_dashed_line_still_gets_solid_ends() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let mut line = diagonal_line(Some(["ClosedArrow", "ClosedArrow"]));
    line.set("BS", dictionary! { "S" => "D", "W" => 2i64, "D" => numbers(&[4.0, 2.0]) });
    fixture.annot(0, line);
    let input = dir.path("dashed.pdf");
    fixture.write(&input);

    let out = dir.path("dashed-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let ops = ops_of(&doc, sheets(&doc)[0].1, &painted(&doc, sheets(&doc)[0].1)[0].name);
    assert!(ops.contains("[4 2] 0 d\n"), "the line is dashed: {ops:?}");
    assert_eq!(ops.matches("[] 0 d\n").count(), 2, "both ends are solid: {ops:?}");
}

#[test]
fn a_free_text_callout_gets_its_end_at_the_first_point() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let mut note = annot("FreeText", [100.0, 100.0, 300.0, 200.0]);
    note.set("Contents", Object::string_literal("look here"));
    note.set("DA", Object::string_literal("/Helv 10 Tf 0 g"));
    note.set("CL", numbers(&[60.0, 60.0, 120.0, 120.0]));
    note.set("LE", Object::Name(b"ClosedArrow".to_vec()));
    fixture.annot(0, note);
    let input = dir.path("callout.pdf");
    fixture.write(&input);

    let out = dir.path("callout-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let ops = ops_of(&doc, sheets(&doc)[0].1, &painted(&doc, sheets(&doc)[0].1)[0].name);
    assert!(ops.contains("[] 0 d\n"), "the callout end is solid: {ops:?}");
    assert!(ops.contains("60 60 m"), "the arrow sits at the first callout point: {ops:?}");
}

#[test]
fn a_polyline_carries_ends_on_its_last_segment() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let mut line = annot("PolyLine", [100.0, 100.0, 300.0, 200.0]);
    line.set("Vertices", numbers(&[100.0, 100.0, 200.0, 200.0, 300.0, 100.0]));
    line.set("C", numbers(&[0.0, 0.0, 0.0]));
    line.set("BS", dictionary! { "W" => 2i64 });
    line.set("LE", Object::Array(vec![Object::Name(b"None".to_vec()), Object::Name(b"OpenArrow".to_vec())]));
    fixture.annot(0, line);
    let input = dir.path("polyline.pdf");
    fixture.write(&input);

    let out = dir.path("polyline-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let ops = ops_of(&doc, sheets(&doc)[0].1, &painted(&doc, sheets(&doc)[0].1)[0].name);
    let blocks: Vec<&str> = ops.split("q\n[] 0 d\n").skip(1).collect();
    assert_eq!(blocks.len(), 1, "only the end of the polyline has an arrow: {ops:?}");
    assert!(blocks[0].contains("300 100"), "the arrow sits at the last point: {ops:?}");
}

#[test]
fn the_box_covers_the_line_ends() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    fixture.annot(0, diagonal_line(Some(["ClosedArrow", "ClosedArrow"])));
    let input = dir.path("box.pdf");
    fixture.write(&input);

    let out = dir.path("box-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let bbox = bbox_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert!(bbox[2] > 130.0 && bbox[3] > 140.0, "the box grows past the tip: {bbox:?}");
}

#[test]
fn a_rotated_page_keeps_the_same_end_operators() {
    fn ends_ops(rotate: i64) -> String {
        let dir = TestDir::new();
        let mut fixture = Fixture::new(&[PageSpec { rotate, ..PageSpec::default() }]);
        fixture.annot(0, diagonal_line(Some(["ClosedArrow", "OpenArrow"])));
        let input = dir.path("rot.pdf");
        fixture.write(&input);
        let out = dir.path("rot-out.pdf");
        impose(&input, &out, &NupOptions { pages: vec![1], ..Default::default() });
        let doc = Document::load(&out).expect("load output");
        ops_of(&doc, sheets(&doc)[0].1, &painted(&doc, sheets(&doc)[0].1)[0].name)
    }
    assert_eq!(ends_ops(0), ends_ops(90), "the ends are built in page space");
}
