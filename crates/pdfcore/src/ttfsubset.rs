//! Glyph-preserving TrueType subsetting for the programs that
//! [`crate::fontembed`] writes into converted documents.
//!
//! "Glyph-preserving" means glyph ids never change: every requested glyph
//! keeps its outline at its original index and every other glyph becomes an
//! empty (zero-length) `glyf` entry. Content streams, `/Widths`, the font's
//! own `cmap` and any `/CIDToGIDMap` therefore stay valid without rewriting.
//! The kept set is closed over composite-glyph components and always
//! contains `.notdef` (glyph 0).
//!
//! The output keeps the tables a PDF consumer needs to rasterize a TrueType
//! program: `cmap`, `cvt `, `fpgm`, `glyf`, `head`, `hhea`, `hmtx`, `loca`,
//! `maxp`, `name`, `OS/2`, `post` and `prep` (each optional one only when
//! present). Layout and auxiliary tables (GSUB, GPOS, GDEF, kern, DSIG, hdmx,
//! LTSH, gasp, FFTM...) are dropped: PDF text is already positioned, so no
//! viewer consults them. `post` is cut to its 32-byte version 3.0 header,
//! which drops the glyph-name list (about 5 KB per bundled face); PDF viewers
//! reach the glyphs of an embedded TrueType program through `cmap` or a
//! CID-to-GID map, not through `post` names.
//!
//! `loca` keeps the source's short or long format unless the rebuilt `glyf`
//! no longer fits the short format. The table directory is rebuilt sorted by
//! tag with 4-byte aligned table data and fresh per-table checksums, and
//! `head.checkSumAdjustment` is recomputed over the whole file.
//!
//! The reader is bounds-checked throughout and never panics. A program it
//! cannot parse comes back as a [`SubsetError`] and the caller embeds the full
//! program instead. CFF-flavoured OpenType (`OTTO`) and collections (`ttcf`)
//! are rejected because they have no `glyf`/`loca` pair to subset.

use std::collections::BTreeSet;
use std::fmt;

/// Tables copied into a subset; everything else is dropped.
const KEPT_TABLES: [&[u8; 4]; 13] = [
    b"OS/2", b"cmap", b"cvt ", b"fpgm", b"glyf", b"head", b"hhea", b"hmtx", b"loca", b"maxp", b"name", b"post", b"prep",
];

/// Tables without which the program is not a renderable TrueType font.
const REQUIRED_TABLES: [&[u8; 4]; 7] = [b"cmap", b"glyf", b"head", b"hhea", b"hmtx", b"loca", b"maxp"];

/// The sfnt format allows 65535 tables; real fonts carry fewer than 50.
const MAX_TABLES: usize = 512;

/// A composite glyph with more components than this is treated as corrupt.
const MAX_COMPONENTS: usize = 4096;

// Composite glyph flags (OpenType `glyf` specification).
const ARG_1_AND_2_ARE_WORDS: u16 = 0x0001;
const WE_HAVE_A_SCALE: u16 = 0x0008;
const MORE_COMPONENTS: u16 = 0x0020;
const WE_HAVE_AN_X_AND_Y_SCALE: u16 = 0x0040;
const WE_HAVE_A_TWO_BY_TWO: u16 = 0x0080;

/// Why a program could not be subset. The caller falls back to embedding the
/// full program and may report the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubsetError(&'static str);

impl SubsetError {
    pub fn reason(&self) -> &'static str {
        self.0
    }
}

impl fmt::Display for SubsetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for SubsetError {}

/// A rebuilt TrueType program and the glyphs whose outlines it kept.
#[derive(Debug, Clone)]
pub struct TrueTypeSubset {
    /// The complete sfnt program, ready for a `FontFile2` stream.
    pub program: Vec<u8>,
    /// Every glyph id that kept its outline: the requested ids that exist in
    /// the font, `.notdef` and all composite components.
    pub glyphs: BTreeSet<u16>,
}

