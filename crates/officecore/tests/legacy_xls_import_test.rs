//! Legacy `.xls` (BIFF8) import.
//!
//! The backend accepts the format through `calamine`; this pins it against a
//! real file so the Android open-with association (manifest + `IncomingFiles`)
//! cannot drift from a backend that stopped reading legacy Excel.
//!
//! `tests/fixtures/test-spreadsheet.xls` is `samples/test-spreadsheet.xlsx`
//! converted once with LibreOffice (`soffice --headless --convert-to xls`); the
//! expected values come from `crates/officecore/examples/make-office-samples.rs`.
//! BIFF stores formulas as records calamine does not surface, so only the last
//! cached results arrive - the importer warns about that explicitly.

use officecore::model::CellValue;
use officecore::xlsx;
use std::path::Path;

#[test]
fn legacy_xls_imports_values_and_formulas() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/test-spreadsheet.xls");
    let read = xlsx::read_workbook_file(&path).expect("a legacy .xls workbook imports");

    assert_eq!(read.workbook.sheets.len(), 2, "Data + Summary");
    let data = &read.workbook.sheets[0];
    assert_eq!(data.name, "Data");
    assert_eq!(data.get("A1").map(|cell| cell.value.clone()), Some(CellValue::Text("Name".into())));
    assert_eq!(data.get("B2").map(|cell| cell.value.clone()), Some(CellValue::Number(2.0)));
    // D2 is `=B2*C2` in the source; BIFF keeps the cached result, not the
    // formula record, so the value imports and the formula slot stays empty.
    assert_eq!(data.get("D2").map(|cell| cell.value.clone()), Some(CellValue::Number(3.0)));
    assert_eq!(data.get("D2").and_then(|cell| cell.formula.clone()), None);

    let summary = &read.workbook.sheets[1];
    assert_eq!(summary.name, "Summary");
    assert_eq!(summary.get("A1").map(|cell| cell.value.clone()), Some(CellValue::Text("Sheet overview".into())));

    // Honest warning: values only, no formatting and no formula records.
    let warning = read.warnings.iter().find(|warning| warning.starts_with("Legacy Excel (.xls)"));
    assert!(warning.is_some(), "warnings: {:?}", read.warnings);
}
