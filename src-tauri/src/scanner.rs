//! Document scanner commands (Android camera path): the corner picker's
//! preview, the on-device page detector (C12) and the multi-page PDF build
//! with its colour filters.
//!
//! Everything here is local-first: no network access, no telemetry, and no
//! page photo leaves the machine.

use crate::office::{input_path, output_path, payload, OfficeErrorPayload};
use officecore::error::OfficeError;
use serde::{Deserialize, Serialize};

/// One captured page: an image file plus the four page corners the user tapped
/// (in source pixels). Without corners the image is used as it is.
#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanPage {
    pub path: String,
    #[serde(default)]
    #[cfg_attr(feature = "bindings", specta(type = Option<Vec<[specta_typescript::Number; 2]>>))]
    pub corners: Option<Vec<[f32; 2]>>,
    /// Colour, greyscale or black-and-white output for this page.
    #[serde(default)]
    pub filter: Option<ScanFilter>,
}

/// Post-processing for a corrected page.
#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ScanFilter {
    #[default]
    Colour,
    Greyscale,
    BlackWhite,
}

impl From<ScanFilter> for pdfcore::page_detect::ScanFilter {
    fn from(filter: ScanFilter) -> Self {
        match filter {
            ScanFilter::Colour => Self::Colour,
            ScanFilter::Greyscale => Self::Greyscale,
            ScanFilter::BlackWhite => Self::BlackWhite,
        }
    }
}

#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanRequest {
    pub pages: Vec<ScanPage>,
    pub output: String,
}

#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub output: String,
    #[cfg_attr(feature = "bindings", specta(type = specta_typescript::Number))]
    pub pages: usize,
    #[cfg_attr(feature = "bindings", specta(type = specta_typescript::Number))]
    pub corrected: usize,
    pub warnings: Vec<String>,
}

/// The corner picker's view of one page photo (see `pdfcore::scan::preview`).
#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanPreview {
    /// A small upright JPEG as a `data:` URL.
    pub data_url: String,
    /// Upright full-size width and height: the corners are sent in these pixels.
    pub width: u32,
    pub height: u32,
}

/// The automatically detected page in one photo (C12). Empty `corners` means
/// the detector found nothing and the corner picker stays in charge.
#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanDetection {
    pub detected: bool,
    /// `[top-left, top-right, bottom-right, bottom-left]` in upright pixels.
    #[cfg_attr(feature = "bindings", specta(type = Vec<[specta_typescript::Number; 2]>))]
    pub corners: Vec<[f32; 2]>,
    /// 0..1; area and edge support combined.
    pub score: f32,
}

/// Longest side of the corner picker's preview.
const SCAN_PREVIEW_SIDE: u32 = 1280;

/// A small upright preview of a page photo for the corner picker. The photo
/// itself can be 12 MP and carry its rotation in EXIF; the preview is turned
/// the same way the page will be, so the picked corners line up.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn office_scan_preview(path: String) -> Result<ScanPreview, OfficeErrorPayload> {
    let task = tauri::async_runtime::spawn_blocking(move || scan_preview(&path));
    task.await.map_err(|error| payload(OfficeError::internal(format!("worker thread failed: {error}"))))?
}

fn scan_preview(path: &str) -> Result<ScanPreview, OfficeErrorPayload> {
    use base64::Engine;
    let source = input_path(path)?;
    let preview = pdfcore::scan::preview(&source, SCAN_PREVIEW_SIDE)
        .map_err(|error| payload(OfficeError::invalid(error.to_string())))?;
    let data_url =
        format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(&preview.jpeg));
    Ok(ScanPreview { data_url, width: preview.width, height: preview.height })
}

/// Finds the page's four corners on the device (C12): a downscaled
/// grayscale -> Gaussian -> Canny -> Hough pipeline plus a bright-region
/// contour candidate, scored by area and edge support. No network call; the
/// corner picker stays as the fallback when nothing is found.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn office_scan_detect(path: String) -> Result<ScanDetection, OfficeErrorPayload> {
    let task = tauri::async_runtime::spawn_blocking(move || scan_detect(&path));
    task.await.map_err(|error| payload(OfficeError::internal(format!("worker thread failed: {error}"))))?
}

