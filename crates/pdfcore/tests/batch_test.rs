//! Simulated batch processing: the desktop queue applies the same operation
//! to many files; these tests exercise the underlying repeated-operation
//! stability plus per-file status accounting.

mod common;

use common::*;
use pdfcore::compress::{compress_pdf, CompressOptions};
use pdfcore::docutil::OverwritePolicy;
use pdfcore::progress::CancelToken;
use pdfcore::watermark::{add_watermark, WatermarkOptions};
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq)]
enum Status {
    Completed,
    Failed,
}

#[test]
fn batch_watermark_and_compress_all_files() {
    let dir = TestDir::new();
    let mut inputs: Vec<PathBuf> = Vec::new();
    for i in 0..6 {
        let path = dir.path(&format!("batch-{i}.pdf"));
        write_doc(&mut build_text_doc(2, &format!("batch{i}"), "batch"), &path);
        inputs.push(path);
    }

    // Batch 1: watermark every file.
    let watermark_options = WatermarkOptions {
        kind: "text".into(),
        text: "BATCH".into(),
        font_size_pt: 36.0,
        opacity: 0.3,
        rotation_deg: 30.0,
        ..Default::default()
    };
    let mut statuses = Vec::new();
    for (i, input) in inputs.iter().enumerate() {
        let out = dir.path(&format!("wm-{i}.pdf"));
        let status = match add_watermark(
            input,
            &out,
            &watermark_options,
            OverwritePolicy::Replace,
            None,
            &no_progress,
            &CancelToken::new(),
        ) {
            Ok(_) => Status::Completed,
            Err(_) => Status::Failed,
        };
        statuses.push(status);
    }
    assert!(statuses.iter().all(|s| *s == Status::Completed));

    // Batch 2: compress every file (raster is slow; use lossless here as the
    // representative operation, plus one raster pass to cover both paths).
    let lossless = CompressOptions { strategy: "lossless".into(), ..Default::default() };
    let raster = CompressOptions { strategy: "raster".into(), preset: "high".into(), ..Default::default() };
    for (i, input) in inputs.iter().enumerate() {
        let out = dir.path(&format!("cmp-{i}.pdf"));
        let options = if i % 2 == 0 { &lossless } else { &raster };
        compress_pdf(input, &out, options, OverwritePolicy::Replace, None, &no_progress, &CancelToken::new())
            .unwrap_or_else(|e| panic!("compress failed for {i}: {e}"));
        assert_eq!(page_count(&out), 2);
    }
}

#[test]
fn batch_continues_after_one_failure() {
    let dir = TestDir::new();
    let good = dir.path("good.pdf");
    write_doc(&mut build_text_doc(1, "good", "good"), &good);
    let bad = dir.path("bad.pdf");
    std::fs::write(&bad, b"%PDF broken").unwrap();

    let mut results: Vec<(PathBuf, Status)> = Vec::new();
    for (input, name) in [(bad.clone(), "bad"), (good.clone(), "good")] {
        let out = dir.path(&format!("{name}-out.pdf"));
        let status = match compress_pdf(
            &input,
            &out,
            &CompressOptions { strategy: "lossless".into(), ..Default::default() },
            OverwritePolicy::Replace,
            None,
            &no_progress,
            &CancelToken::new(),
        ) {
            Ok(_) => Status::Completed,
            Err(_) => Status::Failed,
        };
        results.push((input, status));
    }
    assert_eq!(results[0].1, Status::Failed);
    assert_eq!(results[1].1, Status::Completed);
    assert!(Path::new(&dir.path("good-out.pdf")).exists());
}
