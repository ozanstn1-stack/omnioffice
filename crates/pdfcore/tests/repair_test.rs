//! qpdf-backed repair and linearization tests.
//!
//! The engine is a bundled Windows binary; on runners without it (Linux CI
//! --lib/integration runs) the tests report the skip and return instead of
//! failing.

mod common;

use common::{build_text_doc, no_progress, page_count, page_text, write_doc, TestDir};
use pdfcore::progress::CancelToken;

fn qpdf_available() -> bool {
    pdfcore::engines::qpdf_path().is_some()
}

/// Breaks the trailing cross-reference structure, which is the classic "PDF
/// will not open after a bad download" damage: the objects are intact, the
/// trailer/xref are not.
fn break_cross_reference(path: &std::path::Path) {
    let bytes = std::fs::read(path).expect("read input");
    // Byte search, not a lossy string: binary streams make the byte offsets of
    // a String::from_utf8_lossy copy differ from the file.
    let marker =
        bytes.windows(b"startxref".len()).rposition(|window| window == b"startxref").expect("startxref in a saved PDF");
    // Drop the trailer, the startxref offset and %%EOF, and scramble the xref
    // table keyword before that.
    let mut output = bytes[..marker].to_vec();
    if let Some(xref) = output.windows(b"\nxref".len()).rposition(|window| window == b"\nxref") {
        for byte in output.iter_mut().skip(xref + 1).take(4) {
            *byte = b'x';
        }
    }
    std::fs::write(path, output).expect("write damaged input");
}

#[test]
fn repair_rewrites_a_damaged_pdf() {
    if !qpdf_available() {
        eprintln!("skipping: qpdf engine not available");
        return;
    }
    let dir = TestDir::new();
    let input = dir.path("input.pdf");
    let output = dir.path("repaired.pdf");
    let mut doc = build_text_doc(2, "Repair me", "Repair");
    write_doc(&mut doc, &input);
    break_cross_reference(&input);

    // The damaged file must actually be unreadable by the simple parser,
    // otherwise the test would pass without exercising qpdf.
    assert!(lopdf::Document::load(&input).is_err(), "the damaged input should not load");

    let report =
        pdfcore::repair::repair_pdf(&input, &output, &no_progress, &CancelToken::new()).expect("repair succeeds");
    assert!(output.is_file());
    assert_eq!(report.pages, 2);
    assert_eq!(page_count(&output), 2);
    assert!(page_text(&output, 1).contains("Repair me page 1"), "text survived the repair");
}

#[test]
fn linearize_writes_a_fast_web_view_file() {
    if !qpdf_available() {
        eprintln!("skipping: qpdf engine not available");
        return;
    }
    let dir = TestDir::new();
    let input = dir.path("input.pdf");
    let output = dir.path("linearized.pdf");
    let mut doc = build_text_doc(3, "Linear", "Linearize");
    write_doc(&mut doc, &input);

    let report =
        pdfcore::repair::linearize_pdf(&input, &output, &no_progress, &CancelToken::new()).expect("linearize succeeds");
    assert_eq!(report.pages, 3);
    assert_eq!(page_count(&output), 3);
    // qpdf's linearized output carries the /Linearized parameter dictionary.
    let bytes = std::fs::read(&output).expect("read linearized");
    assert!(bytes.windows(b"/Linearized".len()).any(|window| window == b"/Linearized"));
}
