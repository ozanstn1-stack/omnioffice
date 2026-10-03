//! Fuzz the hardened ZIP reader that every OOXML/ODF container goes through:
//! entry limits, CRC checks, duplicate names, lying headers and ratio bombs.
#![no_main]

use libfuzzer_sys::fuzz_target;
use officecore::zip::{ZipLimits, ZipReader};

fuzz_target!(|data: &[u8]| {
    if data.len() > 16 * 1024 * 1024 {
        return;
    }
    let Ok(archive) = ZipReader::open(data.to_vec()) else {
        return;
    };
    let _ = archive.read_all(ZipLimits::default());
});
