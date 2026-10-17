//! UDF (UYAP) reader target. UDF is a text container, so mutations exercise
//! the field/paragraph parser directly; the corpus seed keeps the fuzzer in
//! valid territory. Errors are the expected outcome; the point is that no
//! input panics or hangs.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 16 * 1024 * 1024 {
        return;
    }
    let _ = officecore::udf::read_udf(data);
});
