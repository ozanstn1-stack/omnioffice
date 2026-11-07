//! Text decoding, encoding and line breaking for synthesized `FreeText`.
//!
//! The standard fonts only cover Latin-1 (WinAnsi); the glyphs PDF's Latin
//! files need but WinAnsi lacks are carried at private codes declared through
//! `/Differences`. A character outside that set cannot be shown without an
//! embedded font, so the caller drops the annotation instead of printing a
//! placeholder.

use super::std14::{Std14, EXTRA_BASE, EXTRA_GLYPHS};

/// The first private code the extra glyphs use.
pub(crate) fn extra_base() -> u8 {
    EXTRA_BASE
}

/// Unicode symbols for the glyphs in [`EXTRA_GLYPHS`], same order.
const EXTRA_SYMBOLS: [char; 36] = [
    'Ğ', 'ğ', 'İ', 'ı', 'Ş', 'ş', 'Ć', 'ć', 'Č', 'č', 'Ď', 'ď', 'Ě', 'ě', 'Ę', 'ę', 'Ł', 'ł', 'Ń', 'ń', 'Ŕ', 'ŕ', 'Ř',
    'ř', 'Ś', 'ś', 'Ů', 'ů', 'Ź', 'ź', 'Ż', 'ż', 'Ą', 'ą', 'Ă', 'ă',
];

/// PDFDocEncoding for 0x80..=0x9F; the rest is Latin-1 with 0xA0 as the euro.
const PDF_DOC_HIGH: [char; 32] = [
    '†', '‡', '…', '—', '–', 'ƒ', '⁄', '‹', '›', '−', '‰', '„', '“', '”', '‘', '’', '‚', '™', 'ﬁ', 'ﬂ', 'Ł', 'Œ', 'Š',
    'Ÿ', 'Ž', 'ı', 'ł', 'œ', 'š', 'ž', '\u{0}', '€',
];

/// Decodes a PDF text string: UTF-16BE with a BOM, UTF-8 with a BOM (PDF 2.0),
/// or PDFDocEncoding.
pub(super) fn decode(raw: &[u8]) -> Option<String> {
    if let Some(rest) = raw.strip_prefix(&[0xFE, 0xFF]) {
        if !rest.len().is_multiple_of(2) {
            return None;
        }
        let units: Vec<u16> = rest.chunks(2).map(|pair| u16::from_be_bytes([pair[0], pair[1]])).collect();
        return String::from_utf16(&units).ok();
    }
    if let Some(rest) = raw.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8(rest.to_vec()).ok();
    }
    let mut out = String::with_capacity(raw.len());
    for byte in raw {
        let symbol = match *byte {
            0x80..=0x9F => PDF_DOC_HIGH[usize::from(*byte - 0x80)],
            0xA0 => '€',
            other => char::from(other),
        };
        out.push(symbol);
    }
    Some(out)
}

/// Encodes text in WinAnsi plus the private extra codes; `None` when a
/// character has no representation.
pub(super) fn encode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len());
    for symbol in text.chars() {
        if let Some(index) = EXTRA_SYMBOLS.iter().position(|candidate| *candidate == symbol) {
            out.push(EXTRA_BASE + index as u8);
            continue;
        }
        out.push(win_ansi(symbol)?);
    }
    Some(out)
}

/// The WinAnsi code of one character, when it has one.
fn win_ansi(symbol: char) -> Option<u8> {
    // Latin-1 covers 0x20..=0x7E and 0xA0..=0xFF; the 0x80 block is sparse.
    let code = u32::from(symbol);
    if (0x20..=0x7E).contains(&code) {
        return Some(code as u8);
    }
    if (0xA1..=0xFF).contains(&code) && symbol != '\u{AD}' {
        return Some(code as u8);
    }
    Some(match symbol {
        '€' => 0x80,
        '‚' => 0x82,
        'ƒ' => 0x83,
        '„' => 0x84,
        '…' => 0x85,
        '†' => 0x86,
        '‡' => 0x87,
        'ˆ' => 0x88,
        '‰' => 0x89,
        'Š' => 0x8A,
        '‹' => 0x8B,
        'Œ' => 0x8C,
        'Ž' => 0x8E,
        '‘' => 0x91,
        '’' => 0x92,
        '“' => 0x93,
        '”' => 0x94,
        '•' => 0x95,
        '–' => 0x96,
        '—' => 0x97,
        '˜' => 0x98,
        '™' => 0x99,
        'š' => 0x9A,
        '›' => 0x9B,
        'œ' => 0x9C,
        'ž' => 0x9E,
        'Ÿ' => 0x9F,
        '\u{0}' => return None,
        _ => return None,
    })
}

/// Width of encoded text in 1/1000 em units.
pub(super) fn width(font: Std14, bytes: &[u8]) -> f32 {
    bytes.iter().map(|code| f32::from(font.width(*code))).sum()
}

/// The glyph names the font dictionary must declare through `/Differences`,
/// starting at [`EXTRA_BASE`].
pub(crate) fn differences() -> Vec<&'static str> {
    EXTRA_GLYPHS.iter().map(|(name, _)| *name).collect()
}

