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

use lopdf::{dictionary, Dictionary, Document, Object, Stream};
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

fn rect(rect: [f64; 4]) -> Object {
    Object::Array(rect.iter().map(|value| Object::Real(*value as f32)).collect())
}

/// A note whose text is UTF-16BE, as a producer writes CJK.
fn utf16(text: &str) -> Object {
    let mut units = vec![0xFE, 0xFF];
    for unit in text.encode_utf16() {
        units.extend_from_slice(&unit.to_be_bytes());
    }
    Object::String(units, lopdf::StringFormat::Literal)
}

/// How large a composite font to build: a real CJK font carries thousands of
/// entries, and the lookups have to stay fast at that size.
struct FontSize {
    to_unicode: u32,
    widths: usize,
    glyph_map: bool,
}

impl Default for FontSize {
    fn default() -> Self {
        Self { to_unicode: 1_000, widths: 2_000, glyph_map: false }
    }
}

/// One composite font: Identity-H, an embedded program, `/W` and `/ToUnicode`
/// for the two ideographs the notes use.
fn composite_font(doc: &mut Document, size: &FontSize) -> Object {
    let program = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), vec![0u8; 4096])));
    let map = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), to_unicode_entries(size.to_unicode))));
    let descriptor = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "FontDescriptor",
        "Ascent" => 880i64,
        "FontFile2" => Object::Reference(program),
    }));
    let widths: Vec<Object> = (0..size.widths).map(|index| Object::Integer(500 + (index % 200) as i64)).collect();
    let mut child = dictionary! {
        "Type" => "Font",
        "Subtype" => "CIDFontType2",
        "BaseFont" => Object::Name(b"Bench".to_vec()),
        "FontDescriptor" => Object::Reference(descriptor),
        "DW" => 1000i64,
        "W" => Object::Array(vec![Object::Integer(3), Object::Array(widths)]),
    };
    if size.glyph_map {
        // Two bytes per CID, with every glyph present.
        let mut table = vec![0u8; 65_536 * 2];
        for cid in 0u16..65_535 {
            table[usize::from(cid) * 2 + 1] = (cid % 251) as u8 | 1;
        }
        let stream = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), table)));
        child.set("CIDToGIDMap", Object::Reference(stream));
    }
    let child = doc.add_object(Object::Dictionary(child));
    Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => Object::Name(b"Bench".to_vec()),
        "Encoding" => "Identity-H",
        "DescendantFonts" => Object::Array(vec![Object::Reference(child)]),
        "ToUnicode" => Object::Reference(map),
    })))
}

/// A `/ToUnicode` with `entries` codes, ending with the four the notes use.
fn to_unicode_entries(entries: u32) -> Vec<u8> {
    let mut body = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    body.push_str(&format!("{entries} beginbfchar\n"));
    for code in 0x0300u32..0x0300 + entries {
        body.push_str(&format!("<{code:04X}> <{code:04X}>\n"));
    }
    body.push_str("endbfchar\n4 beginbfchar\n<0003> <0020>\n<0024> <4E2D>\n<0025> <4E01>\n<0026> <4E09>\nendbfchar\n");
    body.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    body.into_bytes()
}

/// One simple font with 224 widths and a `/Differences` list.
fn simple_font(doc: &mut Document) -> Object {
    let descriptor = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "FontDescriptor",
        "Ascent" => 700i64,
        "MissingWidth" => 0i64,
    }));
    let widths: Vec<Object> = (0..224).map(|index| Object::Integer(400 + index % 300)).collect();
    Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "TrueType",
        "BaseFont" => Object::Name(b"Bench-Simple".to_vec()),
        "FirstChar" => 32i64,
        "Widths" => Object::Array(widths),
        "Encoding" => "WinAnsiEncoding",
        "FontDescriptor" => Object::Reference(descriptor),
    })))
}

/// A document with `pages` pages, `notes` notes on each, sharing one font.
fn build(pages: usize, notes: usize, kind: &str) -> Vec<u8> {
    let size = match kind {
        "composite-large" => FontSize { to_unicode: 20_000, widths: 4_000, glyph_map: true },
        _ => FontSize::default(),
    };
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let composite = kind != "simple";
    let mut fonts = Dictionary::new();
    if composite {
        fonts.set("F1", composite_font(&mut doc, &size));
    } else {
        fonts.set("F1", simple_font(&mut doc));
    }
    let page_font = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    }));
    let mut kids: Vec<Object> = Vec::with_capacity(pages);
    for index in 0..pages {
        let body = format!("BT /F1 18 Tf 40 760 Td (Page {}) Tj ET", index + 1);
        let content = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), body.into_bytes())));
        let mut annots: Vec<Object> = Vec::with_capacity(notes);
        for note in 0..notes {
            let top = 700.0 - note as f64 * 40.0;
            let mut annot = dictionary! {
                "Type" => "Annot",
                "Subtype" => "FreeText",
                "Rect" => rect([40.0, top, 540.0, top + 30.0]),
                "F" => 4i64,
                "DA" => Object::string_literal("/F1 12 Tf 0 g"),
            };
            if composite {
                annot.set("Contents", utf16("中丁三"));
            } else {
                annot.set("Contents", Object::string_literal("A note with some words"));
            }
            annots.push(Object::Reference(doc.add_object(Object::Dictionary(annot))));
        }
        let page = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => rect([0.0, 0.0, 595.0, 842.0]),
            "Resources" => dictionary! { "Font" => dictionary! { "F0" => Object::Reference(page_font) } },
            "Contents" => Object::Reference(content),
            "Annots" => Object::Array(annots),
        }));
        kids.push(Object::Reference(page));
    }
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => Object::Array(kids),
            "Count" => Object::Integer(pages as i64),
        }),
    );
    let catalog = doc.new_object_id();
    doc.objects.insert(
        catalog,
        Object::Dictionary(dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) }),
    );
    doc.trailer.set("Root", Object::Reference(catalog));
    // The form's `/DR`, where the note's `/DA` font is found.
    let form = dictionary! {
        "DA" => Object::string_literal("/F1 12 Tf 0 g"),
        "DR" => dictionary! { "Font" => fonts },
    };
    doc.catalog_mut().expect("catalog").set("AcroForm", Object::Dictionary(form));
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save");
    bytes
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
