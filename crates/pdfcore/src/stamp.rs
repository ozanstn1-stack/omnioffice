//! Header/footer text and Bates numbering stamps.
//!
//! Both stamps append a new content revision to every selected page, so
//! signed documents keep their signatures. Text is drawn with base14
//! Helvetica; `{page}` and `{pages}` placeholders are substituted per page.

use crate::docutil::{
    add_resource_entry, append_page_content, load_document, materialize_all_pages, page_mediabox, page_rotation,
    resolve_output_path, save_document, Matrix, OverwritePolicy,
};
use crate::error::{PdfError, PdfResult};
use crate::progress::{CancelToken, ProgressCallback, ProgressEvent};
use lopdf::{dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Header and footer templates; empty strings skip that slot.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HeaderFooterOptions {
    pub header_left: String,
    pub header_center: String,
    pub header_right: String,
    pub footer_left: String,
    pub footer_center: String,
    pub footer_right: String,
    pub font_size_pt: f64,
    pub color: String,
    pub margin_pt: f64,
    /// 1-based pages; empty means every page.
    pub pages: Vec<u32>,
    /// Visible number of the first selected page (`{page}` on it).
    pub start_number: u32,
    /// Number pages by their position in the selection instead of the document.
    pub count_from_start: bool,
}

impl Default for HeaderFooterOptions {
    fn default() -> Self {
        Self {
            header_left: String::new(),
            header_center: String::new(),
            header_right: String::new(),
            footer_left: String::new(),
            footer_center: String::new(),
            footer_right: String::new(),
            font_size_pt: 9.0,
            color: "#334155".into(),
            margin_pt: 24.0,
            pages: Vec::new(),
            start_number: 1,
            count_from_start: true,
        }
    }
}

/// Bates numbering: `{prefix}{number}{suffix}` with zero padding.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BatesOptions {
    pub prefix: String,
    pub suffix: String,
    pub start: u32,
    /// Zero-padded digit count for the number part.
    pub digits: u32,
    /// `bottomLeft` | `bottomCenter` | `bottomRight` | `topLeft` | `topCenter` | `topRight`.
    pub position: String,
    pub font_size_pt: f64,
    pub color: String,
    pub margin_pt: f64,
    /// 1-based pages; empty means every page.
    pub pages: Vec<u32>,
}

impl Default for BatesOptions {
    fn default() -> Self {
        Self {
            prefix: String::new(),
            suffix: String::new(),
            start: 1,
            digits: 6,
            position: "bottomRight".into(),
            font_size_pt: 8.0,
            color: "#334155".into(),
            margin_pt: 24.0,
            pages: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Base14 Helvetica text (same metrics as `numbering.rs`, kept local so the
// numbering module stays untouched)
// ---------------------------------------------------------------------------

/// Helvetica AFM widths (units/1000) for ASCII 32..126.
#[rustfmt::skip]
const HELVETICA_WIDTHS: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278,
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556,
    1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778,
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556,
    333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556,
    556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
];

fn helvetica_text_width(text: &str, size_pt: f64) -> f64 {
    let mut width = 0.0;
    for ch in text.chars() {
        let code = ch as u32;
        let w = if (32..=126).contains(&code) { HELVETICA_WIDTHS[(code - 32) as usize] as f64 } else { 556.0 };
        width += w;
    }
    width / 1000.0 * size_pt
}

/// Adds the base14 Helvetica font to the page resources under `/Helv`.
fn ensure_helvetica_font(doc: &mut Document, page_id: ObjectId) -> PdfResult<()> {
    let existing = {
        let resources = doc.get_dictionary(page_id)?.get(b"Resources").ok();
        resources
            .and_then(|r| match r {
                Object::Reference(id) => doc.get_dictionary(*id).ok(),
                Object::Dictionary(d) => Some(d),
                _ => None,
            })
            .and_then(|d| d.get(b"Font").ok())
            .cloned()
    };
    let has_helv = match &existing {
        Some(Object::Reference(id)) => doc.get_dictionary(*id).map(|d| d.has(b"Helv")).unwrap_or(false),
        Some(Object::Dictionary(d)) => d.has(b"Helv"),
        _ => false,
    };
    if has_helv {
        return Ok(());
    }
    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    }));
    add_resource_entry(doc, page_id, b"Font", "Helv", Object::Reference(font_id))
}

