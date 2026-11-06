//! Annotations without an appearance stream are synthesized from geometry.

#[path = "common/link_fixtures.rs"]
mod fixtures;
#[path = "common/markup_fixtures.rs"]
mod markup_fixtures;

use fixtures::{impose, page_content, rect_array, sheets, Fixture, PageSpec, TestDir};
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId};
use markup_fixtures::{appearance, blocks, markup, xobject};
use pdfcore::nup::NupOptions;

fn page_specs(count: usize) -> Vec<PageSpec> {
    (0..count).map(|_| PageSpec::default()).collect()
}

/// A printable annotation with no `/AP`, so the synthesizer has to run.
fn annot(subtype: &str, rect: [f64; 4]) -> Dictionary {
    dictionary! {
        "Type" => "Annot",
        "Subtype" => Object::Name(subtype.as_bytes().to_vec()),
        "Rect" => rect_array(rect),
        "F" => 4i64,
    }
}

fn painted(doc: &Document, sheet: ObjectId) -> Vec<markup_fixtures::Block> {
    blocks(&page_content(doc, sheet)).into_iter().filter(|block| block.name.starts_with("AnX")).collect()
}

fn xobject_ops(doc: &Document, sheet: ObjectId, name: &str) -> String {
    let id = xobject(doc, sheet, name).expect("drawn XObject");
    let stream = doc.get_object(id).and_then(|object| object.as_stream()).expect("stream");
    let bytes = stream.decompressed_content().unwrap_or_else(|_| stream.content.clone());
    String::from_utf8_lossy(&bytes).to_string()
}

/// The synthesized appearance is a form whose content is page space.
#[test]
fn a_highlight_is_filled_and_blends() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let mut highlight = annot("Highlight", [100.0, 100.0, 200.0, 150.0]);
    highlight.set(
        "QuadPoints",
        Object::Array(vec![
            Object::Real(100.0),
            Object::Real(150.0),
            Object::Real(200.0),
            Object::Real(150.0),
            Object::Real(100.0),
            Object::Real(100.0),
            Object::Real(200.0),
            Object::Real(100.0),
        ]),
    );
    highlight.set("C", Object::Array(vec![Object::Real(1.0), Object::Real(0.9), Object::Real(0.0)]));
    fixture.annot(0, highlight);
    let input = dir.path("highlight.pdf");
    fixture.write(&input);

    let out = dir.path("highlight-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let drawn = painted(&doc, sheet);
    assert_eq!(drawn.len(), 1, "a highlight without /AP is synthesized");
    let ops = xobject_ops(&doc, sheet, &drawn[0].name);
    assert!(ops.starts_with("/H0 gs\n"), "a highlight blends: {ops:?}");
    assert!(ops.contains(" m\n") && ops.contains(" l\n"), "the quad becomes a path: {ops:?}");
    assert!(ops.ends_with("f\n"), "the path is filled: {ops:?}");
    let id = xobject(&doc, sheet, &drawn[0].name).unwrap();
    let stream = doc.get_object(id).and_then(|object| object.as_stream()).unwrap();
    let resources = stream.dict.get(b"Resources").expect("own resources").as_dict().unwrap();
    let states = resources.get(b"ExtGState").unwrap().as_dict().unwrap();
    assert_eq!(states.get(b"H0").unwrap().as_dict().unwrap().get(b"BM").unwrap().as_name().unwrap(), b"Multiply");
}

