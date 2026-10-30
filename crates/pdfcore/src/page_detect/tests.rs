//! Tests for the automatic page detector and the scanner filters.
//!
//! The corpus is synthetic and deterministic: a bright paper quad on a grainy
//! desk, optionally rotated, shaded or scaled. That keeps the tests fast and
//! reproducible on every runner; a real-photo corpus is tracked in the roadmap
//! (C12) because the photos cannot be committed here.

use super::*;
use image::{DynamicImage, Rgba, RgbaImage};

fn inside(quad: &Quad, point: [f32; 2]) -> bool {
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
}

fn fill_quad(image: &mut RgbaImage, quad: Quad, colour: [u8; 3]) {
    let quad = crate::scan::order_corners(quad);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        if inside(&quad, [x as f32 + 0.5, y as f32 + 0.5]) {
            *pixel = Rgba([colour[0], colour[1], colour[2], 255]);
        }
    }
}

/// A page photo: grainy dark desk plus a bright paper quad.
fn page_photo(width: u32, height: u32, quad: Quad) -> RgbaImage {
    let mut image = RgbaImage::from_pixel(width, height, Rgba([72, 74, 80, 255]));
    let mut seed = 0x02F6_E2B1u32;
    for (_x, _y, pixel) in image.enumerate_pixels_mut() {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let grain = (seed >> 24) as i32 - 128;
        for channel in 0..3 {
            let value = pixel[channel] as i32 + grain / 10;
            pixel[channel] = value.clamp(0, 255) as u8;
        }
    }
    fill_quad(&mut image, quad, [246, 243, 236]);
    image
}

fn shade_left_side(image: &mut RgbaImage, strength: f32) {
    let width = image.width() as f32;
    for (x, _y, pixel) in image.enumerate_pixels_mut() {
        let factor = 1.0 - strength * (x as f32 / width);
        for channel in 0..3 {
            pixel[channel] = (pixel[channel] as f32 * factor).clamp(0.0, 255.0) as u8;
        }
    }
}

fn expect_page(detected: Option<PageDetection>, truth: Quad, tolerance: f32) {
    let detected = detected.expect("a page outline");
    assert!(detected.score > 0.0 && detected.score <= 1.0, "score {}", detected.score);
    for (actual, expected) in detected.corners.iter().zip(truth.iter()) {
        let error = distance(*actual, *expected);
        assert!(error <= tolerance, "corner {actual:?} is {error:.1} px from {expected:?}");
    }
}

#[test]
fn detects_a_perspective_page() {
    let truth: Quad = [[92.0, 58.0], [556.0, 74.0], [536.0, 418.0], [76.0, 402.0]];
    let image = page_photo(640, 480, truth);
    expect_page(detect_page(&image), truth, 8.0);
}

#[test]
fn detects_a_page_turned_on_the_desk() {
    let angle = 20.0f32.to_radians();
    let (sin, cos) = angle.sin_cos();
    let map = |x: f32, y: f32| [320.0 + x * cos - y * sin, 240.0 + x * sin + y * cos];
    let truth: Quad = [map(-220.0, -150.0), map(220.0, -150.0), map(220.0, 150.0), map(-220.0, 150.0)];
    let image = page_photo(640, 480, truth);
    expect_page(detect_page(&image), truth, 9.0);
}

#[test]
fn detects_a_page_under_uneven_light() {
    let truth: Quad = [[110.0, 70.0], [570.0, 90.0], [540.0, 420.0], [86.0, 396.0]];
    let mut image = page_photo(640, 480, truth);
    shade_left_side(&mut image, 0.45);
    expect_page(detect_page(&image), truth, 10.0);
}

#[test]
fn scales_the_corners_back_to_the_full_size_photo() {
    let truth: Quad = [[184.0, 116.0], [1112.0, 148.0], [1072.0, 836.0], [152.0, 804.0]];
    let image = page_photo(1280, 960, truth);
    expect_page(detect_page(&image), truth, 16.0);
}

#[test]
fn refuses_a_blank_wall() {
    let image = RgbaImage::from_pixel(640, 480, Rgba([128, 128, 128, 255]));
    assert!(detect_page(&image).is_none());
}

#[test]
fn refuses_a_page_that_fills_too_little_of_the_photo() {
    let truth: Quad = [[280.0, 200.0], [360.0, 200.0], [360.0, 280.0], [280.0, 280.0]];
    let image = page_photo(640, 480, truth);
    assert!(detect_page(&image).is_none(), "2 % of the frame is not a page");
}

#[test]
fn the_greyscale_filter_flattens_the_colour() {
    let image = RgbaImage::from_pixel(4, 4, Rgba([200, 40, 10, 255]));
    let grey = apply_filter(&image, ScanFilter::Greyscale);
    for pixel in grey.pixels() {
        assert_eq!(pixel[0], pixel[1]);
        assert_eq!(pixel[1], pixel[2]);
    }
}

#[test]
fn the_black_and_white_filter_keeps_ink_and_paper() {
    let mut image = RgbaImage::new(80, 80);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let paper = 180 + (x * 60 / 80) as u8;
        *pixel = Rgba([paper, paper, paper, 255]);
        if (10..70).contains(&x) && (20..30).contains(&y) {
            *pixel = Rgba([40, 40, 40, 255]);
        }
    }
    let filtered = apply_filter(&image, ScanFilter::BlackWhite);
    let mut ink = 0usize;
    let mut paper = 0usize;
    for pixel in filtered.pixels() {
        assert!(pixel[0] == 0 || pixel[0] == 255, "bilevel only: {}", pixel[0]);
        if pixel[0] == 0 {
            ink += 1;
        } else {
            paper += 1;
        }
    }
    assert!(ink > 300, "the dark bar becomes ink ({ink})");
    assert!(paper > ink, "most of the page stays paper ({paper} vs {ink})");
}

#[test]
fn the_colour_filter_changes_nothing() {
    let image = RgbaImage::from_pixel(2, 2, Rgba([1, 2, 3, 255]));
    assert_eq!(apply_filter(&image, ScanFilter::Colour), image);
}

#[test]
fn a_filtered_page_is_written_through_the_scan_path() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("page.png");
    let mut image = RgbaImage::new(60, 40);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        *pixel = Rgba([200, 200, 200, 255]);
        if (10..50).contains(&x) && (15..25).contains(&y) {
            *pixel = Rgba([30, 30, 30, 255]);
        }
    }
    DynamicImage::ImageRgba8(image).save(&source).unwrap();

    let corners = Some([[0.0, 0.0], [60.0, 0.0], [60.0, 40.0], [0.0, 40.0]]);
    let output = dir.path().join("out.jpg");
    assert!(prepare_page_filtered(&source, corners, ScanFilter::BlackWhite, &output).unwrap());
    let written = image::open(&output).unwrap().to_luma8();
    let ink = written.pixels().filter(|pixel| pixel[0] < 64).count();
    assert!(ink > 100, "the text bar survives the JPEG ({ink})");

    let plain = dir.path().join("plain.jpg");
    assert!(!prepare_page_filtered(&source, None, ScanFilter::Colour, &plain).unwrap());
    assert!(!plain.exists());
}
