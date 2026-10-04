//! Page rasterization and text extraction through pdfium (bundled DLL).
//! pdfium is not thread safe, so every call goes through the global instance
//! which serializes access internally (pdfium-render `thread_safe` bindings).

use crate::engines;
use crate::error::{PdfError, PdfResult};
use pdfium_render::prelude::*;
use std::path::Path;
use std::sync::OnceLock;

static PDFIUM: OnceLock<Result<Pdfium, String>> = OnceLock::new();

fn pdfium() -> PdfResult<&'static Pdfium> {
    let result = PDFIUM.get_or_init(|| {
        let bindings = match engines::pdfium_path() {
            Some(dll) => {
                let dir = dll.parent().unwrap_or_else(|| Path::new("."));
                let name = Pdfium::pdfium_platform_library_name_at_path(dir);
                Pdfium::bind_to_library(name)
            }
            None => Pdfium::bind_to_system_library(),
        };
        let bindings = bindings.map_err(|e| format!("pdfium could not be loaded: {e}"))?;
        Ok(Pdfium::new(bindings))
    });
    match result {
        Ok(pdfium) => Ok(pdfium),
        Err(msg) => Err(PdfError::EngineMissing(msg.clone())),
    }
}

pub fn is_available() -> bool {
    pdfium().is_ok()
}

pub fn ensure_available() -> PdfResult<()> {
    pdfium().map(|_| ())
}

/// The cached pdfium instance, for modules that need more than page rendering.
///
/// Exposed so text geometry, comparison and accessibility checks can load a
/// document once and work through every page, instead of re-parsing the file
/// for each page the way the per-page helpers do.
pub fn pdfium_instance() -> PdfResult<&'static Pdfium> {
    pdfium()
}

#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    pub dpi: f32,
    /// Upper bounds in pixels; the render is scaled down proportionally when
    /// exceeded (protects against giant pages).
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self { dpi: 150.0, max_width: None, max_height: None }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PageGeometry {
    pub page: u32,
    pub width_pt: f64,
    pub height_pt: f64,
    /// Width/height as displayed, i.e. after the page's /Rotate value.
    pub display_width_pt: f64,
    pub display_height_pt: f64,
    pub rotation: i32,
}

/// RGB(A) pixel buffer returned by the renderer.
#[derive(Debug, Clone)]
pub struct RenderedPage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl RenderedPage {
    pub fn to_dynamic_image(&self) -> PdfResult<image::DynamicImage> {
        let buffer = image::RgbaImage::from_raw(self.width, self.height, self.rgba.clone())
            .ok_or_else(|| PdfError::ConversionFailed("invalid render buffer".into()))?;
        Ok(image::DynamicImage::ImageRgba8(buffer))
    }
}

fn render_config(options: &RenderOptions, page_w: f32, page_h: f32) -> PdfRenderConfig {
    let mut target_w = (page_w * options.dpi / 72.0).round().max(1.0);
    let mut target_h = (page_h * options.dpi / 72.0).round().max(1.0);
    if let Some(max_w) = options.max_width {
        if target_w > max_w as f32 {
            let scale = max_w as f32 / target_w;
            target_w = max_w as f32;
            target_h = (target_h * scale).round().max(1.0);
        }
    }
    if let Some(max_h) = options.max_height {
        if target_h > max_h as f32 {
            let scale = max_h as f32 / target_h;
            target_h = max_h as f32;
            target_w = (target_w * scale).round().max(1.0);
        }
    }
    PdfRenderConfig::new().set_target_width(target_w as i32).set_target_height(target_h as i32).render_form_data(true)
}

