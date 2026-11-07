//! A Type0/CID TrueType font writer for the text this crate draws itself
//! (header/footer and Bates stamps, page numbering, form appearances).
//!
//! The base14 Helvetica those paths used can only encode WinAnsi, so Turkish
//! `ş`, `ğ`, `ı`, `İ`, `Ş` came out as `?` or mojibake. This module embeds a
//! subset of the bundled Liberation Sans as an Identity-H Type0 font with a
//! `ToUnicode` CMap, so text extraction returns the original characters.
//!
//! The font is subset per call to the characters that will actually be drawn
//! (`ttfsubset::subset_truetype`), which keeps a stamp revision at roughly
//! 10-15 KB. Text is written as `<hex>` strings of glyph ids; widths come from
//! the font's own `hmtx`, so placement matches the embedded outlines.

use lopdf::{Document, Object, ObjectId};
use std::collections::BTreeSet;

use crate::error::PdfResult;

mod subset;
pub use subset::SubsetFace;

/// Resource name the drawn text refers to (`/OmniTr <size> Tf`).
pub const RESOURCE_NAME: &str = "OmniTr";

/// A subset font ready to be referenced from a page resource dictionary.
#[derive(Debug, Clone)]
pub struct TextFont {
    font_id: ObjectId,
    face: SubsetFace,
}

impl TextFont {
    /// Builds the font for `chars`, subsets the bundled face to the glyphs
    /// those characters need and writes the font objects into `doc`.
    pub fn create(doc: &mut Document, chars: &BTreeSet<char>) -> PdfResult<Self> {
        let face = SubsetFace::bundled()
            .ok_or_else(|| crate::error::PdfError::Internal("the bundled font is not readable".into()))?;
        let font_id = face.write(doc, chars)?;
        Ok(Self { font_id, face: face.clone() })
    }

    /// Reference to the font object, for a page's `/Font` resource dictionary.
    pub fn reference(&self) -> Object {
        Object::Reference(self.font_id)
    }

    /// Advance width of `text` in points at `size_pt`.
    pub fn measure(&self, text: &str, size_pt: f64) -> f64 {
        self.face.measure(text, size_pt)
    }

    /// `<hex>` string of two-byte glyph ids for a `Tj`/`TJ` operand.
    pub fn encode(&self, text: &str) -> String {
        self.face.encode(text)
    }
}

/// Adds the font to a page's `/Font` resource dictionary under `name`.
pub fn ensure_page_font_named(doc: &mut Document, page_id: ObjectId, font: &TextFont, name: &str) -> PdfResult<()> {
    crate::docutil::add_resource_entry(doc, page_id, b"Font", name, font.reference())
}

/// Adds the font to a page's `/Font` resource dictionary (idempotent per page).
pub fn ensure_page_font(doc: &mut Document, page_id: ObjectId, font: &TextFont) -> PdfResult<()> {
    ensure_page_font_named(doc, page_id, font, RESOURCE_NAME)
}

// ---------------------------------------------------------------------------
// Font object writing
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::Document;

    #[test]
    fn encoded_text_is_two_byte_glyph_ids() {
        let mut doc = Document::with_version("1.7");
        let chars: BTreeSet<char> = "Şişli Güneş İĞDIR ı".chars().collect();
        let font = TextFont::create(&mut doc, &chars).expect("font");
        let encoded = font.encode("Şişli");
        assert_eq!(encoded.len(), 2 + 5 * 4);
        assert!(encoded.starts_with('<') && encoded.ends_with('>'));
        assert!(font.measure("Şişli", 12.0) > 0.0);
    }
}