fn scan_detect(path: &str) -> Result<ScanDetection, OfficeErrorPayload> {
    let source = input_path(path)?;
    let detection =
        pdfcore::page_detect::detect_file(&source).map_err(|error| payload(OfficeError::invalid(error.to_string())))?;
    Ok(match detection {
        Some(page) => ScanDetection {
            detected: true,
            corners: page.corners.map(|corner| [corner[0], corner[1]]).to_vec(),
            score: page.score,
        },
        None => ScanDetection { detected: false, corners: Vec::new(), score: 0.0 },
    })
}

/// Turns each captured page upright, corrects its perspective when it has
/// corners and assembles a multi-page PDF through the existing image-to-PDF
/// writer. The photos come from the system camera (Android) or a file pick.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn office_scan_pages(request: ScanRequest) -> Result<ScanResult, OfficeErrorPayload> {
    let _permit = crate::concurrency::acquire().await;
    let task = tauri::async_runtime::spawn_blocking(move || scan_pages(request));
    task.await.map_err(|error| payload(OfficeError::internal(format!("worker thread failed: {error}"))))?
}

fn scan_pages(request: ScanRequest) -> Result<ScanResult, OfficeErrorPayload> {
    if request.pages.is_empty() {
        return Err(payload(OfficeError::invalid("Select at least one page image.")));
    }
    let output = output_path(&request.output)?;
    let temp = tempfile::tempdir().map_err(|error| payload(OfficeError::internal(error.to_string())))?;
    let mut items = Vec::with_capacity(request.pages.len());
    let mut warnings = Vec::new();
    let mut corrected = 0usize;
    for (index, page) in request.pages.iter().enumerate() {
        let source = input_path(&page.path)?;
        let quad = page.corners.as_ref().filter(|corners| corners.len() == 4).map(|corners| {
            [
                [corners[0][0], corners[0][1]],
                [corners[1][0], corners[1][1]],
                [corners[2][0], corners[2][1]],
                [corners[3][0], corners[3][1]],
            ]
        });
        // Every page is turned upright first (camera photos carry their
        // rotation in EXIF), then flattened when it has corners.
        let path = temp.path().join(format!("page-{index}.jpg"));
        let filter = page.filter.unwrap_or_default().into();
        let target = match pdfcore::page_detect::prepare_page_filtered(&source, quad, filter, &path) {
            Ok(true) => {
                if quad.is_some() {
                    corrected += 1;
                }
                path
            }
            Ok(false) => source.clone(),
            Err(error) => {
                warnings.push(format!("Page {} was used as it is: {error}", index + 1));
                source.clone()
            }
        };
        items.push(pdfcore::images::ImageItem { path: target.to_string_lossy().to_string(), rotation_delta: 0 });
    }
    let silent = |_| ();
    let cancel = pdfcore::CancelToken::new();
    pdfcore::images::images_to_pdf(
        &items,
        &Default::default(),
        &output,
        pdfcore::docutil::OverwritePolicy::Replace,
        &silent,
        &cancel,
    )
    .map_err(|error| payload(OfficeError::internal(error.to_string())))?;
    Ok(ScanResult { output: output.to_string_lossy().to_string(), pages: items.len(), corrected, warnings })
}

#[cfg(test)]
mod scan_tests {
    use super::*;
    use std::path::Path;

    fn write_page(path: &Path, width: u32, height: u32) {
        let mut image = image::RgbaImage::new(width, height);
        for y in 0..height {
            for x in 0..width {
                let value = if (x / 8 + y / 8) % 2 == 0 { 40 } else { 220 };
                image.put_pixel(x, y, image::Rgba([value, value, value, 255]));
            }
        }
        image.save(path).expect("write page");
    }

    #[test]
    fn scan_corrects_corners_and_assembles_a_pdf() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("page1.png");
        let second = dir.path().join("page2.png");
        write_page(&first, 60, 80);
        write_page(&second, 60, 80);
        let output = dir.path().join("scan.pdf");

