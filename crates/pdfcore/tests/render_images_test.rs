mod common;

use common::*;
use pdfcore::annotate::{annotate_pdf, Annotation};
use pdfcore::docutil::OverwritePolicy;
use pdfcore::error::PdfError;
use pdfcore::images::{images_to_pdf, ImageFormat, ImageItem, ImageToPdfOptions};
use pdfcore::numbering::{add_page_numbers, NumberingOptions};
use pdfcore::progress::CancelToken;
use pdfcore::render::{PixelRegion, RenderOptions};
use pdfcore::watermark::{add_watermark, WatermarkOptions};

fn engine_available() -> bool {
    pdfcore::render::is_available()
}

fn setup(label: &str, pages: u32) -> (TestDir, std::path::PathBuf) {
    let dir = TestDir::new();
    let path = dir.path(&format!("{label}.pdf"));
    write_doc(&mut build_text_doc(pages, label, &format!("{label} title")), &path);
    (dir, path)
}

#[test]
fn pdf_to_images_at_dpi() {
    if !engine_available() {
        eprintln!("skipping: pdfium not available");
        return;
    }
    let (dir, input) = setup("toimg", 2);
    let out_dir = dir.path("images");
    std::fs::create_dir_all(&out_dir).unwrap();

    let result = pdfcore::convert::pdf_to_images(
        &input,
        &out_dir,
        ImageFormat::Jpeg,
        150,
        90,
        false,
        "page",
        &[1, 2],
        OverwritePolicy::Replace,
        None,
        &no_progress,
        &CancelToken::new(),
    )
    .unwrap();
    assert_eq!(result.files.len(), 2);
    for file in &result.files {
        let path = std::path::Path::new(&file.path);
        assert!(path.exists());
        let img = image::open(path).unwrap();
        // A4 at 150 dpi ~= 1240 x 1754
        assert!((img.width() as i64 - 1240).abs() < 6, "width {}", img.width());
        assert!((img.height() as i64 - 1754).abs() < 6, "height {}", img.height());
    }
    assert!(result.files[0].path.ends_with("page_001.jpg"));

    let png_result = pdfcore::convert::pdf_to_images(
        &input,
        &out_dir,
        ImageFormat::Png,
        72,
        90,
        true,
        "render",
        &[1],
        OverwritePolicy::Replace,
        None,
        &no_progress,
        &CancelToken::new(),
    )
    .unwrap();
    let img = image::open(std::path::Path::new(&png_result.files[0].path)).unwrap();
    assert_eq!(img.color(), image::ColorType::L8);
}

/// Regression for the blurry reader zoom: `page_preview` used to render at a
/// fixed 96 dpi and only ever downscale, so asking for a larger raster
/// returned the same ~793 px A4 bitmap and the webview upscaled it. A high
/// render dpi with the same `max_width` must produce the requested width.
#[test]
fn high_dpi_render_honours_the_requested_raster_width() {
    if !engine_available() {
        eprintln!("skipping: pdfium not available");
        return;
    }
    let (_dir, input) = setup("previewdpi", 1);
    let low = pdfcore::render::render_page(
        &input,
        None,
        1,
        &RenderOptions { dpi: 96.0, max_width: Some(2000), max_height: None },
    )
    .unwrap();
    let high = pdfcore::render::render_page(
        &input,
        None,
        1,
        &RenderOptions { dpi: 600.0, max_width: Some(2000), max_height: None },
    )
    .unwrap();
    // A4 at 96 dpi is ~793 px, so the low-dpi render stays at its own width…
    assert!(low.width < 1000, "expected the 96 dpi render to stay small, got {}", low.width);
    // …while the high-dpi render reaches the requested raster width.
    assert_eq!(high.width, 2000, "high dpi render must reach the requested width");
    assert!(high.height > low.height);
}

