//! Tests for the `/ToUnicode` reader: what it maps, what it refuses and how a
//! malformed entry costs only itself.

use super::*;

/// A CMap around the given body, shaped like a real one.
fn cmap(body: &str) -> String {
    format!(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n{body}\
         endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n"
    )
}

#[test]
fn a_bfchar_section_maps_the_characters_it_lists() {
    let map = read(cmap("2 beginbfchar\n<0021> <6771>\n<0022> <4eac>\nendbfchar\n").as_bytes());
    assert_eq!(map.len(), 2);
    assert_eq!(map.code('東'), Some(0x21));
    assert_eq!(map.code('京'), Some(0x22));
    assert_eq!(map.encode("東京"), Some(vec![0x00, 0x21, 0x00, 0x22]));
    assert!(map.covers("東京"));
    assert!(!map.covers("大阪"));
}

#[test]
fn a_bfrange_increments_the_destination() {
    // Three codes, three consecutive characters.
    let map = read(cmap("1 beginbfrange\n<0010> <0012> <0041>\nendbfrange\n").as_bytes());
    assert_eq!(map.code('A'), Some(0x10));
    assert_eq!(map.code('B'), Some(0x11));
    assert_eq!(map.code('C'), Some(0x12));
    assert_eq!(map.len(), 3);
}

#[test]
fn a_bfrange_array_pairs_the_destinations() {
    let map = read(cmap("1 beginbfrange\n<0020> <0022> [<0061> <0062> <0063>]\nendbfrange\n").as_bytes());
    assert_eq!(map.code('a'), Some(0x20));
    assert_eq!(map.code('c'), Some(0x22));
    // More destinations than codes are ignored, not an error.
    let extra = read(cmap("1 beginbfrange\n<0030> <0031> [<0078> <0079> <007A>]\nendbfrange\n").as_bytes());
    assert_eq!(extra.len(), 2);
    assert_eq!(extra.code('z'), None);
}

#[test]
fn a_reversed_range_maps_nothing() {
    assert!(read(cmap("1 beginbfrange\n<0025> <0020> <0041>\nendbfrange\n").as_bytes()).is_empty());
}

#[test]
fn the_lowest_code_wins_for_a_character_listed_twice() {
    let first = read(cmap("2 beginbfchar\n<0030> <0041>\n<0020> <0041>\nendbfchar\n").as_bytes());
    let second = read(cmap("2 beginbfchar\n<0020> <0041>\n<0030> <0041>\nendbfchar\n").as_bytes());
    assert_eq!(first.code('A'), Some(0x20));
    assert_eq!(second.code('A'), Some(0x20), "the order of the entries does not matter");
}

#[test]
fn a_destination_that_is_not_one_character_is_skipped() {
    // A ligature maps two scalars back to one code; there is no single
    // character to draw it with, so the entry is dropped.
    let ligature = read(cmap("1 beginbfchar\n<0003> <00660069>\nendbfchar\n").as_bytes());
    assert!(ligature.is_empty(), "a two-character destination has no inverse");
    // A lone surrogate is not text either.
    assert!(read(cmap("1 beginbfchar\n<0004> <D800>\nendbfchar\n").as_bytes()).is_empty());
    // A supplementary character is one scalar in two units: kept.
    let emoji = read(cmap("1 beginbfchar\n<0005> <D83DDE00>\nendbfchar\n").as_bytes());
    assert_eq!(emoji.code('\u{1F600}'), Some(0x05));
}

#[test]
fn placeholders_are_never_drawn() {
    for destination in ["<0000>", "<FFFD>", "<E000>", "<FFFF>"] {
        let body = format!("1 beginbfchar\n<0007> {destination}\nendbfchar\n");
        assert!(read(cmap(&body).as_bytes()).is_empty(), "{destination} was kept");
    }
}

#[test]
fn a_source_of_the_wrong_length_is_skipped() {
    // One byte cannot be an Identity-H code.
    assert!(read(cmap("1 beginbfchar\n<41> <0041>\nendbfchar\n").as_bytes()).is_empty());
    // Three hex digits are padded to two bytes, so the entry survives.
    let padded = read(cmap("1 beginbfchar\n<041> <0042>\nendbfchar\n").as_bytes());
    assert_eq!(padded.code('B'), Some(0x0410));
}

#[test]
fn a_malformed_entry_costs_only_itself() {
    let body = "2 beginbfchar\n<0021> <0041>\nnot-an-entry\n<0022> <0042>\nendbfchar\n";
    let map = read(cmap(body).as_bytes());
    assert!(map.covers("A"), "the first entry survives");
    // The broken line ends the section, so the entry after it is not read;
    // a second section is read normally.
    let recovered =
        read(cmap("1 beginbfchar\n<0021> <0041>\nendbfchar\n1 beginbfchar\n<0022> <0042>\nendbfchar\n").as_bytes());
    assert!(recovered.covers("AB"));
}

#[test]
fn comments_and_nested_dictionaries_are_ignored() {
    let body = "% a comment <0099> <0041>\n2 beginbfchar\n<0021> <0041>\n<0022> <0042>\nendbfchar\n";
    let map = read(cmap(body).as_bytes());
    assert_eq!(map.len(), 2, "{map:?}");
    assert_eq!(map.code('A'), Some(0x21), "the comment did not map anything");
}

#[test]
fn an_enormous_or_empty_stream_gives_an_empty_map() {
    assert!(read(&[]).is_empty());
    assert!(read(&vec![b' '; MAX_STREAM + 1]).is_empty());
    assert!(read(cmap("").as_bytes()).is_empty());
    // Garbage is not a crash.
    assert!(read(&[0x00, 0xFF, b'<', 0x80, b'>', b'[', b']']).is_empty());
}

#[test]
fn a_range_that_runs_out_of_the_code_space_stops() {
    // The destination would leave its two-byte width after one step.
    let map = read(cmap("1 beginbfrange\n<00FE> <0101> <FFFF>\nendbfrange\n").as_bytes());
    assert_eq!(map.code('\u{FFFF}'), None, "the noncharacter was never kept");
    assert_eq!(map.len(), 0, "the first destination is a noncharacter: {map:?}");
    // A destination whose width holds for every code maps them all.
    let short = read(cmap("1 beginbfrange\n<00FE> <0101> <FF41>\nendbfrange\n").as_bytes());
    assert_eq!(short.code('\u{FF41}'), Some(0x00FE));
    assert_eq!(short.len(), 4, "four codes fit in the destination's width: {short:?}");
}
