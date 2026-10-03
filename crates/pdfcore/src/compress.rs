//! PDF compression.
//!
//! Two strategies are offered:
//! * `lossless` - structural optimization: recompress streams, drop unused
//!   objects, optionally strip metadata. Text stays text.
//! * `raster`   - render every page at a target DPI and rebuild the document
//!   from JPEG images. This is the mode that reaches the big reductions on
//!   scanned documents; text becomes part of the image (the UI states this
//!   clearly and the estimate is computed from real sample pages).

use crate::docutil::*;
use crate::error::{PdfError, PdfResult};
use crate::images::{self, ImageFormat};
use crate::metadata::remove_metadata;
use crate::progress::{CancelToken, ProgressCallback, ProgressEvent};
use crate::render::{self, RenderOptions};
use lopdf::{dictionary, Document, Object};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressOptions {
    /// lossless | raster
    #[serde(default = "default_strategy")]
    pub strategy: String,
    /// low | medium | high | custom (only for raster)
    #[serde(default = "default_preset")]
    pub preset: String,
    #[serde(default = "default_dpi")]
    pub dpi: u32,
    #[serde(default = "default_quality")]
    pub jpeg_quality: u8,
    #[serde(default)]
    pub grayscale: bool,
    #[serde(default = "default_true")]
    pub remove_metadata: bool,
}

fn default_strategy() -> String {
    "raster".into()
}
fn default_preset() -> String {
    "medium".into()
}
fn default_dpi() -> u32 {
    150
}
fn default_quality() -> u8 {
    60
}
fn default_true() -> bool {
    true
}

impl Default for CompressOptions {
    fn default() -> Self {
        Self {
            strategy: default_strategy(),
            preset: default_preset(),
            dpi: default_dpi(),
            jpeg_quality: default_quality(),
            grayscale: false,
            remove_metadata: true,
        }
    }
}

