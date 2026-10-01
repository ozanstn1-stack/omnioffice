//! PDF -> images conversion (JPG / PNG at a chosen DPI).

use crate::docutil::{resolve_output_path, OverwritePolicy};
use crate::error::{PdfError, PdfResult};
use crate::images::{encode_image, ImageFormat};
use crate::progress::{CancelToken, ProgressCallback, ProgressEvent};
use crate::render::{self, RenderOptions};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct ImageOutput {
    pub path: String,
    pub page: u32,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfToImagesResult {
    pub files: Vec<ImageOutput>,
    pub total_bytes: u64,
    pub dpi: u32,
    pub format: String,
}

/// Converts the selected pages to image files named `<prefix>_001.jpg`.
#[allow(clippy::too_many_arguments)]
pub fn pdf_to_images(
    input: &Path,
    output_dir: &Path,
    format: ImageFormat,
    dpi: u32,
    jpeg_quality: u8,
    grayscale: bool,
    name_prefix: &str,
    pages: &[u32],
    policy: OverwritePolicy,
    password: Option<&str>,
    progress: &ProgressCallback,
    cancel: &CancelToken,
) -> PdfResult<PdfToImagesResult> {
    let geometries = render::page_geometries(input, password)?;
    let total = geometries.len() as u32;
    if total == 0 {
        return Err(PdfError::InvalidPdf("document has no pages".into()));
    }
    let target: Vec<u32> = if pages.is_empty() {
        (1..=total).collect()
    } else {
        for p in pages {
            if *p == 0 || *p > total {
                return Err(PdfError::RangeOutOfBounds);
            }
        }
        pages.to_vec()
    };
    if !output_dir.exists() {
        std::fs::create_dir_all(output_dir).map_err(PdfError::from_io)?;
    }
    let dpi = dpi.clamp(36, 600);
    let render_options = RenderOptions {
        dpi: dpi as f32,
        max_width: Some(20000),
        max_height: Some(20000),
    };
    let prefix = if name_prefix.trim().is_empty() {
        "page"
    } else {
        name_prefix.trim()
    };
    let mut files = Vec::with_capacity(target.len());
    let mut total_bytes = 0u64;
    for (index, page) in target.iter().enumerate() {
        cancel.check()?;
        progress(ProgressEvent::new("convert.page", index as u64, target.len() as u64));
        let rendered = render::render_page(input, password, *page, &render_options)?;
        let bytes = encode_image(&rendered, format, jpeg_quality, grayscale)?;
        let name = format!("{prefix}_{page:03}.{}", format.extension());
        let target_path = resolve_output_path(&output_dir.join(name), policy)?;
        // Atomic: an interrupted conversion must not truncate an image that is
        // already there under the Replace policy.
        crate::docutil::write_bytes_atomic(&target_path, &bytes)?;
        total_bytes += bytes.len() as u64;
        files.push(ImageOutput {
            path: target_path.display().to_string(),
            page: *page,
            width: rendered.width,
            height: rendered.height,
            bytes: bytes.len() as u64,
        });
    }
    progress(ProgressEvent::new("convert.page", target.len() as u64, target.len() as u64));
    Ok(PdfToImagesResult {
        files,
        total_bytes,
        dpi,
        format: format.extension().to_string(),
    })
}