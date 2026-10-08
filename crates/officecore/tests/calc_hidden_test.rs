//! Hidden rows and columns in XLSX, ODS and PDF.
//!
//! The editor hides a row or a column by storing size 0 for it in the sheet's
//! sparse `row_heights` / `col_widths` tables (`src/office/calc/visibility.ts`;
//! an AutoFilter hides rows the same way). Files say it with a flag instead:
//! `hidden="1"` on `<row>` / `<col>` in XLSX, `table:visibility="collapse"` (or
//! `"filter"`) on `table:table-row` / `table:table-column` in ODS. A hidden row
//! without cells must survive too, which the old writer (rows with cells only)
//! and the old reader (`ht` only) both missed.

use officecore::model::*;
use officecore::zip::{ZipLimits, ZipReader, ZipWriter};
use officecore::{layout, odf, xlsx};

fn text_cell(text: &str) -> Cell {
    Cell { value: CellValue::Text(text.into()), ..Default::default() }
}

fn number_cell(value: f64) -> Cell {
    Cell { value: CellValue::Number(value), ..Default::default() }
}

/// Rows 2 (with cells), 7 and 9 (empty) hidden, row 6 resized and empty;
/// columns B (with cells) and F (empty) hidden, column C resized.
fn hidden_workbook() -> Workbook {
    let mut workbook = Workbook::new_blank("Hidden");
    let sheet = &mut workbook.sheets[0];
    sheet.name = "Data".into();
    for row in 1..=5u32 {
        sheet.set(&format!("A{row}"), text_cell(&format!("row {row}")));
        sheet.set(&format!("B{row}"), number_cell(f64::from(row) * 10.0));
        sheet.set(&format!("C{row}"), number_cell(f64::from(row)));
    }
    sheet.row_heights.insert(1, 0.0);
    sheet.row_heights.insert(6, 0.0);
    sheet.row_heights.insert(8, 0.0);
    sheet.row_heights.insert(5, 40.0);
    sheet.col_widths.insert(1, 0.0);
    sheet.col_widths.insert(5, 0.0);
    sheet.col_widths.insert(2, 150.0);
    workbook
}

fn sheet_part(bytes: &[u8]) -> String {
    ZipReader::open(bytes.to_vec()).unwrap().read_text("xl/worksheets/sheet1.xml").unwrap()
}

fn hidden_rows(sheet: &Sheet) -> Vec<u32> {
    sheet.row_heights.iter().filter(|(_, height)| **height == 0.0).map(|(row, _)| *row).collect()
}

fn hidden_columns(sheet: &Sheet) -> Vec<u32> {
    sheet.col_widths.iter().filter(|(_, width)| **width == 0.0).map(|(column, _)| *column).collect()
}

#[test]
fn xlsx_writes_hidden_rows_and_columns_even_without_cells() {
    let sheet = sheet_part(&xlsx::write_xlsx(&hidden_workbook()).unwrap());
    // A hidden row with cells, and hidden rows with none.
    assert!(sheet.contains("<row r=\"2\" hidden=\"1\">"), "{sheet}");
    assert!(sheet.contains("<row r=\"7\" hidden=\"1\"/>"), "{sheet}");
    assert!(sheet.contains("<row r=\"9\" hidden=\"1\"/>"), "{sheet}");
    // A resized row without cells.
    assert!(sheet.contains("<row r=\"6\" ht=\"30.00\" customHeight=\"1\"/>"), "{sheet}");
    // A hidden column keeps a width Excel shows again on unhide.
    assert!(sheet.contains("<col min=\"2\" max=\"2\" width=\"13.00\" hidden=\"1\" customWidth=\"1\"/>"), "{sheet}");
    assert!(sheet.contains("<col min=\"6\" max=\"6\" width=\"13.00\" hidden=\"1\" customWidth=\"1\"/>"), "{sheet}");
    assert!(sheet.contains("<col min=\"3\" max=\"3\" width=\"20.71\" customWidth=\"1\"/>"), "{sheet}");
    // Rows stay in ascending order, which Excel requires.
    let positions: Vec<usize> = [2, 6, 7, 9]
        .iter()
        .map(|row| sheet.find(&format!("<row r=\"{row}\"")).unwrap_or_else(|| panic!("row {row} missing")))
        .collect();
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]), "{sheet}");
}

