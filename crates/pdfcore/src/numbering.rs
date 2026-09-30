//! Page numbering using the standard Helvetica Type1 font (always available
//! in PDF viewers, no embedding required, crisp vector output).

use crate::docutil::*;
use crate::error::{PdfError, PdfResult};
use crate::progress::{CancelToken, ProgressCallback, ProgressEvent};
use lopdf::{dictionary, Document, Object};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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

pub fn helvetica_text_width(text: &str, size_pt: f64) -> f64 {
    let mut width = 0.0;
    for ch in text.chars() {
        let code = ch as u32;
        let w = if (32..=126).contains(&code) { HELVETICA_WIDTHS[(code - 32) as usize] as f64 } else { 556.0 };
        width += w;
    }
    width / 1000.0 * size_pt
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumberingOptions {
    /// top_left | top_center | top_right | bottom_left | bottom_center | bottom_right
    #[serde(default = "default_position")]
    pub position: String,
    /// "n" | "page_n" | "n_of_total" | "page_n_of_total"
    #[serde(default = "default_format")]
    pub format: String,
    #[serde(default = "default_start")]
    pub start_number: u32,
    #[serde(default = "default_font_size")]
    pub font_size_pt: f64,
    /// "#RRGGBB"
    #[serde(default = "default_color")]
    pub color: String,
    #[serde(default = "default_margin")]
    pub margin_pt: f64,
    /// Empty = all pages.
    #[serde(default)]
    pub pages: Vec<u32>,
    /// When true the first rendered number equals start_number, otherwise the
    /// displayed number matches the physical page index.
    #[serde(default)]
    pub count_from_start: bool,
}

fn default_position() -> String {
    "bottom_center".into()
}
fn default_format() -> String {
    "n".into()
}
fn default_start() -> u32 {
    1
}
fn default_font_size() -> f64 {
    11.0
}
fn default_color() -> String {
    "#333333".into()
}
fn default_margin() -> f64 {
    28.0
}

impl Default for NumberingOptions {
    fn default() -> Self {
        Self {
            position: default_position(),
            format: default_format(),
            start_number: 1,
            font_size_pt: default_font_size(),
            color: default_color(),
            margin_pt: default_margin(),
            pages: Vec::new(),
            count_from_start: true,
        }
    }
}

fn label_for(options: &NumberingOptions, page_number: u32, total: u32) -> String {
    let display_number = if options.count_from_start { options.start_number + page_number - 1 } else { page_number };
    match options.format.as_str() {
        "page_n" => format!("Page {display_number}"),
        "n_of_total" => format!("{display_number} / {total}"),
        "page_n_of_total" => format!("Page {display_number} of {total}"),
        _ => format!("{display_number}"),
    }
}

/// `ensure_helvetica_font` adds a Type1 Helvetica font object to the page.
fn ensure_helvetica_font(doc: &mut Document, page_id: lopdf::ObjectId) -> PdfResult<()> {
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

/// Draws the page numbers into the document.
///
/// Shared by the rewrite path and the incremental one; it only appends content
/// (and, when needed, a font resource), so the difference stays additive.
fn apply_page_numbers(
    doc: &mut Document,
    options: &NumberingOptions,
    progress: &ProgressCallback,
    cancel: &CancelToken,
) -> PdfResult<()> {
    let color = crate::watermark::parse_hex_color(&options.color);
    let total = doc.get_pages().len() as u32;
    if total == 0 {
        return Err(PdfError::InvalidPdf("document has no pages".into()));
    }
    materialize_all_pages(doc)?;
    let target: Vec<u32> = if options.pages.is_empty() {
        (1..=total).collect()
    } else {
        for p in &options.pages {
            if *p == 0 || *p > total {
                return Err(PdfError::RangeOutOfBounds);
            }
        }
        options.pages.clone()
    };

    for (index, page_number) in target.iter().enumerate() {
        cancel.check()?;
        progress(ProgressEvent::new("numbering.page", index as u64, target.len() as u64));
        let page_id = doc.get_pages().get(page_number).copied().ok_or(PdfError::RangeOutOfBounds)?;
        let label = label_for(options, *page_number, total);
        let rotation = page_rotation(doc, page_id)?;
        let media = page_mediabox(doc, page_id)?;
        let (page_w, page_h) = (media[2] - media[0], media[3] - media[1]);
        let (display_w, display_h) = Matrix::displayed_size(rotation, page_w, page_h);
        let to_page = Matrix::display_to_page(rotation, page_w, page_h);

        let text_w = helvetica_text_width(&label, options.font_size_pt);
        let margin = options.margin_pt;
        let baseline_offset = options.font_size_pt * 0.25;
        let (x, y) = match options.position.as_str() {
            "top_left" => (margin, display_h - margin - options.font_size_pt),
            "top_center" => ((display_w - text_w) / 2.0, display_h - margin - options.font_size_pt),
            "top_right" => (display_w - margin - text_w, display_h - margin - options.font_size_pt),
            "bottom_left" => (margin, margin + baseline_offset),
            "bottom_right" => (display_w - margin - text_w, margin + baseline_offset),
            _ => ((display_w - text_w) / 2.0, margin + baseline_offset),
        };

        ensure_helvetica_font(&mut *doc, page_id)?;
        let content = format!(
            "q\n{}\nBT\n{r} {g} {b} rg\n/Helv {size:.2} Tf\n{x:.2} {y:.2} Td\n({text}) Tj\nET\nQ\n",
            to_page.to_cm(),
            r = color[0] as f64 / 255.0,
            g = color[1] as f64 / 255.0,
            b = color[2] as f64 / 255.0,
            size = options.font_size_pt,
            x = x,
            y = y,
            text = escape_pdf_literal(&label),
        );
        append_page_content(&mut *doc, page_id, content.into_bytes())?;
    }

    Ok(())
}

/// Adds the page numbers as an appended revision instead of rewriting the file.
///
/// Every byte of `input` is preserved, so a document that is already signed
/// keeps every signature valid.
pub fn numbering_pdf_incremental(input: &[u8], options: &NumberingOptions) -> PdfResult<Vec<u8>> {
    let mut doc = Document::load_mem(input).map_err(|error| PdfError::from_lopdf(error, None))?;
    if doc.is_encrypted() || doc.was_encrypted() {
        return Err(PdfError::PasswordRequired);
    }
    let silent = |_event: ProgressEvent| {};
    apply_page_numbers(&mut doc, options, &silent, &CancelToken::new())?;
    crate::incremental::apply_difference(input, &doc)
}

pub fn add_page_numbers(
    input: &Path,
    output: &Path,
    options: &NumberingOptions,
    policy: OverwritePolicy,
    password: Option<&str>,
    progress: &ProgressCallback,
    cancel: &CancelToken,
) -> PdfResult<PathBuf> {
    let mut doc = load_document(input, password)?;
    apply_page_numbers(&mut doc, options, progress, cancel)?;
    let final_path = resolve_output_path(output, policy)?;
    save_document(&mut doc, &final_path, true)?;
    Ok(final_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_are_reasonable() {
        let w = helvetica_text_width("12345", 12.0);
        assert!((w - 12.0 * 5.0 * 0.556).abs() < 0.01, "width was {w}");
        assert!(helvetica_text_width("Page 1 / 10", 11.0) > 40.0);
    }

    #[test]
    fn label_formats() {
        let mut options = NumberingOptions::default();
        assert_eq!(label_for(&options, 3, 10), "3");
        options.format = "page_n".into();
        assert_eq!(label_for(&options, 3, 10), "Page 3");
        options.format = "n_of_total".into();
        assert_eq!(label_for(&options, 3, 10), "3 / 10");
        options.start_number = 5;
        assert_eq!(label_for(&options, 1, 10), "5 / 10");
        options.count_from_start = false;
        assert_eq!(label_for(&options, 1, 10), "1 / 10");
    }
}
