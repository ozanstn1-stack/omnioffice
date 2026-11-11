//! Dynamic arrays in XLSX exports (4.2b).
//!
//! The editor spills an array formula into the cells below and to the right.
//! Excel only does the same for a cell it knows is a dynamic array: the cell
//! carries `cm="1"` (cell metadata 1), its formula is written as
//! `<f t="array" ref="B2:B4">`, and the package has an `xl/metadata.xml` part
//! whose metadata type `XLDAPR` marks it as dynamic. Without them Excel reads
//! the formula as a legacy one and shows a single value. The editor sets
//! [`Cell::spill`] on each spilling formula when it saves.
use crate::model::{Cell, Workbook};
use crate::xml::escape_attr;
use crate::zip::ZipWriter;

/// Excel's own `xl/metadata.xml` for dynamic arrays: one `XLDAPR` entry that
/// every spilling cell points at with `cm="1"`.
const METADATA_XML: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n",
    "<metadata xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" ",
    "xmlns:xda=\"http://schemas.microsoft.com/office/spreadsheetml/2017/dynamicarray\">",
    "<metadataTypes count=\"1\"><metadataType name=\"XLDAPR\" minSupportedVersion=\"120000\" copy=\"1\" ",
    "pasteAll=\"1\" pasteValues=\"1\" merge=\"1\" splitFirst=\"1\" rowColShift=\"1\" clearFormats=\"1\" ",
    "clearComments=\"1\" assign=\"1\" coerce=\"1\" cellMeta=\"1\"/></metadataTypes>",
    "<futureMetadata name=\"XLDAPR\" count=\"1\"><bk><extLst><ext uri=\"{bdbb8cdc-fa1e-496e-a857-3c3f30c029c3}\">",
    "<xda:dynamicArrayProperties fDynamic=\"1\" fCollapsed=\"0\"/></ext></extLst></bk></futureMetadata>",
    "<cellMetadata count=\"1\"><bk><rc t=\"1\" v=\"0\"/></bk></cellMetadata></metadata>",
);

/// The spill range of a formula cell when it is a valid `A1:B2` range.
fn spill_range(cell: &Cell) -> Option<&str> {
    cell.formula.as_ref()?;
    let range = cell.spill.as_deref()?.trim();
    let (start, end) = range.split_once(':')?;
    let valid = |address: &str| crate::address::parse(address).is_some();
    (valid(start) && valid(end)).then_some(range)
}

/// A written cell, marked as a dynamic array when the editor spilled it.
pub(super) fn dynamic_array(xml: String, cell: &Cell) -> String {
    let Some(range) = spill_range(cell) else { return xml };
    xml.replacen("<f>", &format!("<f t=\"array\" ref=\"{}\">", escape_attr(range)), 1).replacen(
        "<c ",
        "<c cm=\"1\" ",
        1,
    )
}

/// Whether any cell of the workbook is written as a dynamic array.
fn has_dynamic_arrays(workbook: &Workbook) -> bool {
    workbook.sheets.iter().any(|sheet| sheet.cells.values().any(|cell| spill_range(cell).is_some()))
}

/// Closes the content types list, adding the metadata part (its content type,
/// its workbook relationship and the part itself) when a cell needs it.
pub(super) fn close_package(
    workbook: &Workbook,
    content_types: &mut String,
    workbook_rels: &mut String,
    zip: &mut ZipWriter,
) {
    if has_dynamic_arrays(workbook) {
        content_types.push_str(
            "<Override PartName=\"/xl/metadata.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheetMetadata+xml\"/>",
        );
        let relationship = "<Relationship Id=\"rIdMetadata\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/sheetMetadata\" Target=\"metadata.xml\"/>";
        *workbook_rels = workbook_rels.replacen("</Relationships>", &format!("{relationship}</Relationships>"), 1);
        zip.add_text("xl/metadata.xml", METADATA_XML);
    }
    content_types.push_str("</Types>");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CellValue, Sheet};
    use crate::zip::ZipReader;

    fn spilling(range: &str) -> Cell {
        Cell {
            formula: Some("SEQUENCE(3)".into()),
            value: CellValue::Number(1.0),
            spill: Some(range.into()),
            ..Default::default()
        }
    }

    #[test]
    fn marks_a_spilling_formula_and_leaves_the_rest_alone() {
        let xml = "<c r=\"B2\"><f>_xlfn.SEQUENCE(3)</f><v>1</v></c>".to_string();
        assert_eq!(
            dynamic_array(xml.clone(), &spilling("B2:B4")),
            "<c cm=\"1\" r=\"B2\"><f t=\"array\" ref=\"B2:B4\">_xlfn.SEQUENCE(3)</f><v>1</v></c>"
        );
        // No spill, a broken range or no formula: written as before.
        assert_eq!(dynamic_array(xml.clone(), &Cell { spill: None, ..spilling("B2:B4") }), xml);
        assert_eq!(dynamic_array(xml.clone(), &spilling("B2:nope")), xml);
        assert_eq!(dynamic_array(xml.clone(), &Cell { formula: None, ..spilling("B2:B4") }), xml);
    }

    #[test]
    fn a_saved_spill_carries_excels_metadata_and_reads_back_as_one_formula() {
        let mut workbook = Workbook::new_blank("Spill");
        let mut sheet = Sheet::new("Sheet1");
        sheet.set("B2", spilling("B2:B4"));
        sheet.set("D1", Cell { value: CellValue::Number(5.0), ..Default::default() });
        workbook.sheets = vec![sheet];
        let bytes = crate::xlsx::write_xlsx_package(&workbook).expect("the workbook must write").bytes;

        let reader = ZipReader::open(bytes.clone()).unwrap();
        assert!(reader.read_text("xl/metadata.xml").unwrap().contains("XLDAPR"));
        assert!(reader.read_text("[Content_Types].xml").unwrap().contains("/xl/metadata.xml"));
        assert!(reader.read_text("xl/_rels/workbook.xml.rels").unwrap().contains("sheetMetadata"));
        let sheet_xml = reader.read_text("xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet_xml.contains("<c cm=\"1\" r=\"B2\""), "{sheet_xml}");
        assert!(sheet_xml.contains("<f t=\"array\" ref=\"B2:B4\">_xlfn.SEQUENCE(3)</f>"), "{sheet_xml}");

        let read = crate::xlsx::read_workbook_bytes(&bytes).expect("the workbook must read back");
        let anchor = read.workbook.sheets[0].get("B2").expect("the anchor cell stays");
        assert_eq!(anchor.formula.as_deref().map(|text| text.trim_start_matches('=')), Some("SEQUENCE(3)"));
    }

    #[test]
    fn a_workbook_without_spills_gets_no_metadata_part() {
        let mut workbook = Workbook::new_blank("Plain");
        let mut sheet = Sheet::new("Sheet1");
        sheet.set("A1", Cell { formula: Some("1+1".into()), ..Default::default() });
        workbook.sheets = vec![sheet];
        let bytes = crate::xlsx::write_xlsx_package(&workbook).unwrap().bytes;
        let reader = ZipReader::open(bytes).unwrap();
        assert!(reader.read_text("xl/metadata.xml").is_err());
        assert!(!reader.read_text("[Content_Types].xml").unwrap().contains("metadata.xml"));
    }
}
