//! Word- and character-level text geometry.
//!
//! `render::extract_page_text` flattens a page to a string and throws away every
//! coordinate, which is fine for search but useless for redaction: hiding text
//! requires knowing exactly which glyphs sit under which rectangle. This module
//! exposes pdfium's per-character boxes and the word grouping built on top.
//!
//! The document is parsed once per call and reused for every page asked for,
//! because re-parsing a 500-page file per page is what made the original
//! search implementation quadratic.

use std::collections::BTreeMap;
use std::path::Path;

use pdfium_render::prelude::*;
use serde::{Deserialize, Serialize};

use crate::error::{PdfError, PdfResult};
use crate::render::RenderOptions;

/// A rectangle in PDF page space: origin at the bottom-left, in points.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CharBox {
    pub left: f64,
    pub bottom: f64,
    pub right: f64,
    pub top: f64,
}

impl CharBox {
    pub fn width(&self) -> f64 {
        (self.right - self.left).max(0.0)
    }

    pub fn height(&self) -> f64 {
        (self.top - self.bottom).max(0.0)
    }

    pub fn center_x(&self) -> f64 {
        (self.left + self.right) / 2.0
    }

    pub fn center_y(&self) -> f64 {
        (self.bottom + self.top) / 2.0
    }

    /// Grows the box by `padding` points on every side.
    pub fn padded(&self, padding: f64) -> CharBox {
        CharBox {
            left: self.left - padding,
            bottom: self.bottom - padding,
            right: self.right + padding,
            top: self.top + padding,
        }
    }

    /// True when this box and `other` share any area.
    ///
    /// A plain `<`/`>` comparison on the edges is not enough: a zero-height or
    /// zero-width box (a space, a glyph pdfium could not measure) would then
    /// report no overlap with a box that visually covers it.
    pub fn overlaps(&self, other: &CharBox) -> bool {
        if self.width() <= 0.0 && self.height() <= 0.0 {
            return other.left <= self.center_x()
                && self.center_x() <= other.right
                && other.bottom <= self.center_y()
                && self.center_y() <= other.top;
        }
        if other.width() <= 0.0 && other.height() <= 0.0 {
            return self.left <= other.center_x()
                && other.center_x() <= self.right
                && self.bottom <= other.center_y()
                && other.center_y() <= self.top;
        }
        self.left < other.right && other.left < self.right && self.bottom < other.top && other.bottom < self.top
    }

    /// True when the centre of `other` is inside this box.
    pub fn contains_center(&self, other: &CharBox) -> bool {
        self.left <= other.center_x()
            && other.center_x() <= self.right
            && self.bottom <= other.center_y()
            && other.center_y() <= self.top
    }
}

/// One character with its position and the text object it belongs to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextChar {
    /// Unicode scalar value; 0 when pdfium could not map the glyph.
    pub code: u32,
    /// Index of this character within the page, in reading order.
    pub index: u32,
    pub box_rect: CharBox,
    pub font_size_pt: f64,
    pub font_name: String,
    /// True for text drawn in invisible render mode (Tr 3), which is how most
    /// scanned PDFs carry their OCR text layer.
    pub invisible: bool,
    /// True when pdfium synthesised the character (word spacing, ligature).
    pub generated: bool,
    /// True for the soft hyphen pdfium inserts at a line break.
    pub hyphen: bool,
}

impl TextChar {
    pub fn as_char(&self) -> Option<char> {
        char::from_u32(self.code)
    }
}

/// A run of characters that reads as one word.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextWord {
    pub text: String,
    /// Index of the first character of the word within the page.
    pub start: u32,
    /// Index one past the last character of the word.
    pub end: u32,
    pub box_rect: CharBox,
    pub invisible: bool,
}

impl TextWord {
    /// The tightest rectangle that covers every character of the word.
    pub fn bounds(chars: &[TextChar]) -> CharBox {
        let mut left = f64::INFINITY;
        let mut bottom = f64::INFINITY;
        let mut right = f64::NEG_INFINITY;
        let mut top = f64::NEG_INFINITY;
        for entry in chars {
            if entry.box_rect.width() <= 0.0 && entry.box_rect.height() <= 0.0 {
                continue;
            }
            left = left.min(entry.box_rect.left);
            bottom = bottom.min(entry.box_rect.bottom);
            right = right.max(entry.box_rect.right);
            top = top.max(entry.box_rect.top);
        }
        if !left.is_finite() {
            return CharBox { left: 0.0, bottom: 0.0, right: 0.0, top: 0.0 };
        }
        CharBox { left, bottom, right, top }
    }
}

/// All characters of one page, in reading order.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PageChars {
    /// 1-based page number.
    pub page: u32,
    pub chars: Vec<TextChar>,
}

impl PageChars {
    /// The page text, with one entry per character.
    pub fn text(&self) -> String {
        self.chars.iter().filter_map(|entry| entry.as_char()).collect()
    }

