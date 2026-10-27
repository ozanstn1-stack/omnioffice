//! Document-scanner geometry: turn a photographed page (four corners in the
//! image) into a rectangular scan.
//!
//! The scanner takes page photos from the system camera app (Android) or a
//! file pick and lets the user drag the four corners of the page on an upright
//! preview; ML Kit's scanner was passed over because it brings Google's usage
//! logging along. The corner list ends up here: the quad is ordered, an output
//! size is derived from the edge lengths and a homography resamples the pixels
//! so the page comes out flat. Every page is first turned upright from its
//! EXIF orientation (`open_upright`).
//!
//! The homography is implemented directly (an 8×8 linear solve plus bilinear
//! sampling) instead of pulling in `imageproc`'s `warp_perspective`: it is the
//! same projection, it keeps the binary small, and it is fully unit-tested.
//! The corrected image then goes through the existing OCR preprocessing
//! (`crate::ocr::preprocess`) before tesseract.

use crate::error::{PdfError, PdfResult};
use image::codecs::jpeg::JpegEncoder;
use image::metadata::Orientation;
use image::{DynamicImage, GenericImageView, ImageDecoder, ImageReader, RgbaImage};
use std::path::Path;

/// A page corner in source-image pixels.
pub type Point = [f32; 2];

/// Orders four arbitrary corners into `[top-left, top-right, bottom-right,
/// bottom-left]`, which is what the homography expects.
pub fn order_corners(points: [Point; 4]) -> [Point; 4] {
    // Top-left has the smallest x+y, bottom-right the largest; top-right has
    // the smallest y-x, bottom-left the largest.
    let mut sorted = points;
    sorted.sort_by(|a, b| (a[0] + a[1]).partial_cmp(&(b[0] + b[1])).unwrap_or(std::cmp::Ordering::Equal));
    let top_left = sorted[0];
    let bottom_right = sorted[3];
    let remaining = [sorted[1], sorted[2]];
    let (top_right, bottom_left) = if remaining[0][1] - remaining[0][0] < remaining[1][1] - remaining[1][0] {
        (remaining[0], remaining[1])
    } else {
        (remaining[1], remaining[0])
    };
    [top_left, top_right, bottom_right, bottom_left]
}

fn distance(a: Point, b: Point) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// The output size for a quad: the longer pair of opposite edges, rounded up.
pub fn quad_size(quad: [Point; 4]) -> (u32, u32) {
    let [tl, tr, br, bl] = quad;
    let width = distance(tl, tr).max(distance(bl, br)).round().max(1.0);
    let height = distance(tl, bl).max(distance(tr, br)).round().max(1.0);
    // Guard against a degenerate quad producing a giant buffer.
    let width = width.min(10_000.0) as u32;
    let height = height.min(10_000.0) as u32;
    (width.max(1), height.max(1))
}

/// Solves `A x = b` for an 8×8 system with partial pivoting. Returns `None`
/// when the system is singular (a degenerate quad).
fn solve8(mut a: [[f64; 8]; 8], mut b: [f64; 8]) -> Option<[f64; 8]> {
    for column in 0..8 {
        let mut pivot = column;
        for row in column + 1..8 {
            if a[row][column].abs() > a[pivot][column].abs() {
                pivot = row;
            }
        }
        if a[pivot][column].abs() < 1e-9 {
            return None;
        }
        a.swap(column, pivot);
        b.swap(column, pivot);
        for row in column + 1..8 {
            let factor = a[row][column] / a[column][column];
            let pivot = a[column];
            for (offset, value) in a[row][column..].iter_mut().enumerate() {
                *value -= factor * pivot[column + offset];
            }
            b[row] -= factor * b[column];
        }
    }
    let mut x = [0.0f64; 8];
    for row in (0..8).rev() {
        let mut sum = b[row];
        for k in row + 1..8 {
            sum -= a[row][k] * x[k];
        }
        x[row] = sum / a[row][row];
    }
    Some(x)
}

/// The homography that maps the destination rectangle `(0,0)..(w,h)` onto the
/// source quad, so sampling is a forward lookup.
fn homography(quad: [Point; 4], width: u32, height: u32) -> Option<[f64; 8]> {
    let dst = [[0.0f32, 0.0], [width as f32, 0.0], [width as f32, height as f32], [0.0, height as f32]];
    let mut a = [[0.0f64; 8]; 8];
    let mut b = [0.0f64; 8];
    for i in 0..4 {
        let (x, y) = (dst[i][0] as f64, dst[i][1] as f64);
        let (u, v) = (quad[i][0] as f64, quad[i][1] as f64);
        a[i * 2] = [x, y, 1.0, 0.0, 0.0, 0.0, -x * u, -y * u];
        b[i * 2] = u;
        a[i * 2 + 1] = [0.0, 0.0, 0.0, x, y, 1.0, -x * v, -y * v];
        b[i * 2 + 1] = v;
    }
    solve8(a, b)
}