#[test]
fn images_to_pdf_builds_pages() {
    let dir = TestDir::new();
    let img1 = dir.path("one.png");
    let img2 = dir.path("two.jpg");
    write_test_image(&img1, 800, 600, [200, 40, 40]);
    write_test_image(&img2, 600, 900, [40, 200, 40]);

    let items = vec![
        ImageItem { path: img1.display().to_string(), rotation_delta: 0 },
        ImageItem { path: img2.display().to_string(), rotation_delta: 90 },
    ];
    let options = ImageToPdfOptions {
        page_size: "a4".into(),
        fit: "fit".into(),
        orientation: "auto".into(),
        ..Default::default()
    };

    let out = dir.path("album.pdf");
    let path =
        images_to_pdf(&items, &options, &out, OverwritePolicy::Replace, &no_progress, &CancelToken::new()).unwrap();
    assert_eq!(page_count(&path), 2);
    let first = media_box(&path, 1);
    // Landscape source in auto mode -> landscape A4
    assert!(first[2] > first[3], "expected landscape page, got {first:?}");
    let second = media_box(&path, 2);
    assert!(second[3] > second[2], "expected portrait page, got {second:?}");

    // Original page size mode
    let original = ImageToPdfOptions { page_size: "original".into(), dpi: 96, ..Default::default() };
    let out2 = dir.path("original.pdf");
    images_to_pdf(&items[..1], &original, &out2, OverwritePolicy::Replace, &no_progress, &CancelToken::new()).unwrap();
    let box_ = media_box(&out2, 1);
    assert!((box_[2] - 800.0 * 72.0 / 96.0).abs() < 1.0);
}

#[test]
fn images_to_pdf_rejects_invalid_image() {
    let dir = TestDir::new();
    let bad = dir.path("bad.png");
    std::fs::write(&bad, b"this is not a png").unwrap();
    let items = vec![ImageItem { path: bad.display().to_string(), rotation_delta: 0 }];
    let result = images_to_pdf(
        &items,
        &ImageToPdfOptions::default(),
        &dir.path("out.pdf"),
        OverwritePolicy::Replace,
        &no_progress,
        &CancelToken::new(),
    );
    assert!(matches!(result, Err(pdfcore::PdfError::InvalidImage(_))));
}

#[test]
fn watermark_text_changes_render_and_only_selected_pages() {
    if !engine_available() {
        eprintln!("skipping: pdfium not available");
        return;
    }
    let (dir, input) = setup("wm", 3);
    let out = dir.path("wm-out.pdf");
    let options = WatermarkOptions {
        kind: "text".into(),
        text: "GİZLİ - CONFIDENTIAL".into(),
        font_size_pt: 42.0,
        bold: true,
        color: "#cc0000".into(),
        opacity: 0.4,
        rotation_deg: 45.0,
        position: "center".into(),
        tile: false,
        pages: vec![1, 3],
        ..Default::default()
    };
    add_watermark(&input, &out, &options, OverwritePolicy::Replace, None, &no_progress, &CancelToken::new()).unwrap();
    assert_eq!(page_count(&out), 3);

    // Render page 1 (watermarked) and 2 (untouched) and compare against the source.
    let render_options = RenderOptions { dpi: 72.0, max_width: Some(800), max_height: Some(800) };
    let src1 = pdfcore::render::render_page(&input, None, 1, &render_options).unwrap();
    let wm1 = pdfcore::render::render_page(&out, None, 1, &render_options).unwrap();
    let src2 = pdfcore::render::render_page(&input, None, 2, &render_options).unwrap();
    let wm2 = pdfcore::render::render_page(&out, None, 2, &render_options).unwrap();
    assert!(
        changed_pixels(&src1, &wm1, 8) > 300,
        "watermark should change page 1 (changed: {})",
        changed_pixels(&src1, &wm1, 8)
    );
    assert_eq!(changed_pixels(&src2, &wm2, 8), 0, "page 2 must stay untouched");
}