#[test]
fn xlsx_round_trip_keeps_hidden_rows_and_columns() {
    let bytes = xlsx::write_xlsx(&hidden_workbook()).unwrap();
    let read = xlsx::read_workbook_bytes(&bytes).unwrap();
    let sheet = &read.workbook.sheets[0];
    assert_eq!(hidden_rows(sheet), vec![1, 6, 8]);
    assert_eq!(hidden_columns(sheet), vec![1, 5]);
    // Visible sizes survive next to them.
    assert!((sheet.row_heights[&5] - 40.0).abs() < 0.01, "{:?}", sheet.row_heights);
    assert!((sheet.col_widths[&2] - 150.0).abs() < 1.0, "{:?}", sheet.col_widths);
    // The cells in a hidden row or column are still there.
    assert_eq!(sheet.get("A2").map(|cell| cell.value.clone()), Some(CellValue::Text("row 2".into())));
    assert_eq!(sheet.get("B2").map(|cell| cell.value.clone()), Some(CellValue::Number(20.0)));
}

#[test]
fn xlsx_hidden_state_survives_a_second_round_trip() {
    let once = xlsx::read_workbook_bytes(&xlsx::write_xlsx(&hidden_workbook()).unwrap()).unwrap();
    let twice = xlsx::read_workbook_bytes(&xlsx::write_xlsx(&once.workbook).unwrap()).unwrap();
    assert_eq!(hidden_rows(&twice.workbook.sheets[0]), vec![1, 6, 8]);
    assert_eq!(hidden_columns(&twice.workbook.sheets[0]), vec![1, 5]);
}

#[test]
fn xlsx_rows_hidden_by_a_filter_are_hidden_rows() {
    let mut workbook = hidden_workbook();
    let sheet = &mut workbook.sheets[0];
    sheet.row_heights.clear();
    sheet.filter = Some(FilterState { range: "A1:C5".into(), ..Default::default() });
    for row in [2, 3] {
        sheet.row_heights.insert(row, 0.0);
    }
    let bytes = xlsx::write_xlsx(&workbook).unwrap();
    let text = sheet_part(&bytes);
    assert!(text.contains("<row r=\"3\" hidden=\"1\">") && text.contains("<row r=\"4\" hidden=\"1\">"), "{text}");
    let read = xlsx::read_workbook_bytes(&bytes).unwrap();
    assert_eq!(hidden_rows(&read.workbook.sheets[0]), vec![2, 3]);
    assert!(read.workbook.sheets[0].filter.is_some());
}

/// A worksheet as Excel writes it: `hidden` next to a stored height, spans,
/// `true` as a boolean, a hidden column group, and hidden rows far below the data.
const EXCEL_HIDDEN_SHEET: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n",
    "<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><dimension ref=\"A1:E4\"/>",
    "<sheetFormatPr defaultRowHeight=\"15\"/>",
    "<cols>",
    "<col min=\"1\" max=\"1\" width=\"12.7109375\" customWidth=\"1\"/>",
    "<col min=\"2\" max=\"3\" width=\"9.140625\" hidden=\"1\" customWidth=\"1\"/>",
    "<col min=\"4\" max=\"4\" hidden=\"1\"/>",
    "<col min=\"5\" max=\"5\" width=\"0\" customWidth=\"1\"/>",
    "<col min=\"27\" max=\"16384\" width=\"9.140625\" hidden=\"1\" customWidth=\"1\"/>",
    "</cols><sheetData>",
    "<row r=\"1\" spans=\"1:5\"><c r=\"A1\"><v>1</v></c><c r=\"B1\"><v>2</v></c></row>",
    "<row r=\"2\" spans=\"1:5\" ht=\"15\" hidden=\"1\" customHeight=\"1\"><c r=\"A2\"><v>3</v></c></row>",
    "<row r=\"3\" hidden=\"true\"/>",
    "<row r=\"4\" ht=\"30\" customHeight=\"1\"><c r=\"A4\"><v>4</v></c></row>",
    "<row r=\"5000\" hidden=\"1\"/>",
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
fn xlsx_import_reads_hidden_flags_excel_writes() {
    let package = with_sheet(&xlsx::write_xlsx(&Workbook::new_blank("Excel")).unwrap(), EXCEL_HIDDEN_SHEET);
    let read = xlsx::read_workbook_bytes(&package).unwrap();
    let sheet = &read.workbook.sheets[0];
    // The stored height of a hidden row does not make it visible.
    assert_eq!(hidden_rows(sheet), vec![1, 2]);
    assert!((sheet.row_heights[&3] - 40.0).abs() < 0.01, "{:?}", sheet.row_heights);
    // Hidden ranges, a hidden column without a width, and a zero-width column.
    assert_eq!(hidden_columns(sheet), vec![1, 2, 3, 4]);
    assert!((sheet.col_widths[&0] - 94.0).abs() < 1.0, "{:?}", sheet.col_widths);
    // Nothing is stored for rows and columns outside the sheet's grid.
    assert!(!sheet.row_heights.contains_key(&4999), "{:?}", sheet.row_heights);
    assert!(sheet.col_widths.len() <= 5, "{:?}", sheet.col_widths);
    // Values next to them are intact.
    assert_eq!(sheet.get("A2").map(|cell| cell.value.clone()), Some(CellValue::Number(3.0)));
    assert_eq!(sheet.get("A4").map(|cell| cell.value.clone()), Some(CellValue::Number(4.0)));
}

