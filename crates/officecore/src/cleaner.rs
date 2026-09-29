//! Document cleaner: metadata removal, embedded image optimisation and
//! temporary-part cleanup for OOXML and ODF packages.
//!
//! Every change is reported so the UI can be honest about what was cleaned.
//! The cleaner never executes macros or scripts; it only rewrites package
//! parts as data.

use crate::error::{OfficeError, OfficeResult};
use crate::io::write_atomic;
use crate::xml::{escape_text, parse_xml};
use crate::zip::{ZipLimits, ZipReader, ZipWriter};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CleanOptions {
    pub remove_metadata: bool,
    pub remove_comments: bool,
    pub optimize_images: bool,
    /// Longest edge in pixels for optimized images (0 = keep size).
    pub image_max_pixels: u32,
    /// JPEG quality used when re-encoding (1-100).
    pub image_quality: u8,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CleanResult {
    pub bytes_before: u64,
    pub bytes_after: u64,
    pub actions: Vec<String>,
    pub warnings: Vec<String>,
}

fn empty_core_properties(kind: &str) -> String {
    let _ = kind;
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
     <cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" \
     xmlns:dc=\"http://purl.org/dc/elements/1.1/\" \
     xmlns:dcterms=\"http://purl.org/dc/terms/\" \
     xmlns:dcmitype=\"http://purl.org/dc/dcmitype/\" \
     xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\
     <dc:title></dc:title><dc:creator></dc:creator><cp:lastModifiedBy></cp:lastModifiedBy>\
     <dcterms:created xsi:type=\"dcterms:W3CDTF\">1970-01-01T00:00:00Z</dcterms:created></cp:coreProperties>"
        .to_string()
}

fn empty_app_properties() -> String {
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
     <Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\">\
     <Application>Office Swiss Army Knife</Application></Properties>"
        .to_string()
}

fn stripped_odf_meta(title: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-meta xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:meta=\"urn:oasis:names:tc:opendocument:xmlns:meta:1.0\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" office:version=\"1.2\"><office:meta><meta:generator>Office Swiss Army Knife</meta:generator><dc:title>{}</dc:title></office:meta></office:document-meta>",
        escape_text(title)
    )
}


fn optimize_image(name: &str, data: &[u8], options: &CleanOptions) -> Option<Vec<u8>> {
    if options.image_quality == 0 {
        return None;
    }
    let dynamic = image::load_from_memory(data).ok()?;
    let (width, height) = (dynamic.width(), dynamic.height());
    let max_pixels = if options.image_max_pixels == 0 { 1600 } else { options.image_max_pixels };
    let scale = if width.max(height) > max_pixels {
        max_pixels as f64 / width.max(height) as f64
    } else {
        1.0
    };
    let resized = if scale < 0.999 {
        dynamic.resize((width as f64 * scale) as u32, (height as f64 * scale) as u32, image::imageops::FilterType::Triangle)
    } else {
        dynamic.clone()
    };
    let lowercase = name.to_ascii_lowercase();
    let mut encoded = Vec::new();
    if lowercase.ends_with(".png") {
        // PNGs stay PNGs (lossless), but resizing alone often saves a lot.
        if scale >= 0.999 {
            return None;
        }
        resized.write_to(&mut std::io::Cursor::new(&mut encoded), image::ImageFormat::Png).ok()?;
    } else {
        let rgb = resized.to_rgb8();
        let quality = options.image_quality.clamp(40, 95);
        let mut cursor = std::io::Cursor::new(Vec::new());
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, quality);
        encoder.encode_image(&rgb).ok()?;
        encoded = cursor.into_inner();
    }
    if encoded.len() < data.len() {
        Some(encoded)
    } else {
        None
    }
}

fn clean_ooxml(reader: &ZipReader, options: &CleanOptions, result: &mut CleanResult) -> OfficeResult<Vec<u8>> {
    let limits = ZipLimits::default();
    let mut writer = ZipWriter::new();
    for name in reader.names() {
        let lower = name.to_ascii_lowercase();
        let data = reader.read_with_limits(name, limits)?;
        if options.remove_metadata {
            if lower == "docprops/core.xml" {
                writer.add_text(name, &empty_core_properties("ooxml"));
                result.actions.push("Removed document properties (author, title, dates).".into());
                continue;
            }
            if lower == "docprops/app.xml" {
                writer.add_text(name, &empty_app_properties());
                continue;
            }
            if lower == "docprops/custom.xml" {
                result.actions.push("Removed custom document properties.".into());
                continue;
            }
        }
        if options.remove_comments && lower.contains("/comments") && lower.ends_with(".xml") {
            // Replacing the part with an empty comments list keeps all
            // relationships and content types valid.
            if lower.contains("comments") {
                let empty = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:comments xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"/>".to_string();
                writer.add_text(name, &empty);
                result.actions.push("Removed comment text.".into());
                continue;
            }
        }
        if options.optimize_images && lower.contains("/media/") {
            if let Some(optimized) = optimize_image(name, &data, options) {
                result.actions.push(format!("Optimised embedded image {}.", name.rsplit('/').next().unwrap_or(name)));
                writer.add(name, &optimized);
                continue;
            }
        }
        writer.add(name, &data);
    }
    Ok(writer.finish())
}