/// Subsets `program` to `glyphs` without renumbering. Requested ids at or
/// beyond the font's `numGlyphs` are ignored.
pub fn subset_truetype(program: &[u8], glyphs: &BTreeSet<u16>) -> Result<TrueTypeSubset, SubsetError> {
    let tables = read_table_directory(program)?;
    for tag in REQUIRED_TABLES {
        if !tables.iter().any(|table| &table.tag == tag) {
            return Err(SubsetError("a table required for a TrueType program is missing"));
        }
    }
    let table = |tag: &[u8; 4]| tables.iter().find(|table| &table.tag == tag).map(|table| table.data);
    let head = table(b"head").unwrap_or_default();
    let maxp = table(b"maxp").unwrap_or_default();
    let loca = table(b"loca").unwrap_or_default();
    let glyf = table(b"glyf").unwrap_or_default();

    if head.len() < 54 || read_u32(head, 12) != Some(0x5F0F_3CF5) {
        return Err(SubsetError("the head table is malformed"));
    }
    let long_loca = match read_u16(head, 50) {
        Some(0) => false,
        Some(1) => true,
        _ => return Err(SubsetError("head.indexToLocFormat is neither short nor long")),
    };
    let glyph_count = read_u16(maxp, 4).ok_or(SubsetError("the maxp table is malformed"))? as usize;
    if glyph_count == 0 {
        return Err(SubsetError("the font declares no glyphs"));
    }
    let offsets = read_loca(loca, glyf.len(), glyph_count, long_loca)?;

    // Closure over composite components, .notdef included.
    let mut kept: BTreeSet<u16> = BTreeSet::new();
    let mut pending: Vec<u16> = vec![0];
    pending.extend(glyphs.iter().copied().filter(|glyph| (*glyph as usize) < glyph_count));
    while let Some(glyph) = pending.pop() {
        if !kept.insert(glyph) {
            continue;
        }
        for component in composite_components(glyph_data(glyf, &offsets, glyph))? {
            if component as usize >= glyph_count {
                return Err(SubsetError("a composite glyph refers to a glyph id past numGlyphs"));
            }
            if !kept.contains(&component) {
                pending.push(component);
            }
        }
    }

    // Rebuild glyf and loca with unchanged glyph ids.
    let alignment = if long_loca { 4 } else { 2 };
    let mut new_glyf: Vec<u8> = Vec::new();
    let mut new_offsets: Vec<usize> = Vec::with_capacity(glyph_count + 1);
    for index in 0..glyph_count {
        new_offsets.push(new_glyf.len());
        let glyph = index as u16;
        if kept.contains(&glyph) {
            new_glyf.extend_from_slice(glyph_data(glyf, &offsets, glyph));
            while !new_glyf.len().is_multiple_of(alignment) {
                new_glyf.push(0);
            }
        }
    }
    new_offsets.push(new_glyf.len());
    // Short offsets store offset / 2 in a u16. Every offset is even (2-byte
    // alignment above), so only the total size can force the long format.
    let write_long = long_loca || new_glyf.len() > 0x1_FFFE;
    let mut new_loca: Vec<u8> = Vec::with_capacity(new_offsets.len() * if write_long { 4 } else { 2 });
    for offset in &new_offsets {
        if write_long {
            new_loca.extend_from_slice(&(*offset as u32).to_be_bytes());
        } else {
            new_loca.extend_from_slice(&((*offset / 2) as u16).to_be_bytes());
        }
    }

    let mut out_tables: Vec<([u8; 4], Vec<u8>)> = Vec::with_capacity(KEPT_TABLES.len());
    for source in &tables {
        if !KEPT_TABLES.contains(&&source.tag) {
            continue;
        }
        let data = match &source.tag {
            b"glyf" => std::mem::take(&mut new_glyf),
            b"loca" => std::mem::take(&mut new_loca),
            b"head" => {
                let mut head = source.data.to_vec();
                // checkSumAdjustment is computed once the file is assembled.
                head[8..12].fill(0);
                head[50..52].copy_from_slice(&(write_long as u16).to_be_bytes());
                head
            }
            b"post" => {
                // Version 3.0: the fixed header only, no glyph names.
                if source.data.len() < 32 {
                    continue;
                }
                let mut post = source.data[..32].to_vec();
                post[0..4].copy_from_slice(&0x0003_0000u32.to_be_bytes());
                post
            }
            _ => source.data.to_vec(),
        };
        out_tables.push((source.tag, data));
    }

    let program = assemble(out_tables);
    Ok(TrueTypeSubset { program, glyphs: kept })
}

struct TableRecord<'a> {
    tag: [u8; 4],
    data: &'a [u8],
}

fn read_table_directory(program: &[u8]) -> Result<Vec<TableRecord<'_>>, SubsetError> {
    match read_u32(program, 0) {
        Some(0x0001_0000) | Some(0x7472_7565) => {}
        Some(0x4F54_544F) => return Err(SubsetError("CFF-flavoured OpenType has no glyf table to subset")),
        Some(0x7474_6366) => return Err(SubsetError("font collections are not subset")),
        _ => return Err(SubsetError("the program is not a TrueType sfnt")),
    }
    let count = read_u16(program, 4).ok_or(SubsetError("the table directory is truncated"))? as usize;
    if count == 0 || count > MAX_TABLES {
        return Err(SubsetError("the table directory has an implausible table count"));
    }
    let mut tables: Vec<TableRecord<'_>> = Vec::with_capacity(count);
    for index in 0..count {
        let start = 12 + index * 16;
        let record = program.get(start..start + 16).ok_or(SubsetError("the table directory is truncated"))?;
        let tag = [record[0], record[1], record[2], record[3]];
        let (Some(offset), Some(length)) = (read_u32(record, 8), read_u32(record, 12)) else {
            return Err(SubsetError("the table directory is truncated"));
        };
        let offset = offset as usize;
        let end = offset.checked_add(length as usize).ok_or(SubsetError("a table record overflows"))?;
        let data = program.get(offset..end).ok_or(SubsetError("a table runs past the end of the program"))?;
        if tables.iter().any(|table| table.tag == tag) {
            return Err(SubsetError("the table directory lists a table twice"));
        }
        tables.push(TableRecord { tag, data });
    }
    Ok(tables)
}

