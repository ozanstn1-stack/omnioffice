//! One note's view of a two-byte font: characters mapped to one-byte slots so
//! the layout can keep speaking [`FontMetrics`].
//!
//! Free-text layout works in one-byte codes: the wrapping and measuring code
//! speaks `FontMetrics`, whose `encode` returns a `u8`. Two fonts address glyphs
//! with two bytes - the bundled face, whose slots are subset glyph ids, and a
//! composite font of the document, whose codes are CIDs. Both reach the page the
//! same way: the layout gets dense slot codes, and only the emission turns slots
//! back into the real two-byte code of the font that will draw them.
//!
//! The space keeps slot `0x20` because the wrapper writes that byte between two
//! words without asking the metrics. A note with more than 256 distinct
//! characters gives up, which is today's behaviour for text no font could show.

use std::collections::{BTreeMap, BTreeSet};

use super::super::textfit::FontMetrics;
use super::HELVETICA;
use crate::textfont::SubsetFace;

/// The code the wrapper writes for the space between two words.
pub(crate) const SPACE: u8 = b' ';

/// The font whose codes the slots stand for.
pub(crate) enum GlyphSource {
    /// The bundled face, written into the document as a subset.
    Bundled(&'static SubsetFace),
}

/// Slot -> character, and slot -> advance, for one note and one font.
pub(crate) struct SlotMetrics {
    source: GlyphSource,
    /// Slot -> character, in first-seen order (0x20 is the space).
    chars: Vec<char>,
    /// Character -> slot.
    slots: BTreeMap<char, u8>,
    /// Slot -> advance in 1/1000 em, for `width`.
    widths: [f32; 256],
    ascent: f32,
}

impl SlotMetrics {
    /// `None` when the font cannot show the whole text or the note has more
    /// distinct characters than a byte can address.
    pub(crate) fn new(face: &'static SubsetFace, text: &str) -> Option<Self> {
        let content = visible(text);
        if !face.can_encode(&content) {
            return None;
        }
        let (chars, slots) = assign_slots(&content)?;
        let mut widths = [0.0f32; 256];
        for (symbol, slot) in &slots {
            widths[usize::from(*slot)] = face.measure(&symbol.to_string(), 1000.0) as f32;
        }
        Some(Self { source: GlyphSource::Bundled(face), chars, slots, widths, ascent: f32::from(HELVETICA.ascent()) })
    }

    /// The `<hex>` operand for one line of slots.
    pub(crate) fn operand(&self, codes: &[u8]) -> String {
        match self.source {
            GlyphSource::Bundled(face) => {
                let text: String = codes.iter().filter_map(|code| self.chars.get(usize::from(*code))).collect();
                face.encode(&text)
            }
        }
    }

    /// The characters the font has to carry for this note.
    pub(crate) fn charset(&self) -> BTreeSet<char> {
        self.chars.iter().copied().collect()
    }
}

impl FontMetrics for SlotMetrics {
    fn encode(&self, symbol: char) -> Option<u8> {
        self.slots.get(&symbol).copied()
    }

    fn width(&self, code: u8) -> f32 {
        self.widths[usize::from(code)]
    }

    fn ascent(&self) -> f32 {
        self.ascent
    }
}

/// The text a note actually draws: line breaks and tabs are layout, not glyphs.
fn visible(text: &str) -> String {
    text.chars().filter(|symbol| !matches!(symbol, '\n' | '\r' | '\u{2028}' | '\t')).collect()
}

/// Dense slots for every character of `content`, with the space pinned to
/// [`SPACE`].
fn assign_slots(content: &str) -> Option<(Vec<char>, BTreeMap<char, u8>)> {
    let mut slots: BTreeMap<char, u8> = BTreeMap::new();
    let mut chars: Vec<char> = Vec::new();
    slots.insert(' ', SPACE);
    chars.push(' ');
    for symbol in content.chars() {
        if symbol == ' ' || slots.contains_key(&symbol) {
            continue;
        }
        let slot = u8::try_from(chars.len()).ok()?;
        if slot == SPACE {
            return None;
        }
        slots.insert(symbol, slot);
        chars.push(symbol);
    }
    if chars.len() > usize::from(u8::MAX) {
        return None;
    }
    Some((chars, slots))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face() -> &'static SubsetFace {
        SubsetFace::bundled().expect("bundled face")
    }

    #[test]
    fn slots_are_dense_and_the_space_keeps_its_byte() {
        let metrics = SlotMetrics::new(face(), "аб ба").expect("slots");
        assert_eq!(metrics.encode(' '), Some(SPACE), "the wrapper writes this byte itself");
        let first = metrics.encode('а').expect("slot");
        let second = metrics.encode('б').expect("slot");
        assert_ne!(first, SPACE);
        assert_ne!(second, SPACE);
        assert_ne!(first, second);
        assert_eq!(metrics.encode('в'), None, "a character outside the note has no slot");
    }

    #[test]
    fn slot_widths_are_the_face_advances() {
        let metrics = SlotMetrics::new(face(), "Aб").expect("slots");
        for symbol in ['A', 'б', ' '] {
            let slot = metrics.encode(symbol).expect("slot");
            let expected = face().measure(&symbol.to_string(), 1000.0) as f32;
            assert!((metrics.width(slot) - expected).abs() < 0.01, "{symbol}: {}", metrics.width(slot));
        }
        assert!((metrics.ascent() - f32::from(HELVETICA.ascent())).abs() < 0.01);
    }

    #[test]
    fn a_note_the_face_cannot_show_has_no_slots() {
        assert!(SlotMetrics::new(face(), "報告").is_none(), "no CJK glyphs in the bundled face");
        // An unmappable character in the middle drops the whole note, as before.
        assert!(SlotMetrics::new(face(), "Сл😀во").is_none());
    }

    #[test]
    fn more_than_a_byte_of_characters_gives_up() {
        // 300 characters the face really has, so the cap is what refuses them.
        let mut long = String::new();
        for code in 0x20u32..0x2000 {
            let Some(symbol) = char::from_u32(code) else { continue };
            if face().can_encode(&symbol.to_string()) {
                long.push(symbol);
                if long.chars().count() >= 300 {
                    break;
                }
            }
        }
        assert_eq!(long.chars().count(), 300);
        assert!(SlotMetrics::new(face(), &long).is_none());
    }

    #[test]
    fn the_operand_round_trips_through_the_face() {
        let metrics = SlotMetrics::new(face(), "Привет").expect("slots");
        let codes: Vec<u8> = "Привет".chars().filter_map(|symbol| metrics.encode(symbol)).collect();
        assert_eq!(metrics.operand(&codes), face().encode("Привет"));
        let mut expected: BTreeSet<char> = "Привет".chars().collect();
        expected.insert(' '); // the space always has a slot
        assert_eq!(metrics.charset(), expected);
    }
}