/// Resamples `image` so the quad becomes a `width × height` rectangle.
pub fn warp_perspective(image: &RgbaImage, quad: [Point; 4], width: u32, height: u32) -> PdfResult<RgbaImage> {
    let width = width.max(1);
    let height = height.max(1);
    let h = homography(quad, width, height)
        .ok_or_else(|| PdfError::ConversionFailed("the four corners do not form a page".into()))?;
    let (src_w, src_h) = (image.width() as f32, image.height() as f32);
    let mut out = RgbaImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let (xf, yf) = (x as f64, y as f64);
            let denominator = h[6] * xf + h[7] * yf + 1.0;
            if denominator.abs() < 1e-9 {
                continue;
            }
            let u = (h[0] * xf + h[1] * yf + h[2]) / denominator;
            let v = (h[3] * xf + h[4] * yf + h[5]) / denominator;
            let pixel = sample_bilinear(image, u as f32, v as f32, src_w, src_h);
            out.put_pixel(x, y, pixel);
        }
    }
    Ok(out)
}

/// Bilinear sample with edge clamping.
fn sample_bilinear(image: &RgbaImage, x: f32, y: f32, width: f32, height: f32) -> image::Rgba<u8> {
    let x = x.clamp(0.0, width - 1.0);
    let y = y.clamp(0.0, height - 1.0);
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(image.width().saturating_sub(1));
    let y1 = (y0 + 1).min(image.height().saturating_sub(1));
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let p00 = image.get_pixel(x0, y0).0;
    let p10 = image.get_pixel(x1, y0).0;
    let p01 = image.get_pixel(x0, y1).0;
    let p11 = image.get_pixel(x1, y1).0;
    let mut out = [0u8; 4];
    for channel in 0..4 {
        let top = p00[channel] as f32 * (1.0 - fx) + p10[channel] as f32 * fx;
        let bottom = p01[channel] as f32 * (1.0 - fx) + p11[channel] as f32 * fx;
        out[channel] = (top * (1.0 - fy) + bottom * fy).round().clamp(0.0, 255.0) as u8;
    }
    image::Rgba(out)
}

/// Orders the corners, derives the size and warps — the whole CameraX path.
pub fn correct_perspective(image: &RgbaImage, corners: [Point; 4]) -> PdfResult<RgbaImage> {
    let quad = order_corners(corners);
    let (width, height) = quad_size(quad);
    warp_perspective(image, quad, width, height)
}

/// Reads an image file, corrects its perspective and writes a PNG.
pub fn correct_file(input: &Path, corners: [Point; 4], output: &Path) -> PdfResult<()> {
    let (image, _) = open_upright(input)?;
    let corrected = correct_perspective(&image.to_rgba8(), corners)?;
    corrected
        .save_with_format(output, image::ImageFormat::Png)
        .map_err(|error| PdfError::ConversionFailed(format!("the corrected page could not be written: {error}")))?;
    Ok(())
}

fn unreadable(error: impl std::fmt::Display) -> PdfError {
    PdfError::ConversionFailed(format!("the page image could not be read: {error}"))
}

/// Reads an image the way a viewer shows it. A phone camera stores most
/// photos sideways and records the turn in the EXIF orientation tag, which
/// `image::open` ignores: a captured page came out on its side in the PDF, and
/// corners picked on the (upright) preview missed the page. Returns the image
/// and whether a turn or flip was applied.
pub fn open_upright(input: &Path) -> PdfResult<(DynamicImage, bool)> {
    let reader = ImageReader::open(input).map_err(unreadable)?.with_guessed_format().map_err(unreadable)?;
    let mut decoder = reader.into_decoder().map_err(unreadable)?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut image = DynamicImage::from_decoder(decoder).map_err(unreadable)?;
    let turned = orientation != Orientation::NoTransforms;
    if turned {
        image.apply_orientation(orientation);
    }
    Ok((image, turned))
}

/// JPEG quality of prepared pages: a 12 MP page photo stays well under a few
/// MB in the PDF instead of the tens of MB a lossless PNG would take.
const PAGE_JPEG_QUALITY: u8 = 88;

