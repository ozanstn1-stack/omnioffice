//! PDF signature & PKCS#12 verification fuzz target (E2.5): exercises pure-Rust
//! CMS/DER signature verification and PKCS#12 container parsing on arbitrary
//! bytes with no network calls.
#![no_main]

use libfuzzer_sys::fuzz_target;
use pdfcore::sign::{parse_pkcs12, verify_signatures};

fuzz_target!(|data: &[u8]| {
    if data.len() > 2 * 1024 * 1024 {
        return;
    }
    let _ = verify_signatures(data);
    let _ = parse_pkcs12(data, "");
    if data.len() >= 4 {
        let split = ((data[0] as usize) % (data.len() - 1)).clamp(1, 64);
        let password = String::from_utf8_lossy(&data[1..split]);
        let _ = parse_pkcs12(&data[split..], &password);
    }
});
