//! Composite (`Type0`) fonts from the document's `/DR` resources, for free-text
//! notes.
//!
//! A `/DA` font can be a Type0 font with `/Encoding /Identity-H`: its codes are
//! two-byte CIDs, and `/ToUnicode` is the only honest way back from a character
//! to the code the document used. Drawing a note with that font means reading
//! four things and refusing the font when any of them is missing:
//!
//! * exactly one descendant `CIDFontType0`/`CIDFontType2` with an **embedded**
//!   program (a non-embedded Identity-H font means nothing to a viewer),
//! * `/W` and `/DW`, the advances ([`widths`]),
//! * `/ToUnicode`, the character-to-code map ([`crate::cmap`]),
//! * for `CIDFontType2`, a `/CIDToGIDMap` that does not send a used CID to
//!   glyph 0 - the `.notdef` box.
//!
//! Vertical writing (`/Identity-V`), predefined or embedded CMaps, several
//! descendants and a missing program are all refused: the caller then leaves the
//! text out, which is what happens today. `/CIDSystemInfo` is deliberately not
//! consulted - with Identity-H and an embedded program it only matters for the
//! substitution we never do.

use lopdf::{Dictionary, Document, Object, ObjectId};

use super::super::super::dests::resolve;
use super::super::super::links::number;
use super::drfont;
use crate::cmap::{self, Code, ToUnicode};

mod mapping;
mod widths;

/// The `/ToUnicode` stream of one font stops here, as in [`crate::cmap`].
const MAX_TO_UNICODE: usize = 1 << 20;

/// A composite font of the document, ready to be referenced by an appearance.
pub(super) struct DocCid {
    /// The `/DA` name, which the appearance uses as its resource key.
    key: String,
    /// The font dictionary object, which the form's `/Resources` references.
    id: ObjectId,
    font: CidFont,
}

struct CidFont {
    to_unicode: ToUnicode,
    widths: widths::Widths,
    mapping: mapping::Mapping,
    ascent: f32,
}

impl DocCid {
    pub(super) fn key(&self) -> &str {
        &self.key
    }

    pub(super) fn id(&self) -> ObjectId {
        self.id
    }

    /// The code `symbol` is drawn with, when the font can really draw it.
    ///
    /// The code is what `/ToUnicode` inverted and what the appearance writes;
    /// the CID it selects is checked through `/Encoding`, so an unmapped code,
    /// CID 0 and a `.notdef` glyph all cost the character.
    pub(super) fn code(&self, symbol: char) -> Option<Code> {
        let code = self.font.to_unicode.sized(symbol)?;
        if code.value() == 0 {
            return None;
        }
        self.font.mapping.admit(code)
    }

    /// The advance of `code` in 1/1000 em, or `None` when it draws nothing.
    /// `/W` is keyed by CID, not by code.
    pub(super) fn advance(&self, code: Code) -> Option<f32> {
        let cid = self.font.mapping.cid(code)?;
        let width = self.font.widths.get(cid);
        (width > 0.0).then_some(width)
    }

    pub(super) fn ascent(&self) -> f32 {
        self.font.ascent
    }
}

/// Resolves the composite font named by `/DA` in the annotation's `/DR`, then
/// in the form's; `None` when it cannot be used for metrics.
pub(super) fn resolve_font(doc: &Document, annot: &Dictionary, name: &str) -> Option<DocCid> {
    let entry = drfont::font_entry(doc, annot, name)?;
    // Only an indirect font can be referenced by the appearance: a direct
    // dictionary would have to be copied into every form.
    let Object::Reference(id) = entry else { return None };
    let dict = doc.get_dictionary(id).ok()?;
    if dict.get(b"Subtype").ok()?.as_name().ok()? != b"Type0" {
        return None;
    }
    let child = descendant(doc, dict)?;
    let subtype = child.get(b"Subtype").ok()?.as_name().ok()?;
    // `/Encoding` has no default; a vertical or unknown one is refused there.
    let mapping = mapping::mapping(doc, dict, child, subtype)?;
    let descriptor = resolve(doc, child.get(b"FontDescriptor").ok()?).as_dict().ok()?;
    if !embedded(doc, descriptor) {
        return None;
    }
    let widths = widths::parse(doc, child.get(b"W").ok(), child.get(b"DW").ok())?;
    let to_unicode = read_to_unicode(doc, dict)?;
    let ascent = descriptor
        .get(b"Ascent")
        .ok()
        .and_then(number)
        .filter(|value| value.is_finite() && *value > 0.0 && *value <= 2000.0)
        .map(|value| (value / 1000.0) as f32)
        .unwrap_or(0.718);
    Some(DocCid { key: name.to_string(), id, font: CidFont { to_unicode, widths, mapping, ascent } })
}

/// The one descendant CID font, as the spec's one-element array allows.
fn descendant<'a>(doc: &'a Document, dict: &'a Dictionary) -> Option<&'a Dictionary> {
    let array = resolve(doc, dict.get(b"DescendantFonts").ok()?).as_array().ok()?;
    if array.len() != 1 {
        return None;
    }
    resolve(doc, array.first()?).as_dict().ok()
}

/// Whether the descriptor names a font program the viewer will find.
fn embedded(doc: &Document, descriptor: &Dictionary) -> bool {
    [b"FontFile".as_slice(), b"FontFile2", b"FontFile3"]
        .iter()
        .any(|key| descriptor.get(key).ok().is_some_and(|value| !matches!(resolve(doc, value), Object::Null)))
}

/// The font's `/ToUnicode`, which is the only way from text back to its codes.
fn read_to_unicode(doc: &Document, dict: &Dictionary) -> Option<ToUnicode> {
    let id = dict.get(b"ToUnicode").ok()?.as_reference().ok()?;
    let stream = doc.get_object(id).ok()?.as_stream().ok()?;
    let data = stream.decompressed_content_with_limit(MAX_TO_UNICODE).ok()?;
    let map = cmap::read(&data);
    (!map.is_empty()).then_some(map)
}

#[cfg(test)]
mod tests;
