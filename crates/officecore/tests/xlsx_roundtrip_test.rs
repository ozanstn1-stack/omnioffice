//! XLSX round-trip and export-structure tests.
//!
//! The workflow these cover is the one a user performs:
//!
//!   build/edit a workbook -> save XLSX -> reopen XLSX -> compare the model
//!
//! Import reads values and formulas through `calamine`; cell styling, merges,
//! column widths, freeze panes, validations, conditional formatting and charts
//! are written by the exporter but not read back by the importer, so those are
//! verified structurally against the package parts (and the loss is asserted,
//! not hidden). Values and formulas are compared cell by cell with no loss
//! permitted.

use officecore::model::*;
use officecore::xlsx;
use officecore::zip::ZipReader;

fn sheet_by_name<'a>(workbook: &'a Workbook, name: &str) -> Option<&'a Sheet> {
    workbook.sheets.iter().find(|sheet| sheet.name == name)
}

fn cell_text(value: &CellValue) -> String {
    match value {
        CellValue::Empty => String::new(),
        CellValue::Number(number) => number.to_string(),
        CellValue::Text(text) => text.clone(),
        CellValue::Bool(flag) => flag.to_string(),
        CellValue::Error(error) => error.clone(),
    }
}

/// A workbook with a 100-row sheet, formulas, number formats, merges, widths,
/// heights, freeze panes, a filter, validation, conditional rules and charts.
fn golden_workbook() -> Workbook {
    let mut workbook = Workbook::new_blank("Golden");
    let sheet = &mut workbook.sheets[0];
    sheet.name = "Data".into();
    sheet.set("A1", Cell { value: CellValue::Text("Month".into()), style: CellStyle { bold: true, ..Default::default() }, ..Default::default() });
    sheet.set("B1", Cell { value: CellValue::Text("Sales".into()), style: CellStyle { bold: true, ..Default::default() }, ..Default::default() });
    sheet.set("C1", Cell { value: CellValue::Text("Cost".into()), style: CellStyle { bold: true, ..Default::default() }, ..Default::default() });
    sheet.set("D1", Cell { value: CellValue::Text("Margin".into()), style: CellStyle { bold: true, ..Default::default() }, ..Default::default() });
    for row in 2..=101u32 {
        let index = row - 1;
        sheet.set(&format!("A{row}"), Cell {
            value: CellValue::Text(format!("2026-{:02}", (index % 12) + 1)),
            ..Default::default()
        });
        sheet.set(&format!("B{row}"), Cell { value: CellValue::Number(index as f64 * 10.0), ..Default::default() });
        sheet.set(&format!("C{row}"), Cell {
            value: CellValue::Number(index as f64 * 4.0),
            style: CellStyle { number_format: "#,##0.00".into(), ..Default::default() },
            ..Default::default()
        });
        sheet.set(&format!("D{row}"), Cell {
            value: CellValue::Number(index as f64 * 6.0),
            formula: Some(format!("=B{row}-C{row}")),
            style: CellStyle { number_format: "#,##0.00".into(), ..Default::default() },
            ..Default::default()
        });
    }
    sheet.set("E1", Cell {
        value: CellValue::Number(46023.0),
        style: CellStyle { number_format: "dd.mm.yyyy".into(), ..Default::default() },
        ..Default::default()
    });
    sheet.set("F1", Cell { value: CellValue::Text("Total".into()), ..Default::default() });
    sheet.set("G1", Cell { value: CellValue::Number(5050.0), ..Default::default() });
    sheet.set("H1", Cell { value: CellValue::Bool(true), ..Default::default() });
    sheet.merges.push(MergeRange { start: "F1".into(), end: "F2".into() });
    sheet.col_widths.insert(0, 140.0);
    sheet.col_widths.insert(3, 120.0);
    sheet.row_heights.insert(0, 28.0);
    sheet.freeze_rows = 1;
    sheet.filter = Some(FilterState { range: "A1:D101".into(), column: 0, values: vec!["2026-01".into()] });
    sheet.validations.push(Validation {
        id: "v1".into(),
        range: "B2:B101".into(),
        kind: "number".into(),
        values: vec![],
        min: Some(0.0),
        max: Some(100_000.0),
        message: "Sales must be between 0 and 100000".into(),
        allow_blank: true,
    });
    sheet.validations.push(Validation {
        id: "v2".into(),
        range: "A2:A101".into(),
        kind: "list".into(),
        values: vec!["2026-01".into(), "2026-02".into()],
        min: None,
        max: None,
        message: String::new(),
        allow_blank: true,
    });
    sheet.conditional.push(CondRule {
        id: "c1".into(),
        range: "D2:D101".into(),
        kind: "greater".into(),
        values: vec!["500".into()],
        fill: Some("#C6EFCE".into()),
        color: None,
        top_n: None,
        stop_if_true: false,
    });
    sheet.conditional.push(CondRule {
        id: "c2".into(),
        range: "B2:B101".into(),
        kind: "dataBar".into(),
        values: vec![],
        fill: Some("#638EC6".into()),
        color: None,
        top_n: None,
        stop_if_true: false,
    });
    sheet.charts.push(ChartPlacement {
        id: "chart-1".into(),
        chart: ChartData {
            kind: "column".into(),
            title: "Sales vs cost".into(),
            categories: "A2:A13".into(),
            series: vec![
                ChartSeries { name: "Sales".into(), range: "B2:B13".into(), color: Some("#4472C4".into()) },
                ChartSeries { name: "Cost".into(), range: "C2:C13".into(), color: Some("#ED7D31".into()) },
            ],
            legend: true,
            x_title: "Month".into(),
            y_title: "EUR".into(),
            stacked: false,
            show_labels: false,
        },
        anchor: "F5".into(),
        width_px: 420.0,
        height_px: 260.0,
    });

    // A second sheet with a cross-sheet formula and its own small chart.
    let mut summary = Sheet::new("Summary");
    summary.set("A1", Cell { value: CellValue::Text("Metric".into()), ..Default::default() });
    summary.set("B1", Cell { value: CellValue::Text("Value".into()), ..Default::default() });
    summary.set("A2", Cell { value: CellValue::Text("Sales total".into()), ..Default::default() });
    summary.set("B2", Cell { value: CellValue::Number(50500.0), formula: Some("=SUM(Data!B2:B101)".into()), ..Default::default() });
    summary.set("A3", Cell { value: CellValue::Text("Rows".into()), ..Default::default() });
    summary.set("B3", Cell { value: CellValue::Number(100.0), formula: Some("=COUNTA(Data!A2:A101)".into()), ..Default::default() });
    summary.charts.push(ChartPlacement {
        id: "chart-2".into(),
        chart: ChartData {
            kind: "pie".into(),
            title: "Share".into(),
            categories: "A2:A3".into(),
            series: vec![ChartSeries { name: "Value".into(), range: "B2:B3".into(), color: None }],
            legend: true,
            x_title: String::new(),
            y_title: String::new(),
            stacked: false,
            show_labels: true,
        },
        anchor: "D2".into(),
        width_px: 300.0,
        height_px: 200.0,
    });
    workbook.sheets.push(summary);
    workbook
}

