//! Unit tests for `xlsx` (kept apart to respect the file-size ratchet).
use super::*;

fn sample_workbook() -> Workbook {
    let mut workbook = Workbook::new_blank("Budget");
    let sheet = &mut workbook.sheets[0];
    sheet.name = "Budget".into();
    sheet.set(
        "A1",
        Cell {
            value: CellValue::Text("Item".into()),
            style: CellStyle { bold: true, ..Default::default() },
            ..Default::default()
        },
    );
    sheet.set("B1", Cell { value: CellValue::Text("Qty".into()), ..Default::default() });
    sheet.set("C1", Cell { value: CellValue::Text("Price".into()), ..Default::default() });
    sheet.set("A2", Cell { value: CellValue::Text("Pen".into()), ..Default::default() });
    sheet.set("B2", Cell { value: CellValue::Number(10.0), ..Default::default() });
    sheet.set(
        "C2",
        Cell {
            value: CellValue::Number(5.5),
            style: CellStyle { number_format: "0.00".into(), ..Default::default() },
            ..Default::default()
        },
    );
    sheet.set("D2", Cell { value: CellValue::Number(55.0), formula: Some("=B2*C2".into()), ..Default::default() });
    sheet.set("A3", Cell { value: CellValue::Bool(true), ..Default::default() });
    sheet.merges.push(MergeRange { start: "A4".into(), end: "C4".into() });
    sheet.col_widths.insert(0, 140.0);
    sheet.row_heights.insert(0, 28.0);
    sheet.freeze_rows = 1;
    workbook.sheets.push(Sheet::new("Second"));
    workbook
}

#[test]
fn xlsx_package_structure() {
    let result = write_xlsx_package(&sample_workbook()).unwrap();
    let reader = crate::zip::ZipReader::open(result.bytes.clone()).unwrap();
    assert!(reader.contains("[Content_Types].xml"));
    assert!(reader.contains("xl/workbook.xml"));
    assert!(reader.contains("xl/worksheets/sheet1.xml"));
    assert!(reader.contains("xl/worksheets/sheet2.xml"));
    assert!(reader.contains("xl/styles.xml"));
    assert!(reader.contains("xl/sharedStrings.xml"));
    let sheet = reader.read_text("xl/worksheets/sheet1.xml").unwrap();
    assert!(sheet.contains("<f>B2*C2</f>"));
    assert!(sheet.contains("mergeCell"));
    assert!(sheet.contains("pane"));
}

#[test]
fn xlsx_roundtrip_values() {
    let bytes = write_xlsx(&sample_workbook()).unwrap();
    let read = read_workbook_bytes(&bytes).unwrap();
    assert_eq!(read.workbook.sheets.len(), 2);
    let sheet = &read.workbook.sheets[0];
    assert_eq!(sheet.get("A1").map(|cell| cell.value.clone()), Some(CellValue::Text("Item".into())));
    assert_eq!(sheet.get("B2").map(|cell| cell.value.clone()), Some(CellValue::Number(10.0)));
    let formula = sheet.get("D2").and_then(|cell| cell.formula.clone());
    assert_eq!(formula.as_deref(), Some("=B2*C2"));
}

#[test]
fn rejects_garbage() {
    assert!(read_workbook_bytes(b"").is_err());
    assert!(read_workbook_bytes(&[0u8; 32]).is_err());
    assert!(read_workbook_bytes(b"PK\x03\x04\x14\x00\x00\x00\x08\x00corrupt_payload").is_err());
}

fn chart_placement(kind: &str) -> ChartPlacement {
    ChartPlacement {
        id: "chart-1".into(),
        chart: ChartData {
            kind: kind.into(),
            title: "Sales".into(),
            categories: "A2:A4".into(),
            series: vec![ChartSeries { name: "Revenue".into(), range: "B2:B4".into(), color: None }],
            legend: true,
            x_title: "Month".into(),
            y_title: "EUR".into(),
            stacked: false,
            show_labels: true,
            ..Default::default()
        },
        anchor: "D2".into(),
        width_px: 420.0,
        height_px: 260.0,
    }
}

#[test]
fn xlsx_exports_charts_as_chartml_parts() {
    let mut workbook = sample_workbook();
    workbook.sheets[0].charts.push(chart_placement("column"));
    let result = write_xlsx_package(&workbook).unwrap();
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    let reader = crate::zip::ZipReader::open(result.bytes.clone()).unwrap();
    assert!(reader.contains("xl/charts/chart1.xml"));
    assert!(reader.contains("xl/drawings/drawing1.xml"));
    assert!(reader.contains("xl/drawings/_rels/drawing1.xml.rels"));
    let sheet = reader.read_text("xl/worksheets/sheet1.xml").unwrap();
    assert!(sheet.contains("<drawing r:id=\"rIdDrawing\"/>"));
    let rels = reader.read_text("xl/worksheets/_rels/sheet1.xml.rels").unwrap();
    assert!(rels.contains("../drawings/drawing1.xml"));
    let chart = reader.read_text("xl/charts/chart1.xml").unwrap();
    assert!(chart.contains("<c:barChart>"));
    assert!(chart.contains("<c:barDir val=\"col\"/>"));
    assert!(chart.contains("Budget!$B$2:$B$4"));
    assert!(chart.contains("Budget!$A$2:$A$4"));
    assert!(chart.contains("<c:showVal val=\"1\"/>"));
    assert!(chart.contains("<c:title>"));
    let content_types = reader.read_text("[Content_Types].xml").unwrap();
    assert!(content_types.contains("/xl/charts/chart1.xml"));
    assert!(content_types.contains("/xl/drawings/drawing1.xml"));
}

