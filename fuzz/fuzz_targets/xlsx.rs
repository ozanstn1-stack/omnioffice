//! Dedicated XLSX reader target: the corpus seed gets the fuzzer past the ZIP
//! layer, where mutations exercise the spreadsheet parts (shared strings,
//! styles, formulas). Errors are the expected outcome; the point is that no
//! input panics, hangs or escapes the hardened ZIP/XML layers.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 16 * 1024 * 1024 {
        return;
    }
    let _ = officecore::xlsx::read_workbook_bytes(data);
});