#[test]
fn xlsx_roundtrip_preserves_every_value_and_formula() {
    let original = golden_workbook();
    let bytes = xlsx::write_xlsx(&original).unwrap();
    let read = xlsx::read_workbook_bytes(&bytes).unwrap();

    assert_eq!(read.workbook.sheets.len(), 2, "both sheets must survive the round trip");
    assert_eq!(sheet_by_name(&read.workbook, "Data").is_some(), true, "sheet names are preserved");

    let mut value_losses = Vec::new();
    let mut formula_losses = Vec::new();
    for (before, after) in original.sheets.iter().zip(read.workbook.sheets.iter()) {
        for (address, cell) in &before.cells {
            let Some(round) = after.get(address) else {
                value_losses.push(format!("{}!{address} disappeared", before.name));
                continue;
            };
            if cell_text(&cell.value) != cell_text(&round.value) {
                value_losses.push(format!(
                    "{}!{address}: {:?} -> {:?}",
                    before.name, cell.value, round.value
                ));
            }
            if cell.formula != round.formula {
                formula_losses.push(format!(
                    "{}!{address}: {:?} -> {:?}",
                    before.name, cell.formula, round.formula
                ));
            }
        }
    }
    assert!(value_losses.is_empty(), "value loss after round trip: {value_losses:#?}");
    assert!(formula_losses.is_empty(), "formula loss after round trip: {formula_losses:#?}");
}

