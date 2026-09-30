//! Text and image watermarks with position, opacity, rotation, tiling and
//! rotation-aware page handling.

use crate::docutil::*;
use crate::error::{PdfError, PdfResult};
use crate::progress::{CancelToken, ProgressCallback, ProgressEvent};
use crate::textimg::{self, TextRenderRequest};
use lopdf::{Document, Object};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatermarkOptions {
    /// "text" or "image"
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default)]
    pub text: String,
    #[serde(default = "default_font_size")]
    pub font_size_pt: f64,
    #[serde(default)]
    pub bold: bool,
    /// "#RRGGBB"
    #[serde(default = "default_color")]
    pub color: String,
    /// 0.0 - 1.0
    #[serde(default = "default_opacity")]
    pub opacity: f64,
    #[serde(default = "default_rotation")]
    pub rotation_deg: f64,
    /// top_left | top_center | top_right | center | bottom_left |
    /// bottom_center | bottom_right
    #[serde(default = "default_position")]
    pub position: String,
    #[serde(default = "default_margin")]
    pub margin_pt: f64,
    #[serde(default)]
    pub tile: bool,
    #[serde(default)]
    pub image_path: Option<String>,
    /// Image watermark width as a fraction of the page width (0.05 - 1.0).
    #[serde(default = "default_image_scale")]
    pub image_scale: f64,
    /// Empty = all pages.
    #[serde(default)]
    pub pages: Vec<u32>,
}

fn default_kind() -> String {
    "text".into()
}
fn default_font_size() -> f64 {
    48.0
}
fn default_color() -> String {
    "#9aa0a6".into()
}
fn default_opacity() -> f64 {
    0.25
}
fn default_rotation() -> f64 {
    45.0
}
fn default_position() -> String {
    "center".into()
}
fn default_margin() -> f64 {
    24.0
}
fn default_image_scale() -> f64 {
    0.35
}

impl Default for WatermarkOptions {
    fn default() -> Self {
        Self {
            kind: default_kind(),
            text: String::new(),
            font_size_pt: default_font_size(),
            bold: false,
            color: default_color(),
            opacity: default_opacity(),
            rotation_deg: default_rotation(),
            position: default_position(),
            margin_pt: default_margin(),
            tile: false,
            image_path: None,
            image_scale: default_image_scale(),
            pages: Vec::new(),
        }
    }
}

pub fn parse_hex_color(input: &str) -> [u8; 4] {
    let hex = input.trim().trim_start_matches('#');
    let parse = |s: &str| u8::from_str_radix(s, 16).unwrap_or(0);
    match hex.len() {
        6 => [parse(&hex[0..2]), parse(&hex[2..4]), parse(&hex[4..6]), 255],
        8 => [parse(&hex[0..2]), parse(&hex[2..4]), parse(&hex[4..6]), parse(&hex[6..8])],
        _ => [0, 0, 0, 255],
    }
}

/// Renders the watermark artwork once (as RGBA) plus its natural size in
/// points, then places it on every target page.
struct Artwork {
    image: RawImage,
    /// Size in points at the requested font size / scale.
    width_pt: f64,
    height_pt: f64,
}

