//! Printable annotations are painted onto N-up sheets instead of vanishing.

#[path = "common/link_fixtures.rs"]
mod fixtures;
#[path = "common/markup_fixtures.rs"]
mod markup_fixtures;

use fixtures::{
    impose, link, page_content, paint_matrices, sheets, uri_action, with_action, Fixture, PageSpec, TestDir,
};
use lopdf::{Document, Object, ObjectId};
use markup_fixtures::{
    appearance, blocks, close, graphics_state, link_rect, map_rect, markup, number, state_dictionary, xobject,
};
use pdfcore::nup::NupOptions;

const TOLERANCE: f64 = 0.05;

fn page_specs(count: usize) -> Vec<PageSpec> {
    (0..count).map(|_| PageSpec::default()).collect()
}

/// The paint blocks the flattening pass appended (names start with `AnX`).
fn painted(doc: &Document, sheet: ObjectId) -> Vec<markup_fixtures::Block> {
    blocks(&page_content(doc, sheet)).into_iter().filter(|block| block.name.starts_with("AnX")).collect()
}

fn contents_is_single(doc: &Document, sheet: ObjectId) -> bool {
    matches!(doc.get_dictionary(sheet).unwrap().get(b"Contents"), Ok(Object::Reference(_)))
}

#[test]
fn a_printable_square_is_painted_on_the_sheet() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let rect = [100.0, 100.0, 250.0, 250.0];
    let ap = fixture.add_object(appearance([0.0, 0.0, 100.0, 50.0], None));
    fixture.annot(0, markup("Square", rect, Some(4), ap));
    // A link over the same rectangle is carried by the established pass, so its
    // mapped rectangle is an independent expectation for the geometry.
    fixture.annot(0, with_action(link(rect), uri_action("https://example.com/markup")));
    let input = dir.path("square.pdf");
    fixture.write(&input);

    let out = dir.path("square-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let drawn = painted(&doc, sheet);
    assert_eq!(drawn.len(), 1, "one printable annotation is painted");
    let referenced = xobject(&doc, sheet, &drawn[0].name).expect("the drawn XObject");
    let stream = doc.get_object(referenced).and_then(|object| object.as_stream()).expect("a form XObject");
    let bbox = stream.dict.get(b"BBox").expect("bbox").as_array().expect("bbox array");
    assert_eq!(bbox.len(), 4, "the appearance keeps its own box");
    assert_eq!(stream.dict.get(b"Subtype").unwrap().as_name().unwrap(), b"Form");
    let _ = ap;

    let expected = link_rect(&doc, sheet).expect("the link rectangle");
    let boxed = map_rect(drawn[0].cm, [0.0, 0.0, 100.0, 50.0]);
    assert!(close(boxed, expected, TOLERANCE), "painted box {boxed:?} != link rect {expected:?}");
}

#[test]
fn an_annotation_without_the_print_flag_stays_behind() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let ap = fixture.add_object(appearance([0.0, 0.0, 100.0, 50.0], None));
    fixture.annot(0, markup("Square", [100.0, 100.0, 200.0, 200.0], Some(0), ap));
    fixture.annot(0, markup("Square", [100.0, 300.0, 200.0, 400.0], None, ap));
    let input = dir.path("flags.pdf");
    fixture.write(&input);

    let out = dir.path("flags-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    assert!(painted(&doc, sheet).is_empty(), "nothing may be painted");
    assert!(contents_is_single(&doc, sheet), "the sheet content stream is untouched");
}

#[test]
fn hidden_is_dropped_while_noview_and_invisible_are_printed() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let ap = fixture.add_object(appearance([0.0, 0.0, 100.0, 50.0], None));
    // Print + Hidden, Print + NoView, Print + Invisible.
    fixture.annot(0, markup("Square", [100.0, 100.0, 200.0, 200.0], Some(6), ap));
    fixture.annot(0, markup("Square", [100.0, 250.0, 200.0, 350.0], Some(36), ap));
    fixture.annot(0, markup("Square", [100.0, 400.0, 200.0, 500.0], Some(5), ap));
    let input = dir.path("hidden.pdf");
    fixture.write(&input);

    let out = dir.path("hidden-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    assert_eq!(painted(&doc, sheet).len(), 2, "Hidden loses, the other two print");
}

