#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 256 * 1024 {
        return;
    }
    if let Ok(xml) = std::str::from_utf8(data) {
        let _ = synccore::webdav::parse_multistatus(xml);
    }
});