/// Prepares one captured page for the PDF: turned upright and, when corners
/// are given (in upright pixels), flattened. Writes a JPEG to `output` and
/// returns true; returns false, writing nothing, when the source can go into
/// the PDF as it is.
pub fn prepare_page(input: &Path, corners: Option<[Point; 4]>, output: &Path) -> PdfResult<bool> {
    let (image, turned) = open_upright(input)?;
    let page = match corners {
        Some(corners) => DynamicImage::ImageRgba8(correct_perspective(&image.to_rgba8(), corners)?),
        None if turned => image,
        None => return Ok(false),
    };
    write_jpeg(&page, output, PAGE_JPEG_QUALITY)?;
    Ok(true)
}

/// A downscaled, upright copy of a page photo for the corner picker.
pub struct PagePreview {
    /// JPEG bytes of the preview.
    pub jpeg: Vec<u8>,
    /// Width of the upright full-size image; corners are given in its pixels.
    pub width: u32,
    /// Height of the upright full-size image.
    pub height: u32,
}

/// Builds the corner picker's preview, its longest side at most `max_side`.
pub fn preview(input: &Path, max_side: u32) -> PdfResult<PagePreview> {
    let (image, _) = open_upright(input)?;
    let (width, height) = image.dimensions();
    let small = if width.max(height) > max_side { image.thumbnail(max_side, max_side) } else { image };
    let mut jpeg = Vec::new();
    small
        .to_rgb8()
        .write_with_encoder(JpegEncoder::new_with_quality(&mut jpeg, 80))
        .map_err(|error| PdfError::ConversionFailed(format!("the page preview could not be made: {error}")))?;
    Ok(PagePreview { jpeg, width, height })
}

