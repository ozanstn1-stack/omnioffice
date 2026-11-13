//! PDF rebuild/repair fuzz target (E2.5): exercises the built-in pure-Rust PDF
//! reconstructor on damaged, truncated, or adversarial byte streams.
#![no_main]

use libfuzzer_sys::fuzz_target;
use pdfcore::progress::CancelToken;
use pdfcore::rebuild::rebuild_pdf;

fuzz_target!(|data: &[u8]| {
    if data.len() > 2 * 1024 * 1024 {
        return;
    }
    let cancel = CancelToken::new();
    let _ = rebuild_pdf(data, &cancel);
});
