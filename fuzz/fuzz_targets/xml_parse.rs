//! Fuzz the depth-limited XML parser used by every OOXML/ODF part.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let _ = officecore::xml::parse_xml(&text);
});
