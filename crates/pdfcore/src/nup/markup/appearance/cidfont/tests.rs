//! Tests for the composite-font reader: what a note may draw with, and every
//! shape the reader refuses.

use super::*;
use lopdf::{dictionary, Document, Object, ObjectId};

/// `/ToUnicode` for a font whose space is CID 3 and whose one ideograph is CID
/// 36 (`$`), the code the file itself would have written.
fn to_unicode() -> Vec<u8> {
    let body = "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
        /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
        1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
        2 beginbfchar\n<0003> <0020>\n<0024> <4E2D>\nendbfchar\n\
        endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n";
    body.as_bytes().to_vec()
}

/// One knob per refusal: every test starts from a font that works.
struct Knobs {
    encoding: Object,
    descendants: usize,
    subtype: &'static str,
    program: bool,
    to_unicode: Option<Object>,
    widths: Option<Object>,
    default_width: Option<Object>,
    gid_map: Option<Object>,
    direct: bool,
}

impl Default for Knobs {
    fn default() -> Self {
        Self {
            encoding: Object::Name(b"Identity-H".to_vec()),
            descendants: 1,
            subtype: "CIDFontType2",
            program: true,
            to_unicode: None,
            widths: Some(Object::Array(vec![Object::Integer(3), Object::Array(vec![Object::Integer(250)])])),
            default_width: Some(Object::Integer(1000)),
            gid_map: None,
            direct: false,
        }
    }
}

/// A document with one Type0 font and an annotation whose `/DR` names it, plus
/// the object id of the font (when it is indirect).
fn build(knobs: Knobs) -> (Document, Dictionary, Option<ObjectId>) {
    let mut doc = Document::with_version("1.7");
    let program =
        knobs.program.then(|| doc.add_object(Object::Stream(lopdf::Stream::new(Dictionary::new(), vec![0u8; 16]))));
    let mut descriptor = dictionary! { "Type" => "FontDescriptor", "Ascent" => 880i64 };
    if let Some(program) = program {
        descriptor.set("FontFile2", Object::Reference(program));
    }
    let descriptor = doc.add_object(Object::Dictionary(descriptor));
    let mut child = dictionary! {
        "Type" => "Font",
        "Subtype" => Object::Name(knobs.subtype.as_bytes().to_vec()),
        "BaseFont" => Object::Name(b"NotoSansCJK".to_vec()),
        "FontDescriptor" => Object::Reference(descriptor),
    };
    if let Some(widths) = knobs.widths {
        child.set("W", widths);
    }
    if let Some(default) = knobs.default_width {
        child.set("DW", default);
    }
    if let Some(map) = knobs.gid_map {
        child.set("CIDToGIDMap", map);
    }
    let child = doc.add_object(Object::Dictionary(child));
    let to_unicode = match knobs.to_unicode {
        Some(Object::Reference(id)) => Some(Object::Reference(id)),
        Some(Object::Stream(stream)) => Some(Object::Reference(doc.add_object(Object::Stream(stream)))),
        Some(other) => Some(other),
        None => {
            Some(Object::Reference(doc.add_object(Object::Stream(lopdf::Stream::new(Dictionary::new(), to_unicode())))))
        }
    };
    let mut font = dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => Object::Name(b"NotoSansCJK".to_vec()),
        "Encoding" => knobs.encoding,
        "DescendantFonts" => Object::Array(
            (0..knobs.descendants).map(|_| Object::Reference(child)).collect::<Vec<Object>>(),
        ),
    };
    if let Some(to_unicode) = to_unicode {
        font.set("ToUnicode", to_unicode);
    }
    let entry = if knobs.direct {
        Object::Dictionary(font)
    } else {
        Object::Reference(doc.add_object(Object::Dictionary(font)))
    };
    let id = entry.as_reference().ok();
    let annot = dictionary! {
        "DR" => dictionary! { "Font" => dictionary! { "F1" => entry } },
    };
    (doc, annot, id)
}

fn resolve(knobs: Knobs) -> Option<DocCid> {
    let (doc, annot, _) = build(knobs);
    resolve_font(&doc, &annot, "F1")
}

#[test]
fn an_identity_h_cid_font_is_read() {
    let font = resolve(Knobs::default()).expect("the font is usable");
    assert_eq!(font.code(' '), Some(3));
    assert_eq!(font.code('\u{4E2D}'), Some(0x24));
    assert_eq!(font.advance(0x24), Some(1000.0), "a CID outside /W takes /DW");
    assert_eq!(font.advance(3), Some(250.0));
    assert_eq!(font.code('A'), None, "the map does not cover it");
    assert!((font.ascent() - 0.88).abs() < 0.01);
}

#[test]
fn the_font_is_referenced_where_it_already_is() {
    let (doc, annot, id) = build(Knobs::default());
    let font = resolve_font(&doc, &annot, "F1").expect("the font is usable");
    assert_eq!(Some(font.id()), id);
    assert_eq!(font.key(), "F1");
}

