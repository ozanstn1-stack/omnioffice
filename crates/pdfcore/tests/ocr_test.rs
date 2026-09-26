mod common;

use common::*;
use pdfcore::docutil::OverwritePolicy;
use pdfcore::ocr::{ocr_pdf, OcrOptions, OcrPreprocess};
use pdfcore::progress::CancelToken;

fn setup_scanned(dir: &TestDir, label: &str) -> std::path::PathBuf {
    let path = dir.path(&format!("{label}.pdf"));
    write_doc(
        &mut build_scanned_doc(2, "HELLO OCR 12345 MEETING NOTES", &format!("{label} scanned")),
        &path,
    );
    path
}

#[test]
fn ocr_searchable_pdf_adds_text_layer() {
    if !pdfcore::ocr::tesseract_available() {
        eprintln!("skipping: tesseract engine not available");
        return;
    }
    let dir = TestDir::new();
    let input = setup_scanned(&dir, "scanned");
    let out = dir.path("searchable.pdf");
    let options = OcrOptions {
        languages: vec!["eng".into()],
        psm: 3,
        dpi: 200,
        output_mode: "searchable_pdf".into(),
        pages: vec![],
        preprocess: OcrPreprocess { contrast: true, ..Default::default() },
        skip_text_pages: false,
    };
    let result = ocr_pdf(
        &input,
        &out,
        &options,
        OverwritePolicy::Replace,
        None,
        &no_progress,
        &CancelToken::new(),
    )
    .expect("ocr runs");

    assert_eq!(result.pages_processed, 2);
    assert!(result.characters > 10, "characters: {}", result.characters);
    let path = std::path::Path::new(&result.path);
    assert_eq!(page_count(path), 2);
    // The page dimensions must survive OCR.
    let box_ = media_box(path, 1);
    assert!((box_[2] - 595.28).abs() < 2.0, "mediabox width {}", box_[2]);
    assert!((box_[3] - 841.89).abs() < 2.0, "mediabox height {}", box_[3]);
    // The output must carry a searchable text layer.
    let text = pdfcore::render::extract_page_text(path, None, 1).unwrap_or_default();
    let normalized = text.to_uppercase();
    assert!(
        normalized.contains("HELLO") || normalized.contains("OCR"),
        "text layer was: {text}"
    );
}

#[test]
fn ocr_text_and_markdown_modes() {
    if !pdfcore::ocr::tesseract_available() {
        eprintln!("skipping: tesseract engine not available");
        return;
    }
    let dir = TestDir::new();
    let input = setup_scanned(&dir, "text-mode");
    let mut options = OcrOptions {
        languages: vec!["eng".into()],
        output_mode: "text".into(),
        dpi: 200,
        ..Default::default()
    };
    let result = ocr_pdf(
        &input,
        &dir.path("out.txt"),
        &options,
        OverwritePolicy::Replace,
        None,
        &no_progress,
        &CancelToken::new(),
    )
    .unwrap();
    let text = std::fs::read_to_string(&result.path).unwrap();
    assert!(text.to_uppercase().contains("MEETING") || text.to_uppercase().contains("HELLO"));
    assert!(text.contains("--- Page 1 ---"));

    options.output_mode = "markdown".into();
    let result = ocr_pdf(
        &input,
        &dir.path("out.md"),
        &options,
        OverwritePolicy::Replace,
        None,
        &no_progress,
        &CancelToken::new(),
    )
    .unwrap();
    let md = std::fs::read_to_string(&result.path).unwrap();
    assert!(md.contains("## Page 1"));
}

#[test]
fn ocr_reads_noisy_jpeg_scans() {
    if !pdfcore::ocr::tesseract_available() {
        eprintln!("skipping: tesseract engine not available");
        return;
    }
    let dir = TestDir::new();
    let input = dir.path("noisy-scan.pdf");
    write_doc(&mut build_noisy_jpeg_scan(1, "noisy scan"), &input);
    let out = dir.path("noisy-ocr.pdf");
    let options = OcrOptions {
        languages: vec!["eng".into()],
        output_mode: "searchable_pdf".into(),
        dpi: 200,
        skip_text_pages: true,
        ..Default::default()
    };
    let result = ocr_pdf(
        &input,
        &out,
        &options,
        OverwritePolicy::Replace,
        None,
        &no_progress,
        &CancelToken::new(),
    )
    .expect("ocr runs on noisy scans");
    assert_eq!(result.pages_processed, 1, "contrast preprocessing must not destroy the text");
    let path = std::path::Path::new(&result.path);
    let text = pdfcore::render::extract_page_text(path, None, 1).unwrap_or_default().to_uppercase();
    assert!(text.contains("INVOICE"), "text layer was: {text}");
}

#[test]
fn ocr_unknown_language_is_rejected() {
    if !pdfcore::ocr::tesseract_available() {
        eprintln!("skipping: tesseract engine not available");
        return;
    }
    let dir = TestDir::new();
    let input = setup_scanned(&dir, "lang");
    let options = OcrOptions {
        languages: vec!["zzz".into()],
        ..Default::default()
    };
    let result = ocr_pdf(
        &input,
        &dir.path("out.pdf"),
        &options,
        OverwritePolicy::Replace,
        None,
        &no_progress,
        &CancelToken::new(),
    );
    assert!(matches!(result, Err(pdfcore::PdfError::InvalidInput(_))));
}

