//! Annotations.
//!
//! Two independent paths share this module:
//!
//! * The original flatten path ([`annotate_pdf`], [`annotate_pdf_incremental`])
//!   draws text/image/rect/highlight/line (and the newer kinds where a vector
//!   drawing makes sense) straight into the page content. The result is
//!   portable and printable but no longer editable.
//! * The editable path ([`annotate_editable_pdf`],
//!   [`annotate_editable_pdf_incremental`], [`edit_annotations`]) writes real
//!   annotation dictionaries into each page's `/Annots` array, so a viewer can
//!   select, move, edit and delete them. [`list_annotations`] reads them back
//!   in display space (top-left origin) for the UI canvas.

use crate::docutil::*;
use crate::error::{PdfError, PdfResult};
use crate::progress::{CancelToken, ProgressCallback, ProgressEvent};
use crate::textimg::{self, TextRenderRequest};
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream, StringFormat};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    /// text | image | rect | highlight | line | note | underline | strike |
    /// ink | textbox | signature
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
    /// Ink strokes in display space (top-left origin), absolute on the page.
    #[serde(default)]
    pub strokes: Vec<Vec<[f64; 2]>>,
    /// PNG bytes for image/signature annotations, base64 encoded (a `data:`
    /// URL prefix is accepted). Takes precedence over `image_path`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_base64: Option<String>,
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

/// An annotation read back from a document, in display space (top-left).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditableAnnotation {
    /// 1-based page number.
    pub page: u32,
    /// Index among the page's annotations (stable for edit/delete).
    pub index: u32,
    /// note|highlight|underline|strike|ink|textbox|signature|image|rect|line|text
    pub kind: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub text: String,
    /// "#rrggbb"
    pub color: String,
    pub opacity: f64,
    pub line_width_pt: f64,
    pub font_size_pt: f64,
    pub bold: bool,
    /// Ink strokes in display space (top-left origin).
    pub strokes: Vec<Vec<[f64; 2]>>,
}

/// One edit applied to an existing annotation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "action")]
pub enum AnnotationAction {
    /// Display-space delta.
    Move {
        dx: f64,
        dy: f64,
    },
    /// New display-space rectangle.
    Resize {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    },
    Delete,
    Update {
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        color: Option<String>,
        #[serde(default)]
        opacity: Option<f64>,
        #[serde(default, rename = "lineWidthPt")]
        line_width_pt: Option<f64>,
    },
}

/// A page/index handle plus the action to apply to that annotation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationEditItem {
    pub page: u32,
    pub index: u32,
    #[serde(flatten)]
    pub action: AnnotationAction,
}

