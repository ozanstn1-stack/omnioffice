//! Markup flattening under hostile input: only sound appearances are painted,
//! and a document without printable markup comes out byte-for-byte unchanged.

#[path = "common/link_fixtures.rs"]
mod fixtures;
#[path = "common/markup_fixtures.rs"]
mod markup_fixtures;

use fixtures::{
    annots, impose, link, page_content, rect_of, sheets, uri_action, with_action, Fixture, PageSpec, TestDir,
};
use lopdf::{dictionary, Document, Object, ObjectId, Stream};
use markup_fixtures::{appearance, blocks, close, map_rect, markup, read_rect, xobject};
use pdfcore::nup::NupOptions;

const TOLERANCE: f64 = 0.05;

fn page_specs(count: usize) -> Vec<PageSpec> {
    (0..count).map(|_| PageSpec::default()).collect()
}

fn painted(doc: &Document, sheet: ObjectId) -> Vec<markup_fixtures::Block> {
    blocks(&page_content(doc, sheet)).into_iter().filter(|block| block.name.starts_with("AnX")).collect()
}

#[test]
fn markup_outside_the_artwork_is_clipped_away() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let ap = fixture.add_object(appearance([0.0, 0.0, 100.0, 100.0], None));
    // Sticks out over the right and top edges of the page.
    fixture.annot(0, markup("Square", [500.0, 700.0, 800.0, 1000.0], Some(4), ap));
    // Entirely outside the page box.
    fixture.annot(0, markup("Square", [700.0, 900.0, 800.0, 1000.0], Some(4), ap));
    let input = dir.path("clip.pdf");
    fixture.write(&input);

    let out = dir.path("clip-out.pdf");
    impose(&input, &out, &NupOptions { pages: vec![1], ..Default::default() });
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let drawn = painted(&doc, sheet);
    assert_eq!(drawn.len(), 1, "only the partly visible annotation is painted");
    let clip = drawn[0].clip;
    let sheet_box = rect_of(doc.get_dictionary(sheet).unwrap().get(b"MediaBox").unwrap()).expect("media box");
    assert!(clip[2] - clip[0] > 1.0 && clip[3] - clip[1] > 1.0, "a non-empty clip: {clip:?}");
    assert!(
        clip[0] >= sheet_box[0] - TOLERANCE
            && clip[2] <= sheet_box[2] + TOLERANCE
            && clip[3] <= sheet_box[3] + TOLERANCE,
        "the clip stays inside the sheet: {clip:?} vs {sheet_box:?}"
    );
}

#[test]
fn broken_appearances_are_skipped_one_by_one() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let good = fixture.add_object(appearance([0.0, 0.0, 100.0, 50.0], None));
    // No `/AP` at all.
    fixture.annot(0, {
        let mut annot = markup("Square", [10.0, 10.0, 100.0, 100.0], Some(4), good);
        annot.remove(b"AP");
        annot
    });
    // `/N` is not a stream.
    fixture.annot(0, {
        let mut annot = markup("Square", [20.0, 20.0, 120.0, 120.0], Some(4), good);
        annot.set("AP", dictionary! { "N" => Object::Integer(5) });
        annot
    });
    // No `/BBox`.
    let no_bbox = fixture.add_object(Object::Stream(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Form" },
        b"0 0 10 10 re S".to_vec(),
    )));
    fixture.annot(0, markup("Ink", [30.0, 30.0, 130.0, 130.0], Some(4), no_bbox));
    // A box with no area.
    let flat = fixture.add_object(appearance([0.0, 0.0, 0.0, 50.0], None));
    fixture.annot(0, markup("Ink", [40.0, 40.0, 140.0, 140.0], Some(4), flat));
    // A singular matrix.
    let singular = fixture.add_object(appearance([0.0, 0.0, 100.0, 100.0], Some([1.0, 0.0, 0.0, 0.0, 0.0, 0.0])));
    fixture.annot(0, markup("Line", [50.0, 50.0, 150.0, 150.0], Some(4), singular));
    // A dangling appearance reference.
    fixture.annot(0, {
        let mut annot = markup("Square", [60.0, 60.0, 160.0, 160.0], Some(4), good);
        annot.set("AP", dictionary! { "N" => Object::Reference((9999, 0)) });
        annot
    });
    // A stream that is not a form.
    let image = fixture.add_object(Object::Stream(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 1i64, "Height" => 1i64, "BBox" => Object::Array(vec![0.into(), 0.into(), 10.into(), 10.into()]) },
        b"".to_vec(),
    )));
    fixture.annot(0, markup("Square", [70.0, 70.0, 170.0, 170.0], Some(4), image));
    // And one sound annotation.
    fixture.annot(0, markup("Square", [300.0, 300.0, 400.0, 400.0], Some(4), good));
    let input = dir.path("broken.pdf");
    fixture.write(&input);

    let out = dir.path("broken-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("the imposition survives broken appearances");
    let sheet = sheets(&doc)[0].1;
    assert_eq!(painted(&doc, sheet).len(), 1, "only the sound annotation is painted");
}