#[test]
fn watermark_image_and_tile() {
    if !engine_available() {
        eprintln!("skipping: pdfium not available");
        return;
    }
    let (dir, input) = setup("wmimg", 1);
    let logo = dir.path("logo.png");
    write_test_image(&logo, 200, 200, [20, 60, 220]);
    let out = dir.path("wm-img.pdf");
    let options = WatermarkOptions {
        kind: "image".into(),
        image_path: Some(logo.display().to_string()),
        image_scale: 0.25,
        opacity: 0.5,
        rotation_deg: 0.0,
        tile: true,
        ..Default::default()
    };
    add_watermark(&input, &out, &options, OverwritePolicy::Replace, None, &no_progress, &CancelToken::new()).unwrap();
    let render_options = RenderOptions { dpi: 72.0, max_width: Some(800), max_height: Some(800) };
    let src = pdfcore::render::render_page(&input, None, 1, &render_options).unwrap();
    let tiled = pdfcore::render::render_page(&out, None, 1, &render_options).unwrap();
    assert!(changed_pixels(&src, &tiled, 8) > 1000);
}

#[test]
fn page_numbers_are_added() {
    let (dir, input) = setup("num", 2);
    let out = dir.path("num-out.pdf");
    let options = NumberingOptions {
        position: "bottom_center".into(),
        format: "page_n_of_total".into(),
        start_number: 1,
        ..Default::default()
    };
    add_page_numbers(&input, &out, &options, OverwritePolicy::Replace, None, &no_progress, &CancelToken::new())
        .unwrap();
    assert_eq!(page_count(&out), 2);
    let text = page_text(&out, 1);
    assert!(text.contains("Page 1 of 2"), "text was: {text}");
    let text2 = page_text(&out, 2);
    assert!(text2.contains("Page 2 of 2"));
}

#[test]
fn text_search_finds_matches_across_pages() {
    if !engine_available() {
        eprintln!("skipping: pdfium not available");
        return;
    }
    let (dir, input) = setup("search", 4);
    // The sample generator labels pages "<label> page N"; search for a phrase
    // that exists on several pages plus one that does not exist at all.
    let cancel = CancelToken::new();
    let result = pdfcore::render::search_document(&input, None, "search page", false, 50, &cancel, &|_, _| {})
        .expect("search runs");
    assert_eq!(result.total_matches, 4, "one match on each page");
    assert_eq!(result.pages_with_matches, 4);
    assert!(!result.truncated);
    let first = &result.matches[0];
    assert_eq!(first.page, 1);
    assert!(first.snippet.to_lowercase().contains("search page"), "snippet was {:?}", first.snippet);

    // Case-insensitive by default, case-sensitive when asked.
    let insensitive =
        pdfcore::render::search_document(&input, None, "SEARCH PAGE", false, 50, &cancel, &|_, _| {}).unwrap();
    assert_eq!(insensitive.total_matches, 4);
    let sensitive =
        pdfcore::render::search_document(&input, None, "SEARCH PAGE", true, 50, &cancel, &|_, _| {}).unwrap();
    assert_eq!(sensitive.total_matches, 0);

    // No matches for text that is not present.
    let missing =
        pdfcore::render::search_document(&input, None, "zzzz-not-there", false, 50, &cancel, &|_, _| {}).unwrap();
    assert_eq!(missing.total_matches, 0);
    assert_eq!(missing.pages_with_matches, 0);

    // The result cap is honoured and reported.
    let capped = pdfcore::render::search_document(&input, None, "page", false, 3, &cancel, &|_, _| {}).unwrap();
    assert_eq!(capped.total_matches, 3);
    assert!(capped.truncated);

    // Cancellation stops the scan.
    let cancelled = CancelToken::new();
    cancelled.cancel();
    assert!(matches!(
        pdfcore::render::search_document(&input, None, "page", false, 50, &cancelled, &|_, _| {}),
        Err(pdfcore::PdfError::Cancelled)
    ));

    // Empty queries are rejected.
    assert!(pdfcore::render::search_document(&input, None, "   ", false, 50, &cancel, &|_, _| {}).is_err());
    let _ = dir;
}

