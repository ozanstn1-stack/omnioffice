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

use lopdf::{Dictionary, Document, ObjectId};

use super::super::FontSpec;
use super::cidfont::{self, FontCache};
use super::drfont::DrFont;
use super::std14::{Family, Std14};
use super::textfit::{FontMetrics, Std14Metrics};
use crate::textfont::SubsetFace;

mod slots;
pub(super) use slots::SlotMetrics;

/// The standard face a note falls back to when its `/DA` font cannot show the
/// text.
pub(super) const HELVETICA: Std14 = Std14 { family: Family::Helvetica, bold: false, italic: false };

/// Which font the note is set in.
pub(super) enum Face {
    /// One of the standard 14, written by us.
    Standard(Std14),
    /// A simple font of the document, referenced as it already is.
    Document { key: String, id: ObjectId, metrics: DrFont },
    /// A two-byte font reached through slots: the bundled face, or a composite
    /// font of the document.
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
            Face::Subset(metrics) => metrics.resource_key(),
        }
    }

    pub(super) fn spec(&self) -> FontSpec {
        match self {
            Face::Standard(font) => FontSpec::Standard(*font),
            Face::Document { key, id, .. } => FontSpec::Resource { key: key.as_bytes().to_vec(), id: *id },
            Face::Subset(metrics) => metrics.spec(),
        }
    }

    /// The `Tj` operand for one laid-out line of codes.
    pub(super) fn operand(&self, codes: &[u8]) -> String {
        match self {
            Face::Subset(metrics) => metrics.operand(codes),
            _ => format!("<{}>", hex(codes)),
        }
    }

    /// Whether the face can draw every character of `text`; the standard 14 and
    /// a slotted face were chosen for exactly that text.
    pub(super) fn can_show(&self, text: &str) -> bool {
        match self {
            Face::Document { metrics, .. } => metrics.can_show(text),
            Face::Standard(_) | Face::Subset(_) => true,
        }
    }

    /// The face for text WinAnsi cannot carry: the bundled face first, then the
    /// document's own composite font, and `None` when neither can show it.
    pub(super) fn script_for(
        doc: &Document,
        annot: &Dictionary,
        name: &str,
        text: &str,
        fonts: &mut FontCache,
    ) -> Option<Self> {
        SubsetFace::bundled().and_then(|face| SlotMetrics::new(face, text).map(Face::Subset)).or_else(|| {
            let font = cidfont::resolve_font(doc, annot, name, fonts)?;
            SlotMetrics::for_document(font, text).map(Face::Subset)
        })
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

    #[test]
    fn the_hex_writer_pads_each_byte() {
        assert_eq!(hex(&[0x01, 0xAB]), "01AB");
        assert_eq!(hex(b""), "");
    }
}
