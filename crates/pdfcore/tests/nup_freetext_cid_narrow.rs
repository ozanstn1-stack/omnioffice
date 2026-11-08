//! FreeText notes in a document font whose embedded CMap mixes code widths.

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

/// A note whose text is UTF-16BE, as a producer writes CJK.
fn cjk_freetext(rect: [f64; 4], text: &str, da: &str) -> Dictionary {
    let mut units = vec![0xFE, 0xFF];
    for unit in text.encode_utf16() {
        units.extend_from_slice(&unit.to_be_bytes());
    }
    let mut annot = freetext(rect, "x", da);
    annot.set("Contents", Object::String(units, lopdf::StringFormat::Literal));
    annot
}

fn painted(doc: &Document, sheet: ObjectId) -> Vec<markup_fixtures::Block> {
    blocks(&page_content(doc, sheet)).into_iter().filter(|block| block.name.starts_with("AnX")).collect()
}

fn stream_of<'a>(doc: &'a Document, sheet: ObjectId, name: &str) -> &'a lopdf::Stream {
    let id = xobject(doc, sheet, name).expect("drawn XObject");
    doc.get_object(id).and_then(|object| object.as_stream()).expect("stream")
}

fn ops_of(doc: &Document, sheet: ObjectId, name: &str) -> String {
    let stream = stream_of(doc, sheet, name);
    let bytes = stream.decompressed_content().unwrap_or_else(|_| stream.content.clone());
    String::from_utf8_lossy(&bytes).to_string()
}

fn operands(doc: &Document, sheet: ObjectId, name: &str) -> String {
    ops_of(doc, sheet, name).split_whitespace().filter(|token| token.starts_with('<') && token.ends_with('>')).collect()
}

/// `/ToUnicode` for the fixture: `中` is code `<41>`, `丁` is `<8140>`, and the
/// space is `<20>`, each at the width the encoding declares.
fn to_unicode() -> String {
    "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
     1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
     3 beginbfchar\n<20> <0020>\n<41> <4E2D>\n<8140> <4E01>\nendbfchar\n\
     endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n"
        .to_string()
}

/// One-byte ASCII plus two-byte leads: the Shift-JIS shape.
fn mixed_cmap() -> String {
    "begincmap\n2 begincodespacerange\n<20> <7E>\n<8140> <9FFC>\nendcodespacerange\n\
     1 begincidrange\n<20> <7E> 600\nendcidrange\n\
     1 begincidrange\n<8140> <9FFC> 700\nendcidrange\n\
     endcmap\n"
        .to_string()
}

/// A one-byte range that overlaps the lead byte of a two-byte one: refused.
fn overlapping_cmap() -> String {
    "begincmap\n2 begincodespacerange\n<00> <FF>\n<8140> <9FFC>\nendcodespacerange\n\
     1 begincidrange\n<20> <7E> 600\nendcidrange\n\
     1 begincidrange\n<8140> <9FFC> 700\nendcidrange\n\
     endcmap\n"
        .to_string()
}

/// `/W` for the two CIDs the mixed fixture uses, plus the space.
fn widths() -> Object {
    Object::Array(vec![
        Object::Integer(600),
        Object::Array(vec![Object::Integer(500)]),
        Object::Integer(633),
        Object::Array(vec![Object::Integer(1000)]),
        Object::Integer(700),
        Object::Array(vec![Object::Integer(1000)]),
    ])
}

fn cid_font(fixture: &mut Fixture, encoding: Object) -> ObjectId {
    let program = fixture.add_object(Object::Stream(lopdf::Stream::new(Dictionary::new(), vec![0u8; 32])));
    let map = fixture.add_object(Object::Stream(lopdf::Stream::new(Dictionary::new(), to_unicode().into_bytes())));
    let descriptor = fixture.add_object(Object::Dictionary(dictionary! {
        "Type" => "FontDescriptor",
        "Ascent" => 880i64,
        "FontFile2" => Object::Reference(program),
    }));
    let child = fixture.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "CIDFontType2",
        "BaseFont" => Object::Name(b"Test-RKSJ".to_vec()),
        "FontDescriptor" => Object::Reference(descriptor),
        "DW" => 0i64,
        "W" => widths(),
    }));
    fixture.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => Object::Name(b"Test-RKSJ".to_vec()),
        "Encoding" => encoding,
        "DescendantFonts" => Object::Array(vec![Object::Reference(child)]),
        "ToUnicode" => Object::Reference(map),
    }))
}

fn cmap_stream(fixture: &mut Fixture, body: String) -> ObjectId {
    fixture.add_object(Object::Stream(lopdf::Stream::new(Dictionary::new(), body.into_bytes())))
}

