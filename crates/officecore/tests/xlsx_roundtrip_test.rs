//! XLSX round-trip and export-structure tests.
//!
//! The workflow these cover is the one a user performs:
//!
//!   build/edit a workbook -> save XLSX -> reopen XLSX -> compare the model
//!
//! Import reads values and formulas through `calamine`; cell styling, merges,
//! column widths, freeze panes, validations, conditional formatting, charts,
//! pictures, print settings, protection and preserved pivot parts are all
//! rebuilt by the V3.1 OOXML pass and compared on the model. Values and
//! formulas are compared cell by cell with no loss permitted.

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
    sheet.set(
        "A1",
        Cell {
            value: CellValue::Text("Month".into()),
            style: CellStyle { bold: true, ..Default::default() },
            ..Default::default()
        },
    );
    sheet.set(
        "B1",
        Cell {
            value: CellValue::Text("Sales".into()),
            style: CellStyle { bold: true, ..Default::default() },
            ..Default::default()
        },
    );
    sheet.set(
        "C1",
        Cell {
            value: CellValue::Text("Cost".into()),
            style: CellStyle { bold: true, ..Default::default() },
            ..Default::default()
        },
    );
    sheet.set(
        "D1",
        Cell {
            value: CellValue::Text("Margin".into()),
            style: CellStyle { bold: true, ..Default::default() },
            ..Default::default()
        },
    );
    for row in 2..=101u32 {
        let index = row - 1;
        sheet.set(
            &format!("A{row}"),
            Cell { value: CellValue::Text(format!("2026-{:02}", (index % 12) + 1)), ..Default::default() },
        );
        sheet.set(&format!("B{row}"), Cell { value: CellValue::Number(index as f64 * 10.0), ..Default::default() });
        sheet.set(
            &format!("C{row}"),
            Cell {
                value: CellValue::Number(index as f64 * 4.0),
                style: CellStyle { number_format: "#,##0.00".into(), ..Default::default() },
                ..Default::default()
            },
        );
        sheet.set(
            &format!("D{row}"),
            Cell {
                value: CellValue::Number(index as f64 * 6.0),
                formula: Some(format!("=B{row}-C{row}")),
                style: CellStyle { number_format: "#,##0.00".into(), ..Default::default() },
                ..Default::default()
            },
        );
    }
    sheet.set(
        "E1",
        Cell {
            value: CellValue::Number(46023.0),
            style: CellStyle { number_format: "dd.mm.yyyy".into(), ..Default::default() },
            ..Default::default()
        },
    );
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
            ..Default::default()
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
    summary.set(
        "B2",
        Cell { value: CellValue::Number(50500.0), formula: Some("=SUM(Data!B2:B101)".into()), ..Default::default() },
    );
    summary.set("A3", Cell { value: CellValue::Text("Rows".into()), ..Default::default() });
    summary.set(
        "B3",
        Cell { value: CellValue::Number(100.0), formula: Some("=COUNTA(Data!A2:A101)".into()), ..Default::default() },
    );
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
            ..Default::default()
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
    assert!(sheet_by_name(&read.workbook, "Data").is_some(), "sheet names are preserved");

    let mut value_losses = Vec::new();
    let mut formula_losses = Vec::new();
    for (before, after) in original.sheets.iter().zip(read.workbook.sheets.iter()) {
        for (address, cell) in &before.cells {
            let Some(round) = after.get(address) else {
                value_losses.push(format!("{}!{address} disappeared", before.name));
                continue;
            };
            if cell_text(&cell.value) != cell_text(&round.value) {
                value_losses.push(format!("{}!{address}: {:?} -> {:?}", before.name, cell.value, round.value));
            }
            if cell.formula != round.formula {
                formula_losses.push(format!("{}!{address}: {:?} -> {:?}", before.name, cell.formula, round.formula));
            }
        }
    }
    assert!(value_losses.is_empty(), "value loss after round trip: {value_losses:#?}");
    assert!(formula_losses.is_empty(), "formula loss after round trip: {formula_losses:#?}");
}