// ---------------------------------------------------------------------------
// Placement
// ---------------------------------------------------------------------------

/// The six stamp slots: left/center/right across the top or bottom edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    TopLeft,
    TopCenter,
    TopRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl Slot {
    /// Bates positions arrive as camelCase strings; unknown values fall back
    /// to the default bottom-right corner rather than failing the stamp.
    fn from_position(position: &str) -> Slot {
        match position {
            "topLeft" | "top_left" | "top-left" => Slot::TopLeft,
            "topCenter" | "top_center" | "top-center" => Slot::TopCenter,
            "topRight" | "top_right" | "top-right" => Slot::TopRight,
            "bottomLeft" | "bottom_left" | "bottom-left" => Slot::BottomLeft,
            "bottomCenter" | "bottom_center" | "bottom-center" => Slot::BottomCenter,
            _ => Slot::BottomRight,
        }
    }
}

/// A page's rotation-aware drawing canvas in display space.
struct PageCanvas {
    to_page: Matrix,
    display_width: f64,
    display_height: f64,
}

impl PageCanvas {
    fn new(rotation: i32, media_box: [f64; 4]) -> Self {
        let page_w = media_box[2] - media_box[0];
        let page_h = media_box[3] - media_box[1];
        let (display_width, display_height) = Matrix::displayed_size(rotation, page_w, page_h);
        Self { to_page: Matrix::display_to_page(rotation, page_w, page_h), display_width, display_height }
    }
}

fn draw_text(
    out: &mut String,
    canvas: &PageCanvas,
    slot: Slot,
    text: &str,
    font_size_pt: f64,
    color: [u8; 4],
    margin_pt: f64,
) {
    if text.is_empty() {
        return;
    }
    let width = helvetica_text_width(text, font_size_pt);
    let baseline = font_size_pt * 0.25;
    let top = canvas.display_height - margin_pt - font_size_pt;
    let bottom = margin_pt + baseline;
    let (x, y) = match slot {
        Slot::TopLeft => (margin_pt, top),
        Slot::TopCenter => ((canvas.display_width - width) / 2.0, top),
        Slot::TopRight => (canvas.display_width - margin_pt - width, top),
        Slot::BottomLeft => (margin_pt, bottom),
        Slot::BottomCenter => ((canvas.display_width - width) / 2.0, bottom),
        Slot::BottomRight => (canvas.display_width - margin_pt - width, bottom),
    };
    out.push_str(&format!(
        "q\n{}\nBT\n{r:.3} {g:.3} {b:.3} rg\n/Helv {size:.2} Tf\n{x:.2} {y:.2} Td\n({text}) Tj\nET\nQ\n",
        canvas.to_page.to_cm(),
        r = color[0] as f64 / 255.0,
        g = color[1] as f64 / 255.0,
        b = color[2] as f64 / 255.0,
        size = font_size_pt,
        x = x,
        y = y,
        text = crate::docutil::escape_pdf_literal(text),
    ));
}

// ---------------------------------------------------------------------------
// Text generation
// ---------------------------------------------------------------------------

fn selected_pages(pages: &[u32], total: u32) -> PdfResult<Vec<u32>> {
    if pages.is_empty() {
        return Ok((1..=total).collect());
    }
    let mut out: Vec<u32> = Vec::with_capacity(pages.len());
    for page in pages {
        if *page == 0 || *page > total {
            return Err(PdfError::RangeOutOfBounds);
        }
        if !out.contains(page) {
            out.push(*page);
        }
    }
    Ok(out)
}