/// Reads page geometry (sizes in points, rotation) for every page.
pub fn page_geometries(path: &Path, password: Option<&str>) -> PdfResult<Vec<PageGeometry>> {
    let pdfium = pdfium()?;
    let doc = pdfium.load_pdf_from_file(path, password).map_err(|e| PdfError::InvalidPdf(format!("{e}")))?;
    let mut out = Vec::with_capacity(doc.pages().len() as usize);
    for (index, page) in doc.pages().iter().enumerate() {
        let (w, h) = (page.width().value as f64, page.height().value as f64);
        // pdfium reports dimensions after applying /Rotate; recover the raw
        // MediaBox orientation for completeness.
        let rotation = match page.rotation() {
            Ok(PdfPageRenderRotation::None) => 0,
            Ok(PdfPageRenderRotation::Degrees90) => 90,
            Ok(PdfPageRenderRotation::Degrees180) => 180,
            Ok(PdfPageRenderRotation::Degrees270) => 270,
            _ => 0,
        };
        out.push(PageGeometry {
            page: index as u32 + 1,
            width_pt: w,
            height_pt: h,
            display_width_pt: w,
            display_height_pt: h,
            rotation,
        });
    }
    Ok(out)
}

/// Renders a single page (1-based index) as RGBA.
pub fn render_page(
    path: &Path,
    password: Option<&str>,
    page_number: u32,
    options: &RenderOptions,
) -> PdfResult<RenderedPage> {
    let pdfium = pdfium()?;
    let doc = pdfium.load_pdf_from_file(path, password).map_err(|e| PdfError::InvalidPdf(format!("{e}")))?;
    let index = page_number.checked_sub(1).ok_or(PdfError::RangeOutOfBounds)? as PdfPageIndex;
    if index as usize >= doc.pages().len() as usize {
        return Err(PdfError::RangeOutOfBounds);
    }
    let page = doc.pages().get(index).map_err(|e| PdfError::ProcessingFailed(format!("{e}")))?;
    let config = render_config(options, page.width().value, page.height().value);
    let bitmap =
        page.render_with_config(&config).map_err(|e| PdfError::ConversionFailed(format!("render failed: {e}")))?;
    let width = bitmap.width() as u32;
    let height = bitmap.height() as u32;
    let image = bitmap.as_image().map_err(|e| PdfError::ConversionFailed(format!("bitmap conversion failed: {e}")))?;
    Ok(RenderedPage { width, height, rgba: image.to_rgba8().into_raw() })
}

/// Renders a page straight to PNG or JPEG bytes.
pub fn render_page_bytes(
    path: &Path,
    password: Option<&str>,
    page_number: u32,
    options: &RenderOptions,
    format: crate::images::ImageFormat,
    jpeg_quality: u8,
    grayscale: bool,
) -> PdfResult<Vec<u8>> {
    let rendered = render_page(path, password, page_number, options)?;
    crate::images::encode_image(&rendered, format, jpeg_quality, grayscale)
}

/// Extracts the text layer of a page via pdfium (empty when the page is a
/// scanned image without OCR).
pub fn extract_page_text(path: &Path, password: Option<&str>, page_number: u32) -> PdfResult<String> {
    let pdfium = pdfium()?;
    let doc = pdfium.load_pdf_from_file(path, password).map_err(|e| PdfError::InvalidPdf(format!("{e}")))?;
    let index = page_number.checked_sub(1).ok_or(PdfError::RangeOutOfBounds)? as PdfPageIndex;
    let page = doc.pages().get(index).map_err(|_| PdfError::RangeOutOfBounds)?;
    let text = page.text().map_err(|e| PdfError::ProcessingFailed(format!("{e}")))?.all();
    Ok(text)
}

/// Heuristic: does the document already contain a usable text layer?
/// Samples up to `sample` pages spread through the document.
pub fn document_has_text(path: &Path, password: Option<&str>, sample: u32) -> PdfResult<bool> {
    let pdfium = pdfium()?;
    let doc = pdfium.load_pdf_from_file(path, password).map_err(|e| PdfError::InvalidPdf(format!("{e}")))?;
    let total = doc.pages().len().max(0) as u32;
    if total == 0 {
        return Ok(false);
    }
    let take = sample.clamp(1, total);
    let step = (total / take).max(1);
    let mut checked = 0;
    let mut index = 0u32;
    while checked < take && index < total {
        if let Ok(page) = doc.pages().get(index as PdfPageIndex) {
            if let Ok(text) = page.text() {
                let content = text.all();
                if content.trim().chars().count() > 3 {
                    return Ok(true);
                }
            }
        }
        checked += 1;
        index += step;
    }
    Ok(false)
}

