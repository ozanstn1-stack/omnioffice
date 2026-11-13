//! ODF container reader target: exercises ODT, ODS and ODP import on
//! arbitrary ZIP-shaped or raw bytes without panicking or hanging.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 16 * 1024 * 1024 {
        return;
    }
    let _ = officecore::odf::read_odt(data);
    let _ = officecore::odf::read_ods(data);
    let _ = officecore::odf::read_odp(data);
});