#[test]
fn xlsx_roundtrip_keeps_a_hyperlink_on_a_blank_cell() {
    // Regression: `Cell::is_empty` ignored `link`, so a link-only cell (which
    // is what the importer itself creates) was skipped on export and lost.
    let mut workbook = Workbook::new_blank("Links");
    workbook.sheets[0].name = "Links".into();
    workbook.sheets[0].set("B2", Cell { link: Some("https://example.org/report".into()), ..Default::default() });
    let bytes = xlsx::write_xlsx(&workbook).unwrap();
    let read = xlsx::read_workbook_bytes(&bytes).unwrap();
    let sheet = sheet_by_name(&read.workbook, "Links").expect("sheet");
    assert_eq!(
        sheet.get("B2").and_then(|cell| cell.link.as_deref()),
        Some("https://example.org/report"),
        "a hyperlink on a blank cell must survive the round trip"
    );
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
                    value: if numeric {
                        CellValue::Number(value.parse().unwrap())
                    } else {
                        CellValue::Text((*value).into())
                    },
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
/// import pass; V3.1 rebuilt the parts the exporter writes (charts, pictures,
/// print settings, protection and pivot caches) so this now checks the model
/// instead of accepting the loss.
#[test]
fn xlsx_roundtrip_preserves_values_and_presentation_metadata() {
    let original = golden_workbook();
    let bytes = xlsx::write_xlsx(&original).unwrap();
    let read = xlsx::read_workbook_bytes(&bytes).unwrap();

    let before = &original.sheets[0];
    let after = sheet_by_name(&read.workbook, "Data").unwrap();
    assert_eq!(after.cells.len(), before.cells.len(), "every value and formula cell must survive the round trip");
    assert_eq!(after.merges.len(), before.merges.len(), "merges must survive");
    assert!(!after.col_widths.is_empty(), "column widths must survive");
    assert!(!after.row_heights.is_empty(), "row heights must survive");
    assert!(after.freeze_rows > 0 || after.freeze_cols > 0, "freeze panes must survive");
    assert_eq!(after.conditional.len(), before.conditional.len(), "conditional rules must survive");
    assert_eq!(after.validations.len(), before.validations.len(), "validations must survive");

    // Charts: the ChartML part is read back into the model, anchors and all.
    assert_eq!(after.charts.len(), before.charts.len(), "charts must survive the round trip");
    let chart = after.charts.iter().find(|chart| chart.chart.title == "Sales vs cost").expect("column chart");
    assert_eq!(chart.chart.kind, "column");
    assert_eq!(chart.anchor, "F5");
    assert_eq!(chart.chart.categories, "A2:A13");
    assert_eq!(chart.chart.series.len(), 2);
    assert_eq!(chart.chart.series[0].name, "Sales");
    assert_eq!(chart.chart.series[0].range, "B2:B13");
    assert_eq!(chart.chart.series[0].color.as_deref(), Some("#4472C4"));
    assert_eq!(chart.chart.series[1].name, "Cost");
    assert_eq!(chart.chart.x_title, "Month");
    assert_eq!(chart.chart.y_title, "EUR");
    assert!(chart.chart.legend);
    assert!((chart.width_px - 420.0).abs() < 0.01, "chart width drifted: {}", chart.width_px);
    assert!((chart.height_px - 260.0).abs() < 0.01, "chart height drifted: {}", chart.height_px);

    let summary = sheet_by_name(&read.workbook, "Summary").unwrap();
    let pie = summary.charts.first().expect("pie chart");
    assert_eq!(pie.chart.kind, "pie");
    assert_eq!(pie.chart.title, "Share");
    assert_eq!(pie.chart.categories, "A2:A3");
    assert_eq!(pie.chart.series[0].range, "B2:B3");
    assert!(pie.chart.show_labels);
    assert_eq!(pie.anchor, "D2");
}

// ---------------------------------------------------------------------------
// V3.1: pictures, print settings, protection and preserved pivots
// ---------------------------------------------------------------------------

/// A hand-written pivot cache definition, as Excel writes one. The importer
/// must read the source range and field names out of it and keep the raw part.
const PIVOT_DEFINITION: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<pivotCacheDefinition xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" refreshOnLoad=\"1\" recordCount=\"3\"><cacheSource type=\"worksheet\"><worksheetSource ref=\"A1:C4\" sheet=\"Data\"/></cacheSource><cacheFields count=\"3\"><cacheField name=\"Department\" numFmtId=\"0\"><sharedItems count=\"2\"><s v=\"Hardware\"/><s v=\"Software\"/></sharedItems></cacheField><cacheField name=\"Year\" numFmtId=\"0\"><sharedItems count=\"1\"><n v=\"2025\"/></sharedItems></cacheField><cacheField name=\"Sales\" numFmtId=\"0\"><sharedItems containsString=\"0\" containsNumber=\"1\"/></cacheField></cacheFields></pivotCacheDefinition>";

/// The matching pivot table part.
const PIVOT_TABLE: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<pivotTableDefinition xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" name=\"PivotTable1\" cacheId=\"7\" dataOnRows=\"1\" dataCaption=\"Values\" updatedVersion=\"6\" minRefreshableVersion=\"3\" useAutoFormatting=\"1\" itemPrintTitles=\"1\" createdVersion=\"6\" indent=\"0\" compact=\"0\" compactData=\"0\" gridDropZones=\"1\"><location ref=\"F1:G3\" firstHeaderRow=\"1\" firstDataRow=\"1\" firstDataCol=\"1\"/><pivotFields count=\"3\"><pivotField axis=\"axisRow\" showAll=\"0\" compact=\"0\" outline=\"0\"><items count=\"2\"><item t=\"default\"/><item x=\"0\"/></items></pivotField><pivotField dataField=\"1\" showAll=\"0\" compact=\"0\" outline=\"0\"/><pivotField showAll=\"0\" compact=\"0\" outline=\"0\"/></pivotFields><rowFields count=\"1\"><field x=\"0\"/></rowFields><rowItems count=\"2\"><i><x/></i><i><x v=\"1\"/></i></rowItems><dataFields count=\"1\"><dataField name=\"Sum of Sales\" fld=\"2\" baseField=\"0\" baseItem=\"0\"/></dataFields></pivotTableDefinition>";

const PIVOT_RECORDS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<pivotCacheRecords xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" count=\"3\"><r><s v=\"Hardware\"/><n v=\"2025\"/><n v=\"100\"/></r><r><s v=\"Hardware\"/><n v=\"2025\"/><n v=\"150\"/></r><r><s v=\"Software\"/><n v=\"2025\"/><n v=\"200\"/></r></pivotCacheRecords>";

fn pivot_records_base64() -> String {
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, PIVOT_RECORDS.as_bytes())
}

