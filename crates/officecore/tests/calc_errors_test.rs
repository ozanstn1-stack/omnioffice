//! Error values in XLSX, ODS and CSV, including `#CALC!`.
//!
//! The formula engine returns `#CALC!` for an empty array (Excel 365's name for
//! it) next to `#SPILL!` and the classic codes. Excel stores them as `t="e"`
//! cells; its newer codes are not in the table `calamine` parses, and an error
//! it does not know used to fail the whole sheet. Nothing here may panic or
//! drop the values around an error.

use officecore::model::*;
use officecore::zip::{ZipLimits, ZipReader, ZipWriter};
use officecore::{csvio, odf, xlsx};

fn error_cell(text: &str, formula: Option<&str>) -> Cell {
    Cell { value: CellValue::Error(text.into()), formula: formula.map(str::to_string), ..Default::default() }
}

fn workbook_with_errors() -> Workbook {
    let mut workbook = Workbook::new_blank("Errors");
    let sheet = &mut workbook.sheets[0];
    sheet.name = "Data".into();
    sheet.set("A1", Cell { value: CellValue::Number(5.0), ..Default::default() });
    sheet.set("B1", error_cell("#CALC!", Some("=FILTER(A1:A3,A1:A3>100)")));
    sheet.set("B2", error_cell("#SPILL!", Some("=SEQUENCE(3)")));
    sheet.set("B3", error_cell("#DIV/0!", Some("=1/0")));
    sheet.set("B4", error_cell("#N/A", Some("=NA()")));
    sheet.set("B5", error_cell("#NAME?", None));
    sheet.set("B6", error_cell("#NUM!", None));
    sheet.set("B7", error_cell("#REF!", None));
    sheet.set("B8", error_cell("#VALUE!", None));
    sheet.set("B9", error_cell("#NULL!", None));
    workbook
}

fn worksheet(bytes: &[u8]) -> String {
    ZipReader::open(bytes.to_vec()).unwrap().read_text("xl/worksheets/sheet1.xml").unwrap()
}

fn error_at(workbook: &Workbook, address: &str) -> Option<String> {
    match workbook.sheets[0].get(address).map(|cell| cell.value.clone()) {
        Some(CellValue::Error(text)) => Some(text),
        _ => None,
    }
}

#[test]
fn xlsx_writes_calc_as_an_error_cell() {
    let bytes = xlsx::write_xlsx(&workbook_with_errors()).unwrap();
    let sheet = worksheet(&bytes);
    assert!(
        sheet.contains("t=\"e\"><f>_xlfn._xlws.FILTER(A1:A3,A1:A3&gt;100)</f><v>#CALC!</v></c>"),
        "an empty array must be a #CALC! error cell: {sheet}"
    );
    assert!(sheet.contains("t=\"e\"><f>_xlfn.SEQUENCE(3)</f><v>#SPILL!</v></c>"), "{sheet}");
    assert!(sheet.contains("<v>#DIV/0!</v>"));
}

#[test]
fn xlsx_round_trip_keeps_every_error_and_its_formula() {
    let bytes = xlsx::write_xlsx(&workbook_with_errors()).unwrap();
    let read = xlsx::read_workbook_bytes(&bytes).unwrap();
    let sheet = &read.workbook.sheets[0];
    for (address, text) in [
        ("B1", "#CALC!"),
        ("B2", "#SPILL!"),
        ("B3", "#DIV/0!"),
        ("B4", "#N/A"),
        ("B5", "#NAME?"),
        ("B6", "#NUM!"),
        ("B7", "#REF!"),
        ("B8", "#VALUE!"),
        ("B9", "#NULL!"),
    ] {
        assert_eq!(error_at(&read.workbook, address).as_deref(), Some(text), "{address}");
    }
    assert_eq!(sheet.get("B1").and_then(|cell| cell.formula.as_deref()), Some("=FILTER(A1:A3,A1:A3>100)"));
    assert_eq!(sheet.get("B2").and_then(|cell| cell.formula.as_deref()), Some("=SEQUENCE(3)"));
    assert_eq!(sheet.get("A1").map(|cell| cell.value.clone()), Some(CellValue::Number(5.0)));
}