#[test]
fn an_identity_v_or_predefined_encoding_is_refused() {
    let vertical = Knobs { encoding: Object::Name(b"Identity-V".to_vec()), ..Knobs::default() };
    assert!(resolve(vertical).is_none(), "vertical writing is not laid out");
    let predefined = Knobs { encoding: Object::Name(b"UniGB-UCS2-H".to_vec()), ..Knobs::default() };
    assert!(resolve(predefined).is_none(), "the codes would not be two-byte CIDs");
    let embedded = Knobs {
        encoding: Object::Stream(lopdf::Stream::new(Dictionary::new(), b"begincmap".to_vec())),
        ..Knobs::default()
    };
    assert!(resolve(embedded).is_none(), "an embedded CMap is not parsed");
    let missing = Knobs { encoding: Object::Null, ..Knobs::default() };
    assert!(resolve(missing).is_none(), "/Encoding has no default");
}

#[test]
fn the_wrong_number_of_descendants_is_refused() {
    assert!(resolve(Knobs { descendants: 0, ..Knobs::default() }).is_none());
    assert!(resolve(Knobs { descendants: 2, ..Knobs::default() }).is_none());
}

#[test]
fn a_descendant_that_is_not_a_cid_font_is_refused() {
    for subtype in ["Type3", "Type1", "CIDFontType3"] {
        assert!(resolve(Knobs { subtype, ..Knobs::default() }).is_none(), "{subtype}");
    }
}

#[test]
fn a_font_without_a_program_is_refused() {
    assert!(resolve(Knobs { program: false, ..Knobs::default() }).is_none());
}

#[test]
fn a_font_without_a_usable_to_unicode_is_refused() {
    assert!(resolve(Knobs { to_unicode: Some(Object::Null), ..Knobs::default() }).is_none());
    let empty = lopdf::Stream::new(Dictionary::new(), b"1 begincmap endcmap".to_vec());
    assert!(resolve(Knobs { to_unicode: Some(Object::Stream(empty)), ..Knobs::default() }).is_none());
    let garbage = lopdf::Stream::new(Dictionary::new(), vec![0xFF; 8]);
    assert!(resolve(Knobs { to_unicode: Some(Object::Stream(garbage)), ..Knobs::default() }).is_none());
}

#[test]
fn broken_widths_reject_the_font() {
    let broken = |widths: Option<Object>, default_width: Option<Object>| {
        resolve(Knobs { widths, default_width, ..Knobs::default() }).is_none()
    };
    assert!(broken(Some(Object::Array(vec![Object::Integer(3)])), None), "a CID with no width");
    assert!(broken(Some(Object::Array(vec![Object::Integer(3), Object::Integer(3)])), None));
    assert!(broken(None, Some(Object::Integer(-5))), "a negative default");
    assert!(broken(None, Some(Object::Real(f32::NAN))));
    // A font with no /W at all is legal: every CID takes /DW.
    assert!(resolve(Knobs { widths: None, ..Knobs::default() }).is_some());
}

#[test]
fn a_direct_font_dictionary_cannot_be_referenced() {
    let (doc, annot, id) = build(Knobs { direct: true, ..Knobs::default() });
    assert!(id.is_none());
    assert!(resolve_font(&doc, &annot, "F1").is_none());
}

#[test]
fn a_missing_name_is_not_a_font() {
    let (doc, annot, _) = build(Knobs::default());
    assert!(resolve_font(&doc, &annot, "F2").is_none());
}

#[test]
fn a_cid_map_that_points_at_notdef_costs_that_character() {
    // The map gives CID 0x24 glyph 7 and CID 3 glyph 0.
    let mut table = vec![0u8; 0x26 * 2];
    table[0x24 * 2 + 1] = 7;
    let stream = lopdf::Stream::new(Dictionary::new(), table);
    let font = resolve(Knobs { gid_map: Some(Object::Stream(stream)), ..Knobs::default() }).expect("font");
    assert_eq!(font.code('\u{4E2D}'), Some(0x24), "glyph 7 draws");
    assert_eq!(font.code(' '), None, "glyph 0 is the .notdef box");
}

#[test]
fn a_cid_past_the_map_has_no_glyph() {
    // Four entries only: CID 3 has glyph 5, and CID 0x24 is past the end.
    let stream = lopdf::Stream::new(Dictionary::new(), vec![0, 1, 0, 2, 0, 3, 0, 5]);
    let font = resolve(Knobs { gid_map: Some(Object::Stream(stream)), ..Knobs::default() }).expect("font");
    assert_eq!(font.code(' '), Some(3));
    assert_eq!(font.code('\u{4E2D}'), None);
}

#[test]
fn a_map_that_says_something_else_is_refused() {
    let name = Object::Name(b"Never".to_vec());
    assert!(resolve(Knobs { gid_map: Some(name), ..Knobs::default() }).is_none());
    let identity = Object::Name(b"Identity".to_vec());
    assert!(resolve(Knobs { gid_map: Some(identity), ..Knobs::default() }).is_some());
}
