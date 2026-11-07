//! Polygons, polylines and dashed borders in synthesized appearances.

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

/// An annotation without `/AP`, so the synthesizer runs.
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

#[test]
fn a_polygon_is_filled_and_stroked() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let mut polygon = annot("Polygon", [100.0, 100.0, 300.0, 200.0]);
    polygon.set("Vertices", numbers(&[100.0, 100.0, 300.0, 100.0, 200.0, 200.0]));
    polygon.set("C", numbers(&[1.0, 0.0, 0.0]));
    polygon.set("IC", numbers(&[0.0, 0.0, 1.0]));
    polygon.set("BS", dictionary! { "W" => 2i64 });
    fixture.annot(0, polygon);
    let input = dir.path("polygon.pdf");
    fixture.write(&input);

    let out = dir.path("polygon-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let drawn = painted(&doc, sheet);
    assert_eq!(drawn.len(), 1);
    let ops = ops_of(&doc, sheet, &drawn[0].name);
    assert!(ops.contains("1 0 0 RG") && ops.contains("0 0 1 rg"), "{ops:?}");
    assert!(ops.contains("2 w"), "{ops:?}");
    assert!(ops.contains("100 100 m\n300 100 l\n200 200 l\nh\n"), "the path is closed: {ops:?}");
    assert!(ops.ends_with("B\n"), "stroke and fill: {ops:?}");
}

#[test]
fn a_polygon_can_be_stroke_only_or_fill_only() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let mut stroked = annot("Polygon", [100.0, 100.0, 200.0, 200.0]);
    stroked.set("Vertices", numbers(&[100.0, 100.0, 200.0, 100.0, 150.0, 200.0]));
    stroked.set("C", numbers(&[0.0, 0.0, 0.0]));
    fixture.annot(0, stroked);
    let mut filled = annot("Polygon", [300.0, 100.0, 400.0, 200.0]);
    filled.set("Vertices", numbers(&[300.0, 100.0, 400.0, 100.0, 350.0, 200.0]));
    filled.set("IC", numbers(&[0.0, 1.0, 0.0]));
    fixture.annot(0, filled);
    let input = dir.path("modes.pdf");
    fixture.write(&input);

    let out = dir.path("modes-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let drawn = painted(&doc, sheet);
    assert_eq!(drawn.len(), 2);
    let stroked = ops_of(&doc, sheet, &drawn[0].name);
    assert!(stroked.ends_with("S\n") && !stroked.contains(" rg"), "{stroked:?}");
    let filled = ops_of(&doc, sheet, &drawn[1].name);
    assert!(filled.ends_with("f\n") && !filled.contains("RG"), "{filled:?}");
}

#[test]
fn a_polyline_stays_open_and_is_never_filled() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let mut line = annot("PolyLine", [100.0, 100.0, 300.0, 200.0]);
    line.set("Vertices", numbers(&[100.0, 100.0, 200.0, 200.0, 300.0, 100.0]));
    line.set("C", numbers(&[0.0, 0.0, 0.0]));
    line.set("IC", numbers(&[1.0, 0.0, 0.0]));
    fixture.annot(0, line);
    let input = dir.path("polyline.pdf");
    fixture.write(&input);

    let out = dir.path("polyline-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let ops = ops_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert!(ops.ends_with("S\n"), "{ops:?}");
    assert!(!ops.contains("\nh\n"), "the path stays open: {ops:?}");
    assert!(!ops.contains(" rg"), "the interior colour does not fill a polyline: {ops:?}");
}

#[test]
fn broken_vertices_skip_only_their_annotation() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    for (index, vertices) in [
        vec![Object::Integer(1), Object::Integer(2), Object::Integer(3)],
        vec![Object::Integer(1), Object::Integer(2)],
        vec![Object::Name(b"x".to_vec()), Object::Integer(2), Object::Integer(3), Object::Integer(4)],
    ]
    .into_iter()
    .enumerate()
    {
        let mut broken = annot("Polygon", [100.0 + index as f64 * 10.0, 100.0, 200.0, 200.0]);
        broken.set("Vertices", Object::Array(vertices));
        broken.set("C", numbers(&[0.0, 0.0, 0.0]));
        fixture.annot(0, broken);
    }
    let mut missing = annot("Polygon", [300.0, 100.0, 400.0, 200.0]);
    missing.set("C", numbers(&[0.0, 0.0, 0.0]));
    fixture.annot(0, missing);
    let mut good = annot("Polygon", [450.0, 100.0, 550.0, 200.0]);
    good.set("Vertices", numbers(&[450.0, 100.0, 550.0, 100.0, 500.0, 200.0]));
    good.set("C", numbers(&[0.0, 0.0, 0.0]));
    fixture.annot(0, good);
    let input = dir.path("broken.pdf");
    fixture.write(&input);

    let out = dir.path("broken-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    assert_eq!(painted(&doc, sheets(&doc)[0].1).len(), 1, "only the sound polygon is drawn");
}

#[test]
fn a_dashed_border_is_written_before_the_shape() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let mut square = annot("Square", [100.0, 100.0, 200.0, 200.0]);
    square.set("C", numbers(&[0.0, 0.0, 0.0]));
    square.set("BS", dictionary! { "S" => "D", "W" => 2i64, "D" => numbers(&[3.0, 2.0]) });
    fixture.annot(0, square);
    let input = dir.path("dashed.pdf");
    fixture.write(&input);

    let out = dir.path("dashed-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let ops = ops_of(&doc, sheets(&doc)[0].1, &painted(&doc, sheets(&doc)[0].1)[0].name);
    assert!(ops.contains("[3 2] 0 d\n"), "the dash pattern is emitted: {ops:?}");
}

