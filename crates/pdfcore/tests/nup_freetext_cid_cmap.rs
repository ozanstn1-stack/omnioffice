//! FreeText notes in a document font whose `/Encoding` is an embedded CMap.

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

/// The `/ToUnicode` of the font: the space and one ideograph.
fn to_unicode() -> String {
    "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
     1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
     2 beginbfchar\n<3000> <0020>\n<3042> <3042>\nendbfchar\n\
     endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n"
        .to_string()
}

/// An embedded `/Encoding` CMap: `あ` (3042) selects CID 844, the space CID 633.
fn encoding_cmap() -> String {
    "begincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (Japan1) /Supplement 2 >> def\n\
     /CMapName /Test-RKSJ-H def\n/CMapType 1 def\n\
     1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
     1 begincidrange\n<3040> <309F> 842\nendcidrange\n\
     1 begincidchar\n<3000> 633\nendcidchar\n\
     endcmap\nCMapName currentdict /CMap defineresource pop\n"
        .to_string()
}

/// A CMap whose codespace is one byte wide, which the reader refuses.
fn one_byte_cmap() -> String {
    "begincmap\n1 begincodespacerange\n<00> <FF>\nendcodespacerange\n\
     1 begincidrange\n<20> <7E> 1\nendcidrange\n\
     endcmap\n"
        .to_string()
}