fn write_jpeg(image: &DynamicImage, output: &Path, quality: u8) -> PdfResult<()> {
    let file = std::fs::File::create(output)
        .map_err(|error| PdfError::ConversionFailed(format!("the prepared page could not be written: {error}")))?;
    let mut writer = std::io::BufWriter::new(file);
    image
        .to_rgb8()
        .write_with_encoder(JpegEncoder::new_with_quality(&mut writer, quality))
        .map_err(|error| PdfError::ConversionFailed(format!("the prepared page could not be written: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checkerboard(width: u32, height: u32) -> RgbaImage {
        let mut image = RgbaImage::new(width, height);
        for y in 0..height {
            for x in 0..width {
                let value = if (x / 4 + y / 4) % 2 == 0 { 0 } else { 255 };
                image.put_pixel(x, y, image::Rgba([value, value, value, 255]));
            }
        }
        image
    }

    #[test]
    fn orders_corners_from_an_unordered_quad() {
        // The same rectangle given in a shuffled order.
        let corners = [[100.0, 100.0], [10.0, 10.0], [10.0, 100.0], [100.0, 10.0]];
        let ordered = order_corners(corners);
        assert_eq!(ordered[0], [10.0, 10.0], "top-left");
        assert_eq!(ordered[1], [100.0, 10.0], "top-right");
        assert_eq!(ordered[2], [100.0, 100.0], "bottom-right");
        assert_eq!(ordered[3], [10.0, 100.0], "bottom-left");
    }

    #[test]
    fn an_identity_quad_keeps_the_image() {
        let image = checkerboard(40, 30);
        let quad = [[0.0, 0.0], [40.0, 0.0], [40.0, 30.0], [0.0, 30.0]];
        let warped = warp_perspective(&image, quad, 40, 30).unwrap();
        assert_eq!(warped.dimensions(), (40, 30));
        // Corner pixels are preserved by an identity warp.
        assert_eq!(warped.get_pixel(0, 0), image.get_pixel(0, 0));
        assert_eq!(warped.get_pixel(39, 29), image.get_pixel(39, 29));
    }

    #[test]
    fn a_shifted_quad_lands_on_the_same_pixels() {
        // The quad is the image offset by (5,5); the output is the same patch.
        let image = checkerboard(40, 40);
        let quad = [[5.0, 5.0], [35.0, 5.0], [35.0, 35.0], [5.0, 35.0]];
        let warped = warp_perspective(&image, quad, 30, 30).unwrap();
        assert_eq!(warped.dimensions(), (30, 30));
        // The output's first pixel samples the source at (5,5).
        assert_eq!(warped.get_pixel(0, 0), image.get_pixel(5, 5));
    }

    #[test]
    fn quad_size_uses_the_longer_edges() {
        let quad = [[0.0, 0.0], [200.0, 0.0], [200.0, 100.0], [0.0, 100.0]];
        assert_eq!(quad_size(quad), (200, 100));
    }

    #[test]
    fn a_degenerate_quad_is_reported_not_panicked_on() {
        // All four corners identical: the homography is singular.
        let image = checkerboard(10, 10);
        let quad = [[1.0, 1.0], [1.0, 1.0], [1.0, 1.0], [1.0, 1.0]];
        assert!(warp_perspective(&image, quad, 10, 10).is_err());
    }

    #[test]
    fn correct_perspective_handles_a_shuffled_corner_list() {
        let image = checkerboard(40, 40);
        let corners = [[35.0, 35.0], [5.0, 5.0], [5.0, 35.0], [35.0, 5.0]];
        let corrected = correct_perspective(&image, corners).unwrap();
        assert_eq!(corrected.dimensions(), (30, 30));
    }

    /// A minimal big-endian EXIF block holding only the orientation tag.
    fn exif_orientation(value: u16) -> Vec<u8> {
        let mut exif = b"MM\0*\0\0\0\x08".to_vec();
        exif.extend_from_slice(&1u16.to_be_bytes()); // one IFD entry
        exif.extend_from_slice(&0x0112u16.to_be_bytes()); // Orientation
        exif.extend_from_slice(&3u16.to_be_bytes()); // SHORT
        exif.extend_from_slice(&1u32.to_be_bytes()); // count
        exif.extend_from_slice(&value.to_be_bytes());
        exif.extend_from_slice(&[0, 0]); // value padding
        exif.extend_from_slice(&0u32.to_be_bytes()); // no next IFD
        exif
    }

    /// Writes a `width`x`height` JPEG, optionally tagged with an EXIF orientation.
    fn photo(dir: &Path, name: &str, width: u32, height: u32, orientation: Option<u16>) -> std::path::PathBuf {
        use image::ImageEncoder;
        let path = dir.join(name);
        let pixels = image::RgbImage::from_pixel(width, height, image::Rgb([200, 200, 200]));
        let mut bytes = Vec::new();
        let mut encoder = JpegEncoder::new_with_quality(&mut bytes, 90);
        if let Some(value) = orientation {
            encoder.set_exif_metadata(exif_orientation(value)).expect("exif");
        }
        encoder.write_image(pixels.as_raw(), width, height, image::ExtendedColorType::Rgb8).expect("encode");
        std::fs::write(&path, bytes).expect("write");
        path
    }

    #[test]
    fn a_photo_stored_sideways_is_turned_upright() {
        // Phones save a portrait shot as a landscape JPEG tagged "rotate 90".
        let dir = tempfile::tempdir().unwrap();
        let path = photo(dir.path(), "page.jpg", 40, 20, Some(6));
        let (image, turned) = open_upright(&path).unwrap();
        assert!(turned);
        assert_eq!(image.dimensions(), (20, 40));
    }

    #[test]
    fn an_upright_page_without_corners_goes_in_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = photo(dir.path(), "page.jpg", 40, 20, None);
        let output = dir.path().join("out.jpg");
        assert!(!prepare_page(&path, None, &output).unwrap());
        assert!(!output.exists());
    }

    #[test]
    fn a_sideways_or_cropped_page_is_written_upright_as_jpeg() {
        let dir = tempfile::tempdir().unwrap();
        let sideways = photo(dir.path(), "side.jpg", 40, 20, Some(6));
        let output = dir.path().join("side-out.jpg");
        assert!(prepare_page(&sideways, None, &output).unwrap());
        assert_eq!(image::open(&output).unwrap().dimensions(), (20, 40));

        // Corners are given on the upright image (20 wide, 40 tall).
        let cropped = dir.path().join("crop-out.jpg");
        let corners = [[2.0, 4.0], [18.0, 4.0], [18.0, 36.0], [2.0, 36.0]];
        assert!(prepare_page(&sideways, Some(corners), &cropped).unwrap());
        assert_eq!(image::open(&cropped).unwrap().dimensions(), (16, 32));
        assert_eq!(&std::fs::read(&cropped).unwrap()[..2], &[0xFF, 0xD8], "JPEG, not PNG");
    }

    #[test]
    fn the_preview_is_small_and_reports_the_upright_size() {
        let dir = tempfile::tempdir().unwrap();
        let path = photo(dir.path(), "big.jpg", 1200, 600, Some(6));
        let preview = preview(&path, 300).unwrap();
        assert_eq!((preview.width, preview.height), (600, 1200));
        let small = image::load_from_memory(&preview.jpeg).unwrap();
        assert_eq!(small.dimensions(), (150, 300));
    }
}