        // The first page is corrected from an offset quad, the second is used
        // as it is (no corners).
        let request = ScanRequest {
            pages: vec![
                ScanPage {
                    path: first.to_string_lossy().to_string(),
                    corners: Some(vec![[10.0, 10.0], [50.0, 10.0], [50.0, 70.0], [10.0, 70.0]]),
                    filter: None,
                },
                ScanPage { path: second.to_string_lossy().to_string(), corners: None, filter: None },
            ],
            output: output.to_string_lossy().to_string(),
        };
        let result = scan_pages(request).expect("scan");
        assert_eq!(result.pages, 2);
        assert_eq!(result.corrected, 1, "one page had corners");
        assert!(result.warnings.is_empty(), "warnings: {:?}", result.warnings);
        let bytes = std::fs::read(&output).expect("pdf written");
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn scan_preview_is_a_small_jpeg_with_the_full_size() {
        let dir = tempfile::tempdir().unwrap();
        let page = dir.path().join("page.png");
        write_page(&page, 2560, 1280);
        let preview = scan_preview(&page.to_string_lossy()).expect("preview");
        assert!(preview.data_url.starts_with("data:image/jpeg;base64,"));
        assert_eq!((preview.width, preview.height), (2560, 1280));
        assert!(scan_preview(&dir.path().join("gone.jpg").to_string_lossy()).is_err());
    }

    /// A photo-like image: dark desk plus a bright page quad.
    fn write_photo(path: &Path, quad: [[f32; 2]; 4]) {
        let (width, height) = (320u32, 240u32);
        let quad = pdfcore::scan::order_corners(quad);
        let inside = |point: [f32; 2]| {
            let mut sign = 0.0f32;
            for index in 0..4 {
                let a = quad[index];
                let b = quad[(index + 1) % 4];
                let cross = (b[0] - a[0]) * (point[1] - a[1]) - (b[1] - a[1]) * (point[0] - a[0]);
                let current = cross.signum();
                if sign == 0.0 {
                    sign = current;
                } else if current != sign {
                    return false;
                }
            }
            true
        };
        let mut image = image::RgbaImage::from_pixel(width, height, image::Rgba([64, 66, 72, 255]));
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            if inside([x as f32 + 0.5, y as f32 + 0.5]) {
                *pixel = image::Rgba([246, 243, 236, 255]);
            }
        }
        image.save(path).expect("write photo");
    }

    #[test]
    fn scan_detects_the_page_corners() {
        let dir = tempfile::tempdir().unwrap();
        let page = dir.path().join("photo.png");
        let truth = [[46.0, 30.0], [278.0, 38.0], [270.0, 208.0], [40.0, 200.0]];
        write_photo(&page, truth);
        let detection = scan_detect(&page.to_string_lossy()).expect("detect");
        assert!(detection.detected, "a page outline was found");
        assert_eq!(detection.corners.len(), 4);
        assert!(detection.score > 0.0 && detection.score <= 1.0);
        for (found, expected) in detection.corners.iter().zip(truth.iter()) {
            let dx = found[0] - expected[0];
            let dy = found[1] - expected[1];
            assert!((dx * dx + dy * dy).sqrt() <= 8.0, "corner {found:?} vs {expected:?}");
        }
    }

    #[test]
    fn scan_reports_no_page_on_a_blank_wall() {
        let dir = tempfile::tempdir().unwrap();
        let blank = dir.path().join("blank.png");
        image::RgbaImage::from_pixel(320, 240, image::Rgba([120, 120, 120, 255])).save(&blank).expect("write");
        let detection = scan_detect(&blank.to_string_lossy()).expect("detect");
        assert!(!detection.detected);
        assert!(detection.corners.is_empty());
    }

    #[test]
    fn a_black_and_white_page_goes_through_the_scan_path() {
        let dir = tempfile::tempdir().unwrap();
        let page = dir.path().join("page.png");
        write_page(&page, 80, 60);
        let output = dir.path().join("scan.pdf");
        let request = ScanRequest {
            pages: vec![ScanPage {
                path: page.to_string_lossy().to_string(),
                corners: Some(vec![[0.0, 0.0], [80.0, 0.0], [80.0, 60.0], [0.0, 60.0]]),
                filter: Some(ScanFilter::BlackWhite),
            }],
            output: output.to_string_lossy().to_string(),
        };
        let result = scan_pages(request).expect("scan");
        assert_eq!(result.corrected, 1);
        assert!(std::fs::read(&output).expect("pdf written").starts_with(b"%PDF"));
    }

    #[test]
    fn scan_rejects_an_empty_page_list() {
        let dir = tempfile::tempdir().unwrap();
        let request = ScanRequest { pages: Vec::new(), output: dir.path().join("x.pdf").to_string_lossy().to_string() };
        assert!(scan_pages(request).is_err());
    }
}
