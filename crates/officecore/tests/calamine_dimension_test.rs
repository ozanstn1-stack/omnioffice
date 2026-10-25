//! Regression for the nightly-fuzz crash found on `master` (2026-10-25):
//! an XLSX worksheet whose `<dimension ref>` is reversed (`O1:C4`) made
//! `calamine::get_dimension` subtract past zero (`end - start`), aborting the
//! reader in overflow-checked builds (the fuzz profile).
//!
//! The fixture is the exact fuzz artifact. The test talks to the vendored
//! `calamine` directly - `officecore` wraps its own parse in `catch_unwind`,
//! which would hide the panic this test exists to prevent.
//!
//! `vendor/calamine` is a patched copy (see `vendor/calamine/README.md`); the
//! same crash input is kept in `fuzz/corpus/xlsx/` so the fuzzer cannot lose it.

use calamine::{Reader, Xlsx};
use std::io::Cursor;

#[test]
fn a_reversed_dimension_does_not_overflow() {
    let bytes: &[u8] = include_bytes!("fixtures/calamine-dimension-overflow.xlsx");
    let mut workbook = Xlsx::new(Cursor::new(bytes)).expect("the container still opens");
    // Reading the sheet parses `<dimension ref="O1:C4"/>`; the old code
    // underflowed here (overflow-checks builds panic, fuzz reports a crash).
    let _ = workbook.worksheet_range_at(0);
    assert!(!workbook.sheet_names().is_empty());
}
