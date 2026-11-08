//! The CMap readers take producer-written PostScript out of an untrusted file, so it gets the same treatment as the other parsers: any
//! bytes, no panic, no unbounded allocation.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 1 << 20 {
        return;
    }
    let map = pdfcore::cmap::read(data);
    // Exercising the readers too keeps the entry points hardened, not just the
    // tokenizer.
    let _ = map.len();
    let _ = map.covers("東京都 A");
    let _ = map.encode("東京都 A");
    // The `/Encoding` reader shares the tokenizer; the same bytes go through it
    // with and without an identity base.
    let _ = pdfcore::cmap::fuzz_cid(data, false);
    let _ = pdfcore::cmap::fuzz_cid(data, true);
});
