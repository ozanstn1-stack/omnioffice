//! FreeText annotations without an appearance stream are laid out and drawn.

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

/// A `FreeText` annotation without `/AP`, so the synthesizer has to run.
fn freetext(rect: [f64; 4], contents: &str, da: &str) -> Dictionary {
    dictionary! {
        "Type" => "Annot",
        "Subtype" => "FreeText",
        "Rect" => rect_array(rect),
        "F" => 4i64,
        "Contents" => Object::string_literal(contents),
        "DA" => Object::string_literal(da),
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

fn font_dict(doc: &Document, sheet: ObjectId, name: &str) -> Dictionary {
    let id = xobject(doc, sheet, name).expect("drawn XObject");
    let stream = doc.get_object(id).and_then(|object| object.as_stream()).expect("stream");
    let resources = stream.dict.get(b"Resources").expect("resources").as_dict().expect("dict");
    let fonts = resources.get(b"Font").expect("fonts").as_dict().expect("dict");
    fonts.get(b"FOmni0").expect("the synthesized font").as_dict().expect("dict").clone()
}

#[test]
fn a_free_text_note_is_laid_out_with_a_standard_font() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    fixture.annot(0, freetext([100.0, 100.0, 300.0, 200.0], "Merhaba dünya ğüşİı", "/Helv 12 Tf 0 g"));
    let input = dir.path("note.pdf");
    fixture.write(&input);

    let out = dir.path("note-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let drawn = painted(&doc, sheet);
    assert_eq!(drawn.len(), 1, "the note is synthesized");
    let ops = ops_of(&doc, sheet, &drawn[0].name);
    assert!(ops.contains("BT\n/FOmni0 12 Tf\n"), "the text block uses the standard font: {ops:?}");
    assert!(ops.contains(" Tj\n"), "the text is shown: {ops:?}");
    assert!(ops.contains(" Tm\n"), "every line is positioned: {ops:?}");

    let font = font_dict(&doc, sheet, &drawn[0].name);
    assert_eq!(font.get(b"BaseFont").unwrap().as_name().unwrap(), b"Helvetica");
    let encoding = font.get(b"Encoding").unwrap().as_dict().unwrap();
    assert_eq!(encoding.get(b"BaseEncoding").unwrap().as_name().unwrap(), b"WinAnsiEncoding");
    let differences = encoding.get(b"Differences").unwrap().as_array().unwrap();
    assert!(differences.len() > 10, "the Turkish glyphs are declared: {differences:?}");
}

#[test]
fn an_annotation_with_an_appearance_is_left_alone() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let ap = fixture.add_object(markup_fixtures::appearance([0.0, 0.0, 100.0, 50.0], None));
    let mut annot = freetext([100.0, 100.0, 300.0, 200.0], "already has one", "/Helv 12 Tf 0 g");
    annot.set("AP", dictionary! { "N" => Object::Reference(ap) });
    fixture.annot(0, annot);
    let input = dir.path("has-ap.pdf");
    fixture.write(&input);

    let out = dir.path("has-ap-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let drawn = painted(&doc, sheet);
    assert_eq!(drawn.len(), 1, "the existing appearance is drawn");
    let ops = ops_of(&doc, sheet, &drawn[0].name);
    assert!(!ops.contains("BT\n"), "nothing is re-synthesized");
}

#[test]
fn right_alignment_puts_the_text_at_the_right_edge() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let mut annot = freetext([100.0, 100.0, 300.0, 200.0], "hi", "/Helv 12 Tf 0 g");
    annot.set("Q", Object::Integer(2));
    fixture.annot(0, annot);
    let input = dir.path("right.pdf");
    fixture.write(&input);

    let out = dir.path("right-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let ops = ops_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    let mut x = None;
    for line in ops.lines() {
        if let Some(rest) = line.strip_suffix(" Tm") {
            let values: Vec<f64> = rest.split_whitespace().filter_map(|v| v.parse().ok()).collect();
            if values.len() == 6 {
                x = Some(values[4]);
            }
        }
    }
    let x = x.expect("a text matrix");
    assert!(x > 250.0, "a right-aligned line starts near the right edge: {x}");
}

#[test]
fn the_background_and_border_come_from_the_annotation() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let mut annot = freetext([100.0, 100.0, 300.0, 200.0], "boxed", "/Helv 10 Tf 0 g");
    annot.set("C", Object::Array(vec![Object::Real(1.0), Object::Real(1.0), Object::Real(0.0)]));
    fixture.annot(0, annot);
    let mut plain = freetext([350.0, 100.0, 550.0, 200.0], "plain", "/Helv 10 Tf 0 g");
    plain.set("BS", dictionary! { "W" => 0i64 });
    fixture.annot(0, plain);
    let input = dir.path("box.pdf");
    fixture.write(&input);

    let out = dir.path("box-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let drawn = painted(&doc, sheet);
    assert_eq!(drawn.len(), 2);
    let boxed = ops_of(&doc, sheet, &drawn[0].name);
    assert!(boxed.contains("re f\n"), "a background is filled: {boxed:?}");
    assert!(boxed.contains("re S\n"), "the border is stroked: {boxed:?}");
    let plain = ops_of(&doc, sheet, &drawn[1].name);
    assert!(!plain.contains("re f\n"), "no background");
    assert!(!plain.contains("re S\n"), "no border when the width is zero: {plain:?}");
}

#[test]
fn a_callout_line_follows_cl() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    for (index, points) in
        [vec![100.0, 100.0, 50.0, 40.0], vec![100.0, 100.0, 60.0, 60.0, 40.0, 40.0]].into_iter().enumerate()
    {
        let mut annot = freetext(
            [100.0 + index as f64 * 300.0, 100.0, 300.0 + index as f64 * 300.0, 200.0],
            "pointing",
            "/Helv 10 Tf 0 g",
        );
        annot.set("CL", Object::Array(points.into_iter().map(Object::Real).collect()));
        fixture.annot(0, annot);
    }
    let broken = {
        let mut annot = freetext([300.0, 250.0, 500.0, 350.0], "broken", "/Helv 10 Tf 0 g");
        annot.set("CL", Object::Array(vec![1.0, 2.0, 3.0, 4.0, 5.0].into_iter().map(Object::Real).collect()));
        annot
    };
    fixture.annot(0, broken);
    let input = dir.path("callout.pdf");
    fixture.write(&input);

    let out = dir.path("callout-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let drawn = painted(&doc, sheet);
    assert_eq!(drawn.len(), 3, "all three notes are drawn");
    let four = ops_of(&doc, sheet, &drawn[0].name);
    assert_eq!(four.matches(" l\n").count(), 1, "a four-number callout has one segment: {four:?}");
    let six = ops_of(&doc, sheet, &drawn[1].name);
    assert_eq!(six.matches(" l\n").count(), 2, "a six-number callout has two: {six:?}");
    let broken = ops_of(&doc, sheet, &drawn[2].name);
    assert_eq!(broken.matches(" m\n").count(), 0, "a malformed callout is dropped, the text stays");
    assert!(broken.contains(" Tj\n"));
}

#[test]
fn rectangle_defects_pull_the_text_in() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let mut annot = freetext([100.0, 100.0, 300.0, 200.0], "inset", "/Helv 12 Tf 0 g");
    annot.set("RD", Object::Array(vec![10.0, 20.0, 10.0, 20.0].into_iter().map(Object::Real).collect()));
    fixture.annot(0, annot);
    let input = dir.path("inset.pdf");
    fixture.write(&input);

    let out = dir.path("inset-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let ops = ops_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert!(ops.contains("re W n"), "the text box is clipped: {ops:?}");
    assert!(!ops.contains("100 100 200 100 re"), "the box moved inside: {ops:?}");
}

#[test]
fn a_note_with_an_unrepresentable_character_is_skipped() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    fixture.annot(0, freetext([100.0, 100.0, 300.0, 200.0], "漢字", "/Helv 12 Tf 0 g"));
    fixture.annot(0, freetext([400.0, 100.0, 600.0, 200.0], "latin only", "/Helv 12 Tf 0 g"));
    let input = dir.path("mixed.pdf");
    fixture.write(&input);

    let out = dir.path("mixed-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let drawn = painted(&doc, sheet);
    assert_eq!(drawn.len(), 1, "only the Latin note is drawn");
    assert!(ops_of(&doc, sheet, &drawn[0].name).contains(" Tj\n"));
}

#[test]
fn an_empty_note_still_paints_its_box() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let mut annot = freetext([100.0, 100.0, 300.0, 200.0], "", "/Helv 12 Tf 0 g");
    annot.set("C", Object::Array(vec![Object::Real(1.0), Object::Real(1.0), Object::Real(1.0)]));
    fixture.annot(0, annot);
    let input = dir.path("empty.pdf");
    fixture.write(&input);

    let out = dir.path("empty-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let ops = ops_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert!(ops.contains("re f\n"), "the box is filled");
    assert!(!ops.contains("BT\n"), "there is no text block");
}

#[test]
fn the_form_default_appearance_is_the_fallback() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    fixture.patch(
        fixture.catalog_id(),
        "AcroForm",
        Object::Dictionary(dictionary! {
            "DA" => Object::string_literal("/TiRo 9 Tf 0 0 1 rg"),
        }),
    );
    fixture.annot(0, {
        let mut annot = freetext([100.0, 100.0, 300.0, 200.0], "from the form", "");
        annot.remove(b"DA");
        annot
    });
    let input = dir.path("form-da.pdf");
    fixture.write(&input);

    let out = dir.path("form-da-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let drawn = painted(&doc, sheet);
    assert_eq!(drawn.len(), 1);
    let ops = ops_of(&doc, sheet, &drawn[0].name);
    assert!(ops.contains("/FOmni0 9 Tf"), "the form's size is used: {ops:?}");
    let font = font_dict(&doc, sheet, &drawn[0].name);
    assert_eq!(font.get(b"BaseFont").unwrap().as_name().unwrap(), b"Times-Roman");
}

#[test]
fn the_output_is_deterministic() {
    let dir = TestDir::new();
    let mut first = Fixture::new(&page_specs(2));
    first.annot(0, freetext([100.0, 100.0, 300.0, 200.0], "same input", "/Helv 12 Tf 0 g"));
    let first_input = dir.path("first.pdf");
    first.write(&first_input);
    let first_out = dir.path("first-out.pdf");
    impose(&first_input, &first_out, &NupOptions::default());
    let doc = Document::load(&first_out).expect("load output");
    let a = ops_of(&doc, sheets(&doc)[0].1, &painted(&doc, sheets(&doc)[0].1)[0].name);

    let mut second = Fixture::new(&page_specs(2));
    second.annot(0, freetext([100.0, 100.0, 300.0, 200.0], "same input", "/Helv 12 Tf 0 g"));
    let second_input = dir.path("second.pdf");
    second.write(&second_input);
    let second_out = dir.path("second-out.pdf");
    impose(&second_input, &second_out, &NupOptions::default());
    let doc = Document::load(&second_out).expect("load output");
    let b = ops_of(&doc, sheets(&doc)[0].1, &painted(&doc, sheets(&doc)[0].1)[0].name);
    assert_eq!(a, b, "the same input produces the same operators");
}