#[test]
fn conditional_formatting_kinds_map_to_valid_cfrules() {
    let mut workbook = sample_workbook();
    let sheet = &mut workbook.sheets[0];
    let rules: Vec<(&str, Vec<String>, Option<u32>)> = vec![
        ("greater", vec!["10".into()], None),
        ("less", vec!["5".into()], None),
        ("between", vec!["1".into(), "9".into()], None),
        ("equal", vec!["7".into()], None),
        ("textContains", vec!["foo".into()], None),
        ("text", vec!["legacy".into()], None),
        ("duplicate", vec![], None),
        ("duplicates", vec![], None),
        ("top", vec!["3".into()], Some(3)),
        ("bottom", vec![], Some(5)),
        ("dataBar", vec![], None),
    ];
    for (kind, values, top_n) in rules {
        sheet.conditional.push(CondRule {
            id: format!("rule-{kind}"),
            range: "B2:B5".into(),
            kind: kind.into(),
            values,
            fill: Some("#FF0000".into()),
            color: None,
            top_n,
            stop_if_true: false,
            ..Default::default()
        });
    }
    let bytes = write_xlsx(&workbook).unwrap();
    let reader = crate::zip::ZipReader::open(bytes).unwrap();
    let xml = reader.read_text("xl/worksheets/sheet1.xml").unwrap();
    assert!(xml.contains("sqref=\"B2:B5\""));
    assert!(xml.contains("operator=\"greaterThan\""));
    assert!(xml.contains("operator=\"lessThan\""));
    assert!(xml.contains("operator=\"between\""));
    assert!(xml.contains("operator=\"equal\""));
    assert!(xml.contains("type=\"containsText\""));
    assert!(xml.contains("text=\"foo\""));
    assert!(xml.contains("text=\"legacy\""));
    assert_eq!(xml.matches("type=\"duplicateValues\"").count(), 2);
    assert!(xml.contains("type=\"top10\"") && xml.contains("rank=\"3\""));
    assert!(xml.contains("bottom=\"1\"") && xml.contains("rank=\"5\""));
    assert!(xml.contains("type=\"dataBar\""));
    assert!(xml.contains("<color rgb=\"FFFF0000\"/>"));
}

#[test]
fn content_types_stay_closed_after_every_optional_part() {
    let mut workbook = sample_workbook();
    workbook.sheets[0]
        .set("E1", Cell { value: CellValue::Text("note".into()), comment: Some("check".into()), ..Default::default() });
    workbook.sheets[0].charts.push(chart_placement("column"));
    let bytes = write_xlsx(&workbook).unwrap();
    let reader = crate::zip::ZipReader::open(bytes).unwrap();
    let content_types = reader.read_text("[Content_Types].xml").unwrap();
    assert!(content_types.ends_with("</Types>"), "{content_types}");
    assert_eq!(content_types.matches("</Types>").count(), 1);
    assert!(content_types.contains("/xl/comments1.xml"));
    assert!(content_types.contains("/xl/charts/chart1.xml"));
}

#[test]
fn comments_stay_on_their_own_sheet_across_a_round_trip() {
    let mut workbook = sample_workbook();
    workbook.sheets[0].set(
        "A1",
        Cell { value: CellValue::Text("first".into()), comment: Some("budget note".into()), ..Default::default() },
    );
    workbook.sheets[1].set(
        "A1",
        Cell {
            value: CellValue::Text("second".into()),
            comment: Some("second sheet note".into()),
            ..Default::default()
        },
    );
    let bytes = write_xlsx(&workbook).unwrap();
    let reader = crate::zip::ZipReader::open(bytes.clone()).unwrap();
    assert!(reader.contains("xl/comments1.xml"));
    assert!(reader.contains("xl/comments2.xml"));
    assert!(reader.contains("xl/drawings/vmlDrawing1.vml"));
    assert!(reader.contains("xl/drawings/vmlDrawing2.vml"));
    let content_types = reader.read_text("[Content_Types].xml").unwrap();
    assert_eq!(content_types.matches("spreadsheetml.comments+xml").count(), 2, "{content_types}");
    let first_rels = reader.read_text("xl/worksheets/_rels/sheet1.xml.rels").unwrap();
    assert!(first_rels.contains("../comments1.xml"), "{first_rels}");
    let second_rels = reader.read_text("xl/worksheets/_rels/sheet2.xml.rels").unwrap();
    assert!(second_rels.contains("../comments2.xml"), "{second_rels}");
    let first_comments = reader.read_text("xl/comments1.xml").unwrap();
    assert!(first_comments.contains("budget note"));
    assert!(!first_comments.contains("second sheet note"));
    let read = read_workbook_bytes(&bytes).unwrap();
    assert_eq!(read.workbook.sheets[0].get("A1").and_then(|cell| cell.comment.clone()), Some("budget note".into()));
    assert_eq!(
        read.workbook.sheets[1].get("A1").and_then(|cell| cell.comment.clone()),
        Some("second sheet note".into())
    );
}

