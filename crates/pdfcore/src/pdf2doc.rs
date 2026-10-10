//! Layout recovery for PDF -> Word conversion.
//!
//! A PDF page is a set of positioned glyphs with no notion of a paragraph, a
//! heading or a list. This module rebuilds that structure from geometry and
//! font names, page by page:
//!
//! 1. glyphs are grouped into words and words into lines by baseline;
//! 2. running headers and footers (the same text in the top or bottom band of
//!    most pages) and bare page numbers are dropped;
//! 3. a simple two-column layout is found from a vertical gutter that splits
//!    most lines, and the left column is read before the right one;
//! 4. lines are merged into paragraphs from line spacing, indentation and
//!    sentence-ending punctuation, joining "exam-" + "ple" across the break;
//! 5. headings (larger or bold-only short lines), bulleted or numbered list
//!    items and simple tables (aligned rows of multi-cell lines) are marked.
//!
//! The result is plain data: pdfcore does not depend on the office model, so
//! the app maps [`RecoveredDocument`] onto its own document type. Text comes
//! from pdfium's character boxes when the engine is available and from the
//! content-stream runs of [`crate::content`] otherwise; the grouping itself
//! ([`recover_pages`]) is pure and works on either.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;

use lopdf::{Dictionary, Document, ObjectId};
use serde::{Deserialize, Serialize};

use crate::content::TextRunInfo;
use crate::docutil::object_to_f64;
use crate::error::{PdfError, PdfResult};
use crate::textbox::TextChar;

// ---------------------------------------------------------------------------
// Structure
// ---------------------------------------------------------------------------

/// A positioned piece of text: a word from pdfium or a run from the content
/// stream. Coordinates are points from the bottom-left of the page box.
#[derive(Debug, Clone, PartialEq)]
pub struct TextFragment {
    pub text: String,
    /// Left edge.
    pub x: f64,
    /// Advance width.
    pub width: f64,
    /// Baseline (the bottom of the glyph box for pdfium words).
    pub y: f64,
    /// Font size in points.
    pub size: f64,
    pub bold: bool,
    pub italic: bool,
}

/// The positioned text of one page.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PageText {
    /// 1-based page number.
    pub page: u32,
    pub width: f64,
    pub height: f64,
    pub fragments: Vec<TextFragment>,
}

/// How an ordered list counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberStyle {
    Decimal,
    LowerAlpha,
    UpperAlpha,
    LowerRoman,
    UpperRoman,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKind {
    Bullet,
    Numbered(NumberStyle),
}

/// A list item. The marker is stripped from the block text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItem {
    pub kind: ListKind,
    /// Nesting level, 0 for the outermost list.
    pub level: u32,
    /// Number of the first item of the list this item belongs to (1 for bullets).
    pub start: u32,
    /// The marker as printed: "•", "3.", "b)", "(iv)".
    pub marker: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BlockKind {
    Paragraph,
    /// Heading level 1-3, ranked by font size across the document.
    Heading(u32),
    ListItem(ListItem),
    /// A simple table recovered from aligned cell rows.
    Table(TableLayout),
}

/// Text with one style.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
}

/// One recovered table cell: its text with the original styling.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableCellLayout {
    pub spans: Vec<Span>,
    /// Left edge of the cell in page points, used to size the Word columns.
    #[serde(default)]
    pub x: f64,
}

/// A simple table recovered from consecutive rows of aligned cells.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableLayout {
    pub rows: Vec<Vec<TableCellLayout>>,
}

/// A paragraph, heading or list item.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutBlock {
    pub kind: BlockKind,
    pub spans: Vec<Span>,
    /// Dominant font size in points.
    pub size: f64,
}

impl LayoutBlock {
    pub fn text(&self) -> String {
        spans_text(&self.spans)
    }
}

/// The blocks of one page in reading order.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutPage {
    /// 1-based page number.
    pub page: u32,
    pub width: f64,
    pub height: f64,
    pub blocks: Vec<LayoutBlock>,
}

/// Where the positioned text came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextSource {
    /// pdfium character boxes: exact glyph geometry.
    Pdfium,
    /// Content-stream runs read with lopdf: exact origins, estimated widths.
    ContentStream,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecoveredDocument {
    pub pages: Vec<LayoutPage>,
    pub source: TextSource,
}

impl RecoveredDocument {
    /// False for a document without a text layer, such as a scan that was
    /// never OCRed.
    pub fn has_text(&self) -> bool {
        self.pages.iter().any(|page| !page.blocks.is_empty())
    }
}

// ---------------------------------------------------------------------------
// Extraction
// ---------------------------------------------------------------------------

/// Recovers the block structure of every page of a PDF.
///
/// pdfium is preferred because it places every glyph exactly. When the engine
/// is missing or cannot open the file, the content streams are read with lopdf
/// instead, which also reports password problems precisely.
pub fn recover_file(path: &Path, password: Option<&str>) -> PdfResult<RecoveredDocument> {
    if crate::render::is_available() {
        if let Ok(pages) = pdfium_pages(path, password) {
            return Ok(RecoveredDocument { pages: recover_pages(&pages), source: TextSource::Pdfium });
        }
    }
    let pages = content_stream_pages(path, password)?;
    Ok(RecoveredDocument { pages: recover_pages(&pages), source: TextSource::ContentStream })
}

/// The words of every page, from pdfium's character boxes.
pub fn pdfium_pages(path: &Path, password: Option<&str>) -> PdfResult<Vec<PageText>> {
    let pdfium = crate::render::pdfium_instance()?;
    let document =
        pdfium.load_pdf_from_file(path, password).map_err(|error| PdfError::InvalidPdf(format!("pdfium: {error}")))?;
    let mut out = Vec::new();
    for (index, page) in document.pages().iter().enumerate() {
        let number = index as u32 + 1;
        let (left, bottom, width, height) = match page.boundaries().media() {
            Ok(media) => (
                media.bounds.left().value as f64,
                media.bounds.bottom().value as f64,
                media.bounds.width().value as f64,
                media.bounds.height().value as f64,
            ),
            Err(_) => (0.0, 0.0, page.width().value as f64, page.height().value as f64),
        };
        let chars = crate::textbox::read_page_chars(&page, number);
        out.push(PageText { page: number, width, height, fragments: words_from_chars(&chars.chars, left, bottom) });
    }
    Ok(out)
}

/// Groups pdfium characters into styled words.
///
/// pdfium already orders the characters for reading and inserts generated
/// spaces and line breaks; a word also ends where the style or the baseline
/// changes or where the next glyph does not follow the previous one.
fn words_from_chars(chars: &[TextChar], left: f64, bottom: f64) -> Vec<TextFragment> {
    let mut out = Vec::new();
    let mut current: Option<TextFragment> = None;
    for entry in chars {
        // pdfium reports a line-end hyphen as a special character.
        let character = if entry.hyphen { Some('-') } else { entry.as_char() };
        let character = character.filter(|value| {
            !entry.generated
                && !value.is_whitespace()
                && !value.is_control()
                && !matches!(value, '\u{fffd}' | '\u{fffe}')
        });
        let rect = entry.box_rect;
        let Some(character) = character.filter(|_| rect.width() > 0.0 || rect.height() > 0.0) else {
            out.extend(current.take());
            continue;
        };
        let size = if entry.font_size_pt > 0.0 { entry.font_size_pt } else { rect.height() };
        let (bold, italic) = font_style(&entry.font_name);
        let (x, right, y) = (rect.left - left, rect.right - left, rect.bottom - bottom);
        if let Some(word) = current.as_mut() {
            let end = word.x + word.width;
            let continues = word.bold == bold
                && word.italic == italic
                && (word.size - size).abs() <= 0.5
                && (word.y - y).abs() <= 0.3 * size.max(word.size)
                && x >= end - 0.3 * size
                && x - end <= 0.25 * size;
            if continues {
                word.text.push(character);
                word.width = word.width.max(right - word.x);
                continue;
            }
        }
        out.extend(current.take());
        current = Some(TextFragment { text: character.to_string(), x, width: right - x, y, size, bold, italic });
    }
    out.extend(current);
    out
}

/// Bold and italic as the font name spells them: "Arial-BoldMT",
/// "Times-Italic", "Helvetica-BoldOblique", TeX's "CMBX10" and "CMTI10".
pub fn font_style(name: &str) -> (bool, bool) {
    // Subset fonts carry a six-letter tag: "ABCDEF+Calibri-Bold".
    let base = match name.split_once('+') {
        Some((tag, rest)) if tag.len() == 6 => rest,
        _ => name,
    };
    let lower = base.to_ascii_lowercase();
    let bold = ["bold", "black", "heavy", "demibold"].iter().any(|key| lower.contains(key))
        || lower.ends_with("demi")
        || lower.starts_with("cmbx");
    let italic = ["italic", "oblique", "slanted", "kursiv"].iter().any(|key| lower.contains(key))
        || lower.ends_with("-it")
        || lower.ends_with("boldit")
        || lower.starts_with("cmti")
        || lower.starts_with("cmsl");
    (bold, italic)
}

