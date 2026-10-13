//! Fuzz the pure-Rust PDF parser (lopdf) that pdfcore builds on: the xref,
//! object streams, the page tree and the catalog. Engine-backed paths
//! (pdfium render/text) are covered by the engine-gated tests; this target
//! exercises the parser that runs before any engine call, on arbitrary bytes.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(doc) = lopdf::Document::load_mem(data) {
        let pages = doc.get_pages();
        for (_, page_id) in pages.iter().take(64) {
            if let Ok(page) = doc.get_object(*page_id).and_then(|object| object.as_dict()) {
                let _ = page.get(b"Contents");
                let _ = page.get(b"Resources");
                let _ = page.get(b"MediaBox");
            }
        }
        let _ = doc.catalog();
    }
});