fn header_footer_active(options: &HeaderFooterOptions) -> bool {
    [
        &options.header_left,
        &options.header_center,
        &options.header_right,
        &options.footer_left,
        &options.footer_center,
        &options.footer_right,
    ]
    .iter()
    .any(|template| !template.is_empty())
}

fn substitute_template(template: &str, page: u64, pages: u64) -> String {
    template.replace("{page}", &page.to_string()).replace("{pages}", &pages.to_string())
}

/// Draws the six templates of one page. `position_in_selection` is 1-based.
fn header_footer_content(
    out: &mut String,
    canvas: &PageCanvas,
    options: &HeaderFooterOptions,
    page_number: u32,
    total_pages: u32,
    position_in_selection: u32,
    selected_total: u32,
) {
    // `{page}` is the visible number: the selection position when counting
    // from the start, otherwise the document page number, plus the offset.
    let base = if options.count_from_start { u64::from(position_in_selection) } else { u64::from(page_number) };
    let visible = base + u64::from(options.start_number.saturating_sub(1));
    let pages = if options.count_from_start { u64::from(selected_total) } else { u64::from(total_pages) };
    let color = crate::watermark::parse_hex_color(&options.color);
    let slots = [
        (&options.header_left, Slot::TopLeft),
        (&options.header_center, Slot::TopCenter),
        (&options.header_right, Slot::TopRight),
        (&options.footer_left, Slot::BottomLeft),
        (&options.footer_center, Slot::BottomCenter),
        (&options.footer_right, Slot::BottomRight),
    ];
    for (template, slot) in slots {
        let text = substitute_template(template, visible, pages);
        draw_text(out, canvas, slot, &text, options.font_size_pt, color, options.margin_pt);
    }
}

/// `{prefix}{number}{suffix}` where number = start + position in selection,
/// zero-padded to `digits`. `position_in_selection` is 0-based.
fn bates_text(options: &BatesOptions, position_in_selection: u32) -> String {
    let value = u64::from(options.start) + u64::from(position_in_selection);
    // Clamp the padding so a hostile `digits` cannot allocate gigabytes.
    let width = options.digits.min(64) as usize;
    let number = format!("{value:0width$}");
    format!("{}{}{}", options.prefix, number, options.suffix)
}

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

/// Draws the stamps into an in-memory document and appends the changed objects
/// as a new revision.
fn apply_stamps(
    doc: &mut Document,
    header_footer: Option<&HeaderFooterOptions>,
    bates: Option<&BatesOptions>,
    progress: &ProgressCallback,
    cancel: &CancelToken,
) -> PdfResult<()> {
    let header_footer = match header_footer {
        Some(options) if header_footer_active(options) => Some(options),
        _ => None,
    };
    if header_footer.is_none() && bates.is_none() {
        return Err(PdfError::InvalidInput(
            "nothing to stamp: no header/footer template and no Bates numbering".into(),
        ));
    }
    let total = doc.get_pages().len() as u32;
    if total == 0 {
        return Err(PdfError::InvalidPdf("document has no pages".into()));
    }
    materialize_all_pages(doc)?;

    let header_pages = match header_footer {
        Some(options) => selected_pages(&options.pages, total)?,
        None => Vec::new(),
    };
    let mut header_positions: HashMap<u32, u32> = HashMap::new();
    for (index, page) in header_pages.iter().enumerate() {
        header_positions.insert(*page, index as u32 + 1);
    }
    let bates_pages = match bates {
        Some(options) => selected_pages(&options.pages, total)?,
        None => Vec::new(),
    };
    let mut bates_positions: HashMap<u32, u32> = HashMap::new();
    for (index, page) in bates_pages.iter().enumerate() {
        bates_positions.insert(*page, index as u32);
    }

    let mut targets: Vec<u32> = header_pages.iter().chain(bates_pages.iter()).copied().collect();
    targets.sort_unstable();
    targets.dedup();

    for (index, page_number) in targets.iter().enumerate() {
        cancel.check()?;
        progress(ProgressEvent::new("stamp.page", index as u64, targets.len() as u64));
        let page_id = doc.get_pages().get(page_number).copied().ok_or(PdfError::RangeOutOfBounds)?;
        let canvas = PageCanvas::new(page_rotation(doc, page_id)?, page_mediabox(doc, page_id)?);
        let mut content = String::new();
        if let Some(options) = header_footer {
            if let Some(position) = header_positions.get(page_number) {
                header_footer_content(
                    &mut content,
                    &canvas,
                    options,
                    *page_number,
                    total,
                    *position,
                    header_pages.len() as u32,
                );
            }
        }
        if let Some(options) = bates {
            if let Some(position) = bates_positions.get(page_number) {
                let color = crate::watermark::parse_hex_color(&options.color);
                let text = bates_text(options, *position);
                draw_text(
                    &mut content,
                    &canvas,
                    Slot::from_position(&options.position),
                    &text,
                    options.font_size_pt,
                    color,
                    options.margin_pt,
                );
            }
        }
        if !content.is_empty() {
            ensure_helvetica_font(doc, page_id)?;
            append_page_content(doc, page_id, content.into_bytes())?;
        }
    }
    Ok(())
}

