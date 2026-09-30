//! Shared test utilities: generates safe, synthetic PDFs (no real personal
//! documents) plus small inspection helpers.

#![allow(dead_code)]

use lopdf::{dictionary, Dictionary, Document, Object, Stream};
use pdfcore::docutil::{add_resource_entry, add_rgb_image_xobject, add_rgba_image_xobject, pdf_text_object, RawImage};
use pdfcore::progress::ProgressEvent;
use pdfcore::textimg::{render_text, TextRenderRequest};
use std::path::{Path, PathBuf};

pub struct TestDir {
    dir: tempfile::TempDir,
}

impl TestDir {
    pub fn new() -> Self {
        Self { dir: tempfile::Builder::new().prefix("pdfsak-test-").tempdir().expect("tempdir") }
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    pub fn dir(&self) -> &Path {
        self.dir.path()
    }
}

/// A no-op progress callback for tests.
pub fn no_progress(_event: ProgressEvent) {}

/// Builds a synthetic PDF: `pages` A4 pages, each with vector shapes and a
/// text line that identifies the document/page, e.g. "Sample A page 3".
pub fn build_text_doc(pages: u32, label: &str, title: &str) -> Document {
    let mut doc = Document::new();
    doc.version = "1.7".to_string();
    let mut page_ids = Vec::new();
    for page in 1..=pages {
        let page_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![
                Object::Real(0.0),
                Object::Real(0.0),
                Object::Real(595.28),
                Object::Real(841.89),
            ],
        }));
        let font_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => "WinAnsiEncoding",
        }));
        add_resource_entry(&mut doc, page_id, b"Font", "F1", Object::Reference(font_id)).unwrap();
        let content = format!(
            "0.9 0.9 0.95 rg\n40 40 515 760 re f\n0.1 0.35 0.8 RG\n3 w\n80 600 300 120 re\nS\nBT\n/F1 24 Tf\n72 700 Td\n({label} page {page}) Tj\nET\n",
        );
        let content_id = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), content.into_bytes())));
        doc.get_object_mut(page_id).unwrap().as_dict_mut().unwrap().set("Contents", Object::Reference(content_id));
        page_ids.push(page_id);
    }
    finish_doc(doc, page_ids, title)
}

/// Builds a synthetic PDF whose pages contain only a raster image (like a
/// scan), with a visible caption burned into the bitmap.
pub fn build_scanned_doc(pages: u32, caption: &str, title: &str) -> Document {
    let mut doc = Document::new();
    doc.version = "1.7".to_string();
    let mut page_ids = Vec::new();
    for page in 1..=pages {
        let text = format!("{caption} - page {page}");
        let art = render_text(&TextRenderRequest {
            text,
            size_px: 96.0,
            color: [10, 10, 10, 255],
            bold: true,
            line_spacing: 1.2,
            padding_px: 24,
            wrap_width_px: None,
        })
        .expect("render caption");
        // Compose onto a white page-sized raster (A4 at 150 dpi).
        let (w, h) = (1240u32, 1754u32);
        let mut rgba = vec![255u8; (w * h * 4) as usize];
        let ox = 80i64;
        let oy = 140i64;
        for y in 0..art.height as i64 {
            for x in 0..art.width as i64 {
                let sx = ox + x;
                let sy = oy + y;
                if sx >= w as i64 || sy >= h as i64 {
                    continue;
                }
                let src = ((y as u32 * art.width + x as u32) * 4) as usize;
                let dst = ((sy as u32 * w + sx as u32) * 4) as usize;
                let alpha = art.rgba[src + 3] as f32 / 255.0;
                for c in 0..3 {
                    rgba[dst + c] =
                        (art.rgba[src + c] as f32 * alpha + rgba[dst + c] as f32 * (1.0 - alpha)).round() as u8;
                }
                rgba[dst + 3] = 255;
            }
        }
        let raw = RawImage { width: w, height: h, rgba };
        let image_id = add_rgba_image_xobject(&mut doc, &raw).unwrap();
        let page_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![
                Object::Real(0.0),
                Object::Real(0.0),
                Object::Real(595.28),
                Object::Real(841.89),
            ],
        }));
        add_resource_entry(&mut doc, page_id, b"XObject", "Im0", Object::Reference(image_id)).unwrap();
        let content = "q\n595.28 0 0 841.89 0 0 cm\n/Im0 Do\nQ\n";
        let content_id = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), content.as_bytes().to_vec())));
        doc.get_object_mut(page_id).unwrap().as_dict_mut().unwrap().set("Contents", Object::Reference(content_id));
        page_ids.push(page_id);
    }
    finish_doc(doc, page_ids, title)
}