/// The text runs of every page, read from the content streams with lopdf.
pub fn content_stream_pages(path: &Path, password: Option<&str>) -> PdfResult<Vec<PageText>> {
    let doc = crate::docutil::load_document(path, password)?;
    let mut out = Vec::new();
    let mut fonts = crate::content::FontCache::new(&doc);
    for (number, page_id) in doc.get_pages() {
        let [left, bottom, right, top] = media_box(&doc, page_id).unwrap_or([0.0, 0.0, 612.0, 792.0]);
        // A page whose content cannot be decoded contributes no text rather
        // than failing the whole conversion.
        let runs = crate::content::page_text_runs(&doc, &mut fonts, number, page_id).unwrap_or_default();
        let fragments = run_fragments(&runs, &font_table(&doc, page_id), left, bottom);
        out.push(PageText { page: number, width: right - left, height: top - bottom, fragments });
    }
    Ok(out)
}

/// Turns content-stream runs into fragments with measured widths.
///
/// [`crate::content`] places a run that follows another without a text
/// positioning operator at a rough estimate of the previous run's end; such a
/// chained run is moved to the measured end instead, so the gap between the
/// two is not mistaken for a space.
fn run_fragments(
    runs: &[TextRunInfo],
    fonts: &HashMap<Vec<u8>, FontMetrics>,
    left: f64,
    bottom: f64,
) -> Vec<TextFragment> {
    let mut out = Vec::new();
    // Origin and estimated advance of the previous run, and its measured end.
    let mut previous: Option<(f64, f64, f64, f64)> = None;
    for run in runs {
        let size = run.font_size_pt.abs();
        let metrics = run.font.as_deref().and_then(|name| fonts.get(name.as_bytes()));
        let measure = |text: &str| match metrics {
            Some(metrics) => metrics.width(text, size),
            None => crate::numbering::helvetica_text_width(text, size),
        };
        let chained = previous.is_some_and(|(x, y, advance, _)| {
            (run.y - y).abs() < 0.01 && (run.x - (x + advance)).abs() < 0.01 && advance > 0.0
        });
        let start = match previous {
            Some((.., end)) if chained => end,
            _ => run.x,
        };
        previous = Some((run.x, run.y, run.text.chars().count() as f64 * size * 0.5, start + measure(&run.text)));
        let trimmed = run.text.trim();
        if trimmed.is_empty() || size <= 0.0 {
            continue;
        }
        let lead = &run.text[..run.text.len() - run.text.trim_start().len()];
        let (bold, italic) = metrics.map(|metrics| font_style(&metrics.base_font)).unwrap_or((false, false));
        out.push(TextFragment {
            text: trimmed.to_string(),
            x: start + measure(lead) - left,
            width: measure(trimmed),
            y: run.y - bottom,
            size,
            bold,
            italic,
        });
    }
    out
}

/// Advance widths of one font resource, enough to place the end of a run.
#[derive(Debug, Clone, Default)]
struct FontMetrics {
    base_font: String,
    first_char: u32,
    /// Glyph widths in thousandths of an em, from `/Widths`.
    widths: Vec<f64>,
    /// Width for codes outside the table: the average of the table.
    fallback: f64,
}

impl FontMetrics {
    fn width(&self, text: &str, size: f64) -> f64 {
        // Standard fonts often come without a width table; Helvetica's
        // metrics are a fair estimate for any proportional face.
        if self.widths.is_empty() {
            return crate::numbering::helvetica_text_width(text, size);
        }
        let total: f64 = text
            .chars()
            .map(|character| {
                (character as u32)
                    .checked_sub(self.first_char)
                    .and_then(|index| self.widths.get(index as usize))
                    .copied()
                    .filter(|width| *width > 0.0)
                    .unwrap_or(self.fallback)
            })
            .sum();
        total * size / 1000.0
    }
}

fn font_table(doc: &Document, page_id: ObjectId) -> HashMap<Vec<u8>, FontMetrics> {
    let Ok(fonts) = doc.get_page_fonts(page_id) else { return HashMap::new() };
    fonts.into_iter().map(|(name, font)| (name, font_metrics(doc, font))).collect()
}

fn font_metrics(doc: &Document, font: &Dictionary) -> FontMetrics {
    let resolve = |key: &[u8]| font.get(key).ok().and_then(|value| doc.dereference(value).ok()).map(|(_, value)| value);
    let base_font = resolve(b"BaseFont")
        .and_then(|value| value.as_name().ok())
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .unwrap_or_default();
    let first_char = resolve(b"FirstChar").and_then(object_to_f64).unwrap_or(0.0).max(0.0) as u32;
    let widths: Vec<f64> = resolve(b"Widths")
        .and_then(|value| value.as_array().ok())
        .map(|items| {
            items
                .iter()
                .map(|item| doc.dereference(item).ok().and_then(|(_, value)| object_to_f64(value)).unwrap_or(0.0))
                .collect()
        })
        .unwrap_or_default();
    let used: Vec<f64> = widths.iter().copied().filter(|width| *width > 0.0).collect();
    let fallback = if used.is_empty() { 500.0 } else { used.iter().sum::<f64>() / used.len() as f64 };
    FontMetrics { base_font, first_char, widths, fallback }
}

/// The page's MediaBox, inherited from the page tree when the page has none.
fn media_box(doc: &Document, page_id: ObjectId) -> Option<[f64; 4]> {
    let mut node = doc.get_dictionary(page_id).ok()?;
    for _ in 0..32 {
        if let Ok(value) = node.get(b"MediaBox") {
            let (_, value) = doc.dereference(value).ok()?;
            let numbers: Vec<f64> = value
                .as_array()
                .ok()?
                .iter()
                .filter_map(|item| doc.dereference(item).ok().and_then(|(_, value)| object_to_f64(value)))
                .collect();
            let [x0, y0, x1, y1] = numbers[..] else { return None };
            return Some([x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)]);
        }
        let parent = node.get(b"Parent").ok()?.as_reference().ok()?;
        node = doc.get_dictionary(parent).ok()?;
    }
    None
}

// ---------------------------------------------------------------------------
// Recovery
// ---------------------------------------------------------------------------

/// Rebuilds headings, paragraphs and list items from positioned text.
pub fn recover_pages(pages: &[PageText]) -> Vec<LayoutPage> {
    let mut lines: Vec<Vec<Line>> = pages.iter().map(|page| build_lines(&on_page(page))).collect();
    drop_running_lines(pages, &mut lines);
    let document_body = body_size(lines.iter().flatten());
    let drafts: Vec<Vec<Draft>> = lines
        .into_iter()
        .map(|page_lines| {
            let chars: usize = page_lines.iter().map(Line::chars).sum();
            // A title page or a short last page says little about the body
            // size; fall back to the whole document.
            let body = if chars >= 200 { body_size(page_lines.iter()) } else { document_body };
            let bold: usize = page_lines.iter().filter(|line| line.bold()).map(Line::chars).sum();
            let space = median_space_width(&page_lines);
            page_drafts(&reading_flows(page_lines), body, bold * 2 > chars, space)
        })
        .collect();
    // Heading levels rank the distinct heading sizes of the whole document.
    let mut sizes: Vec<i64> = drafts
        .iter()
        .flatten()
        .filter(|draft| heading_kind(draft) == Some(HeadingKind::Large))
        .map(|draft| size_key(draft.size()))
        .collect();
    sizes.sort_unstable_by(|a, b| b.cmp(a));
    sizes.dedup();
    pages
        .iter()
        .zip(drafts)
        .map(|(page, drafts)| LayoutPage {
            page: page.page,
            width: page.width,
            height: page.height,
            blocks: finish_blocks(drafts, &sizes),
        })
        .collect()
}

/// How far past the page box a fragment may start and still be kept.
const PAGE_MARGIN: f64 = 72.0;

/// The widest text range the gutter search scans, in page units.
const MAX_GUTTER_RANGE: f64 = 10_000.0;

/// The fragments of a page that sit on it (with a margin), without those with
/// coordinates that are not finite. Text placed far outside the page is
/// invisible and would only stretch the layout.
fn on_page(page: &PageText) -> Vec<TextFragment> {
    let sane = |value: f64, fallback: f64| if value.is_finite() && value > 0.0 { value } else { fallback };
    let (width, height) = (sane(page.width, 612.0), sane(page.height, 792.0));
    page.fragments
        .iter()
        .filter(|fragment| {
            fragment.x.is_finite()
                && fragment.y.is_finite()
                && fragment.width.is_finite()
                && fragment.size.is_finite()
                && fragment.x >= -PAGE_MARGIN
                && fragment.x <= width + PAGE_MARGIN
                && fragment.y >= -PAGE_MARGIN
                && fragment.y <= height + PAGE_MARGIN
        })
        .cloned()
        .collect()
}

/// One visual line: fragments on a shared baseline, left to right.
#[derive(Debug, Clone)]
struct Line {
    fragments: Vec<TextFragment>,
    text: String,
    y: f64,
    x0: f64,
    x1: f64,
    size: f64,
}