/// Outcome of a batch of annotation edits.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationEditReport {
    pub edited: u32,
    pub deleted: u32,
    pub warnings: Vec<String>,
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
            "text" | "textbox" => {
                let rendered = draw_text_image(&mut content, &mut *doc, page_id, display_h, annotation)?;
                if !rendered {
                    continue;
                }
            }
            "note" => {
                // Flattened notes become a solid marker; the editable path
                // writes a real /Text note with a comment icon instead.
                let (x, y) = to_page_space(display_h, annotation.x, annotation.y, annotation.w, annotation.h);
                let w = annotation.w.max(4.0);
                let h = annotation.h.max(4.0);
                content.push_str(&format!("{r:.4} {g:.4} {b:.4} rg\n{x:.2} {y:.2} {w:.2} {h:.2} re\nf\n"));
            }
            "underline" | "strike" => {
                let line_top = if annotation.kind == "underline" {
                    annotation.y + annotation.h
                } else {
                    annotation.y + annotation.h / 2.0
                };
                let (x, y) = to_page_space(display_h, annotation.x, line_top, annotation.w, 0.0);
                content.push_str(&format!(
                    "{r:.4} {g:.4} {b:.4} RG\n{lw:.2} w\n{x:.2} {y:.2} m\n{x2:.2} {y:.2} l\nS\n",
                    lw = annotation.line_width_pt.max(0.1),
                    x2 = x + annotation.w
                ));
            }
            "ink" => {
                for stroke in &annotation.strokes {
                    if stroke.len() < 2 {
                        continue;
                    }
                    content.push_str(&format!(
                        "{r:.4} {g:.4} {b:.4} RG\n{lw:.2} w\n",
                        lw = annotation.line_width_pt.max(0.1)
                    ));
                    for (point_index, point) in stroke.iter().enumerate() {
                        let px = point[0];
                        let py = display_h - point[1];
                        let operator = if point_index == 0 { "m" } else { "l" };
                        content.push_str(&format!("{px:.2} {py:.2} {operator}\n"));
                    }
                    content.push_str("S\n");
                }
            }
            "signature" => {
                let image = annotation_image(annotation)?;
                let rgba = image.to_rgba8();
                let raw = RawImage { width: rgba.width(), height: rgba.height(), rgba: rgba.into_raw() };
                let xobject_id = add_rgba_image_xobject(&mut *doc, &raw)?;
                let name = format!("AN{}", xobject_id.0);
                add_resource_entry(&mut *doc, page_id, b"XObject", &name, Object::Reference(xobject_id))?;
                let (x, y) = to_page_space(display_h, annotation.x, annotation.y, annotation.w, annotation.h);
                let m = Matrix::translate(x, y).mul(Matrix::scale(annotation.w, annotation.h));
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

// ---------------------------------------------------------------------------
// Real (editable) annotations: /Annots dictionaries, listing and editing
// ---------------------------------------------------------------------------

/// Page geometry needed to translate between display space (top-left origin,
/// /Rotate applied) and the page's own coordinate system (bottom-left).
struct PageGeometry {
    page_id: ObjectId,
    rotation: i32,
    page_w: f64,
    page_h: f64,
    display_h: f64,
    to_page: Matrix,
    to_display: Matrix,
}

impl PageGeometry {
    fn new(doc: &Document, page_id: ObjectId) -> PdfResult<Self> {
        let rotation = page_rotation_or_inherited(doc, page_id);
        let media = page_mediabox_or_inherited(doc, page_id)?;
        let (page_w, page_h) = (media[2] - media[0], media[3] - media[1]);
        let (_, display_h) = Matrix::displayed_size(rotation, page_w, page_h);
        let to_page = Matrix::display_to_page(rotation, page_w, page_h);
        let to_display = to_page.inverse().unwrap_or(Matrix::IDENTITY);
        Ok(Self { page_id, rotation, page_w, page_h, display_h, to_page, to_display })
    }
}

/// MediaBox lookup that follows the page tree, so listing works on documents
/// whose pages inherit it (reading is not allowed to materialize attributes).
fn page_mediabox_or_inherited(doc: &Document, page_id: ObjectId) -> PdfResult<[f64; 4]> {
    let mut current = Some(page_id);
    let mut hops = 0;
    while let Some(id) = current {
        hops += 1;
        if hops > 64 {
            break;
        }
        if let Ok(media) = page_mediabox(doc, id) {
            return Ok(media);
        }
        current = doc
            .get_dictionary(id)
            .ok()
            .and_then(|page| page.get(b"Parent").ok().and_then(|value| value.as_reference().ok()));
    }
    Err(PdfError::CorruptPdf("page has no MediaBox".into()))
}

/// /Rotate lookup following the page tree (inherited rotation).
fn page_rotation_or_inherited(doc: &Document, page_id: ObjectId) -> i32 {
    let mut current = Some(page_id);
    let mut hops = 0;
    while let Some(id) = current {
        hops += 1;
        if hops > 64 {
            break;
        }
        let page = match doc.get_dictionary(id) {
            Ok(page) => page,
            Err(_) => break,
        };
        let rotation = page.get(b"Rotate").ok().and_then(|value| value.as_i64().ok());
        if let Some(rotation) = rotation {
            return rotation.rem_euclid(360) as i32;
        }
        current = page.get(b"Parent").ok().and_then(|value| value.as_reference().ok());
    }
    0
}

/// Display rect (top-left) -> page rect [x y w h] (bottom-left, normalized).
fn display_rect_to_page(geo: &PageGeometry, x: f64, y: f64, w: f64, h: f64) -> [f64; 4] {
    let rect = [x, geo.display_h - y - h, w, h];
    normalize_rect(display_rect_to_page_rect(geo.rotation, geo.page_w, geo.page_h, rect))
}

/// Page rect [x y w h] -> display rect (x, y_top, w, h).
fn page_rect_to_display(geo: &PageGeometry, rect: [f64; 4]) -> (f64, f64, f64, f64) {
    let rect = normalize_rect(rect);
    let corners = [
        geo.to_display.apply(rect[0], rect[1]),
        geo.to_display.apply(rect[0] + rect[2], rect[1]),
        geo.to_display.apply(rect[0], rect[1] + rect[3]),
        geo.to_display.apply(rect[0] + rect[2], rect[1] + rect[3]),
    ];
    let min_x = corners.iter().map(|point| point.0).fold(f64::MAX, f64::min);
    let max_x = corners.iter().map(|point| point.0).fold(f64::MIN, f64::max);
    let min_y = corners.iter().map(|point| point.1).fold(f64::MAX, f64::min);
    let max_y = corners.iter().map(|point| point.1).fold(f64::MIN, f64::max);
    (min_x, geo.display_h - max_y, max_x - min_x, max_y - min_y)
}

fn normalize_rect(rect: [f64; 4]) -> [f64; 4] {
    let (x0, x1) = if rect[2] >= 0.0 { (rect[0], rect[0] + rect[2]) } else { (rect[0] + rect[2], rect[0]) };
    let (y0, y1) = if rect[3] >= 0.0 { (rect[1], rect[1] + rect[3]) } else { (rect[1] + rect[3], rect[1]) };
    [x0, y0, x1 - x0, y1 - y0]
}

/// lopdf stores numbers as f32; every coordinate conversion lands here.
fn real(value: f64) -> Object {
    Object::Real(value as f32)
}

/// Display point (top-left) -> page point (bottom-left).
fn display_point_to_page(geo: &PageGeometry, x: f64, y: f64) -> (f64, f64) {
    geo.to_page.apply(x, geo.display_h - y)
}

/// Page point (bottom-left) -> display point (top-left).
fn page_point_to_display(geo: &PageGeometry, x: f64, y: f64) -> (f64, f64) {
    let (dx, dy) = geo.to_display.apply(x, y);
    (dx, geo.display_h - dy)
}

fn rect_objects(rect: [f64; 4]) -> Object {
    Object::Array(vec![real(rect[0]), real(rect[1]), real(rect[0] + rect[2]), real(rect[1] + rect[3])])
}

/// `/QuadPoints` in page space, ordered top-left, top-right, bottom-left,
/// bottom-right (ISO 32000-1, 12.5.6.10).
fn quad_points_object(geo: &PageGeometry, x: f64, y: f64, w: f64, h: f64) -> Object {
    let corners = [(x, y), (x + w, y), (x, y + h), (x + w, y + h)];
    let mut points = Vec::with_capacity(8);
    for (px, py) in corners {
        let (nx, ny) = display_point_to_page(geo, px, py);
        points.push(real(nx));
        points.push(real(ny));
    }
    Object::Array(points)
}

/// `/InkList` in page space, one flat `[x y x y ...]` array per stroke.
fn ink_list_object(geo: &PageGeometry, strokes: &[Vec<[f64; 2]>]) -> Object {
    let mut list: Vec<Object> = Vec::with_capacity(strokes.len());
    for stroke in strokes {
        let mut flat: Vec<Object> = Vec::with_capacity(stroke.len() * 2);
        for point in stroke {
            let (nx, ny) = display_point_to_page(geo, point[0], point[1]);
            flat.push(real(nx));
            flat.push(real(ny));
        }
        list.push(Object::Array(flat));
    }
    Object::Array(list)
}

/// Parses a `/Rect` array. ISO 32000-1 stores it as the two corners
/// `[x0 y0 x1 y1]` (either corner may come first), this returns the normalized
/// `[x y w h]` rectangle.
fn dict_rect(dict: &Dictionary) -> Option<[f64; 4]> {
    let values = dict.get(b"Rect").ok()?.as_array().ok()?;
    if values.len() != 4 {
        return None;
    }
    let x0 = object_to_f64(&values[0])?;
    let y0 = object_to_f64(&values[1])?;
    let x1 = object_to_f64(&values[2])?;
    let y1 = object_to_f64(&values[3])?;
    let (min_x, max_x) = (x0.min(x1), x0.max(x1));
    let (min_y, max_y) = (y0.min(y1), y0.max(y1));
    Some([min_x, min_y, max_x - min_x, max_y - min_y])
}

/// Resolves an array-valued key that may be stored inline or behind a
/// reference (foreign annotation dictionaries do both).
fn dict_array(doc: &Document, dict: &Dictionary, key: &[u8]) -> Option<Vec<Object>> {
    match dict.get(key).ok()? {
        Object::Array(items) => Some(items.clone()),
        Object::Reference(id) => doc.get_object(*id).ok()?.as_array().ok().cloned(),
        _ => None,
    }
}

fn dict_dict_value(doc: &Document, dict: &Dictionary, key: &[u8]) -> Option<Dictionary> {
    match dict.get(key).ok()? {
        Object::Dictionary(value) => Some(value.clone()),
        Object::Reference(id) => match doc.get_object(*id).ok()? {
            Object::Dictionary(value) => Some(value.clone()),
            _ => None,
        },
        _ => None,
    }
}

fn annotation_subtype(dict: &Dictionary) -> String {
    dict.get(b"Subtype")
        .ok()
        .and_then(|value| value.as_name().ok())
        .map(|name| String::from_utf8_lossy(name).to_string())
        .unwrap_or_default()
}

fn color_components(color: &str) -> (f64, f64, f64) {
    let rgba = crate::watermark::parse_hex_color(color);
    (rgba[0] as f64 / 255.0, rgba[1] as f64 / 255.0, rgba[2] as f64 / 255.0)
}

fn rgb_array(r: f64, g: f64, b: f64) -> Object {
    Object::Array(vec![real(r), real(g), real(b)])
}

/// Parses a `/C` array (DeviceGray, DeviceRGB or DeviceCMYK) to "#rrggbb".
fn parse_pdf_color(obj: &Object) -> Option<String> {
    let values = obj.as_array().ok()?;
    let numbers: Vec<f64> = values.iter().map(object_to_f64).collect::<Option<Vec<f64>>>()?;
    let to_byte = |value: f64| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    Some(match numbers.len() {
        1 => {
            let gray = to_byte(numbers[0]);
            format!("#{gray:02x}{gray:02x}{gray:02x}")
        }
        3 => format!("#{:02x}{:02x}{:02x}", to_byte(numbers[0]), to_byte(numbers[1]), to_byte(numbers[2])),
        4 => {
            let k = numbers[3].clamp(0.0, 1.0);
            let r = (1.0 - numbers[0].clamp(0.0, 1.0)) * (1.0 - k);
            let g = (1.0 - numbers[1].clamp(0.0, 1.0)) * (1.0 - k);
            let b = (1.0 - numbers[2].clamp(0.0, 1.0)) * (1.0 - k);
            format!("#{:02x}{:02x}{:02x}", to_byte(r), to_byte(g), to_byte(b))
        }
        _ => return None,
    })
}

fn default_listing_color(kind: &str) -> &'static str {
    match kind {
        "note" => "#f59e0b",
        "highlight" => "#facc15",
        "underline" | "strike" | "ink" => "#e11d48",
        "textbox" => "#0f172a",
        _ => "#000000",
    }
}

