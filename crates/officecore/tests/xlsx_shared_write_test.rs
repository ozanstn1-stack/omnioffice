//! XLSX shared-formula export grouping and round-trip tests.

use officecore::model::{Cell, CellValue, Workbook};
use officecore::xlsx;
use officecore::zip::ZipReader;

fn sheet_xml_from(workbook: &Workbook) -> (Vec<u8>, String) {
    let bytes = xlsx::write_xlsx_package(workbook).expect("write xlsx package").bytes;
    let xml =
        ZipReader::open(bytes.clone()).expect("open zip").read_text("xl/worksheets/sheet1.xml").expect("sheet1.xml");
    (bytes, xml)
}

#[test]
fn vertical_shared_formula_group_exports_and_round_trips() {
    let mut workbook = Workbook::new_blank("SharedVertical");
    let sheet = &mut workbook.sheets[0];
    sheet.name = "Calc".into();
    for row in 2..=5u32 {
        let val = (row - 1) as f64 * 10.0;
        sheet.set(&format!("A{row}"), Cell { value: CellValue::Number(val), ..Default::default() });
        sheet.set(
            &format!("B{row}"),
            Cell { value: CellValue::Number(val * 2.0), formula: Some(format!("=A{row}*2")), ..Default::default() },
        );
    }
    for (row, label) in [(2u32, "low"), (3, "high"), (4, "high")] {
        sheet.set(
            &format!("C{row}"),
            Cell {
                value: CellValue::Text(label.into()),
                formula: Some(format!("=IF(A{row}>15,\"high\",\"low\")")),
                ..Default::default()
            },
        );
    }
    for row in 2..=3u32 {
        sheet.set(&format!("D{row}"), Cell { formula: Some(format!("=A{row}+1")), ..Default::default() });
    }

    let (bytes, xml) = sheet_xml_from(&workbook);
    assert!(xml.contains("<f t=\"shared\" ref=\"B2:B5\" si=\"0\">A2*2</f>"), "{xml}");
    assert_eq!(xml.matches("<f t=\"shared\" si=\"0\"/>").count(), 3, "{xml}");
    assert!(xml.contains("<f t=\"shared\" ref=\"C2:C4\" si=\"1\">IF(A2&gt;15,\"high\",\"low\")</f>"), "{xml}");
    assert_eq!(xml.matches("<f t=\"shared\" si=\"1\"/>").count(), 2, "{xml}");
    assert!(xml.contains("<c r=\"C3\"") && xml.contains("t=\"str\"><f t=\"shared\" si=\"1\"/><v>high</v>"), "{xml}");
    assert!(xml.contains("<f t=\"shared\" ref=\"D2:D3\" si=\"2\">A2+1</f>"), "{xml}");
    assert!(xml.contains("<c r=\"D3\"") && xml.contains("><f t=\"shared\" si=\"2\"/></c>"), "{xml}");

    let reopened = xlsx::read_workbook_bytes(&bytes).expect("reopen xlsx");
    let data = &reopened.workbook.sheets[0];
    for row in 2..=5u32 {
        assert_eq!(
            data.get(&format!("B{row}")).and_then(|c| c.formula.as_deref()),
            Some(format!("=A{row}*2").as_str())
        );
    }
    for row in 2..=4u32 {
        assert_eq!(
            data.get(&format!("C{row}")).and_then(|c| c.formula.as_deref()),
            Some(format!("=IF(A{row}>15,\"high\",\"low\")").as_str())
        );
    }
    assert_eq!(data.get("D3").and_then(|c| c.formula.as_deref()), Some("=A3+1"));
}

#[test]
fn horizontal_and_mixed_absolute_references_group_correctly() {
    let mut workbook = Workbook::new_blank("SharedHorizontal");
    let sheet = &mut workbook.sheets[0];
    for (col_idx, col_letter) in ["B", "C", "D", "E"].iter().enumerate() {
        sheet.set(
            &format!("{col_letter}2"),
            Cell {
                value: CellValue::Number((col_idx + 1) as f64),
                formula: Some(format!("=SUM({col_letter}$1:$B1)")),
                ..Default::default()
            },
        );
    }
    sheet
        .set("F2", Cell { value: CellValue::Number(0.0), formula: Some("=AVERAGE(F$1)".into()), ..Default::default() });

    let (bytes, xml) = sheet_xml_from(&workbook);
    assert!(xml.contains("<f t=\"shared\" ref=\"B2:E2\" si=\"0\">SUM(B$1:$B1)</f>"), "{xml}");
    assert_eq!(xml.matches("<f t=\"shared\" si=\"0\"/>").count(), 3, "{xml}");
    assert!(xml.contains("<f>AVERAGE(F$1)</f>"), "{xml}");

    let reopened = xlsx::read_workbook_bytes(&bytes).expect("reopen horizontal shared");
    let data = &reopened.workbook.sheets[0];
    for col_letter in ["B", "C", "D", "E"] {
        assert_eq!(
            data.get(&format!("{col_letter}2")).and_then(|c| c.formula.as_deref()),
            Some(format!("=SUM({col_letter}$1:$B1)").as_str())
        );
    }
    assert_eq!(data.get("F2").and_then(|c| c.formula.as_deref()), Some("=AVERAGE(F$1)"));
}

#[test]
fn future_functions_coalesce_and_dynamic_array_spills_stay_isolated() {
    let mut workbook = Workbook::new_blank("SharedFutureAndSpill");
    let sheet = &mut workbook.sheets[0];
    for row in 2..=4u32 {
        let end = row + 3;
        sheet.set(
            &format!("B{row}"),
            Cell {
                value: CellValue::Number(row as f64),
                formula: Some(format!("=XLOOKUP(A{row},D{row}:D{end},E{row}:E{end})")),
                ..Default::default()
            },
        );
    }
    sheet.set(
        "F2",
        Cell {
            value: CellValue::Number(1.0),
            formula: Some("=SEQUENCE(2)".into()),
            spill: Some("F2:F3".into()),
            ..Default::default()
        },
    );
    sheet.set(
        "F3",
        Cell {
            value: CellValue::Number(2.0),
            formula: Some("=SEQUENCE(2)".into()),
            spill: Some("F3:F4".into()),
            ..Default::default()
        },
    );

    let (bytes, xml) = sheet_xml_from(&workbook);
    assert!(xml.contains("<f t=\"shared\" ref=\"B2:B4\" si=\"0\">_xlfn.XLOOKUP(A2,D2:D5,E2:E5)</f>"), "{xml}");
    assert_eq!(xml.matches("<f t=\"shared\" si=\"0\"/>").count(), 2, "{xml}");
    assert!(
        xml.contains("<c cm=\"1\" r=\"F2\"") && xml.contains("<f t=\"array\" ref=\"F2:F3\">_xlfn.SEQUENCE(2)</f>"),
        "{xml}"
    );
    assert!(
        xml.contains("<c cm=\"1\" r=\"F3\"") && xml.contains("<f t=\"array\" ref=\"F3:F4\">_xlfn.SEQUENCE(2)</f>"),
        "{xml}"
    );

    let reopened = xlsx::read_workbook_bytes(&bytes).expect("reopen future+spill");
    let data = &reopened.workbook.sheets[0];
    assert_eq!(data.get("B3").and_then(|c| c.formula.as_deref()), Some("=XLOOKUP(A3,D3:D6,E3:E6)"));
    assert_eq!(data.get("B4").and_then(|c| c.formula.as_deref()), Some("=XLOOKUP(A4,D4:D7,E4:E7)"));
}
