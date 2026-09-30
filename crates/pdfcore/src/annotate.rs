//! Basic annotations: text stamps, image stamps, rectangles, highlights and
//! lines. Everything is flattened into the page content (no optional content
//! or AcroForm machinery), which keeps the result portable and printable.

use crate::docutil::*;
use crate::error::{PdfError, PdfResult};
use crate::progress::{CancelToken, ProgressCallback, ProgressEvent};
use crate::textimg::{self, TextRenderRequest};
use lopdf::{dictionary, Document, Object};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    /// text | image | rect | highlight | line
    pub kind: String,
    /// 1-based page number.
    pub page: u32,
    /// Display-space rectangle in points with the origin at the TOP-LEFT of
    /// the displayed page (matches how the UI canvas reports coordinates).
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    #[serde(default)]
    pub text: String,
    #[serde(default = "default_font_size")]
    pub font_size_pt: f64,
    #[serde(default)]
    pub bold: bool,
    /// "#RRGGBB"
    #[serde(default = "default_color")]
    pub color: String,
    #[serde(default = "default_opacity")]
    pub opacity: f64,
    #[serde(default)]
    pub image_path: Option<String>,
    #[serde(default = "default_line_width")]
    pub line_width_pt: f64,
    /// For line annotations: the second point, in the same top-left space.
    #[serde(default)]
    pub x2: Option<f64>,
    #[serde(default)]
    pub y2: Option<f64>,
}

fn default_font_size() -> f64 {
    14.0
}
fn default_color() -> String {
    "#e11d48".into()
}
fn default_opacity() -> f64 {
    0.35
}
fn default_line_width() -> f64 {
    2.0
}

/// Registry of ExtGState objects created per opacity value.
struct ExtGStateCache {
    map: std::collections::HashMap<u32, lopdf::ObjectId>,
}

impl ExtGStateCache {
    fn new() -> Self {
        Self { map: std::collections::HashMap::new() }
    }

    fn get(&mut self, doc: &mut Document, page_id: lopdf::ObjectId, opacity: f64) -> PdfResult<String> {
        let key = (opacity.clamp(0.0, 1.0) * 1000.0).round() as u32;
        if let Some(id) = self.map.get(&key) {
            return Ok(format!("GS{}", id.0));
        }
        let alpha = opacity.clamp(0.0, 1.0) as f32;
        let id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "ExtGState",
            "ca" => Object::Real(alpha),
            "CA" => Object::Real(alpha),
        }));
        let name = format!("GS{}", id.0);
        add_resource_entry(doc, page_id, b"ExtGState", &name, Object::Reference(id))?;
        self.map.insert(key, id);
        Ok(name)
    }
}

fn to_page_space(page_h: f64, x: f64, y_top: f64, _w: f64, h: f64) -> (f64, f64) {
    // UI top-left -> display-space bottom-left.
    (x, page_h - y_top - h)
}

