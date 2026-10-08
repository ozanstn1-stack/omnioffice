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
    CellStyle { bold: true, fill: Some("#DDEBF7".into()), color: Some("#1F4E78".into()), ..Default::default() }
}

fn fidelity_workbook() -> Workbook {
    let mut workbook = Workbook::new_blank("Fidelity");
    let sheet = &mut workbook.sheets[0];
    sheet.name = "Sales".into();
    sheet.tab_color = Some("#FF0000".into());

    sheet.set("A1", Cell { value: CellValue::Text("Region".into()), style: header_style(), ..Default::default() });
    sheet.set("B1", Cell { value: CellValue::Text("Amount".into()), style: header_style(), ..Default::default() });
    sheet.set("A2", Cell { value: CellValue::Text("North".into()), ..Default::default() });
    sheet.set(
        "B2",
        Cell {
            value: CellValue::Number(1200.5),
            style: CellStyle { number_format: "#,##0.00".into(), ..Default::default() },
            ..Default::default()
        },
    );
    sheet.set(
        "A3",
        Cell { value: CellValue::Text("South".into()), comment: Some("check this".into()), ..Default::default() },
    );
    sheet.set(
        "B3",
        Cell {
            value: CellValue::Number(900.0),
            style: CellStyle { number_format: "#,##0.00".into(), ..Default::default() },
            ..Default::default()
        },
    );
    sheet.set(
        "C3",
        Cell {
            value: CellValue::Text("site".into()),
            link: Some("https://example.org/report".into()),
            ..Default::default()
        },
    );
    sheet.set("D3", Cell { value: CellValue::Number(2401.0), formula: Some("=B2*2".into()), ..Default::default() });
    sheet.set(
        "E3",
        Cell {
            value: CellValue::Number(46023.0),
            style: CellStyle { number_format: "dd.mm.yyyy".into(), ..Default::default() },
            ..Default::default()
        },
    );
    sheet.set("E5", Cell { comment: Some("standalone note".into()), ..Default::default() });
    sheet.set(
        "A6",
        Cell {
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
        },
    );

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
        ..Default::default()
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
        ..Default::default()
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

    // Conditional rules: greater-than with its own highlight dxf, plus the
    // data bar whose colour lives in the rule itself.
    assert_eq!(sheet.conditional.len(), 2);
    let rule = sheet.conditional.iter().find(|rule| rule.kind == "greater").expect("greater rule");
    assert_eq!(rule.range, "B2:B3");
    assert_eq!(rule.values, vec!["1000".to_string()]);
    assert_eq!(rule.fill.as_deref(), Some("#FFC7CE"), "the rule's own colour survives, not a shared one");
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
    assert!(sheet_xml.contains(
        "<tableParts count=\"2\"><tablePart r:id=\"rIdTable0\"/><tablePart r:id=\"rIdTable1\"/></tableParts>"
    ));
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
    let broken = rebuild(&bytes, &[("xl/styles.xml", "<styleSheet><cellXfs"), ("xl/tables/table1.xml", "<table")]);
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

// ---------------------------------------------------------------------------
// V3.1: foreign drawing, print, protection and pivot fixtures
// ---------------------------------------------------------------------------

/// Rebuilds a package with parts replaced and extra parts appended, so the
/// importer can be fed files our own writer would never produce.
fn rebuild_with(bytes: &[u8], replacements: &[(&str, &str)], additions: &[(&str, &[u8])]) -> Vec<u8> {
    let reader = ZipReader::open(bytes.to_vec()).unwrap();
    let mut writer = ZipWriter::new();
    let mut written: Vec<String> = Vec::new();
    for (name, data) in reader.read_all(ZipLimits::default()).unwrap() {
        match replacements.iter().find(|(part, _)| *part == name) {
            Some((_, replacement)) => writer.add_text(&name, replacement),
            None => writer.add(&name, &data),
        }
        written.push(name);
    }
    for (name, data) in additions {
        if !written.iter().any(|existing| existing == name) {
            writer.add(name, data);
        }
    }
    writer.finish()
}

/// A worksheet from a package that is not ours: inline strings, print options
/// in a foreign shape, SHA-512 protection and manual page breaks.
const FOREIGN_SHEET: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><dimension ref=\"A1:C4\"/><sheetViews><sheetView workbookViewId=\"0\"/></sheetViews><sheetFormatPr defaultRowHeight=\"15\"/><sheetData><row r=\"1\"><c r=\"A1\" t=\"inlineStr\"><is><t>Q1</t></is></c><c r=\"B1\" t=\"inlineStr\"><is><t>Revenue</t></is></c></row><row r=\"2\"><c r=\"A2\" t=\"inlineStr\"><is><t>Q1</t></is></c><c r=\"B2\"><v>10</v></c></row><row r=\"3\"><c r=\"A3\" t=\"inlineStr\"><is><t>Q2</t></is></c><c r=\"B3\"><v>20</v></c></row><row r=\"4\"><c r=\"A4\" t=\"inlineStr\"><is><t>Q3</t></is></c><c r=\"B4\"><v>30</v></c></row></sheetData><sheetProtection sheet=\"1\" algorithmName=\"SHA-512\" hashValue=\"aGFzaA==\" saltValue=\"c2FsdA==\" spinCount=\"50000\" selectLockedCells=\"1\" sort=\"1\"/><printOptions gridLines=\"1\" headings=\"1\" horizontalCentered=\"1\" verticalCentered=\"1\"/><pageMargins left=\"0.25\" right=\"0.5\" top=\"0.55\" bottom=\"0.65\" header=\"0.15\" footer=\"0.2\"/><pageSetup paperSize=\"1\" orientation=\"landscape\" scale=\"80\" fitToWidth=\"2\" fitToHeight=\"4\"/><headerFooter differentFirst=\"1\" differentOddEven=\"1\"><oddFooter>&amp;Lfooter left</oddFooter><evenHeader>&amp;Ceven head</evenHeader><firstHeader>&amp;Cfirst head</firstHeader><firstFooter>&amp;Cfirst foot</firstFooter></headerFooter><rowBreaks count=\"1\" manualBreakCount=\"1\"><brk id=\"7\" max=\"16383\" man=\"1\"/></rowBreaks><colBreaks count=\"1\" manualBreakCount=\"1\"><brk id=\"3\" max=\"1048575\" man=\"1\"/></colBreaks><drawing r:id=\"rIdDrawing\"/></worksheet>";

/// A two-cell anchored chart (no `ext`) and a one-cell anchored rotated
/// picture with `descr`, as Excel writes them.
const FOREIGN_DRAWING: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<xdr:wsDr xmlns:xdr=\"http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><xdr:twoCellAnchor editAs=\"oneCell\"><xdr:from><xdr:col>4</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>1</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>8</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>12</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to><xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"2\" name=\"Chart 1\"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/></xdr:xfrm><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" r:id=\"rId1\"/></a:graphicData></a:graphic></xdr:graphicFrame><xdr:clientData/></xdr:twoCellAnchor><xdr:oneCellAnchor><xdr:from><xdr:col>1</xdr:col><xdr:colOff>12700</xdr:colOff><xdr:row>14</xdr:row><xdr:rowOff>6350</xdr:rowOff></xdr:from><xdr:ext cx=\"952500\" cy=\"476250\"/><xdr:pic><xdr:nvPicPr><xdr:cNvPr id=\"3\" name=\"Logo\" descr=\"company logo\"/><xdr:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></xdr:cNvPicPr></xdr:nvPicPr><xdr:blipFill><a:blip xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" r:embed=\"rId2\"/><a:stretch><a:fillRect/></a:stretch></xdr:blipFill><xdr:spPr><a:xfrm rot=\"2700000\"><a:off x=\"0\" y=\"0\"/><a:ext cx=\"952500\" cy=\"476250\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic><xdr:clientData/></xdr:oneCellAnchor></xdr:wsDr>";

/// A ChartML part whose series names and categories only exist in caches.
const FOREIGN_CHART: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><c:roundedCorners val=\"0\"/><c:chart><c:title><c:tx><c:rich><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>Foreign chart</a:t></a:r></a:p></c:rich></c:tx></c:title><c:plotArea><c:layout/><c:lineChart><c:grouping val=\"stacked\"/><c:varyColors val=\"0\"/><c:ser><c:idx val=\"0\"/><c:order val=\"0\"/><c:tx><c:strRef><c:f>Data!$B$1</c:f><c:strCache><c:ptCount val=\"1\"/><c:pt idx=\"0\"><c:v>Revenue</c:v></c:pt></c:strCache></c:strRef></c:tx><c:cat><c:strRef><c:f>Data!$A$2:$A$4</c:f><c:strCache><c:ptCount val=\"3\"/><c:pt idx=\"0\"><c:v>Q1</c:v></c:pt><c:pt idx=\"1\"><c:v>Q2</c:v></c:pt><c:pt idx=\"2\"><c:v>Q3</c:v></c:pt></c:strCache></c:strRef></c:cat><c:val><c:numRef><c:f>Data!$B$2:$B$4</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"3\"/><c:pt idx=\"0\"><c:v>10</c:v></c:pt><c:pt idx=\"1\"><c:v>20</c:v></c:pt><c:pt idx=\"2\"><c:v>30</c:v></c:pt></c:numCache></c:numRef></c:val></c:ser><c:marker val=\"1\"/><c:axId val=\"111111111\"/><c:axId val=\"222222222\"/></c:lineChart><c:catAx><c:axId val=\"111111111\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"b\"/><c:title><c:tx><c:rich><a:p><a:r><a:t>Quarter</a:t></a:r></a:p></c:rich></c:tx></c:title><c:crossAx val=\"222222222\"/></c:catAx><c:valAx><c:axId val=\"222222222\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"l\"/><c:title><c:tx><c:rich><a:p><a:r><a:t>Units</a:t></a:r></a:p></c:rich></c:tx></c:title><c:crossAx val=\"111111111\"/></c:valAx></c:plotArea><c:legend><c:legendPos val=\"r\"/><c:overlay val=\"0\"/></c:legend><c:plotVisOnly val=\"1\"/><c:dispBlanksAs val=\"gap\"/></c:chart><c:printSettings><c:headerFooter/><c:pageMargins b=\"0.75\" l=\"0.7\" r=\"0.7\" t=\"0.75\" header=\"0.3\" footer=\"0.3\"/><c:pageSetup/></c:printSettings></c:chartSpace>";

const FOREIGN_PIVOT_DEFINITION: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<pivotCacheDefinition xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" refreshOnLoad=\"1\" recordCount=\"3\"><cacheSource type=\"worksheet\"><worksheetSource ref=\"A1:B4\" sheet=\"Data\"/></cacheSource><cacheFields count=\"2\"><cacheField name=\"Quarter\" numFmtId=\"0\"><sharedItems count=\"3\"><s v=\"Q1\"/><s v=\"Q2\"/><s v=\"Q3\"/></sharedItems></cacheField><cacheField name=\"Revenue\" numFmtId=\"0\"><sharedItems containsString=\"0\" containsNumber=\"1\"/></cacheField></cacheFields></pivotCacheDefinition>";

const FOREIGN_PIVOT_TABLE: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<pivotTableDefinition xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" name=\"PT_Foreign\" cacheId=\"3\" dataOnRows=\"1\" dataCaption=\"Values\" updatedVersion=\"6\" minRefreshableVersion=\"3\" itemPrintTitles=\"1\" createdVersion=\"6\" indent=\"0\" compact=\"0\" compactData=\"0\" gridDropZones=\"1\"><location ref=\"D1:E4\" firstHeaderRow=\"1\" firstDataRow=\"1\" firstDataCol=\"1\"/><pivotFields count=\"2\"><pivotField axis=\"axisRow\" showAll=\"0\"><items count=\"3\"><item t=\"default\"/><item x=\"0\"/><item x=\"1\"/></items></pivotField><pivotField dataField=\"1\" showAll=\"0\"/></pivotFields><rowFields count=\"1\"><field x=\"0\"/></rowFields><rowItems count=\"3\"><i><x/></i><i><x v=\"1\"/></i><i><x v=\"2\"/></i></rowItems><dataFields count=\"1\"><dataField name=\"Sum of Revenue\" fld=\"1\" baseField=\"0\" baseItem=\"0\"/></dataFields></pivotTableDefinition>";

const FOREIGN_PIVOT_RECORDS: &[u8] = b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<pivotCacheRecords xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" count=\"3\"><r><s v=\"Q1\"/><n v=\"10\"/></r><r><s v=\"Q2\"/><n v=\"20\"/></r><r><s v=\"Q3\"/><n v=\"30\"/></r></pivotCacheRecords>";

fn simple_picture() -> SheetImage {
    let png = [0x89u8, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 9, 8, 7, 6];
    SheetImage {
        image: ImageData::from_bytes("logo.png", &png),
        anchor: CellAnchor::at("G2"),
        width_px: 120.0,
        height_px: 60.0,
        rotation_deg: 0.0,
    }
}

#[test]
fn foreign_drawing_print_protection_and_pivots_import() {
    let mut workbook = Workbook::new_blank("Foreign");
    workbook.sheets[0].name = "Data".into();
    workbook.sheets[0].set("A1", Cell { value: CellValue::Text("Q1".into()), ..Default::default() });
    workbook.sheets[0].sheet_protection = "ABCD".into();
    workbook.sheets[0].charts.push(ChartPlacement {
        id: "chart-1".into(),
        chart: ChartData {
            kind: "column".into(),
            title: "Placeholder".into(),
            categories: "A2:A4".into(),
            series: vec![ChartSeries { name: "Revenue".into(), range: "B2:B4".into(), color: None }],
            legend: true,
            ..Default::default()
        },
        anchor: "A1".into(),
        width_px: 300.0,
        height_px: 200.0,
    });
    workbook.sheets[0].images.push(simple_picture());
    let bytes = xlsx::write_xlsx(&workbook).unwrap();

    // Point the worksheet's pivot relationship at a part we add below.
    let reader = ZipReader::open(bytes.clone()).unwrap();
    let sheet_rels = reader.read_text("xl/worksheets/_rels/sheet1.xml.rels").unwrap();
    let sheet_rels_foreign = sheet_rels.replace(
        "</Relationships>",
        "<Relationship Id=\"rIdPivot1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotTable\" Target=\"../pivotTables/pivotTable1.xml\"/></Relationships>",
    );
    let content_types = reader.read_text("[Content_Types].xml").unwrap();
    let content_types_foreign = content_types.replace(
        "</Types>",
        "<Override PartName=\"/xl/pivotTables/pivotTable1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.pivotTable+xml\"/><Override PartName=\"/xl/pivotCache/pivotCacheDefinition1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheDefinition+xml\"/><Override PartName=\"/xl/pivotCache/pivotCacheRecords1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheRecords+xml\"/></Types>",
    );
    let patched = rebuild_with(
        &bytes,
        &[
            ("xl/worksheets/sheet1.xml", FOREIGN_SHEET),
            ("xl/drawings/drawing1.xml", FOREIGN_DRAWING),
            ("xl/charts/chart1.xml", FOREIGN_CHART),
            ("xl/worksheets/_rels/sheet1.xml.rels", &sheet_rels_foreign),
            ("[Content_Types].xml", &content_types_foreign),
        ],
        &[
            ("xl/pivotTables/pivotTable1.xml", FOREIGN_PIVOT_TABLE.as_bytes()),
            (
                "xl/pivotTables/_rels/pivotTable1.xml.rels",
                b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotCacheDefinition\" Target=\"../pivotCache/pivotCacheDefinition1.xml\"/></Relationships>",
            ),
            ("xl/pivotCache/pivotCacheDefinition1.xml", FOREIGN_PIVOT_DEFINITION.as_bytes()),
            (
                "xl/pivotCache/_rels/pivotCacheDefinition1.xml.rels",
                b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotCacheRecords\" Target=\"pivotCacheRecords1.xml\"/></Relationships>",
            ),
            ("xl/pivotCache/pivotCacheRecords1.xml", FOREIGN_PIVOT_RECORDS),
        ],
    );

    let read = xlsx::read_workbook_bytes(&patched).unwrap();
    assert!(
        !read.warnings.iter().any(|warning| warning.contains("not imported")),
        "imported parts should not warn as unimported: {:?}",
        read.warnings
    );
    let sheet = read.workbook.sheets.iter().find(|sheet| sheet.name == "Data").expect("Data sheet");

    // Values still come from the inline-string fixture.
    assert_eq!(sheet.get("A2").map(|cell| cell.value.clone()), Some(CellValue::Text("Q1".into())));
    assert_eq!(sheet.get("B4").map(|cell| cell.value.clone()), Some(CellValue::Number(30.0)));

    // Print settings from a foreign `<pageSetup>`/`<pageMargins>`/`<headerFooter>`.
    assert_eq!(sheet.print.paper_size, 1);
    assert!(sheet.print.landscape);
    assert_eq!(sheet.print.scale, 80);
    assert_eq!((sheet.print.fit_to_width, sheet.print.fit_to_height), (2, 4));
    assert!(sheet.print.center_horizontally && sheet.print.center_vertically);
    assert!(sheet.print.print_gridlines && sheet.print.print_headings);
    assert_eq!((sheet.print.margin_left, sheet.print.margin_right), (0.25, 0.5));
    assert_eq!((sheet.print.margin_top, sheet.print.margin_bottom), (0.55, 0.65));
    assert_eq!((sheet.print.margin_header, sheet.print.margin_footer), (0.15, 0.2));
    assert!(sheet.print.different_first_page && sheet.print.different_odd_even);
    assert_eq!(sheet.print.header, "");
    assert_eq!(sheet.print.footer, "&Lfooter left");
    assert_eq!(sheet.print.first_header, "first head");
    assert_eq!(sheet.print.first_footer, "first foot");
    assert_eq!(sheet.print.even_header, "even head");
    assert_eq!(sheet.print.row_breaks, vec![7]);
    assert_eq!(sheet.print.col_breaks, vec![3]);

    // Protection: verifier preserved, locked actions sorted.
    assert!(sheet.protection.enabled);
    assert_eq!(sheet.protection.password_hash, None);
    assert_eq!(sheet.protection.algorithm_name, "SHA-512");
    assert_eq!(sheet.protection.hash_value, "aGFzaA==");
    assert_eq!(sheet.protection.salt_value, "c2FsdA==");
    assert_eq!(sheet.protection.spin_count, 50_000);
    assert_eq!(sheet.protection.options, vec!["selectLockedCells".to_string(), "sort".to_string()]);
    assert_eq!(sheet.sheet_protection, "");

    // Drawing: the two-cell anchored line chart with cache-only series.
    let chart = sheet.charts.first().expect("chart");
    assert_eq!(chart.chart.kind, "line");
    assert!(chart.chart.stacked);
    assert_eq!(chart.chart.title, "Foreign chart");
    assert_eq!(chart.anchor, "E2");
    assert!((chart.width_px - 256.0).abs() < 0.01, "two-cell width drifted: {}", chart.width_px);
    assert!((chart.height_px - 220.0).abs() < 0.01, "two-cell height drifted: {}", chart.height_px);
    assert_eq!(chart.chart.series[0].name, "Revenue");
    assert_eq!(chart.chart.series[0].range, "B2:B4");
    assert_eq!(chart.chart.categories, "A2:A4");
    assert_eq!(chart.chart.categories_cache, vec!["Q1".to_string(), "Q2".to_string(), "Q3".to_string()]);
    assert_eq!(chart.chart.series_values_cache, vec![vec![10.0, 20.0, 30.0]]);
    assert_eq!(chart.chart.x_title, "Quarter");
    assert_eq!(chart.chart.y_title, "Units");
    assert!(chart.chart.legend);

    // Picture: one-cell anchored, rotated, with alt text.
    let image = sheet.images.first().expect("picture");
    assert_eq!(image.anchor.address, "B15");
    assert_eq!(image.anchor.col_off_emu, 12_700);
    assert_eq!(image.anchor.row_off_emu, 6_350);
    assert!((image.width_px - 100.0).abs() < 0.01);
    assert!((image.height_px - 50.0).abs() < 0.01);
    assert!((image.rotation_deg - 45.0).abs() < 0.01);
    assert_eq!(image.image.alt, "company logo");
    assert_eq!(image.image.bytes(), simple_picture().image.bytes());

    // Pivot cache parts preserved with their source and field names.
    assert_eq!(read.workbook.preserved_pivots.len(), 1);
    let pivot = &read.workbook.preserved_pivots[0];
    assert_eq!(pivot.name, "PT_Foreign");
    assert_eq!(pivot.cache_id, 3);
    assert_eq!(pivot.source, "Data!A1:B4");
    assert_eq!(pivot.fields, vec!["Quarter".to_string(), "Revenue".to_string()]);
    assert_eq!(pivot.definition_xml, FOREIGN_PIVOT_DEFINITION);
    assert_eq!(pivot.table_xml, FOREIGN_PIVOT_TABLE);
    assert_eq!(
        pivot.records_base64.as_deref(),
        Some(base64::Engine::encode(&base64::engine::general_purpose::STANDARD, FOREIGN_PIVOT_RECORDS).as_str())
    );
}
