//! UBL (e-Fatura) reader target. A single invoice goes through `read_ubl`;
//! arbitrary bytes additionally go through `read_ubl_package`, which treats
//! the input as a ZIP of invoices and exercises the archive layer too. Errors
//! are the expected outcome; the point is that no input panics or hangs.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 16 * 1024 * 1024 {
        return;
    }
    let _ = officecore::ubl::read_ubl(data);
    let _ = officecore::ubl::read_ubl_package(data);
});