impl Line {
    fn new(mut fragments: Vec<TextFragment>) -> Line {
        fragments.sort_by(|a, b| a.x.total_cmp(&b.x));
        let text = spans_text(&assemble([fragments.as_slice()]));
        // The size carrying most characters sets the size and baseline, so a
        // superscript does not move the line.
        let size = dominant_size(fragments.iter().map(|fragment| (fragment.size, fragment.text.chars().count())));
        let y = fragments
            .iter()
            .filter(|fragment| (fragment.size - size).abs() < 0.06)
            .max_by_key(|fragment| fragment.text.chars().count())
            .or(fragments.first())
            .map_or(0.0, |fragment| fragment.y);
        let x0 = fragments.iter().map(|fragment| fragment.x).fold(f64::INFINITY, f64::min);
        let x1 = fragments.iter().map(|fragment| fragment.x + fragment.width).fold(f64::NEG_INFINITY, f64::max);
        Line { fragments, text, y, x0, x1, size }
    }

    fn chars(&self) -> usize {
        self.text.chars().filter(|character| !character.is_whitespace()).count()
    }

    fn bold(&self) -> bool {
        self.fragments.iter().all(|fragment| fragment.bold)
    }
}

/// Groups fragments into lines by baseline, top of the page first.
fn build_lines(fragments: &[TextFragment]) -> Vec<Line> {
    let mut sorted: Vec<&TextFragment> = fragments
        .iter()
        .filter(|fragment| {
            !fragment.text.trim().is_empty()
                && fragment.size > 0.0
                && fragment.x.is_finite()
                && fragment.y.is_finite()
                && fragment.width.is_finite()
        })
        .collect();
    sorted.sort_by(|a, b| b.y.total_cmp(&a.y).then(a.x.total_cmp(&b.x)));
    // (reference baseline, reference size, members)
    let mut groups: Vec<(f64, f64, Vec<TextFragment>)> = Vec::new();
    for fragment in sorted {
        if let Some((y, size, members)) = groups.last_mut() {
            if (*y - fragment.y).abs() <= 0.45 * size.max(fragment.size) {
                members.push(fragment.clone());
                // Larger glyphs set the reference, so a superscript that
                // sorted first does not hold the line above its baseline.
                if fragment.size > *size {
                    (*y, *size) = (fragment.y, fragment.size);
                }
                continue;
            }
        }
        groups.push((fragment.y, fragment.size, vec![fragment.clone()]));
    }
    groups.into_iter().map(|(_, _, members)| Line::new(members)).collect()
}

/// The weighted median font size: the size most of the text is set in.
fn body_size<'a>(lines: impl Iterator<Item = &'a Line>) -> f64 {
    let mut sizes: Vec<(f64, usize)> =
        lines.map(|line| (line.size, line.chars())).filter(|(_, chars)| *chars > 0).collect();
    sizes.sort_by(|a, b| a.0.total_cmp(&b.0));
    let total: usize = sizes.iter().map(|(_, chars)| chars).sum();
    let mut seen = 0;
    for (size, chars) in &sizes {
        seen += chars;
        if seen * 2 >= total {
            return *size;
        }
    }
    12.0
}

fn dominant_size(entries: impl Iterator<Item = (f64, usize)>) -> f64 {
    let mut totals: HashMap<i64, usize> = HashMap::new();
    for (size, chars) in entries {
        *totals.entry((size * 10.0).round() as i64).or_default() += chars.max(1);
    }
    totals.into_iter().max_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0))).map_or(12.0, |(key, _)| key as f64 / 10.0)
}

fn size_key(size: f64) -> i64 {
    (size * 2.0).round() as i64
}

fn percentile(values: &mut [f64], rank: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    Some(values[((values.len() - 1) as f64 * rank).round() as usize])
}

// ---------------------------------------------------------------------------
// Running headers, footers and page numbers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Band {
    Top,
    Bottom,
}

/// Removes running headers and footers and bare page numbers.
///
/// Only the two outermost lines in the top and bottom 15% of a page are
/// candidates. A candidate goes when the same text sits in the same band on at
/// least half the pages, when it differs only in its numbers on nearly every
/// page ("Report - page 3"), or when it is a page number set apart from the
/// body.
fn drop_running_lines(pages: &[PageText], lines: &mut [Vec<Line>]) {
    let candidates: Vec<Vec<(usize, Band)>> = pages
        .iter()
        .zip(lines.iter())
        .map(|(page, page_lines)| {
            let mut found = Vec::new();
            if page.height <= 0.0 {
                return found;
            }
            for (index, line) in page_lines.iter().enumerate().take(2) {
                if line.y < page.height * 0.85 {
                    break;
                }
                found.push((index, Band::Top));
            }
            for (index, line) in page_lines.iter().enumerate().rev().take(2) {
                if line.y > page.height * 0.15 || found.iter().any(|(taken, _)| *taken == index) {
                    break;
                }
                found.push((index, Band::Bottom));
            }
            found
        })
        .collect();

    let mut exact: HashMap<(Band, String), usize> = HashMap::new();
    let mut loose: HashMap<(Band, String), usize> = HashMap::new();
    for (found, page_lines) in candidates.iter().zip(lines.iter()) {
        let mut seen_exact = HashSet::new();
        let mut seen_loose = HashSet::new();
        for (index, band) in found {
            let (key, numbered) = running_keys(&page_lines[*index].text);
            if seen_exact.insert((*band, key.clone())) {
                *exact.entry((*band, key)).or_default() += 1;
            }
            if seen_loose.insert((*band, numbered.clone())) {
                *loose.entry((*band, numbered)).or_default() += 1;
            }
        }
    }

    let total = pages.len();
    for (found, page_lines) in candidates.into_iter().zip(lines.iter_mut()) {
        let mut drop = BTreeSet::new();
        for (index, band) in found {
            let line = &page_lines[index];
            let (key, numbered) = running_keys(&line.text);
            let repeated = exact.get(&(band, key)).copied().unwrap_or(0);
            let renumbered = loose.get(&(band, numbered)).copied().unwrap_or(0);
            if (repeated >= 2 && repeated * 2 >= total)
                || (renumbered >= 3 && renumbered * 5 >= total * 4)
                || (is_page_number(&line.text) && set_apart(page_lines, index, band))
            {
                drop.insert(index);
            }
        }
        for index in drop.into_iter().rev() {
            page_lines.remove(index);
        }
    }
}

/// The exact text of a line for comparison, and the same with every number
/// replaced by `#`.
fn running_keys(text: &str) -> (String, String) {
    let exact = text.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    let mut numbered = String::new();
    let mut in_number = false;
    for character in exact.chars() {
        if character.is_ascii_digit() {
            if !in_number {
                numbered.push('#');
            }
            in_number = true;
        } else {
            numbered.push(character);
            in_number = false;
        }
    }
    (exact, numbered)
}

/// "7", "- 7 -", "Page 7", "7 / 12", "Page 7 of 12", "vii".
fn is_page_number(text: &str) -> bool {
    let lower = text.trim().to_lowercase();
    let core = lower.trim_matches(|character: char| {
        character.is_whitespace() || matches!(character, '-' | '–' | '—' | '|' | '[' | ']' | '(' | ')' | '·' | '•')
    });
    let core = ["page", "pag.", "p.", "sayfa", "seite", "página", "pagina"]
        .iter()
        .find_map(|prefix| core.strip_prefix(prefix))
        .unwrap_or(core)
        .trim();
    let words: Vec<&str> = core
        .split(|character: char| character.is_whitespace() || character == '/')
        .filter(|word| !word.is_empty())
        .collect();
    let number = |word: &str| word.len() <= 4 && word.chars().all(|character| character.is_ascii_digit());
    match words.as_slice() {
        [single] => number(single) || roman_value(single).is_some(),
        [current, total] => number(current) && number(total),
        [current, joiner, total] => number(current) && ["of", "von", "de", "sur"].contains(joiner) && number(total),
        _ => false,
    }
}

/// True when the line is separated from the body by more than a line of space.
fn set_apart(lines: &[Line], index: usize, band: Band) -> bool {
    let line = &lines[index];
    let neighbour = match band {
        Band::Top => lines.get(index + 1),
        Band::Bottom => index.checked_sub(1).and_then(|before| lines.get(before)),
    };
    neighbour.is_none_or(|other| (line.y - other.y).abs() >= 1.6 * line.size.max(other.size))
}

// ---------------------------------------------------------------------------
// Columns
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Column {
    Full,
    Left,
    Right,
}

/// Lines that are read one after the other.
#[derive(Debug, Clone)]
struct Flow {
    lines: Vec<Line>,
    column: Column,
}

/// Orders a page's lines for reading.
///
/// Without a gutter the page is one flow. With one, lines that cross it
/// (a title or an abstract over both columns) are read in place, and each
/// stretch between them is read left column first, then right column.
fn reading_flows(lines: Vec<Line>) -> Vec<Flow> {
    let Some((start, end)) = find_gutter(&lines) else {
        return vec![Flow { lines, column: Column::Full }];
    };
    let middle = (start + end) / 2.0;
    let mut flows: Vec<Flow> = Vec::new();
    let mut left: Vec<Line> = Vec::new();
    let mut right: Vec<Line> = Vec::new();
    let flush = |flows: &mut Vec<Flow>, left: &mut Vec<Line>, right: &mut Vec<Line>| {
        if !left.is_empty() {
            flows.push(Flow { lines: std::mem::take(left), column: Column::Left });
        }
        if !right.is_empty() {
            flows.push(Flow { lines: std::mem::take(right), column: Column::Right });
        }
    };
    for line in lines {
        if crosses(&line, start, end) {
            flush(&mut flows, &mut left, &mut right);
            match flows.last_mut() {
                Some(flow) if flow.column == Column::Full => flow.lines.push(line),
                _ => flows.push(Flow { lines: vec![line], column: Column::Full }),
            }
            continue;
        }
        let (in_left, in_right): (Vec<TextFragment>, Vec<TextFragment>) =
            line.fragments.into_iter().partition(|fragment| fragment.x + fragment.width / 2.0 < middle);
        if !in_left.is_empty() {
            left.push(Line::new(in_left));
        }
        if !in_right.is_empty() {
            right.push(Line::new(in_right));
        }
    }
    flush(&mut flows, &mut left, &mut right);
    flows
}

