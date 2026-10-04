//! Document information and statistics.

use crate::docutil::*;
use crate::error::{PdfError, PdfResult};
use crate::metadata::{read_metadata, PdfMetadata};
use crate::render::{self, PageGeometry};
use lopdf::Document;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfInfo {
    pub path: String,
    pub file_name: String,
    pub file_size_bytes: u64,
    pub page_count: u32,
    pub pdf_version: String,
    pub encrypted: bool,
    pub has_text_layer: bool,
    pub metadata: PdfMetadata,
    pub page_geometries: Vec<PageGeometry>,
    pub image_count: u32,
    pub title: String,
    pub author: String,
    pub producer: String,
}

/// Unique page sizes with counts, as a human-friendly summary.
#[derive(Debug, Clone, Serialize)]
pub struct PageSizeSummary {
    pub width_pt: f64,
    pub height_pt: f64,
    pub count: u32,
}

pub fn summarize_sizes(geometries: &[PageGeometry]) -> Vec<PageSizeSummary> {
    let mut out: Vec<PageSizeSummary> = Vec::new();
    for geometry in geometries {
        let w = (geometry.display_width_pt * 10.0).round() / 10.0;
        let h = (geometry.display_height_pt * 10.0).round() / 10.0;
        if let Some(existing) = out.iter_mut().find(|s| (s.width_pt - w).abs() < 1.0 && (s.height_pt - h).abs() < 1.0) {
            existing.count += 1;
        } else {
            out.push(PageSizeSummary { width_pt: w, height_pt: h, count: 1 });
        }
    }
    out
}

/// Points to millimeters helper for the UI.
pub fn points_to_mm(points: f64) -> f64 {
    points * 25.4 / 72.0
}

fn count_images(doc: &Document, max_pages: u32) -> u32 {
    let mut count = 0u32;
    for (index, (_, page_id)) in doc.get_pages().iter().enumerate() {
        if index as u32 >= max_pages {
            break;
        }
        if let Ok(images) = doc.get_page_images(*page_id) {
            count += images.len() as u32;
        }
    }
    count
}

pub fn pdf_info(path: &Path, password: Option<&str>) -> PdfResult<PdfInfo> {
    if !path.exists() {
        return Err(PdfError::NotFound(path.display().to_string()));
    }
    let file_size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let encrypted = is_encrypted_document(path);

    // Structural info via lopdf (needs a password when encrypted).
    let doc = load_document(path, password);
    let (pdf_version, metadata, page_count, image_count) = match doc {
        Ok(ref doc) => (doc.version.clone(), read_metadata(doc), doc.get_pages().len() as u32, count_images(doc, 50)),
        Err(PdfError::PasswordRequired) | Err(PdfError::WrongPassword) => {
            (String::from("unknown"), PdfMetadata::default(), 0, 0)
        }
        Err(err) => return Err(err),
    };

    // Geometry + text layer via pdfium (degrades gracefully).
    let (page_geometries, has_text_layer) = match render::page_geometries(path, password) {
        Ok(geometries) => {
            let has_text = render::document_has_text(path, password, 5).unwrap_or(false);
            (geometries, has_text)
        }
        Err(_) => (Vec::new(), false),
    };
    let page_count = if page_count == 0 { page_geometries.len() as u32 } else { page_count };

    Ok(PdfInfo {
        path: path.display().to_string(),
        file_name: path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
        file_size_bytes,
        page_count,
        pdf_version,
        encrypted,
        has_text_layer,
        title: metadata.title.clone(),
        author: metadata.author.clone(),
        producer: metadata.producer.clone(),
        metadata,
        page_geometries,
        image_count,
    })
}