#[test]
fn annotations_are_flattened_into_pages() {
    if !engine_available() {
        eprintln!("skipping: pdfium not available");
        return;
    }
    let (dir, input) = setup("ann", 1);
    let stamp = dir.path("stamp.png");
    write_test_image(&stamp, 120, 80, [250, 120, 10]);
    let out = dir.path("ann-out.pdf");
    let annotations = vec![
        Annotation {
            kind: "rect".into(),
            page: 1,
            x: 60.0,
            y: 80.0,
            w: 220.0,
            h: 120.0,
            text: String::new(),
            font_size_pt: 14.0,
            bold: false,
            color: "#e11d48".into(),
            opacity: 1.0,
            image_path: None,
            line_width_pt: 3.0,
            x2: None,
            y2: None,
            strokes: Vec::new(),
            image_base64: None,
        },
        Annotation {
            kind: "highlight".into(),
            page: 1,
            x: 60.0,
            y: 240.0,
            w: 260.0,
            h: 40.0,
            text: String::new(),
            font_size_pt: 14.0,
            bold: false,
            color: "#facc15".into(),
            opacity: 0.4,
            image_path: None,
            line_width_pt: 2.0,
            x2: None,
            y2: None,
            strokes: Vec::new(),
            image_base64: None,
        },
        Annotation {
            kind: "line".into(),
            page: 1,
            x: 60.0,
            y: 320.0,
            w: 0.0,
            h: 0.0,
            text: String::new(),
            font_size_pt: 14.0,
            bold: false,
            color: "#2563eb".into(),
            opacity: 1.0,
            image_path: None,
            line_width_pt: 2.0,
            x2: Some(340.0),
            y2: Some(360.0),
            strokes: Vec::new(),
            image_base64: None,
        },
        Annotation {
            kind: "text".into(),
            page: 1,
            x: 60.0,
            y: 420.0,
            w: 300.0,
            h: 60.0,
            text: "Onaylandı ✓ şğüöç".into(),
            font_size_pt: 18.0,
            bold: true,
            color: "#111827".into(),
            opacity: 1.0,
            image_path: None,
            line_width_pt: 1.0,
            x2: None,
            y2: None,
            strokes: Vec::new(),
            image_base64: None,
        },
        Annotation {
            kind: "image".into(),
            page: 1,
            x: 60.0,
            y: 520.0,
            w: 120.0,
            h: 80.0,
            text: String::new(),
            font_size_pt: 14.0,
            bold: false,
            color: "#000000".into(),
            opacity: 1.0,
            image_path: Some(stamp.display().to_string()),
            line_width_pt: 1.0,
            x2: None,
            y2: None,
            strokes: Vec::new(),
            image_base64: None,
        },
    ];
    annotate_pdf(&input, &out, &annotations, OverwritePolicy::Replace, None, &no_progress, &CancelToken::new())
        .unwrap();
    let render_options = RenderOptions { dpi: 72.0, max_width: Some(800), max_height: Some(800) };
    let src = pdfcore::render::render_page(&input, None, 1, &render_options).unwrap();
    let annotated = pdfcore::render::render_page(&out, None, 1, &render_options).unwrap();
    assert!(changed_pixels(&src, &annotated, 8) > 200);
}

/// The pixels of `region` cut out of a full-page render.
fn crop(full: &pdfcore::render::RenderedPage, region: PixelRegion) -> Vec<u8> {
    let mut out = Vec::with_capacity((region.width * region.height * 4) as usize);
    for row in region.y..region.y + region.height {
        let start = ((row * full.width + region.x) * 4) as usize;
        out.extend_from_slice(&full.rgba[start..start + (region.width * 4) as usize]);
    }
    out
}

/// (largest, mean) absolute channel difference between two RGBA buffers.
fn channel_diff(a: &[u8], b: &[u8]) -> (u8, f64) {
    assert_eq!(a.len(), b.len(), "buffers must have the same size");
    let mut largest = 0u8;
    let mut total = 0u64;
    for (x, y) in a.iter().zip(b) {
        let diff = x.abs_diff(*y);
        largest = largest.max(diff);
        total += diff as u64;
    }
    (largest, total as f64 / a.len().max(1) as f64)
}

