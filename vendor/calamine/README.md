# Vendored calamine 0.38.0 (two fixes)

0.38.0 fixes RUSTSEC-2026-0436: the CFB/OLE2 parser (`src/cfb.rs`, used for
`.xls`, `.xlsb` and VBA extraction) could panic or allocate without bound on a
crafted file, reachable through `open_workbook`/`Xls::new`/`Xlsb::new` - a
denial of service on a file a user opens. This copy exists to carry that fix
together with the two arithmetic fixes 0.37 needed, which upstream still does
not have:

1. **Cell-reference overflow** (`get_row_and_optional_column`, `src/xlsx/mod.rs`):
   `col * 26` / `row * 10` overflowed on a hostile reference (a 7+ letter column
   or a giant row number). It now uses `checked_mul`/`checked_add` and returns
   `XlsxError::Alphanumeric` for an impossible reference.
2. **Reversed dimension underflow** (`get_dimension`, `src/xlsx/mod.rs`): a
   worksheet with `<dimension ref="O1:C4"/>` made `end - start` (and
   `Dimensions::len`, which repeats the arithmetic) subtract past zero. A
   reversed reference is normalized to its bounding box before it is returned.

Nightly fuzzing found both in 0.37 (cargo-fuzz builds with overflow checks, so
each panic aborted the fuzz target even though the app's worker would catch it).
Both fixes are arithmetic-only: parsing behaviour for well-formed files is
unchanged.

The crash inputs live in `fuzz/corpus/xlsx/`; the regression tests are
`crates/officecore/tests/calamine_dimension_test.rs` plus the OLE2 case in
`crates/officecore/tests/calamine_hostile_input.rs`.

Upstream releases to watch: drop this directory once a release carries the two
fixes above (check `get_dimension` and `get_row_and_optional_column`).
