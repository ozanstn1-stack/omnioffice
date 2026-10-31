//! Peak memory budgets for the large-document flows (D9).
//!
//! A wall-clock bound cannot catch "opening a 500-page document allocates a
//! gigabyte", and a process RSS reading is not portable across the three CI
//! operating systems. This binary therefore installs a counting global
//! allocator and measures the **peak live bytes** of each flow, then checks it
//! against a named budget. The measurements run sequentially in one test: two
//! parallel measurements would share the counters.
//!
//! The budgets are ceilings with headroom over the development measurement, so
//! the test fails on a real allocation regression (say a full second copy of
//! the document) and not on allocator noise.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

struct CountingAllocator;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = System.alloc(layout);
        if !pointer.is_null() {
            let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(live, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        System.dealloc(pointer, layout)
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// Live bytes now (drop everything measured before calling this).
fn reset_peak() -> f64 {
    PEAK.store(LIVE.load(Ordering::Relaxed), Ordering::Relaxed);
    megabyte(0)
}

fn megabyte(basis: usize) -> f64 {
    basis as f64 / (1024.0 * 1024.0)
}

fn peak_mib(basis: usize) -> f64 {
    megabyte(PEAK.load(Ordering::Relaxed).saturating_sub(basis))
}

fn writer_document(pages: usize) -> officecore::model::TextDocument {
    use officecore::model::{Block, PageSetup, TextDocument};
    let mut document = TextDocument::new_blank("Memory writer");
    document.page = PageSetup::from_preset("a4", "portrait");
    let mut blocks = Vec::with_capacity(pages * 12);
    for page in 0..pages {
        blocks.push(Block::heading(&format!("Section {page}"), 1));
        for line in 0..10 {
            blocks.push(Block::paragraph(&format!(
                "Page {page} line {line}: the quick brown fox jumps over the lazy dog while the layout engine measures this line."
            )));
        }
        blocks.push(Block::PageBreak);
    }
    document.blocks = blocks;
    document
}

#[test]
fn large_document_flows_stay_under_their_memory_budgets() {
    use officecore::docx::{read_docx, write_docx};
    use officecore::model::{Cell, CellValue, Sheet, Workbook};
    use officecore::xlsx::{read_workbook_bytes, write_xlsx};

    // Writer: a 500-page document, the size the product promises to open.
    let baseline = LIVE.load(Ordering::Relaxed);
    let mut peak_docx = reset_peak();
    let document = writer_document(500);
    peak_docx = peak_mib(baseline).max(peak_docx);
    let bytes = write_docx(&document).expect("write DOCX");
    let peak_docx = peak_docx.max(peak_mib(baseline));
    drop(document);
    let read = read_docx(&bytes).expect("read DOCX");
    let peak_docx = peak_docx.max(peak_mib(baseline));
    drop(read);
    drop(bytes);
    let docx_budget = 384.0;
    println!("memory: 500-page DOCX open/save peak {peak_docx:.1} MiB (budget {docx_budget:.0} MiB)");
    assert!(peak_docx < docx_budget, "500-page DOCX peak {peak_docx:.1} MiB, budget {docx_budget:.0} MiB");

    // Calc: 50 000 cells with 5 000 formulas in one workbook.
    let baseline = LIVE.load(Ordering::Relaxed);
    let mut workbook = Workbook::new_blank("Memory calc");
    workbook.sheets.clear();
    let mut sheet = Sheet::new("Perf");
    for row in 0..50_000u32 {
        let address = officecore::address::format(row, 0);
        sheet.set(&address, Cell { value: CellValue::Number((row + 1) as f64), ..Default::default() });
    }
    for row in 0..5_000u32 {
        let address = officecore::address::format(row, 1);
        sheet.set(
            &address,
            Cell {
                value: CellValue::Number(((row + 1) * 2) as f64),
                formula: Some(format!("=A{}*2", row + 1)),
                ..Default::default()
            },
        );
    }
    workbook.sheets.push(sheet);
    let peak_calc = peak_mib(baseline);
    let bytes = write_xlsx(&workbook).expect("write XLSX");
    let peak_calc = peak_calc.max(peak_mib(baseline));
    drop(workbook);
    let read = read_workbook_bytes(&bytes).expect("read XLSX");
    let peak_calc = peak_calc.max(peak_mib(baseline));
    drop(read);
    drop(bytes);
    let calc_budget = 256.0;
    println!("memory: 50k-cell XLSX open/save peak {peak_calc:.1} MiB (budget {calc_budget:.0} MiB)");
    assert!(peak_calc < calc_budget, "50k-cell XLSX peak {peak_calc:.1} MiB, budget {calc_budget:.0} MiB");
}
