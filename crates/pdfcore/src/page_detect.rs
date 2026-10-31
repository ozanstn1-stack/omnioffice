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

#[cfg(test)]
mod corpus_tests;
mod filter;
mod lines;
#[cfg(test)]
mod tests;

pub use filter::apply_filter;

use lines::{quad_candidates_from_lines, quad_from_bright_regions, spread_from_regions};

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
    let (quad, score, luma, small_width, small_height) = detect_small(image)?;
    let refined = refine_quad(quad, &imageproc::gradients::sobel_gradients(&luma));
    Some(scale_detection(image, refined, score, small_width, small_height))
}

/// Finds every page in a photo: one page normally, two when the photo is a
/// two-page spread (a book or a notebook captured open).
///
/// The split is only made when the detected outline is clearly landscape and a
/// darker spine runs down its middle; a landscape single page (a photo in
/// landscape orientation) keeps one page.
pub fn detect_pages(image: &RgbaImage) -> Vec<PageDetection> {
    let Some((quad, score, luma, small_width, small_height)) = detect_small(image) else {
        return Vec::new();
    };
    let magnitude = imageproc::gradients::sobel_gradients(&luma);
    let refine = |page: Quad| refine_quad(page, &magnitude);
    if let Some([left, right]) = split_spread(quad, &luma) {
        return vec![
            scale_detection(image, refine(left), score, small_width, small_height),
            scale_detection(image, refine(right), score, small_width, small_height),
        ];
    }
    // A dark spine disconnects the paper, so the single-outline path sees only
    // one page; two large portrait-ish regions side by side are a spread.
    if let Some([left, right]) = spread_from_regions(&luma) {
        return vec![
            scale_detection(image, refine(left), score, small_width, small_height),
            scale_detection(image, refine(right), score, small_width, small_height),
        ];
    }
    vec![scale_detection(image, refine(quad), score, small_width, small_height)]
}

/// The detector's work on the downscaled copy: the winning quad in downscaled
/// pixels, its score, the luminance copy and the copy's size.
fn detect_small(image: &RgbaImage) -> Option<(Quad, f32, GrayImage, u32, u32)> {
    let (width, height) = image.dimensions();
    if width < 96 || height < 96 {
        return None;
    }
    let small = downscale(image);
    let (small_width, small_height) = small.dimensions();
    let luma = DynamicImage::ImageRgba8(small).to_luma8();
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
    Some((quad, score, luma, small_width, small_height))
}

fn scale_detection(image: &RgbaImage, quad: Quad, score: f32, small_width: u32, small_height: u32) -> PageDetection {
    let (width, height) = image.dimensions();
    let scale_x = width as f32 / small_width as f32;
    let scale_y = height as f32 / small_height as f32;
    let corners = crate::scan::order_corners(quad.map(|point| [point[0] * scale_x, point[1] * scale_y]));
    PageDetection { corners, score }
}

/// The Sobel gradient-magnitude image used to snap an edge to the strongest
/// nearby gradient.
type MagnitudeImage = image::ImageBuffer<image::Luma<u16>, Vec<u16>>;

/// Snaps the four edges to the strongest gradient nearby and rebuilds the
/// corners from the refined lines. The bright-region path can land a few pixels
/// inside the paper (shading, vignetting); the edges themselves are sharp.
fn refine_quad(quad: Quad, magnitude: &MagnitudeImage) -> Quad {
    let quad = crate::scan::order_corners(quad);
    let mut edges: [Option<(Point, Point)>; 4] = [None; 4];
    for index in 0..4 {
        edges[index] = fit_edge(quad[index], quad[(index + 1) % 4], magnitude);
    }
    let mut refined = quad;
    for index in 0..4 {
        let previous = edges[(index + 3) % 4];
        let current = edges[index];
        if let (Some(first), Some(second)) = (previous, current) {
            if let Some(corner) = intersect_lines(first, second) {
                refined[index] = corner;
            }
        }
    }
    if is_convex(&refined) && quad_area(&refined) > 1.0 {
        refined
    } else {
        quad
    }
}

/// Fits a line through the strongest gradient sample along one side.
fn fit_edge(start: Point, end: Point, magnitude: &MagnitudeImage) -> Option<(Point, Point)> {
    let length = distance(start, end);
    if length < 8.0 {
        return None;
    }
    let direction = [(end[0] - start[0]) / length, (end[1] - start[1]) / length];
    let normal = [-direction[1], direction[0]];
    let (width, height) = magnitude.dimensions();
    let mut points: Vec<Point> = Vec::new();
    for sample in 0..=12 {
        let t = 0.08 + 0.84 * sample as f32 / 12.0;
        let base = [start[0] + (end[0] - start[0]) * t, start[1] + (end[1] - start[1]) * t];
        let mut best_offset = 0.0f32;
        let mut best_strength = 0.0f32;
        for offset in [-4.0f32, -3.0, -2.0, -1.0, 0.0, 1.0, 2.0, 3.0, 4.0] {
            let x = (base[0] + normal[0] * offset).round();
            let y = (base[1] + normal[1] * offset).round();
            if x < 0.0 || y < 0.0 || x >= width as f32 || y >= height as f32 {
                continue;
            }
            let strength = magnitude.get_pixel(x as u32, y as u32)[0] as f32;
            if strength > best_strength {
                best_strength = strength;
                best_offset = offset;
            }
        }
        if best_strength > 0.0 {
            points.push([base[0] + normal[0] * best_offset, base[1] + normal[1] * best_offset]);
        }
    }
    if points.len() < 4 {
        return None;
    }
    let count = points.len() as f32;
    let mean_x = points.iter().map(|point| point[0]).sum::<f32>() / count;
    let mean_y = points.iter().map(|point| point[1]).sum::<f32>() / count;
    let (mut xx, mut xy, mut yy) = (0.0f32, 0.0f32, 0.0f32);
    for point in &points {
        let dx = point[0] - mean_x;
        let dy = point[1] - mean_y;
        xx += dx * dx;
        xy += dx * dy;
        yy += dy * dy;
    }
    let angle = 0.5 * (2.0 * xy).atan2(xx - yy);
    let fitted = [angle.cos(), angle.sin()];
    Some(([mean_x, mean_y], [mean_x + fitted[0], mean_y + fitted[1]]))
}

