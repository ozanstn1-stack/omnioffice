//! Page size conversion (Fit / Stretch / Actual) and cropping via CropBox.

use crate::docutil::*;
use crate::error::{PdfError, PdfResult};
use crate::images::page_size_points;
use crate::progress::{CancelToken, ProgressCallback, ProgressEvent};
use lopdf::{Document, Object};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResizeOptions {
    /// a3 | a4 | a5 | letter | legal | custom
    #[serde(default = "default_size")]
    pub page_size: String,
    #[serde(default)]
    pub custom_width_pt: f64,
    #[serde(default)]
    pub custom_height_pt: f64,
    /// auto | portrait | landscape
    #[serde(default = "default_orientation")]
    pub orientation: String,
    /// fit (contain, keep aspect) | stretch (fill, may distort)
    #[serde(default = "default_mode")]
    pub mode: String,
    /// Empty = all pages.
    #[serde(default)]
    pub pages: Vec<u32>,
}

fn default_size() -> String {
    "a4".into()
}
fn default_orientation() -> String {
    "auto".into()
}
fn default_mode() -> String {
    "fit".into()
}

impl Default for ResizeOptions {
    fn default() -> Self {
        Self {
            page_size: default_size(),
            custom_width_pt: 595.28,
            custom_height_pt: 841.89,
            orientation: default_orientation(),
            mode: default_mode(),
            pages: Vec::new(),
        }
    }
}

fn resolve_target_size(options: &ResizeOptions, current_w: f64, current_h: f64) -> PdfResult<(f64, f64)> {
    let (mut w, mut h) = match options.page_size.to_lowercase().as_str() {
        "custom" => (options.custom_width_pt.max(1.0), options.custom_height_pt.max(1.0)),
        other => {
            page_size_points(other).ok_or_else(|| PdfError::InvalidInput(format!("unknown page size '{other}'")))?
        }
    };
    let current_landscape = current_w > current_h;
    match options.orientation.to_lowercase().as_str() {
        "landscape" => {
            if w < h {
                std::mem::swap(&mut w, &mut h);
            }
        }
        "portrait" => {
            if w > h {
                std::mem::swap(&mut w, &mut h);
            }
        }
        _ => {
            // auto: follow the current orientation
            let target_landscape = w > h;
            if current_landscape != target_landscape {
                std::mem::swap(&mut w, &mut h);
            }
        }
    }
    Ok((w, h))
}

pub fn resize_pages(
    input: &Path,
    output: &Path,
    options: &ResizeOptions,
    policy: OverwritePolicy,
    password: Option<&str>,
    progress: &ProgressCallback,
    cancel: &CancelToken,
) -> PdfResult<PathBuf> {
    let mut doc = load_document(input, password)?;
    let total = doc.get_pages().len() as u32;
    if total == 0 {
        return Err(PdfError::InvalidPdf("document has no pages".into()));
    }
    materialize_all_pages(&mut doc)?;
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
        progress(ProgressEvent::new("resize.page", index as u64, target.len() as u64));
        let page_id = doc.get_pages().get(page_number).copied().ok_or(PdfError::RangeOutOfBounds)?;
        let media = page_mediabox(&doc, page_id)?;
        let (x0, y0, x1, y1) = (media[0], media[1], media[2], media[3]);
        let (cur_w, cur_h) = ((x1 - x0).abs(), (y1 - y0).abs());
        if cur_w < 1.0 || cur_h < 1.0 {
            return Err(PdfError::CorruptPdf(format!("page {page_number} has an empty MediaBox")));
        }
        let (target_w, target_h) = resolve_target_size(options, cur_w, cur_h)?;
        let (scale_x, scale_y) = if options.mode.eq_ignore_ascii_case("stretch") {
            (target_w / cur_w, target_h / cur_h)
        } else {
            let s = (target_w / cur_w).min(target_h / cur_h);
            (s, s)
        };
        let draw_w = cur_w * scale_x;
        let draw_h = cur_h * scale_y;
        let offset_x = (target_w - draw_w) / 2.0;
        let offset_y = (target_h - draw_h) / 2.0;
        // Move the old origin to (0,0), scale, then center on the new page.
        let matrix =
            Matrix::translate(offset_x, offset_y).mul(Matrix::scale(scale_x, scale_y)).mul(Matrix::translate(-x0, -y0));
        wrap_page_content_transform(&mut doc, page_id, matrix)?;
        let page = doc.get_object_mut(page_id)?.as_dict_mut()?;
        page.set(
            "MediaBox",
            vec![Object::Real(0.0), Object::Real(0.0), Object::Real(target_w as f32), Object::Real(target_h as f32)],
        );
        page.remove(b"CropBox");
        page.remove(b"BleedBox");
        page.remove(b"TrimBox");
        page.remove(b"ArtBox");
    }

    let final_path = resolve_output_path(output, policy)?;
    save_document(&mut doc, &final_path, true)?;
    Ok(final_path)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CropItem {
    pub page: u32,
    /// Display-space rectangle (top-left origin, points), as reported by the
    /// crop UI overlay.
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Applies CropBox values (non-destructive visual cropping, as all PDF
/// editors do by default).
pub fn crop_pages(
    input: &Path,
    output: &Path,
    crops: &[CropItem],
    policy: OverwritePolicy,
    password: Option<&str>,
) -> PdfResult<PathBuf> {
    if crops.is_empty() {
        return Err(PdfError::InvalidInput("no crop areas given".into()));
    }
    let mut doc = load_document(input, password)?;
    let total = doc.get_pages().len() as u32;
    materialize_all_pages(&mut doc)?;
    for crop in crops {
        if crop.page == 0 || crop.page > total {
            return Err(PdfError::RangeOutOfBounds);
        }
        if crop.w < 1.0 || crop.h < 1.0 {
            return Err(PdfError::InvalidInput("crop area is too small".into()));
        }
        let page_id = doc.get_pages().get(&crop.page).copied().ok_or(PdfError::RangeOutOfBounds)?;
        let rotation = page_rotation(&doc, page_id)?;
        let media = page_mediabox(&doc, page_id)?;
        let (page_w, page_h) = (media[2] - media[0], media[3] - media[1]);
        let (display_w, display_h) = Matrix::displayed_size(rotation, page_w, page_h);
        // UI top-left -> display bottom-left
        let display_rect =
            [crop.x.max(0.0), (display_h - crop.y - crop.h).max(0.0), crop.w.min(display_w), crop.h.min(display_h)];
        let page_rect = display_rect_to_page_rect(rotation, page_w, page_h, display_rect);
        let page = doc.get_object_mut(page_id)?.as_dict_mut()?;
        page.set(
            "CropBox",
            vec![
                Object::Real(page_rect[0] as f32),
                Object::Real(page_rect[1] as f32),
                Object::Real((page_rect[0] + page_rect[2]) as f32),
                Object::Real((page_rect[1] + page_rect[3]) as f32),
            ],
        );
    }
    let final_path = resolve_output_path(output, policy)?;
    save_document(&mut doc, &final_path, true)?;
    Ok(final_path)
}

/// Utility for tests: reads the MediaBox/rotation of a page.
pub fn read_page_box(path: &Path, page_number: u32) -> PdfResult<(f64, f64, i32)> {
    let doc = Document::load(path).map_err(|e| PdfError::from_lopdf(e, Some(path)))?;
    let page_id = doc.get_pages().get(&page_number).copied().ok_or(PdfError::RangeOutOfBounds)?;
    let media = page_mediabox(&doc, page_id)?;
    let rotation = page_rotation(&doc, page_id)?;
    Ok((media[2] - media[0], media[3] - media[1], rotation))
}
