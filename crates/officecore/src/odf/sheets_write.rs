//! ODS (Calc) export.

use super::*;
use crate::xml::XmlWriter;
use std::path::Path;

pub fn write_ods(workbook: &Workbook) -> OfficeResult<Vec<u8>> {
    let mut styles = AutoStyles::default();
    let mut body = String::new();
    let mut chart_objects: Vec<OdsChartObject> = Vec::new();
    // Highlight looks of conditional-formatting rules, shared by every sheet.
    let mut look_styles: Vec<(String, String)> = Vec::new();
    for sheet in &workbook.sheets {
        body.push_str(&format!("<table:table table:name=\"{}\">", escape(&sheet.name)));
        let cells = ods_cells_with_pivots(workbook, sheet);
        let frames = ods_chart_frames(sheet, &cells, &mut chart_objects);
        let mut max_col = 0u32;
        for address in cells.keys() {
            if let Some((_, column)) = crate::address::parse(address) {
                max_col = max_col.max(column);
            }
        }
        for (_, column) in frames.keys() {
            max_col = max_col.max(*column);
        }
        // The editor hides a column or a row by storing size 0; ODF says so with
        // `table:visibility="collapse"`. A hidden column past the last cell is
        // still written. (`filter` is for rows an AutoFilter hid and needs the
        // filter's database range, which this writer does not export.)
        let hidden_columns = ods_hidden(&sheet.col_widths);
        let hidden_rows = ods_hidden(&sheet.row_heights);
        let last_column = hidden_columns.last().map_or(max_col, |last| max_col.max(*last)).min(200);
        for column in 0..=last_column {
            let width = sheet.col_widths.get(&column).copied().unwrap_or(90.0);
            let collapse = if hidden_columns.contains(&column) { " table:visibility=\"collapse\"" } else { "" };
            body.push_str(&format!(
                "<table:table-column table:style-name=\"co{}\"{collapse} style:column-width=\"{}\"/>",
                column,
                cm(width * 0.75)
            ));
        }
        // LibreOffice writes the sheet's conditional formats between the
        // columns and the rows.
        body.push_str(&ods_conditional_formats(sheet, &mut look_styles));
        // Only rows holding a cell or a chart are written one by one; the gaps
        // between them become repeated empty rows, so a far-away anchor costs
        // one element instead of a row per index.
        let mut occupied: BTreeSet<u32> =
            cells.keys().filter_map(|address| crate::address::parse(address).map(|(row, _)| row)).collect();
        occupied.extend(frames.keys().map(|(row, _)| *row));
        occupied.extend(hidden_rows.iter().copied());
        occupied.insert(0);
        let mut next_row = 0u32;
        for row in occupied {
            if row > next_row {
                body.push_str(&format!(
                    "<table:table-row table:number-rows-repeated=\"{}\"><table:table-cell table:number-columns-repeated=\"{}\"/></table:table-row>",
                    row - next_row,
                    max_col + 1
                ));
            }
            next_row = row + 1;
            let mut row_output = String::new();
            let mut column = 0u32;
            let mut empty_run = 0u32;
            while column <= max_col {
                let address = crate::address::format(row, column);
                let cell = cells.get(&address).filter(|cell| !cell.is_empty());
                let frame = frames.get(&(row, column)).map(String::as_str);
                if cell.is_none() && frame.is_none() {
                    empty_run += 1;
                    column += 1;
                    continue;
                }
                if empty_run > 0 {
                    row_output
                        .push_str(&format!("<table:table-cell table:number-columns-repeated=\"{}\"/>", empty_run));
                    empty_run = 0;
                }
                // Cell-anchored charts sit inside their anchor cell, before its text.
                let frame = frame.unwrap_or("");
                let Some(cell) = cell else {
                    row_output.push_str(&format!("<table:table-cell>{frame}</table:table-cell>"));
                    column += 1;
                    continue;
                };
                let style_name = styles.cell(cell_style_xml(&cell.style));
                let mut attributes = format!(" table:style-name=\"{style_name}\"");
                if let Some(merge) = sheet.merges.iter().find(|merge| merge.start == address) {
                    if let (Some((start_row, start_col)), Some((_, end_col))) =
                        (crate::address::parse(&merge.start), crate::address::parse(&merge.end))
                    {
                        let _ = start_row;
                        attributes.push_str(&format!(" table:number-columns-spanned=\"{}\"", end_col - start_col + 1));
                    }
                }
                let label = match &cell.value {
                    CellValue::Empty => String::new(),
                    CellValue::Number(number) => format!("{number}"),
                    CellValue::Bool(value) => if *value { "TRUE" } else { "FALSE" }.to_string(),
                    CellValue::Text(text) | CellValue::Error(text) => text.clone(),
                };
                // A hyperlink is a `text:a` around the cell text. A cell with no
                // text shows the link's display text (or its target) instead, since
                // ODF has no link without a label. Targets outside the allowed
                // schemes are not written.
                let link_target = cell.link.as_deref().and_then(safe_link_target);
                let paragraph = match &link_target {
                    Some(target) => {
                        let shown = if label.is_empty() {
                            cell.link_display.clone().filter(|text| !text.is_empty()).unwrap_or_else(|| target.clone())
                        } else {
                            label
                        };
                        let tip = cell
                            .link_tooltip
                            .as_deref()
                            .filter(|text| !text.is_empty())
                            .map(|text| format!(" office:title=\"{}\"", escape(text)))
                            .unwrap_or_default();
                        format!(
                            "<text:p><text:a xlink:type=\"simple\" xlink:href=\"{}\"{tip}>{}</text:a></text:p>",
                            escape(&ods_href(target)),
                            crate::xml::escape_text(&shown)
                        )
                    }
                    None if label.is_empty() => "<text:p/>".to_string(),
                    None => format!("<text:p>{}</text:p>", crate::xml::escape_text(&label)),
                };
                let annotation = cell
                    .comment
                    .as_deref()
                    .filter(|text| !text.trim().is_empty())
                    .map(|text| ods_annotation(text, cell.comment_author.as_deref(), cell.comment_visible))
                    .unwrap_or_default();
                let value_xml = format!("{annotation}{paragraph}");
                let value_type = match &cell.value {
                    CellValue::Number(_) => "float",
                    CellValue::Bool(_) => "boolean",
                    _ => "string",
                };
                // LibreOffice marks an error cell as a string with its own flag, which is
                // what tells an error from text that merely looks like one.
                let error_flag =
                    if matches!(cell.value, CellValue::Error(_)) { " calcext:value-type=\"error\"" } else { "" };
                let formula = cell
                    .formula
                    .as_deref()
                    .map(|formula| format!(" table:formula=\"{}\"", escape(&our_formula_to_odf(formula))))
                    .unwrap_or_default();
                let value_attr = match &cell.value {
                    CellValue::Number(number) => format!(" office:value=\"{number}\""),
                    CellValue::Bool(value) => {
                        format!(" office:boolean-value=\"{}\"", if *value { "true" } else { "false" })
                    }
                    // LibreOffice takes a cell's text from `office:string-value` when
                    // it is there and then never sees the `text:a` link, so a linked
                    // cell carries its text in the paragraph only.
                    CellValue::Text(text) if link_target.is_none() => {
                        format!(" office:string-value=\"{}\"", escape(text))
                    }
                    _ => String::new(),
                };
                row_output.push_str(&format!("<table:table-cell office:value-type=\"{value_type}\"{error_flag}{value_attr}{attributes}{formula}>{frame}{value_xml}</table:table-cell>"));
                column += 1;
            }
            if empty_run > 0 {
                row_output.push_str(&format!("<table:table-cell table:number-columns-repeated=\"{}\"/>", empty_run));
            }
            let collapse = if hidden_rows.contains(&row) { " table:visibility=\"collapse\"" } else { "" };
            body.push_str(&format!("<table:table-row{collapse}>{row_output}</table:table-row>"));
        }
        body.push_str("</table:table>");
    }
    let content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-content {NS} {CALCEXT_NS} {OF_NS} office:version=\"1.2\">{}{}<office:body><office:spreadsheet>{body}</office:spreadsheet></office:body></office:document-content>",
        styles.xml(),
        "<office:styles/>"
    );
    // The looks a conditional format applies are named cell styles.
    let look_styles_xml: String = look_styles
        .iter()
        .map(|(name, xml)| {
            format!("<style:style style:name=\"{name}\" style:family=\"table-cell\">{xml}</style:style>")
        })
        .collect();
    // Every chart object is a sub-document with its own manifest entries.
    let mut files = vec!["content.xml".to_string(), "styles.xml".to_string(), "meta.xml".to_string()];
    for object in &chart_objects {
        files.push(format!("{}/", object.name));
        files.push(format!("{}/content.xml", object.name));
        files.push(format!("{}/styles.xml", object.name));
    }
    let manifest = manifest_for("application/vnd.oasis.opendocument.spreadsheet", &files);
    let mut zip = ZipWriter::new();
    zip.add_text("mimetype", "application/vnd.oasis.opendocument.spreadsheet");
    zip.add_text("META-INF/manifest.xml", &manifest);
    zip.add_text("content.xml", &content);
    zip.add_text(
        "styles.xml",
        &if look_styles_xml.is_empty() {
            format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-styles {NS} office:version=\"1.2\"/>")
        } else {
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-styles {NS} office:version=\"1.2\"><office:styles>{look_styles_xml}</office:styles></office:document-styles>"
            )
        },
    );
    zip.add_text("meta.xml", &meta_xml(&workbook.title, "OmniOffice", None));
    for object in &chart_objects {
        zip.add_text(&format!("{}/content.xml", object.name), &object.content);
        zip.add_text(
            &format!("{}/styles.xml", object.name),
            &format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-styles {NS} {CHART_NS} office:version=\"1.2\"><office:styles/></office:document-styles>"
            ),
        );
    }
    Ok(zip.finish())
}

pub fn write_ods_file(path: &Path, workbook: &Workbook) -> OfficeResult<()> {
    crate::io::write_atomic(path, &write_ods(workbook)?)
}

/// A chart's local table: the series names from the header row and, per body
/// row, the category label followed by one value per series.
pub(super) struct OdsLocalTable {
    pub(super) header: Vec<String>,
    pub(super) rows: Vec<Vec<(String, Option<f64>)>>,
}