fn intersect_lines(first: (Point, Point), second: (Point, Point)) -> Option<Point> {
    let (p1, p2) = first;
    let (p3, p4) = second;
    let d1 = [p2[0] - p1[0], p2[1] - p1[1]];
    let d2 = [p4[0] - p3[0], p4[1] - p3[1]];
    let determinant = d1[0] * d2[1] - d1[1] * d2[0];
    if determinant.abs() < 1e-4 {
        return None;
    }
    let t = ((p3[0] - p1[0]) * d2[1] - (p3[1] - p1[1]) * d2[0]) / determinant;
    Some([p1[0] + d1[0] * t, p1[1] + d1[1] * t])
}

/// Splits a detected outline into two page quads when it looks like a spread:
/// clearly landscape (width/height >= 1.25) and with a darker vertical strip
/// down its middle (the spine and its shadow).
fn split_spread(quad: Quad, luma: &GrayImage) -> Option<[Quad; 2]> {
    let quad = crate::scan::order_corners(quad);
    let [top_left, top_right, bottom_right, bottom_left] = quad;
    let width = distance(top_left, top_right).max(distance(bottom_left, bottom_right));
    let height = distance(top_left, bottom_left).max(distance(top_right, bottom_right));
    if height <= 1.0 || width / height < 1.25 {
        return None;
    }
    let center = mean_luma(quad, luma, 0.46, 0.54, 0.30, 0.70);
    let left_side = mean_luma(quad, luma, 0.33, 0.41, 0.30, 0.70);
    let right_side = mean_luma(quad, luma, 0.59, 0.67, 0.30, 0.70);
    let sides = (left_side + right_side) / 2.0;
    if center + 2.5 >= sides {
        return None;
    }
    let middle_top = midpoint(top_left, top_right);
    let middle_bottom = midpoint(bottom_left, bottom_right);
    Some([[top_left, middle_top, middle_bottom, bottom_left], [middle_top, top_right, bottom_right, middle_bottom]])
}

fn midpoint(left: Point, right: Point) -> Point {
    [(left[0] + right[0]) / 2.0, (left[1] + right[1]) / 2.0]
}

/// Mean luminance of a grid of bilinearly sampled points inside the quad,
/// between the given fractions of its width and height.
fn mean_luma(quad: Quad, luma: &GrayImage, left: f32, right: f32, top: f32, bottom: f32) -> f32 {
    let [top_left, top_right, bottom_right, bottom_left] = quad;
    let mut total: f32 = 0.0;
    let mut samples: f32 = 0.0;
    for row in 0..7 {
        let v = top + (bottom - top) * (row as f32 + 0.5) / 7.0;
        for column in 0..9 {
            let u = left + (right - left) * (column as f32 + 0.5) / 9.0;
            let x = top_left[0] * (1.0 - u) * (1.0 - v)
                + top_right[0] * u * (1.0 - v)
                + bottom_right[0] * u * v
                + bottom_left[0] * (1.0 - u) * v;
            let y = top_left[1] * (1.0 - u) * (1.0 - v)
                + top_right[1] * u * (1.0 - v)
                + bottom_right[1] * u * v
                + bottom_left[1] * (1.0 - u) * v;
            total += bilinear_luma(luma, x, y);
            samples += 1.0;
        }
    }
    total / samples.max(1.0)
}

fn bilinear_luma(luma: &GrayImage, x: f32, y: f32) -> f32 {
    let (width, height) = luma.dimensions();
    if width == 0 || height == 0 {
        return 0.0;
    }
    let x = x.clamp(0.0, (width.saturating_sub(1)) as f32);
    let y = y.clamp(0.0, (height.saturating_sub(1)) as f32);
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let top = luma.get_pixel(x0, y0)[0] as f32 * (1.0 - fx) + luma.get_pixel(x1, y0)[0] as f32 * fx;
    let bottom = luma.get_pixel(x0, y1)[0] as f32 * (1.0 - fx) + luma.get_pixel(x1, y1)[0] as f32 * fx;
    top * (1.0 - fy) + bottom * fy
}

/// Finds the page by reading `input` the way the viewer shows it (EXIF turned
/// upright), so the corners line up with `scan::prepare_page`.
pub fn detect_file(input: &Path) -> PdfResult<Option<PageDetection>> {
    let (image, _) = crate::scan::open_upright(input)?;
    Ok(detect_page(&image.to_rgba8()))
}

/// Finds the page or the two pages of a spread in one photo file (D4). The
/// batch auto-crop flow calls this once per captured photo.
pub fn detect_file_pages(input: &Path) -> PdfResult<Vec<PageDetection>> {
    let (image, _) = crate::scan::open_upright(input)?;
    Ok(detect_pages(&image.to_rgba8()))
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