fn default_listing_opacity(kind: &str) -> f64 {
    match kind {
        "highlight" => 0.35,
        _ => 1.0,
    }
}

/// Font size and bold flag from a `/DA` string (`0 0 0 rg /Helvetica 12 Tf`).
fn da_font_info(dict: &Dictionary) -> Option<(f64, bool)> {
    let appearance = dict.get(b"DA").ok().and_then(pdf_text_value)?;
    let tokens: Vec<&str> = appearance.split_whitespace().collect();
    for index in 2..tokens.len() {
        if tokens[index] == "Tf" {
            let size = tokens[index - 1].parse::<f64>().ok().filter(|size| *size > 0.0).unwrap_or(12.0);
            let bold = tokens[index - 2].to_ascii_lowercase().contains("bold");
            return Some((size, bold));
        }
    }
    None
}

/// `/M` timestamp for a fresh or edited annotation.
fn annotation_moment() -> String {
    let now = time::OffsetDateTime::now_utc();
    format!(
        "D:{:04}{:02}{:02}{:02}{:02}{:02}Z",
        now.year(),
        now.month() as u8,
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// `/RC` rich text mirroring `/Contents`, so editors that prefer XHTML show
/// the same text.
fn set_rich_text(dict: &mut Dictionary, text: &str) {
    let rc = format!(
        "<?xml version=\"1.0\"?><body xmlns=\"http://www.w3.org/1999/xhtml\"><p>{}</p></body>",
        xml_escape(text)
    );
    dict.set("RC", pdf_text_object(&rc));
}

/// Standard base64 decoder (with optional `data:` URL prefix).
fn base64_decode(input: &str) -> PdfResult<Vec<u8>> {
    let trimmed = input.trim();
    let payload = match trimmed.rsplit_once(',') {
        Some((prefix, rest)) if prefix.trim_start().starts_with("data:") => rest.trim(),
        _ => trimmed,
    };
    let mut out = Vec::with_capacity(payload.len() / 4 * 3);
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for byte in payload.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            b' ' | b'\r' | b'\n' | b'\t' => continue,
            _ => return Err(PdfError::InvalidInput("image_base64 is not valid base64".into())),
        };
        buffer = (buffer << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    Ok(out)
}

/// Decodes the payload of an image/signature annotation. `image_base64` (PNG
/// or any format the image crate sniffs) wins over `image_path`; with neither
/// present the annotation is rejected with `InvalidInput` rather than silently
/// dropped, so a broken signature cannot slip through unnoticed.
fn annotation_image(annotation: &Annotation) -> PdfResult<image::DynamicImage> {
    if let Some(encoded) = annotation.image_base64.as_deref().filter(|value| !value.trim().is_empty()) {
        let bytes = base64_decode(encoded)?;
        return image::load_from_memory(&bytes)
            .map_err(|error| PdfError::InvalidImage(format!("image annotation payload: {error}")));
    }
    if let Some(path) = annotation.image_path.as_deref().filter(|value| !value.trim().is_empty()) {
        return crate::images::decode_image(Path::new(path));
    }
    Err(PdfError::InvalidInput("image/signature annotation needs image_base64 or image_path".into()))
}

fn image_cache_key(annotation: &Annotation) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    annotation.image_base64.as_deref().unwrap_or_default().hash(&mut hasher);
    annotation.image_path.as_deref().unwrap_or_default().hash(&mut hasher);
    hasher.finish()
}

/// Wraps drawing operations in a Form XObject (used as an annotation `/AP`).
fn add_form_xobject(doc: &mut Document, width: f64, height: f64, resources: Dictionary, content: String) -> ObjectId {
    let mut dict = Dictionary::new();
    dict.set("Type", "XObject");
    dict.set("Subtype", "Form");
    dict.set("FormType", 1i64);
    dict.set("BBox", vec![real(0.0), real(0.0), real(width.max(1.0)), real(height.max(1.0))]);
    dict.set("Resources", Object::Dictionary(resources));
    doc.add_object(Object::Stream(Stream::new(dict, content.into_bytes())))
}

/// Appearance for an image/signature stamp; the image XObject is cached so a
/// repeated signature embeds its pixels once.
fn add_image_appearance_cached(
    doc: &mut Document,
    raw: &RawImage,
    width: f64,
    height: f64,
    cache: &mut HashMap<u64, ObjectId>,
    cache_key: u64,
) -> PdfResult<ObjectId> {
    let image_id = match cache.get(&cache_key) {
        Some(id) => *id,
        None => {
            let id = add_rgba_image_xobject(doc, raw)?;
            cache.insert(cache_key, id);
            id
        }
    };
    let resources =
        dictionary! { "XObject" => Object::Dictionary(dictionary! { "Im0" => Object::Reference(image_id) }) };
    let content = format!("q\n{w:.4} 0 0 {h:.4} 0 0 cm\n/Im0 Do\nQ\n", w = width.max(1.0), h = height.max(1.0));
    Ok(add_form_xobject(doc, width, height, resources, content))
}

struct TextAppearanceRequest<'a> {
    text: &'a str,
    font_size_pt: f64,
    bold: bool,
    color: [u8; 4],
    width: f64,
    height: f64,
}

