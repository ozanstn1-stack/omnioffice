//! Colour filters for a corrected page: colour, greyscale and adaptive black
//! and white. The bilevel filter subtracts a local box mean, so uneven light
//! across the page does not turn one corner solid black.

use super::ScanFilter;
use image::{DynamicImage, GrayImage, Luma, RgbaImage};

/// Applies one of the scanner's filters to a corrected page.
pub fn apply_filter(image: &RgbaImage, filter: ScanFilter) -> RgbaImage {
    match filter {
        ScanFilter::Colour => image.clone(),
        ScanFilter::Greyscale => {
            let luma = DynamicImage::ImageRgba8(image.clone()).to_luma8();
            DynamicImage::ImageLuma8(luma).to_rgba8()
        }
        ScanFilter::BlackWhite => {
            let luma = DynamicImage::ImageRgba8(image.clone()).to_luma8();
            let bilevel = adaptive_bilevel(&luma);
            DynamicImage::ImageLuma8(bilevel).to_rgba8()
        }
    }
}

/// Local-mean threshold: a pixel becomes ink when it is clearly darker than
/// its neighbourhood, so a shadow gradient does not swallow the page.
fn adaptive_bilevel(luma: &GrayImage) -> GrayImage {
    let (width, height) = luma.dimensions();
    let (width, height) = (width as usize, height as usize);
    if width == 0 || height == 0 {
        return GrayImage::new(0, 0);
    }
    let stride = width + 1;
    let mut integral = vec![0u64; stride * (height + 1)];
    for y in 0..height {
        let mut row = 0u64;
        for x in 0..width {
            row += luma.get_pixel(x as u32, y as u32)[0] as u64;
            integral[(y + 1) * stride + (x + 1)] = integral[y * stride + (x + 1)] + row;
        }
    }
    const RADIUS: usize = 15;
    let mut output = GrayImage::new(width as u32, height as u32);
    for y in 0..height {
        for x in 0..width {
            let left = x.saturating_sub(RADIUS);
            let top = y.saturating_sub(RADIUS);
            let right = (x + RADIUS + 1).min(width);
            let bottom = (y + RADIUS + 1).min(height);
            let count = ((right - left) * (bottom - top)) as u64;
            let sum = integral[bottom * stride + right] + integral[top * stride + left]
                - integral[bottom * stride + left]
                - integral[top * stride + right];
            let mean = (sum / count) as i64;
            let value = luma.get_pixel(x as u32, y as u32)[0] as i64;
            let ink = value * 100 < mean * 92;
            output.put_pixel(x as u32, y as u32, Luma([if ink { 0u8 } else { 255u8 }]));
        }
    }
    output
}