#[test]
fn xlsx_roundtrip_keeps_cross_sheet_references_and_fifty_thousand_values() {
    let original = golden_workbook();
    let bytes = xlsx::write_xlsx(&original).unwrap();
    let read = xlsx::read_workbook_bytes(&bytes).unwrap();
    let summary = sheet_by_name(&read.workbook, "Summary").expect("Summary sheet");
    assert_eq!(summary.get("B2").and_then(|cell| cell.formula.as_deref()), Some("=SUM(Data!B2:B101)"));
    assert_eq!(summary.get("B3").and_then(|cell| cell.formula.as_deref()), Some("=COUNTA(Data!A2:A101)"));
    let data = sheet_by_name(&read.workbook, "Data").expect("Data sheet");
    let mut rows = 0;
    for row in 2..=101 {
        if let Some(cell) = data.get(&format!("D{row}")) {
            if cell.formula.is_some() {
                rows += 1;
            }
        }
    }
    assert_eq!(rows, 100, "the 100-row formula block must come back complete");
    assert_eq!(data.get("D101").and_then(|cell| cell.formula.as_deref()), Some("=B101-C101"));
}

#[test]
fn xlsx_export_writes_styles_merges_layout_and_freeze_panes() {
    let bytes = xlsx::write_xlsx(&golden_workbook()).unwrap();
    let reader = ZipReader::open(bytes).unwrap();

    let styles = reader.read_text("xl/styles.xml").unwrap();
    assert!(styles.contains("<b/>"), "bold header font missing");
    // `#,##0.00` is a built-in Excel format, so it is referenced by id…
    assert!(styles.contains("numFmtId=\"4\""), "the built-in margin format is not referenced");
    // …while `dd.mm.yyyy` has to be declared as a custom format.
    assert!(styles.contains("formatCode=\"dd.mm.yyyy\""), "the date format code is missing");

    let sheet = reader.read_text("xl/worksheets/sheet1.xml").unwrap();
    assert!(sheet.contains("<mergeCell ref=\"F1:F2\"/>"));
    assert!(sheet.contains("customWidth=\"1\""), "column widths missing");
    assert!(sheet.contains("customHeight=\"1\""), "row heights missing");
    assert!(sheet.contains("state=\"frozen\""), "freeze panes missing");
    assert!(sheet.contains("<autoFilter ref=\"A1:D101\"/>"));
}

#[test]
fn xlsx_export_writes_validation_conditional_formatting_and_charts() {
    let bytes = xlsx::write_xlsx(&golden_workbook()).unwrap();
    let reader = ZipReader::open(bytes).unwrap();

    let sheet = reader.read_text("xl/worksheets/sheet1.xml").unwrap();
    assert!(sheet.contains("dataValidation"), "validation missing");
    assert!(sheet.contains("type=\"decimal\""), "numeric validation missing");
    assert!(sheet.contains("type=\"list\""), "list validation missing");
    assert!(sheet.contains("Sales must be between 0 and 100000"));
    assert!(sheet.contains("conditionalFormatting sqref=\"D2:D101\""));
    assert!(sheet.contains("operator=\"greaterThan\""));
    assert!(sheet.contains("type=\"dataBar\""));
    assert!(sheet.contains("<drawing r:id=\"rIdDrawing\"/>"));

    assert!(reader.contains("xl/charts/chart1.xml"), "column chart missing");
    assert!(reader.contains("xl/charts/chart2.xml"), "pie chart missing");
    let chart = reader.read_text("xl/charts/chart1.xml").unwrap();
    assert!(chart.contains("Data!$B$2:$B$13"));
    assert!(chart.contains("Data!$C$2:$C$13"));
    let pie = reader.read_text("xl/charts/chart2.xml").unwrap();
    assert!(pie.contains("<c:pieChart>"));
    assert!(pie.contains("Summary!$B$2:$B$3"));

    let rels = reader.read_text("xl/drawings/_rels/drawing1.xml.rels").unwrap();
    assert!(rels.contains("../charts/chart1.xml"));
    let content_types = reader.read_text("[Content_Types].xml").unwrap();
    assert!(content_types.ends_with("</Types>"));
    assert!(content_types.contains("drawingml.chart+xml"));
}

