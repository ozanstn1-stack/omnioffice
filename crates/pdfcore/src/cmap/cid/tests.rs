//! Tests for the `/Encoding` CMap reader: what a code selects, and every shape
//! it refuses.

use super::*;

/// A two-byte code, which is what an Identity-H font writes.
fn two(value: u16) -> Code {
    Code::two(value)
}

/// A CMap around the given body, shaped like a real one.
fn cmap(body: &str) -> String {
    format!(
        "%!PS-Adobe-3.0 Resource-CMap\n/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> def\n\
         /CMapName /Test-H def\n/CMapType 1 def\n/UIDOffset 0 def\n/XUID [1 10 100] def\n{body}\
         endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n"
    )
}

/// The very common case: every code selects itself.
fn identity_body() -> &'static str {
    "1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n1 begincidrange\n<0000> <FFFF> 0\nendcidrange\n"
}

fn read_body(body: &str) -> Option<CidMap> {
    read(cmap(body).as_bytes(), None)
}

#[test]
fn an_identity_equivalent_stream_selects_each_code_as_its_cid() {
    let map = read_body(identity_body()).expect("identity map");
    assert_eq!(map.cid(two(0x0000)), Some(0x0000));
    assert_eq!(map.cid(two(0x3042)), Some(0x3042));
    assert_eq!(map.cid(two(0xFFFF)), Some(0xFFFF));
    // Code 0 maps to CID 0, which the caller refuses; the map itself keeps it.
    assert_eq!(map.cid(two(0xFFFE)), Some(0xFFFE));
}

#[test]
fn ranges_and_single_codes_are_read() {
    let body = "1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
                1 begincidrange\n<3040> <309F> 842\nendcidrange\n\
                1 begincidchar\n<3000> 633\nendcidchar\n";
    let map = read_body(body).expect("map");
    assert_eq!(map.cid(two(0x3040)), Some(842));
    assert_eq!(map.cid(two(0x3042)), Some(844), "the CID follows its code");
    assert_eq!(map.cid(two(0x309F)), Some(842 + 0x5F));
    assert_eq!(map.cid(two(0x3000)), Some(633));
    assert_eq!(map.cid(two(0x3041)), Some(843));
    // A code inside the codespace that no entry mentions.
    assert_eq!(map.cid(two(0x4000)), None);
}

#[test]
fn a_code_outside_the_declared_codespace_selects_nothing() {
    let body = "1 begincodespacerange\n<8140> <9FFC>\nendcodespacerange\n\
                1 begincidrange\n<8100> <9FFF> 100\nendcidrange\n";
    let map = read_body(body).expect("map");
    assert_eq!(map.cid(two(0x8140)), Some(100 + 0x40), "the first byte and the second are checked");
    assert_eq!(map.cid(two(0x8240)), Some(100 + 0x140), "inside the range");
    assert_eq!(map.cid(two(0x81FD)), None, "the second byte is outside 40..=FC");
    assert_eq!(map.cid(two(0x0041)), None, "the first byte is outside 81..=9F");
}

#[test]
fn notdef_codes_select_nothing() {
    let body = "1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
                1 begincidrange\n<0020> <007E> 20\nendcidrange\n\
                1 beginnotdefrange\n<0025> <0025> 0\nendnotdefrange\n";
    // A notdef range over a mapped code is a conflict, not an override.
    assert!(read_body(body).is_none());
    let clean = "1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
                 1 begincidrange\n<0020> <007E> 20\nendcidrange\n\
                 1 beginnotdefrange\n<0000> <0002> 0\nendnotdefrange\n";
    let map = read_body(clean).expect("map");
    assert_eq!(map.cid(two(0x0001)), None, "notdef");
    assert_eq!(map.cid(two(0x0041)), Some(0x35), "20 + (0x41 - 0x20)");
}

