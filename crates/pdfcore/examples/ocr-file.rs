//! Development helper: runs the OCR pipeline on a file so the same code path
//! as the UI can be verified from the command line.
//!
//! Usage: cargo run -p pdfcore --example ocr-file -- <input.pdf> <output.pdf> [lang]

use pdfcore::docutil::OverwritePolicy;
use pdfcore::ocr::{ocr_pdf, OcrOptions, OcrPreprocess};
use pdfcore::progress::CancelToken;
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let input = args.get(1).expect("input path");
    let output = args.get(2).expect("output path");
    let language = args.get(3).cloned().unwrap_or_else(|| "eng".to_string());

    println!("tesseract available: {}", pdfcore::ocr::tesseract_available());
    println!("engine status: {:?}", pdfcore::engines::engine_status());

    // Mirror the UI defaults: 300 DPI with image cleanup enabled.
    let options = OcrOptions {
        languages: vec![language],
        output_mode: "searchable_pdf".into(),
        dpi: 300,
        skip_text_pages: true,
        preprocess: OcrPreprocess {
            auto_rotate: true,
            deskew: true,
            contrast: true,
            denoise: false,
            binarize: false,
            grayscale: false,
        },
        ..Default::default()
    };
    let progress = |event: pdfcore::progress::ProgressEvent| {
        println!("  progress: {} {}/{}", event.stage, event.current, event.total);
    };
    let cancel = CancelToken::new();
    match ocr_pdf(Path::new(input), Path::new(output), &options, OverwritePolicy::Replace, None, &progress, &cancel) {
        Ok(result) => {
            println!(
                "ok: {} pages processed, {} skipped, {} chars, {} ms -> {}",
                result.pages_processed, result.pages_skipped, result.characters, result.duration_ms, result.path
            );
            // Verify the text layer really exists.
            if let Ok(text) = pdfcore::render::extract_page_text(Path::new(&result.path), None, 1) {
                println!("page 1 text: {:?}", text.chars().take(90).collect::<String>());
            }
        }
        Err(err) => {
            eprintln!("OCR failed: {err:?}");
            std::process::exit(1);
        }
    }
    let _ = OcrPreprocess::default();
}
