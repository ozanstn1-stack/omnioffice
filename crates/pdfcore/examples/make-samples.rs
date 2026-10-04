//! Generates safe, synthetic sample documents used for development and for
//! the documented "final validation" walkthrough. No real personal data is
//! involved anywhere.
//!
//! Usage: cargo run -p pdfcore --example make-samples -- <output-dir>

use lopdf::{dictionary, Dictionary, Document, Object, Stream};
use pdfcore::docutil::{add_resource_entry, add_rgb_image_xobject, add_rgba_image_xobject, pdf_text_object, RawImage};
use pdfcore::textimg::{render_text, TextRenderRequest};
use std::path::{Path, PathBuf};

fn finish(mut doc: Document, page_ids: Vec<lopdf::ObjectId>, title: &str) -> Document {
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
        "Author" => pdf_text_object("PDF Swiss Army Knife samples"),
        "Subject" => pdf_text_object("Synthetic test document"),
    }));
    doc.trailer.set("Info", Object::Reference(info_id));
    doc
}

fn text_document(pages: u32, label: &str, title: &str) -> Document {
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
            "0.94 0.94 0.97 rg\n40 40 515 760 re f\n0.35 0.4 0.85 RG\n3 w\n80 560 300 140 re\nS\n0.1 0.1 0.12 rg\nBT\n/F1 26 Tf\n72 700 Td\n({label} - page {page}) Tj\nET\nBT\n/F1 12 Tf\n72 640 Td\n(Sample document generated for testing.) Tj\nET\n",
        );
        let content_id = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), content.into_bytes())));
        doc.get_object_mut(page_id).unwrap().as_dict_mut().unwrap().set("Contents", Object::Reference(content_id));
        page_ids.push(page_id);
    }
    finish(doc, page_ids, title)
}

fn image_document(pages: u32, title: &str) -> Document {
    let mut doc = Document::new();
    doc.version = "1.7".to_string();
    let mut page_ids = Vec::new();
    let (w, h) = (1000u32, 1400u32);
    for page in 1..=pages {
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h {
            for x in 0..w {
                let base = ((x * 255 / w) as u16 + (y * 255 / h) as u16) / 2;
                let r = (base as u8).wrapping_add(20 * page as u8);
                let g = ((base as u8) as u16 * 3 / 4) as u8;
                let b = 200u8.wrapping_sub((page * 15) as u8);
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
    finish(doc, page_ids, title)
}

fn scanned_document(pages: u32, title: &str) -> Document {
    let mut doc = Document::new();
    doc.version = "1.7".to_string();
    let mut page_ids = Vec::new();
    for page in 1..=pages {
        let art = render_text(&TextRenderRequest {
            text: format!("SCANNED INVOICE {page}\nTotal: 1{page}0.00 EUR\nThank you for your business"),
            size_px: 84.0,
            color: [15, 15, 20, 255],
            bold: false,
            line_spacing: 1.4,
            padding_px: 30,
            wrap_width_px: None,
        })
        .unwrap();
        let (w, h) = (1240u32, 1754u32);
        let mut rgba = vec![248u8; (w * h * 4) as usize];
        for i in (3..rgba.len()).step_by(4) {
            rgba[i] = 255;
        }
        let ox = 110i64;
        let oy = 220i64;
        for y in 0..art.height as i64 {
            for x in 0..art.width as i64 {
                let (sx, sy) = (ox + x, oy + y);
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
        let image_id = add_rgba_image_xobject(&mut doc, &RawImage { width: w, height: h, rgba }).unwrap();
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
    finish(doc, page_ids, title)
}

/// A realistic "desktop scan": 300 DPI grayscale-ish JPEG with sensor noise
/// and JPEG artifacts, embedded with DCTDecode like a real scanner produces.
fn scanned_jpeg_document(pages: u32, title: &str) -> Document {
    let mut doc = Document::new();
    doc.version = "1.7".to_string();
    let mut page_ids = Vec::new();
    for page in 1..=pages {
        let art = render_text(&TextRenderRequest {
            text: format!(
                "INVOICE {page:04}\nAcme Consulting Ltd.\nTotal due: {} EUR\nPayment terms: 30 days",
                1250 + page * 17
            ),
            size_px: 64.0,
            color: [20, 20, 24, 255],
            bold: false,
            line_spacing: 1.5,
            padding_px: 20,
            wrap_width_px: None,
        })
        .unwrap();
        let (w, h) = (2480u32, 3508u32); // A4 at 300 DPI
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        // Paper background with noise, like a real scanner.
        let mut seed = 0x12345678u32 ^ page;
        for i in 0..(w * h) as usize {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let noise = ((seed >> 24) & 0x0f) as u8;
            let value = 244u8.saturating_sub(noise);
            let index = i * 4;
            rgba[index] = value;
            rgba[index + 1] = value;
            rgba[index + 2] = value;
            rgba[index + 3] = 255;
        }
        let ox = 260i64;
        let oy = 420i64;
        for y in 0..art.height as i64 {
            for x in 0..art.width as i64 {
                let (sx, sy) = (ox + x, oy + y);
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
            }
        }
        let (rw, rh) = (w, h);
        let rgb_image = image::RgbImage::from_raw(rw, rh, {
            let mut rgb = Vec::with_capacity((rw * rh * 3) as usize);
            for px in rgba.as_chunks::<4>().0 {
                rgb.extend_from_slice(&px[0..3]);
            }
            rgb
        })
        .unwrap();
        let mut jpeg = Vec::new();
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 70);
        encoder.encode_image(&rgb_image).unwrap();

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
    finish(doc, page_ids, title)
}

fn main() {
    let out_dir = std::env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("samples"));
    std::fs::create_dir_all(&out_dir).expect("create output dir");

    let write = |name: &str, doc: &mut Document, dir: &Path| {
        let path = dir.join(name);
        doc.save(&path).expect("save sample");
        println!("wrote {}", path.display());
    };

    write("sample-1.pdf", &mut text_document(5, "Sample report A", "Sample Report A"), &out_dir);
    write("sample-2.pdf", &mut text_document(3, "Sample report B", "Sample Report B"), &out_dir);
    write("sample-images.pdf", &mut image_document(2, "Image-heavy sample"), &out_dir);
    write("sample-ocr.pdf", &mut scanned_document(2, "Scanned sample (no text layer)"), &out_dir);
    write("sample-scan-large.pdf", &mut scanned_jpeg_document(2, "Large 300 DPI scan"), &out_dir);
    write("sample-100-pages.pdf", &mut text_document(100, "Bulk sample", "Bulk Sample"), &out_dir);
    println!("done");
}
