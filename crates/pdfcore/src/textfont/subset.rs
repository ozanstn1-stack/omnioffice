//! The bundled face as a reusable subset: metrics without a document, the
//! glyph-id encoding for text, and the subset font the writer objects come
//! from. `TextFont` in the parent module is the object-oriented wrapper the
//! stamp and form paths use; the imposition reads a face here first (it has no
//! document while it measures) and only writes the font when it emits.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};

use crate::error::{PdfError, PdfResult};
use crate::ttfsubset::{checksum, subset_truetype};

/// The bundled regular face. Liberation Sans is metric-compatible with
/// Helvetica/Arial and carries the full Turkish repertoire.
const LIBERATION_SANS: &[u8] = include_bytes!("../../assets/fonts/LiberationSans-Regular.ttf");

/// The parsed bundled face: everything a measurement needs, no document.
#[derive(Clone, Debug)]
pub struct SubsetFace {
    cmap: BTreeMap<char, u16>,
    metrics: FontMetrics,
}

impl SubsetFace {
    /// The bundled face, parsed once per process.
    pub fn bundled() -> Option<&'static Self> {
        static FACE: OnceLock<Option<SubsetFace>> = OnceLock::new();
        FACE.get_or_init(|| {
            let program = LIBERATION_SANS;
            Some(Self { cmap: parse_cmap(program)?, metrics: parse_metrics(program)? })
        })
        .as_ref()
    }

    /// True when every character has a glyph in this face.
    pub fn can_encode(&self, text: &str) -> bool {
        text.chars().all(|ch| self.cmap.contains_key(&ch))
    }

    /// `<hex>` string of two-byte glyph ids for a `Tj` operand.
    pub fn encode(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.chars().count() * 4 + 2);
        out.push('<');
        for ch in text.chars() {
            let glyph = self.cmap.get(&ch).copied().unwrap_or(0);
            out.push_str(&format!("{glyph:04X}"));
        }
        out.push('>');
        out
    }

    /// Advance width of `text` in points at `size_pt`.
    pub fn measure(&self, text: &str, size_pt: f64) -> f64 {
        let units: f64 = text.chars().map(|ch| f64::from(self.advance_of(ch))).sum();
        units / f64::from(self.metrics.units_per_em.max(1)) * size_pt
    }

    /// Writes the subset font for `chars` and returns the Type0 object.
    pub fn write(&self, doc: &mut Document, chars: &BTreeSet<char>) -> PdfResult<ObjectId> {
        let program = LIBERATION_SANS;
        let mut glyphs: BTreeSet<u16> = BTreeSet::new();
        for ch in chars {
            if let Some(glyph) = self.cmap.get(ch) {
                glyphs.insert(*glyph);
            }
        }
        let subset = subset_truetype(program, &glyphs)
            .map_err(|error| PdfError::Internal(format!("the bundled font could not be subset: {error}")))?;
        write_font(doc, &subset.program, &subset.glyphs, &self.cmap, &self.metrics)
    }

    pub(crate) fn advance_of(&self, ch: char) -> u16 {
        let glyph = self.cmap.get(&ch).copied().unwrap_or(0) as usize;
        self.metrics.advances.get(glyph).copied().unwrap_or(1000)
    }
}

#[derive(Clone, Debug)]
pub(super) struct FontMetrics {
    units_per_em: u16,
    advances: Vec<u16>,
    bbox: [i16; 4],
    ascent: i16,
    descent: i16,
    cap_height: i16,
}