/// What Excel 365 writes: error cells with codes older parsers do not list.
const EXCEL_ERRORS: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n",
    "<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><dimension ref=\"A1:B9\"/><sheetData>",
    "<row r=\"1\"><c r=\"A1\"><v>5</v></c><c r=\"B1\" t=\"e\"><f>_xlfn._xlws.FILTER(A1:A3,A1:A3&gt;100)</f><v>#CALC!</v></c></row>",
    "<row r=\"2\"><c r=\"A2\"><v>6</v></c><c r=\"B2\" t=\"e\"><f>_xlfn.SEQUENCE(3)</f><v>#SPILL!</v></c></row>",
    "<row r=\"3\"><c r=\"A3\"><v>7</v></c><c r=\"B3\" t=\"e\"><f>1/0</f><v>#DIV/0!</v></c></row>",
    "<row r=\"4\"><c r=\"A4\"><v>8</v></c><c r=\"B4\" t=\"e\"><f>NA()</f><v>#N/A</v></c></row>",
    "<row r=\"5\"><c r=\"A5\"><v>9</v></c><c r=\"B5\" t=\"e\"><v>#NAME?</v></c></row>",
    "<row r=\"6\"><c r=\"B6\" t=\"e\"><v>#NULL!</v></c></row>",
    "<row r=\"7\"><c r=\"B7\" t=\"e\"><v>#GETTING_DATA</v></c></row>",
    "<row r=\"8\"><c r=\"B8\" t=\"e\"><v>#UNKNOWN!</v></c></row>",
    "<row r=\"9\"><c r=\"A9\"><v>10</v></c><c r=\"B9\" t=\"e\"><v>#FIELD!</v></c><c r=\"C9\"><v>11</v></c></row>",
    "</sheetData></worksheet>",
);

fn with_sheet(bytes: &[u8], sheet: &str) -> Vec<u8> {
    let reader = ZipReader::open(bytes.to_vec()).unwrap();
    let mut writer = ZipWriter::new();
    for (name, data) in reader.read_all(ZipLimits::default()).unwrap() {
        if name == "xl/worksheets/sheet1.xml" {
            writer.add_text(&name, sheet);
        } else {
            writer.add(&name, &data);
        }
    }
    writer.finish()
}

#[test]
fn xlsx_import_reads_the_error_codes_excel_writes() {
    let package = with_sheet(&xlsx::write_xlsx(&workbook_with_errors()).unwrap(), EXCEL_ERRORS);
    let read = xlsx::read_workbook_bytes(&package).expect("an unknown error code must not fail the import");
    for (address, text) in [
        ("B1", "#CALC!"),
        ("B2", "#SPILL!"),
        ("B3", "#DIV/0!"),
        ("B4", "#N/A"),
        ("B5", "#NAME?"),
        ("B6", "#NULL!"),
        ("B7", "#GETTING_DATA"),
        ("B8", "#UNKNOWN!"),
        ("B9", "#FIELD!"),
    ] {
        assert_eq!(error_at(&read.workbook, address).as_deref(), Some(text), "{address}");
    }
    let sheet = &read.workbook.sheets[0];
    assert_eq!(sheet.get("B1").and_then(|cell| cell.formula.as_deref()), Some("=FILTER(A1:A3,A1:A3>100)"));
    // The values on either side of an error cell are still there.
    assert_eq!(sheet.get("A1").map(|cell| cell.value.clone()), Some(CellValue::Number(5.0)));
    assert_eq!(sheet.get("A9").map(|cell| cell.value.clone()), Some(CellValue::Number(10.0)));
    assert_eq!(sheet.get("C9").map(|cell| cell.value.clone()), Some(CellValue::Number(11.0)));
}

#[test]
fn xlsx_replaces_text_excel_would_reject_with_value_error() {
    let mut workbook = Workbook::new_blank("Foreign errors");
    let sheet = &mut workbook.sheets[0];
    // LibreOffice's own codes, an empty string and a lower-case literal.
    sheet.set("A1", error_cell("Err:502", Some("=SQRT(-1)")));
    sheet.set("A2", error_cell("", None));
    sheet.set("A3", error_cell("#n/a", None));
    sheet.set("A4", error_cell("#SPILL!", None));
    let bytes = xlsx::write_xlsx(&workbook).unwrap();
    let text = worksheet(&bytes);
    assert!(text.contains("<f>SQRT(-1)</f><v>#VALUE!</v>"), "{text}");
    assert_eq!(text.matches("<v>#VALUE!</v>").count(), 2, "{text}");
    assert!(text.contains("<v>#N/A</v>"));
    assert!(text.contains("<v>#SPILL!</v>"));
    let read = xlsx::read_workbook_bytes(&bytes).unwrap();
    assert_eq!(error_at(&read.workbook, "A1").as_deref(), Some("#VALUE!"));
    assert_eq!(error_at(&read.workbook, "A3").as_deref(), Some("#N/A"));
}