#[test]
fn a_link_only_document_is_left_alone() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    fixture.annot(0, with_action(link([100.0, 100.0, 200.0, 200.0]), uri_action("https://example.com/plain")));
    let input = dir.path("plain.pdf");
    fixture.write(&input);

    let out = dir.path("plain-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    assert!(painted(&doc, sheet).is_empty());
    assert!(
        matches!(doc.get_dictionary(sheet).unwrap().get(b"Contents"), Ok(Object::Reference(_))),
        "no second content stream is added"
    );
    let page = doc.get_dictionary(sheet).unwrap();
    let resources = page.get(b"Resources").unwrap().as_reference().unwrap();
    let resources = doc.get_dictionary(resources).unwrap();
    let xobjects = resources.get(b"XObject").unwrap().as_dict().unwrap();
    assert!(xobjects.iter().all(|(name, _)| name.starts_with(b"Fm")), "only the page forms are named");
}

#[test]
fn flattened_markup_survives_a_save_and_reload() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let ap = fixture.add_object(appearance([0.0, 0.0, 100.0, 50.0], None));
    fixture.annot(0, markup("Highlight", [100.0, 100.0, 300.0, 200.0], Some(4), ap));
    let input = dir.path("reload.pdf");
    fixture.write(&input);

    let out = dir.path("reload-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let mut doc = Document::load(&out).expect("load output");
    let again = dir.path("reload-again.pdf");
    doc.save(&again).expect("save again");
    let reloaded = Document::load(&again).expect("reload");
    let sheet = sheets(&reloaded)[0].1;
    let drawn = painted(&reloaded, sheet);
    assert_eq!(drawn.len(), 1, "the draw block survives the renumbering");
    let referenced = xobject(&reloaded, sheet, &drawn[0].name).expect("XObject survives pruning");
    let stream = reloaded.get_object(referenced).and_then(|object| object.as_stream()).expect("stream");
    assert_eq!(read_rect(stream.dict.get(b"BBox").unwrap()).unwrap(), [0.0, 0.0, 100.0, 50.0]);
}

#[test]
fn the_annotation_count_is_capped() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let ap = fixture.add_object(appearance([0.0, 0.0, 10.0, 10.0], None));
    for index in 0..4097 {
        let x = 10.0 + (index % 20) as f64;
        fixture.annot(0, markup("Square", [x, x, x + 5.0, x + 5.0], Some(4), ap));
    }
    let input = dir.path("cap.pdf");
    fixture.write(&input);

    let out = dir.path("cap-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    assert_eq!(painted(&doc, sheet).len(), 4096, "the cap keeps the sheet sane");
}

#[test]
fn an_indirect_annots_array_is_read() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let ap = fixture.add_object(appearance([0.0, 0.0, 100.0, 50.0], None));
    let annot = fixture.add_object(Object::Dictionary(markup("Square", [100.0, 100.0, 200.0, 200.0], Some(4), ap)));
    let array = fixture.add_object(Object::Array(vec![Object::Reference(annot)]));
    fixture.raw_annots(0, Object::Reference(array));
    let input = dir.path("indirect.pdf");
    fixture.write(&input);

    let out = dir.path("indirect-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    assert_eq!(painted(&doc, sheet).len(), 1, "a referenced /Annots array is honoured");
}

#[test]
fn a_translucent_highlight_keeps_its_blend_mode() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let ap = fixture.add_object(appearance([0.0, 0.0, 100.0, 50.0], None));
    let mut annot = markup("Highlight", [100.0, 100.0, 300.0, 200.0], Some(4), ap);
    annot.set("CA", Object::Real(0.5));
    annot.set("ca", Object::Real(0.25));
    annot.set("BM", Object::Name(b"Multiply".to_vec()));
    fixture.annot(0, annot);
    let input = dir.path("blend.pdf");
    fixture.write(&input);

    let out = dir.path("blend-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let drawn = painted(&doc, sheet);
    assert_eq!(drawn.len(), 1);
    let state = markup_fixtures::graphics_state(&doc, sheet, drawn[0].graphics_state.as_deref().unwrap()).unwrap();
    assert_eq!(state.get(b"BM").unwrap().as_name().unwrap(), b"Multiply");
    assert!((markup_fixtures::number(state.get(b"ca").unwrap()).unwrap() - 0.25).abs() < 1e-3);
    let boxed = map_rect(drawn[0].cm, [0.0, 0.0, 100.0, 50.0]);
    let link = annots(&doc, sheet)
        .into_iter()
        .find(|annot| annot.get(b"Subtype").ok().and_then(|value| value.as_name().ok()) == Some(b"Link".as_slice()))
        .and_then(|annot| rect_of(annot.get(b"Rect").ok()?));
    if let Some(link) = link {
        assert!(close(boxed, link, TOLERANCE), "the highlight sits on its rectangle");
    }
}
