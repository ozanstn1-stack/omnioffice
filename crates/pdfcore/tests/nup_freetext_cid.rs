//! FreeText notes reuse a composite (`Type0`) font of the document when neither
//! WinAnsi nor the bundled face can show their text.

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

/// The `Tj` operands of one appearance stream.
fn operands(doc: &Document, sheet: ObjectId, name: &str) -> String {
    let ops = ops_of(doc, sheet, name);
    ops.split_whitespace().filter(|token| token.starts_with('<') && token.ends_with('>')).collect()
}

/// The `/ToUnicode` of a font whose space is CID 3 and whose one ideograph is
/// CID 36.
fn to_unicode(with_ideograph: bool) -> String {
    let ideograph = if with_ideograph { "<0024> <4E2D>\n" } else { "" };
    format!(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
         1 beginbfchar\n<0003> <0020>\nendbfchar\n\
         {count} beginbfchar\n{ideograph}endbfchar\n\
         endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n",
        count = usize::from(with_ideograph),
    )
}

/// A composite font of the document: Identity-H, one embedded CIDFontType2.
fn cid_font(fixture: &mut Fixture, with_ideograph: bool) -> ObjectId {
    let program = fixture.add_object(Object::Stream(lopdf::Stream::new(Dictionary::new(), vec![0u8; 32])));
    let map = fixture
        .add_object(Object::Stream(lopdf::Stream::new(Dictionary::new(), to_unicode(with_ideograph).into_bytes())));
    let descriptor = fixture.add_object(Object::Dictionary(dictionary! {
        "Type" => "FontDescriptor",
        "Ascent" => 880i64,
        "FontFile2" => Object::Reference(program),
    }));
    let child = fixture.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "CIDFontType2",
        "BaseFont" => Object::Name(b"NotoSansCJK".to_vec()),
        "FontDescriptor" => Object::Reference(descriptor),
        "DW" => 1000i64,
        "W" => Object::Array(vec![
            Object::Integer(3),
            Object::Array(vec![Object::Integer(250)]),
        ]),
    }));
    fixture.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => Object::Name(b"NotoSansCJK".to_vec()),
        "Encoding" => "Identity-H",
        "DescendantFonts" => Object::Array(vec![Object::Reference(child)]),
        "ToUnicode" => Object::Reference(map),
    }))
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

#[test]
fn a_cjk_note_is_drawn_in_the_document_font() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let font = cid_font(&mut fixture, true);
    install_dr(&mut fixture, "F1", font);
    fixture.annot(0, cjk_freetext([100.0, 100.0, 300.0, 200.0], "中", "/F1 12 Tf 0 g"));
    let input = dir.path("cid.pdf");
    fixture.write(&input);

    let out = dir.path("cid-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let drawn = painted(&doc, sheet);
    assert_eq!(drawn.len(), 1);
    let ops = ops_of(&doc, sheet, &drawn[0].name);
    assert!(ops.contains("/F1 12 Tf"), "the document's composite font is used: {ops:?}");
    assert!(!ops.contains("/FOmni0"), "no standard font is written: {ops:?}");
    assert_eq!(operands(&doc, sheet, &drawn[0].name), "<0024>", "the CID the file itself used");
}

#[test]
fn a_missing_glyph_keeps_the_note_out() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    // The map does not cover the ideograph, and no other face can show it.
    let font = cid_font(&mut fixture, false);
    install_dr(&mut fixture, "F1", font);
    fixture.annot(0, cjk_freetext([100.0, 100.0, 300.0, 200.0], "中", "/F1 12 Tf 0 g"));
    fixture.annot(0, freetext([350.0, 100.0, 550.0, 200.0], "latin", "/F1 12 Tf 0 g"));
    let input = dir.path("no-glyph.pdf");
    fixture.write(&input);

    let out = dir.path("no-glyph-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    assert_eq!(painted(&doc, sheets(&doc)[0].1).len(), 1, "only the Latin note survives");
}

#[test]
fn latin_text_still_uses_winansi() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let font = cid_font(&mut fixture, true);
    install_dr(&mut fixture, "F1", font);
    fixture.annot(0, freetext([100.0, 100.0, 300.0, 200.0], "Hello world", "/F1 12 Tf 0 g"));
    let input = dir.path("latin.pdf");
    fixture.write(&input);

    let out = dir.path("latin-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let ops = ops_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert!(ops.contains("/FOmni0 12 Tf"), "WinAnsi keeps its face: {ops:?}");
    assert!(!ops.contains("/F1 "), "the composite font is not consulted: {ops:?}");
}

#[test]
fn two_notes_share_one_font_object() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let font = cid_font(&mut fixture, true);
    install_dr(&mut fixture, "F1", font);
    fixture.annot(0, cjk_freetext([100.0, 100.0, 300.0, 200.0], "中", "/F1 12 Tf 0 g"));
    fixture.annot(1, cjk_freetext([100.0, 100.0, 300.0, 200.0], "中中", "/F1 12 Tf 0 g"));
    let input = dir.path("shared.pdf");
    fixture.write(&input);

    let out = dir.path("shared-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let count = |key: &[u8], want: &[u8]| {
        doc.objects
            .values()
            .filter(|object| {
                object.as_dict().ok().and_then(|dict| dict.get(key).ok()).and_then(|value| value.as_name().ok())
                    == Some(want)
            })
            .count()
    };
    assert_eq!(count(b"Subtype", b"Type0"), 1, "one composite font, referenced twice");
    assert_eq!(count(b"Type", b"FontDescriptor"), 1, "one descriptor");
    let programs = doc
        .objects
        .values()
        .filter(|object| object.as_dict().ok().is_some_and(|dict| dict.get(b"FontFile2").is_ok()))
        .count();
    assert_eq!(programs, 1, "the program is not copied either");
    // Two sheets, two notes, both drawn in the same resource name.
    for sheet in sheets(&doc).iter().map(|(_, id)| *id).take(2) {
        let drawn = painted(&doc, sheet);
        if drawn.is_empty() {
            continue;
        }
        assert!(operands(&doc, sheet, &drawn[0].name).starts_with("<0024"), "the CID is drawn");
    }
}