/// A workbook exercising every V3.1 import target at once.
fn v31_workbook() -> Workbook {
    let mut workbook = Workbook::new_blank("V31");
    let sheet = &mut workbook.sheets[0];
    sheet.name = "Data".into();
    for row in 0..4u32 {
        for column in 0..3u32 {
            let address = officecore::address::format(row, column);
            let value = match (row, column) {
                (0, 0) => CellValue::Text("Department".into()),
                (0, 1) => CellValue::Text("Year".into()),
                (0, 2) => CellValue::Text("Sales".into()),
                (1, 0) => CellValue::Text("Hardware".into()),
                (1, 1) => CellValue::Number(2025.0),
                (1, 2) => CellValue::Number(100.0),
                (2, 0) => CellValue::Text("Hardware".into()),
                (2, 1) => CellValue::Number(2025.0),
                (2, 2) => CellValue::Number(150.0),
                (3, 0) => CellValue::Text("Software".into()),
                (3, 1) => CellValue::Number(2025.0),
                (3, 2) => CellValue::Number(200.0),
                _ => CellValue::Empty,
            };
            sheet.set(&address, Cell { value, ..Default::default() });
        }
    }

    let png = [0x89u8, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 1, 2, 3, 4];
    sheet.images.push(SheetImage {
        image: ImageData::from_bytes("logo.png", &png),
        anchor: CellAnchor { address: "B2".into(), col_off_emu: 95_250, row_off_emu: 47_625, ..Default::default() },
        width_px: 160.0,
        height_px: 90.0,
        rotation_deg: 15.0,
    });

    sheet.charts.push(ChartPlacement {
        id: "chart-1".into(),
        chart: ChartData {
            kind: "column".into(),
            title: "Sales by department".into(),
            categories: "A2:A4".into(),
            series: vec![ChartSeries { name: "Sales".into(), range: "C2:C4".into(), color: Some("#4472C4".into()) }],
            legend: true,
            x_title: "Department".into(),
            y_title: "EUR".into(),
            stacked: false,
            show_labels: true,
            categories_cache: vec!["Hardware".into(), "Hardware".into(), "Software".into()],
            series_values_cache: vec![vec![100.0, 150.0, 200.0]],
            ..Default::default()
        },
        anchor: "E2".into(),
        width_px: 420.0,
        height_px: 260.0,
    });

    sheet.print = PrintSettings {
        paper_size: 1,
        landscape: true,
        scale: 85,
        fit_to_width: 2,
        fit_to_height: 3,
        center_horizontally: true,
        center_vertically: true,
        print_gridlines: true,
        print_headings: true,
        print_titles_rows: Some("1:1".into()),
        print_titles_cols: Some("A:A".into()),
        print_area: Some("A1:C4".into()),
        different_first_page: true,
        different_odd_even: true,
        header: "Report".into(),
        footer: "Page &P".into(),
        margin_left: 0.5,
        margin_right: 0.6,
        margin_top: 0.7,
        margin_bottom: 0.8,
        margin_header: 0.2,
        margin_footer: 0.25,
        first_header: "First".into(),
        first_footer: "First bottom".into(),
        even_header: "Even".into(),
        even_footer: "Even bottom".into(),
        row_breaks: vec![5, 9],
        col_breaks: vec![2],
    };

    sheet.protection = SheetProtection {
        enabled: true,
        password_hash: Some("ABCD".into()),
        algorithm_name: "SHA-512".into(),
        hash_value: "aGFzaA==".into(),
        salt_value: "c2FsdA==".into(),
        spin_count: 100_000,
        options: vec!["formatCells".into(), "objects".into()],
    };

    workbook.preserved_pivots.push(PreservedPivot {
        name: "PivotTable1".into(),
        sheet: "Data".into(),
        cache_id: 7,
        definition_xml: PIVOT_DEFINITION.into(),
        records_base64: Some(pivot_records_base64()),
        table_xml: PIVOT_TABLE.into(),
        records_part: Some("xl/pivotCache/pivotCacheRecords1.xml".into()),
        source: "Data!A1:C4".into(),
        fields: vec!["Department".into(), "Year".into(), "Sales".into()],
    });
    workbook
}

