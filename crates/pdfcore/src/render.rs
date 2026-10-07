//! Page rasterization and text extraction through pdfium (bundled DLL).
//! pdfium is not thread safe, so every call goes through the global instance
//! which serializes access internally (pdfium-render `thread_safe` bindings).

use crate::engines;
use crate::error::{PdfError, PdfResult};
use pdfium_render::prelude::*;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

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

// ---------------------------------------------------------------------------
// Region rendering (reader tiles at high zoom)
// ---------------------------------------------------------------------------

/// A rectangle of a page raster, in output pixels from the top-left corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Largest region edge [`render_page_region`] accepts, in pixels.
pub const MAX_REGION_EDGE: u32 = 4096;

/// Largest region scale in output pixels per PDF point (7200 dpi).
pub const MAX_REGION_SCALE: f32 = 100.0;

/// How many documents the region renderer keeps open. Two covers the reader
/// plus one document it was switched away from.
const DOCUMENT_CACHE_CAPACITY: usize = 2;

/// Identifies one version of a file on disk; a save (an atomic rename over
/// the path) changes at least one of these.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FileStamp {
    len: u64,
    modified: Option<SystemTime>,
    #[cfg(unix)]
    inode: (u64, u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DocumentKey {
    path: PathBuf,
    file: FileStamp,
    /// SHA-256 of the password, so the plain text does not outlive the call.
    password: Option<[u8; 32]>,
}

impl DocumentKey {
    fn new(path: &Path, password: Option<&str>) -> PdfResult<Self> {
        let meta = std::fs::metadata(path).map_err(PdfError::from_io)?;
        #[cfg(unix)]
        let inode = {
            use std::os::unix::fs::MetadataExt;
            (meta.dev(), meta.ino())
        };
        Ok(Self {
            path: path.to_path_buf(),
            file: FileStamp {
                len: meta.len(),
                modified: meta.modified().ok(),
                #[cfg(unix)]
                inode,
            },
            password: password.map(|value| Sha256::digest(value.as_bytes()).into()),
        })
    }
}

struct CachedDocument {
    key: DocumentKey,
    document: PdfDocument<'static>,
}

/// Open documents for region rendering, most recently used first. A reader
/// at high zoom asks for a dozen tiles per screen, and re-parsing the file for
/// every one of them would dominate the render time.
///
/// The lock is held for the whole render, so a cached document is never used
/// from two threads at once; pdfium serializes every call through the
/// `thread_safe` bindings anyway, so this costs no parallelism. Documents are
/// opened through `std::fs::File`, which on Windows shares delete and write
/// access, so a cached document never blocks a save over the same path.
static DOCUMENT_CACHE: Mutex<Vec<CachedDocument>> = Mutex::new(Vec::new());

fn with_cached_document<T>(
    path: &Path,
    password: Option<&str>,
    work: impl FnOnce(&PdfDocument<'static>) -> PdfResult<T>,
) -> PdfResult<T> {
    let key = DocumentKey::new(path, password)?;
    let mut cache = DOCUMENT_CACHE.lock().unwrap_or_else(|poisoned| {
        // A panic mid-render may have left a document in an unknown state.
        DOCUMENT_CACHE.clear_poison();
        let mut cache = poisoned.into_inner();
        cache.clear();
        cache
    });
    let entry = match cache.iter().position(|entry| entry.key == key) {
        Some(index) => cache.remove(index),
        None => {
            // An older version of the same file is never needed again.
            cache.retain(|entry| entry.key.path != key.path || entry.key.file == key.file);
            let document =
                pdfium()?.load_pdf_from_file(path, password).map_err(|e| PdfError::InvalidPdf(format!("{e}")))?;
            CachedDocument { key, document }
        }
    };
    let result = work(&entry.document);
    cache.insert(0, entry);
    cache.truncate(DOCUMENT_CACHE_CAPACITY);
    result
}

/// Renders `region` of a page (1-based index) as if the whole page were
/// rasterized at `scale` output pixels per PDF point, without rendering the
/// rest of the page. The region is clipped to the page; the returned image has
/// the clipped size.
///
/// This uses pdfium's regular page render (`FPDF_RenderPageBitmap` plus the
/// form overlay) with the page origin shifted into a region-sized bitmap, so
/// the pixels match a full-page [`render_page`] at the same scale. The matrix
/// render path would clip as well, but it cannot draw form field data.
pub fn render_page_region(
    path: &Path,
    password: Option<&str>,
    page_number: u32,
    scale: f32,
    region: PixelRegion,
) -> PdfResult<RenderedPage> {
    if !scale.is_finite() || scale <= 0.0 || scale > MAX_REGION_SCALE {
        return Err(PdfError::InvalidInput(format!("render scale out of range: {scale}")));
    }
    if region.width == 0 || region.height == 0 || region.width > MAX_REGION_EDGE || region.height > MAX_REGION_EDGE {
        return Err(PdfError::InvalidInput(format!(
            "render region must be 1-{MAX_REGION_EDGE} px per side, got {}x{}",
            region.width, region.height
        )));
    }
    let index = page_number.checked_sub(1).ok_or(PdfError::RangeOutOfBounds)? as PdfPageIndex;
    with_cached_document(path, password, |doc| {
        if index as usize >= doc.pages().len() as usize {
            return Err(PdfError::RangeOutOfBounds);
        }
        let page = doc.pages().get(index).map_err(|e| PdfError::ProcessingFailed(format!("{e}")))?;
        let full_w = (page.width().value * scale).round().max(1.0);
        let full_h = (page.height().value * scale).round().max(1.0);
        if full_w > i32::MAX as f32 || full_h > i32::MAX as f32 {
            return Err(PdfError::InvalidInput("page is too large for this render scale".into()));
        }
        let (full_w, full_h) = (full_w as u32, full_h as u32);
        if region.x >= full_w || region.y >= full_h {
            return Err(PdfError::InvalidInput("render region lies outside the page".into()));
        }
        let width = region.width.min(full_w - region.x);
        let height = region.height.min(full_h - region.y);
        let config = PdfRenderConfig::new()
            .set_target_width(full_w as i32)
            .set_target_height(full_h as i32)
            .render_form_data(true)
            .set_origin(-(region.x as i32), -(region.y as i32));
        let mut bitmap = PdfBitmap::empty(width as i32, height as i32, PdfBitmapFormat::default())
            .map_err(|e| PdfError::ConversionFailed(format!("bitmap allocation failed: {e}")))?;
        page.render_into_bitmap_with_config(&mut bitmap, &config)
            .map_err(|e| PdfError::ConversionFailed(format!("render failed: {e}")))?;
        let image =
            bitmap.as_image().map_err(|e| PdfError::ConversionFailed(format!("bitmap conversion failed: {e}")))?;
        Ok(RenderedPage { width, height, rgba: image.to_rgba8().into_raw() })
    })
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