#[test]
fn an_identity_base_fills_the_codes_the_stream_does_not_map() {
    let body = "/Identity-H usecmap\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
                1 begincidrange\n<3040> <309F> 842\nendcidrange\n";
    let map = read_body(body).expect("map");
    assert_eq!(map.cid(two(0x3042)), Some(844), "the stream's own entry wins");
    assert_eq!(map.cid(two(0x0041)), Some(0x41), "the base fills the rest");
}

#[test]
fn the_dictionary_may_name_the_base_instead() {
    let body = "1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
                1 begincidrange\n<3040> <309F> 842\nendcidrange\n";
    let map = read(cmap(body).as_bytes(), Some(b"Identity-H")).expect("map");
    assert_eq!(map.cid(two(0x3042)), Some(844));
    assert_eq!(map.cid(two(0x0041)), Some(0x41), "the base fills the rest");
}

#[test]
fn a_vertical_or_unknown_base_is_refused() {
    for base in ["/Identity-V usecmap\n", "/UniJIS-UCS2-H usecmap\n", "/90ms-RKSJ-H usecmap\n"] {
        assert!(read_body(&format!("{base}{}", identity_body())).is_none(), "{base}");
    }
    let body = identity_body();
    assert!(read(cmap(body).as_bytes(), Some(b"Identity-V")).is_none());
}

#[test]
fn vertical_writing_is_refused() {
    let body = "/WMode 1 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
                1 begincidrange\n<0000> <FFFF> 0\nendcidrange\n";
    assert!(read_body(body).is_none());
    let horizontal = format!("/WMode 0 def\n{}", identity_body());
    assert!(read_body(&horizontal).is_some());
}

#[test]
fn a_codespace_wider_than_two_bytes_is_refused() {
    let four = "1 begincodespacerange\n<00000000> <FFFFFFFF>\nendcodespacerange\n";
    assert!(read_body(four).is_none(), "four-byte codes");
    let three = "1 begincodespacerange\n<000000> <FFFFFF>\nendcodespacerange\n";
    assert!(read_body(three).is_none(), "three-byte codes");
    // The two ends of one range must agree on their width.
    let ragged = "1 begincodespacerange\n<00> <FFFF>\nendcodespacerange\n";
    assert!(read_body(ragged).is_none(), "a one-byte start and a two-byte end");
}

#[test]
fn a_one_byte_codespace_selects_one_byte_codes() {
    let body = "1 begincodespacerange\n<20> <7E>\nendcodespacerange\n\
                1 begincidrange\n<20> <7E> 1\nendcidrange\n";
    let map = read_body(body).expect("one-byte map");
    assert_eq!(map.cid(Code::one(0x41)), Some(0x22), "1 + (0x41 - 0x20)");
    assert_eq!(map.cid(Code::one(0x20)), Some(1));
    assert_eq!(map.cid(Code::one(0x7F)), None, "outside the range");
    // The same value at width two is a different code, and the map has no
    // two-byte range, so it is not admitted.
    assert_eq!(map.cid(Code::two(0x41)), None, "no two-byte range is declared");
}

#[test]
fn a_shift_jis_codespace_mixes_widths() {
    // The shape a Japanese producer writes: one-byte ASCII, then two-byte leads.
    let body = "2 begincodespacerange\n<20> <7E>\n<8140> <9FFC>\nendcodespacerange\n\
                1 begincidrange\n<20> <7E> 1\nendcidrange\n\
                1 begincidrange\n<8140> <9FFC> 633\nendcidrange\n";
    let map = read_body(body).expect("mixed map");
    assert_eq!(map.cid(Code::one(0x41)), Some(0x22));
    assert_eq!(map.cid(Code::two(0x8140)), Some(633));
    assert_eq!(map.cid(Code::two(0x8150)), Some(633 + 0x10));
    assert_eq!(map.cid(Code::two(0x41)), None, "0x41 is not a two-byte code here");
}