#[test]
fn xlsx_roundtrip_preserves_pictures_print_protection_and_pivots() {
    let original = v31_workbook();
    let bytes = xlsx::write_xlsx(&original).unwrap();
    let read = xlsx::read_workbook_bytes(&bytes).unwrap();
    let sheet = sheet_by_name(&read.workbook, "Data").expect("Data sheet");

    // Picture: bytes, anchor offsets, size and rotation all survive.
    assert_eq!(sheet.images.len(), 1, "the picture must survive the round trip");
    let image = &sheet.images[0];
    assert_eq!(image.image.mime, "image/png");
    assert_eq!(image.image.bytes(), original.sheets[0].images[0].image.bytes());
    assert_eq!(image.anchor.address, "B2");
    assert_eq!(image.anchor.col_off_emu, 95_250);
    assert_eq!(image.anchor.row_off_emu, 47_625);
    assert!((image.width_px - 160.0).abs() < 0.01);
    assert!((image.height_px - 90.0).abs() < 0.01);
    assert!((image.rotation_deg - 15.0).abs() < 0.01);

    // Chart: ranges, cache, titles and anchor come back exactly.
    let chart = sheet.charts.first().expect("chart");
    assert_eq!(chart.chart, original.sheets[0].charts[0].chart);
    assert_eq!(chart.anchor, "E2");
    assert!((chart.width_px - 420.0).abs() < 0.01);
    assert!((chart.height_px - 260.0).abs() < 0.01);

    // Print settings: every stored field round-trips.
    assert_eq!(sheet.print, original.sheets[0].print);

    // Protection: attributes and the locked-action list survive verbatim.
    assert_eq!(sheet.protection, original.sheets[0].protection);
    assert_eq!(sheet.sheet_protection, "ABCD");

    // Pivot: raw parts preserved and readable.
    assert_eq!(read.workbook.preserved_pivots.len(), 1);
    let pivot = &read.workbook.preserved_pivots[0];
    assert_eq!(pivot.name, "PivotTable1");
    assert_eq!(pivot.sheet, "Data");
    assert_eq!(pivot.cache_id, 7);
    assert_eq!(pivot.definition_xml, PIVOT_DEFINITION);
    assert_eq!(pivot.table_xml, PIVOT_TABLE);
    assert_eq!(pivot.records_base64.as_deref(), Some(pivot_records_base64().as_str()));
    assert_eq!(pivot.source, "Data!A1:C4");
    assert_eq!(pivot.fields, vec!["Department".to_string(), "Year".to_string(), "Sales".to_string()]);
    assert!(read.warnings.iter().any(|warning| warning.contains("pivot cache")), "{:?}", read.warnings);

    // A second write/read cycle must be byte-for-byte stable at the model level.
    let second = xlsx::write_xlsx(&read.workbook).unwrap();
    let read2 = xlsx::read_workbook_bytes(&second).unwrap();
    let sheet2 = sheet_by_name(&read2.workbook, "Data").expect("Data sheet");
    assert_eq!(sheet2.images, sheet.images, "picture drifted on a second cycle");
    assert_eq!(sheet2.charts, sheet.charts, "chart drifted on a second cycle");
    assert_eq!(sheet2.print, sheet.print, "print settings drifted on a second cycle");
    assert_eq!(sheet2.protection, sheet.protection, "protection drifted on a second cycle");
    assert_eq!(read2.workbook.preserved_pivots, read.workbook.preserved_pivots);
}

