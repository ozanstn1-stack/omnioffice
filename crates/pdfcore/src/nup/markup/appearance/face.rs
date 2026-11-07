//! Which font a synthesized text appearance is set in, and how its characters
//! reach the page.
//!
//! Free-text notes are laid out in one-byte codes: the wrapping and measuring
//! code speaks `FontMetrics`, whose `encode` returns a `u8`. WinAnsi fits that
//! shape, and so does a simple `/DR` font of the document. Text outside those -
//! Cyrillic, Greek, Latin beyond the built-in `/Differences` table - does not,
//! because the bundled face addresses glyphs with two-byte ids.
//!
//! [`SlotMetrics`] bridges the two: every distinct character of one note gets a
//! dense slot code (the space keeps 0x20, which the wrapper writes literally
//! between words), the layout works in slots with the face's own advance
//! widths, and only the emission turns slots back into characters and asks the
//! face for its `<hex>` operand. A note with more than 256 distinct characters
//! gives up, which is today's behaviour for text no font could show.

use std::collections::{BTreeMap, BTreeSet};

use lopdf::ObjectId;

use super::super::FontSpec;
use super::drfont::DrFont;
use super::std14::{Family, Std14};
use super::textfit::{FontMetrics, Std14Metrics};
use crate::textfont::{SubsetFace, RESOURCE_NAME};

/// The standard face a note falls back to when its `/DA` font cannot show the
/// text.
pub(super) const HELVETICA: Std14 = Std14 { family: Family::Helvetica, bold: false, italic: false };

/// The code the wrapper writes for the space between two words.
const SPACE: u8 = b' ';

/// Which font the note is set in.
pub(super) enum Face {
    /// One of the standard 14, written by us.
    Standard(Std14),
    /// A simple font of the document, referenced as it already is.
    Document { key: String, id: ObjectId, metrics: DrFont },
    /// The bundled face, for text WinAnsi cannot carry.
    Subset(SlotMetrics),
}

impl Face {
    pub(super) fn with_metrics<R>(&self, run: impl FnOnce(&dyn FontMetrics) -> R) -> R {
        match self {
            Face::Standard(font) => run(&Std14Metrics(*font)),
            Face::Document { metrics, .. } => run(metrics),
            Face::Subset(metrics) => run(metrics),
        }
    }

    pub(super) fn resource_key(&self) -> String {
        match self {
            Face::Standard(_) => "FOmni0".to_string(),
            Face::Document { key, .. } => key.clone(),
            Face::Subset(_) => RESOURCE_NAME.to_string(),
        }
    }

    pub(super) fn spec(&self) -> FontSpec {
        match self {
            Face::Standard(font) => FontSpec::Standard(*font),
            Face::Document { key, id, .. } => FontSpec::Resource { key: key.as_bytes().to_vec(), id: *id },
            Face::Subset(metrics) => FontSpec::Subset(metrics.charset()),
        }
    }

    /// The `Tj` operand for one laid-out line of codes.
    pub(super) fn operand(&self, codes: &[u8]) -> String {
        match self {
            Face::Subset(metrics) => metrics.operand(codes),
            _ => format!("<{}>", hex(codes)),
        }
    }

    /// The bundled face with a slot for every character of `text`, or `None`
    /// when the face has no glyph for one of them.
    pub(super) fn subset_for(text: &str) -> Option<Self> {
        let face = SubsetFace::bundled()?;
        SlotMetrics::new(face, text).map(Face::Subset)
    }
}

/// One note's view of the bundled face: characters mapped to one-byte slots so
/// the layout can keep speaking `FontMetrics`.
pub(super) struct SlotMetrics {
    face: &'static SubsetFace,
    /// Slot -> character, in first-seen order (0x20 is the space).
    chars: Vec<char>,
    /// Character -> slot.
    slots: BTreeMap<char, u8>,
    /// Slot -> advance in 1/1000 em, for `width`.
    widths: [f32; 256],
    ascent: f32,
}

impl SlotMetrics {
    /// `None` when the face cannot show the whole text or the note has more
    /// distinct characters than a byte can address.
    pub(super) fn new(face: &'static SubsetFace, text: &str) -> Option<Self> {
        let content: String =
            text.chars().filter(|symbol| !matches!(symbol, '\n' | '\r' | '\u{2028}' | '\t')).collect();
        if !face.can_encode(&content) {
            return None;
        }
        let mut widths = [0.0f32; 256];
        let mut slots: BTreeMap<char, u8> = BTreeMap::new();
        let mut chars: Vec<char> = Vec::new();
        // The space must live at 0x20: the wrapper writes that byte between two
        // words without asking the metrics.
        slots.insert(' ', SPACE);
        chars.push(' ');
        widths[usize::from(SPACE)] = face.measure(" ", 1000.0) as f32;
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
            widths[usize::from(slot)] = face.measure(&symbol.to_string(), 1000.0) as f32;
        }
        if chars.len() > usize::from(u8::MAX) {
            return None;
        }
        Some(Self { face, chars, slots, widths, ascent: f32::from(HELVETICA.ascent()) })
    }

    /// The `<hex>` operand for one line of slots.
    pub(super) fn operand(&self, codes: &[u8]) -> String {
        let text: String = codes.iter().filter_map(|code| self.chars.get(usize::from(*code))).collect();
        self.face.encode(&text)
    }

    /// The characters a subset font has to carry for this note.
    pub(super) fn charset(&self) -> BTreeSet<char> {
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

/// Hex, because a hex string needs no escaping for `(`, `)` or `\`.
fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02X}"));
    }
    out
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

    #[test]
    fn the_hex_writer_pads_each_byte() {
        assert_eq!(hex(&[0x01, 0xAB]), "01AB");
        assert_eq!(hex(b""), "");
    }
}