#[test]
fn popups_and_links_are_not_flattened() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let ap = fixture.add_object(appearance([0.0, 0.0, 100.0, 50.0], None));
    fixture.annot(0, markup("Popup", [100.0, 250.0, 200.0, 350.0], Some(4), ap));
    fixture.annot(0, with_action(link([100.0, 400.0, 200.0, 500.0]), uri_action("https://example.com/only-link")));
    let input = dir.path("others.pdf");
    fixture.write(&input);

    let out = dir.path("others-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    assert!(painted(&doc, sheet).is_empty(), "a popup is a screen hint, not paper");
    assert!(link_rect(&doc, sheet).is_some(), "the link is still carried as an annotation");
}

#[test]
fn an_offset_box_with_a_matrix_still_lands_on_the_rectangle() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let rect = [120.0, 160.0, 320.0, 460.0];
    let ap = fixture.add_object(appearance([10.0, 10.0, 60.0, 110.0], Some([0.0, 1.0, -1.0, 0.0, 50.0, 0.0])));
    fixture.annot(0, markup("Square", rect, Some(4), ap));
    fixture.annot(0, with_action(link(rect), uri_action("https://example.com/matrix")));
    let input = dir.path("matrix.pdf");
    fixture.write(&input);

    let out = dir.path("matrix-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let drawn = painted(&doc, sheet);
    assert_eq!(drawn.len(), 1);
    let expected = link_rect(&doc, sheet).expect("link rect");
    // `Do` applies the appearance matrix itself, so the visible box is the
    // transformed one before the placement takes it to the sheet.
    let boxed = map_rect([0.0, 1.0, -1.0, 0.0, 50.0, 0.0], [10.0, 10.0, 60.0, 110.0]);
    let on_sheet = map_rect(drawn[0].cm, boxed);
    assert!(close(on_sheet, expected, TOLERANCE), "on sheet {on_sheet:?} != {expected:?}");
}

#[test]
fn a_rotated_page_carries_the_markup_with_it() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&[PageSpec { rotate: 90, ..PageSpec::default() }]);
    let rect = [80.0, 120.0, 280.0, 320.0];
    let ap = fixture.add_object(appearance([0.0, 0.0, 100.0, 100.0], None));
    fixture.annot(0, markup("Ink", rect, Some(4), ap));
    fixture.annot(0, with_action(link(rect), uri_action("https://example.com/rotated")));
    let input = dir.path("rotated.pdf");
    fixture.write(&input);

    let out = dir.path("rotated-out.pdf");
    impose(&input, &out, &NupOptions { pages: vec![1], ..Default::default() });
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let drawn = painted(&doc, sheet);
    assert_eq!(drawn.len(), 1);
    assert!(drawn[0].cm[1].abs() > 0.1 && drawn[0].cm[2].abs() > 0.1, "a quarter turn shows: {:?}", drawn[0].cm);
    let expected = link_rect(&doc, sheet).expect("link rect");
    let boxed = map_rect(drawn[0].cm, [0.0, 0.0, 100.0, 100.0]);
    assert!(close(boxed, expected, TOLERANCE), "boxed {boxed:?} != {expected:?}");
}

#[test]
fn norotate_keeps_the_markup_upright() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&[PageSpec { rotate: 90, ..PageSpec::default() }]);
    let ap = fixture.add_object(appearance([0.0, 0.0, 100.0, 100.0], None));
    // Print + NoRotate.
    fixture.annot(0, markup("Text", [80.0, 120.0, 280.0, 320.0], Some(20), ap));
    let input = dir.path("norotate.pdf");
    fixture.write(&input);

    let out = dir.path("norotate-out.pdf");
    impose(&input, &out, &NupOptions { pages: vec![1], ..Default::default() });
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let drawn = painted(&doc, sheet);
    assert_eq!(drawn.len(), 1);
    let cm = drawn[0].cm;
    assert!(cm[1].abs() < 1e-4 && cm[2].abs() < 1e-4, "the appearance must stay upright: {cm:?}");
    assert!(cm[0] > 0.0 && cm[3] > 0.0, "and unmirrored: {cm:?}");
}

