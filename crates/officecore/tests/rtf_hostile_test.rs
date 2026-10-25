//! RTF reader robustness: hostile control-word parameters must not abort the
//! parser. Regression for the 2026-10-25 nightly-fuzz crash, where an unbounded
//! digit run overflowed the parameter accumulator in `crates/officecore/src/rtf.rs`.

use officecore::rtf::read_rtf;

#[test]
fn clamps_an_unbounded_control_word_parameter() {
    let mut rtf = b"{\\rtf1\\ansi\\fs".to_vec();
    rtf.extend(std::iter::repeat_n(b'9', 400));
    rtf.extend_from_slice(b" Hello\\par}");
    let read = read_rtf(&rtf).expect("a hostile parameter must not abort the reader");
    assert!(read.document.plain_text().contains("Hello"));
}
