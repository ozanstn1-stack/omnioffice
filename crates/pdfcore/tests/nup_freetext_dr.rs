//! FreeText notes reuse a simple `/DR` font from the document when they can.

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

fn font_keys(doc: &Document, sheet: ObjectId, name: &str) -> Vec<String> {
    let stream = stream_of(doc, sheet, name);
    let Some(resources) = stream.dict.get(b"Resources").ok().and_then(|value| value.as_dict().ok()) else {
        return Vec::new();
    };
    let Some(fonts) = resources.get(b"Font").ok().and_then(|value| value.as_dict().ok()) else {
        return Vec::new();
    };
    fonts.iter().map(|(key, _)| String::from_utf8_lossy(key).to_string()).collect()
}

/// A simple font dictionary with the given widths starting at `/FirstChar`.
fn simple_font(fixture: &mut Fixture, base: &str, subtype: &str, first: i64, widths: &[i64]) -> ObjectId {
    let descriptor = fixture.add_object(Object::Dictionary(dictionary! {
        "Type" => "FontDescriptor",
        "Ascent" => 700i64,
        "Descent" => -200i64,
        "MissingWidth" => 0i64,
    }));
    fixture.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => Object::Name(subtype.as_bytes().to_vec()),
        "BaseFont" => Object::Name(base.as_bytes().to_vec()),
        "FirstChar" => Object::Integer(first),
        "Widths" => Object::Array(widths.iter().map(|width| Object::Integer(*width)).collect::<Vec<Object>>()),
        "Encoding" => "WinAnsiEncoding",
        "FontDescriptor" => Object::Reference(descriptor),
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
fn a_document_font_is_referenced_instead_of_a_standard_one() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let font = simple_font(&mut fixture, "Helvetica", "Type1", 32, &[600; 224]);
    install_dr(&mut fixture, "F1", font);
    fixture.annot(0, freetext([100.0, 100.0, 300.0, 200.0], "Hello world", "/F1 12 Tf 0 g"));
    let input = dir.path("dr.pdf");
    fixture.write(&input);

    let out = dir.path("dr-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let drawn = painted(&doc, sheet);
    assert_eq!(drawn.len(), 1);
    let ops = ops_of(&doc, sheet, &drawn[0].name);
    assert!(ops.contains("/F1 12 Tf"), "the document's font is used: {ops:?}");
    assert!(!ops.contains("/FOmni0"), "no standard font is written: {ops:?}");
    assert_eq!(font_keys(&doc, sheet, &drawn[0].name), vec!["F1".to_string()]);
}

#[test]
fn narrow_widths_change_the_line_breaking() {
    let dir = TestDir::new();
    let mut narrow = Fixture::new(&page_specs(2));
    let font = simple_font(&mut narrow, "Helvetica", "Type1", 32, &[900; 224]);
    install_dr(&mut narrow, "F1", font);
    narrow.annot(0, freetext([100.0, 100.0, 200.0, 200.0], "wide words here", "/F1 12 Tf 0 g"));
    let narrow_input = dir.path("narrow.pdf");
    narrow.write(&narrow_input);
    let narrow_out = dir.path("narrow-out.pdf");
    impose(&narrow_input, &narrow_out, &NupOptions::default());
    let doc = Document::load(&narrow_out).expect("load output");
    let narrow_lines =
        ops_of(&doc, sheets(&doc)[0].1, &painted(&doc, sheets(&doc)[0].1)[0].name).matches(" Tm\n").count();

    let mut wide = Fixture::new(&page_specs(2));
    let font = simple_font(&mut wide, "Helvetica", "Type1", 32, &[200; 224]);
    install_dr(&mut wide, "F1", font);
    wide.annot(0, freetext([100.0, 100.0, 200.0, 200.0], "wide words here", "/F1 12 Tf 0 g"));
    let wide_input = dir.path("wide.pdf");
    wide.write(&wide_input);
    let wide_out = dir.path("wide-out.pdf");
    impose(&wide_input, &wide_out, &NupOptions::default());
    let doc = Document::load(&wide_out).expect("load output");
    let wide_lines =
        ops_of(&doc, sheets(&doc)[0].1, &painted(&doc, sheets(&doc)[0].1)[0].name).matches(" Tm\n").count();
    assert!(narrow_lines > wide_lines, "the narrower widths wrap sooner: {narrow_lines} vs {wide_lines}");
}

#[test]
fn a_missing_glyph_falls_back_to_the_standard_face() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    // Every width is zero: the subset carries no real glyph.
    let font = simple_font(&mut fixture, "Helvetica", "Type1", 32, &[0; 224]);
    install_dr(&mut fixture, "F1", font);
    fixture.annot(0, freetext([100.0, 100.0, 300.0, 200.0], "Hello", "/F1 12 Tf 0 g"));
    let input = dir.path("no-glyph.pdf");
    fixture.write(&input);

    let out = dir.path("no-glyph-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let ops = ops_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert!(ops.contains("/FOmni0 12 Tf"), "the standard font takes over: {ops:?}");
}

#[test]
fn an_unrepresentable_character_drops_the_note() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let font = simple_font(&mut fixture, "Helvetica", "Type1", 32, &[600; 224]);
    install_dr(&mut fixture, "F1", font);
    // UTF-16, as a producer writes it (a UTF-8 literal would read as PDFDoc).
    let mut cjk = freetext([100.0, 100.0, 300.0, 200.0], "x", "/F1 12 Tf 0 g");
    cjk.set("Contents", Object::String(vec![0xFE, 0xFF, 0x6F, 0x22, 0x5B, 0x57], lopdf::StringFormat::Literal));
    fixture.annot(0, cjk);
    fixture.annot(0, freetext([350.0, 100.0, 550.0, 200.0], "latin", "/F1 12 Tf 0 g"));
    let input = dir.path("cjk.pdf");
    fixture.write(&input);

    let out = dir.path("cjk-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    assert_eq!(painted(&doc, sheets(&doc)[0].1).len(), 1, "only the Latin note survives");
}

#[test]
fn a_composite_font_is_refused() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let font = simple_font(&mut fixture, "HeiseiMin-W3", "Type0", 0, &[1000; 4]);
    install_dr(&mut fixture, "F1", font);
    fixture.annot(0, freetext([100.0, 100.0, 300.0, 200.0], "Hello", "/F1 12 Tf 0 g"));
    let input = dir.path("type0.pdf");
    fixture.write(&input);

    let out = dir.path("type0-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let ops = ops_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert!(ops.contains("/FOmni0"), "a composite font falls back: {ops:?}");
}

#[test]
fn a_font_without_widths_is_refused() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let font = fixture.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    }));
    install_dr(&mut fixture, "F1", font);
    fixture.annot(0, freetext([100.0, 100.0, 300.0, 200.0], "Hello", "/F1 12 Tf 0 g"));
    let input = dir.path("no-widths.pdf");
    fixture.write(&input);

    let out = dir.path("no-widths-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let ops = ops_of(&doc, sheets(&doc)[0].1, &painted(&doc, sheets(&doc)[0].1)[0].name);
    assert!(ops.contains("/FOmni0"), "an unmeasurable font falls back: {ops:?}");
}