fn crosses(line: &Line, start: f64, end: f64) -> bool {
    line.fragments.iter().any(|fragment| fragment.x < start - 1.0 && fragment.x + fragment.width > end + 1.0)
}

/// Finds the vertical gutter of a two-column page, as an x range.
///
/// The text block is scanned for the widest vertical stretch in its middle
/// half that the fewest lines touch (word gaps are closed first). It counts
/// as a gutter only when at most 40% of the lines cross it and both sides
/// hold prose: a quarter of the lines each, running most of the way to the
/// gutter, and each side is at least a fifth of the text width. The last two
/// tests keep label/value tables in one flow.
fn find_gutter(lines: &[Line]) -> Option<(f64, f64)> {
    if lines.len() < 6 {
        return None;
    }
    let left = lines.iter().map(|line| line.x0).fold(f64::INFINITY, f64::min);
    let right = lines.iter().map(|line| line.x1).fold(f64::NEG_INFINITY, f64::max);
    let width = right - left;
    // A range that is not finite, or absurdly wide, is not a page of text.
    if !width.is_finite() || width <= 120.0 || width > MAX_GUTTER_RANGE {
        return None;
    }
    let bins = width.ceil() as usize + 1;
    let mut coverage = vec![0usize; bins];
    for line in lines {
        let mut intervals: Vec<(f64, f64)> = Vec::new();
        for fragment in &line.fragments {
            let (from, to) = (fragment.x, fragment.x + fragment.width);
            match intervals.last_mut() {
                Some(last) if from - last.1 < 0.9 * line.size => last.1 = last.1.max(to),
                _ => intervals.push((from, to)),
            }
        }
        for (from, to) in intervals {
            let first = ((from - left).floor().max(0.0) as usize).min(bins);
            let last = ((to - left).ceil().max(0.0) as usize).min(bins);
            for slot in coverage.iter_mut().take(last).skip(first) {
                *slot += 1;
            }
        }
    }
    let low = (width * 0.25) as usize;
    let high = ((width * 0.75) as usize).min(bins - 1);
    let floor = *coverage[low..=high].iter().min()?;
    if floor * 10 > lines.len() * 4 {
        return None;
    }
    let mut best: Option<(usize, usize)> = None;
    let mut index = low;
    while index <= high {
        if coverage[index] > floor {
            index += 1;
            continue;
        }
        let (mut from, mut to) = (index, index);
        while to + 1 < bins && coverage[to + 1] <= floor {
            to += 1;
        }
        while from > 0 && coverage[from - 1] <= floor {
            from -= 1;
        }
        if best.is_none_or(|(a, b)| to - from > b - a) {
            best = Some((from, to));
        }
        index = to + 1;
    }
    let (from, to) = best?;
    let (start, end) = (left + from as f64, left + to as f64 + 1.0);
    let body = body_size(lines.iter());
    if end - start < (0.8 * body).max(6.0) {
        return None;
    }

    // Each side needs room for prose: a narrow label column is not one.
    if start - left < 0.2 * width || right - end < 0.2 * width {
        return None;
    }

    let middle = (start + end) / 2.0;
    let mut left_fill = Vec::new();
    let mut right_fill = Vec::new();
    for line in lines.iter().filter(|line| !crosses(line, start, end)) {
        let reach = |left_side: bool| {
            line.fragments
                .iter()
                .filter(|fragment| (fragment.x + fragment.width / 2.0 < middle) == left_side)
                .map(|fragment| fragment.x + fragment.width)
                .reduce(f64::max)
        };
        if let Some(reach) = reach(true) {
            left_fill.push((reach - left) / (start - left).max(1.0));
        }
        if let Some(reach) = reach(false) {
            right_fill.push((reach - end) / (right - end).max(1.0));
        }
    }
    let needed = ((lines.len() as f64) * 0.25).ceil().max(3.0) as usize;
    if left_fill.len() < needed || right_fill.len() < needed {
        return None;
    }
    if percentile(&mut left_fill, 0.5)? < 0.6 || percentile(&mut right_fill, 0.5)? < 0.6 {
        return None;
    }
    Some((start, end))
}

// ---------------------------------------------------------------------------
// Tables
// ---------------------------------------------------------------------------

/// A gap wider than this between two fragments opens a new table cell.
const TABLE_X_JUMP_PT: f64 = 18.0;

/// Cell starts of consecutive rows must line up within this many points.
const TABLE_ALIGN_PT: f64 = 6.0;

/// The fewest rows a run of cell lines needs to become a table.
const TABLE_MIN_ROWS: usize = 2;

/// The gap most word pairs are separated by: a proxy for the width of a space.
/// Table column gaps are far wider, so they are left out of the estimate.
fn median_space_width(lines: &[Line]) -> f64 {
    let mut gaps: Vec<f64> = Vec::new();
    for line in lines {
        for pair in line.fragments.windows(2) {
            let gap = pair[1].x - (pair[0].x + pair[0].width);
            if gap.is_finite() && gap > 0.0 && gap < TABLE_X_JUMP_PT {
                gaps.push(gap);
            }
        }
    }
    percentile(&mut gaps, 0.5).unwrap_or(3.0)
}

/// Finds table runs in top-to-bottom lines: each start maps to the end of the
/// run (exclusive) and the recovered table. The check is conservative: every
/// row needs at least two cells, consecutive rows must agree on the cell count
/// and their cell starts must line up, so an ordinary sentence never turns
/// into a table.
fn table_starts(lines: &[Line], space: f64) -> HashMap<usize, (usize, TableLayout)> {
    let mut tables = HashMap::new();
    let mut index = 0;
    while index < lines.len() {
        let Some(first) = split_cells(&lines[index], space) else {
            index += 1;
            continue;
        };
        let starts: Vec<f64> = first.iter().map(|cell| cell[0].x).collect();
        let mut rows = vec![cells_to_layout(first)];
        let mut end = index + 1;
        while end < lines.len() {
            let Some(cells) = split_cells(&lines[end], space) else { break };
            if cells.len() != starts.len() {
                break;
            }
            if !cells.iter().zip(&starts).all(|(cell, start)| (cell[0].x - start).abs() <= TABLE_ALIGN_PT) {
                break;
            }
            rows.push(cells_to_layout(cells));
            end += 1;
        }
        if rows.len() >= TABLE_MIN_ROWS {
            tables.insert(index, (end, TableLayout { rows }));
            index = end;
        } else {
            index += 1;
        }
    }
    tables
}

/// Splits a line into table cells: a gap wider than 2.5 spaces (or above
/// [`TABLE_X_JUMP_PT`]) opens the next cell. A line that does not split into
/// at least two cells is never a table row.
fn split_cells(line: &Line, space: f64) -> Option<Vec<&[TextFragment]>> {
    if line.fragments.len() < 2 {
        return None;
    }
    let limit = (2.5 * space).max(1.0);
    let mut cells: Vec<&[TextFragment]> = Vec::new();
    let mut start = 0;
    for index in 1..line.fragments.len() {
        let before = &line.fragments[index - 1];
        let gap = line.fragments[index].x - (before.x + before.width);
        if gap > limit || gap > TABLE_X_JUMP_PT {
            cells.push(&line.fragments[start..index]);
            start = index;
        }
    }
    cells.push(&line.fragments[start..]);
    (cells.len() >= 2).then_some(cells)
}