/// Draws the annotations into the document (page content and resources).
///
/// Shared by the rewrite path and the incremental one; everything it touches is
/// a page, a resource dictionary or a new object, never a deletion.
fn apply_annotations(
    doc: &mut Document,
    annotations: &[Annotation],
    progress: &ProgressCallback,
    cancel: &CancelToken,
) -> PdfResult<()> {
    let total = doc.get_pages().len() as u32;
    materialize_all_pages(doc)?;
    let mut extgstates = ExtGStateCache::new();
    let mut image_cache: std::collections::HashMap<String, lopdf::ObjectId> = std::collections::HashMap::new();

    for (index, annotation) in annotations.iter().enumerate() {
        cancel.check()?;
        progress(ProgressEvent::new("annotate.item", index as u64, annotations.len() as u64));
        if annotation.page == 0 || annotation.page > total {
            return Err(PdfError::RangeOutOfBounds);
        }
        let page_id = doc.get_pages().get(&annotation.page).copied().ok_or(PdfError::RangeOutOfBounds)?;
        let rotation = page_rotation(doc, page_id)?;
        let media = page_mediabox(doc, page_id)?;
        let (page_w, page_h) = (media[2] - media[0], media[3] - media[1]);
        let (_, display_h) = Matrix::displayed_size(rotation, page_w, page_h);
        let to_page = Matrix::display_to_page(rotation, page_w, page_h);
        let color = crate::watermark::parse_hex_color(&annotation.color);
        let (r, g, b) = (color[0] as f64 / 255.0, color[1] as f64 / 255.0, color[2] as f64 / 255.0);
        let mut content = String::new();
        content.push_str("q\n");
        content.push_str(&format!("{}\n", to_page.to_cm()));

        match annotation.kind.as_str() {
            "rect" => {
                let (x, y) = to_page_space(display_h, annotation.x, annotation.y, annotation.w, annotation.h);
                content.push_str(&format!(
                    "{r:.4} {g:.4} {b:.4} RG\n{lw:.2} w\n{x:.2} {y:.2} {w:.2} {h:.2} re\nS\n",
                    lw = annotation.line_width_pt.max(0.1),
                    x = x,
                    y = y,
                    w = annotation.w,
                    h = annotation.h
                ));
            }
            "highlight" => {
                let gs = extgstates.get(&mut *doc, page_id, annotation.opacity)?;
                let (x, y) = to_page_space(display_h, annotation.x, annotation.y, annotation.w, annotation.h);
                content.push_str(&format!(
                    "/{gs} gs\n{r:.4} {g:.4} {b:.4} rg\n{x:.2} {y:.2} {w:.2} {h:.2} re\nf\n",
                    x = x,
                    y = y,
                    w = annotation.w,
                    h = annotation.h
                ));
            }
            "line" => {
                let x1 = annotation.x;
                let y1 = display_h - annotation.y;
                let x2 = annotation.x2.unwrap_or(annotation.x + annotation.w);
                let y2 = display_h - annotation.y2.unwrap_or(annotation.y + annotation.h);
                content.push_str(&format!(
                    "{r:.4} {g:.4} {b:.4} RG\n{lw:.2} w\n{x1:.2} {y1:.2} m\n{x2:.2} {y2:.2} l\nS\n",
                    lw = annotation.line_width_pt.max(0.1)
                ));
            }
            "text" => {
                if annotation.text.trim().is_empty() {
                    continue;
                }
                let scale = 4.0f64;
                let request = TextRenderRequest {
                    text: annotation.text.clone(),
                    size_px: (annotation.font_size_pt * scale) as f32,
                    color: [color[0], color[1], color[2], color[3]],
                    bold: annotation.bold,
                    line_spacing: 1.2,
                    padding_px: (scale * 2.0) as u32,
                    wrap_width_px: if annotation.w > 0.0 { Some((annotation.w * scale) as f32) } else { None },
                };
                let art = textimg::render_text(&request)?;
                let xobject_id = add_rgba_image_xobject(&mut *doc, &art)?;
                let name = format!("AN{}", xobject_id.0);
                add_resource_entry(&mut *doc, page_id, b"XObject", &name, Object::Reference(xobject_id))?;
                let draw_w = art.width as f64 / scale;
                let draw_h = art.height as f64 / scale;
                // Anchor the text box top-left at the requested position.
                let x = annotation.x;
                let y = display_h - annotation.y - draw_h;
                let m = Matrix::translate(x, y).mul(Matrix::scale(draw_w, draw_h));
                content.push_str(&format!("{}\n/{name} Do\n", m.to_cm()));
            }
            "image" => {
                let path = annotation
                    .image_path
                    .as_ref()
                    .ok_or_else(|| PdfError::InvalidInput("image annotation needs a file".into()))?;
                let xobject_id = match image_cache.get(path) {
                    Some(id) => *id,
                    None => {
                        let decoded = crate::images::decode_image(Path::new(path))?;
                        let rgba = decoded.to_rgba8();
                        let raw = RawImage { width: rgba.width(), height: rgba.height(), rgba: rgba.into_raw() };
                        let id = add_rgba_image_xobject(&mut *doc, &raw)?;
                        image_cache.insert(path.clone(), id);
                        id
                    }
                };
                let name = format!("AN{}", xobject_id.0);
                add_resource_entry(&mut *doc, page_id, b"XObject", &name, Object::Reference(xobject_id))?;
                let (x, y) = to_page_space(display_h, annotation.x, annotation.y, annotation.w, annotation.h);
                let m = Matrix::translate(x, y).mul(Matrix::scale(annotation.w, annotation.h));
                content.push_str(&format!("{}\n/{name} Do\n", m.to_cm()));
            }
            other => {
                return Err(PdfError::InvalidInput(format!("unknown annotation '{other}'")));
            }
        }
        content.push_str("Q\n");
        append_page_content(&mut *doc, page_id, content.into_bytes())?;
    }
    Ok(())
}

/// Adds the annotations as an appended revision instead of rewriting the file.
///
/// Every byte of `input` is preserved, so a document that is already signed
/// keeps every signature valid: the stamp is simply a later revision, and the
/// reader can still tell what was signed and what was added afterwards.
pub fn annotate_pdf_incremental(input: &[u8], annotations: &[Annotation]) -> PdfResult<Vec<u8>> {
    if annotations.is_empty() {
        return Err(PdfError::InvalidInput("no annotations to apply".into()));
    }
    let mut doc = Document::load_mem(input).map_err(|error| PdfError::from_lopdf(error, None))?;
    if doc.is_encrypted() || doc.was_encrypted() {
        return Err(PdfError::PasswordRequired);
    }
    let silent = |_event: ProgressEvent| {};
    apply_annotations(&mut doc, annotations, &silent, &CancelToken::new())?;
    crate::incremental::apply_difference(input, &doc)
}

pub fn annotate_pdf(
    input: &Path,
    output: &Path,
    annotations: &[Annotation],
    policy: OverwritePolicy,
    password: Option<&str>,
    progress: &ProgressCallback,
    cancel: &CancelToken,
) -> PdfResult<PathBuf> {
    if annotations.is_empty() {
        return Err(PdfError::InvalidInput("no annotations to apply".into()));
    }
    let mut doc = load_document(input, password)?;
    apply_annotations(&mut doc, annotations, progress, cancel)?;
    let final_path = resolve_output_path(output, policy)?;
    save_document(&mut doc, &final_path, true)?;
    Ok(final_path)
}