#[test]
fn an_unknown_resource_name_falls_back() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let font = simple_font(&mut fixture, "Helvetica", "Type1", 32, &[600; 224]);
    install_dr(&mut fixture, "F1", font);
    fixture.annot(0, freetext([100.0, 100.0, 300.0, 200.0], "Hello", "/Other 12 Tf 0 g"));
    let input = dir.path("unknown.pdf");
    fixture.write(&input);

    let out = dir.path("unknown-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let ops = ops_of(&doc, sheets(&doc)[0].1, &painted(&doc, sheets(&doc)[0].1)[0].name);
    assert!(ops.contains("/FOmni0"), "the unknown name falls back: {ops:?}");
}

#[test]
fn differences_carry_turkish_characters_through_the_document_font() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let descriptor = fixture.add_object(Object::Dictionary(dictionary! { "Type" => "FontDescriptor" }));
    let font = fixture.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "TrueType",
        "BaseFont" => "ABCDEF+Custom",
        "FirstChar" => 65,
        "Widths" => Object::Array(vec![Object::Integer(700)]),
        "Encoding" => Object::Dictionary(dictionary! {
            "BaseEncoding" => "WinAnsiEncoding",
            "Differences" => Object::Array(vec![Object::Integer(65), Object::Name(b"gbreve".to_vec())]),
        }),
        "FontDescriptor" => Object::Reference(descriptor),
    }));
    install_dr(&mut fixture, "F1", font);
    // UTF-16BE with a BOM carries a non-Latin-1 glyph.
    let mut annot = freetext([100.0, 100.0, 300.0, 200.0], "x", "/F1 12 Tf 0 g");
    annot.set("Contents", Object::String(vec![0xFE, 0xFF, 0x01, 0x1F], lopdf::StringFormat::Literal));
    fixture.annot(0, annot);
    let input = dir.path("turkish.pdf");
    fixture.write(&input);

    let out = dir.path("turkish-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let ops = ops_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert!(ops.contains("/F1 12 Tf"), "the document font is used: {ops:?}");
    assert!(ops.contains("<41> Tj"), "the difference maps the letter to code 65: {ops:?}");
}

#[test]
fn a_symbolic_truetype_without_encoding_is_refused() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let font = fixture.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "TrueType",
        "BaseFont" => "Symbol",
        "FirstChar" => 32,
        "Widths" => Object::Array(vec![Object::Integer(600); 224]),
        "Flags" => 4i64,
    }));
    install_dr(&mut fixture, "F1", font);
    fixture.annot(0, freetext([100.0, 100.0, 300.0, 200.0], "Hello", "/F1 12 Tf 0 g"));
    let input = dir.path("symbolic.pdf");
    fixture.write(&input);

    let out = dir.path("symbolic-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let ops = ops_of(&doc, sheets(&doc)[0].1, &painted(&doc, sheets(&doc)[0].1)[0].name);
    assert!(ops.contains("/FOmni0"), "a symbolic font without encoding falls back: {ops:?}");
}

#[test]
fn an_annotation_with_an_appearance_is_left_alone() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let font = simple_font(&mut fixture, "Helvetica", "Type1", 32, &[600; 224]);
    install_dr(&mut fixture, "F1", font);
    let ap = fixture.add_object(markup_fixtures::appearance([0.0, 0.0, 100.0, 50.0], None));
    let mut annot = freetext([100.0, 100.0, 300.0, 200.0], "already drawn", "/F1 12 Tf 0 g");
    annot.set("AP", dictionary! { "N" => Object::Reference(ap) });
    fixture.annot(0, annot);
    let input = dir.path("has-ap.pdf");
    fixture.write(&input);

    let out = dir.path("has-ap-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let ops = ops_of(&doc, sheet, &painted(&doc, sheet)[0].name);
    assert!(!ops.contains("BT\n"), "nothing is re-synthesized: {ops:?}");
}