/// Builds a row of cells from fragment slices, joining each cell's fragments
/// into styled spans and trimming the result.
fn cells_to_layout(cells: Vec<&[TextFragment]>) -> Vec<TableCellLayout> {
    cells
        .into_iter()
        .map(|fragments| {
            let x = fragments.first().map_or(0.0, |fragment| fragment.x);
            let mut spans = assemble([fragments]);
            trim_spans(&mut spans);
            TableCellLayout { spans, x }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Paragraphs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Body,
    /// Every fragment bold, at body size.
    Bold,
    /// Noticeably larger than the body text.
    Large,
}

fn classify(line: &Line, body: f64, bold_body: bool) -> Class {
    if !line.text.chars().any(char::is_alphabetic) {
        return Class::Body;
    }
    if line.size >= body * 1.15 && line.size - body >= 1.0 {
        return Class::Large;
    }
    // On a page set in bold throughout, bold says nothing.
    if !bold_body && line.bold() {
        return Class::Bold;
    }
    Class::Body
}

/// A paragraph under construction.
#[derive(Debug, Clone)]
struct Draft {
    lines: Vec<Line>,
    class: Class,
    marker: Option<Marker>,
    /// Set for a table block: its lines were consumed by the table rows.
    table: Option<TableLayout>,
}

impl Draft {
    fn size(&self) -> f64 {
        dominant_size(self.lines.iter().map(|line| (line.size, line.chars())))
    }
}

/// Margins and line pitch of one column of a page.
struct FlowGeometry {
    left: f64,
    right: f64,
    pitch: f64,
    body: f64,
}

impl FlowGeometry {
    fn new(flows: &[&Flow], body: f64, page_pitch: f64) -> Option<FlowGeometry> {
        let lines: Vec<&Line> = flows.iter().flat_map(|flow| flow.lines.iter()).collect();
        let body_lines: Vec<&Line> = lines.iter().copied().filter(|line| (line.size - body).abs() <= 1.0).collect();
        let measured = if body_lines.is_empty() { &lines } else { &body_lines };
        let left = measured.iter().map(|line| line.x0).reduce(f64::min)?;
        // The 90th percentile, so one overlong line does not move the margin.
        let mut ends: Vec<f64> = measured.iter().map(|line| line.x1).collect();
        let right = percentile(&mut ends, 0.9)?;
        let pitch = line_pitch(flows.iter().flat_map(|flow| flow.lines.windows(2)), body).unwrap_or(page_pitch);
        Some(FlowGeometry { left, right, pitch, body })
    }

    fn pitch_for(&self, size: f64) -> f64 {
        if (size - self.body).abs() <= 1.0 {
            self.pitch
        } else {
            self.pitch * size / self.body
        }
    }
}

/// The distance between consecutive body lines inside a paragraph: the lower
/// quartile of the gaps, which skips the extra space between paragraphs.
fn line_pitch<'a>(pairs: impl Iterator<Item = &'a [Line]>, body: f64) -> Option<f64> {
    let mut gaps: Vec<f64> = pairs
        .filter(|pair| pair.len() == 2 && pair.iter().all(|line| (line.size - body).abs() <= 1.0))
        .map(|pair| pair[0].y - pair[1].y)
        .filter(|gap| *gap > 0.8 * body && *gap < 2.5 * body)
        .collect();
    percentile(&mut gaps, 0.25)
}

/// Splits a page's flows into paragraph drafts.
fn page_drafts(flows: &[Flow], body: f64, bold_body: bool, space: f64) -> Vec<Draft> {
    let page_pitch = line_pitch(flows.iter().flat_map(|flow| flow.lines.windows(2)), body).unwrap_or(1.2 * body);
    let geometry_of = |column: Column| {
        let members: Vec<&Flow> = flows.iter().filter(|flow| flow.column == column).collect();
        FlowGeometry::new(&members, body, page_pitch)
    };
    let (full, left, right) = (geometry_of(Column::Full), geometry_of(Column::Left), geometry_of(Column::Right));
    let mut drafts: Vec<Draft> = Vec::new();
    let mut previous_column: Option<Column> = None;
    for flow in flows {
        let geometry = match flow.column {
            Column::Full => full.as_ref(),
            Column::Left => left.as_ref(),
            Column::Right => right.as_ref(),
        };
        let Some(geometry) = geometry else { continue };
        // Rows of a table are consumed as one block; the remaining lines run
        // through the paragraph logic below.
        let tables = table_starts(&flow.lines, space);
        let mut index = 0;
        while index < flow.lines.len() {
            if let Some((end, table)) = tables.get(&index) {
                drafts.push(Draft { lines: Vec::new(), class: Class::Body, marker: None, table: Some(table.clone()) });
                index = *end;
                continue;
            }
            let line = &flow.lines[index];
            let class = classify(line, body, bold_body);
            let mut marker = if class == Class::Large { None } else { parse_marker(&line.text) };
            let boundary = match drafts.last() {
                None => true,
                // A sentence running from the foot of the left column on to
                // the top of the right one stays one paragraph.
                Some(current) if index == 0 => {
                    !(flow.column == Column::Right
                        && previous_column == Some(Column::Left)
                        && marker.is_none()
                        && continues_in_next_column(current, line, class))
                }
                Some(current) => breaks_between(current, line, class, geometry),
            };
            // A dash opening a wrapped line is punctuation, not a bullet,
            // unless the paragraph ends there anyway or a dash list is running.
            if marker.as_ref().is_some_and(|found| !found.strong) && !boundary {
                let same_list = drafts
                    .last()
                    .and_then(|current| current.marker.as_ref())
                    .zip(marker.as_ref())
                    .is_some_and(|(current, found)| current.text == found.text);
                if !same_list {
                    marker = None;
                }
            }
            match drafts.last_mut() {
                Some(current) if !boundary && marker.is_none() => current.lines.push(line.clone()),
                _ => drafts.push(Draft { lines: vec![line.clone()], class, marker, table: None }),
            }
            index += 1;
        }
        previous_column = Some(flow.column);
    }
    drafts
}

fn continues_in_next_column(current: &Draft, next: &Line, class: Class) -> bool {
    let Some(last) = current.lines.last() else { return false };
    current.class == Class::Body
        && class == Class::Body
        && (last.size - next.size).abs() <= 0.75
        && !ends_sentence(&last.text)
        && next.text.chars().next().is_some_and(char::is_lowercase)
}

/// True when `next` starts a new paragraph rather than continuing `current`.
fn breaks_between(current: &Draft, next: &Line, class: Class, geometry: &FlowGeometry) -> bool {
    let Some(previous) = current.lines.last() else { return true };
    if class != current.class {
        return true;
    }
    let size = previous.size.max(next.size);
    if (previous.size - next.size).abs() > (0.08 * size).max(0.75) {
        return true;
    }
    let gap = previous.y - next.y;
    if gap <= 0.0 || gap > geometry.pitch_for(size) * 1.3 + 0.5 {
        return true;
    }
    // A heading over several lines is held together by size and spacing alone.
    if class == Class::Large {
        return false;
    }
    let tolerance = (0.6 * next.size).max(4.0);
    if current.marker.is_some() {
        // List text wraps under the item; a line left of the marker is body
        // text again.
        if next.x0 < current.lines[0].x0 - tolerance {
            return true;
        }
    } else if next.x0 > previous.x0 + tolerance && (current.lines.len() > 1 || previous.x0 <= geometry.left + tolerance)
    {
        // A first-line indent opens the next paragraph.
        return true;
    }
    let width = geometry.right - geometry.left;
    if width <= 0.0 {
        return false;
    }
    let short = geometry.right - previous.x1;
    if ends_sentence(&previous.text) && short > (1.5 * previous.size).max(0.08 * width) {
        return true;
    }
    // A line that stops well short of the margin ends its paragraph.
    previous.x1 - geometry.left < 0.6 * width
}

fn ends_sentence(text: &str) -> bool {
    text.trim_end()
        .trim_end_matches(['"', '\'', '”', '’', ')', ']', '»', '*'])
        .ends_with(['.', '!', '?', ':', '…', '。', '！', '？'])
}

// ---------------------------------------------------------------------------
// Lists
// ---------------------------------------------------------------------------

const BULLETS: [char; 17] =
    ['•', '◦', '▪', '▫', '●', '○', '■', '□', '‣', '⁃', '∙', '·', '►', '▸', '➢', '\u{f0b7}', '\u{f0a7}'];
/// Markers that also open wrapped lines as punctuation.
const DASHES: [char; 3] = ['-', '–', '*'];

#[derive(Debug, Clone, PartialEq)]
struct Marker {
    /// As printed: "•", "2.", "(b)".
    text: String,
    /// Characters to strip from the start of the line: the marker and the
    /// space after it.
    len: usize,
    kind: MarkerKind,
    /// False for dashes and asterisks, see [`DASHES`].
    strong: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MarkerKind {
    Bullet,
    /// `roman` is set for a letter that also reads as a roman numeral (i, v, x).
    Number {
        style: NumberStyle,
        value: u32,
        roman: Option<u32>,
    },
}

/// Reads a list marker at the start of a line: a bullet, or "1." "1)" "a)"
/// "(a)" "iv." and the like followed by a space and text.
fn parse_marker(text: &str) -> Option<Marker> {
    let first = text.chars().next()?;
    if BULLETS.contains(&first) || DASHES.contains(&first) {
        let rest = &text[first.len_utf8()..];
        let body = rest.trim_start();
        let strong = BULLETS.contains(&first);
        // "- item" needs the space; "•item" does not. "--" and "**" are not markers.
        if body.is_empty() || (!strong && body.len() == rest.len()) || body.starts_with(first) {
            return None;
        }
        // Symbol and Wingdings bullets come through in the private use area.
        let printed = if ('\u{f000}'..='\u{f0ff}').contains(&first) { '•' } else { first };
        return Some(Marker {
            text: printed.to_string(),
            len: text.chars().count() - body.chars().count(),
            kind: MarkerKind::Bullet,
            strong,
        });
    }
    let (open, inner) = match text.strip_prefix('(') {
        Some(inner) => (true, inner),
        None => (false, text),
    };
    let token_len = inner.find(|character: char| !character.is_ascii_alphanumeric()).unwrap_or(inner.len());
    if token_len == 0 || token_len > 4 {
        return None;
    }
    let (token, after) = inner.split_at(token_len);
    let closing = after.chars().next()?;
    if closing != ')' && (closing != '.' || open) {
        return None;
    }
    let rest = &after[1..];
    let body = rest.trim_start();
    if body.is_empty() || body.len() == rest.len() {
        return None;
    }
    let (style, value, roman) = classify_token(token, closing)?;
    Some(Marker {
        text: text[..text.len() - rest.len()].to_string(),
        len: text.chars().count() - body.chars().count(),
        kind: MarkerKind::Number { style, value, roman },
        strong: true,
    })
}

fn classify_token(token: &str, closing: char) -> Option<(NumberStyle, u32, Option<u32>)> {
    if token.chars().all(|character| character.is_ascii_digit()) {
        return if token.len() <= 3 { Some((NumberStyle::Decimal, token.parse().ok()?, None)) } else { None };
    }
    let lower = token.chars().all(|character| character.is_ascii_lowercase());
    let upper = token.chars().all(|character| character.is_ascii_uppercase());
    if !lower && !upper {
        return None;
    }
    let roman = roman_value(token);
    let mut letters = token.chars();
    if let (Some(letter), None) = (letters.next(), letters.next()) {
        // "A. Smith" and "I. Newton" open sentences too often; a capital
        // letter needs a parenthesis to count.
        if upper && closing == '.' {
            return None;
        }
        let style = if lower { NumberStyle::LowerAlpha } else { NumberStyle::UpperAlpha };
        return Some((style, letter.to_ascii_lowercase() as u32 - 'a' as u32 + 1, roman));
    }
    let style = if lower { NumberStyle::LowerRoman } else { NumberStyle::UpperRoman };
    roman.map(|value| (style, value, None))
}

/// The value of a canonical roman numeral from i to xxxix, either case.
fn roman_value(token: &str) -> Option<u32> {
    let lower = token.to_ascii_lowercase();
    let mut total = 0u32;
    let mut largest = 0u32;
    for character in lower.chars().rev() {
        let value = match character {
            'i' => 1,
            'v' => 5,
            'x' => 10,
            'l' => 50,
            'c' => 100,
            'd' => 500,
            'm' => 1000,
            _ => return None,
        };
        if value < largest {
            total = total.checked_sub(value)?;
        } else {
            total += value;
            largest = value;
        }
    }
    // Only the canonical spelling: "iv" but not "iiii", and no words like "mix".
    let mut canonical = String::new();
    let mut rest = total;
    for (unit, numeral) in [(10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i")] {
        while rest >= unit {
            canonical.push_str(numeral);
            rest -= unit;
        }
    }
    ((1..=39).contains(&total) && canonical == lower).then_some(total)
}

/// Tracks nested lists down a page: marker columns give the level, and a run
/// of numbered items at one level shares the number of its first item.
#[derive(Default)]
struct ListTracker {
    /// Marker column of each open level and its numbering: style, start, last.
    levels: Vec<(f64, Option<(NumberStyle, u32, u32)>)>,
}

impl ListTracker {
    fn reset(&mut self) {
        self.levels.clear();
    }

    fn place(&mut self, x: f64, size: f64, marker: &Marker) -> ListItem {
        let tolerance = (0.6 * size).max(4.0);
        while self.levels.last().is_some_and(|(column, _)| x < column - tolerance) {
            self.levels.pop();
        }
        if !self.levels.last().is_some_and(|(column, _)| x <= column + tolerance) {
            self.levels.push((x, None));
        }
        let level = self.levels.len() - 1;
        let state = &mut self.levels[level].1;
        match marker.kind {
            MarkerKind::Bullet => {
                *state = None;
                ListItem { kind: ListKind::Bullet, level: level as u32, start: 1, marker: marker.text.clone() }
            }
            MarkerKind::Number { style, value, roman } => {
                let roman_style =
                    if style == NumberStyle::UpperAlpha { NumberStyle::UpperRoman } else { NumberStyle::LowerRoman };
                // "v" after "iv" is a numeral, "i" after "h" is a letter, and a
                // list opening with "i" counts in numerals.
                let (style, value) = match (roman, *state) {
                    (Some(numeral), Some((current, _, last))) if current == roman_style && numeral == last + 1 => {
                        (roman_style, numeral)
                    }
                    (Some(_), Some((current, _, last))) if current == style && value == last + 1 => (style, value),
                    (Some(1), _) => (roman_style, 1),
                    _ => (style, value),
                };
                let start = match *state {
                    Some((current, start, last)) if current == style && value == last + 1 => start,
                    _ => value,
                };
                *state = Some((style, start, value));
                ListItem { kind: ListKind::Numbered(style), level: level as u32, start, marker: marker.text.clone() }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Blocks
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeadingKind {
    /// Set larger than the body text.
    Large,
    /// A short bold line at body size.
    Bold,
}

fn heading_kind(draft: &Draft) -> Option<HeadingKind> {
    if draft.marker.is_some() {
        return None;
    }
    let chars: usize = draft.lines.iter().map(|line| line.text.chars().count()).sum();
    let last = draft.lines.last()?.text.trim_end();
    match draft.class {
        Class::Large if draft.lines.len() <= 3 && chars <= 200 => Some(HeadingKind::Large),
        Class::Bold if draft.lines.len() == 1 && chars <= 100 && !last.ends_with('.') => Some(HeadingKind::Bold),
        _ => None,
    }
}

fn finish_blocks(drafts: Vec<Draft>, heading_sizes: &[i64]) -> Vec<LayoutBlock> {
    let mut lists = ListTracker::default();
    let mut blocks = Vec::new();
    for mut draft in drafts {
        let size = draft.size();
        if let Some(table) = draft.table.take() {
            lists.reset();
            blocks.push(LayoutBlock { kind: BlockKind::Table(table), spans: Vec::new(), size });
            continue;
        }
        let mut spans = assemble(draft.lines.iter().map(|line| line.fragments.as_slice()));
        let kind = match &draft.marker {
            Some(marker) => {
                strip_leading(&mut spans, marker.len);
                BlockKind::ListItem(lists.place(draft.lines[0].x0, size, marker))
            }
            None => {
                lists.reset();
                match heading_kind(&draft) {
                    Some(HeadingKind::Large) => {
                        let rank = heading_sizes.iter().position(|key| *key == size_key(size)).unwrap_or(2);
                        BlockKind::Heading(rank.min(2) as u32 + 1)
                    }
                    // Bold body-size headings rank below every larger one.
                    Some(HeadingKind::Bold) => BlockKind::Heading(heading_sizes.len().min(2) as u32 + 1),
                    None => BlockKind::Paragraph,
                }
            }
        };
        trim_spans(&mut spans);
        if !spans.is_empty() {
            blocks.push(LayoutBlock { kind, spans, size });
        }
    }
    blocks
}

/// Joins fragments into styled text: a space where the gap between two
/// fragments is wider than a thin space, and line ends joined with a space or
/// across a hyphen.
fn assemble<'a>(lines: impl IntoIterator<Item = &'a [TextFragment]>) -> Vec<Span> {
    let mut spans: Vec<Span> = Vec::new();
    for (line_index, fragments) in lines.into_iter().enumerate() {
        let mut previous: Option<&TextFragment> = None;
        for fragment in fragments {
            let text = fragment.text.split_whitespace().collect::<Vec<_>>().join(" ");
            if text.is_empty() {
                continue;
            }
            match previous {
                Some(before) => {
                    if fragment.x - (before.x + before.width) > 0.15 * before.size.min(fragment.size) {
                        push_space(&mut spans);
                    }
                }
                None if line_index > 0 => join_line_end(&mut spans, &text),
                None => {}
            }
            match spans.last_mut() {
                Some(last) if last.bold == fragment.bold && last.italic == fragment.italic => last.text.push_str(&text),
                _ => spans.push(Span { text, bold: fragment.bold, italic: fragment.italic }),
            }
            previous = Some(fragment);
        }
    }
    spans
}

fn push_space(spans: &mut [Span]) {
    if let Some(last) = spans.last_mut() {
        if !last.text.ends_with(' ') {
            last.text.push(' ');
        }
    }
}

/// Joins the next line on: across a hyphen when the word goes on in lowercase
/// ("exam-" + "ple"), keeping the hyphen of a compound split before a capital
/// ("Anglo-" + "Saxon"), and with a space otherwise.
fn join_line_end(spans: &mut Vec<Span>, next: &str) {
    let Some(last) = spans.last_mut() else { return };
    let mut ending = last.text.chars().rev();
    let (end, before) = (ending.next(), ending.next());
    match end {
        Some('\u{ad}') => {
            last.text.pop();
        }
        Some('-' | '\u{2010}') if before.is_some_and(char::is_alphabetic) => {
            if next.chars().next().is_some_and(char::is_lowercase) {
                last.text.pop();
            }
        }
        _ => {
            if !last.text.ends_with(' ') {
                last.text.push(' ');
            }
        }
    }
    if last.text.is_empty() {
        spans.pop();
    }
}

/// Removes the first `count` characters (a list marker) from the spans.
fn strip_leading(spans: &mut Vec<Span>, mut count: usize) {
    while count > 0 && !spans.is_empty() {
        let length = spans[0].text.chars().count();
        if length <= count {
            count -= length;
            spans.remove(0);
        } else {
            spans[0].text = spans[0].text.chars().skip(count).collect();
            count = 0;
        }
    }
}

fn trim_spans(spans: &mut Vec<Span>) {
    while let Some(first) = spans.first_mut() {
        first.text = first.text.trim_start().to_string();
        if !first.text.is_empty() {
            break;
        }
        spans.remove(0);
    }
    while let Some(last) = spans.last_mut() {
        last.text.truncate(last.text.trim_end().len());
        if !last.text.is_empty() {
            break;
        }
        spans.pop();
    }
}

fn spans_text(spans: &[Span]) -> String {
    spans.iter().map(|span| span.text.as_str()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fragment whose width follows Helvetica's metrics, like a real word.
    fn word(text: &str, x: f64, y: f64, size: f64) -> TextFragment {
        let width = crate::numbering::helvetica_text_width(text, size);
        TextFragment { text: text.into(), x, width, y, size, bold: false, italic: false }
    }

    fn bold(mut fragment: TextFragment) -> TextFragment {
        fragment.bold = true;
        fragment
    }

    /// One fragment per word of `text`, laid out from `x` on baseline `y`.
    fn line(text: &str, x: f64, y: f64, size: f64) -> Vec<TextFragment> {
        let space = crate::numbering::helvetica_text_width(" ", size);
        let mut cursor = x;
        text.split(' ')
            .map(|part| {
                let fragment = word(part, cursor, y, size);
                cursor += fragment.width + space;
                fragment
            })
            .collect()
    }

    fn page(number: u32, lines: Vec<Vec<TextFragment>>) -> PageText {
        PageText { page: number, width: 595.0, height: 842.0, fragments: lines.into_iter().flatten().collect() }
    }

    fn texts(page: &LayoutPage) -> Vec<String> {
        page.blocks.iter().map(LayoutBlock::text).collect()
    }

    const BODY: [&str; 6] = [
        "Field teams measured the water level at every station along the river during",
        "the spring survey and compared the readings with the records of the previous",
        "year, which showed a steady rise.",
        "The second paragraph starts after a larger gap and explains how the sensors",
        "were calibrated before each run.",
        "Short closing line.",
    ];

    #[test]
    fn merges_wrapped_lines_and_splits_on_spacing_and_short_lines() {
        let pages = vec![page(
            1,
            vec![
                line(BODY[0], 72.0, 700.0, 11.0),
                line(BODY[1], 72.0, 686.0, 11.0),
                line(BODY[2], 72.0, 672.0, 11.0),
                line(BODY[3], 72.0, 648.0, 11.0),
                line(BODY[4], 72.0, 634.0, 11.0),
                line(BODY[5], 72.0, 620.0, 11.0),
            ],
        )];
        let recovered = recover_pages(&pages);
        let blocks = texts(&recovered[0]);
        assert_eq!(blocks.len(), 3, "{blocks:#?}");
        assert_eq!(blocks[0], format!("{} {} {}", BODY[0], BODY[1], BODY[2]));
        assert_eq!(blocks[1], format!("{} {}", BODY[3], BODY[4]));
        assert_eq!(blocks[2], BODY[5]);
        assert!(recovered[0].blocks.iter().all(|block| block.kind == BlockKind::Paragraph));
    }

    #[test]
    fn a_first_line_indent_starts_a_paragraph() {
        let pages = vec![page(
            1,
            vec![
                line(BODY[0], 90.0, 700.0, 11.0),
                line(BODY[1], 72.0, 686.0, 11.0),
                line(BODY[0], 90.0, 672.0, 11.0),
                line(BODY[1], 72.0, 658.0, 11.0),
            ],
        )];
        let blocks = texts(&recover_pages(&pages)[0]);
        assert_eq!(blocks.len(), 2, "{blocks:#?}");
    }

    #[test]
    fn joins_hyphenated_line_ends_only_before_lowercase() {
        let pages = vec![page(
            1,
            vec![
                line("This sentence is long enough to wrap and ends with an exam-", 72.0, 700.0, 11.0),
                line("ple that continues here, followed by a long tail of Anglo-", 72.0, 686.0, 11.0),
                line("Saxon words.", 72.0, 672.0, 11.0),
            ],
        )];
        let blocks = texts(&recover_pages(&pages)[0]);
        assert_eq!(blocks.len(), 1, "{blocks:#?}");
        assert!(blocks[0].contains(" example that"), "{}", blocks[0]);
        assert!(blocks[0].contains("Anglo-Saxon words."), "{}", blocks[0]);
    }

    #[test]
    fn larger_lines_become_headings_ranked_by_size() {
        let pages = vec![
            page(
                1,
                vec![
                    line("Annual Report", 72.0, 760.0, 24.0),
                    line("Overview", 72.0, 720.0, 16.0),
                    line(BODY[0], 72.0, 700.0, 11.0),
                    line(BODY[2], 72.0, 686.0, 11.0),
                ],
            ),
            page(
                2,
                vec![
                    line("Details", 72.0, 760.0, 16.0),
                    vec![bold(word("Method", 72.0, 740.0, 11.0))],
                    line(BODY[3], 72.0, 724.0, 11.0),
                    line(BODY[4], 72.0, 710.0, 11.0),
                ],
            ),
        ];
        let recovered = recover_pages(&pages);
        let kinds: Vec<&BlockKind> =
            recovered.iter().flat_map(|page| page.blocks.iter().map(|block| &block.kind)).collect();
        assert_eq!(
            kinds,
            vec![
                &BlockKind::Heading(1),
                &BlockKind::Heading(2),
                &BlockKind::Paragraph,
                &BlockKind::Heading(2),
                &BlockKind::Heading(3),
                &BlockKind::Paragraph,
            ]
        );
        assert_eq!(recovered[1].blocks[1].text(), "Method");
        assert!(recovered[1].blocks[1].spans[0].bold);
    }

    #[test]
    fn detects_bullets_and_numbered_items_and_strips_markers() {
        let pages = vec![page(
            1,
            vec![
                line("Before the list comes a sentence that introduces it:", 72.0, 700.0, 11.0),
                line("• First point", 72.0, 686.0, 11.0),
                line("• Second point that is long enough to wrap onto a second line in", 72.0, 672.0, 11.0),
                line("the column", 82.0, 658.0, 11.0),
                line("1. Collect samples", 72.0, 634.0, 11.0),
                line("2. Label them", 72.0, 620.0, 11.0),
                line("a) by site", 90.0, 606.0, 11.0),
                line("b) by date", 90.0, 592.0, 11.0),
                line("3. Ship them", 72.0, 578.0, 11.0),
            ],
        )];
        let recovered = recover_pages(&pages);
        let blocks = &recovered[0].blocks;
        assert_eq!(
            texts(&recovered[0])[1..],
            [
                "First point",
                "Second point that is long enough to wrap onto a second line in the column",
                "Collect samples",
                "Label them",
                "by site",
                "by date",
                "Ship them"
            ]
        );
        let item = |index: usize| match &blocks[index].kind {
            BlockKind::ListItem(item) => item.clone(),
            other => panic!("block {index} is {other:?}"),
        };
        assert_eq!(item(1).kind, ListKind::Bullet);
        assert_eq!(item(2).kind, ListKind::Bullet);
        assert_eq!((item(3).kind, item(3).level, item(3).start), (ListKind::Numbered(NumberStyle::Decimal), 0, 1));
        assert_eq!((item(4).start, item(4).marker.as_str()), (1, "2."));
        assert_eq!((item(5).kind, item(5).level), (ListKind::Numbered(NumberStyle::LowerAlpha), 1));
        assert_eq!(item(6).marker, "b)");
        assert_eq!((item(7).level, item(7).start), (0, 1));
    }

    #[test]
    fn markers_need_a_space_and_dashes_need_a_paragraph_boundary() {
        assert!(parse_marker("1.5 million people").is_none());
        assert!(parse_marker("2024. The year").is_none());
        assert!(parse_marker("e.g. this").is_none());
        assert!(parse_marker("A. Smith wrote").is_none());
        assert!(parse_marker("-5 degrees").is_none());
        assert!(parse_marker("mix. it").is_none());
        assert_eq!(
            parse_marker("(iv) fourth").map(|marker| marker.kind),
            Some(MarkerKind::Number { style: NumberStyle::LowerRoman, value: 4, roman: None })
        );
        assert_eq!(parse_marker("•item").map(|marker| marker.len), Some(1));
        assert_eq!(parse_marker("\u{f0b7}\titem").map(|marker| marker.text), Some("•".to_string()));

        // "– and" opening a wrapped line is an en dash in the sentence.
        let pages = vec![page(
            1,
            vec![
                line("The survey covered every station on the river from the source to the", 72.0, 700.0, 11.0),
                line("– and this matters – the delta, where readings were taken twice a day.", 72.0, 686.0, 11.0),
            ],
        )];
        let recovered = recover_pages(&pages);
        assert_eq!(recovered[0].blocks.len(), 1);
        assert_eq!(recovered[0].blocks[0].kind, BlockKind::Paragraph);
    }

    #[test]
    fn roman_numerals_and_letters_are_told_apart_by_context() {
        let numerals = vec![page(
            1,
            vec![
                line("i. one", 72.0, 700.0, 11.0),
                line("ii. two", 72.0, 686.0, 11.0),
                line("iii. three", 72.0, 672.0, 11.0),
            ],
        )];
        let recovered = recover_pages(&numerals);
        assert!(recovered[0].blocks.iter().all(|block| matches!(&block.kind, BlockKind::ListItem(item) if item.kind == ListKind::Numbered(NumberStyle::LowerRoman))));
        let letters = vec![page(1, vec![line("h) eighth", 72.0, 700.0, 11.0), line("i) ninth", 72.0, 686.0, 11.0)])];
        let recovered = recover_pages(&letters);
        assert!(
            matches!(&recovered[0].blocks[1].kind, BlockKind::ListItem(item) if item.kind == ListKind::Numbered(NumberStyle::LowerAlpha))
        );
    }

    #[test]
    fn reads_the_left_column_before_the_right() {
        let left = [
            "Left column text starts the",
            "article and runs down the",
            "page in narrow lines that",
            "fill the column width well",
            "until this sentence ends.",
        ];
        let right = [
            "Right column text continues",
            "with its own paragraph that",
            "also runs in narrow lines to",
            "the bottom of the page where",
            "the right column ends too.",
        ];
        let mut lines = vec![line("Two Column Article", 72.0, 760.0, 20.0)];
        for (row, (l, r)) in left.iter().zip(right.iter()).enumerate() {
            let y = 720.0 - row as f64 * 13.0;
            lines.push(line(l, 72.0, y, 10.0));
            lines.push(line(r, 320.0, y, 10.0));
        }
        let recovered = recover_pages(&[page(1, lines)]);
        let blocks = texts(&recovered[0]);
        assert_eq!(blocks.len(), 3, "{blocks:#?}");
        assert_eq!(blocks[0], "Two Column Article");
        assert_eq!(blocks[1], left.join(" "));
        assert_eq!(blocks[2], right.join(" "));
    }

    #[test]
    fn a_sentence_carries_over_from_the_left_column_to_the_right() {
        let left = [
            "Earlier text opens the left",
            "column.",
            "The first column holds",
            "a sentence that does not",
            "end at the foot of the",
            "column but carries on to",
            "the top of the next one",
        ];
        let right = [
            "and finishes there.",
            "Another paragraph then",
            "runs down the column in",
            "lines of similar width to",
            "the lines on the left.",
            "A closing line follows",
            "and then the page ends.",
        ];
        let (left, right) = (&left[..], &right[..]);
        let mut lines = Vec::new();
        for (row, (l, r)) in left.iter().zip(right.iter()).enumerate() {
            let y = 720.0 - row as f64 * 13.0;
            lines.push(line(l, 72.0, y, 10.0));
            lines.push(line(r, 320.0, y, 10.0));
        }
        let blocks = texts(&recover_pages(&[page(1, lines)])[0]);
        assert_eq!(blocks[1], format!("{} {}", left[2..].join(" "), "and finishes there."), "{blocks:#?}");
    }

    #[test]
    fn a_label_value_table_is_not_read_as_columns() {
        let mut lines = Vec::new();
        for row in 0..8 {
            let y = 720.0 - row as f64 * 14.0;
            lines.push(line("Label", 72.0, y, 10.0));
            lines.push(line("A value that is long enough to fill the right side", 320.0, y, 10.0));
        }
        let page_lines = build_lines(&page(1, lines).fragments);
        assert!(find_gutter(&page_lines).is_none());
    }

    #[test]
    fn drops_running_headers_footers_and_page_numbers() {
        let pages: Vec<PageText> = (1..=3)
            .map(|number| {
                page(
                    number,
                    vec![
                        line("ACME Quarterly Review", 72.0, 805.0, 9.0),
                        line(BODY[0], 72.0, 700.0, 11.0),
                        line(BODY[2], 72.0, 686.0, 11.0),
                        line(&format!("Confidential draft {number}"), 72.0, 50.0, 9.0),
                        line(&format!("- {number} -"), 290.0, 30.0, 9.0),
                    ],
                )
            })
            .collect();
        for page in recover_pages(&pages) {
            assert_eq!(texts(&page), vec![format!("{} {}", BODY[0], BODY[2])]);
        }
    }

    #[test]
    fn keeps_a_number_that_is_part_of_the_body() {
        let pages = vec![page(
            1,
            vec![
                line(BODY[0], 72.0, 700.0, 11.0),
                line(BODY[2], 72.0, 686.0, 11.0),
                line("Total", 72.0, 120.0, 11.0),
                line("42", 72.0, 106.0, 11.0),
            ],
        )];
        assert!(texts(&recover_pages(&pages)[0]).iter().any(|text| text.contains("42")));
    }

    #[test]
    fn keeps_bold_and_italic_runs() {
        let mut first = line("plain words then", 72.0, 700.0, 11.0);
        let mut emphasis = word("bold", 72.0 + 90.0, 700.0, 11.0);
        emphasis.bold = true;
        let mut slanted = word("slanted", 72.0 + 120.0, 700.0, 11.0);
        slanted.italic = true;
        first.push(emphasis);
        first.push(slanted);
        let recovered = recover_pages(&[page(1, vec![first])]);
        let spans = &recovered[0].blocks[0].spans;
        assert_eq!(spans.len(), 3, "{spans:#?}");
        assert_eq!((spans[1].text.trim(), spans[1].bold), ("bold", true));
        assert_eq!((spans[2].text.as_str(), spans[2].italic), ("slanted", true));
    }

    #[test]
    fn font_names_carry_the_style() {
        assert_eq!(font_style("ABCDEF+Arial-BoldMT"), (true, false));
        assert_eq!(font_style("Helvetica-BoldOblique"), (true, true));
        assert_eq!(font_style("TimesNewRomanPS-ItalicMT"), (false, true));
        assert_eq!(font_style("MinionPro-It"), (false, true));
        assert_eq!(font_style("CMBX10"), (true, false));
        assert_eq!(font_style("Helvetica"), (false, false));
    }

    /// A table row: the words of a left and a right cell on one baseline.
    fn row(left: &str, x: f64, right: &str, rx: f64, y: f64, bold_left: bool) -> Vec<TextFragment> {
        let mut fragments: Vec<TextFragment> = line(left, x, y, 11.0);
        if bold_left {
            fragments = fragments.into_iter().map(bold).collect();
        }
        fragments.extend(line(right, rx, y, 11.0));
        fragments
    }

    #[test]
    fn recovers_a_two_column_table_from_aligned_rows() {
        let pages = vec![page(
            1,
            vec![
                row("Station", 72.0, "Flow rate", 300.0, 700.0, true),
                row("North bridge", 72.0, "12", 300.0, 686.0, false),
                row("Old mill", 72.0, "9", 300.0, 672.0, false),
            ],
        )];
        let recovered = recover_pages(&pages);
        assert_eq!(recovered[0].blocks.len(), 1, "{:#?}", recovered[0].blocks);
        let BlockKind::Table(table) = &recovered[0].blocks[0].kind else {
            panic!("a table block, got {:?}", recovered[0].blocks[0].kind);
        };
        assert_eq!(table.rows.len(), 3);
        let cell = |row: usize, column: usize| {
            table.rows[row][column].spans.iter().map(|span| span.text.as_str()).collect::<String>()
        };
        assert_eq!(cell(0, 0), "Station");
        assert_eq!(cell(0, 1), "Flow rate");
        assert_eq!(cell(1, 0), "North bridge");
        assert_eq!(cell(1, 1), "12");
        assert_eq!(cell(2, 0), "Old mill");
        assert_eq!(cell(2, 1), "9");
        assert!(table.rows[0][0].spans.iter().all(|span| span.bold));
        assert!(!table.rows[1][0].spans.iter().any(|span| span.bold));
    }

    #[test]
    fn doubled_spaces_in_prose_do_not_make_a_table() {
        let pages = vec![page(
            1,
            vec![
                line("The gauges were read twice a day and the readings were", 72.0, 700.0, 11.0),
                line("checked against a reference stick  before every run at noon", 72.0, 686.0, 11.0),
            ],
        )];
        let recovered = recover_pages(&pages);
        assert!(
            recovered[0].blocks.iter().all(|block| block.kind == BlockKind::Paragraph),
            "{:#?}",
            recovered[0].blocks
        );
    }

    #[test]
    fn rows_whose_cells_do_not_line_up_stay_prose() {
        let pages = vec![page(
            1,
            vec![
                row("Alpha", 72.0, "first value", 300.0, 700.0, false),
                row("Beta", 140.0, "second value", 360.0, 686.0, false),
            ],
        )];
        let recovered = recover_pages(&pages);
        assert!(
            recovered[0].blocks.iter().all(|block| block.kind == BlockKind::Paragraph),
            "{:#?}",
            recovered[0].blocks
        );
    }

    #[test]
    fn page_numbers_in_common_forms() {
        for text in ["7", "- 7 -", "Page 7", "page 7 of 12", "7 / 12", "vii", "Sayfa 3"] {
            assert!(is_page_number(text), "{text}");
        }
        for text in ["Chapter 7", "7 apples", "Total 42 units"] {
            assert!(!is_page_number(text), "{text}");
        }
    }
}
