//! Spilled array formulas in XLSX imports (4.2).
//!
//! Excel saves a dynamic array that spilled, and a legacy multi-cell array
//! formula, as `<f t="array" ref="B2:B6">SEQUENCE(5)</f>` in the first cell
//! of the range and plain cached values in the other cells. The editor
//! calculates spills itself, and a spill range that already holds values is
//! blocked (`#SPILL!`). So the cached cells of an array range are dropped on
//! import, unless one carries a formula of its own; the anchor formula spills
//! into them again when the sheet is calculated.
use crate::error::OfficeResult;
use crate::model::Sheet;
use crate::xlsx::shared_formulas::{coordinates, sheet_part};
use crate::xml::parse_xml;
use crate::zip::ZipReader;

/// The array ranges of one sheet's XML: (anchor address, every other cell in `ref`).
fn array_ranges(sheet_xml: &str) -> Vec<(String, Vec<String>)> {
    let Ok(root) = parse_xml(sheet_xml) else {
        return Vec::new();
    };
    let mut cells = Vec::new();
    root.find_all("c", &mut cells);
    let mut out = Vec::new();
    for cell in cells {
        let (Some(anchor), Some(formula)) = (cell.attr("r"), cell.child("f")) else {
            continue;
        };
        if formula.attr("t") != Some("array") {
            continue;
        }
        let Some((start, end)) = formula.attr("ref").and_then(|range| range.split_once(':')) else {
            continue;
        };
        let (Some((top, left)), Some((bottom, right))) = (coordinates(start), coordinates(end)) else {
            continue;
        };
        // A malformed ref must not make the import walk a huge range.
        if bottom < top || right < left || (bottom - top + 1) * (right - left + 1) > 1_000_000 {
            continue;
        }
        let mut others = Vec::new();
        for row in top..=bottom {
            for column in left..=right {
                let address = crate::address::format(row as u32, column as u32);
                if address != anchor {
                    others.push(address);
                }
            }
        }
        out.push((anchor.to_string(), others));
    }
    out
}

/// Addresses in the sheet XML that carry a formula with text of their own.
fn own_formulas(sheet_xml: &str) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    let Ok(root) = parse_xml(sheet_xml) else {
        return out;
    };
    let mut cells = Vec::new();
    root.find_all("c", &mut cells);
    for cell in cells {
        let has_text = cell.child("f").is_some_and(|formula| !formula.direct_text().trim().is_empty());
        if let (true, Some(address)) = (has_text, cell.attr("r")) {
            out.insert(address.to_string());
        }
    }
    out
}

/// Drops the cached cells of one sheet's array ranges from `sheet`. Returns how
/// many cells were dropped.
pub fn apply_xml(sheet_xml: &str, sheet: &mut Sheet) -> usize {
    let ranges = array_ranges(sheet_xml);
    if ranges.is_empty() {
        return 0;
    }
    let keep = own_formulas(sheet_xml);
    let mut dropped = 0usize;
    for (_, others) in ranges {
        for address in others {
            if keep.contains(&address) {
                continue;
            }
            if sheet.cells.remove(&address).is_some() {
                dropped += 1;
            }
        }
    }
    dropped
}

/// Drops the cached array cells of every sheet; the sheets must be in workbook
/// order, as for `shared_formulas::apply_all`.
pub fn apply_all(workbook: &[u8], sheets: &mut [Sheet]) -> OfficeResult<usize> {
    let reader = ZipReader::open(workbook.to_vec())?;
    let mut dropped = 0usize;
    for (index, sheet) in sheets.iter_mut().enumerate() {
        let Some(path) = sheet_part(&reader, index) else { continue };
        let Ok(text) = reader.read_text(&path) else { continue };
        dropped += apply_xml(&text, sheet);
    }
    Ok(dropped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Cell, CellValue};

    fn sheet_with(addresses: &[&str]) -> Sheet {
        let mut sheet = Sheet::new("Sheet1");
        for address in addresses {
            sheet.set(address, Cell { value: CellValue::Number(1.0), ..Default::default() });
        }
        sheet
    }

    const SPILLED: &str = r#"<worksheet><sheetData>
        <row r="2"><c r="B2" cm="1"><f t="array" ref="B2:B4">_xlfn.SEQUENCE(3)</f><v>1</v></c></row>
        <row r="3"><c r="B3"><v>2</v></c></row>
        <row r="4"><c r="B4"><v>3</v></c><c r="C4"><v>9</v></c></row>
    </sheetData></worksheet>"#;

    #[test]
    fn drops_the_cached_cells_of_a_spilled_array() {
        let mut sheet = sheet_with(&["B2", "B3", "B4", "C4"]);
        assert_eq!(apply_xml(SPILLED, &mut sheet), 2);
        // The anchor keeps its formula cell; the cell beside the range stays.
        assert!(sheet.get("B2").is_some());
        assert!(sheet.get("B3").is_none() && sheet.get("B4").is_none());
        assert!(sheet.get("C4").is_some());
    }

    #[test]
    fn keeps_a_cell_in_the_range_that_has_its_own_formula() {
        let xml = r#"<worksheet><sheetData><row r="1">
            <c r="A1"><f t="array" ref="A1:B1">{1,2}</f><v>1</v></c>
            <c r="B1"><f>A1+1</f><v>2</v></c>
        </row></sheetData></worksheet>"#;
        let mut sheet = sheet_with(&["A1", "B1"]);
        assert_eq!(apply_xml(xml, &mut sheet), 0);
        assert!(sheet.get("B1").is_some());
    }

    #[test]
    fn ignores_single_cell_arrays_and_plain_formulas() {
        let xml = r#"<worksheet><sheetData><row r="1">
            <c r="A1"><f t="array" ref="A1">SUM(B1:B3*C1:C3)</f><v>6</v></c>
            <c r="B1"><f>1+1</f><v>2</v></c>
        </row></sheetData></worksheet>"#;
        let mut sheet = sheet_with(&["A1", "B1"]);
        assert_eq!(apply_xml(xml, &mut sheet), 0);
    }

    /// A minimal workbook as Excel writes a spilled `SEQUENCE(3)` in B2.
    fn spilled_workbook() -> Vec<u8> {
        let mut zip = crate::zip::ZipWriter::new();
        zip.add_text(
            "[Content_Types].xml",
            r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#,
        );
        zip.add_text(
            "_rels/.rels",
            r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#,
        );
        zip.add_text(
            "xl/workbook.xml",
            r#"<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
        );
        zip.add_text(
            "xl/_rels/workbook.xml.rels",
            r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#,
        );
        zip.add_text(
            "xl/worksheets/sheet1.xml",
            &format!(
                r#"<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">{}</worksheet>"#,
                SPILLED.trim_start_matches("<worksheet>").trim_end_matches("</worksheet>")
            ),
        );
        zip.finish()
    }

    #[test]
    fn an_excel_spill_imports_as_one_formula_the_editor_can_spill_again() {
        let read = crate::xlsx::read_workbook_bytes(&spilled_workbook()).expect("the workbook must open");
        let sheet = &read.workbook.sheets[0];
        let anchor = sheet.get("B2").expect("the anchor cell stays");
        assert_eq!(anchor.formula.as_deref().map(|text| text.trim_start_matches('=')), Some("SEQUENCE(3)"));
        assert!(sheet.get("B3").is_none() && sheet.get("B4").is_none());
        assert!(sheet.get("C4").is_some());
    }
}
