//! V3.0 XLSX import fidelity tests.
//!
//! The workflow these cover is:
//!
//!   build a workbook with rich formatting -> save XLSX -> reopen XLSX
//!
//! Unlike the older calamine-only tests, the importer is now expected to bring
//! back cell styles, number formats, column widths, row heights, merges, freeze
//! panes, validations, conditional rules, hyperlinks, comments, defined names
//! and structured tables. A deliberately corrupt styles or table part must only
//! cost a warning, never the values and formulas from the first pass.

use officecore::model::*;
use officecore::xlsx;
use officecore::zip::{ZipLimits, ZipReader, ZipWriter};

fn header_style() -> CellStyle {
    CellStyle {
        bold: true,
        fill: Some("#DDEBF7".into()),
        color: Some("#1F4E78".into()),
        ..Default::default()
    }
}

fn fidelity_workbook() -> Workbook {
    let mut workbook = Workbook::new_blank("Fidelity");
    let sheet = &mut workbook.sheets[0];
    sheet.name = "Sales".into();
    sheet.tab_color = Some("#FF0000".into());

    sheet.set("A1", Cell { value: CellValue::Text("Region".into()), style: header_style(), ..Default::default() });
    sheet.set("B1", Cell { value: CellValue::Text("Amount".into()), style: header_style(), ..Default::default() });
    sheet.set("A2", Cell { value: CellValue::Text("North".into()), ..Default::default() });
    sheet.set("B2", Cell {
        value: CellValue::Number(1200.5),
        style: CellStyle { number_format: "#,##0.00".into(), ..Default::default() },
        ..Default::default()
    });
    sheet.set("A3", Cell {
        value: CellValue::Text("South".into()),
        comment: Some("check this".into()),
        ..Default::default()
    });
    sheet.set("B3", Cell {
        value: CellValue::Number(900.0),
        style: CellStyle { number_format: "#,##0.00".into(), ..Default::default() },
        ..Default::default()
    });
    sheet.set("C3", Cell {
        value: CellValue::Text("site".into()),
        link: Some("https://example.org/report".into()),
        ..Default::default()
    });
    sheet.set("D3", Cell {
        value: CellValue::Number(2401.0),
        formula: Some("=B2*2".into()),
        ..Default::default()
    });
    sheet.set("E3", Cell {
        value: CellValue::Number(46023.0),
        style: CellStyle { number_format: "dd.mm.yyyy".into(), ..Default::default() },
        ..Default::default()
    });
    sheet.set("E5", Cell { comment: Some("standalone note".into()), ..Default::default() });
    sheet.set("A6", Cell {
        value: CellValue::Text("aligned".into()),
        style: CellStyle {
            align: "center".into(),
            valign: "top".into(),
            wrap: true,
            rotation: 45,
            borders: CellBorders {
                bottom: Some(BorderStyle { style: "thin".into(), color: "#000000".into() }),
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    });

    sheet.col_widths.insert(0, 120.0);
    sheet.col_widths.insert(1, 80.0);
    sheet.row_heights.insert(0, 30.0);
    sheet.merges.push(MergeRange { start: "A5".into(), end: "B5".into() });
    sheet.freeze_rows = 1;
    sheet.freeze_cols = 1;

    sheet.validations.push(Validation {
        id: "v1".into(),
        range: "A2:A3".into(),
        kind: "list".into(),
        values: vec!["North".into(), "South".into()],
        min: None,
        max: None,
        message: String::new(),
        allow_blank: true,
    });
    sheet.conditional.push(CondRule {
        id: "c1".into(),
        range: "B2:B3".into(),
        kind: "greater".into(),
        values: vec!["1000".into()],
        fill: Some("#FFC7CE".into()),
        color: None,
        top_n: None,
        stop_if_true: false,
    });
    sheet.conditional.push(CondRule {
        id: "c2".into(),
        range: "B2:B3".into(),
        kind: "dataBar".into(),
        values: Vec::new(),
        fill: Some("#638EC6".into()),
        color: None,
        top_n: None,
        stop_if_true: false,
    });
    sheet.tables.push(SpreadsheetTable {
        id: "t1".into(),
        name: "SalesTable".into(),
        range: "A1:B3".into(),
        has_headers: true,
        has_totals: false,
        banded_rows: true,
        banded_columns: false,
        header_fill: None,
        header_bold: true,
        style_name: "TableStyleMedium2".into(),
        columns: vec![
            TableColumn { name: "Region".into(), formula: None },
            TableColumn { name: "Amount".into(), formula: Some("=B2*2".into()) },
        ],
        filter: Some(FilterState { range: "A1:B3".into(), column: 0, values: vec!["North".into()] }),
    });
    workbook.names.push(NamedRange {
        name: "TaxRate".into(),
        definition: "Sales!$B$2".into(),
        sheet: None,
        comment: String::new(),
    });
    workbook
}

/// Rebuilds a package with some parts replaced, exercising the importer with
/// files our own writer would never produce.
fn rebuild(bytes: &[u8], replacements: &[(&str, &str)]) -> Vec<u8> {
    let reader = ZipReader::open(bytes.to_vec()).unwrap();
    let mut writer = ZipWriter::new();
    for (name, data) in reader.read_all(ZipLimits::default()).unwrap() {
        match replacements.iter().find(|(part, _)| *part == name) {
            Some((_, replacement)) => writer.add_text(&name, replacement),
            None => writer.add(&name, &data),
        }
    }
    writer.finish()
}

#[test]
fn xlsx_import_restores_styles_layout_rules_links_comments_names_and_tables() {
    let original = fidelity_workbook();
    let bytes = xlsx::write_xlsx(&original).unwrap();
    let read = xlsx::read_workbook_bytes(&bytes).unwrap();
    assert!(!read.warnings.iter().any(|warning| warning.contains("limited support")), "{:?}", read.warnings);

    let sheet = read.workbook.sheets.iter().find(|sheet| sheet.name == "Sales").expect("Sales sheet");

    // Styles: bold/fill/font colour and a built-in number format.
    let header = sheet.get("A1").expect("A1");
    assert!(header.style.bold, "bold header was not imported");
    assert_eq!(header.style.fill.as_deref(), Some("#DDEBF7"));
    assert_eq!(header.style.color.as_deref(), Some("#1F4E78"));
    assert_eq!(sheet.get("B2").map(|cell| cell.style.number_format.as_str()), Some("#,##0.00"));
    assert_eq!(sheet.get("B3").map(|cell| cell.style.number_format.as_str()), Some("#,##0.00"));
    assert_eq!(sheet.get("E3").map(|cell| cell.style.number_format.as_str()), Some("dd.mm.yyyy"));
    let aligned = sheet.get("A6").expect("A6");
    assert_eq!(aligned.style.align, "center");
    assert_eq!(aligned.style.valign, "top");
    assert!(aligned.style.wrap);
    assert_eq!(aligned.style.rotation, 45);
    assert_eq!(
        aligned.style.borders.bottom.as_ref().map(|border| (border.style.as_str(), border.color.as_str())),
        Some(("thin", "#000000"))
    );

    // Layout: widths survive the character-unit conversion, heights exactly.
    let width = sheet.col_widths.get(&0).copied().expect("column width");
    assert!((width - 120.0).abs() < 0.1, "column width drifted: {width}");
    assert_eq!(sheet.row_heights.get(&0).copied(), Some(30.0));
    assert_eq!(sheet.merges, vec![MergeRange { start: "A5".into(), end: "B5".into() }]);
    assert_eq!((sheet.freeze_rows, sheet.freeze_cols), (1, 1));
    assert_eq!(sheet.tab_color.as_deref(), Some("#FF0000"));

    // Data validation list.
    assert_eq!(sheet.validations.len(), 1);
    assert_eq!(sheet.validations[0].kind, "list");
    assert_eq!(sheet.validations[0].range, "A2:A3");
    assert_eq!(sheet.validations[0].values, vec!["North".to_string(), "South".to_string()]);
    assert!(sheet.validations[0].allow_blank);

    // Conditional rules: greater-than with the shared highlight dxf, plus the
    // data bar whose colour lives in the rule itself.
    assert_eq!(sheet.conditional.len(), 2);
    let rule = sheet.conditional.iter().find(|rule| rule.kind == "greater").expect("greater rule");
    assert_eq!(rule.range, "B2:B3");
    assert_eq!(rule.values, vec!["1000".to_string()]);
    assert_eq!(rule.fill.as_deref(), Some("#FFF3C4"));
    let bar = sheet.conditional.iter().find(|rule| rule.kind == "dataBar").expect("data bar rule");
    assert_eq!(bar.range, "B2:B3");
    assert_eq!(bar.fill.as_deref(), Some("#638EC6"));

    // Hyperlink and comments (including a comment on a cell with no value).
    assert_eq!(sheet.get("C3").and_then(|cell| cell.link.as_deref()), Some("https://example.org/report"));
    assert_eq!(sheet.get("A3").and_then(|cell| cell.comment.as_deref()), Some("check this"));
    assert_eq!(sheet.get("E5").and_then(|cell| cell.comment.as_deref()), Some("standalone note"));
    assert_eq!(sheet.get("E5").map(|cell| cell.value.clone()), Some(CellValue::Empty));

    // Defined name.
    let name = read.workbook.names.iter().find(|entry| entry.name == "TaxRate").expect("TaxRate name");
    assert_eq!(name.definition, "Sales!$B$2");
    assert!(name.is_workbook_scope());

    // Structured table: range, flags, style, columns, calculated formula and
    // its own filter with the selected values.
    assert_eq!(sheet.tables.len(), 1);
    let table = &sheet.tables[0];
    assert_eq!(table.name, "SalesTable");
    assert_eq!(table.range, "A1:B3");
    assert!(table.has_headers);
    assert!(!table.has_totals);
    assert!(table.banded_rows);
    assert!(!table.banded_columns);
    assert_eq!(table.style_name, "TableStyleMedium2");
    assert_eq!(table.header_fill.as_deref(), Some("#DDEBF7"));
    assert!(table.header_bold);
    assert_eq!(table.columns.len(), 2);
    assert_eq!(table.columns[0].name, "Region");
    assert_eq!(table.columns[1].name, "Amount");
    assert_eq!(table.columns[1].formula.as_deref(), Some("=B2*2"));
    let filter = table.filter.as_ref().expect("table filter");
    assert_eq!(filter.range, "A1:B3");
    assert_eq!(filter.values, vec!["North".to_string()]);
}

#[test]
fn xlsx_table_export_writes_parts_and_does_not_double_write_filters() {
    let mut workbook = Workbook::new_blank("Tables");
    workbook.sheets[0].name = "Data".into();
    let sheet = &mut workbook.sheets[0];
    sheet.set("A1", Cell { value: CellValue::Text("Name".into()), ..Default::default() });
    sheet.set("B1", Cell { value: CellValue::Text("Qty".into()), ..Default::default() });
    sheet.set("A2", Cell { value: CellValue::Text("Bolt".into()), ..Default::default() });
    sheet.set("B2", Cell { value: CellValue::Number(4.0), ..Default::default() });
    sheet.tables.push(SpreadsheetTable {
        id: "t1".into(),
        name: "DataTable".into(),
        range: "A1:B2".into(),
        has_headers: true,
        has_totals: false,
        banded_rows: true,
        banded_columns: false,
        header_fill: None,
        header_bold: true,
        style_name: "TableStyleMedium2".into(),
        columns: vec![
            TableColumn { name: "Name".into(), formula: None },
            TableColumn { name: "Qty".into(), formula: None },
        ],
        filter: Some(FilterState { range: "A1:B2".into(), column: 0, values: vec!["Bolt".into()] }),
    });
    // A sheet-level filter over the same range must not be written twice.
    sheet.filter = Some(FilterState { range: "A1:B2".into(), column: 0, values: vec!["Bolt".into()] });
    // A second table with a totals row and no filter of its own.
    sheet.tables.push(SpreadsheetTable {
        id: "t2".into(),
        name: "TotalsTable".into(),
        range: "A4:B5".into(),
        has_headers: true,
        has_totals: true,
        banded_rows: false,
        banded_columns: true,
        header_fill: None,
        header_bold: false,
        style_name: String::new(),
        columns: Vec::new(),
        filter: None,
    });

    let result = xlsx::write_xlsx_package(&workbook).unwrap();
    let reader = ZipReader::open(result.bytes.clone()).unwrap();
    assert!(reader.contains("xl/tables/table1.xml"), "the table part was not written");
    assert!(reader.contains("xl/tables/table2.xml"), "the totals table part was not written");

    let sheet_xml = reader.read_text("xl/worksheets/sheet1.xml").unwrap();
    assert!(!sheet_xml.contains("<autoFilter"), "the table-owned filter was written twice");
    assert!(sheet_xml.contains("<tableParts count=\"2\"><tablePart r:id=\"rIdTable0\"/><tablePart r:id=\"rIdTable1\"/></tableParts>"));
    let workbook_xml = reader.read_text("xl/workbook.xml").unwrap();
    assert!(!workbook_xml.contains("_FilterDatabase"), "the table-owned filter name was written twice");

    let rels = reader.read_text("xl/worksheets/_rels/sheet1.xml.rels").unwrap();
    assert!(rels.contains("relationships/table"));
    assert!(rels.contains("../tables/table1.xml"));
    let content_types = reader.read_text("[Content_Types].xml").unwrap();
    assert!(content_types.ends_with("</Types>"));
    assert!(content_types.contains("/xl/tables/table1.xml"));

    let table_xml = reader.read_text("xl/tables/table1.xml").unwrap();
    assert!(table_xml.contains("name=\"DataTable\""));
    assert!(table_xml.contains("headerRowCount=\"1\""));
    assert!(table_xml.contains("totalsRowCount=\"0\""));
    assert!(table_xml.contains("<autoFilter ref=\"A1:B2\"><filterColumn colId=\"0\"><filters><filter val=\"Bolt\"/>"));
    assert!(table_xml.contains("<tableColumn id=\"1\" name=\"Name\"/>"));
    assert!(table_xml.contains("tableStyleInfo name=\"TableStyleMedium2\""));

    let totals_xml = reader.read_text("xl/tables/table2.xml").unwrap();
    assert!(totals_xml.contains("name=\"TotalsTable\""));
    assert!(totals_xml.contains("totalsRowCount=\"1\""));
    assert!(!totals_xml.contains("<autoFilter"));
    // Columns were not filled in, so they come from the range width.
    assert!(totals_xml.contains("<tableColumn id=\"1\" name=\"Column1\"/>"));
    assert!(totals_xml.contains("<tableColumn id=\"2\" name=\"Column2\"/>"));

    // Re-import: the filter belongs to the table, not the sheet.
    let read = xlsx::read_workbook_bytes(&result.bytes).unwrap();
    let sheet = read.workbook.sheets.iter().find(|sheet| sheet.name == "Data").unwrap();
    assert!(sheet.filter.is_none(), "the filter should stay with the table");
    assert_eq!(sheet.tables.len(), 2);
    assert_eq!(sheet.tables[0].filter.as_ref().map(|filter| filter.values.clone()), Some(vec!["Bolt".to_string()]));
    assert!(sheet.tables[1].has_totals);
    assert!(sheet.tables[1].filter.is_none());
}

#[test]
fn corrupt_styles_and_table_parts_degrade_to_values_with_warnings() {
    let bytes = xlsx::write_xlsx(&fidelity_workbook()).unwrap();
    let broken = rebuild(
        &bytes,
        &[
            ("xl/styles.xml", "<styleSheet><cellXfs"),
            ("xl/tables/table1.xml", "<table"),
        ],
    );
    let read = xlsx::read_workbook_bytes(&broken).expect("a corrupt style/table part must not fail the import");
    let sheet = read.workbook.sheets.iter().find(|sheet| sheet.name == "Sales").expect("Sales sheet");
    assert_eq!(sheet.get("A1").map(|cell| cell.value.clone()), Some(CellValue::Text("Region".into())));
    assert_eq!(sheet.get("B2").map(|cell| cell.value.clone()), Some(CellValue::Number(1200.5)));
    assert_eq!(sheet.get("D3").and_then(|cell| cell.formula.as_deref()), Some("=B2*2"));
    assert!(read.warnings.iter().any(|warning| warning.contains("styles")), "{:?}", read.warnings);
    assert!(read.warnings.iter().any(|warning| warning.contains("table")), "{:?}", read.warnings);
}

#[test]
fn unsupported_validation_and_conditional_kinds_are_reported_not_silently_dropped() {
    let mut workbook = Workbook::new_blank("Unsupported");
    workbook.sheets[0].set("A1", Cell { value: CellValue::Number(7.0), ..Default::default() });
    let bytes = xlsx::write_xlsx(&workbook).unwrap();
    let sheet_xml = ZipReader::open(bytes.clone()).unwrap().read_text("xl/worksheets/sheet1.xml").unwrap();
    let injected = sheet_xml.replace(
        "</worksheet>",
        "<dataValidations count=\"1\"><dataValidation type=\"date\" sqref=\"A1\"/></dataValidations><conditionalFormatting sqref=\"A1\"><cfRule type=\"colorScale\" priority=\"1\"/></conditionalFormatting></worksheet>",
    );
    let broken = rebuild(&bytes, &[("xl/worksheets/sheet1.xml", &injected)]);
    let read = xlsx::read_workbook_bytes(&broken).unwrap();
    assert_eq!(read.workbook.sheets[0].get("A1").map(|cell| cell.value.clone()), Some(CellValue::Number(7.0)));
    assert!(read.workbook.sheets[0].validations.is_empty());
    assert!(read.workbook.sheets[0].conditional.is_empty());
    assert!(read.warnings.iter().any(|warning| warning.contains("data validation")), "{:?}", read.warnings);
    assert!(read.warnings.iter().any(|warning| warning.contains("conditional formatting")), "{:?}", read.warnings);
}

#[test]
fn sheet_names_map_to_parts_through_the_workbook_rels_not_the_file_order() {
    let mut workbook = Workbook::new_blank("Mapping");
    workbook.sheets[0].name = "Alpha".into();
    workbook.sheets[0].set("A1", Cell { value: CellValue::Text("alpha-only".into()), ..Default::default() });
    let mut zeta = Sheet::new("Zeta");
    zeta.set("A1", Cell { value: CellValue::Text("zeta-only".into()), ..Default::default() });
    workbook.sheets.push(zeta);
    let bytes = xlsx::write_xlsx(&workbook).unwrap();

    // Rename the worksheet parts so their file names no longer encode the
    // sheet order; the workbook rels are the only mapping left.
    let reader = ZipReader::open(bytes.clone()).unwrap();
    let rels = reader.read_text("xl/_rels/workbook.xml.rels").unwrap();
    let renamed_rels = rels
        .replace("worksheets/sheet1.xml", "worksheets/alpha.xml")
        .replace("worksheets/sheet2.xml", "worksheets/zeta.xml");
    let content_types = reader.read_text("[Content_Types].xml").unwrap();
    let renamed_types = content_types
        .replace("/xl/worksheets/sheet1.xml", "/xl/worksheets/alpha.xml")
        .replace("/xl/worksheets/sheet2.xml", "/xl/worksheets/zeta.xml");
    let mut writer = ZipWriter::new();
    for (name, data) in reader.read_all(ZipLimits::default()).unwrap() {
        match name.as_str() {
            "xl/_rels/workbook.xml.rels" => writer.add_text(&name, &renamed_rels),
            "[Content_Types].xml" => writer.add_text(&name, &renamed_types),
            "xl/worksheets/sheet1.xml" => writer.add("xl/worksheets/alpha.xml", &data),
            "xl/worksheets/sheet2.xml" => writer.add("xl/worksheets/zeta.xml", &data),
            _ => writer.add(&name, &data),
        }
    }
    let renamed = writer.finish();
    assert!(!ZipReader::open(renamed.clone()).unwrap().contains("xl/worksheets/sheet1.xml"));

    let read = xlsx::read_workbook_bytes(&renamed).unwrap();
    let alpha = read.workbook.sheets.iter().find(|sheet| sheet.name == "Alpha").expect("Alpha");
    let zeta = read.workbook.sheets.iter().find(|sheet| sheet.name == "Zeta").expect("Zeta");
    assert_eq!(alpha.get("A1").map(|cell| cell.value.clone()), Some(CellValue::Text("alpha-only".into())));
    assert_eq!(zeta.get("A1").map(|cell| cell.value.clone()), Some(CellValue::Text("zeta-only".into())));
}