/// A region render must be the matching crop of a full-page render at the
/// same scale: the reader lays tiles over the page by those coordinates, so
/// any drift shows up as seams or doubled text. Covers a text page, a
/// `/Rotate 90` page and a raster (scan-like) page, interior and edge regions.
#[test]
fn region_render_matches_a_crop_of_the_full_page() {
    if !engine_available() {
        eprintln!("skipping: pdfium not available");
        return;
    }
    let dir = TestDir::new();
    let text = dir.path("tiles-text.pdf");
    write_doc(&mut build_text_doc(1, "tiles", "Tiles"), &text);
    let rotated = dir.path("tiles-rotated.pdf");
    let mut doc = build_text_doc(1, "rotated", "Rotated");
    let page_id = *doc.get_pages().get(&1).unwrap();
    doc.get_object_mut(page_id).unwrap().as_dict_mut().unwrap().set("Rotate", 90);
    write_doc(&mut doc, &rotated);
    let scanned = dir.path("tiles-scan.pdf");
    write_doc(&mut build_scanned_doc(1, "Tile scan", "Scan"), &scanned);

    // 2 px per point is 144 dpi; the full render goes through `render_page`.
    let scale = 2.0f32;
    let options = RenderOptions { dpi: 72.0 * scale, max_width: None, max_height: None };
    // Each document's interior region covers its heading (and the box stroke):
    // content, not just the page background.
    let documents = [
        (&text, PixelRegion { x: 130, y: 210, width: 512, height: 300 }),
        (&rotated, PixelRegion { x: 1150, y: 120, width: 512, height: 300 }),
        (&scanned, PixelRegion { x: 60, y: 120, width: 512, height: 300 }),
    ];
    for (input, interior) in documents {
        let full = pdfcore::render::render_page(input, None, 1, &options).unwrap();
        let regions = [
            // The top-left corner, the interior region and an edge region the
            // page boundary clips.
            PixelRegion { x: 0, y: 0, width: 256, height: 256 },
            interior,
            PixelRegion { x: full.width - 100, y: full.height - 60, width: 512, height: 512 },
        ];
        for region in regions {
            let tile = pdfcore::render::render_page_region(input, None, 1, scale, region).unwrap();
            let expected = PixelRegion {
                width: region.width.min(full.width - region.x),
                height: region.height.min(full.height - region.y),
                ..region
            };
            assert_eq!((tile.width, tile.height), (expected.width, expected.height), "{input:?} {region:?}");
            let (largest, mean) = channel_diff(&tile.rgba, &crop(&full, expected));
            assert!(
                largest <= 24 && mean < 0.5,
                "{input:?} {region:?}: region differs from the full render (max {largest}, mean {mean:.3})"
            );
        }
        // Negative control: a crop shifted by a few pixels must not match, so
        // the agreement above cannot come from comparing two blank areas.
        let tile = pdfcore::render::render_page_region(input, None, 1, scale, interior).unwrap();
        let shifted = crop(&full, PixelRegion { x: interior.x + 7, y: interior.y + 5, ..interior });
        let (_, mean) = channel_diff(&tile.rgba, &shifted);
        assert!(mean > 1.0, "{input:?}: the comparison is not discriminating (mean {mean:.3})");
    }
}

/// Region renders reuse an open document between calls; a save over the same
/// path must still be picked up instead of serving the old file's pixels.
#[test]
fn region_render_reloads_a_file_that_changed_on_disk() {
    if !engine_available() {
        eprintln!("skipping: pdfium not available");
        return;
    }
    let dir = TestDir::new();
    let path = dir.path("tiles-reload.pdf");
    write_doc(&mut build_text_doc(1, "first", "First"), &path);
    let region = PixelRegion { x: 100, y: 200, width: 400, height: 120 };
    let before = pdfcore::render::render_page_region(&path, None, 1, 2.0, region).unwrap();
    // Served from the open document: the same pixels again.
    let again = pdfcore::render::render_page_region(&path, None, 1, 2.0, region).unwrap();
    assert_eq!(before.rgba, again.rgba);

    // Replace the file the way the app saves: write elsewhere, rename over.
    let replacement = dir.path("tiles-reload-new.pdf");
    write_doc(&mut build_scanned_doc(1, "Second version", "Second"), &replacement);
    std::fs::rename(&replacement, &path).unwrap();
    let after = pdfcore::render::render_page_region(&path, None, 1, 2.0, region).unwrap();
    let options = RenderOptions { dpi: 144.0, max_width: None, max_height: None };
    let full = pdfcore::render::render_page(&path, None, 1, &options).unwrap();
    let (_, mean) = channel_diff(&after.rgba, &crop(&full, region));
    assert!(mean < 0.5, "the region must come from the new file (mean {mean:.3})");
    let (_, stale) = channel_diff(&after.rgba, &before.rgba);
    assert!(stale > 1.0, "the region still shows the old file (mean {stale:.3})");
}