/// Builds a document with a photographic-ish color image on each page
/// (for compression tests).
pub fn build_image_doc(pages: u32, title: &str) -> Document {
    let mut doc = Document::new();
    doc.version = "1.7".to_string();
    let mut page_ids = Vec::new();
    let (w, h) = (900u32, 1300u32);
    for page in 1..=pages {
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h {
            for x in 0..w {
                let r = ((x * 255 / w) as u8).wrapping_add((page * 20) as u8);
                let g = ((y * 255 / h) as u8).wrapping_add(30);
                let b = (((x + y) % 255) as u8).wrapping_add(10);
                rgb.extend_from_slice(&[r, g, b]);
            }
        }
        let image_id = add_rgb_image_xobject(&mut doc, w, h, rgb, false).unwrap();
        let page_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![
                Object::Real(0.0),
                Object::Real(0.0),
                Object::Real(595.28),
                Object::Real(841.89),
            ],
        }));
        add_resource_entry(&mut doc, page_id, b"XObject", "Im0", Object::Reference(image_id)).unwrap();
        let content = "q\n595.28 0 0 841.89 0 0 cm\n/Im0 Do\nQ\n";
        let content_id = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), content.as_bytes().to_vec())));
        doc.get_object_mut(page_id).unwrap().as_dict_mut().unwrap().set("Contents", Object::Reference(content_id));
        page_ids.push(page_id);
    }
    finish_doc(doc, page_ids, title)
}

/// A realistic scanner output: noisy paper background, JPEG compressed and
/// embedded with DCTDecode (this is what compression/OCR have to handle).
pub fn build_noisy_jpeg_scan(pages: u32, title: &str) -> Document {
    let mut doc = Document::new();
    doc.version = "1.7".to_string();
    let mut page_ids = Vec::new();
    for page in 1..=pages {
        let art = render_text(&TextRenderRequest {
            text: format!("INVOICE {page:04}\nAcme Consulting Ltd.\nTotal due: {} EUR", 1250 + page * 17),
            size_px: 46.0,
            color: [20, 20, 24, 255],
            bold: false,
            line_spacing: 1.5,
            padding_px: 16,
            wrap_width_px: None,
        })
        .unwrap();
        let (w, h) = (1700u32, 2400u32);
        let mut rgb = vec![0u8; (w * h * 3) as usize];
        let mut seed = 0x2468_ace0u32 ^ page;
        for i in 0..(w * h) as usize {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let noise = ((seed >> 26) & 0x07) as u8;
            let value = 243u8.saturating_sub(noise);
            let index = i * 3;
            rgb[index] = value;
            rgb[index + 1] = value;
            rgb[index + 2] = value;
        }
        let ox = 170i64;
        let oy = 260i64;
        for y in 0..art.height as i64 {
            for x in 0..art.width as i64 {
                let (sx, sy) = (ox + x, oy + y);
                if sx >= w as i64 || sy >= h as i64 {
                    continue;
                }
                let src = ((y as u32 * art.width + x as u32) * 4) as usize;
                let dst = ((sy as u32 * w + sx as u32) * 3) as usize;
                let alpha = art.rgba[src + 3] as f32 / 255.0;
                for c in 0..3 {
                    rgb[dst + c] =
                        (art.rgba[src + c] as f32 * alpha + rgb[dst + c] as f32 * (1.0 - alpha)).round() as u8;
                }
            }
        }
        let image = image::RgbImage::from_raw(w, h, rgb).unwrap();
        let mut jpeg = Vec::new();
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 72);
        encoder.encode_image(&image).unwrap();
        let image_id = pdfcore::docutil::add_jpeg_image_xobject(&mut doc, w, h, jpeg, false).unwrap();
        let page_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![
                Object::Real(0.0),
                Object::Real(0.0),
                Object::Real(595.28),
                Object::Real(841.89),
            ],
        }));
        add_resource_entry(&mut doc, page_id, b"XObject", "Im0", Object::Reference(image_id)).unwrap();
        let content = "q\n595.28 0 0 841.89 0 0 cm\n/Im0 Do\nQ\n";
        let content_id = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), content.as_bytes().to_vec())));
        doc.get_object_mut(page_id).unwrap().as_dict_mut().unwrap().set("Contents", Object::Reference(content_id));
        page_ids.push(page_id);
    }
    finish_doc(doc, page_ids, title)
}