/// Reads `glyph_count + 1` glyph offsets and checks that they are ordered and
/// inside `glyf`, so later slicing cannot fail.
fn read_loca(loca: &[u8], glyf_len: usize, glyph_count: usize, long: bool) -> Result<Vec<usize>, SubsetError> {
    let mut offsets: Vec<usize> = Vec::with_capacity(glyph_count + 1);
    for index in 0..=glyph_count {
        let offset = if long {
            read_u32(loca, index * 4).map(|value| value as usize)
        } else {
            read_u16(loca, index * 2).map(|value| value as usize * 2)
        };
        let offset = offset.ok_or(SubsetError("the loca table is shorter than numGlyphs requires"))?;
        if offsets.last().is_some_and(|previous| offset < *previous) {
            return Err(SubsetError("the loca offsets are not in ascending order"));
        }
        if offset > glyf_len {
            return Err(SubsetError("a loca offset points past the end of glyf"));
        }
        offsets.push(offset);
    }
    Ok(offsets)
}

/// One glyph's `glyf` bytes. `read_loca` validated that the offsets are
/// ordered and inside `glyf`, so the fallback is never taken in practice.
fn glyph_data<'a>(glyf: &'a [u8], offsets: &[usize], glyph: u16) -> &'a [u8] {
    let index = glyph as usize;
    match (offsets.get(index), offsets.get(index + 1)) {
        (Some(&start), Some(&end)) => glyf.get(start..end).unwrap_or_default(),
        _ => &[],
    }
}

/// The glyph ids a composite glyph refers to; empty for simple and empty
/// glyphs.
fn composite_components(data: &[u8]) -> Result<Vec<u16>, SubsetError> {
    if data.is_empty() {
        return Ok(Vec::new());
    }
    let contours = read_u16(data, 0).ok_or(SubsetError("a glyph header is truncated"))? as i16;
    if contours >= 0 {
        return Ok(Vec::new());
    }
    let truncated = SubsetError("a composite glyph is truncated");
    let mut components: Vec<u16> = Vec::new();
    // Component records start after the 10-byte glyph header.
    let mut offset = 10usize;
    loop {
        let (Some(flags), Some(glyph)) = (read_u16(data, offset), read_u16(data, offset + 2)) else {
            return Err(truncated);
        };
        components.push(glyph);
        if components.len() > MAX_COMPONENTS {
            return Err(SubsetError("a composite glyph has an implausible number of components"));
        }
        offset += 4;
        offset += if flags & ARG_1_AND_2_ARE_WORDS != 0 { 4 } else { 2 };
        if flags & WE_HAVE_A_SCALE != 0 {
            offset += 2;
        } else if flags & WE_HAVE_AN_X_AND_Y_SCALE != 0 {
            offset += 4;
        } else if flags & WE_HAVE_A_TWO_BY_TWO != 0 {
            offset += 8;
        }
        if offset > data.len() {
            return Err(truncated);
        }
        if flags & MORE_COMPONENTS == 0 {
            break;
        }
    }
    Ok(components)
}

/// Writes the sfnt: offset table, sorted table records, 4-byte aligned table
/// data, per-table checksums and finally `head.checkSumAdjustment`.
fn assemble(mut tables: Vec<([u8; 4], Vec<u8>)>) -> Vec<u8> {
    tables.sort_by_key(|table| table.0);
    let count = tables.len();
    let mut entry_selector = 0u16;
    while (2usize << entry_selector) <= count {
        entry_selector += 1;
    }
    let search_range = (1u16 << entry_selector) * 16;
    let range_shift = (count as u16) * 16 - search_range;

    let directory_size = 12 + count * 16;
    let data_size: usize = tables.iter().map(|(_, data)| padded_len(data.len())).sum();
    let mut out: Vec<u8> = Vec::with_capacity(directory_size + data_size);
    out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    out.extend_from_slice(&(count as u16).to_be_bytes());
    out.extend_from_slice(&search_range.to_be_bytes());
    out.extend_from_slice(&entry_selector.to_be_bytes());
    out.extend_from_slice(&range_shift.to_be_bytes());

    let mut offset = directory_size;
    let mut head_offset: Option<usize> = None;
    for (tag, data) in &tables {
        if tag == b"head" {
            head_offset = Some(offset);
        }
        out.extend_from_slice(tag);
        out.extend_from_slice(&checksum(data).to_be_bytes());
        out.extend_from_slice(&(offset as u32).to_be_bytes());
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        offset += padded_len(data.len());
    }
    for (_, data) in &tables {
        out.extend_from_slice(data);
        out.resize(padded_len(out.len()), 0);
    }

    if let Some(head_offset) = head_offset {
        let adjustment = 0xB1B0_AFBAu32.wrapping_sub(checksum(&out));
        if let Some(slot) = out.get_mut(head_offset + 8..head_offset + 12) {
            slot.copy_from_slice(&adjustment.to_be_bytes());
        }
    }
    out
}