#[test]
fn ocr_selected_pages_keep_others_original() {
    if !pdfcore::ocr::tesseract_available() {
        eprintln!("skipping: tesseract engine not available");
        return;
    }
    let dir = TestDir::new();
    let input = setup_scanned(&dir, "partial");
    let out = dir.path("partial-ocr.pdf");
    let options = OcrOptions {
        languages: vec!["eng".into()],
        output_mode: "searchable_pdf".into(),
        dpi: 200,
        pages: vec![2],
        ..Default::default()
    };
    let result = ocr_pdf(
        &input,
        &out,
        &options,
        OverwritePolicy::Replace,
        None,
        &no_progress,
        &CancelToken::new(),
    )
    .unwrap();
    assert_eq!(result.pages_processed, 1);
    let path = std::path::Path::new(&result.path);
    assert_eq!(page_count(path), 2);
    // Page 2 got a text layer, page 1 stayed a pure scan (still renderable).
    let text2 = pdfcore::render::extract_page_text(path, None, 2).unwrap_or_default();
    assert!(!text2.trim().is_empty(), "page 2 should carry OCR text");
    if pdfcore::render::is_available() {
        let rendered = pdfcore::render::render_page(
            path,
            None,
            1,
            &pdfcore::render::RenderOptions { dpi: 72.0, max_width: Some(600), max_height: Some(600) },
        );
        assert!(rendered.is_ok(), "page 1 must stay renderable");
    }
}

#[test]
fn ocr_skips_text_pages_when_requested() {
    if !pdfcore::ocr::tesseract_available() {
        eprintln!("skipping: tesseract engine not available");
        return;
    }
    let dir = TestDir::new();
    // Mixed document: page 1 has a text layer, page 2 is a scan.
    let scan = build_scanned_doc(1, "SCANNED PAGE TWO", "mixed");
    let single = dir.path("scan.pdf");
    write_doc(&mut { scan.clone() }, &single);
    let texty = dir.path("texty.pdf");
    write_doc(&mut build_text_doc(1, "DIGITAL", "digital"), &texty);
    let merged = dir.path("mixed.pdf");
    pdfcore::merge::merge_files(
        &[texty, single],
        &merged,
        &pdfcore::merge::MergeOptions::default(),
        OverwritePolicy::Replace,
        &no_progress,
        &CancelToken::new(),
    )
    .unwrap();

    let options = OcrOptions {
        languages: vec!["eng".into()],
        output_mode: "searchable_pdf".into(),
        dpi: 200,
        skip_text_pages: true,
        ..Default::default()
    };
    let result = ocr_pdf(
        &merged,
        &dir.path("mixed-ocr.pdf"),
        &options,
        OverwritePolicy::Replace,
        None,
        &no_progress,
        &CancelToken::new(),
    )
    .unwrap();
    assert_eq!(result.pages_processed, 1);
    assert_eq!(result.pages_skipped, 1);
}

#[test]
fn ocr_auto_rotates_a_quarter_turned_scan() {
    if !pdfcore::ocr::tesseract_available() {
        eprintln!("skipping: tesseract engine not available");
        return;
    }
    if !pdfcore::render::is_available() {
        eprintln!("skipping: pdfium is not available");
        return;
    }
    let dir = TestDir::new();
    let source = dir.path("upright.pdf");
    // Orientation detection needs a text-dense page; a single short line is
    // rejected by tesseract OSD with "Too few characters".
    let caption = "HELLO OCR MEETING NOTES\nAGENDA ITEMS AND ATTENDEES\nBUDGET REVIEW AND PLANNING\nFOLLOW UP ACTIONS FOR OWNERS\nNEXT MEETING SCHEDULED SOON";
    write_doc(&mut build_scanned_doc(1, caption, "rotated scan"), &source);

    // Render the upright page, turn it a quarter clockwise, and put the turned
    // raster into a PDF so OCR has to correct the orientation itself.
    let rendered = pdfcore::render::render_page(
        &source,
        None,
        1,
        &pdfcore::render::RenderOptions {
            dpi: 150.0,
            max_width: Some(2500),
            max_height: Some(2500),
        },
    )
    .expect("render");
    let image = rendered.to_dynamic_image().expect("rgba").to_rgba8();
    let rotated = pdfcore::ocr::rotate_quarters(&image, 90);
    let png = dir.path("rotated.png");
    rotated.save(&png).expect("save rotated png");

    let rotated_pdf = dir.path("rotated.pdf");
    pdfcore::images::images_to_pdf(
        &[pdfcore::images::ImageItem {
            path: png.to_string_lossy().to_string(),
            rotation_delta: 0,
        }],
        &pdfcore::images::ImageToPdfOptions {
            page_size: "original".into(),
            ..Default::default()
        },
        &rotated_pdf,
        OverwritePolicy::Replace,
        &no_progress,
        &CancelToken::new(),
    )
    .expect("image to pdf");

    let options = OcrOptions {
        languages: vec!["eng".into()],
        output_mode: "searchable_pdf".into(),
        dpi: 200,
        preprocess: OcrPreprocess {
            auto_rotate: true,
            contrast: false,
            ..Default::default()
        },
        skip_text_pages: false,
        ..Default::default()
    };
    let result = ocr_pdf(
        &rotated_pdf,
        &dir.path("rotated-ocr.pdf"),
        &options,
        OverwritePolicy::Replace,
        None,
        &no_progress,
        &CancelToken::new(),
    )
    .expect("ocr runs");

    assert!(
        result.preprocess_applied.iter().any(|step| step.starts_with("rotate")),
        "a rotation must be reported: {:?}",
        result.preprocess_applied
    );
    let text = pdfcore::render::extract_page_text(std::path::Path::new(&result.path), None, 1)
        .unwrap_or_default()
        .to_uppercase();
    assert!(
        text.contains("HELLO") || text.contains("OCR") || text.contains("MEETING"),
        "the corrected text layer was: {text}"
    );
}