pub(super) fn write_font(
    doc: &mut Document,
    program: &[u8],
    glyphs: &BTreeSet<u16>,
    cmap: &BTreeMap<char, u16>,
    metrics: &FontMetrics,
) -> PdfResult<ObjectId> {
    let upem = f64::from(metrics.units_per_em.max(1));
    let scale = |value: i16| (f64::from(value) / upem * 1000.0).round() as i64;
    let tag = subset_tag(program);
    let base_font = format!("{tag}+LiberationSans");

    // FontFile2: the subset program itself.
    let mut file_stream = Stream::new(dictionary! { "Length1" => program.len() as i64 }, program.to_vec());
    file_stream.compress().map_err(|error| PdfError::from_lopdf(error, None))?;
    let file_id = doc.add_object(file_stream);

    let descriptor_id = doc.add_object(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => base_font.as_str(),
        "Flags" => 32_i64, // nonsymbolic
        "FontBBox" => vec![
            Object::Integer(scale(metrics.bbox[0])),
            Object::Integer(scale(metrics.bbox[1])),
            Object::Integer(scale(metrics.bbox[2])),
            Object::Integer(scale(metrics.bbox[3])),
        ],
        "ItalicAngle" => 0_i64,
        "Ascent" => scale(metrics.ascent),
        "Descent" => scale(metrics.descent),
        "CapHeight" => scale(metrics.cap_height),
        "StemV" => 80_i64,
        "FontFile2" => Object::Reference(file_id),
    });

    // /W: one [width] array per glyph id, in 1000-unit text space.
    let mut widths: Vec<Object> = Vec::with_capacity(glyphs.len());
    for glyph in glyphs {
        let advance = metrics.advances.get(*glyph as usize).copied().unwrap_or(1000);
        let scaled = (f64::from(advance) / upem * 1000.0).round() as i64;
        widths.push(Object::Integer(i64::from(*glyph)));
        widths.push(Object::Array(vec![Object::Integer(scaled)]));
    }

    let descendant_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "CIDFontType2",
        "BaseFont" => base_font.as_str(),
        "CIDSystemInfo" => dictionary! {
            "Registry" => Object::string_literal("Adobe"),
            "Ordering" => Object::string_literal("Identity"),
            "Supplement" => 0_i64,
        },
        "FontDescriptor" => Object::Reference(descriptor_id),
        "DW" => 1000_i64,
        "W" => Object::Array(widths),
        "CIDToGIDMap" => "Identity",
    });

    // ToUnicode: CID (= glyph id, Identity-H) back to the original character.
    let to_unicode = build_to_unicode(glyphs, cmap);
    let mut to_unicode_stream = Stream::new(Dictionary::new(), to_unicode.into_bytes());
    to_unicode_stream.compress().map_err(|error| PdfError::from_lopdf(error, None))?;
    let to_unicode_id = doc.add_object(to_unicode_stream);

    Ok(doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => base_font.as_str(),
        "Encoding" => "Identity-H",
        "DescendantFonts" => vec![Object::Reference(descendant_id)],
        "ToUnicode" => Object::Reference(to_unicode_id),
    }))
}

/// Six-letter subset tag from the program's checksum, the usual
/// `ABCDEF+FontName` convention.
fn subset_tag(program: &[u8]) -> String {
    let mut value = checksum(program);
    let mut tag = String::with_capacity(6);
    for _ in 0..6 {
        tag.push(char::from(b'A' + (value % 26) as u8));
        value = value / 26 + 7;
    }
    tag
}

/// A minimal ToUnicode CMap mapping each glyph id back to the character it
/// was requested for (several characters can share a glyph; the first wins).
fn build_to_unicode(glyphs: &BTreeSet<u16>, cmap: &BTreeMap<char, u16>) -> String {
    let mut by_glyph: BTreeMap<u16, char> = BTreeMap::new();
    for (ch, glyph) in cmap {
        if glyphs.contains(glyph) {
            by_glyph.entry(*glyph).or_insert(*ch);
        }
    }

    let mut out = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    let entries: Vec<(&u16, &char)> = by_glyph.iter().collect();
    for chunk in entries.chunks(100) {
        out.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for (glyph, ch) in chunk {
            let mut utf16 = [0u16; 2];
            let units = ch.encode_utf16(&mut utf16);
            let hex: String = units.iter().map(|unit| format!("{unit:04X}")).collect();
            out.push_str(&format!("<{glyph:04X}> <{hex}>\n"));
        }
        out.push_str("endbfchar\n");
    }
    out.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    out
}

// ---------------------------------------------------------------------------
// TrueType parsing
// ---------------------------------------------------------------------------

/// Locates an sfnt table in a TrueType program.
fn table<'a>(program: &'a [u8], tag: &[u8; 4]) -> Option<&'a [u8]> {
    let count = read_u16(program, 4)? as usize;
    if count == 0 || count > 512 {
        return None;
    }
    for index in 0..count {
        let record = program.get(12 + index * 16..28 + index * 16)?;
        if &record[0..4] != tag {
            continue;
        }
        let offset = read_u32(record, 8)? as usize;
        let length = read_u32(record, 12)? as usize;
        return program.get(offset..offset.checked_add(length)?);
    }
    None
}

/// The Windows Unicode BMP `cmap` subtable (format 4) as char -> glyph id.
fn parse_cmap(program: &[u8]) -> Option<BTreeMap<char, u16>> {
    let cmap = table(program, b"cmap")?;
    let count = read_u16(cmap, 2)? as usize;
    if count == 0 || count > 64 {
        return None;
    }
    let mut format4: Option<usize> = None;
    let mut fallback: Option<usize> = None;
    for index in 0..count {
        let record = cmap.get(4 + index * 8..12 + index * 8)?;
        let platform = read_u16(record, 0)?;
        let encoding = read_u16(record, 2)?;
        let offset = read_u32(record, 4)? as usize;
        if read_u16(cmap, offset)? != 4 {
            continue;
        }
        // (3, 1) is Windows Unicode BMP; (0, x) is Unicode platform.
        if platform == 3 && encoding == 1 {
            format4 = Some(offset);
            break;
        }
        if fallback.is_none() && (platform == 0 || platform == 3) {
            fallback = Some(offset);
        }
    }
    let start = format4.or(fallback)?;
    parse_cmap_format4(cmap, start)
}

