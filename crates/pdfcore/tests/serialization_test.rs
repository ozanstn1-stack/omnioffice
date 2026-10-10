//! Locks the IPC wire format between the Rust command layer and the
//! TypeScript frontend. The two conventions are deliberate:
//!
//! * `PdfInfo`, `PdfToImagesResult` and the command-layer DTOs use camelCase.
//! * The pdfcore parameter/result structs (options, plans, annotations) keep
//!   their documented snake_case names.
//!
//! A silent rename here would break the UI at runtime, so this test guards
//! every struct that crosses the Tauri boundary.

use pdfcore::annotate::Annotation;
use pdfcore::compress::{CompressEstimate, CompressOptions};
use pdfcore::convert::ImageOutput;
use pdfcore::convert::PdfToImagesResult;
use pdfcore::docutil::PagePlanItem;
use pdfcore::images::{ImageItem, ImageToPdfOptions};
use pdfcore::info::PdfInfo;
use pdfcore::metadata::PdfMetadata;
use pdfcore::numbering::NumberingOptions;
use pdfcore::ocr::OcrOptions;
use pdfcore::organize::SplitPart;
use pdfcore::pagelayout::ResizeOptions;
use pdfcore::render::PageGeometry;
use pdfcore::watermark::WatermarkOptions;

fn keys(value: &serde_json::Value) -> Vec<String> {
    value.as_object().expect("object").keys().cloned().collect()
}

fn assert_keys(value: &serde_json::Value, expected: &[&str]) {
    let present = keys(value);
    for key in expected {
        assert!(present.contains(&key.to_string()), "missing key '{key}' in {present:?}");
    }
}

#[test]
fn pdf_info_is_camel_case() {
    let info = PdfInfo {
        path: "C:/in.pdf".into(),
        file_name: "in.pdf".into(),
        file_size_bytes: 1234,
        page_count: 3,
        pdf_version: "1.7".into(),
        encrypted: false,
        has_text_layer: true,
        metadata: PdfMetadata::default(),
        page_geometries: vec![],
        image_count: 2,
        title: "T".into(),
        author: "A".into(),
        producer: "P".into(),
        outline: vec![],
    };
    let json = serde_json::to_value(&info).unwrap();
    assert_keys(
        &json,
        &[
            "fileName",
            "fileSizeBytes",
            "pageCount",
            "pdfVersion",
            "hasTextLayer",
            "pageGeometries",
            "imageCount",
            "metadata",
        ],
    );
}

#[test]
fn geometry_and_metadata_keep_snake_case() {
    let geometry = PageGeometry {
        page: 1,
        width_pt: 595.28,
        height_pt: 841.89,
        display_width_pt: 595.28,
        display_height_pt: 841.89,
        rotation: 0,
    };
    assert_keys(&serde_json::to_value(&geometry).unwrap(), &["display_width_pt", "display_height_pt", "rotation"]);

    let metadata = serde_json::to_value(PdfMetadata::default()).unwrap();
    assert_keys(&metadata, &["creation_date", "mod_date", "title", "author"]);
}

#[test]
fn options_keep_snake_case() {
    assert_keys(
        &serde_json::to_value(CompressOptions::default()).unwrap(),
        &["jpeg_quality", "remove_metadata", "strategy", "preset", "grayscale", "dpi"],
    );
    assert_keys(
        &serde_json::to_value(OcrOptions::default()).unwrap(),
        &["languages", "psm", "dpi", "output_mode", "pages", "preprocess", "skip_text_pages"],
    );
    let ocr = serde_json::to_value(OcrOptions::default()).unwrap();
    assert_keys(&ocr["preprocess"], &["auto_rotate", "deskew", "contrast", "denoise", "binarize", "grayscale"]);
    assert_keys(
        &serde_json::to_value(WatermarkOptions::default()).unwrap(),
        &[
            "kind",
            "text",
            "font_size_pt",
            "opacity",
            "rotation_deg",
            "position",
            "margin_pt",
            "tile",
            "image_path",
            "image_scale",
            "pages",
        ],
    );
    assert_keys(
        &serde_json::to_value(NumberingOptions::default()).unwrap(),
        &["position", "format", "start_number", "font_size_pt", "color", "margin_pt", "pages", "count_from_start"],
    );
    assert_keys(
        &serde_json::to_value(ResizeOptions::default()).unwrap(),
        &["page_size", "custom_width_pt", "custom_height_pt", "orientation", "mode", "pages"],
    );
    assert_keys(
        &serde_json::to_value(ImageToPdfOptions::default()).unwrap(),
        &["page_size", "custom_width_pt", "custom_height_pt", "orientation", "fit", "margin_pt", "dpi", "jpeg_quality"],
    );
}

#[test]
fn plans_annotations_and_results_keep_their_shapes() {
    assert_keys(
        &serde_json::to_value(PagePlanItem { source_page: 2, rotation_delta: 90, ..Default::default() }).unwrap(),
        &["source_page", "rotation_delta"],
    );

    let annotation = Annotation {
        kind: "text".into(),
        page: 1,
        x: 0.0,
        y: 0.0,
        w: 10.0,
        h: 10.0,
        text: "hi".into(),
        font_size_pt: 12.0,
        bold: false,
        color: "#000000".into(),
        opacity: 1.0,
        image_path: None,
        line_width_pt: 1.0,
        x2: None,
        y2: None,
        strokes: Vec::new(),
        image_base64: None,
    };
    assert_keys(
        &serde_json::to_value(&annotation).unwrap(),
        &["kind", "page", "font_size_pt", "image_path", "line_width_pt", "x2", "y2"],
    );

    assert_keys(
        &serde_json::to_value(SplitPart { path: "x.pdf".into(), first_page: 1, last_page: 3 }).unwrap(),
        &["path", "first_page", "last_page"],
    );

    assert_keys(
        &serde_json::to_value(ImageItem { path: "a.png".into(), rotation_delta: 90 }).unwrap(),
        &["path", "rotation_delta"],
    );

    assert_keys(
        &serde_json::to_value(ImageOutput { path: "a.jpg".into(), page: 1, width: 100, height: 200, bytes: 4096 })
            .unwrap(),
        &["path", "page", "width", "height", "bytes"],
    );

    assert_keys(
        &serde_json::to_value(CompressEstimate {
            original_bytes: 10,
            estimated_bytes: 5,
            page_count: 1,
            reduction: 0.5,
            method: "raster".into(),
            sample_pages: 1,
            accurate: true,
        })
        .unwrap(),
        &["original_bytes", "estimated_bytes", "page_count", "reduction", "method", "sample_pages", "accurate"],
    );
}

#[test]
fn pdf_to_images_result_is_camel_case() {
    // The Rust struct mixes words, so it is explicitly camelCase for the UI.
    let result = PdfToImagesResult { files: vec![], total_bytes: 1234, dpi: 150, format: "jpg".into() };
    assert_keys(&serde_json::to_value(&result).unwrap(), &["files", "totalBytes", "dpi", "format"]);
}
