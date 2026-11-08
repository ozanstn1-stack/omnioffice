//! Simple fonts from the document's `/DR` resources, for free-text notes.
//!
//! A form's default appearance names a font in `/AcroForm /DR /Font`. When that
//! font is a simple Type1 or TrueType program with `/Widths`, the imposition can
//! reference it instead of falling back to Helvetica: the text keeps its face
//! and the line breaking keeps its real advances.
//!
//! Only the shapes whose metrics can be read without parsing the program are
//! accepted. A composite (Type0) font, a Type3 font, a symbolic TrueType or a
//! font whose encoding is neither WinAnsi nor a `/Differences` list is refused,
//! and the caller falls back to the standard 14 path.

use super::super::super::dests::resolve;
use glyph::glyph_symbol;
use std::sync::Arc;

use super::super::super::links::number;
use super::cidfont::FontCache;
use super::textfit::{win_ansi_symbol, FontMetrics};
use lopdf::{Dictionary, Document, Object, ObjectId};
use std::collections::HashMap;

/// A simple font of the document, ready to be referenced by an appearance.
pub(crate) struct DrFont {
    /// The font dictionary object, which the form's `/Resources` references.
    pub(super) id: ObjectId,
    widths: Vec<f32>,
    first_char: u8,
    missing_width: f32,
    reverse: HashMap<char, u8>,
    ascent: f32,
}

impl FontMetrics for DrFont {
    fn encode(&self, symbol: char) -> Option<u8> {
        self.reverse.get(&symbol).copied()
    }

    fn width(&self, code: u8) -> f32 {
        let index = usize::from(code.saturating_sub(self.first_char));
        match self.widths.get(index) {
            Some(width) if code >= self.first_char && *width > 0.0 => *width,
            _ => self.missing_width,
        }
    }

    fn ascent(&self) -> f32 {
        self.ascent
    }
}

impl DrFont {
    /// An estimate of what this font holds, for the cache's budget.
    pub(super) fn heap_bytes(&self) -> usize {
        self.widths.len() * std::mem::size_of::<f32>() + self.reverse.len() * 8
    }

    /// Whether every character of `text` is encoded and has a real glyph.
    pub(super) fn can_show(&self, text: &str) -> bool {
        text.chars()
            .filter(|symbol| *symbol != ' ' && *symbol != '\n' && *symbol != '\r' && *symbol != '\t')
            .all(|symbol| self.encode(symbol).is_some_and(|code| self.width(code) > 0.0))
    }
}

/// Resolves the font named by `/DA` in the annotation's `/DR`, then in the
/// form's; `None` when it cannot be used for metrics.
pub(super) fn resolve_font(
    doc: &Document,
    annot: &Dictionary,
    name: &str,
    cache: &mut FontCache,
) -> Option<Arc<DrFont>> {
    let entry = font_entry(doc, annot, name)?;
    let Object::Reference(id) = entry else {
        // A font dictionary written inline has no object to cache; the caller's
        // standard-font fallback is the honest answer.
        return None;
    };
    let dict = doc.get_dictionary(id).ok()?;
    cache.simple(doc, id, dict)
}

/// Reads one simple font, or `None` when its metrics cannot be read.
pub(super) fn parse_font(doc: &Document, id: ObjectId, dict: &Dictionary) -> Option<DrFont> {
    let subtype = dict.get(b"Subtype").ok()?.as_name().ok()?;
    if !matches!(subtype, b"Type1" | b"MMType1" | b"TrueType") {
        return None;
    }
    if dict.get(b"Flags").ok().and_then(|value| value.as_i64().ok()).is_some_and(|flags| flags & 4 != 0)
        && dict.get(b"Encoding").is_err()
    {
        return None;
    }
    let widths = read_widths(doc, dict)?;
    let forward = encoding_table(doc, dict)?;
    let mut reverse: HashMap<char, u8> = HashMap::new();
    for (code, symbol) in forward.iter().enumerate() {
        if let Some(symbol) = symbol {
            // The smallest code wins, so the output is deterministic.
            reverse.entry(*symbol).or_insert(code as u8);
        }
    }
    let descriptor = dict
        .get(b"FontDescriptor")
        .ok()
        .map(|value| resolve(doc, value))
        .and_then(|value| value.as_dict().ok().cloned());
    let missing_width = descriptor
        .as_ref()
        .and_then(|descriptor| descriptor.get(b"MissingWidth").ok().and_then(number))
        .filter(|width| width.is_finite() && *width >= 0.0)
        .unwrap_or(0.0) as f32;
    let ascent = descriptor
        .as_ref()
        .and_then(|descriptor| descriptor.get(b"Ascent").ok().and_then(number))
        .filter(|value| value.is_finite() && *value > 0.0 && *value <= 2000.0)
        .map(|value| (value / 1000.0) as f32)
        .unwrap_or(0.718);
    Some(DrFont {
        id,
        widths,
        first_char: dict.get(b"FirstChar").ok().and_then(|value| value.as_i64().ok()).unwrap_or(0) as u8,
        missing_width,
        reverse,
        ascent,
    })
}