#[test]
fn the_quad_order_does_not_change_the_path() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    // Acrobat order: top-left, top-right, bottom-left, bottom-right.
    let mut first = annot("Highlight", [100.0, 100.0, 200.0, 150.0]);
    first.set(
        "QuadPoints",
        Object::Array(
            vec![100.0, 150.0, 200.0, 150.0, 100.0, 100.0, 200.0, 100.0].into_iter().map(Object::Real).collect(),
        ),
    );
    first.set("C", Object::Array(vec![Object::Real(1.0), Object::Real(0.0), Object::Real(0.0)]));
    fixture.annot(0, first);
    // Counter-clockwise order, as the specification lists them.
    let mut second = annot("Highlight", [300.0, 100.0, 400.0, 150.0]);
    second.set(
        "QuadPoints",
        Object::Array(
            vec![300.0, 100.0, 400.0, 100.0, 400.0, 150.0, 300.0, 150.0].into_iter().map(Object::Real).collect(),
        ),
    );
    second.set("C", Object::Array(vec![Object::Real(1.0), Object::Real(0.0), Object::Real(0.0)]));
    fixture.annot(0, second);
    let input = dir.path("order.pdf");
    fixture.write(&input);

    let out = dir.path("order-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let drawn = painted(&doc, sheet);
    assert_eq!(drawn.len(), 2);
    let first_ops = xobject_ops(&doc, sheet, &drawn[0].name);
    let second_ops = xobject_ops(&doc, sheet, &drawn[1].name);
    // Same shape, different location: the path must name the same corners.
    assert!(first_ops.contains("100 150 m\n"), "{first_ops:?}");
    assert!(second_ops.contains("300 150 m\n"), "{second_ops:?}");
}

#[test]
fn underline_and_strike_out_run_along_the_quad() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    for (subtype, x) in [("Underline", 100.0), ("StrikeOut", 300.0)] {
        let mut annot = annot(subtype, [x, 100.0, x + 100.0, 150.0]);
        annot.set(
            "QuadPoints",
            Object::Array(
                vec![x, 150.0, x + 100.0, 150.0, x, 100.0, x + 100.0, 100.0]
                    .into_iter()
                    .map(|value| Object::Real(value as f32))
                    .collect(),
            ),
        );
        annot.set("C", Object::Array(vec![Object::Real(0.0), Object::Real(0.0), Object::Real(1.0)]));
        fixture.annot(0, annot);
    }
    let input = dir.path("lines.pdf");
    fixture.write(&input);

    let out = dir.path("lines-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let drawn = painted(&doc, sheet);
    assert_eq!(drawn.len(), 2);
    let underline = xobject_ops(&doc, sheet, &drawn[0].name);
    assert!(underline.contains("100 100.5 m"), "the underline sits just inside the bottom edge: {underline:?}");
    let strikeout = xobject_ops(&doc, sheet, &drawn[1].name);
    assert!(strikeout.contains("300 125 m"), "the strike-out runs through the middle: {strikeout:?}");
}

#[test]
fn a_squiggly_walks_along_the_bottom_band() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let mut annot = annot("Squiggly", [100.0, 100.0, 200.0, 150.0]);
    annot.set(
        "QuadPoints",
        Object::Array(
            vec![100.0, 150.0, 200.0, 150.0, 100.0, 100.0, 200.0, 100.0].into_iter().map(Object::Real).collect(),
        ),
    );
    annot.set("C", Object::Array(vec![Object::Real(1.0), Object::Real(0.0), Object::Real(0.0)]));
    fixture.annot(0, annot);
    let input = dir.path("squiggly.pdf");
    fixture.write(&input);

    let out = dir.path("squiggly-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let drawn = painted(&doc, sheet);
    assert_eq!(drawn.len(), 1);
    let ops = xobject_ops(&doc, sheet, &drawn[0].name);
    let lifts: Vec<f64> = ops
        .lines()
        .filter(|line| {
            let line = line.trim_end();
            line.ends_with(" m") || line.ends_with(" l")
        })
        .filter_map(|line| line.split_whitespace().nth(1)?.parse::<f64>().ok())
        .collect();
    assert!(lifts.len() > 3, "a squiggle has several points: {ops:?}");
    assert!(lifts.iter().any(|y| *y < 101.0), "the low points stay near the baseline: {lifts:?}");
    assert!(lifts.iter().all(|y| (99.0..=103.0).contains(y)), "the band stays at the bottom: {lifts:?}");
}