fn padded_len(length: usize) -> usize {
    length.div_ceil(4) * 4
}

/// The sfnt table checksum: the wrapping sum of big-endian u32 words, with
/// the final partial word zero-padded.
pub fn checksum(data: &[u8]) -> u32 {
    let (words, rest) = data.as_chunks::<4>();
    let mut sum = words.iter().fold(0u32, |sum, word| sum.wrapping_add(u32::from_be_bytes(*word)));
    if !rest.is_empty() {
        let mut last = [0u8; 4];
        last[..rest.len()].copy_from_slice(rest);
        sum = sum.wrapping_add(u32::from_be_bytes(last));
    }
    sum
}

fn read_u16(data: &[u8], offset: usize) -> Option<u16> {
    let bytes = data.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_be_bytes([bytes[0], bytes[1]]))
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIBERATION_SANS: &[u8] = include_bytes!("../assets/fonts/LiberationSans-Regular.ttf");
    const PT_SANS: &[u8] = include_bytes!("../assets/fonts/PT_Sans-Web-Regular.ttf");

    fn table<'a>(program: &'a [u8], tag: &[u8; 4]) -> Option<&'a [u8]> {
        read_table_directory(program).ok()?.into_iter().find(|table| &table.tag == tag).map(|table| table.data)
    }

    #[test]
    fn whole_file_checksum_matches_the_adjustment() {
        let subset = subset_truetype(LIBERATION_SANS, &BTreeSet::from([36u16, 68])).expect("subset");
        // With checkSumAdjustment in place the whole file sums to the magic.
        assert_eq!(checksum(&subset.program), 0xB1B0_AFBA);
        let directory = read_table_directory(&subset.program).expect("directory");
        let mut tags: Vec<[u8; 4]> = directory.iter().map(|table| table.tag).collect();
        let sorted = {
            let mut copy = tags.clone();
            copy.sort();
            copy
        };
        assert_eq!(tags, sorted, "table records must be sorted by tag");
        tags.retain(|tag| !KEPT_TABLES.contains(&tag));
        assert!(tags.is_empty(), "only the kept tables survive: {tags:?}");
        for (index, table) in directory.iter().enumerate() {
            let record = &subset.program[12 + index * 16..28 + index * 16];
            let offset = read_u32(record, 8).unwrap() as usize;
            assert_eq!(offset % 4, 0, "table data must be 4-byte aligned");
            if &record[0..4] != b"head" {
                assert_eq!(read_u32(record, 4).unwrap(), checksum(table.data));
            }
        }
    }

    #[test]
    fn both_loca_formats_round_trip() {
        for (program, long) in [(LIBERATION_SANS, false), (PT_SANS, true)] {
            let subset = subset_truetype(program, &BTreeSet::from([40u16])).expect("subset");
            let head = table(&subset.program, b"head").expect("head");
            assert_eq!(read_u16(head, 50), Some(long as u16), "the loca format is kept");
            assert!(subset.program.len() < program.len() / 3, "the subset is much smaller");
        }
    }

    #[test]
    fn malformed_programs_are_errors_not_panics() {
        assert!(subset_truetype(b"", &BTreeSet::new()).is_err());
        assert!(subset_truetype(b"OTTO\0\x01", &BTreeSet::new()).is_err());
        for cut in [12usize, 100, 400, 4000, 9000, 60_000] {
            let _ = subset_truetype(&LIBERATION_SANS[..cut], &BTreeSet::from([1u16, 2, 3]));
        }
        let mut corrupt = LIBERATION_SANS.to_vec();
        // Scramble the loca table: the offsets stop being monotonic.
        let loca = table(LIBERATION_SANS, b"loca").unwrap();
        let start = loca.as_ptr() as usize - LIBERATION_SANS.as_ptr() as usize;
        corrupt[start + 2..start + 40].fill(0xFF);
        assert!(subset_truetype(&corrupt, &BTreeSet::from([5u16])).is_err());
    }
}
