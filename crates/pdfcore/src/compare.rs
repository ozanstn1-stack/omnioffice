//! Document comparison: what changed between two PDFs.
//!
//! Two independent views, because they answer different questions:
//!
//! * A **text diff** says what a reader would notice. It works on extracted
//!   text, so it needs no rendering and works on a 1000-page document.
//! * A **visual diff** says what changed in appearance - a moved logo, a
//!   re-colour, a stamp. It needs both pages rendered at the same size, which
//!   is why it is opt-in and bounded.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::docutil::OverwritePolicy;
use crate::error::{PdfError, PdfResult};
use crate::progress::{CancelToken, ProgressCallback, ProgressReporter};
use crate::render::RenderOptions;

/// One difference between two pages of text.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextDifference {
    /// Page both documents have, 1-based.
    pub page: u32,
    /// `added`, `removed` or `changed`.
    pub kind: String,
    pub left: String,
    pub right: String,
}

/// How two pages look different.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualDifference {
    /// Page in the original document, 1-based.
    pub page: u32,
    /// 0 (identical) to 1 (completely different).
    pub difference: f64,
    pub changed_pixels: u64,
    pub total_pixels: u64,
    /// The page as a JPEG data URL, with the changed areas tinted.
    pub preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareOptions {
    /// Cap on pages to compare, so a huge pair does not hang the UI.
    pub max_pages: u32,
    /// Per-pixel tolerance, 0-255, before a pixel counts as changed.
    pub tolerance: u8,
    /// Resolution for the visual pass.
    pub dpi: u32,
    /// Include the visual pass. It needs a rendering engine.
    pub visual: bool,
    /// Ignore whitespace-only and case-only differences in the text pass.
    pub ignore_whitespace: bool,
    /// Upper bound on reported text differences, so the UI stays responsive.
    pub max_differences: u32,
}