#[test]
fn the_appearance_state_is_chosen_by_as() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let off = fixture.add_object(markup_fixtures::form(b"0 0 10 10 re S", [0.0, 0.0, 100.0, 50.0], None));
    let on = fixture.add_object(markup_fixtures::form(b"5 5 90 40 re S", [0.0, 0.0, 100.0, 50.0], None));
    let states = state_dictionary(&mut fixture, &[("Off", off), ("On", on)]);
    let mut annot = markup("Square", [100.0, 100.0, 200.0, 200.0], Some(4), off);
    annot.set("AP", lopdf::dictionary! { "N" => Object::Reference(states) });
    annot.set("AS", Object::Name(b"On".to_vec()));
    fixture.annot(0, annot);
    let input = dir.path("states.pdf");
    fixture.write(&input);

    let out = dir.path("states-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let drawn = painted(&doc, sheet);
    assert_eq!(drawn.len(), 1);
    let referenced = xobject(&doc, sheet, &drawn[0].name).expect("drawn XObject");
    let stream = doc.get_object(referenced).and_then(|object| object.as_stream()).expect("stream");
    assert_eq!(stream.content, b"5 5 90 40 re S", "the /AS state is the one drawn");
}

#[test]
fn annotation_opacity_becomes_a_graphics_state() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let ap = fixture.add_object(appearance([0.0, 0.0, 100.0, 50.0], None));
    let mut translucent = markup("Highlight", [100.0, 100.0, 200.0, 150.0], Some(4), ap);
    translucent.set("CA", Object::Real(0.4));
    fixture.annot(0, translucent);
    fixture.annot(0, markup("Highlight", [100.0, 300.0, 200.0, 350.0], Some(4), ap));
    let input = dir.path("opacity.pdf");
    fixture.write(&input);

    let out = dir.path("opacity-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let drawn = painted(&doc, sheet);
    assert_eq!(drawn.len(), 2);
    let faded = drawn.iter().find(|block| block.graphics_state.is_some()).expect("one translucent block");
    let state = graphics_state(&doc, sheet, faded.graphics_state.as_deref().unwrap()).expect("graphics state");
    assert!((number(state.get(b"CA").unwrap()).unwrap() - 0.4).abs() < 1e-3);
    assert!((number(state.get(b"ca").unwrap()).unwrap() - 0.4).abs() < 1e-3);
    assert!(drawn.iter().any(|block| block.graphics_state.is_none()), "the opaque one needs no state");
}

#[test]
fn a_booklet_paints_the_markup_on_the_sheet_that_carries_its_page() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(4));
    let ap = fixture.add_object(appearance([0.0, 0.0, 100.0, 50.0], None));
    // Saddle-stitch order is [4, 1, 2, 3]: page 3 is the second slot of the
    // second sheet.
    fixture.annot(2, markup("Stamp", [50.0, 50.0, 150.0, 150.0], Some(4), ap));
    let input = dir.path("booklet.pdf");
    fixture.write(&input);

    let out = dir.path("booklet-out.pdf");
    impose(&input, &out, &NupOptions { booklet: true, ..Default::default() });
    let doc = Document::load(&out).expect("load output");
    let sheet_pages = sheets(&doc);
    assert_eq!(sheet_pages.len(), 2);
    assert!(painted(&doc, sheet_pages[0].1).is_empty(), "the first sheet has no markup");
    let drawn = painted(&doc, sheet_pages[1].1);
    assert_eq!(drawn.len(), 1);
    let matrices = paint_matrices(&page_content(&doc, sheet_pages[1].1));
    let slot = map_rect(matrices[1], [0.0, 0.0, 600.0, 800.0]);
    let boxed = map_rect(drawn[0].cm, [0.0, 0.0, 100.0, 50.0]);
    assert!(
        boxed[0] >= slot[0] - TOLERANCE && boxed[2] <= slot[2] + TOLERANCE,
        "the block sits in the second slot of the second sheet: {boxed:?} vs {slot:?}"
    );
}
