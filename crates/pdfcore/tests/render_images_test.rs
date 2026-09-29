mod common;

use common::*;
use pdfcore::annotate::{annotate_pdf, Annotation};
use pdfcore::docutil::OverwritePolicy;
use pdfcore::images::{images_to_pdf, ImageFormat, ImageItem, ImageToPdfOptions};
use pdfcore::numbering::{add_page_numbers, NumberingOptions};
use pdfcore::progress::CancelToken;
use pdfcore::render::{RenderOptions};
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
    let path = images_to_pdf(
        &items,
        &options,
        &out,
        OverwritePolicy::Replace,
        &no_progress,
        &CancelToken::new(),
    )
    .unwrap();
    assert_eq!(page_count(&path), 2);
    let first = media_box(&path, 1);
    // Landscape source in auto mode -> landscape A4
    assert!(first[2] > first[3], "expected landscape page, got {first:?}");
    let second = media_box(&path, 2);
    assert!(second[3] > second[2], "expected portrait page, got {second:?}");

    // Original page size mode
    let original = ImageToPdfOptions { page_size: "original".into(), dpi: 96, ..Default::default() };
    let out2 = dir.path("original.pdf");
    images_to_pdf(
        &items[..1],
        &original,
        &out2,
        OverwritePolicy::Replace,
        &no_progress,
        &CancelToken::new(),
    )
    .unwrap();
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
    add_watermark(
        &input,
        &out,
        &options,
        OverwritePolicy::Replace,
        None,
        &no_progress,
        &CancelToken::new(),
    )
    .unwrap();
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
    add_watermark(
        &input,
        &out,
        &options,
        OverwritePolicy::Replace,
        None,
        &no_progress,
        &CancelToken::new(),
    )
    .unwrap();
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
    add_page_numbers(
        &input,
        &out,
        &options,
        OverwritePolicy::Replace,
        None,
        &no_progress,
        &CancelToken::new(),
    )
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
    let result = pdfcore::render::search_document(
        &input,
        None,
        "search page",
        false,
        50,
        &cancel,
        &|_, _| {},
    )
    .expect("search runs");
    assert_eq!(result.total_matches, 4, "one match on each page");
    assert_eq!(result.pages_with_matches, 4);
    assert!(!result.truncated);
    let first = &result.matches[0];
    assert_eq!(first.page, 1);
    assert!(first.snippet.to_lowercase().contains("search page"), "snippet was {:?}", first.snippet);

    // Case-insensitive by default, case-sensitive when asked.
    let insensitive = pdfcore::render::search_document(&input, None, "SEARCH PAGE", false, 50, &cancel, &|_, _| {}).unwrap();
    assert_eq!(insensitive.total_matches, 4);
    let sensitive = pdfcore::render::search_document(&input, None, "SEARCH PAGE", true, 50, &cancel, &|_, _| {}).unwrap();
    assert_eq!(sensitive.total_matches, 0);

    // No matches for text that is not present.
    let missing = pdfcore::render::search_document(&input, None, "zzzz-not-there", false, 50, &cancel, &|_, _| {}).unwrap();
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
        },
    ];
    annotate_pdf(
        &input,
        &out,
        &annotations,
        OverwritePolicy::Replace,
        None,
        &no_progress,
        &CancelToken::new(),
    )
    .unwrap();
    let render_options = RenderOptions { dpi: 72.0, max_width: Some(800), max_height: Some(800) };
    let src = pdfcore::render::render_page(&input, None, 1, &render_options).unwrap();
    let annotated = pdfcore::render::render_page(&out, None, 1, &render_options).unwrap();
    assert!(changed_pixels(&src, &annotated, 8) > 200);
}
