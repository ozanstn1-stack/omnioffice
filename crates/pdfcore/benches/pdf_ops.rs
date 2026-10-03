//! Engine-free PDF operation benchmarks.
//!
//! Only the lossless compression path runs here: the raster path needs pdfium,
//! which is a Windows-only binary in this repository, and the Linux CI runner
//! does not have it. The lossless path (lopdf load -> metadata strip ->
//! rewrite) is the one every compression shares, so its trend still tracks the
//! engine.
use std::hint::black_box;
use std::path::{Path, PathBuf};

use criterion::{criterion_group, criterion_main, Criterion};
use pdfcore::compress::{compress_pdf, CompressOptions};
use pdfcore::docutil::OverwritePolicy;
use pdfcore::progress::{CancelToken, ProgressEvent};

fn sample(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("samples").join(name)
}

fn lossless_compress(c: &mut Criterion) {
    let input = sample("sample-1.pdf");
    let dir = tempfile::tempdir().expect("temp dir");
    let output = dir.path().join("compressed.pdf");
    let options = CompressOptions {
        strategy: "lossless".into(),
        preset: "medium".into(),
        dpi: 150,
        jpeg_quality: 60,
        grayscale: false,
        remove_metadata: true,
    };
    let progress = |_event: ProgressEvent| {};
    c.bench_function("pdf_lossless_compress", |b| {
        b.iter(|| {
            compress_pdf(
                black_box(&input),
                black_box(&output),
                &options,
                OverwritePolicy::Replace,
                None,
                &progress,
                &CancelToken::new(),
            )
            .expect("lossless compression")
        })
    });
}

criterion_group!(benches, lossless_compress);
criterion_main!(benches);
