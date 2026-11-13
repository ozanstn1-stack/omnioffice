//! EML (RFC 5322 / MIME) parser target: exercises header decoding,
//! multipart boundary splitting, quoted-printable/base64 decoding and
//! attachment extraction on arbitrary bytes.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 16 * 1024 * 1024 {
        return;
    }
    let _ = officecore::eml::parse_eml(data);
});
