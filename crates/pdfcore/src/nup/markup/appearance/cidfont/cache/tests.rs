//! Tests for the per-document font cache: one read per font, whatever the note
//! count, and a refusal remembered as well as a success.

use super::*;
use lopdf::{dictionary, Object};

/// `/ToUnicode` for a font whose space is CID 3 and whose one ideograph is CID
/// 36.
fn to_unicode() -> Vec<u8> {
    let body = "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
        1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
        2 beginbfchar\n<0003> <0020>\n<0024> <4E2D>\nendbfchar\n\
        endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n";
    body.as_bytes().to_vec()
}

/// A document with one usable composite font, and its object id.
fn document() -> (Document, ObjectId) {
    let mut doc = Document::with_version("1.7");
    let program = doc.add_object(Object::Stream(lopdf::Stream::new(Dictionary::new(), vec![0u8; 32])));
    let map = doc.add_object(Object::Stream(lopdf::Stream::new(Dictionary::new(), to_unicode())));
    let descriptor = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "FontDescriptor",
        "Ascent" => 880i64,
        "FontFile2" => Object::Reference(program),
    }));
    let child = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "CIDFontType2",
        "BaseFont" => Object::Name(b"Test".to_vec()),
        "FontDescriptor" => Object::Reference(descriptor),
        "DW" => 1000i64,
        "W" => Object::Array(vec![Object::Integer(3), Object::Array(vec![Object::Integer(250)])]),
    }));
    let id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => Object::Name(b"Test".to_vec()),
        "Encoding" => "Identity-H",
        "DescendantFonts" => Object::Array(vec![Object::Reference(child)]),
        "ToUnicode" => Object::Reference(map),
    }));
    (doc, id)
}

fn font_of(doc: &Document, id: ObjectId) -> Dictionary {
    doc.get_dictionary(id).expect("font dictionary").clone()
}

#[test]
fn the_same_font_is_read_once_however_many_notes_use_it() {
    let (doc, id) = document();
    let dict = font_of(&doc, id);
    let mut cache = FontCache::new();
    for _ in 0..200 {
        assert!(cache.get_or_load(&doc, id, &dict).is_some());
    }
    let stats = cache.stats();
    assert_eq!(stats.parses, 1, "the font is read once");
    assert_eq!(stats.hits, 199);
    assert_eq!(stats.misses, 1);
    assert_eq!(stats.bypassed, 0);
}

#[test]
fn two_fonts_get_two_entries() {
    let (mut doc, first) = document();
    // A second font with its own map and widths.
    let second = {
        let (other, id) = document();
        let dict = font_of(&other, id);
        doc.add_object(Object::Dictionary(dict))
    };
    let a = font_of(&doc, first);
    let b = font_of(&doc, second);
    let mut cache = FontCache::new();
    for _ in 0..3 {
        assert!(cache.get_or_load(&doc, first, &a).is_some());
        assert!(cache.get_or_load(&doc, second, &b).is_some());
    }
    assert_eq!(cache.stats().parses, 2);
    assert_eq!(cache.stats().hits, 4);
}

#[test]
fn a_font_that_cannot_be_used_is_remembered_too() {
    let (mut doc, id) = document();
    // A Type0 font without `/ToUnicode` is refused; the reading is not free, so
    // the refusal is worth a cache entry.
    let dict = font_of(&doc, id);
    let mut broken = dict.clone();
    broken.remove(b"ToUnicode");
    let broken_id = doc.add_object(Object::Dictionary(broken));
    let broken = font_of(&doc, broken_id);
    let mut cache = FontCache::new();
    assert!(cache.get_or_load(&doc, broken_id, &broken).is_none());
    assert!(cache.get_or_load(&doc, broken_id, &broken).is_none());
    let stats = cache.stats();
    assert_eq!(stats.misses, 1, "the second call does not read the streams again");
    assert_eq!(stats.hits, 1);
    assert_eq!(stats.parses, 0, "a refusal is not a parse");
}

#[test]
fn a_font_past_the_budget_is_used_but_not_stored() {
    let (doc, id) = document();
    let dict = font_of(&doc, id);
    let mut cache = FontCache::with_budget(0);
    for _ in 0..2 {
        assert!(cache.get_or_load(&doc, id, &dict).is_some(), "the note still draws");
    }
    let stats = cache.stats();
    assert_eq!(stats.parses, 2, "nothing was stored, so the font is read again");
    assert_eq!(stats.bypassed, 2);
    assert_eq!(stats.hits, 0);
}

#[test]
fn the_heap_estimate_grows_with_the_font() {
    let (doc, id) = document();
    let dict = font_of(&doc, id);
    let mut cache = FontCache::new();
    let font = cache.get_or_load(&doc, id, &dict).expect("font");
    assert!(font.heap_bytes() > 0, "a font with a map, widths and a program is not free");
    assert!(font.heap_bytes() < DEFAULT_BUDGET_BYTES);
}