fn content_xml(bytes: &[u8]) -> String {
    ZipReader::open(bytes.to_vec()).unwrap().read_text("content.xml").unwrap()
}

#[test]
fn ods_writes_and_reads_errors() {
    let bytes = odf::write_ods(&workbook_with_errors()).unwrap();
    let content = content_xml(&bytes);
    assert!(
        content.contains("calcext:value-type=\"error\""),
        "LibreOffice marks an error cell with calcext:value-type=\"error\": {content}"
    );
    assert!(content.contains("<text:p>#CALC!</text:p>"));
    let read = odf::read_ods(&bytes).unwrap();
    for (address, text) in [
        ("B1", "#CALC!"),
        ("B2", "#SPILL!"),
        ("B3", "#DIV/0!"),
        // No formula: the marker is what keeps these errors and not text.
        ("B5", "#NAME?"),
        ("B6", "#NUM!"),
        ("B9", "#NULL!"),
    ] {
        assert_eq!(error_at(&read.workbook, address).as_deref(), Some(text), "{address}");
    }
    let sheet = &read.workbook.sheets[0];
    assert_eq!(sheet.get("B1").and_then(|cell| cell.formula.as_deref()), Some("=FILTER(A1:A3,A1:A3>100)"));
}

#[test]
fn ods_text_that_looks_like_an_error_stays_text() {
    let mut workbook = Workbook::new_blank("Text");
    workbook.sheets[0].set("A1", Cell { value: CellValue::Text("#CALC!".into()), ..Default::default() });
    let bytes = odf::write_ods(&workbook).unwrap();
    assert!(!content_xml(&bytes).contains("calcext:value-type=\"error\""));
    let read = odf::read_ods(&bytes).unwrap();
    assert_eq!(
        read.workbook.sheets[0].get("A1").map(|cell| cell.value.clone()),
        Some(CellValue::Text("#CALC!".into()))
    );
}

/// An error cell as LibreOffice writes it.
#[test]
fn ods_import_reads_libreoffice_error_cells() {
    let content = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        "<office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
        "xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" ",
        "xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" ",
        "xmlns:calcext=\"urn:org:documentfoundation:names:experimental:calc:xmlns:calcext:1.0\" office:version=\"1.3\">",
        "<office:body><office:spreadsheet><table:table table:name=\"Sheet1\"><table:table-row>",
        "<table:table-cell table:formula=\"of:=1/0\" office:value-type=\"string\" office:string-value=\"\" calcext:value-type=\"error\"><text:p>#DIV/0!</text:p></table:table-cell>",
        "<table:table-cell table:formula=\"of:=SQRT(-1)\" office:value-type=\"string\" office:string-value=\"\" calcext:value-type=\"error\"><text:p>Err:502</text:p></table:table-cell>",
        "<table:table-cell office:value-type=\"string\" office:string-value=\"#N/A\" calcext:value-type=\"error\"><text:p>#N/A</text:p></table:table-cell>",
        "<table:table-cell office:value-type=\"string\" office:string-value=\"plain\"><text:p>plain</text:p></table:table-cell>",
        "</table:table-row></table:table></office:spreadsheet></office:body></office:document-content>",
    );
    let mut writer = ZipWriter::new();
    writer.add_text("mimetype", "application/vnd.oasis.opendocument.spreadsheet");
    writer.add_text("content.xml", content);
    let read = odf::read_ods(&writer.finish()).unwrap();
    assert_eq!(error_at(&read.workbook, "A1").as_deref(), Some("#DIV/0!"));
    assert_eq!(error_at(&read.workbook, "B1").as_deref(), Some("Err:502"));
    assert_eq!(error_at(&read.workbook, "C1").as_deref(), Some("#N/A"));
    assert_eq!(read.workbook.sheets[0].get("D1").map(|cell| cell.value.clone()), Some(CellValue::Text("plain".into())));
}

#[test]
fn csv_writes_error_text() {
    let mut workbook = Workbook::new_blank("Errors");
    workbook.sheets[0].set("A1", error_cell("#CALC!", None));
    workbook.sheets[0].set("B1", error_cell("#SPILL!", None));
    let bytes = csvio::write_csv(&workbook, 0, &csvio::CsvOptions::default()).unwrap();
    assert_eq!(String::from_utf8(bytes).unwrap(), "#CALC!,#SPILL!\r\n");
}