/// A composite font whose `/Encoding` is `encoding`, with `/W` keyed by CID.
fn cid_font(fixture: &mut Fixture, encoding: Object, widths: Object) -> ObjectId {
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
        "W" => widths,
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

/// The first sheet's content stream of a one-page document, after imposition.
/// The whole stream, not just the note, so a document that paints nothing at all
/// can be compared too.
fn sheet_content(build: impl FnOnce(&mut Fixture), dir: &TestDir, name: &str) -> String {
    let mut fixture = Fixture::new(&page_specs(2));
    build(&mut fixture);
    let input = dir.path(&format!("{name}.pdf"));
    fixture.write(&input);
    let out = dir.path(&format!("{name}-out.pdf"));
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    page_content(&doc, sheet)
}

#[test]
fn an_embedded_cmap_draws_the_code_keyed_widths_by_cid() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let encoding = cmap_stream(&mut fixture, encoding_cmap());
    // Only the two CIDs the note uses have a width; every other CID keeps `/DW 0`.
    let widths = Object::Array(vec![
        Object::Integer(844),
        Object::Array(vec![Object::Integer(1000)]),
        Object::Integer(633),
        Object::Array(vec![Object::Integer(500)]),
    ]);
    let font = cid_font(&mut fixture, Object::Reference(encoding), widths);
    install_dr(&mut fixture, "F1", font);
    fixture.annot(0, cjk_freetext([100.0, 100.0, 300.0, 200.0], "あ", "/F1 12 Tf 0 g"));
    let input = dir.path("embedded.pdf");
    fixture.write(&input);

    let out = dir.path("embedded-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let drawn = painted(&doc, sheet);
    assert_eq!(drawn.len(), 1);
    let ops = ops_of(&doc, sheet, &drawn[0].name);
    assert!(ops.contains("/F1 12 Tf"), "the document's font is used: {ops:?}");
    assert_eq!(operands(&doc, sheet, &drawn[0].name), "<3042>", "the code, not the CID");
}

#[test]
fn a_one_byte_codespace_falls_back_like_a_refused_encoding() {
    let dir = TestDir::new();
    let one_byte = sheet_content(
        |fixture| {
            let encoding = cmap_stream(fixture, one_byte_cmap());
            let widths = Object::Array(vec![Object::Integer(1), Object::Array(vec![Object::Integer(1000)])]);
            let font = cid_font(fixture, Object::Reference(encoding), widths);
            install_dr(fixture, "F1", font);
            fixture.annot(0, cjk_freetext([100.0, 100.0, 300.0, 200.0], "あ", "/F1 12 Tf 0 g"));
            fixture.annot(0, freetext([350.0, 100.0, 550.0, 200.0], "latin", "/F1 12 Tf 0 g"));
        },
        &dir,
        "one-byte",
    );
    // The same document with a vertical encoding, which was always refused.
    let vertical = sheet_content(
        |fixture| {
            let widths = Object::Array(vec![Object::Integer(1), Object::Array(vec![Object::Integer(1000)])]);
            let font = cid_font(fixture, Object::Name(b"Identity-V".to_vec()), widths);
            install_dr(fixture, "F1", font);
            fixture.annot(0, cjk_freetext([100.0, 100.0, 300.0, 200.0], "あ", "/F1 12 Tf 0 g"));
            fixture.annot(0, freetext([350.0, 100.0, 550.0, 200.0], "latin", "/F1 12 Tf 0 g"));
        },
        &dir,
        "vertical",
    );
    assert_eq!(one_byte, vertical, "a refused encoding leaves byte-identical output");
    assert!(!one_byte.contains("/F1 "), "the font is not referenced: {one_byte:?}");
}

#[test]
fn latin_text_is_byte_identical_with_or_without_the_font() {
    let dir = TestDir::new();
    let with_font = sheet_content(
        |fixture| {
            let encoding = cmap_stream(fixture, encoding_cmap());
            let font = cid_font(
                fixture,
                Object::Reference(encoding),
                Object::Array(vec![
                    Object::Integer(844),
                    Object::Array(vec![Object::Integer(1000)]),
                    Object::Integer(633),
                    Object::Array(vec![Object::Integer(500)]),
                ]),
            );
            install_dr(fixture, "F1", font);
            fixture.annot(0, freetext([100.0, 100.0, 300.0, 200.0], "Hello world", "/F1 12 Tf 0 g"));
        },
        &dir,
        "latin-font",
    );
    let without = sheet_content(
        |fixture| {
            fixture.annot(0, freetext([100.0, 100.0, 300.0, 200.0], "Hello world", "/F1 12 Tf 0 g"));
        },
        &dir,
        "latin-none",
    );
    assert_eq!(with_font, without, "a composite font in /DR changes nothing for Latin text");
}

#[test]
fn a_character_the_map_does_not_cover_leaves_the_note_out() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let encoding = cmap_stream(&mut fixture, encoding_cmap());
    // Only the ideograph has a width; the space (CID 633) keeps `/DW 0`.
    let widths = Object::Array(vec![Object::Integer(844), Object::Array(vec![Object::Integer(1000)])]);
    let font = cid_font(&mut fixture, Object::Reference(encoding), widths);
    install_dr(&mut fixture, "F1", font);
    fixture.annot(0, cjk_freetext([100.0, 100.0, 300.0, 200.0], "あ あ", "/F1 12 Tf 0 g"));
    let input = dir.path("no-width.pdf");
    fixture.write(&input);

    let out = dir.path("no-width-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let drawn = painted(&doc, sheets(&doc)[0].1);
    let detail: Vec<String> = drawn.iter().map(|block| ops_of(&doc, sheets(&doc)[0].1, &block.name)).collect();
    assert!(drawn.is_empty(), "a character with no width drops the note: {detail:?}");
}

#[test]
fn the_font_is_referenced_and_its_cmap_kept_once() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let encoding = cmap_stream(&mut fixture, encoding_cmap());
    let widths = Object::Array(vec![
        Object::Integer(844),
        Object::Array(vec![Object::Integer(1000)]),
        Object::Integer(633),
        Object::Array(vec![Object::Integer(500)]),
    ]);
    let font = cid_font(&mut fixture, Object::Reference(encoding), widths);
    install_dr(&mut fixture, "F1", font);
    fixture.annot(0, cjk_freetext([100.0, 100.0, 300.0, 200.0], "あ", "/F1 12 Tf 0 g"));
    fixture.annot(1, cjk_freetext([100.0, 100.0, 300.0, 200.0], "ああ", "/F1 12 Tf 0 g"));
    let input = dir.path("shared-cmap.pdf");
    fixture.write(&input);

    let out = dir.path("shared-cmap-out.pdf");
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
    assert_eq!(count(b"Subtype", b"Type0"), 1, "one composite font");
    assert_eq!(count(b"Type", b"FontDescriptor"), 1, "one descriptor");
    let cmaps = doc
        .objects
        .values()
        .filter(|object| {
            object.as_stream().ok().is_some_and(|stream| {
                let bytes = stream.decompressed_content().unwrap_or_else(|_| stream.content.clone());
                bytes.starts_with(b"begincmap") && bytes.windows(10).any(|window| window == b"begincidra")
            })
        })
        .count();
    assert_eq!(cmaps, 1, "the /Encoding stream is not copied");
}