#[test]
fn xlsx_export_writes_picture_media_and_drawing_relationships() {
    let bytes = xlsx::write_xlsx(&v31_workbook()).unwrap();
    let reader = officecore::zip::ZipReader::open(bytes).unwrap();
    assert!(reader.contains("xl/media/image1.png"), "the media part is missing");
    let drawing = reader.read_text("xl/drawings/drawing1.xml").unwrap();
    assert!(drawing.contains("<xdr:pic>"));
    assert!(drawing.contains("rot=\"900000\""), "the rotation was not written: {drawing}");
    let rels = reader.read_text("xl/drawings/_rels/drawing1.xml.rels").unwrap();
    assert!(rels.contains("../media/image1.png"));
    let content_types = reader.read_text("[Content_Types].xml").unwrap();
    assert!(content_types.contains("Extension=\"png\" ContentType=\"image/png\""));
    assert!(content_types.ends_with("</Types>"));
    // The pivot parts and their wiring are in the package.
    assert!(reader.contains("xl/pivotTables/pivotTable1.xml"));
    assert!(reader.contains("xl/pivotCache/pivotCacheDefinition1.xml"));
    assert!(reader.contains("xl/pivotCache/pivotCacheRecords1.xml"));
    let workbook = reader.read_text("xl/workbook.xml").unwrap();
    assert!(workbook.contains("<pivotCaches><pivotCache cacheId=\"7\" r:id=\"rId4\"/></pivotCaches>"));
    assert!(workbook.contains("_xlnm.Print_Area"));
    assert!(workbook.contains("_xlnm.Print_Titles"));
    let sheet = reader.read_text("xl/worksheets/sheet1.xml").unwrap();
    assert!(sheet.contains("rowBreaks"));
    assert!(sheet.contains("sheetProtection"));
    assert!(!sheet.contains("pivotTablePart"), "pivot tables are linked through rels, not worksheet elements");
}

/// A calc unit written by V3.0 has no `images`, `protection` or
/// `preservedPivots` keys and its `PrintSettings` predates the V3.1 fields.
/// The schema version stays 3, so loading must be carried by serde defaults.
#[test]
fn a_v3_calc_unit_without_the_v31_fields_still_loads() {
    let legacy = r#"{
        "id": "wb",
        "title": "Legacy",
        "activeSheet": 0,
        "names": [],
        "metadata": { "title": "Legacy" },
        "sheets": [{
            "id": "s1",
            "name": "Sheet1",
            "rowCount": 200,
            "colCount": 26,
            "cells": {},
            "colWidths": {},
            "rowHeights": {},
            "merges": [],
            "freezeRows": 0,
            "freezeCols": 0,
            "charts": [],
            "pivotTables": [],
            "conditional": [],
            "validations": [],
            "filter": null,
            "showGridlines": true,
            "tabColor": null,
            "print": {
                "paperSize": 9, "landscape": false, "scale": 100,
                "fitToWidth": 1, "fitToHeight": 0, "centerHorizontally": false,
                "printGridlines": false, "printHeadings": false,
                "printTitlesRows": null, "differentFirstPage": false,
                "differentOddEven": false, "header": "", "footer": ""
            },
            "sheetProtection": ""
        }]
    }"#;
    let model: serde_json::Value = serde_json::from_str(legacy).unwrap();
    let mut unit = serde_json::json!({ "kind": "calc", "version": 3, "model": model });
    let report = officecore::schema::migrate_unit(&mut unit).unwrap();
    assert!(!report.migrated, "schema 3 is already current");
    let workbook: Workbook = serde_json::from_value(unit["model"].clone()).unwrap();
    assert!(workbook.preserved_pivots.is_empty());
    assert!(workbook.sheets[0].images.is_empty());
    assert!(!workbook.sheets[0].protection.enabled);
    assert_eq!(workbook.sheets[0].print.margin_left, 0.7);
    assert_eq!(workbook.sheets[0].print.fit_to_width, 1);
    assert!(workbook.sheets[0].print.row_breaks.is_empty());
}