fn install_dr(fixture: &mut Fixture, key: &str, font: ObjectId) {
    let mut fonts = Dictionary::new();
    fonts.set(key, Object::Reference(font));
    let form = dictionary! {
        "DA" => Object::string_literal(format!("/{key} 12 Tf 0 g").as_str()),
        "DR" => dictionary! { "Font" => Object::Dictionary(fonts) },
    };
    fixture.patch(fixture.catalog_id(), "AcroForm", Object::Dictionary(form));
}

/// Imposes one annotated page and returns the operations of its painted note.
fn note_ops(build: impl FnOnce(&mut Fixture), dir: &TestDir, name: &str) -> String {
    let mut fixture = Fixture::new(&page_specs(2));
    build(&mut fixture);
    let input = dir.path(&format!("{name}.pdf"));
    fixture.write(&input);
    let out = dir.path(&format!("{name}-out.pdf"));
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let drawn = painted(&doc, sheet);
    assert_eq!(drawn.len(), 1, "{name}: exactly one note is painted");
    ops_of(&doc, sheet, &drawn[0].name)
}

#[test]
fn a_mixed_codespace_writes_each_code_at_its_length() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let encoding = cmap_stream(&mut fixture, mixed_cmap());
    let font = cid_font(&mut fixture, Object::Reference(encoding));
    install_dr(&mut fixture, "F1", font);
    // `中` is one byte, `丁` is two: the same note carries both widths.
    fixture.annot(0, cjk_freetext([100.0, 100.0, 400.0, 200.0], "中丁", "/F1 12 Tf 0 g"));
    let input = dir.path("mixed.pdf");
    fixture.write(&input);

    let out = dir.path("mixed-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let drawn = painted(&doc, sheet);
    assert_eq!(drawn.len(), 1);
    let ops = ops_of(&doc, sheet, &drawn[0].name);
    assert!(ops.contains("/F1 12 Tf"), "the document's font is used: {ops:?}");
    assert_eq!(operands(&doc, sheet, &drawn[0].name), "<418140>", "one byte, then two");
    assert!(!ops.contains(" Tw"), "word spacing would apply to the wrong byte");
}

#[test]
fn a_one_byte_space_is_not_word_spacing() {
    let dir = TestDir::new();
    // The space is code `<20>`; a viewer that applies `Tw` to the one-byte code
    // 32 would widen the gap, and the note must not ask for it.
    let ops = note_ops(
        |fixture| {
            let encoding = cmap_stream(fixture, mixed_cmap());
            let font = cid_font(fixture, Object::Reference(encoding));
            install_dr(fixture, "F1", font);
            fixture.annot(0, cjk_freetext([100.0, 100.0, 400.0, 200.0], "中 丁", "/F1 12 Tf 0 g"));
        },
        &dir,
        "space",
    );
    assert_eq!(operands_from(&ops), "<41208140>", "the space keeps its one-byte code");
    assert!(!ops.contains(" Tw"));
}

/// The `<hex>` operands of one appearance stream's operations.
fn operands_from(ops: &str) -> String {
    ops.split_whitespace().filter(|token| token.starts_with('<') && token.ends_with('>')).collect()
}

#[test]
fn an_overlapping_codespace_falls_back_byte_identical() {
    let dir = TestDir::new();
    let refused = note_ops(
        |fixture| {
            let encoding = cmap_stream(fixture, overlapping_cmap());
            let font = cid_font(fixture, Object::Reference(encoding));
            install_dr(fixture, "F1", font);
            fixture.annot(0, cjk_freetext([100.0, 100.0, 400.0, 200.0], "中", "/F1 12 Tf 0 g"));
            fixture.annot(0, freetext([100.0, 300.0, 400.0, 400.0], "latin", "/F1 12 Tf 0 g"));
        },
        &dir,
        "overlap",
    );
    let vertical = note_ops(
        |fixture| {
            let encoding = Object::Name(b"Identity-V".to_vec());
            let font = cid_font(&mut *fixture, encoding);
            install_dr(fixture, "F1", font);
            fixture.annot(0, cjk_freetext([100.0, 100.0, 400.0, 200.0], "中", "/F1 12 Tf 0 g"));
            fixture.annot(0, freetext([100.0, 300.0, 400.0, 400.0], "latin", "/F1 12 Tf 0 g"));
        },
        &dir,
        "vertical",
    );
    assert_eq!(refused, vertical, "a refused encoding leaves the same output as a vertical one");
    assert!(!refused.contains("/F1 "), "the font is not referenced: {refused:?}");
}