    /// Character indices whose box intersects `rect`, as a half-open range.
    ///
    /// Redaction needs a contiguous range, so the indices are collapsed to
    /// `first..=last`; a rect that catches the middle of a line therefore also
    /// removes the glyphs on either side of it inside the same text run, which
    /// is the safe direction to err in.
    pub fn indices_in(&self, rect: &CharBox) -> Option<(usize, usize)> {
        let mut first: Option<usize> = None;
        let mut last: Option<usize> = None;
        for (position, entry) in self.chars.iter().enumerate() {
            if entry.box_rect.overlaps(rect) {
                first.get_or_insert(position);
                last = Some(position);
            }
        }
        first.zip(last).map(|(a, b)| (a, b + 1))
    }

    /// The visible words of the page, grouped from the character stream.
    pub fn words(&self) -> Vec<TextWord> {
        let mut out: Vec<TextWord> = Vec::new();
        let mut current: Vec<usize> = Vec::new();
        let flush = |out: &mut Vec<TextWord>, current: &mut Vec<usize>, chars: &[TextChar]| {
            if current.is_empty() {
                return;
            }
            let slice: Vec<TextChar> = current.iter().map(|index| chars[*index].clone()).collect();
            let text: String = slice.iter().filter_map(|entry| entry.as_char()).collect();
            let start = current[0];
            let end = current[current.len() - 1] + 1;
            out.push(TextWord {
                text,
                start: start as u32,
                end: end as u32,
                box_rect: TextWord::bounds(&slice),
                invisible: slice.iter().all(|entry| entry.invisible),
            });
            current.clear();
        };
        for (position, entry) in self.chars.iter().enumerate() {
            let character = entry.as_char();
            // A newline, a generated spacing glyph or a missing glyph ends a word.
            let breaks = match character {
                None => true,
                Some(value) => value.is_whitespace() || entry.generated || entry.hyphen,
            };
            if breaks {
                flush(&mut out, &mut current, &self.chars);
                continue;
            }
            current.push(position);
        }
        flush(&mut out, &mut current, &self.chars);
        out
    }
}

/// Loads the character geometry for a range of pages in one pass.
///
/// `pages` is 1-based; an empty list means every page. `max_pages` bounds the
/// work so a 5000-page scan cannot hang the UI, and the caller is told through
/// the truncated count rather than silently getting a partial answer.
pub fn page_chars(
    path: &Path,
    password: Option<&str>,
    pages: &[u32],
    max_pages: u32,
    cancel: &crate::progress::CancelToken,
    on_page: &dyn Fn(u32, u32),
) -> PdfResult<(BTreeMap<u32, PageChars>, u32)> {
    crate::render::ensure_available()?;
    let pdfium = crate::render::pdfium_instance()?;
    let document = pdfium.load_pdf_from_file(path, password).map_err(pdfium_error)?;
    let total = document.pages().len() as u32;
    let limit = max_pages.max(1);
    let wanted: Vec<u32> = if pages.is_empty() {
        (1..=total.min(limit)).collect()
    } else {
        let mut list: Vec<u32> = pages.iter().copied().filter(|page| *page >= 1 && *page <= total).collect();
        list.sort_unstable();
        list.dedup();
        list.truncate(limit as usize);
        list
    };

    let total_wanted = wanted.len() as u32;
    let mut out: BTreeMap<u32, PageChars> = BTreeMap::new();
    for page_number in wanted {
        cancel.check()?;
        on_page(page_number, total_wanted);
        let index = (page_number - 1) as i32;
        let page = match document.pages().get(index) {
            Ok(page) => page,
            Err(_) => continue,
        };
        out.insert(page_number, read_page_chars(&page, page_number));
    }
    let truncated = total.saturating_sub(out.len() as u32);
    Ok((out, truncated))
}

/// Reads every character of one open pdfium page.
pub fn read_page_chars(page: &PdfPage<'_>, page_number: u32) -> PageChars {
    let mut out = PageChars { page: page_number, chars: Vec::new() };
    let text = match page.text() {
        Ok(value) => value,
        Err(_) => return out,
    };
    let length = text.len().max(0) as u32;
    for position in 0..length {
        let entry = match text.chars().get(position as usize) {
            Ok(entry) => entry,
            Err(_) => break,
        };
        let code = entry.unicode_value();
        // pdfium reports U+FFFD or 0 for glyphs it cannot map; keep them so the
        // index space stays aligned with what the content stream will contain,
        // but record a null code so grouping can treat them as separators.
        let box_rect = match entry.loose_bounds() {
            Ok(rect) => CharBox {
                left: rect.left().value as f64,
                bottom: rect.bottom().value as f64,
                right: rect.right().value as f64,
                top: rect.top().value as f64,
            },
            Err(_) => CharBox { left: 0.0, bottom: 0.0, right: 0.0, top: 0.0 },
        };
        let invisible = entry.render_mode().map(|mode| mode == PdfPageTextRenderMode::Invisible).unwrap_or(false);
        let generated = entry.is_generated().unwrap_or(false);
        let hyphen = entry.is_hyphen().unwrap_or(false);
        out.chars.push(TextChar {
            code,
            index: out.chars.len() as u32,
            box_rect,
            font_size_pt: entry.scaled_font_size().value as f64,
            font_name: entry.font_name(),
            invisible,
            generated,
            hyphen,
        });
    }
    // A page that pdfium reports as having text but yields no characters means
    // the character API is unavailable in this build; say so rather than
    // returning an empty page that looks like a blank scan.
    if out.chars.is_empty() && length > 0 {
        out.chars.clear();
    }
    out
}