#[test]
fn a_one_byte_range_inside_a_two_byte_lead_is_refused() {
    // 0x81 leads a two-byte code, so a one-byte range that covers it would make a
    // viewer split the string at the wrong place.
    let body = "2 begincodespacerange\n<00> <FF>\n<8140> <9FFC>\nendcodespacerange\n\
                1 begincidrange\n<20> <7E> 1\nendcidrange\n";
    assert!(read_body(body).is_none());
    // The same overlap through an identity base is refused as well.
    let identity = "/Identity-H usecmap\n2 begincodespacerange\n<20> <7E>\n<8140> <9FFC>\nendcodespacerange\n\
                    1 begincidrange\n<20> <7E> 1\nendcidrange\n";
    assert!(read_body(identity).is_none());
}

#[test]
fn a_malformed_entry_rejects_the_map() {
    let cases = [
        // A code of the wrong width in the entry itself.
        "1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n1 begincidchar\n<41> 100\nendcidchar\n",
        // Reversed range.
        "1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n1 begincidrange\n<0100> <0010> 5\nendcidrange\n",
        // The last CID would leave the code space.
        "1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n1 begincidrange\n<0000> <FFFF> 1\nendcidrange\n",
        // Destinations that are not CIDs.
        "1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n1 begincidchar\n<0001> -1\nendcidchar\n",
        "1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n1 begincidchar\n<0001> 70000\nendcidchar\n",
        "1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n1 begincidchar\n<0001> 1.5\nendcidchar\n",
        // A block that is never closed.
        "1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n1 begincidrange\n<0000> <0001> 0\n",
        // A /ToUnicode section in the wrong slot.
        "1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n1 beginbfchar\n<0041> <0042>\nendbfchar\n",
        // No codespace and no base.
        "1 begincidrange\n<0000> <FFFF> 0\nendcidrange\n",
        // No runs and no base.
        "1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    ];
    for body in cases {
        assert!(read_body(body).is_none(), "{body}");
    }
}

#[test]
fn entries_that_disagree_are_refused_and_ones_that_agree_merge() {
    let agree = "1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
                 1 begincidrange\n<0010> <001F> 16\nendcidrange\n\
                 1 begincidchar\n<0015> 21\nendcidchar\n";
    let map = read_body(agree).expect("the same offset twice is not a conflict");
    assert_eq!(map.cid(two(0x0015)), Some(21));

    let disagree = "1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
                    1 begincidrange\n<0010> <001F> 16\nendcidrange\n\
                    1 begincidchar\n<0015> 99\nendcidchar\n";
    assert!(read_body(disagree).is_none());
}

#[test]
fn books_and_streams_are_bounded() {
    assert!(read(&[], None).is_none(), "an empty program has no codespace");
    assert!(read(&vec![b' '; super::super::MAX_STREAM + 1], None).is_none());
    // Garbage is not a crash and not a map.
    assert!(read(&[0x00, 0xFF, b'<', 0x80, b'>'], None).is_none());
    assert!(read(b"<<<<", None).is_none());
}

#[test]
fn the_probe_answers_without_panicking() {
    let map = read_body(identity_body()).expect("map");
    assert!(map.probe() > 0);
}
#[test]
fn a_full_ps_adobe_cmap_is_read() {
    let body = "begincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (Japan1) /Supplement 2 >> def\n\
     /CMapName /Test-RKSJ-H def\n/CMapType 1 def\n\
     1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
     1 begincidrange\n<3040> <309F> 842\nendcidrange\n\
     1 begincidchar\n<3000> 633\nendcidchar\n\
     endcmap\nCMapName currentdict /CMap defineresource pop\n";
    let map = read(body.as_bytes(), None);
    let map = map.expect("a header like a real CMap does not stop the reader");
    assert_eq!(map.cid(two(0x3042)), Some(844));
    assert_eq!(map.cid(two(0x3000)), Some(633));
}
