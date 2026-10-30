//! Automatic page detection and colour filters for the document scanner (C12).
//!
//! The scanner used to ask for four corners on every photo. This module finds
//! them on the device instead: the photo is downscaled, turned into one
//! luminance channel, blurred and run through Canny edge detection; a Hough
//! transform turns the strongest edges into lines, two roughly parallel pairs
//! become a quadrilateral, and the candidate is scored by its area and by how
//! much of its perimeter really sits on an edge. An Otsu bright-region contour
//! is added as a second candidate, because a page border can be low-contrast
//! while the paper itself is bright. The best candidate is scaled back to the
//! full-size photo, so the perspective correction still runs on full
//! resolution pixels.
//!
//! The manual corner picker stays the fallback and the correction surface. No
//! network call is made anywhere here, and `imageproc` is a pure-Rust crate
//! like `image`, so the same pipeline runs on Android.

use crate::error::PdfResult;
use crate::scan::Point;
use image::{DynamicImage, GrayImage, RgbaImage};
use std::path::Path;

mod filter;
mod lines;
#[cfg(test)]
mod tests;

pub use filter::apply_filter;

use lines::{quad_candidates_from_lines, quad_from_bright_regions};

/// Longest side the detector works on. A 12 MP photo is downscaled to this
/// before the edge work; the found corners are scaled back afterwards.
const DETECT_MAX_SIDE: u32 = 640;

/// Gaussian sigma applied before Canny, in downscaled pixels.
const BLUR_SIGMA: f32 = 1.4;

/// A useful page fills at least this share of the photo.
const MIN_AREA_FRACTION: f32 = 0.12;

/// A quad covering (almost) everything is the photo itself, not a page.
const MAX_AREA_FRACTION: f32 = 0.98;

/// Share of the perimeter that must lie on a detected edge.
const MIN_EDGE_SUPPORT: f32 = 0.42;

/// Perimeter samples per side for the edge-support score.
const SAMPLES_PER_SIDE: usize = 32;

/// Corner format used by the scanner: ordered top-left, top-right,
/// bottom-right, bottom-left, in pixels of the upright photo.
pub type Quad = [Point; 4];

/// What the detector proposes for one photo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageDetection {
    /// The four corners in pixels of the upright full-size photo.
    pub corners: Quad,
    /// 0..1; area and edge support combined. Higher is a better fit.
    pub score: f32,
}

/// Post-processing applied to a corrected page.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScanFilter {
    /// Keep the photo colours (the default).
    #[default]
    Colour,
    /// One luminance channel.
    Greyscale,
    /// Adaptive black and white, for text pages shot under uneven light.
    BlackWhite,
}

impl ScanFilter {
    /// True when the filter leaves the pixels alone.
    pub fn is_plain(self) -> bool {
        self == ScanFilter::Colour
    }
}

/// Finds the page in a photo. Returns `None` when no quadrilateral is
/// convincing enough; the caller then keeps the manual corner picker.
pub fn detect_page(image: &RgbaImage) -> Option<PageDetection> {
    let (width, height) = image.dimensions();
    if width < 96 || height < 96 {
        return None;
    }
    let small = downscale(image);
    let (small_width, small_height) = small.dimensions();
    let luma = DynamicImage::ImageRgba8(small.clone()).to_luma8();
    let blurred = imageproc::filter::gaussian_blur_f32(&luma, BLUR_SIGMA);
    let edges = canny_edges(&blurred);
    if edges.iter().filter(|pixel| **pixel > 0).count() < 32 {
        return None;
    }
    let mut best: Option<(Quad, f32)> = None;
    let mut consider = |candidate: Quad| {
        if let Some((quad, score)) = evaluate(candidate, &edges) {
            if best.is_none_or(|(_, current)| score > current) {
                best = Some((quad, score));
            }
        }
    };
    for candidate in quad_candidates_from_lines(&edges) {
        consider(candidate);
    }
    if let Some(candidate) = quad_from_bright_regions(&luma) {
        consider(candidate);
    }
    let (quad, score) = best?;
    let scale_x = width as f32 / small_width as f32;
    let scale_y = height as f32 / small_height as f32;
    let corners = crate::scan::order_corners(quad.map(|point| [point[0] * scale_x, point[1] * scale_y]));
    Some(PageDetection { corners, score })
}

/// Finds the page by reading `input` the way the viewer shows it (EXIF turned
/// upright), so the corners line up with `scan::prepare_page`.
pub fn detect_file(input: &Path) -> PdfResult<Option<PageDetection>> {
    let (image, _) = crate::scan::open_upright(input)?;
    Ok(detect_page(&image.to_rgba8()))
}

/// Turns a captured page upright, corrects its perspective when it has
/// corners and writes the filtered JPEG. The plain colour path delegates to
/// `scan::prepare_page`, so existing files are byte-for-byte unchanged.
pub fn prepare_page_filtered(
    input: &Path,
    corners: Option<Quad>,
    filter: ScanFilter,
    output: &Path,
) -> PdfResult<bool> {
    if filter.is_plain() {
        return crate::scan::prepare_page(input, corners, output);
    }
    let (image, turned) = crate::scan::open_upright(input)?;
    let page = match corners {
        Some(corners) => DynamicImage::ImageRgba8(crate::scan::correct_perspective(&image.to_rgba8(), corners)?),
        None if turned => image,
        None => return Ok(false),
    };
    let filtered = filter::apply_filter(&page.to_rgba8(), filter);
    crate::scan::write_jpeg(&DynamicImage::ImageRgba8(filtered), output, crate::scan::PAGE_JPEG_QUALITY)?;
    Ok(true)
}