/// Adds the stamps as an appended revision instead of rewriting the file.
///
/// Every byte of `input` is preserved, so a document that is already signed
/// keeps every signature valid. At least one non-empty option is required.
pub fn stamp_pdf_incremental(
    input: &[u8],
    header_footer: Option<&HeaderFooterOptions>,
    bates: Option<&BatesOptions>,
) -> PdfResult<Vec<u8>> {
    let mut doc = Document::load_mem(input).map_err(|error| PdfError::from_lopdf(error, None))?;
    if doc.is_encrypted() || doc.was_encrypted() {
        return Err(PdfError::PasswordRequired);
    }
    let silent = |_event: ProgressEvent| {};
    apply_stamps(&mut doc, header_footer, bates, &silent, &CancelToken::new())?;
    crate::incremental::apply_difference(input, &doc)
}

/// Stamps a file on disk, rewriting it (headers/footers and/or Bates numbers).
pub fn stamp_pdf(
    input: &Path,
    output: &Path,
    header_footer: Option<&HeaderFooterOptions>,
    bates: Option<&BatesOptions>,
    policy: OverwritePolicy,
    password: Option<&str>,
    progress: &ProgressCallback,
    cancel: &CancelToken,
) -> PdfResult<PathBuf> {
    let mut doc = load_document(input, password)?;
    apply_stamps(&mut doc, header_footer, bates, progress, cancel)?;
    let final_path = resolve_output_path(output, policy)?;
    save_document(&mut doc, &final_path, true)?;
    Ok(final_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bates_numbers_pad_and_prefix() {
        let options =
            BatesOptions { prefix: "ABC-".into(), suffix: "-X".into(), start: 7, digits: 4, ..Default::default() };
        assert_eq!(bates_text(&options, 0), "ABC-0007-X");
        assert_eq!(bates_text(&options, 2), "ABC-0009-X");
        let no_pad = BatesOptions { digits: 0, start: 42, ..Default::default() };
        assert_eq!(bates_text(&no_pad, 0), "42");
    }

    #[test]
    fn templates_substitute_page_and_pages() {
        assert_eq!(substitute_template("Page {page} of {pages}", 3, 12), "Page 3 of 12");
        assert_eq!(substitute_template("{page}/{page}", 1, 2), "1/1");
    }

    #[test]
    fn bates_positions_accept_camel_and_snake_case() {
        assert_eq!(Slot::from_position("topLeft"), Slot::TopLeft);
        assert_eq!(Slot::from_position("bottom_center"), Slot::BottomCenter);
        assert_eq!(Slot::from_position("nonsense"), Slot::BottomRight);
    }

    #[test]
    fn helvetica_widths_match_the_numbering_table() {
        assert!((helvetica_text_width("12345", 12.0) - 12.0 * 5.0 * 0.556).abs() < 0.01);
    }
}