#[test]
fn pie_charts_skip_axes_and_use_a_single_vary_colors_series() {
    let mut workbook = sample_workbook();
    workbook.sheets[0].charts.push(chart_placement("pie"));
    let bytes = write_xlsx(&workbook).unwrap();
    let reader = crate::zip::ZipReader::open(bytes).unwrap();
    let chart = reader.read_text("xl/charts/chart1.xml").unwrap();
    assert!(chart.contains("<c:pieChart>"));
    assert!(chart.contains("<c:varyColors val=\"1\"/>"));
    assert!(!chart.contains("<c:catAx>"));
}

#[test]
fn bar_and_stacked_column_charts_use_the_right_direction() {
    let mut workbook = sample_workbook();
    workbook.sheets[0].charts.push(chart_placement("bar"));
    let mut stacked = chart_placement("column");
    stacked.chart.stacked = true;
    workbook.sheets[0].charts.push(stacked);
    let bytes = write_xlsx(&workbook).unwrap();
    let reader = crate::zip::ZipReader::open(bytes).unwrap();
    let first = reader.read_text("xl/charts/chart1.xml").unwrap();
    assert!(first.contains("<c:barDir val=\"bar\"/>"));
    let second = reader.read_text("xl/charts/chart2.xml").unwrap();
    assert!(second.contains("<c:grouping val=\"stacked\"/>"));
    assert!(second.contains("<c:overlap val=\"100\"/>"));
}

#[test]
fn unsupported_chart_kinds_are_reported_and_not_written() {
    let mut workbook = sample_workbook();
    workbook.sheets[0].charts.push(chart_placement("radar"));
    let result = write_xlsx_package(&workbook).unwrap();
    assert!(result.warnings.iter().any(|warning| warning.contains("radar")), "{:?}", result.warnings);
    let reader = crate::zip::ZipReader::open(result.bytes.clone()).unwrap();
    assert!(!reader.contains("xl/charts/chart1.xml"));
}

#[test]
fn chart_ranges_that_cannot_be_read_are_reported() {
    let mut workbook = sample_workbook();
    let mut broken = chart_placement("line");
    broken.chart.series[0].range = "not a range".into();
    workbook.sheets[0].charts.push(broken);
    let result = write_xlsx_package(&workbook).unwrap();
    assert!(result.warnings.iter().any(|warning| warning.contains("kept in the .oswk")), "{:?}", result.warnings);
}

#[test]
fn chart_references_quote_sheet_names_with_spaces() {
    let mut workbook = sample_workbook();
    workbook.sheets[0].name = "Q1 Sales".into();
    workbook.sheets[0].charts.push(chart_placement("line"));
    let bytes = write_xlsx(&workbook).unwrap();
    let reader = crate::zip::ZipReader::open(bytes).unwrap();
    let chart = reader.read_text("xl/charts/chart1.xml").unwrap();
    assert!(chart.contains("'Q1 Sales'!$B$2:$B$4"));
    assert!(chart.contains("<c:lineChart>"));
}

#[test]
fn chart_references_escape_sheet_names_that_break_xml() {
    let mut workbook = sample_workbook();
    workbook.sheets[0].name = "R&D <2026>".into();
    workbook.sheets[0].charts.push(chart_placement("column"));
    let bytes = write_xlsx(&workbook).unwrap();
    let reader = crate::zip::ZipReader::open(bytes).unwrap();
    let chart = reader.read_text("xl/charts/chart1.xml").unwrap();
    assert!(chart.contains("&amp;"));
    assert!(chart.contains("&lt;2026&gt;"));
    assert!(!chart.contains("R&D <2026>"));
    // The full part must still be well-formed XML.
    assert!(chart.ends_with("</c:chartSpace>"));
}

#[test]
fn reads_own_file_with_styles() {
    let mut workbook = Workbook::new_blank("Styles");
    let sheet = &mut workbook.sheets[0];
    sheet.set(
        "A1",
        Cell {
            value: CellValue::Text("styled".into()),
            style: CellStyle {
                bold: true,
                italic: true,
                fill: Some("#FFF2CC".into()),
                color: Some("#7F6000".into()),
                number_format: String::new(),
                borders: CellBorders {
                    top: Some(BorderStyle { style: "thin".into(), color: "#000000".into() }),
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let bytes = write_xlsx(&workbook).unwrap();
    let reader = crate::zip::ZipReader::open(bytes.clone()).unwrap();
    let styles = reader.read_text("xl/styles.xml").unwrap();
    assert!(styles.contains("FFF2CC"));
    assert!(styles.contains("<b/>"));
}