/// The font entry named by `/DA`: the annotation's `/DR` first, then the form's.
pub(super) fn font_entry(doc: &Document, annot: &Dictionary, name: &str) -> Option<Object> {
    let own = annot
        .get(b"DR")
        .ok()
        .map(|value| resolve(doc, value))
        .and_then(|value| value.as_dict().ok().cloned())
        .and_then(|dr| dr.get(b"Font").ok().cloned());
    let form = doc
        .catalog()
        .ok()
        .and_then(|catalog| catalog.get(b"AcroForm").ok().cloned())
        .and_then(|form| resolve(doc, &form).as_dict().ok().cloned())
        .and_then(|form| form.get(b"DR").ok().cloned())
        .and_then(|dr| resolve(doc, &dr).as_dict().ok().cloned())
        .and_then(|dr| dr.get(b"Font").ok().cloned());
    for fonts in [own, form].into_iter().flatten() {
        let fonts = resolve(doc, &fonts);
        if let Ok(dict) = fonts.as_dict() {
            if let Ok(entry) = dict.get(name.as_bytes()) {
                return Some(entry.clone());
            }
        }
    }
    None
}

/// `/Widths` with `/FirstChar`; a font without them cannot be measured.
fn read_widths(doc: &Document, dict: &Dictionary) -> Option<Vec<f32>> {
    let items = resolve(doc, dict.get(b"Widths").ok()?).as_array().ok()?.clone();
    if items.is_empty() || items.len() > 256 {
        return None;
    }
    let mut widths = Vec::with_capacity(items.len());
    for item in &items {
        let width = number(item)?;
        if !width.is_finite() || width < 0.0 {
            return None;
        }
        widths.push(width as f32);
    }
    Some(widths)
}

/// The code to character table: base WinAnsi, then `/Differences`.
fn encoding_table(doc: &Document, dict: &Dictionary) -> Option<Vec<Option<char>>> {
    let encoding = dict.get(b"Encoding").ok()?;
    let encoding = resolve(doc, encoding);
    let mut table: Vec<Option<char>> = match encoding {
        Object::Name(name) if name == b"WinAnsiEncoding" => (0..=255u8).map(win_ansi_symbol).collect(),
        Object::Dictionary(dict) => {
            match dict.get(b"BaseEncoding").ok().and_then(|value| value.as_name().ok()) {
                Some(b"WinAnsiEncoding") => (0..=255u8).map(win_ansi_symbol).collect(),
                Some(_) => return None,
                // Only the differences are defined: everything else stays
                // undefined rather than guessed.
                None => vec![None; 256],
            }
        }
        _ => return None,
    };
    if let Object::Dictionary(dict) = encoding {
        if let Ok(differences) = dict.get(b"Differences") {
            apply_differences(doc, differences, &mut table)?;
        }
    }
    Some(table)
}

fn apply_differences(doc: &Document, differences: &Object, table: &mut [Option<char>]) -> Option<()> {
    let items = resolve(doc, differences).as_array().ok()?;
    let mut code: usize = 0;
    for item in items {
        match item {
            Object::Integer(value) => {
                let next = usize::try_from(*value).ok()?;
                if next > 255 {
                    return None;
                }
                code = next;
            }
            Object::Real(_) => return None,
            Object::Name(name) => {
                if code > 255 {
                    return None;
                }
                table[code] = glyph_symbol(name);
                code += 1;
            }
            _ => return None,
        }
    }
    Some(())
}

mod glyph;

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    fn doc() -> Document {
        Document::with_version("1.7")
    }

    #[test]
    fn widths_are_read_in_order() {
        let doc = doc();
        let font = dictionary! {
            "FirstChar" => 65i64,
            "Widths" => Object::Array(vec![Object::Integer(600), Object::Integer(700)]),
        };
        let widths = read_widths(&doc, &font).expect("widths");
        assert_eq!(widths, vec![600.0, 700.0]);
        let broken = dictionary! { "Widths" => Object::Array(vec![Object::Real(-1.0)]) };
        assert!(read_widths(&doc, &broken).is_none(), "a negative advance is refused");
    }

    #[test]
    fn a_missing_encoding_is_refused() {
        let doc = doc();
        assert!(encoding_table(&doc, &Dictionary::new()).is_none());
        let standard = dictionary! { "Encoding" => "StandardEncoding" };
        assert!(encoding_table(&doc, &standard).is_none());
        let win = dictionary! { "Encoding" => "WinAnsiEncoding" };
        let table = encoding_table(&doc, &win).expect("table");
        assert_eq!(table[65], Some('A'));
    }

    #[test]
    fn differences_override_the_base_encoding() {
        let doc = doc();
        let font = dictionary! {
            "Encoding" => Object::Dictionary(dictionary! {
                "BaseEncoding" => "WinAnsiEncoding",
                "Differences" => Object::Array(vec![Object::Integer(65), Object::Name(b"gbreve".to_vec())]),
            }),
        };
        let table = encoding_table(&doc, &font).expect("table");
        assert_eq!(table[65], Some('ğ'));
        assert_eq!(table[66], Some('B'), "the base encoding still holds where no difference is given");
    }
}