/// Renders the text with the bundled Unicode font and wraps the bitmap in a
/// Form XObject, so even viewers that ignore `/DA` show the box contents.
fn add_text_appearance(doc: &mut Document, request: &TextAppearanceRequest<'_>) -> PdfResult<Option<ObjectId>> {
    if request.text.trim().is_empty() {
        return Ok(None);
    }
    let scale = 4.0f64;
    let wrap_width_px = if request.width > 0.0 { Some((request.width * scale) as f32) } else { None };
    let art = textimg::render_text(&TextRenderRequest {
        text: request.text.to_string(),
        size_px: (request.font_size_pt.max(1.0) * scale) as f32,
        color: request.color,
        bold: request.bold,
        line_spacing: 1.2,
        padding_px: (scale * 2.0) as u32,
        wrap_width_px,
    })?;
    let image_id = add_rgba_image_xobject(doc, &art)?;
    let resources =
        dictionary! { "XObject" => Object::Dictionary(dictionary! { "Im0" => Object::Reference(image_id) }) };
    let content =
        format!("q\n{w:.4} 0 0 {h:.4} 0 0 cm\n/Im0 Do\nQ\n", w = request.width.max(1.0), h = request.height.max(1.0));
    Ok(Some(add_form_xobject(doc, request.width, request.height, resources, content)))
}

/// Flatten path helper shared by "text" and "textbox": renders the text
/// bitmap into `content`. Returns false when the text is empty (nothing drawn).
fn draw_text_image(
    content: &mut String,
    doc: &mut Document,
    page_id: ObjectId,
    display_h: f64,
    annotation: &Annotation,
) -> PdfResult<bool> {
    if annotation.text.trim().is_empty() {
        return Ok(false);
    }
    let color = crate::watermark::parse_hex_color(&annotation.color);
    let scale = 4.0f64;
    let request = TextRenderRequest {
        text: annotation.text.clone(),
        size_px: (annotation.font_size_pt * scale) as f32,
        color,
        bold: annotation.bold,
        line_spacing: 1.2,
        padding_px: (scale * 2.0) as u32,
        wrap_width_px: if annotation.w > 0.0 { Some((annotation.w * scale) as f32) } else { None },
    };
    let art = textimg::render_text(&request)?;
    let xobject_id = add_rgba_image_xobject(doc, &art)?;
    let name = format!("AN{}", xobject_id.0);
    add_resource_entry(doc, page_id, b"XObject", &name, Object::Reference(xobject_id))?;
    let draw_w = art.width as f64 / scale;
    let draw_h = art.height as f64 / scale;
    // Anchor the text box top-left at the requested position.
    let x = annotation.x;
    let y = display_h - annotation.y - draw_h;
    let m = Matrix::translate(x, y).mul(Matrix::scale(draw_w, draw_h));
    content.push_str(&format!("{}\n/{name} Do\n", m.to_cm()));
    Ok(true)
}

