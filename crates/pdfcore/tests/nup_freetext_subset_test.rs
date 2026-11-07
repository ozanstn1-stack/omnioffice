//! FreeText notes whose text the standard font cannot carry: the bundled face
//! through its subset, and the note that no face can show.

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

/// A `FreeText` whose `/Contents` is a UTF-16BE text string, the way a producer
/// writes text outside WinAnsi (a literal byte string would be read as PDFDoc
/// bytes instead).
fn freetext_utf16(rect: [f64; 4], contents: &str, da: &str) -> Dictionary {
    let mut bytes = vec![0xFE, 0xFF];
    for unit in contents.encode_utf16() {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    let mut annot = freetext(rect, "", da);
    annot.set("Contents", Object::String(bytes, lopdf::StringFormat::Literal));
    annot
}

/// A `FreeText` note without `/AP`, so the synthesizer has to run. `contents`
/// is a plain ASCII byte string; use [`freetext_utf16`] for anything else.
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

#[test]
fn a_note_winansi_cannot_carry_uses_the_subset_font() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    fixture.annot(0, freetext_utf16([100.0, 100.0, 300.0, 200.0], "Привет мир", "/Helv 12 Tf 0 g"));
    let input = dir.path("cyrillic.pdf");
    fixture.write(&input);
    let out = dir.path("cyrillic-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let drawn = painted(&doc, sheet);
    assert_eq!(drawn.len(), 1, "the note is synthesized");
    let ops = ops_of(&doc, sheet, &drawn[0].name);
    assert!(ops.contains("BT\n/OmniTr 12 Tf\n"), "the bundled face is referenced: {ops:?}");
    assert!(ops.contains(" Tj\n"), "the text is shown: {ops:?}");
    assert!(!ops.contains("/FOmni0"), "and not through the standard font: {ops:?}");

    let fonts = doc
        .objects
        .values()
        .filter_map(|object| object.as_dict().ok())
        .filter(|dict| dict.get(b"Subtype").and_then(|value| value.as_name()).ok() == Some(b"Type0"))
        .count();
    assert_eq!(fonts, 1, "one subset font for the note");
}

#[test]
fn a_note_no_face_can_show_is_dropped() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    // CJK has no glyph in the bundled face either: the note is left out
    // entirely rather than printed with replacement glyphs.
    fixture.annot(0, freetext_utf16([100.0, 100.0, 300.0, 200.0], "報告書", "/Helv 12 Tf 0 g"));
    let input = dir.path("cjk.pdf");
    fixture.write(&input);
    let out = dir.path("cjk-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    assert!(painted(&doc, sheet).is_empty(), "nothing is painted for the note");
}

#[test]
fn a_lone_unshowable_note_paints_nothing() {
    let dir = TestDir::new();
    // CJK has no glyph in the bundled face either: the note is left out
    // entirely rather than printed with replacement glyphs, and the note beside
    // it still paints.
    let mut fixture = Fixture::new(&page_specs(2));
    fixture.annot(0, freetext_utf16([100.0, 100.0, 300.0, 200.0], "漢字", "/Helv 12 Tf 0 g"));
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
