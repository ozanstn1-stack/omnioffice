//! Tests for the code -> CID -> glyph id path of a composite font.

use super::*;
use crate::cmap::Code;

/// A two-byte code, which is what an Identity-H font writes.
fn two(value: u16) -> Code {
    Code::two(value)
}
use lopdf::{dictionary, Object};

fn doc() -> Document {
    Document::with_version("1.7")
}

fn stream(body: &[u8], extra: lopdf::Dictionary) -> Object {
    let mut stream = lopdf::Stream::new(extra, body.to_vec());
    stream.dict.set("Length", Object::Integer(body.len() as i64));
    Object::Stream(stream)
}

/// A CMap that maps `<3040>..<309F>` to CIDs 842.. and nothing else.
fn cmap_body() -> Vec<u8> {
    let body = "begincmap\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
                1 begincidrange\n<3040> <309F> 842\nendcidrange\n\
                endcmap\n";
    body.as_bytes().to_vec()
}

fn type0(encoding: Object) -> lopdf::Dictionary {
    dictionary! { "Type" => "Font", "Subtype" => "Type0", "Encoding" => encoding }
}

#[test]
fn an_identity_name_is_the_identity_encoding() {
    let doc = doc();
    let font = type0(Object::Name(b"Identity-H".to_vec()));
    assert!(matches!(encoding(&doc, &font), Some(Encoding::IdentityH)));
}

#[test]
fn an_embedded_stream_is_read() {
    let doc = doc();
    let font = type0(stream(&cmap_body(), lopdf::Dictionary::new()));
    let Some(Encoding::Embedded(map)) = encoding(&doc, &font) else { panic!("embedded map") };
    assert_eq!(map.cid(two(0x3042)), Some(844));
    assert_eq!(map.cid(two(0x4000)), None);
}

#[test]
fn an_encoding_stream_may_be_a_reference() {
    let mut doc = doc();
    let id = doc.add_object(Object::Stream(lopdf::Stream::new(lopdf::Dictionary::new(), cmap_body())));
    let font = type0(Object::Reference(id));
    let Some(Encoding::Embedded(map)) = encoding(&doc, &font) else { panic!("embedded map") };
    assert_eq!(map.cid(two(0x3042)), Some(844));
}

#[test]
fn a_vertical_or_unknown_name_is_refused() {
    let doc = doc();
    for name in [b"Identity-V".as_slice(), b"UniGB-UCS2-H"] {
        let font = type0(Object::Name(name.to_vec()));
        assert!(encoding(&doc, &font).is_none(), "{name:?}");
    }
    let missing = dictionary! { "Type" => "Font", "Subtype" => "Type0" };
    assert!(encoding(&doc, &missing).is_none(), "/Encoding has no default");
}

#[test]
fn a_vertical_stream_is_refused() {
    let doc = doc();
    let mut extra = lopdf::Dictionary::new();
    extra.set("WMode", Object::Integer(1));
    let font = type0(stream(&cmap_body(), extra));
    assert!(encoding(&doc, &font).is_none());
    let base = {
        let mut extra = lopdf::Dictionary::new();
        extra.set("UseCMap", Object::Name(b"UniJIS-UCS2-H".to_vec()));
        type0(stream(&cmap_body(), extra))
    };
    assert!(encoding(&doc, &base).is_none(), "a base we do not ship");
}

#[test]
fn a_corrupt_stream_is_refused() {
    let doc = doc();
    let font = type0(stream(b"1 begincodespacerange\n", lopdf::Dictionary::new()));
    assert!(encoding(&doc, &font).is_none(), "an unclosed block");
    let four_byte =
        type0(stream(b"1 begincodespacerange\n<00000000> <FFFFFFFF>\nendcodespacerange\n", lopdf::Dictionary::new()));
    assert!(encoding(&doc, &four_byte).is_none(), "four-byte codes");
}

#[test]
fn the_glyph_map_is_identity_when_absent_and_a_table_when_a_stream() {
    let doc = doc();
    let absent = dictionary! { "Type" => "Font", "Subtype" => "CIDFontType2" };
    assert_eq!(gid_map(&doc, &absent), Some(None));
    let identity = dictionary! { "CIDToGIDMap" => "Identity" };
    assert_eq!(gid_map(&doc, &identity), Some(None));
    // CID 3 -> glyph 0, CID 4 -> glyph 9: the first one never draws.
    let table = dictionary! { "CIDToGIDMap" => stream(&[0, 0, 0, 0, 0, 3, 0, 9], lopdf::Dictionary::new()) };
    let Some(Some(bytes)) = gid_map(&doc, &table) else { panic!("table") };
    assert_eq!(bytes.len(), 8);
    let other = dictionary! { "CIDToGIDMap" => "Never" };
    assert!(gid_map(&doc, &other).is_none());
}

#[test]
fn widths_are_keyed_by_cid_and_notdef_never_draws() {
    let doc = doc();
    let font = type0(stream(&cmap_body(), lopdf::Dictionary::new()));
    let child = dictionary! { "Type" => "Font", "Subtype" => "CIDFontType2" };
    let read = mapping(&doc, &font, &child, b"CIDFontType2").expect("mapping");
    assert_eq!(read.cid(two(0x3042)), Some(844), "the code is not the CID");
    assert_eq!(read.cid(two(0x309F)), Some(842 + 0x5F));
    assert_eq!(read.cid(two(0x0041)), None, "no entry for this code");
    // CID 0 is .notdef.
    let zero = type0(stream(
        b"1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n1 begincidchar\n<0001> 0\nendcidchar\n",
        lopdf::Dictionary::new(),
    ));
    let read = mapping(&doc, &zero, &child, b"CIDFontType2").expect("mapping");
    assert_eq!(read.admit(two(0x0001)), None);
}

#[test]
fn a_glyph_map_that_points_at_notdef_costs_that_code() {
    let doc = doc();
    let font = type0(Object::Name(b"Identity-H".to_vec()));
    let mut table = vec![0u8; 0x42 * 2];
    table[0x41 * 2 + 1] = 7;
    let child = dictionary! { "Subtype" => "CIDFontType2", "CIDToGIDMap" => stream(&table, lopdf::Dictionary::new()) };
    let read = mapping(&doc, &font, &child, b"CIDFontType2").expect("mapping");
    assert_eq!(read.admit(two(0x41)), Some(two(0x41)), "glyph 7 draws");
    assert_eq!(read.admit(two(0x42)), None, "past the end of the map");
    assert_eq!(read.admit(two(0x00)), None, "glyph 0 is .notdef");
}

#[test]
fn a_type0_descendant_ignores_the_glyph_map() {
    let doc = doc();
    let font = type0(Object::Name(b"Identity-H".to_vec()));
    let child = dictionary! { "Subtype" => "CIDFontType0", "CIDToGIDMap" => "Never" };
    let read = mapping(&doc, &font, &child, b"CIDFontType0").expect("mapping");
    assert_eq!(read.cid(two(0x41)), Some(0x41));
}