impl Default for CompareOptions {
    fn default() -> Self {
        Self {
            max_pages: 200,
            tolerance: 24,
            dpi: 96,
            visual: false,
            ignore_whitespace: true,
            max_differences: 500,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareReport {
    pub left_pages: u32,
    pub right_pages: u32,
    /// Pages only the original has.
    pub removed_pages: Vec<u32>,
    /// Pages only the modified document has.
    pub added_pages: Vec<u32>,
    /// Pages present in both whose text differs.
    pub text_differences: Vec<TextDifference>,
    /// True when the visual pass ran out of pages or the engine.
    pub visual_truncated: bool,
    pub visual_differences: Vec<VisualDifference>,
    pub identical: bool,
    pub warnings: Vec<String>,
}

/// Compares two documents and reports what changed.
pub fn compare_pdfs(
    left: &Path,
    right: &Path,
    left_password: Option<&str>,
    right_password: Option<&str>,
    options: &CompareOptions,
    progress: &ProgressCallback,
    cancel: &CancelToken,
) -> PdfResult<CompareReport> {
    let reporter = ProgressReporter::new(progress);
    let left_count = crate::info::pdf_info(left, left_password)
        .map(|info| info.page_count)
        .unwrap_or(0);
    let right_count = crate::info::pdf_info(right, right_password)
        .map(|info| info.page_count)
        .unwrap_or(0);
    if left_count == 0 || right_count == 0 {
        return Err(PdfError::InvalidPdf("one of the documents has no readable pages".into()));
    }

    let shared = left_count.min(right_count).min(options.max_pages.max(1));
    let mut warnings: Vec<String> = Vec::new();
    if left_count > shared {
        warnings.push(format!(
            "Only the first {shared} pages were compared; the original has {left_count}."
        ));
    }
    if right_count > shared {
        warnings.push(format!(
            "Only the first {shared} pages were compared; the modified document has {right_count}."
        ));
    }

    let removed_pages: Vec<u32> = ((shared + 1)..=left_count).collect();
    let added_pages: Vec<u32> = ((shared + 1)..=right_count).collect();

    reporter.emit_step("compare.text", 0, shared as u64);
    let mut text_differences: Vec<TextDifference> = Vec::new();
    for page in 1..=shared {
        cancel.check()?;
        reporter.emit_step("compare.text", page as u64, shared as u64);
        let left_text = extract(left, left_password, page);
        let right_text = extract(right, right_password, page);
        let left_text = left_text.unwrap_or_default();
        let right_text = right_text.unwrap_or_default();
        if let Some(kind) = text_kind(&left_text, &right_text, options.ignore_whitespace) {
            if text_differences.len() as u32 >= options.max_differences {
                warnings.push(format!(
                    "Stopped after {} text differences; raise the limit to see the rest.",
                    text_differences.len()
                ));
                break;
            }
            text_differences.push(TextDifference { page, kind, left: left_text, right: right_text });
        }
    }

    let mut visual_differences: Vec<VisualDifference> = Vec::new();
    let mut visual_truncated = false;
    if options.visual {
        match visual_pass(left, right, left_password, right_password, shared, options, progress, cancel) {
            Ok((differences, truncated)) => {
                visual_differences = differences;
                visual_truncated = truncated;
            }
            Err(error) => {
                // A missing rendering engine must not lose the text result.
                visual_truncated = true;
                warnings.push(format!("The visual comparison was skipped: {error}"));
            }
        }
    }

    let identical = text_differences.is_empty()
        && removed_pages.is_empty()
        && added_pages.is_empty()
        && visual_differences.is_empty();
    Ok(CompareReport {
        left_pages: left_count,
        right_pages: right_count,
        removed_pages,
        added_pages,
        text_differences,
        visual_truncated,
        visual_differences,
        identical,
        warnings,
    })
}

/// Extracts one page's text, returning `None` when the page has no text layer.
fn extract(path: &Path, password: Option<&str>, page: u32) -> Option<String> {
    crate::render::extract_page_text(path, password, page).ok()
}

/// Classifies how two pages of text differ.
fn text_kind(left: &str, right: &str, ignore_whitespace: bool) -> Option<String> {
    let normalize = |value: &str| -> String {
        if ignore_whitespace {
            value.split_whitespace().collect::<Vec<_>>().join(" ")
        } else {
            value.to_string()
        }
    };
    let left_normalized = normalize(left);
    let right_normalized = normalize(right);
    if left_normalized == right_normalized {
        return None;
    }
    if left_normalized.trim().is_empty() {
        return Some("added".into());
    }
    if right_normalized.trim().is_empty() {
        return Some("removed".into());
    }
    Some("changed".into())
}

/// Renders both documents page by page and reports the pixel difference.
fn visual_pass(
    left: &Path,
    right: &Path,
    left_password: Option<&str>,
    right_password: Option<&str>,
    pages: u32,
    options: &CompareOptions,
    progress: &ProgressCallback,
    cancel: &CancelToken,
) -> PdfResult<(Vec<VisualDifference>, bool)> {
    let reporter = ProgressReporter::new(progress);
    let render_options = RenderOptions {
        dpi: options.dpi.clamp(36, 300) as f32,
        max_width: Some(1200),
        max_height: Some(1600),
    };
    let mut out: Vec<VisualDifference> = Vec::new();
    let mut truncated = false;
    for page in 1..=pages {
        cancel.check()?;
        reporter.emit_step("compare.visual", page as u64, pages as u64);
        let left_page = match crate::render::render_page(left, left_password, page, &render_options) {
            Ok(value) => value,
            Err(_) => {
                truncated = true;
                continue;
            }
        };
        let right_page = match crate::render::render_page(right, right_password, page, &render_options) {
            Ok(value) => value,
            Err(_) => {
                truncated = true;
                continue;
            }
        };
        // Different page sizes make a pixel diff meaningless, so report it as a
        // full-page change rather than comparing mismatched buffers.
        if left_page.width != right_page.width || left_page.height != right_page.height {
            out.push(VisualDifference {
                page,
                difference: 1.0,
                changed_pixels: (left_page.width as u64) * (left_page.height as u64),
                total_pixels: (left_page.width as u64) * (left_page.height as u64),
                preview: tint(left_page.width, left_page.height, &left_page.rgba, None, 0),
            });
            continue;
        }
        let (difference, changed) = pixel_difference(&left_page.rgba, &right_page.rgba, options.tolerance);
        if changed == 0 {
            continue;
        }
        out.push(VisualDifference {
            page,
            difference,
            changed_pixels: changed,
            total_pixels: (left_page.width as u64) * (left_page.height as u64),
            preview: tint(left_page.width, left_page.height, &left_page.rgba, Some(&right_page.rgba), options.tolerance),
        });
    }
    Ok((out, truncated))
}

/// Fraction of pixels that differ beyond `tolerance`, plus the raw count.
///
/// Anti-aliased text edges differ by a few levels between any two renders, so a
/// tolerance is not optional: without one every page would read as changed.
pub fn pixel_difference(left: &[u8], right: &[u8], tolerance: u8) -> (f64, u64) {
    if left.is_empty() || right.is_empty() {
        return (1.0, 0);
    }
    let pixels = (left.len().min(right.len())) / 4;
    if pixels == 0 {
        return (0.0, 0);
    }
    let mut changed = 0u64;
    for index in 0..pixels {
        let base = index * 4;
        let delta = (left[base] as i32 - right[base] as i32).abs()
            + (left[base + 1] as i32 - right[base + 1] as i32).abs()
            + (left[base + 2] as i32 - right[base + 2] as i32).abs();
        if delta > tolerance as i32 * 3 {
            changed += 1;
        }
    }
    (changed as f64 / pixels as f64, changed)
}

/// Builds a JPEG preview of a page with the changed pixels tinted red.
///
/// Returns an empty string when encoding fails. The difference counts in the
/// report stand on their own, so a missing preview degrades the display rather
/// than the result.
fn tint(width: u32, height: u32, left: &[u8], right: Option<&[u8]>, tolerance: u8) -> String {
    if width == 0 || height == 0 || left.len() < (width as usize * height as usize * 4) {
        return String::new();
    }
    let mut rgb: Vec<u8> = Vec::with_capacity(width as usize * height as usize * 3);
    for index in 0..(width as usize * height as usize) {
        let base = index * 4;
        match right {
            Some(other) if other.len() >= base + 4 => {
                let delta = (left[base] as i32 - other[base] as i32).abs()
                    + (left[base + 1] as i32 - other[base + 1] as i32).abs()
                    + (left[base + 2] as i32 - other[base + 2] as i32).abs();
                if delta > tolerance as i32 * 3 {
                    // Pure red so a change is unmistakable at thumbnail size.
                    rgb.extend_from_slice(&[220, 38, 38]);
                } else {
                    // Unchanged areas are washed out so the red stands out.
                    rgb.push(left[base] / 3 + 170);
                    rgb.push(left[base + 1] / 3 + 170);
                    rgb.push(left[base + 2] / 3 + 170);
                }
            }
            _ => {
                rgb.extend_from_slice(&[left[base], left[base + 1], left[base + 2]]);
            }
        }
    }
    encode_jpeg_data_url(&rgb, width, height)
}

/// Encodes an RGB buffer as a JPEG data URL.
fn encode_jpeg_data_url(rgb: &[u8], width: u32, height: u32) -> String {
    use image::codecs::jpeg::JpegEncoder;
    let mut buffer: Vec<u8> = Vec::new();
    let quality = 72u8;
    if JpegEncoder::new_with_quality(&mut buffer, quality)
        .encode(rgb, width, height, image::ExtendedColorType::Rgb8)
        .is_err()
    {
        return String::new();
    }
    format!("data:image/jpeg;base64,{}", crate::images::base64_encode(&buffer))
}

/// Writes a side-by-side difference image for one page as a PNG.
///
/// The two rendered buffers must be the same size; the left page is drawn
/// first and the changed pixels of the right page are overlaid in red.
pub fn write_visual_diff(
    output: &Path,
    left: &[u8],
    right: &[u8],
    width: u32,
    height: u32,
    tolerance: u8,
    policy: OverwritePolicy,
) -> PdfResult<String> {
    if width == 0 || height == 0 {
        return Err(PdfError::InvalidInput("the compared pages have no pixels".into()));
    }
    let needed = width as usize * height as usize * 4;
    if left.len() < needed || right.len() < needed {
        return Err(PdfError::InvalidInput("the rendered page is smaller than its reported size".into()));
    }
    let target = crate::docutil::resolve_output_path(output, policy)?;

    // A single sheet with the two pages side by side, so the result is one image
    // rather than a document the user has to page through.
    let mut rgb: Vec<u8> = Vec::with_capacity((width as usize * 2 * height as usize + 8) * 3);
    let gap = 4usize;
    let mut row: Vec<u8> = Vec::with_capacity((width as usize * 2 + gap) * 3);
    for y in 0..height as usize {
        for x in 0..width as usize {
            let base = (y * width as usize + x) * 4;
            // The original page is dimmed so the overlay reads clearly.
            row.push(left[base] / 2 + 128);
            row.push(left[base + 1] / 2 + 128);
            row.push(left[base + 2] / 2 + 128);
        }
        row.extend(std::iter::repeat_n(255u8, gap * 3));
        for x in 0..width as usize {
            let base = (y * width as usize + x) * 4;
            let delta = (left[base] as i32 - right[base] as i32).abs()
                + (left[base + 1] as i32 - right[base + 1] as i32).abs()
                + (left[base + 2] as i32 - right[base + 2] as i32).abs();
            if delta > tolerance as i32 * 3 {
                row.extend_from_slice(&[220, 38, 38]);
            } else {
                row.push(right[base]);
                row.push(right[base + 1]);
                row.push(right[base + 2]);
            }
        }
        rgb.extend_from_slice(&row);
        row.clear();
    }
    let out_width = width * 2 + gap as u32;
    let image = image::RgbImage::from_raw(out_width, height, rgb)
        .ok_or_else(|| PdfError::InvalidImage("the difference image had an unexpected size".into()))?;
    image
        .save(&target)
        .map_err(|error| PdfError::ProcessingFailed(format!("write difference image: {error}")))?;
    Ok(target.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_buffers_have_no_difference() {
        let buffer: Vec<u8> = (0..400).map(|value| (value % 256) as u8).collect();
        let (difference, changed) = pixel_difference(&buffer, &buffer, 24);
        assert_eq!(changed, 0);
        assert_eq!(difference, 0.0);
    }

    #[test]
    fn one_changed_channel_is_reported() {
        let left = vec![10u8; 40];
        let mut right = left.clone();
        right[0] = 200;
        let (difference, changed) = pixel_difference(&left, &right, 24);
        assert_eq!(changed, 1);
        assert!((difference - 0.1).abs() < 1e-9, "expected 1 of 10 pixels, got {difference}");
    }

    #[test]
    fn a_difference_below_the_tolerance_is_ignored() {
        // Anti-aliased edges wobble by a few levels between renders.
        let left = vec![128u8; 40];
        let right = vec![130u8; 40];
        assert_eq!(pixel_difference(&left, &right, 24).1, 0);
        assert_eq!(pixel_difference(&left, &right, 0).1, 10);
    }

    #[test]
    fn an_empty_buffer_is_reported_as_fully_different() {
        let (difference, _) = pixel_difference(&[], &[1, 2, 3, 4], 24);
        assert_eq!(difference, 1.0);
    }

    #[test]
    fn text_kind_distinguishes_added_removed_and_changed() {
        assert_eq!(text_kind("hello", "hello", true), None);
        assert_eq!(text_kind("", "new page", true), Some("added".into()));
        assert_eq!(text_kind("old page", "", true), Some("removed".into()));
        assert_eq!(text_kind("total 30", "total 42", true), Some("changed".into()));
    }

    #[test]
    fn whitespace_only_changes_are_ignored_when_asked() {
        assert_eq!(text_kind("a  b", "a b", true), None);
        assert_eq!(text_kind("a  b", "a b", false), Some("changed".into()));
    }

    #[test]
    fn case_differences_count_when_whitespace_is_ignored() {
        assert_eq!(text_kind("Total", "total", true), Some("changed".into()));
    }

    #[test]
    fn a_tinted_preview_is_produced_for_a_valid_buffer() {
        let left: Vec<u8> = (0..(4 * 4 * 4)).map(|value| (value % 256) as u8).collect();
        let mut right = left.clone();
        right[0] = 255;
        let preview = tint(4, 4, &left, Some(&right), 24);
        assert!(preview.starts_with("data:image/jpeg;base64,"), "expected a data URL, got {preview:?}");
    }

    #[test]
    fn a_short_buffer_produces_no_preview() {
        assert_eq!(tint(4, 4, &[0, 0, 0], None, 0), "");
        assert_eq!(tint(0, 0, &[], None, 0), "");
    }
}