/// Builds and registers the annotation dictionary for one [`Annotation`].
fn build_annotation_dict(
    doc: &mut Document,
    geo: &PageGeometry,
    annotation: &Annotation,
    moment: &str,
    image_cache: &mut HashMap<u64, ObjectId>,
) -> PdfResult<ObjectId> {
    let (r, g, b) = color_components(&annotation.color);
    let width = annotation.w.max(0.0);
    let height = annotation.h.max(0.0);

    let mut dict = Dictionary::new();
    dict.set("Type", "Annot");
    dict.set("F", 4i64);
    dict.set("M", Object::String(moment.as_bytes().to_vec(), StringFormat::Literal));
    dict.set("P", Object::Reference(geo.page_id));

    match annotation.kind.as_str() {
        "note" => {
            dict.set("Subtype", "Text");
            dict.set("Name", "Comment");
            dict.set("Contents", pdf_text_object(&annotation.text));
            dict.set("C", rgb_array(r, g, b));
        }
        "highlight" | "underline" | "strike" => {
            let subtype = match annotation.kind.as_str() {
                "highlight" => "Highlight",
                "underline" => "Underline",
                _ => "StrikeOut",
            };
            dict.set("Subtype", subtype);
            dict.set("QuadPoints", quad_points_object(geo, annotation.x, annotation.y, width, height));
            dict.set("C", rgb_array(r, g, b));
            if annotation.kind == "highlight" {
                dict.set("CA", real(annotation.opacity.clamp(0.0, 1.0)));
            }
        }
        "ink" => {
            dict.set("Subtype", "Ink");
            dict.set("InkList", ink_list_object(geo, &annotation.strokes));
            dict.set("C", rgb_array(r, g, b));
            dict.set("BS", Object::Dictionary(dictionary! { "W" => real(annotation.line_width_pt.max(0.1)) }));
        }
        "textbox" | "text" => {
            dict.set("Subtype", "FreeText");
            dict.set("Contents", pdf_text_object(&annotation.text));
            let font = if annotation.bold { "/Helvetica-Bold" } else { "/Helvetica" };
            dict.set("DA", format!("{r:.3} {g:.3} {b:.3} rg {font} {:.2} Tf", annotation.font_size_pt.max(1.0)));
            dict.set("Q", 0i64);
            set_rich_text(&mut dict, &annotation.text);
            let request = TextAppearanceRequest {
                text: &annotation.text,
                font_size_pt: annotation.font_size_pt,
                bold: annotation.bold,
                color: crate::watermark::parse_hex_color(&annotation.color),
                width,
                height,
            };
            if let Some(appearance) = add_text_appearance(doc, &request)? {
                dict.set("AP", Object::Dictionary(dictionary! { "N" => Object::Reference(appearance) }));
            }
        }
        "signature" | "image" => {
            dict.set("Subtype", "Stamp");
            dict.set("Name", if annotation.kind == "signature" { "OsakSignature" } else { "Draft" });
            let image = annotation_image(annotation)?;
            let rgba = image.to_rgba8();
            let raw = RawImage { width: rgba.width(), height: rgba.height(), rgba: rgba.into_raw() };
            let cache_key = image_cache_key(annotation);
            let appearance = add_image_appearance_cached(doc, &raw, width, height, image_cache, cache_key)?;
            dict.set("AP", Object::Dictionary(dictionary! { "N" => Object::Reference(appearance) }));
        }
        "rect" => {
            dict.set("Subtype", "Square");
            dict.set("C", rgb_array(r, g, b));
            dict.set("BS", Object::Dictionary(dictionary! { "W" => real(annotation.line_width_pt.max(0.1)) }));
        }
        "line" => {
            dict.set("Subtype", "Line");
            let (x1, y1) = display_point_to_page(geo, annotation.x, annotation.y);
            let (x2, y2) = display_point_to_page(
                geo,
                annotation.x2.unwrap_or(annotation.x + annotation.w),
                annotation.y2.unwrap_or(annotation.y + annotation.h),
            );
            dict.set("L", vec![real(x1), real(y1), real(x2), real(y2)]);
            dict.set("C", rgb_array(r, g, b));
            dict.set("BS", Object::Dictionary(dictionary! { "W" => real(annotation.line_width_pt.max(0.1)) }));
        }
        other => {
            return Err(PdfError::InvalidInput(format!("unknown annotation '{other}'")));
        }
    }

    let page_rect = display_rect_to_page(geo, annotation.x, annotation.y, width, height);
    dict.set("Rect", rect_objects(page_rect));
    Ok(doc.add_object(Object::Dictionary(dict)))
}

/// The page's `/Annots` entries as stored (inline or behind a reference).
fn annots_items(doc: &Document, page_id: ObjectId) -> Vec<Object> {
    let page = match doc.get_dictionary(page_id) {
        Ok(page) => page,
        Err(_) => return Vec::new(),
    };
    match page.get(b"Annots").ok() {
        Some(Object::Array(items)) => items.clone(),
        Some(Object::Reference(id)) => {
            doc.get_object(*id).ok().and_then(|object| object.as_array().ok().cloned()).unwrap_or_default()
        }
        _ => Vec::new(),
    }
}

/// Stores the annotation array on the page itself. Detaching from a possibly
/// shared indirect array keeps the edit local to this page (the old array
/// object simply stays behind, unreferenced).
fn set_page_annots(doc: &mut Document, page_id: ObjectId, items: Vec<Object>) -> PdfResult<()> {
    doc.get_dictionary_mut(page_id)?.set("Annots", Object::Array(items));
    Ok(())
}

fn append_annotation_to_page(doc: &mut Document, page_id: ObjectId, reference: Object) -> PdfResult<()> {
    let mut items = annots_items(doc, page_id);
    items.push(reference);
    set_page_annots(doc, page_id, items)
}

fn apply_editable_annotations(
    doc: &mut Document,
    annotations: &[Annotation],
    progress: &ProgressCallback,
    cancel: &CancelToken,
) -> PdfResult<()> {
    materialize_all_pages(doc)?;
    let total = doc.get_pages().len() as u32;
    let moment = annotation_moment();
    let mut image_cache: HashMap<u64, ObjectId> = HashMap::new();

    for (index, annotation) in annotations.iter().enumerate() {
        cancel.check()?;
        progress(ProgressEvent::new("annotate.editable", index as u64, annotations.len() as u64));
        if annotation.page == 0 || annotation.page > total {
            return Err(PdfError::RangeOutOfBounds);
        }
        let page_id = doc.get_pages().get(&annotation.page).copied().ok_or(PdfError::RangeOutOfBounds)?;
        let geo = PageGeometry::new(doc, page_id)?;
        let object_id = build_annotation_dict(doc, &geo, annotation, &moment, &mut image_cache)?;
        append_annotation_to_page(doc, page_id, Object::Reference(object_id))?;
    }
    Ok(())
}

/// Writes the annotations as real, editable `/Annots` dictionaries into an
/// appended revision. Every byte of `input` is preserved, so a signed document
/// keeps every signature valid; the annotations are simply a later revision.
pub fn annotate_editable_pdf_incremental(input: &[u8], annotations: &[Annotation]) -> PdfResult<Vec<u8>> {
    if annotations.is_empty() {
        return Err(PdfError::InvalidInput("no annotations to apply".into()));
    }
    let mut doc = Document::load_mem(input).map_err(|error| PdfError::from_lopdf(error, None))?;
    if doc.is_encrypted() || doc.was_encrypted() {
        return Err(PdfError::PasswordRequired);
    }
    let silent = |_event: ProgressEvent| {};
    apply_editable_annotations(&mut doc, annotations, &silent, &CancelToken::new())?;
    crate::incremental::apply_difference(input, &doc)
}

