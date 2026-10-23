# Vendored calamine 0.37.0 (one fix)

Upstream panics on a hostile cell reference: `col * 26` / `row * 10` overflow
(a 7+ letter column or a giant row in a worksheet). The fuzzer found it in
`office_readers` (`attempt to multiply with overflow` in
`src/xlsx/mod.rs`), and cargo-fuzz builds with overflow checks, so the panic
aborts the fuzz target even though the app's worker would catch it.

The only change from crates.io 0.37.0 is `checked_mul`/`checked_add` in
`cell_ref_to_row_col` (`src/xlsx/mod.rs`), returning `XlsxError::Alphanumeric`
on an impossible reference. No other file is modified.

Remove this directory and the `[patch.crates-io]` entries when calamine ships
the same guard upstream.