fn finish_doc(mut doc: Document, page_ids: Vec<lopdf::ObjectId>, title: &str) -> Document {
    let pages_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Pages",
        "Kids" => page_ids.iter().map(|id| Object::Reference(*id)).collect::<Vec<Object>>(),
        "Count" => page_ids.len() as i64,
    }));
    for page_id in &page_ids {
        doc.get_object_mut(*page_id).unwrap().as_dict_mut().unwrap().set("Parent", Object::Reference(pages_id));
    }
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));
    let info_id = doc.add_object(Object::Dictionary(dictionary! {
        "Title" => pdf_text_object(title),
        "Author" => pdf_text_object("Test Suite"),
    }));
    doc.trailer.set("Info", Object::Reference(info_id));
    doc
}

/// Writes a synthetic document to disk.
pub fn write_doc(doc: &mut Document, path: &Path) {
    doc.save(path).expect("save test pdf");
}

/// Generates a small PNG or JPEG test image on disk.
pub fn write_test_image(path: &Path, width: u32, height: u32, color: [u8; 3]) {
    let mut img = image::RgbImage::new(width, height);
    for (x, y, px) in img.enumerate_pixels_mut() {
        px.0 = [color[0].wrapping_add((x % 64) as u8), color[1].wrapping_add((y % 64) as u8), color[2]];
    }
    img.save(path).expect("save test image");
}

// ---------------------------------------------------------------------------
// Inspection helpers
// ---------------------------------------------------------------------------

pub fn page_count(path: &Path) -> u32 {
    let doc = Document::load(path).expect("load pdf");
    doc.get_pages().len() as u32
}

pub fn page_text(path: &Path, page: u32) -> String {
    let doc = Document::load(path).expect("load pdf");
    doc.extract_text(&[page]).unwrap_or_default()
}

pub fn page_rotation(path: &Path, page: u32) -> i32 {
    let doc = Document::load(path).expect("load pdf");
    let pages = doc.get_pages();
    let id = pages.get(&page).expect("page exists");
    doc.get_dictionary(*id)
        .ok()
        .and_then(|d| d.get(b"Rotate").ok())
        .and_then(|o| o.as_i64().ok())
        .map(|v| v.rem_euclid(360) as i32)
        .unwrap_or(0)
}

pub fn media_box(path: &Path, page: u32) -> [f64; 4] {
    let doc = Document::load(path).expect("load pdf");
    let pages = doc.get_pages();
    let id = pages.get(&page).expect("page exists");
    let dict = doc.get_dictionary(*id).expect("page dict");
    let arr = dict.get(b"MediaBox").expect("mediabox").as_array().expect("array");
    let mut out = [0.0; 4];
    for (i, obj) in arr.iter().enumerate() {
        out[i] = match obj {
            Object::Integer(v) => *v as f64,
            Object::Real(v) => *v as f64,
            _ => 0.0,
        };
    }
    out
}

pub fn crop_box(path: &Path, page: u32) -> Option<[f64; 4]> {
    let doc = Document::load(path).expect("load pdf");
    let pages = doc.get_pages();
    let id = pages.get(&page).expect("page exists");
    let dict = doc.get_dictionary(*id).expect("page dict");
    let arr = dict.get(b"CropBox").ok()?.as_array().ok()?;
    let mut out = [0.0; 4];
    for (i, obj) in arr.iter().enumerate() {
        out[i] = match obj {
            Object::Integer(v) => *v as f64,
            Object::Real(v) => *v as f64,
            _ => 0.0,
        };
    }
    Some(out)
}

/// Mean absolute pixel difference between two rendered pages (0-255).
pub fn mean_pixel_diff(a: &pdfcore::render::RenderedPage, b: &pdfcore::render::RenderedPage) -> f64 {
    let len = a.rgba.len().min(b.rgba.len());
    if len == 0 {
        return 0.0;
    }
    let mut total = 0u64;
    for i in 0..len {
        total += (a.rgba[i] as i64 - b.rgba[i] as i64).unsigned_abs();
    }
    total as f64 / len as f64
}

/// Number of pixels whose channel values differ by more than `tolerance`.
pub fn changed_pixels(a: &pdfcore::render::RenderedPage, b: &pdfcore::render::RenderedPage, tolerance: i64) -> usize {
    let len = a.rgba.len().min(b.rgba.len());
    let mut changed = 0;
    for i in 0..len {
        if (a.rgba[i] as i64 - b.rgba[i] as i64).abs() > tolerance {
            changed += 1;
        }
    }
    changed / 4
}