/// Writes the annotations as real, editable `/Annots` dictionaries, rewriting
/// the whole document (the flatten path is [`annotate_pdf`]).
pub fn annotate_editable_pdf(
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
    apply_editable_annotations(&mut doc, annotations, progress, cancel)?;
    let final_path = resolve_output_path(output, policy)?;
    save_document(&mut doc, &final_path, true)?;
    Ok(final_path)
}

/// Every annotation on every page, in page then `/Annots` order, mapped back
/// into display space. Entries that are not dictionaries are skipped, but they
/// still consume their index so edit/delete handles stay stable.
pub fn list_annotations(doc: &Document) -> Vec<EditableAnnotation> {
    let mut out = Vec::new();
    for (page_number, page_id) in doc.get_pages() {
        let geo = match PageGeometry::new(doc, page_id) {
            Ok(geo) => geo,
            Err(_) => continue,
        };
        for (position, entry) in annots_items(doc, page_id).iter().enumerate() {
            let dict = match entry {
                Object::Reference(id) => match doc.get_object(*id) {
                    Ok(Object::Dictionary(dict)) => dict.clone(),
                    _ => continue,
                },
                Object::Dictionary(dict) => dict.clone(),
                _ => continue,
            };
            out.push(read_editable_annotation(doc, &geo, page_number, position as u32, &dict));
        }
    }
    out
}

/// [`list_annotations`] for a file on disk.
pub fn list_annotations_in_file(path: &Path, password: Option<&str>) -> PdfResult<Vec<EditableAnnotation>> {
    let doc = load_document(path, password)?;
    Ok(list_annotations(&doc))
}

fn read_editable_annotation(
    doc: &Document,
    geo: &PageGeometry,
    page: u32,
    index: u32,
    dict: &Dictionary,
) -> EditableAnnotation {
    let subtype = annotation_subtype(dict);
    let stamp_name = dict
        .get(b"Name")
        .ok()
        .and_then(|value| value.as_name().ok())
        .map(|name| String::from_utf8_lossy(name).to_string())
        .unwrap_or_default();
    let kind = match subtype.as_str() {
        "Text" => "note".to_string(),
        "Highlight" => "highlight".to_string(),
        "Underline" => "underline".to_string(),
        "StrikeOut" => "strike".to_string(),
        "Ink" => "ink".to_string(),
        "FreeText" => "textbox".to_string(),
        "Stamp" if stamp_name == "OsakSignature" => "signature".to_string(),
        "Stamp" => "image".to_string(),
        other => other.to_ascii_lowercase(),
    };
    let rect = match kind.as_str() {
        "highlight" | "underline" | "strike" => {
            quad_points_display(doc, dict, geo).or_else(|| dict_rect(dict).map(|rect| page_rect_to_display(geo, rect)))
        }
        _ => dict_rect(dict).map(|rect| page_rect_to_display(geo, rect)),
    }
    .unwrap_or((0.0, 0.0, 0.0, 0.0));
    let strokes = if kind == "ink" { ink_strokes_display(doc, dict, geo) } else { Vec::new() };
    let text = dict.get(b"Contents").ok().and_then(pdf_text_value).unwrap_or_default();
    let color =
        dict.get(b"C").ok().and_then(parse_pdf_color).unwrap_or_else(|| default_listing_color(&kind).to_string());
    let opacity = dict.get(b"CA").ok().and_then(object_to_f64).unwrap_or_else(|| default_listing_opacity(&kind));
    let line_width_pt = dict_dict_value(doc, dict, b"BS")
        .and_then(|style| style.get(b"W").ok().and_then(object_to_f64))
        .or_else(|| dict_array(doc, dict, b"Border").and_then(|border| border.first().and_then(object_to_f64)))
        .unwrap_or_else(default_line_width);
    let (font_size_pt, bold) = da_font_info(dict).unwrap_or((12.0, false));
    EditableAnnotation {
        page,
        index,
        kind,
        x: rect.0,
        y: rect.1,
        w: rect.2,
        h: rect.3,
        text,
        color,
        opacity,
        line_width_pt,
        font_size_pt,
        bold,
        strokes,
    }
}

