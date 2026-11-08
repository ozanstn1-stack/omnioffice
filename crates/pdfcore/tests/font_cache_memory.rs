//! The font cache holds no more than its budget, measured rather than estimated.
//!
//! The cache is bounded at insert time by an estimate of what a font occupies. An
//! estimate that undercounts would let a document push the process past the cap,
//! and on Android that ends in an out-of-memory kill. This test therefore counts
//! real allocations with a global allocator, and compares the peak while imposing
//! against the peak of the same imposition with no cache at all.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use lopdf::{dictionary, Dictionary, Document, Object, Stream};
use pdfcore::nup::bench::{nup_pdf_bytes_in_mode, CacheMode};
use pdfcore::nup::NupOptions;

/// Counts live bytes and remembers the highest the process reached.
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

/// Runs `work`, returning its value and the highest live-byte count it reached.
fn peak_of<T>(work: impl FnOnce() -> T) -> (T, usize) {
    let before = LIVE.load(Ordering::Relaxed);
    PEAK.store(before, Ordering::Relaxed);
    let value = work();
    let peak = PEAK.load(Ordering::Relaxed);
    (value, peak.saturating_sub(before))
}

fn rect(rect: [f64; 4]) -> Object {
    Object::Array(rect.iter().map(|value| Object::Real(*value as f32)).collect())
}

fn utf16(text: &str) -> Object {
    let mut units = vec![0xFE, 0xFF];
    for unit in text.encode_utf16() {
        units.extend_from_slice(&unit.to_be_bytes());
    }
    Object::String(units, lopdf::StringFormat::Literal)
}

/// A `/ToUnicode` with `entries` codes, mapping the codes of `中丁`.
fn to_unicode(entries: u32) -> Vec<u8> {
    let mut body = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    body.push_str(&format!("{entries} beginbfchar\n"));
    for code in 0x0300u32..0x0300 + entries {
        body.push_str(&format!("<{code:04X}> <{code:04X}>\n"));
    }
    body.push_str("endbfchar\n3 beginbfchar\n<0003> <0020>\n<0024> <4E2D>\n<0025> <4E01>\nendbfchar\n");
    body.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    body.into_bytes()
}

/// One composite font with `entries` map entries and `widths` `/W` entries.
fn composite_font(doc: &mut Document, entries: u32, widths: usize) -> Object {
    let program = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), vec![0u8; 1024])));
    let map = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), to_unicode(entries))));
    let descriptor = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "FontDescriptor",
        "Ascent" => 880i64,
        "FontFile2" => Object::Reference(program),
    }));
    let widths: Vec<Object> = (0..widths).map(|index| Object::Integer(500 + (index % 200) as i64)).collect();
    let child = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "CIDFontType2",
        "BaseFont" => Object::Name(b"Scale".to_vec()),
        "FontDescriptor" => Object::Reference(descriptor),
        "DW" => 1000i64,
        "W" => Object::Array(vec![Object::Integer(3), Object::Array(widths)]),
    }));
    Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => Object::Name(b"Scale".to_vec()),
        "Encoding" => "Identity-H",
        "DescendantFonts" => Object::Array(vec![Object::Reference(child)]),
        "ToUnicode" => Object::Reference(map),
    })))
}

/// A document with one composite font per page, each used by `notes` notes.
fn build(fonts: usize, notes: usize, entries: u32) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let mut dr = Dictionary::new();
    let mut kids: Vec<Object> = Vec::with_capacity(fonts);
    for index in 0..fonts {
        let name = format!("F{index}");
        dr.set(name.as_str(), composite_font(&mut doc, entries, 2_000));
        let body = format!("BT /{name} 18 Tf 40 760 Td (Page {}) Tj ET", index + 1);
        let content = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), body.into_bytes())));
        let mut annots: Vec<Object> = Vec::with_capacity(notes);
        for note in 0..notes {
            let top = 700.0 - note as f64 * 40.0;
            let annot = dictionary! {
                "Type" => "Annot",
                "Subtype" => "FreeText",
                "Rect" => rect([40.0, top, 540.0, top + 30.0]),
                "F" => 4i64,
                "DA" => Object::string_literal(format!("/{name} 12 Tf 0 g").as_str()),
                "Contents" => utf16("中丁"),
            };
            annots.push(Object::Reference(doc.add_object(Object::Dictionary(annot))));
        }
        kids.push(Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => rect([0.0, 0.0, 595.0, 842.0]),
            "Contents" => Object::Reference(content),
            "Annots" => Object::Array(annots),
        }))));
    }
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => Object::Array(kids),
            "Count" => Object::Integer(fonts as i64),
        }),
    );
    let catalog = doc.new_object_id();
    doc.objects.insert(
        catalog,
        Object::Dictionary(dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) }),
    );
    doc.trailer.set("Root", Object::Reference(catalog));
    doc.catalog_mut().expect("catalog").set(
        "AcroForm",
        Object::Dictionary(dictionary! {
            "DA" => Object::string_literal("/F0 12 Tf 0 g"),
            "DR" => dictionary! { "Font" => dr },
        }),
    );
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save");
    bytes
}

/// A budget small enough that sixty-four fonts cannot all fit.
const BUDGET: usize = 4 * 1024 * 1024;

#[test]
fn the_cache_never_holds_more_than_its_budget() {
    let input = build(64, 2, 2_000);
    let options = NupOptions::default();
    let (uncached, baseline) = peak_of(|| nup_pdf_bytes_in_mode(&input, &options, CacheMode::Off).expect("uncached"));
    let (cached, peak) =
        peak_of(|| nup_pdf_bytes_in_mode(&input, &options, CacheMode::On { budget_bytes: BUDGET }).expect("cached"));
    assert_eq!(uncached, cached, "the cache does not change the bytes");
    // The cache may hold its budget; a tenth of it again is slack for the
    // bookkeeping around the entries.
    let allowed = BUDGET + BUDGET / 10;
    assert!(
        peak <= baseline + allowed,
        "the cache held {} bytes over the baseline, more than its {} byte budget",
        peak - baseline.min(peak),
        BUDGET
    );
}

#[test]
fn a_cache_that_fits_avoids_the_per_note_maps() {
    let input = build(16, 2, 2_000);
    let options = NupOptions::default();
    let (uncached, baseline) = peak_of(|| nup_pdf_bytes_in_mode(&input, &options, CacheMode::Off).expect("uncached"));
    let (cached, peak) =
        peak_of(|| nup_pdf_bytes_in_mode(&input, &options, CacheMode::On { budget_bytes: 32 << 20 }).expect("cached"));
    assert_eq!(uncached, cached);
    // Reading the map per note builds and drops one map per note; holding it once
    // is cheaper at the peak as well as in time.
    assert!(peak < baseline, "the cached run peaked at {peak}, the uncached one at {baseline}");
}