#[test]
fn a_square_strokes_and_fills() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let mut square = annot("Square", [100.0, 100.0, 200.0, 150.0]);
    square.set("C", Object::Array(vec![Object::Real(0.0), Object::Real(0.0), Object::Real(1.0)]));
    square.set("IC", Object::Array(vec![Object::Real(1.0), Object::Real(1.0), Object::Real(0.0)]));
    fixture.annot(0, square);
    let input = dir.path("square.pdf");
    fixture.write(&input);

    let out = dir.path("square-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let drawn = painted(&doc, sheet);
    assert_eq!(drawn.len(), 1);
    let ops = xobject_ops(&doc, sheet, &drawn[0].name);
    assert!(ops.contains("RG") && ops.contains("rg"), "both colors are set: {ops:?}");
    assert!(ops.contains("100.5 100.5 99 49 re"), "the border is inset by half its width: {ops:?}");
    assert!(ops.ends_with("B\n"), "stroke and fill in one pass: {ops:?}");
}

#[test]
fn a_square_without_a_border_only_fills() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let mut square = annot("Square", [100.0, 100.0, 200.0, 150.0]);
    square.set("IC", Object::Array(vec![Object::Real(0.0), Object::Real(1.0), Object::Real(0.0)]));
    square.set("BS", dictionary! { "W" => 0i64 });
    fixture.annot(0, square);
    let input = dir.path("fill.pdf");
    fixture.write(&input);

    let out = dir.path("fill-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let ops = xobject_ops(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert!(ops.ends_with("f\n"), "no border, so a fill: {ops:?}");
    assert!(!ops.contains(" w\n"), "no line width is set: {ops:?}");
}

#[test]
fn a_circle_is_four_bezier_curves() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let mut circle = annot("Circle", [100.0, 100.0, 200.0, 150.0]);
    circle.set("C", Object::Array(vec![Object::Real(0.0), Object::Real(0.0), Object::Real(0.0)]));
    fixture.annot(0, circle);
    let input = dir.path("circle.pdf");
    fixture.write(&input);

    let out = dir.path("circle-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let ops = xobject_ops(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert_eq!(ops.matches(" c\n").count(), 4, "a circle needs four curves: {ops:?}");
    assert!(ops.contains("199.5 125 m"), "it starts at the right-most point: {ops:?}");
}

#[test]
fn a_line_follows_its_endpoints() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let mut line = annot("Line", [50.0, 50.0, 250.0, 120.0]);
    line.set("L", Object::Array(vec![50.0, 50.0, 250.0, 120.0].into_iter().map(Object::Real).collect()));
    line.set("C", Object::Array(vec![Object::Real(1.0), Object::Real(0.0), Object::Real(0.0)]));
    fixture.annot(0, line);
    let input = dir.path("line.pdf");
    fixture.write(&input);

    let out = dir.path("line-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let ops = xobject_ops(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert!(ops.contains("50 50 m") && ops.contains("250 120 l"), "{ops:?}");
}

#[test]
fn a_line_with_a_leader_is_skipped() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let mut line = annot("Line", [50.0, 50.0, 250.0, 120.0]);
    line.set("L", Object::Array(vec![50.0, 50.0, 250.0, 120.0].into_iter().map(Object::Real).collect()));
    line.set("C", Object::Array(vec![Object::Real(0.0), Object::Real(0.0), Object::Real(0.0)]));
    line.set("LL", Object::Real(10.0));
    fixture.annot(0, line);
    let input = dir.path("leader.pdf");
    fixture.write(&input);

    let out = dir.path("leader-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    assert!(painted(&doc, sheets(&doc)[0].1).is_empty(), "a leader line would move the stroke");
}

#[test]
fn ink_draws_every_stroke() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let mut ink = annot("Ink", [50.0, 50.0, 250.0, 150.0]);
    ink.set(
        "InkList",
        Object::Array(vec![
            Object::Array(vec![50.0, 50.0, 100.0, 100.0, 150.0, 80.0].into_iter().map(Object::Real).collect()),
            Object::Array(vec![200.0, 50.0, 250.0, 150.0].into_iter().map(Object::Real).collect()),
        ]),
    );
    ink.set("C", Object::Array(vec![Object::Real(0.0), Object::Real(0.0), Object::Real(0.0)]));
    fixture.annot(0, ink);
    let input = dir.path("ink.pdf");
    fixture.write(&input);

    let out = dir.path("ink-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let ops = xobject_ops(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert_eq!(ops.matches(" m\n").count(), 2, "two strokes: {ops:?}");
    assert!(ops.contains("1 J") && ops.contains("1 j"), "ink has round caps: {ops:?}");
}