/// The open document is keyed by password too: once a protected file was
/// rendered with the right password, a call without it (or with a wrong one)
/// must still fail instead of being served from the open document.
#[test]
fn region_render_does_not_reuse_a_document_across_passwords() {
    if !engine_available() {
        eprintln!("skipping: pdfium not available");
        return;
    }
    let dir = TestDir::new();
    let plain = dir.path("tiles-plain.pdf");
    write_doc(&mut build_text_doc(1, "secret", "Secret"), &plain);
    let protected = dir.path("tiles-protected.pdf");
    let options = pdfcore::security::ProtectOptions {
        user_password: "user-pass".into(),
        owner_password: "owner-pass".into(),
        allow_printing: true,
        allow_copying: true,
        allow_editing: true,
        allow_commenting: true,
    };
    pdfcore::security::protect_pdf(&plain, &protected, &options, OverwritePolicy::Replace, None).unwrap();
    let region = PixelRegion { x: 100, y: 200, width: 256, height: 128 };
    let unlocked = pdfcore::render::render_page_region(&protected, Some("user-pass"), 1, 2.0, region).unwrap();
    assert!(pdfcore::render::render_page_region(&protected, None, 1, 2.0, region).is_err());
    assert!(pdfcore::render::render_page_region(&protected, Some("wrong"), 1, 2.0, region).is_err());
    let again = pdfcore::render::render_page_region(&protected, Some("user-pass"), 1, 2.0, region).unwrap();
    assert_eq!(unlocked.rgba, again.rgba);
}

#[test]
fn region_render_rejects_bad_scales_and_regions() {
    let dir = TestDir::new();
    let path = dir.path("tiles-bounds.pdf");
    write_doc(&mut build_text_doc(1, "bounds", "Bounds"), &path);
    let region = PixelRegion { x: 0, y: 0, width: 256, height: 256 };
    // Argument checks run before the engine is touched.
    for scale in [0.0, -1.0, f32::NAN, f32::INFINITY, pdfcore::render::MAX_REGION_SCALE + 1.0] {
        let error = pdfcore::render::render_page_region(&path, None, 1, scale, region).unwrap_err();
        assert!(matches!(error, PdfError::InvalidInput(_)), "scale {scale}: {error:?}");
    }
    for (width, height) in [(0, 10), (10, 0), (pdfcore::render::MAX_REGION_EDGE + 1, 10)] {
        let sized = PixelRegion { width, height, ..region };
        let error = pdfcore::render::render_page_region(&path, None, 1, 1.0, sized).unwrap_err();
        assert!(matches!(error, PdfError::InvalidInput(_)), "{width}x{height}: {error:?}");
    }
    if !engine_available() {
        eprintln!("skipping the engine checks: pdfium not available");
        return;
    }
    // A4 at 1 px per point is 595 x 842: a region starting past it is empty.
    let outside = PixelRegion { x: 600, y: 0, width: 64, height: 64 };
    let error = pdfcore::render::render_page_region(&path, None, 1, 1.0, outside).unwrap_err();
    assert!(matches!(error, PdfError::InvalidInput(_)), "{error:?}");
    for page in [0, 2] {
        let error = pdfcore::render::render_page_region(&path, None, page, 1.0, region).unwrap_err();
        assert!(matches!(error, PdfError::RangeOutOfBounds), "page {page}: {error:?}");
    }
}