#[test]
fn xlsx_export_materializes_pivot_tables_as_values() {
    let mut workbook = Workbook::new_blank("Pivot check");
    workbook.sheets[0].name = "Data".into();
    let sheet = &mut workbook.sheets[0];
    let rows = [
        ["Department", "Year", "Sales"],
        ["Hardware", "2025", "100"],
        ["Hardware", "2025", "150"],
        ["Software", "2025", "200"],
    ];
    for (row, line) in rows.iter().enumerate() {
        for (column, value) in line.iter().enumerate() {
            let numeric = value.parse::<f64>().is_ok();
            sheet.set(
                &format!("{}{}", (b'A' + column as u8) as char, row + 1),
                Cell {
                    value: if numeric { CellValue::Number(value.parse().unwrap()) } else { CellValue::Text((*value).into()) },
                    ..Default::default()
                },
            );
        }
    }
    sheet.pivot_tables.push(PivotTable {
        id: "p1".into(),
        name: "Pivot1".into(),
        source_sheet: "Data".into(),
        source: "A1:C4".into(),
        rows: vec!["Department".into()],
        columns: vec!["Year".into()],
        values: vec![PivotValueField { field: "Sales".into(), aggregation: "sum".into() }],
        filters: vec![],
        anchor: "F1".into(),
    });

    // The definition survives a `.oswk`-style JSON round trip…
    let json = serde_json::to_string(&workbook).unwrap();
    let decoded: Workbook = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.sheets[0].pivot_tables.len(), 1);
    assert_eq!(decoded.sheets[0].pivot_tables[0].rows, vec!["Department".to_string()]);

    // …and the XLSX export warns and materialises the computed grid.
    let result = officecore::xlsx::write_xlsx_package(&workbook).unwrap();
    assert!(result.warnings.iter().any(|warning| warning.contains("Pivot tables")), "{:?}", result.warnings);
    let read = officecore::xlsx::read_workbook_bytes(&result.bytes).unwrap();
    let data = sheet_by_name(&read.workbook, "Data").expect("Data sheet");
    assert_eq!(data.get("F1").map(|cell| cell.value.clone()), Some(CellValue::Text("Department".into())));
    assert_eq!(data.get("G1").map(|cell| cell.value.clone()), Some(CellValue::Text("2025".into())));
    assert_eq!(data.get("G2").map(|cell| cell.value.clone()), Some(CellValue::Number(250.0)));
    assert_eq!(data.get("G3").map(|cell| cell.value.clone()), Some(CellValue::Number(200.0)));
}

/// Pins what a pure XLSX round trip preserves. V3.0 added a custom OOXML
/// import pass, so layout and presentation metadata now survive; charts are
/// still export-only (the model keeps them, the importer does not rebuild
/// ChartML).
#[test]
fn xlsx_roundtrip_preserves_values_and_presentation_metadata() {
    let original = golden_workbook();
    let bytes = xlsx::write_xlsx(&original).unwrap();
    let read = xlsx::read_workbook_bytes(&bytes).unwrap();

    let before = &original.sheets[0];
    let after = sheet_by_name(&read.workbook, "Data").unwrap();
    assert_eq!(
        after.cells.len(),
        before.cells.len(),
        "every value and formula cell must survive the round trip"
    );
    assert_eq!(after.merges.len(), before.merges.len(), "merges must survive");
    assert!(!after.col_widths.is_empty(), "column widths must survive");
    assert!(!after.row_heights.is_empty(), "row heights must survive");
    assert!(after.freeze_rows > 0 || after.freeze_cols > 0, "freeze panes must survive");
    assert_eq!(after.conditional.len(), before.conditional.len(), "conditional rules must survive");
    assert_eq!(after.validations.len(), before.validations.len(), "validations must survive");
    // Charts are still export-only: the ChartML part is written but the
    // importer does not rebuild the chart model yet.
    assert!(after.charts.is_empty(), "charts are expected to remain export-only");
}
