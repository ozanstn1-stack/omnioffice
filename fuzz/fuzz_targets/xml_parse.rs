//! Fuzz the depth-limited XML parser used by every OOXML/ODF part.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 16 * 1024 * 1024 {
        return;
    }
    let text = String::from_utf8_lossy(data);
    let _ = officecore::xml::parse_xml(&text);
});
