//! Line annotation captions (`/Cap`, `/CP`, `/CO`) in synthesized appearances:
//! a Line with no appearance stream keeps the text the document gave it.

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

fn line(contents: &str, extra: Vec<(&str, Object)>) -> Dictionary {
    line_in([100.0, 100.0, 200.0, 130.0], contents, extra)
}

fn line_in(rect: [f64; 4], contents: &str, extra: Vec<(&str, Object)>) -> Dictionary {
    let mut annot = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Line",
        "Rect" => rect_array(rect),
        "F" => 4i64,
        "C" => numbers(&[0.0, 0.0, 1.0]),
        "L" => numbers(&[100.0, 110.0, 200.0, 110.0]),
        "Contents" => Object::string_literal(contents),
    };
    for (key, value) in extra {
        annot.set(key.as_bytes().to_vec(), value);
    }
    annot
}

fn painted(doc: &Document, sheet: ObjectId) -> Vec<markup_fixtures::Block> {
    blocks(&page_content(doc, sheet)).into_iter().filter(|block| block.name.starts_with("AnX")).collect()
}

fn xobject_of(doc: &Document, sheet: ObjectId, name: &str) -> (String, [f64; 4]) {
    let id = xobject(doc, sheet, name).expect("drawn XObject");
    let stream = doc.get_object(id).and_then(|object| object.as_stream()).expect("stream");
    let bytes = stream.decompressed_content().unwrap_or_else(|_| stream.content.clone());
    let bbox = stream.dict.get(b"BBox").and_then(|value| value.as_array()).expect("bbox");
    let read = |index: usize| f64::from(bbox[index].as_float().expect("number"));
    (String::from_utf8_lossy(&bytes).to_string(), [read(0), read(1), read(2), read(3)])
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

fn ops_of(doc: &Document, sheet: ObjectId, name: &str) -> String {
    xobject_of(doc, sheet, name).0
}

#[test]
fn a_captioned_line_prints_its_text() {
    let dir = TestDir::new();
    let (doc, sheet) = render(line("Hello", vec![("Cap", true.into())]), &dir, "caption");
    let ops = ops_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert!(ops.contains("/FOmni0 9 Tf"), "the standard font is referenced: {ops:?}");
    assert!(ops.contains("<48656C6C6F> Tj"), "the text is drawn: {ops:?}");
    assert!(ops.contains("BT\n") && ops.contains("ET\n"), "inside a text object: {ops:?}");
    // The stroke is broken around the caption, so the one line becomes two.
    assert_eq!(ops.matches(" m\n").count(), 2, "two stroke segments: {ops:?}");
}

#[test]
fn an_inline_caption_is_not_printed_without_the_flag() {
    let dir = TestDir::new();
    let (doc, sheet) = render(line("Hello", vec![]), &dir, "no-caption");
    let ops = ops_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert!(!ops.contains("Tj"), "no text without /Cap: {ops:?}");
    assert!(!ops.contains("/FOmni0"), "and no font resource either: {ops:?}");
    assert_eq!(ops.matches(" m\n").count(), 1, "the line is drawn in one piece: {ops:?}");
}

#[test]
fn a_top_caption_appears_above_the_line_and_the_box_grows() {
    let dir = TestDir::new();
    let (doc, sheet) = render(
        line_in(
            [100.0, 100.0, 200.0, 112.0],
            "Hello",
            vec![("Cap", true.into()), ("CP", Object::Name(b"Top".to_vec()))],
        ),
        &dir,
        "caption-top",
    );
    let name = &painted(&doc, sheet)[0].name.clone();
    let (ops, bbox) = xobject_of(&doc, sheet, name);
    assert!(ops.contains("<48656C6C6F> Tj"), "{ops:?}");
    assert!(bbox[3] > 112.0, "the box grew above the annotation's own rectangle: {bbox:?}");
    assert_eq!(ops.matches(" m\n").count(), 1, "a caption above the line does not break it: {ops:?}");
}

#[test]
fn a_caption_the_font_cannot_show_leaves_the_line_alone() {
    let dir = TestDir::new();
    let (doc, sheet) = render(line("報告", vec![("Cap", true.into())]), &dir, "caption-cjk");
    let ops = ops_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert!(!ops.contains("Tj"), "no replacement glyphs: {ops:?}");
    assert_eq!(ops.matches(" m\n").count(), 1, "and the line is not broken: {ops:?}");
}

/// Every Type0 font the output document carries.
fn type0_fonts(doc: &Document) -> Vec<&lopdf::Dictionary> {
    doc.objects
        .values()
        .filter_map(|object| object.as_dict().ok())
        .filter(|dict| dict.get(b"Subtype").and_then(|value| value.as_name()).ok() == Some(b"Type0"))
        .collect()
}

#[test]
fn a_caption_winansi_cannot_show_is_set_in_a_subset_font() {
    let dir = TestDir::new();
    // Cyrillic: no WinAnsi code, no /Differences glyph, but the bundled face
    // carries it, so the caption is drawn instead of being dropped.
    let (doc, sheet) = render(line("Привет", vec![("Cap", true.into())]), &dir, "caption-cyrillic");
    let ops = ops_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert!(ops.contains("/OmniTr 9 Tf"), "the subset font is referenced: {ops:?}");
    assert!(ops.contains(" Tj"), "the text is drawn: {ops:?}");
    assert!(!ops.contains("/FOmni0"), "and not through the standard font: {ops:?}");

    // The font the form points at is a Type0 one with an identity encoding and
    // a ToUnicode map, written once for the characters that are drawn.
    let fonts = type0_fonts(&doc);
    assert_eq!(fonts.len(), 1, "one subset font for the sheet");
    let font = fonts[0];
    assert_eq!(font.get(b"Encoding").and_then(|value| value.as_name()).ok(), Some(b"Identity-H".as_slice()));
    assert!(font.get(b"ToUnicode").is_ok(), "the map back to text is written");
    assert!(font.get(b"DescendantFonts").is_ok());

    // Deterministic: the same input gives byte-identical output.
    let dir_two = TestDir::new();
    let (doc_two, sheet_two) = render(line("Привет", vec![("Cap", true.into())]), &dir_two, "caption-cyrillic");
    assert_eq!(ops, ops_of(&doc_two, sheet_two, &painted(&doc_two, sheet_two)[0].name));
    assert_eq!(type0_fonts(&doc_two).len(), 1);
}

#[test]
fn a_western_caption_still_uses_the_standard_font() {
    let dir = TestDir::new();
    let (doc, sheet) = render(line("Hello", vec![("Cap", true.into())]), &dir, "caption-standard");
    let ops = ops_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert!(ops.contains("/FOmni0 9 Tf"), "{ops:?}");
    assert!(type0_fonts(&doc).is_empty(), "no subset font when WinAnsi suffices");
}

#[test]
fn a_caption_follows_the_line_colour() {
    let dir = TestDir::new();
    let (doc, sheet) = render(line("Hello", vec![("Cap", true.into())]), &dir, "caption-colour");
    let ops = ops_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    // /C = [0 0 1] is a blue line; the caption is filled with the same colour.
    let text_block = ops.split("BT\n").nth(1).expect("text object");
    assert!(text_block.contains("0 0 1 rg"), "the caption is filled like the line: {text_block:?}");
}
