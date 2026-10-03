//! Import benchmarks for the three office engines.
//!
//! They read the committed sample documents (`samples/`), so the numbers track
//! the real import path end to end (ZIP + hardened XML + model). The master CI
//! job runs them and uploads the criterion report as an artifact.
use std::hint::black_box;
use std::path::{Path, PathBuf};

use criterion::{criterion_group, criterion_main, Criterion};
use officecore::{docx, pptx, xlsx};

fn sample(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("samples").join(name)
}

fn import(c: &mut Criterion) {
    let docx_path = sample("test-document.docx");
    let xlsx_path = sample("test-spreadsheet.xlsx");
    let pptx_path = sample("test-presentation.pptx");

    let mut group = c.benchmark_group("office_import");
    group.bench_function("docx", |b| b.iter(|| docx::read_docx_file(black_box(&docx_path)).expect("sample docx")));
    group.bench_function("xlsx", |b| b.iter(|| xlsx::read_workbook_file(black_box(&xlsx_path)).expect("sample xlsx")));
    group.bench_function("pptx", |b| b.iter(|| pptx::read_pptx_file(black_box(&pptx_path)).expect("sample pptx")));
    group.finish();
}

criterion_group!(benches, import);
criterion_main!(benches);
