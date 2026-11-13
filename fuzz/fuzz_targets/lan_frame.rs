//! LAN sync wire frame decoder target: exercises length-prefixed JSON
//! header and bounded body framing on arbitrary bytes.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 16 * 1024 * 1024 {
        return;
    }
    let _ = synccore::lan::parse_frame_bytes(data);
});