fn parse_cmap_format4(cmap: &[u8], start: usize) -> Option<BTreeMap<char, u16>> {
    let subtable = cmap.get(start..)?;
    let seg_count_x2 = read_u16(subtable, 6)? as usize;
    if seg_count_x2 == 0 || !seg_count_x2.is_multiple_of(2) {
        return None;
    }
    let seg_count = seg_count_x2 / 2;
    let end_codes = 14;
    let start_codes = end_codes + seg_count * 2 + 2;
    let deltas = start_codes + seg_count * 2;
    let range_offsets = deltas + seg_count * 2;

    let mut map = BTreeMap::new();
    for segment in 0..seg_count {
        let end = read_u16(subtable, end_codes + segment * 2)?;
        let begin = read_u16(subtable, start_codes + segment * 2)?;
        let delta = read_i16(subtable, deltas + segment * 2)?;
        let range_offset = read_u16(subtable, range_offsets + segment * 2)? as usize;
        if begin > end {
            continue;
        }
        for code in begin..=end {
            // 0xFFFF is the required terminator segment.
            if code == 0xFFFF {
                continue;
            }
            let glyph = if range_offset == 0 {
                (code as i32 + i32::from(delta)).rem_euclid(65536) as u16
            } else {
                let address = range_offsets + segment * 2 + range_offset + (code - begin) as usize * 2;
                match read_u16(subtable, address) {
                    Some(0) | None => 0,
                    Some(glyph) => (i32::from(glyph) + i32::from(delta)).rem_euclid(65536) as u16,
                }
            };
            if glyph != 0 {
                if let Some(ch) = char::from_u32(u32::from(code)) {
                    map.entry(ch).or_insert(glyph);
                }
            }
        }
    }
    if map.is_empty() {
        None
    } else {
        Some(map)
    }
}

fn parse_metrics(program: &[u8]) -> Option<FontMetrics> {
    let head = table(program, b"head")?;
    let hhea = table(program, b"hhea")?;
    let hmtx = table(program, b"hmtx")?;
    let maxp = table(program, b"maxp")?;
    let units_per_em = read_u16(head, 18)?;
    if units_per_em == 0 {
        return None;
    }
    let glyph_count = read_u16(maxp, 4)? as usize;
    let metrics_count = read_u16(hhea, 34)? as usize;
    if metrics_count == 0 || metrics_count > glyph_count {
        return None;
    }
    let mut advances = Vec::with_capacity(glyph_count);
    let mut last = 0u16;
    for glyph in 0..glyph_count {
        if glyph < metrics_count {
            last = read_u16(hmtx, glyph * 4)?;
        }
        advances.push(last);
    }
    let bbox = [read_i16(head, 36)?, read_i16(head, 38)?, read_i16(head, 40)?, read_i16(head, 42)?];
    let ascent = read_i16(hhea, 4)?;
    let descent = read_i16(hhea, 6)?;
    let cap_height = table(program, b"OS/2")
        .and_then(|os2| {
            let version = read_u16(os2, 0)?;
            if version >= 2 {
                read_i16(os2, 88)
            } else {
                None
            }
        })
        .filter(|value| *value > 0)
        .unwrap_or((f64::from(ascent) * 0.7) as i16);
    Some(FontMetrics { units_per_em, advances, bbox, ascent, descent, cap_height })
}

fn read_u16(data: &[u8], offset: usize) -> Option<u16> {
    let bytes = data.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_be_bytes([bytes[0], bytes[1]]))
}

fn read_i16(data: &[u8], offset: usize) -> Option<i16> {
    read_u16(data, offset).map(|value| value as i16)
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_face_maps_turkish_characters() {
        let cmap = parse_cmap(LIBERATION_SANS).expect("cmap");
        for ch in "şğıİŞĞÇÖÜ".chars() {
            assert!(cmap.contains_key(&ch), "missing glyph for {ch}");
        }
        assert!(cmap.contains_key(&'A'));
    }

    #[test]
    fn the_to_unicode_cmap_covers_every_requested_character() {
        let chars: BTreeSet<char> = "Şişli Güneş İĞDIR ı".chars().collect();
        let cmap = parse_cmap(LIBERATION_SANS).expect("cmap");
        let glyphs: BTreeSet<u16> = chars.iter().filter_map(|ch| cmap.get(ch).copied()).collect();
        let to_unicode = build_to_unicode(&glyphs, &cmap);
        assert!(to_unicode.contains("beginbfchar"));
        // Every distinct glyph is mapped back to some character (plus the
        // codespacerange line, which has the same shape).
        assert_eq!(to_unicode.matches("> <").count(), glyphs.len() + 1);
    }
}
