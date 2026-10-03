//! Route arbitrary bytes to the container readers: a ZIP magic goes through
//! DOCX, XLSX, PPTX and ODT import, an RTF header through the RTF reader.
//! Errors are the expected outcome; the point is that no input panics, hangs
//! or escapes the hardened ZIP/XML layers.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 16 * 1024 * 1024 {
        return;
    }
    if data.starts_with(b"{\\rtf") {
        let _ = officecore::rtf::read_rtf(data);
        return;
    }
    if data.starts_with(b"PK") {
        let _ = officecore::docx::read_docx(data);
        let _ = officecore::xlsx::read_workbook_bytes(data);
        let _ = officecore::pptx::read_pptx(data);
        let _ = officecore::odf::read_odt(data);
    }
});