/// Bounding box of `/QuadPoints` mapped into display space.
fn quad_points_display(doc: &Document, dict: &Dictionary, geo: &PageGeometry) -> Option<(f64, f64, f64, f64)> {
    let items = dict_array(doc, dict, b"QuadPoints")?;
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    for pair in items.chunks(2) {
        if pair.len() < 2 {
            return None;
        }
        let x = object_to_f64(&pair[0])?;
        let y = object_to_f64(&pair[1])?;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    if min_x > max_x {
        return None;
    }
    Some(page_rect_to_display(geo, [min_x, min_y, max_x - min_x, max_y - min_y]))
}

/// `/InkList` strokes mapped into display space.
fn ink_strokes_display(doc: &Document, dict: &Dictionary, geo: &PageGeometry) -> Vec<Vec<[f64; 2]>> {
    let Some(items) = dict_array(doc, dict, b"InkList") else {
        return Vec::new();
    };
    let mut strokes = Vec::new();
    for stroke in &items {
        let Some(points) = stroke.as_array().ok() else {
            continue;
        };
        let mut converted: Vec<[f64; 2]> = Vec::new();
        for pair in points.chunks(2) {
            if pair.len() < 2 {
                break;
            }
            let (Some(x), Some(y)) = (object_to_f64(&pair[0]), object_to_f64(&pair[1])) else {
                break;
            };
            let (dx, dy) = page_point_to_display(geo, x, y);
            converted.push([dx, dy]);
        }
        if !converted.is_empty() {
            strokes.push(converted);
        }
    }
    strokes
}

/// Stable handle for one annotation entry (indirect or inline).
#[derive(Debug, Clone, Copy)]
enum AnnotHandle {
    Ref(ObjectId),
    Inline { page_id: ObjectId, position: usize },
}

fn find_annotation(doc: &Document, page_id: ObjectId, index: u32) -> Option<(AnnotHandle, Dictionary)> {
    let items = annots_items(doc, page_id);
    match items.get(index as usize)? {
        Object::Reference(id) => {
            let dict = doc.get_dictionary(*id).ok()?.clone();
            Some((AnnotHandle::Ref(*id), dict))
        }
        Object::Dictionary(dict) => Some((AnnotHandle::Inline { page_id, position: index as usize }, dict.clone())),
        _ => None,
    }
}

/// Writes an edited dictionary back to its original location. Inline entries
/// are rewritten as an inline array on the page (see [`set_page_annots`]).
fn store_annotation(doc: &mut Document, handle: AnnotHandle, dict: Dictionary) -> PdfResult<()> {
    match handle {
        AnnotHandle::Ref(id) => {
            *doc.get_dictionary_mut(id)? = dict;
        }
        AnnotHandle::Inline { page_id, position } => {
            let mut items = annots_items(doc, page_id);
            if let Some(slot) = items.get_mut(position) {
                *slot = Object::Dictionary(dict);
            }
            set_page_annots(doc, page_id, items)?;
        }
    }
    Ok(())
}

/// Removes the entry from the page's `/Annots` array. The annotation object
/// itself (if indirect and no longer referenced anywhere) is intentionally
/// left in the file: `incremental::apply_difference` only supports adding and
/// replacing objects, never removing them, so an incremental edit must not
/// drop it. A later full rewrite prunes it.
fn delete_annotation(doc: &mut Document, page_id: ObjectId, index: u32) -> PdfResult<()> {
    let mut items = annots_items(doc, page_id);
    if (index as usize) < items.len() {
        items.remove(index as usize);
    }
    set_page_annots(doc, page_id, items)
}

/// Applies a batch of annotation edits as an appended revision.
///
/// The result always starts with the exact input bytes, so existing signatures
/// stay valid. Deletions remove the `/Annots` entry but leave the orphan
/// annotation object in place (an incremental update cannot remove objects);
/// callers that need the object gone should save the document with
/// [`annotate_editable_pdf`]-style rewriting instead.
///
/// Items are applied in order, and an index refers to the annotation array as
/// it exists at that moment: a deletion shifts the indexes of everything after
/// it, so callers that mix deletes with other edits should send the delete
/// last (or adjust later indexes).
pub fn edit_annotations(pdf: &[u8], items: &[AnnotationEditItem]) -> PdfResult<(Vec<u8>, AnnotationEditReport)> {
    let mut doc = Document::load_mem(pdf).map_err(|error| PdfError::from_lopdf(error, None))?;
    if doc.is_encrypted() || doc.was_encrypted() {
        return Err(PdfError::PasswordRequired);
    }
    let moment = annotation_moment();
    let mut report = AnnotationEditReport::default();

    for item in items {
        let page_id = match doc.get_pages().get(&item.page).copied() {
            Some(page_id) => page_id,
            None => {
                report.warnings.push(format!("page {} does not exist", item.page));
                continue;
            }
        };
        let geo = match PageGeometry::new(&doc, page_id) {
            Ok(geo) => geo,
            Err(error) => {
                report.warnings.push(format!("page {}: {error}", item.page));
                continue;
            }
        };
        let (handle, mut dict) = match find_annotation(&doc, page_id, item.index) {
            Some(found) => found,
            None => {
                report.warnings.push(format!("page {} has no annotation at index {}", item.page, item.index));
                continue;
            }
        };

        match &item.action {
            AnnotationAction::Delete => {
                delete_annotation(&mut doc, page_id, item.index)?;
                report.deleted += 1;
                continue;
            }
            AnnotationAction::Move { dx, dy } => {
                let rect = dict_rect(&dict).unwrap_or([0.0, 0.0, 0.0, 0.0]);
                let old = page_rect_to_display(&geo, rect);
                let new = (old.0 + dx, old.1 + dy, old.2, old.3);
                dict.set("Rect", rect_objects(display_rect_to_page(&geo, new.0, new.1, new.2, new.3)));
                remap_markup(&geo, &mut dict, old, new);
            }
            AnnotationAction::Resize { x, y, w, h } => {
                let rect = dict_rect(&dict).unwrap_or([0.0, 0.0, 0.0, 0.0]);
                let old = page_rect_to_display(&geo, rect);
                let new = (*x, *y, *w, *h);
                dict.set("Rect", rect_objects(display_rect_to_page(&geo, new.0, new.1, new.2, new.3)));
                remap_markup(&geo, &mut dict, old, new);
                if matches!(annotation_subtype(&dict).as_str(), "FreeText" | "Stamp") {
                    rescale_appearance(&mut doc, &mut dict, new.2.max(1.0), new.3.max(1.0))?;
                }
            }
            AnnotationAction::Update { text, color, opacity, line_width_pt } => {
                let subtype = annotation_subtype(&dict);
                let mut text_changed = false;
                let mut color_changed = false;
                if let Some(text) = text {
                    dict.set("Contents", pdf_text_object(text));
                    set_rich_text(&mut dict, text);
                    text_changed = true;
                }
                if let Some(color) = color {
                    let (r, g, b) = color_components(color);
                    dict.set("C", rgb_array(r, g, b));
                    if subtype == "FreeText" {
                        let (font_size, bold) = da_font_info(&dict).unwrap_or((12.0, false));
                        let font = if bold { "/Helvetica-Bold" } else { "/Helvetica" };
                        dict.set("DA", format!("{r:.3} {g:.3} {b:.3} rg {font} {font_size:.2} Tf"));
                    }
                    color_changed = true;
                }
                if let Some(opacity) = opacity {
                    dict.set("CA", real(opacity.clamp(0.0, 1.0)));
                }
                if let Some(width) = line_width_pt {
                    let mut style = dict_dict_value(&doc, &dict, b"BS").unwrap_or_default();
                    style.set("W", real(width.max(0.0)));
                    dict.set("BS", Object::Dictionary(style));
                }
                if subtype == "FreeText" && (text_changed || color_changed) {
                    rebuild_text_appearance(&mut doc, &mut dict, &geo)?;
                }
            }
        }

        dict.set("M", Object::String(moment.as_bytes().to_vec(), StringFormat::Literal));
        store_annotation(&mut doc, handle, dict)?;
        report.edited += 1;
    }

    let output = crate::incremental::apply_difference(pdf, &doc)?;
    Ok((output, report))
}

/// Re-renders a FreeText appearance after a text or colour update.
fn rebuild_text_appearance(doc: &mut Document, dict: &mut Dictionary, geo: &PageGeometry) -> PdfResult<()> {
    let rect = dict_rect(dict).unwrap_or([0.0, 0.0, 0.0, 0.0]);
    let (_, _, width, height) = page_rect_to_display(geo, rect);
    let text = dict.get(b"Contents").ok().and_then(pdf_text_value).unwrap_or_default();
    let (font_size_pt, bold) = da_font_info(dict).unwrap_or((12.0, false));
    let color = dict
        .get(b"C")
        .ok()
        .and_then(parse_pdf_color)
        .map(|hex| crate::watermark::parse_hex_color(&hex))
        .unwrap_or([15, 23, 42, 255]);
    let request = TextAppearanceRequest { text: &text, font_size_pt, bold, color, width, height };
    if let Some(appearance) = add_text_appearance(doc, &request)? {
        dict.set("AP", Object::Dictionary(dictionary! { "N" => Object::Reference(appearance) }));
    }
    Ok(())
}

/// Rescales an existing appearance stream to the new rectangle by wrapping it
/// in a Form XObject, so text boxes and stamps resized in the UI stay in sync
/// without re-decoding their content.
fn rescale_appearance(doc: &mut Document, dict: &mut Dictionary, new_width: f64, new_height: f64) -> PdfResult<()> {
    let appearance =
        dict.get(b"AP").ok().and_then(|value| value.as_dict().ok()).and_then(|ap| ap.get(b"N").ok()).cloned();
    let Some(appearance) = appearance else {
        return Ok(());
    };
    let (old_width, old_height, reference) = match appearance {
        Object::Reference(id) => {
            let size = doc
                .get_object(id)
                .ok()
                .and_then(|object| object.as_stream().ok())
                .map(|stream| bbox_size(&stream.dict))
                .unwrap_or((0.0, 0.0));
            (size.0, size.1, id)
        }
        Object::Stream(stream) => {
            let size = bbox_size(&stream.dict);
            let id = doc.add_object(Object::Stream(stream));
            (size.0, size.1, id)
        }
        _ => return Ok(()),
    };
    let scale_x = if old_width > 0.5 { new_width.max(1.0) / old_width } else { 1.0 };
    let scale_y = if old_height > 0.5 { new_height.max(1.0) / old_height } else { 1.0 };
    let resources =
        dictionary! { "XObject" => Object::Dictionary(dictionary! { "Fm0" => Object::Reference(reference) }) };
    let content = format!("q\n{scale_x:.4} 0 0 {scale_y:.4} 0 0 cm\n/Fm0 Do\nQ\n");
    let rescaled = add_form_xobject(doc, new_width.max(1.0), new_height.max(1.0), resources, content);
    dict.set("AP", Object::Dictionary(dictionary! { "N" => Object::Reference(rescaled) }));
    Ok(())
}

fn bbox_size(dict: &Dictionary) -> (f64, f64) {
    let values = match dict.get(b"BBox").ok().and_then(|value| value.as_array().ok()) {
        Some(values) if values.len() == 4 => values,
        _ => return (0.0, 0.0),
    };
    let x0 = object_to_f64(&values[0]).unwrap_or(0.0);
    let y0 = object_to_f64(&values[1]).unwrap_or(0.0);
    let x1 = object_to_f64(&values[2]).unwrap_or(x0);
    let y1 = object_to_f64(&values[3]).unwrap_or(y0);
    ((x1 - x0).abs(), (y1 - y0).abs())
}

/// Moves/scales `/QuadPoints` and `/InkList` alongside a `/Rect` change, so
/// markup stays glued to what it highlighted.
fn remap_markup(geo: &PageGeometry, dict: &mut Dictionary, old: (f64, f64, f64, f64), new: (f64, f64, f64, f64)) {
    remap_quad_points(geo, dict, old, new);
    remap_ink_list(geo, dict, old, new);
}

fn remap_display_point(point: (f64, f64), old: (f64, f64, f64, f64), new: (f64, f64, f64, f64)) -> (f64, f64) {
    let u = if old.2.abs() > 1e-9 { (point.0 - old.0) / old.2 } else { 0.0 };
    let v = if old.3.abs() > 1e-9 { (point.1 - old.1) / old.3 } else { 0.0 };
    (new.0 + u * new.2, new.1 + v * new.3)
}

fn remap_quad_points(geo: &PageGeometry, dict: &mut Dictionary, old: (f64, f64, f64, f64), new: (f64, f64, f64, f64)) {
    let Some(items) = dict.get(b"QuadPoints").ok().and_then(|value| value.as_array().ok()).cloned() else {
        return;
    };
    let mut remapped = Vec::with_capacity(items.len());
    for pair in items.chunks(2) {
        if pair.len() < 2 {
            return;
        }
        let (Some(x), Some(y)) = (object_to_f64(&pair[0]), object_to_f64(&pair[1])) else {
            return;
        };
        let moved = remap_display_point(page_point_to_display(geo, x, y), old, new);
        let (nx, ny) = display_point_to_page(geo, moved.0, moved.1);
        remapped.push(real(nx));
        remapped.push(real(ny));
    }
    dict.set("QuadPoints", Object::Array(remapped));
}

fn remap_ink_list(geo: &PageGeometry, dict: &mut Dictionary, old: (f64, f64, f64, f64), new: (f64, f64, f64, f64)) {
    let Some(items) = dict.get(b"InkList").ok().and_then(|value| value.as_array().ok()).cloned() else {
        return;
    };
    let mut strokes = Vec::with_capacity(items.len());
    for stroke in &items {
        let Some(points) = stroke.as_array().ok() else {
            return;
        };
        let mut remapped = Vec::with_capacity(points.len());
        for pair in points.chunks(2) {
            if pair.len() < 2 {
                return;
            }
            let (Some(x), Some(y)) = (object_to_f64(&pair[0]), object_to_f64(&pair[1])) else {
                return;
            };
            let moved = remap_display_point(page_point_to_display(geo, x, y), old, new);
            let (nx, ny) = display_point_to_page(geo, moved.0, moved.1);
            remapped.push(real(nx));
            remapped.push(real(ny));
        }
        strokes.push(Object::Array(remapped));
    }
    dict.set("InkList", Object::Array(strokes));
}