fn content_xml(bytes: &[u8]) -> String {
    ZipReader::open(bytes.to_vec()).unwrap().read_text("content.xml").unwrap()
}

#[test]
fn ods_writes_hidden_rows_and_columns_as_collapsed() {
    let content = content_xml(&odf::write_ods(&hidden_workbook()).unwrap());
    // Columns B and F: hidden, the other columns not.
    assert_eq!(content.matches("table:visibility=\"collapse\"").count(), 5, "{content}");
    let columns: Vec<&str> =
        content.split("<table:table-column ").skip(1).map(|part| &part[..part.find("/>").unwrap()]).collect();
    assert!(columns[1].contains("table:visibility=\"collapse\""), "{columns:?}");
    assert!(columns[5].contains("table:visibility=\"collapse\""), "{columns:?}");
    assert!(!columns[0].contains("visibility") && !columns[2].contains("visibility"), "{columns:?}");
    // Rows 2 (with cells), 7 and 9 (empty).
    assert_eq!(content.matches("<table:table-row table:visibility=\"collapse\">").count(), 3, "{content}");
}

#[test]
fn ods_round_trip_keeps_hidden_rows_and_columns() {
    let bytes = odf::write_ods(&hidden_workbook()).unwrap();
    let read = odf::read_ods(&bytes).unwrap();
    let sheet = &read.workbook.sheets[0];
    assert_eq!(hidden_rows(sheet), vec![1, 6, 8]);
    assert_eq!(hidden_columns(sheet), vec![1, 5]);
    assert_eq!(sheet.get("A2").map(|cell| cell.value.clone()), Some(CellValue::Text("row 2".into())));
    assert_eq!(sheet.get("C5").map(|cell| cell.value.clone()), Some(CellValue::Number(5.0)));
}

/// What LibreOffice 24.2 writes for a sheet with hidden rows and columns,
/// header rows, a collapsed row group, a filtered row and repeated runs.
fn libreoffice_hidden_package() -> Vec<u8> {
    let content = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        "<office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
        "xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" ",
        "xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" office:version=\"1.3\">",
        "<office:body><office:spreadsheet><table:table table:name=\"Sheet1\">",
        "<table:table-column table:style-name=\"co1\" table:default-cell-style-name=\"Default\"/>",
        "<table:table-column table:style-name=\"co1\" table:visibility=\"collapse\" table:number-columns-repeated=\"2\" table:default-cell-style-name=\"Default\"/>",
        "<table:table-column table:style-name=\"co1\" table:visibility=\"visible\" table:default-cell-style-name=\"Default\"/>",
        "<table:table-header-columns><table:table-column table:visibility=\"collapse\" table:default-cell-style-name=\"Default\"/></table:table-header-columns>",
        "<table:table-column table:visibility=\"collapse\" table:number-columns-repeated=\"1020\" table:default-cell-style-name=\"Default\"/>",
        "<table:table-header-rows><table:table-row table:style-name=\"ro1\"><table:table-cell office:value-type=\"string\"><text:p>head</text:p></table:table-cell></table:table-row></table:table-header-rows>",
        "<table:table-row table:style-name=\"ro1\" table:visibility=\"collapse\"><table:table-cell office:value-type=\"float\" office:value=\"1\"><text:p>1</text:p></table:table-cell></table:table-row>",
        "<table:table-row table:style-name=\"ro1\" table:visibility=\"collapse\" table:number-rows-repeated=\"3\"><table:table-cell table:number-columns-repeated=\"2\"/></table:table-row>",
        "<table:table-row table:style-name=\"ro1\" table:visibility=\"filter\"><table:table-cell office:value-type=\"float\" office:value=\"2\"><text:p>2</text:p></table:table-cell></table:table-row>",
        "<table:table-row-group table:display=\"false\">",
        "<table:table-row table:style-name=\"ro1\"><table:table-cell office:value-type=\"float\" office:value=\"3\"><text:p>3</text:p></table:table-cell></table:table-row>",
        "<table:table-row-group><table:table-row table:style-name=\"ro1\"><table:table-cell office:value-type=\"float\" office:value=\"4\"><text:p>4</text:p></table:table-cell></table:table-row></table:table-row-group>",
        "</table:table-row-group>",
        "<table:table-row table:style-name=\"ro1\"><table:table-cell office:value-type=\"float\" office:value=\"5\"><text:p>5</text:p></table:table-cell></table:table-row>",
        "<table:table-row table:style-name=\"ro1\" table:visibility=\"collapse\" table:number-rows-repeated=\"1048000\"><table:table-cell table:number-columns-repeated=\"1024\"/></table:table-row>",
        "</table:table></office:spreadsheet></office:body></office:document-content>",
    );
    let mut writer = ZipWriter::new();
    writer.add_text("mimetype", "application/vnd.oasis.opendocument.spreadsheet");
    writer.add_text("content.xml", content);
    writer.finish()
}

