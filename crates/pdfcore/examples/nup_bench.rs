//! A wall-clock measurement of N-up imposition with the font cache on and off.
//!
//! The cache is per document: one read of a font's `/ToUnicode`, `/Encoding`,
//! `/CIDToGIDMap` and `/W` serves every note that uses it. A budget of zero
//! stores nothing, which is exactly how the code behaved before the cache, so one
//! binary can compare both modes on the same document.
//!
//! ```text
//! cargo run --release -p pdfcore --example nup_bench -- --pages 500 --notes 4 --runs 5
//! ```

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

#[path = "nup_bench/fixture.rs"]
mod fixture;

use fixture::build;
use pdfcore::nup::{bench::nup_pdf_bytes_with_font_cache, NupOptions, DEFAULT_FONT_CACHE_BYTES};

/// Counts live bytes so a run can report its peak as well as its time.
struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(live, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// What to measure; the defaults describe a large annotated document.
struct Args {
    pages: usize,
    notes: usize,
    runs: usize,
    fonts: String,
}

fn parse_args() -> Args {
    let mut args = Args { pages: 500, notes: 4, runs: 5, fonts: "all".to_string() };
    let mut items = std::env::args().skip(1);
    while let Some(flag) = items.next() {
        let mut value = || items.next().expect("a value after the flag");
        match flag.as_str() {
            "--pages" => args.pages = value().parse().expect("a page count"),
            "--notes" => args.notes = value().parse().expect("a note count"),
            "--runs" => args.runs = value().parse().expect("a run count"),
            "--fonts" => args.fonts = value(),
            other => panic!("unknown flag {other}"),
        }
    }
    args
}

fn median(mut times: Vec<Duration>) -> Duration {
    times.sort();
    times[times.len() / 2]
}

fn main() {
    let args = parse_args();
    let kinds: Vec<&str> = match args.fonts.as_str() {
        "mixed" => vec!["composite", "simple"],
        "all" => vec!["composite", "composite-large", "simple"],
        other => vec![other],
    };
    let options = NupOptions::default();
    for kind in kinds {
        let input = build(args.pages, args.notes, kind);
        let mut cold = Vec::with_capacity(args.runs);
        let mut warm = Vec::with_capacity(args.runs);
        let mut cold_bytes = Vec::new();
        let mut warm_bytes = Vec::new();
        for _ in 0..args.runs {
            let start = Instant::now();
            cold_bytes = nup_pdf_bytes_with_font_cache(&input, &options, 0).expect("uncached");
            cold.push(start.elapsed());
            let start = Instant::now();
            warm_bytes = nup_pdf_bytes_with_font_cache(&input, &options, DEFAULT_FONT_CACHE_BYTES).expect("cached");
            warm.push(start.elapsed());
        }
        if cold_bytes != warm_bytes {
            eprintln!("{kind}: the cached output differs from the uncached one");
            std::process::exit(1);
        }
        let notes = args.pages * args.notes;
        let peak = PEAK.load(Ordering::Relaxed);
        println!(
            "{kind}: {} pages x {} notes = {notes} notes\n  cache off: {:?} (median)\n  cache on:  {:?} (median)\n  peak heap: {:.1} MiB",
            args.pages,
            args.notes,
            median(cold),
            median(warm),
            peak as f64 / (1024.0 * 1024.0),
        );
    }
}