// ---------------------------------------------------------------------------
// Text search (reading mode)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
pub struct TextMatch {
    pub page: u32,
    /// Short excerpt around the match (whitespace collapsed).
    pub snippet: String,
    /// 1-based index of the match within its page.
    pub index_on_page: u32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchResult {
    pub matches: Vec<TextMatch>,
    pub pages_with_matches: u32,
    pub total_matches: u32,
    /// True when the scan stopped early because `max_results` was reached.
    pub truncated: bool,
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Builds a snippet around a match position in the collapsed page text.
fn snippet_for(text: &str, query_len: usize, position: usize) -> String {
    let context = 48usize;
    let start = position.saturating_sub(context);
    let end = (position + query_len + context).min(text.len());
    if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
        return text.chars().take(96).collect();
    }
    let mut slice = String::new();
    if start > 0 {
        slice.push('…');
    }
    slice.push_str(&text[start..end]);
    if end < text.len() {
        slice.push('…');
    }
    slice
}

/// Searches the document's text layer with pdfium's search engine.
/// `max_results` caps the work for very large documents.
pub fn search_document(
    path: &Path,
    password: Option<&str>,
    query: &str,
    match_case: bool,
    max_results: u32,
    cancel: &crate::progress::CancelToken,
    on_page: &dyn Fn(u32, u32),
) -> PdfResult<SearchResult> {
    let query = query.trim();
    if query.is_empty() {
        return Err(PdfError::InvalidInput("search text is empty".into()));
    }
    let pdfium = pdfium()?;
    let doc = pdfium.load_pdf_from_file(path, password).map_err(|e| PdfError::InvalidPdf(format!("{e}")))?;
    let total = doc.pages().len().max(0) as u32;
    let mut matches: Vec<TextMatch> = Vec::new();
    let mut pages_with_matches = 0u32;
    let mut truncated = false;
    let options = PdfSearchOptions::new().match_case(match_case);
    let limit = max_results.max(1);

    for index in 0..total {
        cancel.check()?;
        on_page(index + 1, total);
        let Ok(page) = doc.pages().get(index as PdfPageIndex) else {
            continue;
        };
        let Ok(text) = page.text() else { continue };
        // Use pdfium's matcher for correctness, then build snippets from the
        // extracted text so the UI can show context.
        let found = match text.search(query, &options) {
            Ok(search) => {
                let mut count = 0u32;
                let cursor = search;
                while let Some(segments) = cursor.find_next() {
                    if !segments.is_empty() {
                        count += 1;
                    }
                    if count >= 64 {
                        break;
                    }
                }
                count
            }
            Err(_) => 0,
        };
        if found == 0 {
            continue;
        }
        let collapsed = collapse_whitespace(&text.all());
        let (needle, hay) = if match_case {
            (query.to_string(), collapsed.clone())
        } else {
            (query.to_lowercase(), collapsed.to_lowercase())
        };
        let mut page_matches = 0u32;
        let mut from = 0usize;
        while let Some(position) = hay[from..].find(&needle) {
            let absolute = from + position;
            matches.push(TextMatch {
                page: index + 1,
                snippet: snippet_for(&collapsed, needle.len(), absolute),
                index_on_page: page_matches + 1,
            });
            page_matches += 1;
            from = absolute + needle.len().max(1);
            if matches.len() as u32 >= limit {
                truncated = true;
                break;
            }
        }
        if page_matches == 0 {
            // pdfium matched but the collapsed-text scan did not (for example
            // when a match spans a line break): still report the page.
            matches.push(TextMatch { page: index + 1, snippet: String::new(), index_on_page: 1 });
        }
        pages_with_matches += 1;
        if truncated {
            break;
        }
    }
    if truncated {
        matches.truncate(limit as usize);
    }
    Ok(SearchResult { total_matches: matches.len() as u32, matches, pages_with_matches, truncated })
}