fn build_text_artwork(options: &WatermarkOptions) -> PdfResult<Artwork> {
    if options.text.trim().is_empty() {
        return Err(PdfError::InvalidInput("watermark text is empty".into()));
    }
    // Render at 4 px per point for crisp edges, clamped to a sane bitmap size.
    let mut scale = 4.0f64;
    let max_px = 6000.0f64;
    let approx_px = options.font_size_pt * scale * (options.text.chars().count().max(1) as f64);
    if approx_px > max_px {
        scale = (max_px / (options.font_size_pt * options.text.chars().count().max(1) as f64)).max(1.0);
    }
    let color = parse_hex_color(&options.color);
    let alpha = (options.opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    let request = TextRenderRequest {
        text: options.text.clone(),
        size_px: (options.font_size_pt * scale) as f32,
        color: [color[0], color[1], color[2], alpha],
        bold: options.bold,
        line_spacing: 1.15,
        padding_px: (scale * 2.0) as u32,
        wrap_width_px: None,
    };
    let image = textimg::render_text(&request)?;
    let width_pt = image.width as f64 / scale;
    let height_pt = image.height as f64 / scale;
    Ok(Artwork { image, width_pt, height_pt })
}

fn build_image_artwork(options: &WatermarkOptions, page_width_pt: f64) -> PdfResult<Artwork> {
    let path = options
        .image_path
        .as_ref()
        .ok_or_else(|| PdfError::InvalidInput("image watermark needs an image file".into()))?;
    let decoded = crate::images::decode_image(Path::new(path))?;
    let rgba = decoded.to_rgba8();
    let (w, h) = (rgba.width() as f64, rgba.height() as f64);
    let width_pt = (page_width_pt * options.image_scale.clamp(0.02, 1.0)).max(4.0);
    let height_pt = width_pt * h / w;
    let opacity = options.opacity.clamp(0.0, 1.0);
    let mut pixels = rgba.into_raw();
    if opacity < 1.0 {
        for px in pixels.as_chunks_mut::<4>().0 {
            px[3] = (px[3] as f64 * opacity).round() as u8;
        }
    }
    Ok(Artwork { image: RawImage { width: w as u32, height: h as u32, rgba: pixels }, width_pt, height_pt })
}

fn position_origin(position: &str, display_w: f64, display_h: f64, art_w: f64, art_h: f64, margin: f64) -> (f64, f64) {
    let (x, y) = match position {
        "top_left" => (margin, display_h - margin - art_h),
        "top_center" => ((display_w - art_w) / 2.0, display_h - margin - art_h),
        "top_right" => (display_w - margin - art_w, display_h - margin - art_h),
        "bottom_left" => (margin, margin),
        "bottom_center" => ((display_w - art_w) / 2.0, margin),
        "bottom_right" => (display_w - margin - art_w, margin),
        // center (default)
        _ => ((display_w - art_w) / 2.0, (display_h - art_h) / 2.0),
    };
    (x, y)
}

/// Paints the artwork once for a tile grid covering the displayed page.
fn tile_placements(display_w: f64, display_h: f64, art_w: f64, art_h: f64, rotation_deg: f64) -> Vec<(f64, f64)> {
    let step_x = (art_w * 1.6).max(40.0);
    let step_y = (art_h * 2.2).max(40.0);
    let diagonal = (display_w * display_w + display_h * display_h).sqrt();
    let cols = ((diagonal / step_x).ceil() as i32) + 2;
    let rows = ((diagonal / step_y).ceil() as i32) + 2;
    let mut out = Vec::new();
    let start_x = (display_w - (cols as f64 - 1.0) * step_x) / 2.0;
    let start_y = (display_h - (rows as f64 - 1.0) * step_y) / 2.0;
    let _ = rotation_deg;
    for row in 0..rows {
        for col in 0..cols {
            out.push((start_x + col as f64 * step_x, start_y + row as f64 * step_y));
        }
    }
    out
}

fn paint_page(
    doc: &mut Document,
    page_id: lopdf::ObjectId,
    artwork: &Artwork,
    options: &WatermarkOptions,
    xobject_id: lopdf::ObjectId,
) -> PdfResult<()> {
    let rotation = page_rotation(doc, page_id)?;
    let media = page_mediabox(doc, page_id)?;
    let (page_w, page_h) = (media[2] - media[0], media[3] - media[1]);
    let (display_w, display_h) = Matrix::displayed_size(rotation, page_w, page_h);
    let to_page = Matrix::display_to_page(rotation, page_w, page_h);

    let placements = if options.tile {
        tile_placements(display_w, display_h, artwork.width_pt, artwork.height_pt, options.rotation_deg)
    } else {
        let origin = position_origin(
            &options.position,
            display_w,
            display_h,
            artwork.width_pt,
            artwork.height_pt,
            options.margin_pt,
        );
        vec![origin]
    };

    // Register the XObject resource once per page.
    let resource_name = format!("WM{}", xobject_id.0);
    add_resource_entry(doc, page_id, b"XObject", &resource_name, Object::Reference(xobject_id))?;

    let mut content = String::new();
    content.push_str("q\n");
    content.push_str(&format!("{}\n", to_page.to_cm()));
    for (x, y) in placements {
        let center = Matrix::translate(x + artwork.width_pt / 2.0, y + artwork.height_pt / 2.0);
        let m = center
            .mul(Matrix::rotate_deg(options.rotation_deg))
            .mul(Matrix::translate(-artwork.width_pt / 2.0, -artwork.height_pt / 2.0))
            .mul(Matrix::scale(artwork.width_pt, artwork.height_pt));
        // Each placement gets its own q/Q so the transforms never accumulate.
        content.push_str("q\n");
        content.push_str(&format!("{}\n", m.to_cm()));
        content.push_str(&format!("/{resource_name} Do\n"));
        content.push_str("Q\n");
    }
    content.push_str("Q\n");
    append_page_content(doc, page_id, content.into_bytes())?;
    Ok(())
}

/// Paints the watermark into the document (page content and resources).
///
/// Shared by the rewrite path and the incremental one; it only appends content
/// and adds objects, so the difference between two revisions stays additive.
fn apply_watermark(
    doc: &mut Document,
    options: &WatermarkOptions,
    progress: &ProgressCallback,
    cancel: &CancelToken,
) -> PdfResult<()> {
    let pages = doc.get_pages();
    let total = pages.len() as u32;
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

    let first_page_w = {
        let id = pages[&1];
        let media = page_mediabox(doc, id)?;
        media[2] - media[0]
    };
    let artwork = if options.kind.eq_ignore_ascii_case("image") {
        build_image_artwork(options, first_page_w)?
    } else {
        build_text_artwork(options)?
    };
    if artwork.image.width == 0 || artwork.image.height == 0 {
        return Err(PdfError::InvalidInput("watermark artwork is empty".into()));
    }
    let xobject_id = add_rgba_image_xobject(&mut *doc, &artwork.image)?;

    for (index, page_number) in target.iter().enumerate() {
        cancel.check()?;
        progress(ProgressEvent::new("watermark.page", index as u64, target.len() as u64));
        let page_id = doc.get_pages().get(page_number).copied().ok_or(PdfError::RangeOutOfBounds)?;
        paint_page(&mut *doc, page_id, &artwork, options, xobject_id)?;
    }

    Ok(())
}

/// Adds the watermark as an appended revision instead of rewriting the file.
///
/// Every byte of `input` is preserved, so a document that is already signed
/// keeps every signature valid: the watermark is a later revision, and a reader
/// can still tell what was signed and what was added afterwards.
pub fn watermark_pdf_incremental(input: &[u8], options: &WatermarkOptions) -> PdfResult<Vec<u8>> {
    let mut doc = Document::load_mem(input).map_err(|error| PdfError::from_lopdf(error, None))?;
    if doc.is_encrypted() || doc.was_encrypted() {
        return Err(PdfError::PasswordRequired);
    }
    let silent = |_event: ProgressEvent| {};
    apply_watermark(&mut doc, options, &silent, &CancelToken::new())?;
    crate::incremental::apply_difference(input, &doc)
}

pub fn add_watermark(
    input: &Path,
    output: &Path,
    options: &WatermarkOptions,
    policy: OverwritePolicy,
    password: Option<&str>,
    progress: &ProgressCallback,
    cancel: &CancelToken,
) -> PdfResult<PathBuf> {
    let mut doc = load_document(input, password)?;
    apply_watermark(&mut doc, options, progress, cancel)?;
    let final_path = resolve_output_path(output, policy)?;
    save_document(&mut doc, &final_path, true)?;
    Ok(final_path)
}