#[test]
fn a_dash_style_without_a_pattern_uses_the_default() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let mut square = annot("Square", [100.0, 100.0, 200.0, 200.0]);
    square.set("C", numbers(&[0.0, 0.0, 0.0]));
    square.set("BS", dictionary! { "S" => "D", "W" => 1i64 });
    fixture.annot(0, square);
    let input = dir.path("default.pdf");
    fixture.write(&input);

    let out = dir.path("default-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let ops = ops_of(&doc, sheets(&doc)[0].1, &painted(&doc, sheets(&doc)[0].1)[0].name);
    assert!(ops.contains("[3] 0 d\n"), "{ops:?}");
}

#[test]
fn a_broken_pattern_draws_a_solid_border() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    for (index, pattern) in [vec![-1.0, 2.0], vec![0.0, 0.0]].into_iter().enumerate() {
        let mut square = annot("Square", [100.0 + index as f64 * 150.0, 100.0, 200.0, 200.0]);
        square.set("C", numbers(&[0.0, 0.0, 0.0]));
        square.set("BS", dictionary! { "S" => "D", "W" => 1i64, "D" => numbers(&pattern) });
        fixture.annot(0, square);
    }
    let input = dir.path("solid.pdf");
    fixture.write(&input);

    let out = dir.path("solid-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let drawn = painted(&doc, sheet);
    assert_eq!(drawn.len(), 2, "both squares are still drawn");
    for block in &drawn {
        let ops = ops_of(&doc, sheet, &block.name);
        assert!(!ops.contains(" 0 d\n"), "the border is solid: {ops:?}");
    }
}

#[test]
fn a_solid_style_ignores_a_pattern() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let mut square = annot("Square", [100.0, 100.0, 200.0, 200.0]);
    square.set("C", numbers(&[0.0, 0.0, 0.0]));
    square.set("BS", dictionary! { "S" => "S", "W" => 1i64, "D" => numbers(&[3.0, 2.0]) });
    fixture.annot(0, square);
    let input = dir.path("style.pdf");
    fixture.write(&input);

    let out = dir.path("style-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let ops = ops_of(&doc, sheets(&doc)[0].1, &painted(&doc, sheets(&doc)[0].1)[0].name);
    assert!(!ops.contains(" 0 d\n"), "a solid style wins over a stray pattern: {ops:?}");
}

#[test]
fn a_dashed_line_carries_the_pattern() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let mut line = annot("Line", [50.0, 50.0, 250.0, 120.0]);
    line.set("L", numbers(&[50.0, 50.0, 250.0, 120.0]));
    line.set("C", numbers(&[0.0, 0.0, 0.0]));
    line.set("BS", dictionary! { "S" => "D", "W" => 1i64, "D" => numbers(&[4.0, 1.0]) });
    fixture.annot(0, line);
    let input = dir.path("line.pdf");
    fixture.write(&input);

    let out = dir.path("line-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let ops = ops_of(&doc, sheets(&doc)[0].1, &painted(&doc, sheets(&doc)[0].1)[0].name);
    assert!(ops.contains("[4 1] 0 d\n") && ops.contains("50 50 m\n250 120 l\n"), "{ops:?}");
}

#[test]
fn a_dashed_free_text_frame_carries_the_pattern() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let mut note = annot("FreeText", [100.0, 100.0, 300.0, 200.0]);
    note.set("Contents", Object::string_literal("dashed box"));
    note.set("DA", Object::string_literal("/Helv 12 Tf 0 g"));
    note.set("BS", dictionary! { "S" => "D", "W" => 1i64, "D" => numbers(&[2.0, 1.0]) });
    fixture.annot(0, note);
    let input = dir.path("note.pdf");
    fixture.write(&input);

    let out = dir.path("note-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let ops = ops_of(&doc, sheets(&doc)[0].1, &painted(&doc, sheets(&doc)[0].1)[0].name);
    assert!(ops.contains("[2 1] 0 d\n"), "the frame is dashed: {ops:?}");
}

#[test]
fn a_polygon_on_a_rotated_page_keeps_its_operators() {
    fn polygon_ops(rotate: i64) -> String {
        let dir = TestDir::new();
        let mut fixture = Fixture::new(&[PageSpec { rotate, ..PageSpec::default() }]);
        let mut polygon = annot("Polygon", [100.0, 100.0, 200.0, 200.0]);
        polygon.set("Vertices", numbers(&[100.0, 100.0, 200.0, 100.0, 150.0, 200.0]));
        polygon.set("C", numbers(&[0.0, 0.0, 0.0]));
        fixture.annot(0, polygon);
        let input = dir.path("rot.pdf");
        fixture.write(&input);
        let out = dir.path("rot-out.pdf");
        impose(&input, &out, &NupOptions { pages: vec![1], ..Default::default() });
        let doc = Document::load(&out).expect("load output");
        ops_of(&doc, sheets(&doc)[0].1, &painted(&doc, sheets(&doc)[0].1)[0].name)
    }
    assert_eq!(polygon_ops(0), polygon_ops(90), "the appearance is built in page space");
}