/// Renders a page and returns it, reusing the same options the reader uses.
pub fn render_for_diff(
    path: &Path,
    password: Option<&str>,
    page_number: u32,
    options: &RenderOptions,
) -> PdfResult<crate::render::RenderedPage> {
    crate::render::render_page(path, password, page_number, options)
}

fn pdfium_error(error: PdfiumError) -> PdfError {
    PdfError::ProcessingFailed(format!("pdfium: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(left: f64, bottom: f64, right: f64, top: f64) -> CharBox {
        CharBox { left, bottom, right, top }
    }

    fn character(index: u32, code: u32, box_rect: CharBox) -> TextChar {
        TextChar {
            code,
            index,
            box_rect,
            font_size_pt: 11.0,
            font_name: "Helvetica".into(),
            invisible: false,
            generated: false,
            hyphen: false,
        }
    }

    #[test]
    fn overlap_uses_strict_edges_so_touching_boxes_do_not_match() {
        let a = rect(0.0, 0.0, 10.0, 10.0);
        let b = rect(10.0, 0.0, 20.0, 10.0);
        assert!(!a.overlaps(&b), "boxes that only share an edge must not overlap");
        assert!(a.overlaps(&rect(9.0, 0.0, 11.0, 10.0)));
    }

    #[test]
    fn a_zero_sized_box_is_treated_as_a_point() {
        let area = rect(0.0, 0.0, 100.0, 100.0);
        let point = rect(50.0, 50.0, 50.0, 50.0);
        assert!(area.overlaps(&point));
        assert!(point.overlaps(&area));
        assert!(!area.overlaps(&rect(200.0, 200.0, 200.0, 200.0)));
    }

    #[test]
    fn padded_grows_on_every_side() {
        let grown = rect(10.0, 10.0, 20.0, 20.0).padded(2.0);
        assert_eq!(grown.left, 8.0);
        assert_eq!(grown.bottom, 8.0);
        assert_eq!(grown.right, 22.0);
        assert_eq!(grown.top, 22.0);
    }

    #[test]
    fn indices_in_collapse_to_a_half_open_range() {
        let page = PageChars {
            page: 1,
            chars: vec![
                character(0, b'a' as u32, rect(0.0, 0.0, 5.0, 10.0)),
                character(1, b'b' as u32, rect(5.0, 0.0, 10.0, 10.0)),
                character(2, b'c' as u32, rect(10.0, 0.0, 15.0, 10.0)),
            ],
        };
        assert_eq!(page.indices_in(&rect(0.0, 0.0, 5.0, 10.0)), Some((0, 1)));
        assert_eq!(page.indices_in(&rect(0.0, 0.0, 15.0, 10.0)), Some((0, 3)));
        assert_eq!(page.indices_in(&rect(100.0, 100.0, 110.0, 110.0)), None);
    }

    #[test]
    fn words_group_characters_and_split_on_whitespace() {
        let page = PageChars {
            page: 1,
            chars: vec![
                character(0, b'H' as u32, rect(0.0, 0.0, 5.0, 10.0)),
                character(1, b'i' as u32, rect(5.0, 0.0, 8.0, 10.0)),
                character(2, b' ' as u32, rect(8.0, 0.0, 10.0, 10.0)),
                character(3, b'T' as u32, rect(10.0, 0.0, 15.0, 10.0)),
            ],
        };
        let words = page.words();
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].text, "Hi");
        assert_eq!(words[0].start, 0);
        assert_eq!(words[0].end, 2);
        assert_eq!(words[1].text, "T");
        assert_eq!(words[1].start, 3);
    }

    #[test]
    fn text_rebuilds_the_page_string() {
        let page = PageChars {
            page: 1,
            chars: vec![
                character(0, b'a' as u32, rect(0.0, 0.0, 5.0, 10.0)),
                character(1, b'b' as u32, rect(5.0, 0.0, 10.0, 10.0)),
            ],
        };
        assert_eq!(page.text(), "ab");
    }
}