// ---------------------------------------------------------------------------
// The pipeline
// ---------------------------------------------------------------------------

fn downscale(image: &RgbaImage) -> RgbaImage {
    let (width, height) = image.dimensions();
    if width.max(height) <= DETECT_MAX_SIDE {
        return image.clone();
    }
    let scale = DETECT_MAX_SIDE as f32 / width.max(height) as f32;
    let target_width = ((width as f32 * scale).round() as u32).max(1);
    let target_height = ((height as f32 * scale).round() as u32).max(1);
    image::imageops::resize(image, target_width, target_height, image::imageops::FilterType::Triangle)
}

/// Canny with thresholds derived from the strongest gradient in the frame, so
/// a dim photo still gets an edge map and a bright one does not flood.
fn canny_edges(luma: &GrayImage) -> GrayImage {
    let gradients = imageproc::gradients::sobel_gradients(luma);
    let strongest = gradients.pixels().map(|pixel| pixel[0] as f32).fold(0.0, f32::max);
    let high = (strongest * 0.1).clamp(24.0, 200.0);
    imageproc::edges::canny(luma, high * 0.4, high)
}

/// Rejects anything that is not a sane page outline and scores the rest.
fn evaluate(candidate: Quad, edges: &GrayImage) -> Option<(Quad, f32)> {
    let quad = crate::scan::order_corners(candidate);
    if !is_convex(&quad) {
        return None;
    }
    let (width, height) = edges.dimensions();
    let fraction = quad_area(&quad) / (width as f32 * height as f32);
    if !(MIN_AREA_FRACTION..=MAX_AREA_FRACTION).contains(&fraction) {
        return None;
    }
    let margin = 12.0;
    let inside = quad.iter().all(|point| {
        point[0] >= -margin
            && point[1] >= -margin
            && point[0] <= width as f32 + margin
            && point[1] <= height as f32 + margin
    });
    if !inside {
        return None;
    }
    let shortest = (0..4).map(|side| distance(quad[side], quad[(side + 1) % 4])).fold(f32::INFINITY, f32::min);
    if shortest < 12.0 {
        return None;
    }
    let support = edge_support(&quad, edges);
    if support < MIN_EDGE_SUPPORT {
        return None;
    }
    let score = 0.55 * (fraction / 0.85).min(1.0) + 0.45 * support;
    Some((quad, score))
}

fn is_convex(quad: &Quad) -> bool {
    let mut sign = 0.0f32;
    for index in 0..4 {
        let a = quad[index];
        let b = quad[(index + 1) % 4];
        let c = quad[(index + 2) % 4];
        let cross = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
        if cross.abs() < 1.0 {
            return false;
        }
        let current = cross.signum();
        if sign == 0.0 {
            sign = current;
        } else if current != sign {
            return false;
        }
    }
    true
}

fn quad_area(quad: &Quad) -> f32 {
    let mut twice = 0.0f32;
    for index in 0..4 {
        let current = quad[index];
        let next = quad[(index + 1) % 4];
        twice += current[0] * next[1] - next[0] * current[1];
    }
    (twice * 0.5).abs()
}

fn distance(left: Point, right: Point) -> f32 {
    ((left[0] - right[0]).powi(2) + (left[1] - right[1]).powi(2)).sqrt()
}

fn edge_support(quad: &Quad, edges: &GrayImage) -> f32 {
    let (width, height) = edges.dimensions();
    let mut hits = 0usize;
    let mut total = 0usize;
    for side in 0..4 {
        let start = quad[side];
        let end = quad[(side + 1) % 4];
        for sample in 1..=SAMPLES_PER_SIDE {
            let fraction = sample as f32 / (SAMPLES_PER_SIDE as f32 + 1.0);
            let x = start[0] + (end[0] - start[0]) * fraction;
            let y = start[1] + (end[1] - start[1]) * fraction;
            total += 1;
            if edge_near(edges, x, y, width, height) {
                hits += 1;
            }
        }
    }
    if total == 0 {
        0.0
    } else {
        hits as f32 / total as f32
    }
}

fn edge_near(edges: &GrayImage, x: f32, y: f32, width: u32, height: u32) -> bool {
    const RADIUS: i32 = 2;
    let center_x = x.round() as i32;
    let center_y = y.round() as i32;
    for offset_y in -RADIUS..=RADIUS {
        for offset_x in -RADIUS..=RADIUS {
            let pixel_x = center_x + offset_x;
            let pixel_y = center_y + offset_y;
            if pixel_x >= 0
                && pixel_y >= 0
                && (pixel_x as u32) < width
                && (pixel_y as u32) < height
                && edges.get_pixel(pixel_x as u32, pixel_y as u32)[0] > 0
            {
                return true;
            }
        }
    }
    false
}