#[test]
fn an_annotation_with_a_broken_appearance_is_not_synthesized() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let ap = fixture.add_object(appearance([0.0, 0.0, 100.0, 50.0], None));
    let mut square = markup("Square", [100.0, 100.0, 200.0, 150.0], Some(4), ap);
    square.set("AP", dictionary! { "N" => Object::Integer(5) });
    square.set("C", Object::Array(vec![Object::Real(0.0), Object::Real(0.0), Object::Real(0.0)]));
    fixture.annot(0, square);
    let input = dir.path("broken-ap.pdf");
    fixture.write(&input);

    let out = dir.path("broken-ap-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    assert!(painted(&doc, sheets(&doc)[0].1).is_empty(), "a broken /AP means nothing to draw");
}

#[test]
fn malformed_geometry_skips_only_that_annotation() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    // QuadPoints with seven entries.
    let mut broken = annot("Highlight", [100.0, 100.0, 200.0, 150.0]);
    broken.set("C", Object::Array(vec![Object::Real(1.0), Object::Real(0.0), Object::Real(0.0)]));
    broken.set("QuadPoints", Object::Array(vec![1.0; 7].into_iter().map(Object::Real).collect()));
    fixture.annot(0, broken);
    // A color array of the wrong length.
    let mut bad_colour = annot("Square", [250.0, 100.0, 350.0, 150.0]);
    bad_colour.set("C", Object::Array(vec![Object::Real(1.0), Object::Real(0.0)]));
    fixture.annot(0, bad_colour);
    // A sound neighbour.
    let mut good = annot("Square", [400.0, 100.0, 500.0, 150.0]);
    good.set("C", Object::Array(vec![Object::Real(0.0), Object::Real(0.0), Object::Real(0.0)]));
    fixture.annot(0, good);
    let input = dir.path("malformed.pdf");
    fixture.write(&input);

    let out = dir.path("malformed-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    assert_eq!(painted(&doc, sheets(&doc)[0].1).len(), 1, "only the sound annotation is drawn");
}

#[test]
fn hidden_and_unprinted_annotations_are_not_synthesized() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let mut hidden = annot("Square", [100.0, 100.0, 200.0, 150.0]);
    hidden.set("F", 2i64);
    hidden.set("C", Object::Array(vec![Object::Real(0.0), Object::Real(0.0), Object::Real(0.0)]));
    fixture.annot(0, hidden);
    let mut unprinted = annot("Square", [300.0, 100.0, 400.0, 150.0]);
    unprinted.set("F", 0i64);
    unprinted.set("C", Object::Array(vec![Object::Real(0.0), Object::Real(0.0), Object::Real(0.0)]));
    fixture.annot(0, unprinted);
    let input = dir.path("flags.pdf");
    fixture.write(&input);

    let out = dir.path("flags-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    assert!(painted(&doc, sheets(&doc)[0].1).is_empty());
}

/// The synthesized operators for a square on a page with `rotate` degrees.
fn square_ops(rotate: i64) -> String {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&[PageSpec { rotate, ..PageSpec::default() }]);
    let mut square = annot("Square", [100.0, 100.0, 200.0, 150.0]);
    square.set("C", Object::Array(vec![Object::Real(0.0), Object::Real(0.0), Object::Real(0.0)]));
    fixture.annot(0, square);
    let input = dir.path("case.pdf");
    fixture.write(&input);
    let out = dir.path("case-out.pdf");
    impose(&input, &out, &NupOptions { pages: vec![1], ..Default::default() });
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    xobject_ops(&doc, sheet, &painted(&doc, sheet)[0].name)
}

#[test]
fn a_rotated_page_keeps_the_same_operators() {
    assert_eq!(square_ops(0), square_ops(90), "the appearance is in page space; the rotation lives in the block");
}
