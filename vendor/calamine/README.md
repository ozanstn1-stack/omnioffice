# Vendored calamine 0.37.0 (two fixes)

Nightly fuzzing found two panics in upstream 0.37.0; cargo-fuzz builds with
overflow checks, so each panic aborts the fuzz target even though the app's
worker would catch it. Both fixes are arithmetic-only: parsing behaviour for
well-formed files is unchanged.

1. **Cell-reference overflow** (`cell_ref_to_row_col`, `src/xlsx/mod.rs`):
   `col * 26` / `row * 10` overflowed on a hostile reference (a 7+ letter
   column or a giant row number). It now uses `checked_mul`/`checked_add` and
   returns `XlsxError::Alphanumeric` for an impossible reference.
2. **Reversed dimension underflow** (`get_dimension`, `src/xlsx/mod.rs`): a
   worksheet with `<dimension ref="O1:C4"/>` made `end - start` (and
   `Dimensions::len`, which repeats the arithmetic) subtract past zero. A
   reversed reference is normalized to its bounding box, and the table
   header/totals row offsets use `saturating_add`/`saturating_sub`.

No other file is modified. The crash inputs live in `fuzz/corpus/xlsx/` and the
regression tests are `crates/officecore/tests/calamine_dimension_test.rs` (plus
the cell-reference guards already covered by the xlsx reader tests).

Remove this directory and the `[patch.crates-io]` entries when calamine ships
the same guards upstream.