/// A picture that came from a two-cell anchor is written back as a two-cell
/// anchor, so both corners (and therefore the size Excel derives from them)
/// survive a second cycle.
#[test]
fn xlsx_two_cell_anchored_picture_round_trips_its_corners() {
    let mut workbook = Workbook::new_blank("TwoCell");
    workbook.sheets[0].name = "Data".into();
    workbook.sheets[0].set("A1", Cell { value: CellValue::Text("x".into()), ..Default::default() });
    workbook.sheets[0].images.push(SheetImage {
        image: ImageData::from_bytes("p.png", &[0x89, b'P', b'N', b'G', 1, 2, 3, 4]),
        anchor: CellAnchor {
            address: "B2".into(),
            col_off_emu: 9_525,
            row_off_emu: 19_050,
            to_address: Some("E8".into()),
            to_col_off_emu: 38_100,
            to_row_off_emu: 47_625,
        },
        width_px: 200.0,
        height_px: 120.0,
        rotation_deg: 0.0,
    });

    let bytes = xlsx::write_xlsx(&workbook).unwrap();
    let reader = officecore::zip::ZipReader::open(bytes.clone()).unwrap();
    let drawing = reader.read_text("xl/drawings/drawing1.xml").unwrap();
    assert!(drawing.contains("<xdr:twoCellAnchor"), "the corners were flattened: {drawing}");
    assert!(!drawing.contains("<xdr:ext "), "a two-cell anchor must not carry an extent");

    let read = xlsx::read_workbook_bytes(&bytes).unwrap();
    let image = &read.workbook.sheets[0].images[0];
    assert_eq!(image.anchor.address, "B2");
    assert_eq!(image.anchor.to_address.as_deref(), Some("E8"));
    assert_eq!(image.anchor.col_off_emu, 9_525);
    assert_eq!(image.anchor.to_row_off_emu, 47_625);
    // The size is derived from the corners: 3 default columns plus the offsets
    // (192 + 3 px wide) and 6 default rows plus the offsets (120 + 3 px tall).
    assert!((image.width_px - 195.0).abs() < 0.01, "width: {}", image.width_px);
    assert!((image.height_px - 123.0).abs() < 0.01, "height: {}", image.height_px);

    let second = xlsx::write_xlsx(&read.workbook).unwrap();
    let read2 = xlsx::read_workbook_bytes(&second).unwrap();
    assert_eq!(read2.workbook.sheets[0].images, read.workbook.sheets[0].images);
}

#[test]
fn xlsx_list_validation_from_cells_is_a_bare_reference() {
    let mut workbook = Workbook::new_blank("Lists");
    let sheet = &mut workbook.sheets[0];
    let list = |id: &str, range: &str, values: &[&str]| Validation {
        id: id.into(),
        range: range.into(),
        kind: "list".into(),
        values: values.iter().map(|value| value.to_string()).collect(),
        allow_blank: true,
        ..Default::default()
    };
    sheet.validations = vec![
        list("v1", "B1:B9", &["=A1:A5"]),
        list("v2", "C1", &["='My Sheet'!$A$1:$A$3"]),
        list("v3", "D1", &["Yes", "No"]),
        list("v4", "E1", &["=SUM(A1:A2)"]),
    ];
    let bytes = xlsx::write_xlsx(&workbook).unwrap();
    let xml = ZipReader::open(bytes.clone()).unwrap().read_text("xl/worksheets/sheet1.xml").unwrap();
    assert!(xml.contains("<formula1>A1:A5</formula1>"), "{xml}");
    assert!(xml.contains("<formula1>'My Sheet'!$A$1:$A$3</formula1>"), "{xml}");
    assert!(xml.contains("<formula1>\"Yes,No\"</formula1>"), "{xml}");
    // Only plain references are written bare; anything else stays a literal.
    assert!(xml.contains("<formula1>\"=SUM(A1:A2)\"</formula1>"), "{xml}");

    let read = xlsx::read_workbook_bytes(&bytes).unwrap();
    let values: Vec<Vec<String>> =
        read.workbook.sheets[0].validations.iter().map(|validation| validation.values.clone()).collect();
    assert_eq!(values[0], vec!["=A1:A5".to_string()]);
    assert_eq!(values[1], vec!["='My Sheet'!$A$1:$A$3".to_string()]);
    assert_eq!(values[2], vec!["Yes".to_string(), "No".to_string()]);
}
