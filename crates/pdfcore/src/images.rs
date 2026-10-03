//! Image encoding helpers and the Images -> PDF builder.

use crate::docutil::*;
use crate::error::{PdfError, PdfResult};
use crate::progress::{CancelToken, ProgressCallback, ProgressEvent};
use crate::render::RenderedPage;
use lopdf::{dictionary, Document, Object};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Standard base64 with padding, for `data:` URLs handed to the webview.
///
/// Written out rather than pulled in as a dependency: the crate has none, and a
/// four-line encoder is cheaper than one.
pub fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((triple >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((triple >> 12) & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 { ALPHABET[((triple >> 6) & 0x3f) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { ALPHABET[(triple & 0x3f) as usize] as char } else { '=' });
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageFormat {
    Jpeg,
    Png,
}

impl ImageFormat {
    pub fn from_extension(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_string_lossy().to_lowercase();
        match ext.as_str() {
            "jpg" | "jpeg" | "jpe" => Some(ImageFormat::Jpeg),
            "png" => Some(ImageFormat::Png),
            _ => None,
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            ImageFormat::Jpeg => "jpg",
            ImageFormat::Png => "png",
        }
    }
}

/// Encodes an RGBA buffer as JPEG or PNG, optionally converting to grayscale.
pub fn encode_image(page: &RenderedPage, format: ImageFormat, jpeg_quality: u8, grayscale: bool) -> PdfResult<Vec<u8>> {
    let img = page.to_dynamic_image()?;
    let img = if grayscale {
        image::DynamicImage::ImageLuma8(img.to_luma8())
    } else {
        image::DynamicImage::ImageRgb8(img.to_rgb8())
    };
    let mut out = Vec::new();
    match format {
        ImageFormat::Jpeg => {
            let quality = jpeg_quality.clamp(1, 100);
            let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality);
            encoder.encode_image(&img).map_err(|e| PdfError::ConversionFailed(format!("JPEG encoding failed: {e}")))?;
        }
        ImageFormat::Png => {
            img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
                .map_err(|e| PdfError::ConversionFailed(format!("PNG encoding failed: {e}")))?;
        }
    }
    Ok(out)
}

/// Decodes an image file into memory, mapping failures to friendly errors.
pub fn decode_image(path: &Path) -> PdfResult<image::DynamicImage> {
    if !path.exists() {
        return Err(PdfError::NotFound(path.display().to_string()));
    }
    let reader =
        image::ImageReader::open(path).map_err(|e| PdfError::InvalidImage(format!("{}: {e}", path.display())))?;
    reader
        .with_guessed_format()
        .map_err(|e| PdfError::InvalidImage(format!("{}: {e}", path.display())))?
        .decode()
        .map_err(|e| PdfError::InvalidImage(format!("{}: {e}", path.display())))
}

// ---------------------------------------------------------------------------
// Images -> PDF
// ---------------------------------------------------------------------------

pub const PAGE_SIZES_PT: &[(&str, f64, f64)] = &[
    ("a3", 841.89, 1190.55),
    ("a4", 595.28, 841.89),
    ("a5", 419.53, 595.28),
    ("letter", 612.0, 792.0),
    ("legal", 612.0, 1008.0),
];

pub fn page_size_points(name: &str) -> Option<(f64, f64)> {
    PAGE_SIZES_PT.iter().find(|(n, _, _)| n.eq_ignore_ascii_case(name)).map(|(_, w, h)| (*w, *h))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageToPdfOptions {
    /// a3 | a4 | a5 | letter | legal | original | custom
    #[serde(default = "default_page_size")]
    pub page_size: String,
    #[serde(default)]
    pub custom_width_pt: f64,
    #[serde(default)]
    pub custom_height_pt: f64,
    /// auto | portrait | landscape
    #[serde(default = "default_orientation")]
    pub orientation: String,
    /// fit (contain) | fill (cover) | actual (1 image pixel = 1/ dpi point)
    #[serde(default = "default_fit")]
    pub fit: String,
    #[serde(default = "default_margin")]
    pub margin_pt: f64,
    /// Assumed resolution for images without DPI metadata.
    #[serde(default = "default_dpi")]
    pub dpi: u32,
    /// JPEG re-encode quality (only used when re-encoding is necessary).
    #[serde(default = "default_quality")]
    pub jpeg_quality: u8,
}

fn default_page_size() -> String {
    "a4".into()
}
fn default_orientation() -> String {
    "auto".into()
}
fn default_fit() -> String {
    "fit".into()
}
fn default_margin() -> f64 {
    0.0
}
fn default_dpi() -> u32 {
    96
}
fn default_quality() -> u8 {
    92
}

impl Default for ImageToPdfOptions {
    fn default() -> Self {
        Self {
            page_size: default_page_size(),
            custom_width_pt: 595.28,
            custom_height_pt: 841.89,
            orientation: default_orientation(),
            fit: default_fit(),
            margin_pt: 0.0,
            dpi: 96,
            jpeg_quality: default_quality(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageItem {
    pub path: String,
    #[serde(default)]
    pub rotation_delta: i32,
}

/// Builds a new PDF from image files, one image per page.
pub fn images_to_pdf(
    items: &[ImageItem],
    options: &ImageToPdfOptions,
    output: &Path,
    policy: OverwritePolicy,
    progress: &ProgressCallback,
    cancel: &CancelToken,
) -> PdfResult<PathBuf> {
    if items.is_empty() {
        return Err(PdfError::InvalidInput("no images selected".into()));
    }
    let mut doc = Document::new();
    doc.version = "1.6".to_string();
    let mut page_ids = Vec::new();

    for (index, item) in items.iter().enumerate() {
        cancel.check()?;
        progress(ProgressEvent::new("images.embed", index as u64, items.len() as u64));
        let path = PathBuf::from(&item.path);
        let image = decode_image(&path)?;
        let (img_w, img_h) = (image.width() as f64, image.height() as f64);
        if img_w < 1.0 || img_h < 1.0 {
            return Err(PdfError::InvalidImage(format!("{} is empty", path.display())));
        }

        let (page_w, page_h) = match options.page_size.to_lowercase().as_str() {
            "original" => (img_w * 72.0 / options.dpi.max(1) as f64, img_h * 72.0 / options.dpi.max(1) as f64),
            "custom" => (options.custom_width_pt.max(1.0), options.custom_height_pt.max(1.0)),
            other => {
                page_size_points(other).ok_or_else(|| PdfError::InvalidInput(format!("unknown page size '{other}'")))?
            }
        };
        let landscape_image = img_w > img_h;
        let (page_w, page_h) = match options.orientation.to_lowercase().as_str() {
            "landscape" => (page_w.max(page_h), page_w.min(page_h)),
            "portrait" => (page_w.min(page_h), page_w.max(page_h)),
            _ => {
                if landscape_image && page_w < page_h {
                    (page_h, page_w)
                } else {
                    (page_w, page_h)
                }
            }
        };

        let margin = options.margin_pt.max(0.0);
        let avail_w = (page_w - margin * 2.0).max(1.0);
        let avail_h = (page_h - margin * 2.0).max(1.0);
        let (draw_w, draw_h) = match options.fit.to_lowercase().as_str() {
            "fill" => {
                let scale = (avail_w / img_w).max(avail_h / img_h);
                (img_w * scale, img_h * scale)
            }
            "actual" => {
                let scale = 72.0 / options.dpi.max(1) as f64;
                (img_w * scale, img_h * scale)
            }
            _ => {
                let scale = (avail_w / img_w).min(avail_h / img_h);
                (img_w * scale, img_h * scale)
            }
        };
        let x = (page_w - draw_w) / 2.0;
        let y = (page_h - draw_h) / 2.0;

        let xobject_id = embed_image(&mut doc, &path, &image, options.jpeg_quality)?;
        let page_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![
                Object::Real(0.0),
                Object::Real(0.0),
                Object::Real(page_w as f32),
                Object::Real(page_h as f32),
            ],
        }));
        add_resource_entry(&mut doc, page_id, b"XObject", "Im0", Object::Reference(xobject_id))?;
        let mut content = String::new();
        content.push_str("q\n");
        if options.fit.eq_ignore_ascii_case("fill") {
            // Clip to the page so cover crops instead of overflowing.
            content.push_str(&format!("0 0 {page_w:.2} {page_h:.2} re W n\n"));
        }
        let rotation = ((item.rotation_delta % 360) + 360) % 360;
        if rotation == 0 {
            content.push_str(&format!("{draw_w:.4} 0 0 {draw_h:.4} {x:.4} {y:.4} cm\n/Im0 Do\n"));
        } else {
            // Rotate the image around the center of its placement rect.
            let center = Matrix::translate(x + draw_w / 2.0, y + draw_h / 2.0);
            let m = center
                .mul(Matrix::rotate_deg(rotation as f64))
                .mul(Matrix::translate(-draw_w / 2.0, -draw_h / 2.0))
                .mul(Matrix::scale(draw_w, draw_h));
            content.push_str(&format!("{}\n/Im0 Do\n", m.to_cm()));
        }
        content.push_str("Q\n");
        append_page_content(&mut doc, page_id, content.into_bytes())?;
        page_ids.push(page_id);
    }

    let pages_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Pages",
        "Kids" => page_ids.iter().map(|id| Object::Reference(*id)).collect::<Vec<Object>>(),
        "Count" => page_ids.len() as i64,
    }));
    for page_id in &page_ids {
        doc.get_object_mut(*page_id)?.as_dict_mut()?.set("Parent", Object::Reference(pages_id));
    }
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));
    let info_id = doc.add_object(Object::Dictionary(dictionary! {
        "Producer" => pdf_text_object("PDF Swiss Army Knife"),
        "Creator" => pdf_text_object("PDF Swiss Army Knife"),
    }));
    doc.trailer.set("Info", Object::Reference(info_id));
    doc.trailer.set("Size", Object::Integer((doc.max_id + 1) as i64));

    let final_path = resolve_output_path(output, policy)?;
    save_document(&mut doc, &final_path, true)?;
    progress(ProgressEvent::new("images.embed", items.len() as u64, items.len() as u64));
    Ok(final_path)
}

/// Embeds a decoded image, passing JPEG bytes through untouched when no
/// re-encode is required.
fn embed_image(
    doc: &mut Document,
    path: &Path,
    image: &image::DynamicImage,
    jpeg_quality: u8,
) -> PdfResult<lopdf::ObjectId> {
    let width = image.width();
    let height = image.height();
    let is_jpeg = ImageFormat::from_extension(path) == Some(ImageFormat::Jpeg);
    if is_jpeg && jpeg_quality >= 95 {
        if let Ok(bytes) = std::fs::read(path) {
            return add_jpeg_image_xobject(doc, width, height, bytes, false);
        }
    }
    let has_alpha = image.color().has_alpha();
    if has_alpha {
        let rgba = image.to_rgba8();
        let raw = RawImage { width, height, rgba: rgba.into_raw() };
        add_rgba_image_xobject(doc, &raw)
    } else {
        let rgb = image.to_rgb8();
        add_rgb_image_xobject(doc, width, height, rgb.into_raw(), false)
    }
}
