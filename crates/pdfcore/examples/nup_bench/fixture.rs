//! The document the benchmark imposes: pages, notes, and the two font kinds.
//!
//! Kept apart from the driver so the scenario can grow without the measurement
//! code growing with it.

use lopdf::{dictionary, Dictionary, Document, Object, Stream};

fn rect(rect: [f64; 4]) -> Object {
    Object::Array(rect.iter().map(|value| Object::Real(*value as f32)).collect())
}

/// A note whose text is UTF-16BE, as a producer writes CJK.
fn utf16(text: &str) -> Object {
    let mut units = vec![0xFE, 0xFF];
    for unit in text.encode_utf16() {
        units.extend_from_slice(&unit.to_be_bytes());
    }
    Object::String(units, lopdf::StringFormat::Literal)
}

/// How large a composite font to build: a real CJK font carries thousands of
/// entries, and the lookups have to stay fast at that size.
struct FontSize {
    to_unicode: u32,
    widths: usize,
    glyph_map: bool,
}

impl Default for FontSize {
    fn default() -> Self {
        Self { to_unicode: 1_000, widths: 2_000, glyph_map: false }
    }
}

/// One composite font: Identity-H, an embedded program, `/W` and `/ToUnicode`
/// for the two ideographs the notes use.
fn composite_font(doc: &mut Document, size: &FontSize) -> Object {
    let program = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), vec![0u8; 4096])));
    let map = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), to_unicode_entries(size.to_unicode))));
    let descriptor = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "FontDescriptor",
        "Ascent" => 880i64,
        "FontFile2" => Object::Reference(program),
    }));
    let widths: Vec<Object> = (0..size.widths).map(|index| Object::Integer(500 + (index % 200) as i64)).collect();
    let mut child = dictionary! {
        "Type" => "Font",
        "Subtype" => "CIDFontType2",
        "BaseFont" => Object::Name(b"Bench".to_vec()),
        "FontDescriptor" => Object::Reference(descriptor),
        "DW" => 1000i64,
        "W" => Object::Array(vec![Object::Integer(3), Object::Array(widths)]),
    };
    if size.glyph_map {
        // Two bytes per CID, with every glyph present.
        let mut table = vec![0u8; 65_536 * 2];
        for cid in 0u16..65_535 {
            table[usize::from(cid) * 2 + 1] = (cid % 251) as u8 | 1;
        }
        let stream = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), table)));
        child.set("CIDToGIDMap", Object::Reference(stream));
    }
    let child = doc.add_object(Object::Dictionary(child));
    Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => Object::Name(b"Bench".to_vec()),
        "Encoding" => "Identity-H",
        "DescendantFonts" => Object::Array(vec![Object::Reference(child)]),
        "ToUnicode" => Object::Reference(map),
    })))
}

/// A `/ToUnicode` with `entries` codes, ending with the four the notes use.
fn to_unicode_entries(entries: u32) -> Vec<u8> {
    let mut body = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    body.push_str(&format!("{entries} beginbfchar\n"));
    for code in 0x0300u32..0x0300 + entries {
        body.push_str(&format!("<{code:04X}> <{code:04X}>\n"));
    }
    body.push_str("endbfchar\n4 beginbfchar\n<0003> <0020>\n<0024> <4E2D>\n<0025> <4E01>\n<0026> <4E09>\nendbfchar\n");
    body.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    body.into_bytes()
}

/// One simple font with 224 widths and a `/Differences` list.
fn simple_font(doc: &mut Document) -> Object {
    let descriptor = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "FontDescriptor",
        "Ascent" => 700i64,
        "MissingWidth" => 0i64,
    }));
    let widths: Vec<Object> = (0..224).map(|index| Object::Integer(400 + index % 300)).collect();
    Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "TrueType",
        "BaseFont" => Object::Name(b"Bench-Simple".to_vec()),
        "FirstChar" => 32i64,
        "Widths" => Object::Array(widths),
        "Encoding" => "WinAnsiEncoding",
        "FontDescriptor" => Object::Reference(descriptor),
    })))
}

/// A document with `pages` pages, `notes` notes on each, sharing one font.
pub fn build(pages: usize, notes: usize, kind: &str) -> Vec<u8> {
    let size = match kind {
        "composite-large" => FontSize { to_unicode: 20_000, widths: 4_000, glyph_map: true },
        _ => FontSize::default(),
    };
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let composite = kind != "simple";
    let mut fonts = Dictionary::new();
    if composite {
        fonts.set("F1", composite_font(&mut doc, &size));
    } else {
        fonts.set("F1", simple_font(&mut doc));
    }
    let page_font = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    }));
    let mut kids: Vec<Object> = Vec::with_capacity(pages);
    for index in 0..pages {
        let body = format!("BT /F1 18 Tf 40 760 Td (Page {}) Tj ET", index + 1);
        let content = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), body.into_bytes())));
        let mut annots: Vec<Object> = Vec::with_capacity(notes);
        for note in 0..notes {
            let top = 700.0 - note as f64 * 40.0;
            let mut annot = dictionary! {
                "Type" => "Annot",
                "Subtype" => "FreeText",
                "Rect" => rect([40.0, top, 540.0, top + 30.0]),
                "F" => 4i64,
                "DA" => Object::string_literal("/F1 12 Tf 0 g"),
            };
            if composite {
                annot.set("Contents", utf16("中丁三"));
            } else {
                annot.set("Contents", Object::string_literal("A note with some words"));
            }
            annots.push(Object::Reference(doc.add_object(Object::Dictionary(annot))));
        }
        let page = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => rect([0.0, 0.0, 595.0, 842.0]),
            "Resources" => dictionary! { "Font" => dictionary! { "F0" => Object::Reference(page_font) } },
            "Contents" => Object::Reference(content),
            "Annots" => Object::Array(annots),
        }));
        kids.push(Object::Reference(page));
    }
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => Object::Array(kids),
            "Count" => Object::Integer(pages as i64),
        }),
    );
    let catalog = doc.new_object_id();
    doc.objects.insert(
        catalog,
        Object::Dictionary(dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) }),
    );
    doc.trailer.set("Root", Object::Reference(catalog));
    // The form's `/DR`, where the note's `/DA` font is found.
    let form = dictionary! {
        "DA" => Object::string_literal("/F1 12 Tf 0 g"),
        "DR" => dictionary! { "Font" => fonts },
    };
    doc.catalog_mut().expect("catalog").set("AcroForm", Object::Dictionary(form));
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save");
    bytes
}