#[test]
fn ods_import_reads_the_visibility_libreoffice_writes() {
    let read = odf::read_ods(&libreoffice_hidden_package()).unwrap();
    let sheet = &read.workbook.sheets[0];
    // Row 0 is the header row (visible), 1 hidden, 2-4 hidden (repeated), 5 filtered,
    // 6 and 7 sit in a collapsed group, 8 is visible, the rest of the sheet is hidden.
    let hidden = hidden_rows(sheet);
    for row in [1, 2, 3, 4, 5, 6, 7] {
        assert!(hidden.contains(&row), "row {row} should be hidden: {hidden:?}");
    }
    for row in [0, 8] {
        assert!(!hidden.contains(&row), "row {row} should be visible: {hidden:?}");
    }
    // Only the sheet's own grid is recorded, not a million rows.
    assert!(hidden.iter().all(|row| *row < sheet.row_count), "{hidden:?} vs {}", sheet.row_count);
    assert!(hidden.len() < 1_000, "{} hidden rows recorded", hidden.len());
    // Column 0 visible, 1-2 hidden (repeated), 3 explicitly visible, 4 (header column) and 5.. hidden.
    let columns = hidden_columns(sheet);
    assert_eq!(&columns[..4], &[1, 2, 4, 5], "{columns:?}");
    assert!(!columns.contains(&0) && !columns.contains(&3));
    // The cells in the collapsed group were not lost to the nesting.
    assert_eq!(sheet.get("A1").map(|cell| cell.value.clone()), Some(CellValue::Text("head".into())));
    assert_eq!(sheet.get("A7").map(|cell| cell.value.clone()), Some(CellValue::Number(3.0)));
    assert_eq!(sheet.get("A8").map(|cell| cell.value.clone()), Some(CellValue::Number(4.0)));
    assert_eq!(sheet.get("A9").map(|cell| cell.value.clone()), Some(CellValue::Number(5.0)));
}

/// Text drawing operations (`Tj`) on every page of a PDF.
fn text_operations(pdf: &[u8]) -> usize {
    let document = lopdf::Document::load_mem(pdf).unwrap();
    document
        .get_pages()
        .values()
        .map(|page| {
            let content = document.get_and_decode_page_content(*page).unwrap();
            content.operations.iter().filter(|operation| operation.operator == "Tj").count()
        })
        .sum()
}

#[test]
fn hidden_columns_and_rows_do_not_draw_in_the_pdf() {
    let mut workbook = Workbook::new_blank("Print");
    let sheet = &mut workbook.sheets[0];
    sheet.set("A1", text_cell("A1"));
    sheet.set("B1", text_cell("B1"));
    sheet.set("A2", text_cell("A2"));
    sheet.set("A3", text_cell("A3"));
    let shown = text_operations(&layout::workbook_to_pdf(&workbook, 1));
    // Hide column B and row 2: its header letter, the cell B1, the row label "2"
    // and the cell A2 are no longer drawn; nothing else changes.
    let sheet = &mut workbook.sheets[0];
    sheet.col_widths.insert(1, 0.0);
    sheet.row_heights.insert(1, 0.0);
    let hidden = text_operations(&layout::workbook_to_pdf(&workbook, 1));
    assert_eq!(hidden + 4, shown, "hidden cells, labels and headers must not be drawn");
}