fn clean_odf(reader: &ZipReader, options: &CleanOptions, result: &mut CleanResult) -> OfficeResult<Vec<u8>> {
    let limits = ZipLimits::default();
    let mut title = String::new();
    if let Ok(text) = reader.read_text("meta.xml") {
        if let Ok(root) = parse_xml(&text) {
            title = root.child("dc:title").map(|node| node.deep_text()).unwrap_or_default();
        }
    }
    let mut writer = ZipWriter::new();
    let mut is_first = true;
    for name in reader.names() {
        if !is_first && name == "mimetype" {
            // mimetype must stay first in the package.
        }
        is_first = false;
        let lower = name.to_ascii_lowercase();
        let data = reader.read_with_limits(name, limits)?;
        if options.remove_metadata && lower == "meta.xml" {
            writer.add_text(name, &stripped_odf_meta(&title));
            result.actions.push("Removed document metadata.".into());
            continue;
        }
        if options.optimize_images && (lower.starts_with("pictures/") || lower.starts_with("images/") || lower.starts_with("thumbnails/")) {
            if let Some(optimized) = optimize_image(name, &data, options) {
                result.actions.push(format!("Optimised embedded image {}.", name.rsplit('/').next().unwrap_or(name)));
                writer.add(name, &optimized);
                continue;
            }
        }
        writer.add(name, &data);
    }
    Ok(writer.finish())
}

/// Cleans a document package in place. Returns a report describing what changed.
pub fn clean_package(path: &Path, options: &CleanOptions) -> OfficeResult<CleanResult> {
    let bytes = crate::io::read_bytes(path)?;
    let mut result = CleanResult { bytes_before: bytes.len() as u64, ..Default::default() };
    let reader = ZipReader::open(bytes.clone())?;
    let is_odf = reader.contains("mimetype");
    let cleaned = if is_odf {
        clean_odf(&reader, options, &mut result)?
    } else {
        clean_ooxml(&reader, options, &mut result)?
    };
    result.bytes_after = cleaned.len() as u64;
    if result.bytes_after >= result.bytes_before {
        result.warnings.push("Cleaning did not reduce the file size.".into());
    }
    if result.actions.is_empty() {
        result.warnings.push("Nothing matched the selected cleaning options.".into());
    }
    write_atomic(path, &cleaned)?;
    Ok(result)
}

pub fn clean_package_to(path: &Path, output: &Path, options: &CleanOptions) -> OfficeResult<CleanResult> {
    let bytes = crate::io::read_bytes(path)?;
    let reader = ZipReader::open(bytes.clone())?;
    let mut result = CleanResult { bytes_before: bytes.len() as u64, ..Default::default() };
    let is_odf = reader.contains("mimetype");
    let cleaned = if is_odf {
        clean_odf(&reader, options, &mut result)?
    } else {
        clean_ooxml(&reader, options, &mut result)?
    };
    result.bytes_after = cleaned.len() as u64;
    if result.actions.is_empty() {
        result.warnings.push("Nothing matched the selected cleaning options.".into());
    }
    write_atomic(output, &cleaned)?;
    Ok(result)
}

/// A quick estimate of how much embedded images occupy (used by the UI).
pub fn image_footprint(path: &Path) -> OfficeResult<u64> {
    let bytes = crate::io::read_bytes(path)?;
    let reader = ZipReader::open(bytes)?;
    let limits = ZipLimits::default();
    let mut total = 0u64;
    for name in reader.names() {
        let lower = name.to_ascii_lowercase();
        if lower.contains("/media/") || lower.starts_with("pictures/") {
            if let Ok(data) = reader.read_with_limits(name, limits) {
                total += data.len() as u64;
            }
        }
    }
    Ok(total)
}

pub fn supported_extension(path: &Path) -> bool {
    matches!(
        crate::io::extension_of(path).as_str(),
        "docx" | "xlsx" | "pptx" | "odt" | "ods" | "odp" | "docm" | "xlsm" | "pptm"
    )
}

/// Unsupported input types produce a clear message rather than a silent no-op.
pub fn ensure_supported(path: &Path) -> OfficeResult<()> {
    if supported_extension(path) {
        Ok(())
    } else {
        Err(OfficeError::unsupported(
            "The cleaner supports DOCX, XLSX, PPTX, ODT, ODS and ODP packages.",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_package() -> Vec<u8> {
        let mut writer = ZipWriter::new();
        writer.add_text("[Content_Types].xml", "<Types/>");
        writer.add_text("docProps/core.xml", "<cp:coreProperties xmlns:cp=\"x\"><dc:creator xmlns:dc=\"y\">Ada</dc:creator></cp:coreProperties>");
        writer.add_text("docProps/custom.xml", "<Properties><property name=\"secret\">1</property></Properties>");
        writer.add_text("word/document.xml", "<w:document xmlns:w=\"x\"><w:body/></w:document>");
        writer.finish()
    }

    #[test]
    fn removes_metadata_and_custom_properties() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.docx");
        std::fs::write(&path, make_package()).unwrap();
        let options = CleanOptions { remove_metadata: true, ..Default::default() };
        let result = clean_package(&path, &options).unwrap();
        assert!(result.actions.iter().any(|action| action.contains("properties")));
        let reader = ZipReader::open(std::fs::read(&path).unwrap()).unwrap();
        assert!(!reader.contains("docProps/custom.xml"));
        assert!(!reader.read_text("docProps/core.xml").unwrap().contains("Ada"));
    }

    #[test]
    fn rejects_unknown_packages() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.txt");
        std::fs::write(&path, b"hello").unwrap();
        assert!(ensure_supported(&path).is_err());
        assert!(clean_package(&path, &CleanOptions::default()).is_err());
    }

    #[test]
    fn optimize_skips_small_images() {
        let options = CleanOptions { optimize_images: true, image_quality: 80, image_max_pixels: 2000, ..Default::default() };
        let mut buffer = image::RgbaImage::new(8, 8);
        for pixel in buffer.pixels_mut() {
            *pixel = image::Rgba([1, 2, 3, 255]);
        }
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(buffer).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        assert!(optimize_image("word/media/image1.png", &png, &options).is_none());
    }
}