impl CompressOptions {
    /// Resolves the effective (dpi, quality) for the selected preset.
    pub fn effective(&self) -> (u32, u8, bool) {
        match self.preset.to_lowercase().as_str() {
            "low" => (200, 78, self.grayscale),
            "medium" => (150, 60, self.grayscale),
            "high" => (110, 45, self.grayscale),
            _ => (self.dpi.clamp(36, 600), self.jpeg_quality.clamp(1, 100), self.grayscale),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CompressEstimate {
    pub original_bytes: u64,
    pub estimated_bytes: u64,
    pub page_count: u32,
    /// Fraction of bytes removed (0.0 - 1.0)
    pub reduction: f64,
    pub method: String,
    pub sample_pages: u32,
    pub accurate: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompressResult {
    pub path: String,
    pub original_bytes: u64,
    pub output_bytes: u64,
    pub reduction: f64,
}

fn file_len(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// Estimates the output size. For raster mode real pages are rendered and
/// encoded at the requested settings, then extrapolated - no fake numbers.
pub fn estimate_compression(
    input: &Path,
    options: &CompressOptions,
    password: Option<&str>,
) -> PdfResult<CompressEstimate> {
    let original_bytes = file_len(input);
    if options.strategy.eq_ignore_ascii_case("lossless") {
        // The lossless pass is cheap enough to actually run in memory.
        let mut doc = load_document(input, password)?;
        if options.remove_metadata {
            remove_metadata(&mut doc)?;
        }
        let mut buffer = Vec::new();
        doc.compress();
        doc.prune_objects();
        doc.renumber_objects();
        doc.save_to(&mut buffer).map_err(PdfError::from_io)?;
        let estimated = buffer.len() as u64;
        return Ok(CompressEstimate {
            original_bytes,
            estimated_bytes: estimated,
            page_count: doc.get_pages().len() as u32,
            reduction: reduction_of(original_bytes, estimated),
            method: "lossless".into(),
            sample_pages: 0,
            accurate: true,
        });
    }

    let geometries = render::page_geometries(input, password)?;
    let page_count = geometries.len() as u32;
    if page_count == 0 {
        return Err(PdfError::InvalidPdf("document has no pages".into()));
    }
    let (dpi, quality, grayscale) = options.effective();
    let sample_indices: Vec<u32> =
        if page_count <= 2 { (1..=page_count).collect() } else { vec![1, page_count / 2, page_count] };
    let render_options = RenderOptions { dpi: dpi as f32, max_width: Some(8000), max_height: Some(8000) };
    let mut total_sample_bytes: u64 = 0;
    for page in &sample_indices {
        let rendered = render::render_page(input, password, *page, &render_options)?;
        let encoded = images::encode_image(&rendered, ImageFormat::Jpeg, quality, grayscale)?;
        total_sample_bytes += encoded.len() as u64;
    }
    let avg = total_sample_bytes / sample_indices.len() as u64;
    // Rough per-page PDF overhead (xref, page dict, resources).
    let overhead_per_page: u64 = 900;
    let estimated = avg.saturating_mul(page_count as u64) + overhead_per_page * page_count as u64 + 4096;
    Ok(CompressEstimate {
        original_bytes,
        estimated_bytes: estimated,
        page_count,
        reduction: reduction_of(original_bytes, estimated),
        method: format!("raster@{dpi}dpi/q{quality}"),
        sample_pages: sample_indices.len() as u32,
        accurate: page_count <= 3,
    })
}

fn reduction_of(original: u64, estimated: u64) -> f64 {
    if original == 0 {
        return 0.0;
    }
    let reduction = 1.0 - (estimated as f64 / original as f64);
    reduction.clamp(-1.0, 1.0)
}

pub fn compress_pdf(
    input: &Path,
    output: &Path,
    options: &CompressOptions,
    policy: OverwritePolicy,
    password: Option<&str>,
    progress: &ProgressCallback,
    cancel: &CancelToken,
) -> PdfResult<CompressResult> {
    let original_bytes = file_len(input);
    if options.strategy.eq_ignore_ascii_case("lossless") {
        progress(ProgressEvent::new("compress.lossless", 0, 2));
        let mut doc = load_document(input, password)?;
        if options.remove_metadata {
            remove_metadata(&mut doc)?;
        }
        cancel.check()?;
        progress(ProgressEvent::new("compress.lossless", 1, 2));
        let final_path = resolve_output_path(output, policy)?;
        save_document(&mut doc, &final_path, true)?;
        progress(ProgressEvent::new("compress.lossless", 2, 2));
        let output_bytes = file_len(&final_path);
        return Ok(CompressResult {
            path: final_path.display().to_string(),
            original_bytes,
            output_bytes,
            reduction: reduction_of(original_bytes, output_bytes),
        });
    }

    let (dpi, quality, grayscale) = options.effective();
    let geometries = render::page_geometries(input, password)?;
    let page_count = geometries.len() as u32;
    if page_count == 0 {
        return Err(PdfError::InvalidPdf("document has no pages".into()));
    }
    let render_options = RenderOptions { dpi: dpi as f32, max_width: Some(8000), max_height: Some(8000) };

    let mut doc = Document::new();
    doc.version = "1.6".to_string();
    let mut page_ids = Vec::new();

    for (index, geometry) in geometries.iter().enumerate() {
        cancel.check()?;
        progress(ProgressEvent::new("compress.render", index as u64, page_count as u64));
        let rendered = render::render_page(input, password, geometry.page, &render_options)?;
        let jpeg = images::encode_image(&rendered, ImageFormat::Jpeg, quality, grayscale)?;
        let xobject_id = add_jpeg_image_xobject(&mut doc, rendered.width, rendered.height, jpeg, grayscale)?;
        let page_w = geometry.display_width_pt.max(1.0);
        let page_h = geometry.display_height_pt.max(1.0);
        let page_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![
                Object::Real(0.0),
                Object::Real(0.0),
                Object::Real(page_w as f32),
                Object::Real(page_h as f32),
            ],
            "Resources" => dictionary! {
                "XObject" => dictionary! {
                    "Im0" => Object::Reference(xobject_id),
                },
            },
        }));
        let content = format!("q\n{page_w:.4} 0 0 {page_h:.4} 0 0 cm\n/Im0 Do\nQ\n");
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
    if !options.remove_metadata {
        // Carry over the original Info dictionary.
        if let Ok(source) = load_document(input, password) {
            if let Ok(info_id) = source.trailer.get(b"Info").and_then(Object::as_reference) {
                if let Ok(info) = source.get_dictionary(info_id) {
                    let new_id = doc.add_object(Object::Dictionary(info.clone()));
                    doc.trailer.set("Info", Object::Reference(new_id));
                }
            }
        }
    } else {
        let info_id = doc.add_object(Object::Dictionary(dictionary! {
            "Producer" => pdf_text_object("PDF Swiss Army Knife"),
        }));
        doc.trailer.set("Info", Object::Reference(info_id));
    }
    doc.trailer.set("Size", Object::Integer((doc.max_id + 1) as i64));

    let final_path = resolve_output_path(output, policy)?;
    save_document(&mut doc, &final_path, true)?;
    progress(ProgressEvent::new("compress.render", page_count as u64, page_count as u64));
    let output_bytes = file_len(&final_path);
    Ok(CompressResult {
        path: final_path.display().to_string(),
        original_bytes,
        output_bytes,
        reduction: reduction_of(original_bytes, output_bytes),
    })
}

/// Preprocessing + rasterization helper shared with the OCR pipeline.
pub fn render_for_analysis(
    input: &Path,
    page: u32,
    dpi: u32,
    password: Option<&str>,
) -> PdfResult<crate::render::RenderedPage> {
    render::render_page(
        input,
        password,
        page,
        &RenderOptions { dpi: dpi as f32, max_width: Some(8000), max_height: Some(8000) },
    )
}