/// Wraps `text` into lines that fit `max_width` points at `size` points.
///
/// Greedy word wrapping; a single word wider than the box is broken by
/// character. An empty paragraph keeps its empty line.
pub(super) fn wrap(font: Std14, size: f32, text: &str, max_width: f32, max_lines: usize) -> Vec<Vec<u8>> {
    let mut lines: Vec<Vec<u8>> = Vec::new();
    for paragraph in text.split(['\n', '\r', '\u{2028}']) {
        if lines.len() >= max_lines {
            break;
        }
        let paragraph = paragraph.replace('\t', " ");
        let mut current: Vec<u8> = Vec::new();
        for word in paragraph.split(' ') {
            let encoded = encode(word);
            let Some(encoded) = encoded else { continue };
            let mut candidate = current.clone();
            if !candidate.is_empty() {
                candidate.push(b' ');
            }
            candidate.extend_from_slice(&encoded);
            if width(font, &candidate) * size / 1000.0 <= max_width {
                current = candidate;
                continue;
            }
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
                if lines.len() >= max_lines {
                    return lines;
                }
            }
            if width(font, &encoded) * size / 1000.0 <= max_width {
                current = encoded;
                continue;
            }
            // A single word wider than the box: break it by character.
            for symbol in word.chars() {
                let piece = match encode(&symbol.to_string()) {
                    Some(piece) => piece,
                    None => continue,
                };
                let mut grown = current.clone();
                grown.extend_from_slice(&piece);
                if width(font, &grown) * size / 1000.0 > max_width && !current.is_empty() {
                    lines.push(std::mem::take(&mut current));
                    if lines.len() >= max_lines {
                        return lines;
                    }
                }
                current.extend_from_slice(&piece);
            }
        }
        lines.push(current);
    }
    lines.truncate(max_lines);
    lines
}

#[cfg(test)]
mod tests {
    use super::super::std14::{Family, Std14};
    use super::*;

    #[test]
    fn text_strings_decode_from_their_encodings() {
        assert_eq!(decode(&[0xFE, 0xFF, 0x00, 0x41]).as_deref(), Some("A"));
        assert_eq!(decode(b"\xEF\xBB\xBFmerhaba").as_deref(), Some("merhaba"));
        assert_eq!(decode(&[0x80, 0x41]).as_deref(), Some("†A"));
        assert_eq!(decode(&[0xA0]).as_deref(), Some("€"));
        assert_eq!(decode(&[0xFE, 0xFF, 0x00]).as_deref(), None);
    }

    #[test]
    fn text_encodes_through_win_ansi_and_the_extra_codes() {
        let encoded = encode("Merhaba ğüşİı").expect("encodable");
        assert_eq!(encoded[0], b'M');
        assert!(encoded.contains(&3), "the dotted capital I uses its private code");
        assert_eq!(encode("漢"), None);
        assert_eq!(win_ansi('A'), Some(b'A'));
        assert_eq!(win_ansi('€'), Some(0x80));
        assert_eq!(win_ansi('\u{AD}'), None);
    }

    #[test]
    fn widths_match_the_afm() {
        let font = Std14 { family: Family::Helvetica, bold: false, italic: false };
        let bytes = encode("Hello").unwrap();
        assert!((width(font, &bytes) - 2278.0).abs() < 0.01, "Helvetica 10pt: {}", width(font, &bytes));
        assert_eq!(font.width(b' '), 278);
        assert_eq!(font.width(b'i'), 222);
        assert_eq!(font.width(b'm'), 833);
    }

    #[test]
    fn wrapping_respects_the_box_and_the_line_limit() {
        let font = Std14 { family: Family::Helvetica, bold: false, italic: false };
        let lines = wrap(font, 10.0, "hello world again", 40.0, 10);
        assert!(lines.len() > 1, "the text does not fit one line: {lines:?}");
        for line in &lines {
            assert!(width(font, line) * 10.0 / 1000.0 <= 40.0 + 0.01, "line {line:?} is too wide");
        }
        let joined: String =
            lines.iter().map(|line| String::from_utf8_lossy(line).to_string()).collect::<Vec<_>>().join(" ");
        assert_eq!(joined, "hello world again");
        let limited = wrap(font, 10.0, "a b c d e f g h", 20.0, 2);
        assert_eq!(limited.len(), 2, "the line limit is honoured");
        let broken = wrap(font, 10.0, "mmmmmmmmmmmm", 5.0, 20);
        assert!(broken.len() > 1, "a long word is broken: {broken:?}");
        let empty = wrap(font, 10.0, "", 100.0, 4);
        assert_eq!(empty.len(), 1, "an empty paragraph keeps its line");
    }

    #[test]
    fn every_extra_glyph_is_declared_once() {
        let names = differences();
        assert_eq!(names.len(), 36);
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len(), "no duplicate glyph names");
        assert!(names.contains(&"gbreve") && names.contains(&"Scedilla"));
    }
}
