//! XLSX export (written directly as OOXML) and spreadsheet import.
//!
//! Export covers values, formulas, styling, number formats, column widths, row
//! heights, merges, freeze panes, gridline settings, defined names, autofilters,
//! tab colours, hyperlinks, cell comments, data validation, conditional
//! formatting, structured tables, sheet protection, print layout (margins,
//! headers/footers, page breaks, print area/titles), charts (column, bar, line,
//! pie, area) as real ChartML parts anchored to their cells and sheet pictures
//! in `xl/media` with drawing anchors. Imported pivot caches/tables are
//! re-exported from the raw parts they came in as.
//!
//! Import is two passes: the well-tested `calamine` parser reads values and
//! formulas from XLSX, XLS and ODS files from Excel and LibreOffice, then a
//! best-effort OOXML pass (only for XLSX/XLSM) reads styles, column widths, row
//! heights, merges, freeze panes, validations, conditional formatting, defined
//! names, hyperlinks, comments, structured tables, print settings, sheet
//! protection, drawings (charts and pictures) and pivot parts straight from the
//! package through the hardened ZIP/XML readers. The second pass never fails the
//! import: a part that cannot be parsed adds a warning and the values from the
//! first pass are still returned.

use crate::error::{OfficeError, OfficeResult};
use crate::io::{normalize_hex, write_atomic};
use crate::model::*;
use crate::xml::{escape_attr, escape_text, parse_xml, XmlNode, XmlWriter};
use crate::zip::ZipWriter;
use base64::Engine as _;
use calamine::Reader as CalamineReader;
use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct SheetWrite {
    pub bytes: Vec<u8>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SheetRead {
    pub workbook: Workbook,
    pub warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// Styles
// ---------------------------------------------------------------------------

#[derive(Default)]
struct StyleTable {
    fonts: Vec<String>,
    fills: Vec<String>,
    borders: Vec<String>,
    num_formats: Vec<(u32, String)>,
    xfs: Vec<String>,
    font_keys: BTreeMap<String, usize>,
    fill_keys: BTreeMap<String, usize>,
    border_keys: BTreeMap<String, usize>,
    xf_keys: BTreeMap<String, usize>,
}

impl StyleTable {
    fn new() -> Self {
        let mut table = Self::default();
        // Index 0: defaults required by the spec.
        table.fonts.push("<font><sz val=\"11\"/><name val=\"Calibri\"/></font>".into());
        table.fills.push("<fill><patternFill patternType=\"none\"/></fill>".into());
        table.fills.push("<fill><patternFill patternType=\"gray125\"/></fill>".into());
        table.borders.push("<border><left/><right/><top/><bottom/><diagonal/></border>".into());
        table.xfs.push("<xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/>".into());
        table
    }

    fn builtin_number_format(format: &str) -> Option<u32> {
        match format.trim() {
            "0" => Some(1),
            "0.00" => Some(2),
            "#,##0" => Some(3),
            "#,##0.00" => Some(4),
            "0%" => Some(9),
            "0.00%" => Some(10),
            "0.00E+00" => Some(11),
            "# ?/?" => Some(12),
            "@" => Some(49),
            _ => None,
        }
    }

    fn font_id(&mut self, style: &CellStyle) -> usize {
        let size = style.size_pt.unwrap_or(11.0);
        let color = style.color.as_deref().and_then(normalize_hex);
        let mut xml = String::from("<font>");
        if style.bold {
            xml.push_str("<b/>");
        }
        if style.italic {
            xml.push_str("<i/>");
        }
        if style.underline {
            xml.push_str("<u/>");
        }
        if style.strike {
            xml.push_str("<strike/>");
        }
        xml.push_str(&format!("<sz val=\"{size}\"/>"));
        if let Some(color) = &color {
            xml.push_str(&format!("<color rgb=\"FF{}\"/>", color.trim_start_matches('#')));
        }
        xml.push_str(&format!("<name val=\"{}\"/></font>", escape_attr(style.font.as_deref().unwrap_or("Calibri"))));
        if let Some(index) = self.font_keys.get(&xml) {
            return *index;
        }
        self.fonts.push(xml.clone());
        let index = self.fonts.len() - 1;
        self.font_keys.insert(xml, index);
        index
    }

    fn fill_id(&mut self, style: &CellStyle) -> usize {
        let Some(fill) = style.fill.as_deref().and_then(normalize_hex) else { return 0 };
        let xml = format!(
            "<fill><patternFill patternType=\"solid\"><fgColor rgb=\"FF{}\"/><bgColor indexed=\"64\"/></patternFill></fill>",
            fill.trim_start_matches('#')
        );
        if let Some(index) = self.fill_keys.get(&xml) {
            return *index;
        }
        self.fills.push(xml.clone());
        let index = self.fills.len() - 1;
        self.fill_keys.insert(xml, index);
        index
    }

    fn border_id(&mut self, style: &CellStyle) -> usize {
        let render = |name: &str, border: &Option<BorderStyle>| -> String {
            match border {
                Some(border) if border.style != "none" => {
                    let color = normalize_hex(&border.color).unwrap_or_else(|| "#000000".into());
                    let kind = match border.style.as_str() {
                        "thick" => "thick",
                        "dashed" => "dashed",
                        "dotted" => "dotted",
                        "double" => "double",
                        _ => "thin",
                    };
                    format!("<{name} style=\"{kind}\"><color rgb=\"FF{}\"/></{name}>", color.trim_start_matches('#'))
                }
                _ => format!("<{name}/>"),
            }
        };
        let borders = &style.borders;
        if borders.top.is_none() && borders.right.is_none() && borders.bottom.is_none() && borders.left.is_none() {
            return 0;
        }
        let xml = format!(
            "<border>{}{}{}{}<diagonal/></border>",
            render("left", &borders.left),
            render("right", &borders.right),
            render("top", &borders.top),
            render("bottom", &borders.bottom)
        );
        if let Some(index) = self.border_keys.get(&xml) {
            return *index;
        }
        self.borders.push(xml.clone());
        let index = self.borders.len() - 1;
        self.border_keys.insert(xml, index);
        index
    }

    fn number_format_id(&mut self, format: &str) -> u32 {
        let format = format.trim();
        if format.is_empty() || format.eq_ignore_ascii_case("general") {
            return 0;
        }
        if let Some(builtin) = Self::builtin_number_format(format) {
            return builtin;
        }
        if let Some((id, _)) = self.num_formats.iter().find(|(_, existing)| existing == format) {
            return *id;
        }
        let id = 164 + self.num_formats.len() as u32;
        self.num_formats.push((id, format.to_string()));
        id
    }

    fn xf_id(&mut self, style: &CellStyle) -> usize {
        let font = self.font_id(style);
        let fill = self.fill_id(style);
        let border = self.border_id(style);
        let number_format = self.number_format_id(&style.number_format);
        let mut alignment = String::new();
        if !style.align.is_empty() && style.align != "general" {
            alignment.push_str(&format!(" horizontal=\"{}\"", style.align));
        }
        if !style.valign.is_empty() && style.valign != "bottom" {
            alignment.push_str(&format!(" vertical=\"{}\"", style.valign));
        }
        if style.wrap {
            alignment.push_str(" wrapText=\"1\"");
        }
        if style.rotation != 0 {
            alignment.push_str(&format!(" textRotation=\"{}\"", style.rotation));
        }
        let mut xml = format!("<xf numFmtId=\"{number_format}\" fontId=\"{font}\" fillId=\"{fill}\" borderId=\"{border}\" xfId=\"0\" applyFont=\"1\" applyFill=\"1\" applyBorder=\"1\" applyNumberFormat=\"1\"");
        if alignment.is_empty() {
            xml.push_str("/>");
        } else {
            xml.push_str(&format!(" applyAlignment=\"1\"><alignment{alignment}/></xf>"));
        }
        if let Some(index) = self.xf_keys.get(&xml) {
            return *index;
        }
        self.xfs.push(xml.clone());
        let index = self.xfs.len() - 1;
        self.xf_keys.insert(xml, index);
        index
    }

    fn xml(&self) -> String {
        let mut writer = XmlWriter::new();
        writer.declaration();
        writer.open("styleSheet", &[("xmlns", "http://schemas.openxmlformats.org/spreadsheetml/2006/main")]);
        if !self.num_formats.is_empty() {
            writer.raw(&format!("<numFmts count=\"{}\">", self.num_formats.len()));
            for (id, format) in &self.num_formats {
                writer.raw(&format!("<numFmt numFmtId=\"{id}\" formatCode=\"{}\"/>", escape_attr(format)));
            }
            writer.raw("</numFmts>");
        }
        writer.raw(&format!("<fonts count=\"{}\">", self.fonts.len()));
        for font in &self.fonts {
            writer.raw(font);
        }
        writer.raw("</fonts>");
        writer.raw(&format!("<fills count=\"{}\">", self.fills.len()));
        for fill in &self.fills {
            writer.raw(fill);
        }
        writer.raw("</fills>");
        writer.raw(&format!("<borders count=\"{}\">", self.borders.len()));
        for border in &self.borders {
            writer.raw(border);
        }
        writer.raw("</borders>");
        writer.raw(
            "<cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs>",
        );
        writer.raw(&format!("<cellXfs count=\"{}\">", self.xfs.len()));
        for xf in &self.xfs {
            writer.raw(xf);
        }
        writer.raw("</cellXfs>");
        writer.raw("<cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles>");
        // One differential format backs every conditional-formatting rule, so
        // the highlight colour a rule shows in the editor is the colour a
        // spreadsheet shows after the round trip.
        writer.raw(&format!(
            "<dxfs count=\"1\"><dxf><font><color rgb=\"{}\"/></font><fill><patternFill><bgColor rgb=\"{}\"/></patternFill></fill></dxf></dxfs>",
            argb_of(CONDITIONAL_FILL),
            argb_of(CONDITIONAL_FILL)
        ));
        writer.raw("</styleSheet>");
        writer.finish()
    }
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

fn column_width_units(pixels: f64) -> f64 {
    // Approximate Excel's character-based width.
    ((pixels - 5.0) / 7.0).max(2.0)
}

/// One worksheet plus the companion parts that hang off it.
///
/// XLSX keeps a worksheet's relationships in a sibling `.rels` file, so the
/// hyperlink targets, the comment part and its VML shapes cannot live in the
/// worksheet XML itself.
struct SheetPart {
    xml: String,
    rels: Vec<(String, String, String)>,
    comments: Vec<(String, String)>,
}

/// One worksheet cell as row XML, or `None` when there is nothing to write.
fn cell_xml(
    address: &str,
    value: &CellValue,
    formula: Option<&str>,
    style: &CellStyle,
    has_link: bool,
    has_comment: bool,
    styles: &mut StyleTable,
    shared: &mut Vec<String>,
    shared_index: &mut BTreeMap<String, usize>,
) -> Option<String> {
    let style_id = styles.xf_id(style);
    let style_attr = if style_id > 0 { format!(" s=\"{style_id}\"") } else { String::new() };
    let formula = formula.map(|formula| formula.trim_start_matches('=').to_string());
    let mut value_xml = String::new();
    let mut type_attr = String::new();
    match value {
        CellValue::Number(number) => value_xml = format!("<v>{number}</v>"),
        CellValue::Bool(value) => {
            type_attr = " t=\"b\"".into();
            value_xml = format!("<v>{}</v>", if *value { 1 } else { 0 });
        }
        CellValue::Error(error) => {
            type_attr = " t=\"e\"".into();
            value_xml = format!("<v>{}</v>", escape_text(error));
        }
        CellValue::Text(text) if !text.is_empty() => {
            if formula.is_some() {
                type_attr = " t=\"str\"".into();
                value_xml = format!("<v>{}</v>", escape_text(text));
            } else {
                let index = match shared_index.get(text) {
                    Some(index) => *index,
                    None => {
                        let index = shared.len();
                        shared.push(text.clone());
                        shared_index.insert(text.clone(), index);
                        index
                    }
                };
                type_attr = " t=\"s\"".into();
                value_xml = format!("<v>{index}</v>");
            }
        }
        _ => {}
    }
    let formula_xml = formula.map(|formula| format!("<f>{}</f>", escape_text(&formula))).unwrap_or_default();
    if formula_xml.is_empty() && value_xml.is_empty() && style == &CellStyle::default() && !has_link && !has_comment {
        return None;
    }
    Some(format!("<c r=\"{address}\"{style_attr}{type_attr}>{formula_xml}{value_xml}</c>"))
}

/// The non-empty comment texts of one sheet, in cell order.
///
/// Collected before `sheet_xml` so the caller can assign each commented sheet
/// its own part number and keep the workbook-wide numbering stable.
fn sheet_comments(sheet: &Sheet) -> Vec<(String, String)> {
    let mut comments = Vec::new();
    for (address, cell) in &sheet.cells {
        if cell.is_empty() {
            continue;
        }
        if let (Some(text), Some((row, column))) = (cell.comment.as_ref(), crate::address::parse(address)) {
            if !text.trim().is_empty() {
                comments.push((crate::address::format(row, column), text.clone()));
            }
        }
    }
    comments
}

fn sheet_xml(
    sheet: &Sheet,
    drawing_number: Option<usize>,
    comment_number: Option<usize>,
    comments: Vec<(String, String)>,
    pivot_cells: &[(String, CellValue)],
    table_numbers: &[usize],
    styles: &mut StyleTable,
    shared: &mut Vec<String>,
    shared_index: &mut BTreeMap<String, usize>,
) -> SheetPart {
    let mut writer = XmlWriter::new();
    writer.declaration();
    writer.raw(
        "<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">",
    );
    // CT_Worksheet is a strict sequence: sheetPr, dimension, sheetViews,
    // sheetFormatPr, cols, sheetData, sheetProtection, autoFilter, mergeCells,
    // conditionalFormatting, dataValidations, hyperlinks, printOptions,
    // pageMargins, pageSetup, headerFooter, legacyDrawing. Emitting them out of
    // order produces a file Excel refuses to open, so the order is not cosmetic.
    if let Some(color) = sheet.tab_color.as_ref().filter(|value| !value.is_empty()) {
        writer.raw(&format!("<sheetPr><tabColor rgb=\"{}\"/></sheetPr>", argb_of(color)));
    }

    let mut max_row = 0u32;
    let mut max_col = 0u32;
    for address in sheet.cells.keys() {
        if let Some((row, column)) = crate::address::parse(address) {
            max_row = max_row.max(row);
            max_col = max_col.max(column);
        }
    }
    writer.raw(&format!("<dimension ref=\"A1:{}\"/>", crate::address::format(max_row, max_col)));

    writer.raw("<sheetViews><sheetView workbookViewId=\"0\"");
    if !sheet.show_gridlines {
        writer.raw(" showGridLines=\"0\"");
    }
    writer.raw(">");
    if sheet.freeze_rows > 0 || sheet.freeze_cols > 0 {
        let x_split = sheet.freeze_cols;
        let y_split = sheet.freeze_rows;
        let top_left = crate::address::format(y_split, x_split);
        writer.raw(&format!(
            "<pane xSplit=\"{x_split}\" ySplit=\"{y_split}\" topLeftCell=\"{top_left}\" activePane=\"bottomRight\" state=\"frozen\"/>"
        ));
    }
    writer.raw("</sheetView></sheetViews>");
    writer.raw("<sheetFormatPr defaultRowHeight=\"15\"/>");

    if !sheet.col_widths.is_empty() {
        writer.raw("<cols>");
        let mut widths: Vec<(&u32, &f64)> = sheet.col_widths.iter().collect();
        widths.sort_by_key(|(column, _)| **column);
        for (column, width) in widths {
            writer.raw(&format!(
                "<col min=\"{}\" max=\"{}\" width=\"{:.2}\" customWidth=\"1\"/>",
                column + 1,
                column + 1,
                column_width_units(*width)
            ));
        }
        writer.raw("</cols>");
    }

    writer.raw("<sheetData>");
    let mut rows: BTreeMap<u32, Vec<(u32, &Cell)>> = BTreeMap::new();
    let mut hyperlinks: Vec<(String, String)> = Vec::new();
    for (address, cell) in &sheet.cells {
        if cell.is_empty() {
            continue;
        }
        if let (Some(target), Some((row, column))) = (cell.link.as_ref(), crate::address::parse(address)) {
            hyperlinks.push((crate::address::format(row, column), target.clone()));
        }
        if let Some((row, column)) = crate::address::parse(address) {
            rows.entry(row).or_default().push((column, cell));
        }
    }
    // Pivot output is materialised as plain values at the anchor; merging it
    // here keeps the exporter from needing a second sheet copy.
    let mut pivot_rows: BTreeMap<u32, Vec<(u32, CellValue)>> = BTreeMap::new();
    for (address, value) in pivot_cells {
        if let Some((row, column)) = crate::address::parse(address) {
            pivot_rows.entry(row).or_default().push((column, value.clone()));
        }
    }
    for line in pivot_rows.values_mut() {
        line.sort_by_key(|(column, _)| *column);
    }

    for (row, mut cells) in rows {
        cells.sort_by_key(|(column, _)| *column);
        match sheet.row_heights.get(&row) {
            Some(height) => {
                writer.raw(&format!("<row r=\"{}\" ht=\"{:.2}\" customHeight=\"1\">", row + 1, height * 0.75));
            }
            None => {
                writer.raw(&format!("<row r=\"{}\">", row + 1));
            }
        }
        // Two-pointer merge of the model cells and the pivot cells, both
        // sorted by column.
        enum RowCell<'a> {
            Model(&'a Cell),
            Pivot(&'a CellValue),
        }
        let pivot_line = pivot_rows.get(&row).map(Vec::as_slice).unwrap_or(&[]);
        let mut pivot_index = 0usize;
        let mut merged: Vec<(u32, RowCell)> = Vec::with_capacity(cells.len() + pivot_line.len());
        for (column, cell) in cells {
            while pivot_index < pivot_line.len() && pivot_line[pivot_index].0 < column {
                merged.push((pivot_line[pivot_index].0, RowCell::Pivot(&pivot_line[pivot_index].1)));
                pivot_index += 1;
            }
            merged.push((column, RowCell::Model(cell)));
        }
        while pivot_index < pivot_line.len() {
            merged.push((pivot_line[pivot_index].0, RowCell::Pivot(&pivot_line[pivot_index].1)));
            pivot_index += 1;
        }
        for (column, entry) in merged {
            let address = crate::address::format(row, column);
            let xml = match entry {
                RowCell::Model(cell) => cell_xml(
                    &address,
                    &cell.value,
                    cell.formula.as_deref(),
                    &cell.style,
                    cell.link.is_some(),
                    cell.comment.as_deref().map(|text| !text.trim().is_empty()).unwrap_or(false),
                    styles,
                    shared,
                    shared_index,
                ),
                RowCell::Pivot(value) => {
                    cell_xml(&address, value, None, &CellStyle::default(), false, false, styles, shared, shared_index)
                }
            };
            if let Some(xml) = xml {
                writer.raw(&xml);
            }
        }
        writer.raw("</row>");
    }
    writer.raw("</sheetData>");

    // CT_Worksheet order from here on: sheetProtection, autoFilter,
    // mergeCells, conditionalFormatting, dataValidations, hyperlinks,
    // printOptions, pageMargins, pageSetup, headerFooter, rowBreaks,
    // colBreaks, drawing, legacyDrawing.
    if let Some(xml) = sheet_protection_xml(sheet) {
        writer.raw(&xml);
    }

    // AutoFilter: the active filter range, which is what the toolbar toggles.
    // A range a structured table already filters is skipped here; the table part
    // carries its own autoFilter and writing both would duplicate the filter.
    if let Some(filter) = &sheet.filter {
        if !filter.range.is_empty() && !filter_owned_by_table(sheet, &filter.range) {
            writer.raw(&format!("<autoFilter ref=\"{}\"/>", escape_attr(&filter.range)));
        }
    }

    if !sheet.merges.is_empty() {
        writer.raw(&format!("<mergeCells count=\"{}\">", sheet.merges.len()));
        for merge in &sheet.merges {
            writer.raw(&format!("<mergeCell ref=\"{}:{}\"/>", escape_attr(&merge.start), escape_attr(&merge.end)));
        }
        writer.raw("</mergeCells>");
    }

    // Conditional formatting; each rule points at the shared dxf in styles.xml.
    for (index, rule) in sheet.conditional.iter().enumerate() {
        if let Some(xml) = conditional_formatting_xml(rule, index) {
            writer.raw(&xml);
        }
    }

    // Data validation: list and numeric range, with the optional error message.
    if !sheet.validations.is_empty() {
        let mut count = 0;
        let mut body = String::new();
        for validation in &sheet.validations {
            let (kind, formula1, formula2) = match validation.kind.as_str() {
                "list" => ("list", validation.values.join(","), String::new()),
                "number" => (
                    "decimal",
                    validation.min.map(|value| value.to_string()).unwrap_or_default(),
                    validation.max.map(|value| value.to_string()).unwrap_or_default(),
                ),
                _ => continue,
            };
            // A list drawn from cells (`=A1:A5`) is a bare reference in Excel;
            // quoting it would offer the literal text "=A1:A5" as the only choice.
            let quoted = match (kind, list_reference(&validation.values)) {
                ("list", Some(reference)) => reference,
                ("list", None) => format!("\"{formula1}\""),
                _ => formula1,
            };
            let second = if formula2.is_empty() {
                String::new()
            } else {
                format!("<formula2>{}</formula2>", escape_text(&formula2))
            };
            let prompt = if validation.message.is_empty() {
                String::new()
            } else {
                format!(" promptTitle=\"Invalid value\" error=\"{}\"", escape_attr(&validation.message))
            };
            let blank = if validation.allow_blank { 1 } else { 0 };
            body.push_str(&format!(
                "<dataValidation type=\"{kind}\" allowBlank=\"{blank}\" showInputMessage=\"1\" showErrorMessage=\"1\"{prompt} sqref=\"{}\"><formula1>{}</formula1>{second}</dataValidation>",
                escape_attr(&validation.range),
                escape_text(&quoted)
            ));
            count += 1;
        }
        if count > 0 {
            writer.raw(&format!("<dataValidations count=\"{count}\">{body}</dataValidations>"));
        }
    }

    // Hyperlinks: one external relationship per target.
    let mut rels: Vec<(String, String, String)> = Vec::new();
    if !hyperlinks.is_empty() {
        writer.raw(&format!("<hyperlinks count=\"{}\">", hyperlinks.len()));
        for (address, target) in &hyperlinks {
            let rid = format!("rId{}", rels.len() + 1);
            writer.raw(&format!("<hyperlink ref=\"{}\" r:id=\"{rid}\"/>", escape_attr(address)));
            rels.push((
                rid,
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink".into(),
                target.clone(),
            ));
        }
        writer.raw("</hyperlinks>");
    }

    writer.raw(&print_settings_xml(sheet));

    // Charts live in a drawing part; the worksheet only points at it. Per
    // CT_Worksheet the `<drawing>` element sits between the print settings and
    // the legacy (comment) drawing.
    if let Some(number) = drawing_number {
        rels.push((
            "rIdDrawing".into(),
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing".into(),
            format!("../drawings/drawing{number}.xml"),
        ));
        writer.raw("<drawing r:id=\"rIdDrawing\"/>");
    }

    // Comments are written as one part per sheet. A workbook-wide part made
    // every sheet read every other sheet's notes on the next import (audit
    // C11), because the comment list carries no sheet reference.
    if let Some(number) = comment_number {
        rels.push((
            "rIdComments".into(),
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments".into(),
            format!("../comments{number}.xml"),
        ));
        rels.push((
            "rIdVml".into(),
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/vmlDrawing".into(),
            format!("../drawings/vmlDrawing{number}.vml"),
        ));
        writer.raw("<legacyDrawing r:id=\"rIdVml\"/>");
    }

    // Structured tables live in their own parts; the worksheet only points at
    // them. Per CT_Worksheet `<tableParts>` comes after `<legacyDrawing>` and is
    // the last element before `</worksheet>`.
    if !table_numbers.is_empty() {
        writer.raw(&format!("<tableParts count=\"{}\">", table_numbers.len()));
        for (position, number) in table_numbers.iter().enumerate() {
            let rid = format!("rIdTable{position}");
            writer.raw(&format!("<tablePart r:id=\"{rid}\"/>"));
            rels.push((
                rid,
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/table".into(),
                format!("../tables/table{number}.xml"),
            ));
        }
        writer.raw("</tableParts>");
    }

    writer.raw("</worksheet>");
    SheetPart { xml: writer.finish(), rels, comments }
}

/// Builds a `<conditionalFormatting>` block plus the dxf index it points at.
///
/// Returns `None` for rule kinds that have no XLSX equivalent, which the caller
/// reports through the import/export warnings rather than emitting a broken
/// package.
fn conditional_formatting_xml(rule: &CondRule, index: usize) -> Option<String> {
    let priority = index + 1;
    let stop = if rule.stop_if_true { " stopIfTrue=\"1\"" } else { "" };
    let range = escape_attr(&rule.range);
    let dxf = CONDITIONAL_DXF_ID;
    let anchor = anchor_of(&rule.range);

    // A data bar does not use the shared dxf: its colour lives in the rule.
    if rule.kind == "dataBar" {
        let color = rule.fill.as_deref().map(argb_of).unwrap_or_else(|| "FF638EC6".into());
        return Some(format!(
            "<conditionalFormatting sqref=\"{range}\"><cfRule type=\"dataBar\" priority=\"{priority}\"{stop}><dataBar><cfvo type=\"min\"/><cfvo type=\"max\"/><color rgb=\"{color}\"/></dataBar></cfRule></conditionalFormatting>"
        ));
    }

    // (attributes, formula body) for the remaining rule kinds. The editor
    // writes `textContains` / `duplicate`; older files may carry the shorter
    // spellings, so both are accepted.
    let (attributes, formula): (String, String) = match rule.kind.as_str() {
        "greater" => (
            format!("type=\"cellIs\" dxfId=\"{dxf}\" priority=\"{priority}\"{stop} operator=\"greaterThan\""),
            format!("&gt;{}", escape_text(first_value(rule, "0"))),
        ),
        "less" => (
            format!("type=\"cellIs\" dxfId=\"{dxf}\" priority=\"{priority}\"{stop} operator=\"lessThan\""),
            format!("&lt;{}", escape_text(first_value(rule, "0"))),
        ),
        "equal" => (
            format!("type=\"cellIs\" dxfId=\"{dxf}\" priority=\"{priority}\"{stop} operator=\"equal\""),
            escape_text(first_value(rule, "0")),
        ),
        "between" => (
            format!("type=\"cellIs\" dxfId=\"{dxf}\" priority=\"{priority}\"{stop} operator=\"between\""),
            format!("{}~{}", escape_text(first_value(rule, "0")), escape_text(second_value(rule, "0"))),
        ),
        "text" | "textContains" => (
            format!(
                "type=\"containsText\" dxfId=\"{dxf}\" priority=\"{priority}\"{stop} operator=\"containsText\" text=\"{}\"",
                escape_attr(first_value(rule, ""))
            ),
            format!("NOT(ISERROR(SEARCH(&quot;{}&quot;,{})))", escape_text(first_value(rule, "")), anchor),
        ),
        "duplicates" | "duplicate" => (
            format!("type=\"duplicateValues\" dxfId=\"{dxf}\" priority=\"{priority}\"{stop}"),
            format!("COUNTIF({anchor},{anchor})>1"),
        ),
        "top" | "bottom" => {
            let rank = rule.top_n.unwrap_or(10);
            let bottom = if rule.kind == "bottom" { " bottom=\"1\"" } else { "" };
            (
                format!("type=\"top10\" dxfId=\"{dxf}\" priority=\"{priority}\"{stop} rank=\"{rank}\"{bottom}"),
                String::new(),
            )
        }
        _ => return None,
    };
    let formulas = if formula.is_empty() { String::new() } else { format!("<formula>{formula}</formula>") };
    Some(format!(
        "<conditionalFormatting sqref=\"{range}\"><cfRule {attributes}>{formulas}</cfRule></conditionalFormatting>"
    ))
}

fn first_value<'a>(rule: &'a CondRule, fallback: &'a str) -> &'a str {
    rule.values.first().map(String::as_str).unwrap_or(fallback)
}

fn second_value<'a>(rule: &'a CondRule, fallback: &'a str) -> &'a str {
    rule.values.get(1).map(String::as_str).unwrap_or(fallback)
}

/// The top-left cell of a range, used as the relative anchor in rule formulas.
fn anchor_of(range: &str) -> String {
    range.split(':').next().unwrap_or(range).to_string()
}

/// One shared dxf for the whole workbook; every rule reuses it so the styles
/// part stays small and consistent with what the editor preview shows.
const CONDITIONAL_DXF_ID: usize = 0;

/// Highlight colour used by the single shared differential format.
const CONDITIONAL_FILL: &str = "#FFF3C4";

// ---------------------------------------------------------------------------
// Charts
// ---------------------------------------------------------------------------

/// A chart that made it into the package, ready to be written.
struct PlannedChart {
    /// Position in the package-wide `xl/charts/chartN.xml` numbering.
    chart_number: usize,
    xml: String,
    anchor: String,
    width_px: f64,
    height_px: f64,
}

/// A sheet picture that made it into the package, ready to be written.
struct PlannedImage {
    /// File name inside `xl/media/`, e.g. `image1.png`.
    part_name: String,
    bytes: Vec<u8>,
    image: SheetImage,
}

/// The chart kinds the exporter writes as real ChartML parts.
fn chart_kind_supported(kind: &str) -> bool {
    matches!(kind, "column" | "bar" | "line" | "pie" | "area" | "scatter" | "doughnut")
}

/// Quotes a sheet name when a formula reference needs it (`'My Sheet'!`).
fn sheet_ref(name: &str) -> String {
    let simple =
        !name.is_empty() && name.chars().all(|character| character.is_ascii_alphanumeric() || character == '_');
    if simple {
        name.to_string()
    } else {
        format!("'{}'", name.replace('\'', "''"))
    }
}

/// `A1:B5` (relative, no sheet) as `Sheet!$A$1:$B$5`. `None` when malformed.
fn absolute_ref(range: &str, sheet: &str) -> Option<String> {
    let range = range.trim();
    if range.is_empty() {
        return None;
    }
    if let Some((start, end)) = range.split_once(':') {
        let (start_row, start_col) = crate::address::parse(start)?;
        let (end_row, end_col) = crate::address::parse(end)?;
        Some(format!(
            "{}!${}${}:${}${}",
            sheet_ref(sheet),
            crate::address::column_name(start_col),
            start_row + 1,
            crate::address::column_name(end_col),
            end_row + 1
        ))
    } else {
        let (row, column) = crate::address::parse(range)?;
        Some(format!("{}!${}${}", sheet_ref(sheet), crate::address::column_name(column), row + 1))
    }
}

/// `1:3` or `A:B` as the `Sheet!$1:$3` / `Sheet!$A:$B` form Excel stores in
/// `_xlnm.Print_Titles`. `None` when the range is not a row or column span.
fn print_titles_ref(range: &str, sheet: &str, rows: bool) -> Option<String> {
    let (start, end) = range.trim().split_once(':')?;
    let (start, end) = (start.trim(), end.trim());
    if start.is_empty() || end.is_empty() {
        return None;
    }
    if rows {
        let start_row = start.parse::<u32>().ok()?;
        let end_row = end.parse::<u32>().ok()?;
        Some(format!("{}!${}:${}", sheet_ref(sheet), start_row, end_row))
    } else {
        if !start.chars().all(|character| character.is_ascii_alphabetic())
            || !end.chars().all(|character| character.is_ascii_alphabetic())
        {
            return None;
        }
        Some(format!("{}!${}:${}", sheet_ref(sheet), start.to_ascii_uppercase(), end.to_ascii_uppercase()))
    }
}

/// A rich-text chart or axis title.
fn chart_title_xml(text: &str) -> String {
    format!(
        "<c:title><c:tx><c:rich><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang=\"en-US\"/><a:t>{}</a:t></a:r></a:p></c:rich></c:tx><c:overlay val=\"0\"/></c:title>",
        escape_text(text)
    )
}

const CHART_CATEGORY_AXIS: u64 = 111_111_111;
const CHART_VALUE_AXIS: u64 = 222_222_222;
/// Excel's default doughnut hole, the value `ChartData::hole_size == None` means.
const DOUGHNUT_DEFAULT_HOLE: u32 = 50;

/// One cached value the way Excel writes `c:v`: plain decimal where possible.
fn chart_cache_number(value: f64) -> String {
    if !value.is_finite() {
        return "0".into();
    }
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

/// `<c:strCache>` for cached category labels; empty when there is no cache.
fn chart_str_cache_xml(labels: &[String]) -> String {
    if labels.is_empty() {
        return String::new();
    }
    let mut out = format!("<c:strCache><c:ptCount val=\"{}\"/>", labels.len());
    for (index, label) in labels.iter().enumerate() {
        out.push_str(&format!("<c:pt idx=\"{index}\"><c:v>{}</c:v></c:pt>", escape_text(label)));
    }
    out.push_str("</c:strCache>");
    out
}

/// `<c:numCache>` for cached series values; empty when there is no cache.
fn chart_num_cache_xml(values: &[f64]) -> String {
    if values.is_empty() {
        return String::new();
    }
    let mut out = format!("<c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"{}\"/>", values.len());
    for (index, value) in values.iter().enumerate() {
        out.push_str(&format!("<c:pt idx=\"{index}\"><c:v>{}</c:v></c:pt>", chart_cache_number(*value)));
    }
    out.push_str("</c:numCache>");
    out
}

/// The category + value axis pair shared by every cartesian chart kind.
fn chart_axes_xml(chart: &ChartData) -> String {
    let category_title = if chart.x_title.is_empty() { String::new() } else { chart_title_xml(&chart.x_title) };
    let value_title = if chart.y_title.is_empty() { String::new() } else { chart_title_xml(&chart.y_title) };
    format!(
        "<c:catAx><c:axId val=\"{cat}\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"b\"/>{category_title}<c:crossAx val=\"{val}\"/></c:catAx><c:valAx><c:axId val=\"{val}\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"l\"/>{value_title}<c:crossAx val=\"{cat}\"/></c:valAx>",
        cat = CHART_CATEGORY_AXIS,
        val = CHART_VALUE_AXIS,
    )
}

/// The two value axes of a scatter chart: X along the bottom, Y on the left.
fn scatter_axes_xml(chart: &ChartData) -> String {
    let x_title = if chart.x_title.is_empty() { String::new() } else { chart_title_xml(&chart.x_title) };
    let y_title = if chart.y_title.is_empty() { String::new() } else { chart_title_xml(&chart.y_title) };
    format!(
        "<c:valAx><c:axId val=\"{x}\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"b\"/>{x_title}<c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:crossAx val=\"{y}\"/><c:crossBetween val=\"midCat\"/></c:valAx><c:valAx><c:axId val=\"{y}\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"l\"/>{y_title}<c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:crossAx val=\"{x}\"/><c:crossBetween val=\"midCat\"/></c:valAx>",
        x = CHART_CATEGORY_AXIS,
        y = CHART_VALUE_AXIS,
    )
}

/// What a scatter style draws: (lines, markers, smoothed lines). `None` and the
/// unknown spellings are markers only, which is Excel's plain "Scatter".
pub(crate) fn scatter_flavour(style: Option<&str>) -> (bool, bool, bool) {
    match style {
        Some("lineMarker") => (true, true, false),
        Some("line") => (true, false, false),
        Some("smoothMarker") => (true, true, true),
        Some("smooth") => (true, false, true),
        _ => (false, true, false),
    }
}

/// The `c:xVal` of a scatter series. X values that are all numbers are written
/// as a number reference with their cache; anything else stays a string
/// reference, which Excel plots as 1, 2, 3, ...
fn scatter_x_values_xml(reference: &str, cache: &[String]) -> String {
    let numbers: Vec<f64> = cache.iter().filter_map(|label| label.trim().parse::<f64>().ok()).collect();
    if cache.is_empty() || numbers.len() == cache.len() {
        format!(
            "<c:xVal><c:numRef><c:f>{}</c:f>{}</c:numRef></c:xVal>",
            escape_text(reference),
            chart_num_cache_xml(&numbers)
        )
    } else {
        format!(
            "<c:xVal><c:strRef><c:f>{}</c:f>{}</c:strRef></c:xVal>",
            escape_text(reference),
            chart_str_cache_xml(cache)
        )
    }
}

/// The marker and line look of one scatter series, before its data.
fn scatter_series_look_xml(color: Option<&str>, lines: bool, markers: bool) -> String {
    let rgb = color.map(|color| argb_of(color).get(2..).unwrap_or("000000").to_string());
    let line = match (&rgb, lines) {
        (_, false) => "<a:ln w=\"19050\"><a:noFill/></a:ln>".to_string(),
        (Some(rgb), true) => {
            format!(
                "<a:ln w=\"28575\" cap=\"rnd\"><a:solidFill><a:srgbClr val=\"{rgb}\"/></a:solidFill><a:round/></a:ln>"
            )
        }
        (None, true) => String::new(),
    };
    let shape = if line.is_empty() { String::new() } else { format!("<c:spPr>{line}</c:spPr>") };
    let marker = if !markers {
        "<c:marker><c:symbol val=\"none\"/></c:marker>".to_string()
    } else {
        let fill = rgb
            .map(|rgb| {
                format!(
                    "<c:spPr><a:solidFill><a:srgbClr val=\"{rgb}\"/></a:solidFill><a:ln w=\"9525\"><a:solidFill><a:srgbClr val=\"{rgb}\"/></a:solidFill></a:ln></c:spPr>"
                )
            })
            .unwrap_or_default();
        format!("<c:marker><c:symbol val=\"circle\"/><c:size val=\"7\"/>{fill}</c:marker>")
    };
    format!("{shape}{marker}")
}

/// Builds one `xl/charts/chartN.xml`. `Err` carries the user-facing reason the
/// chart cannot be represented; the caller keeps it in `.oswk` and warns.
fn chart_xml(placement: &ChartPlacement, sheet_name: &str) -> Result<String, String> {
    let chart = &placement.chart;
    let kind = chart.kind.as_str();
    if !chart_kind_supported(kind) {
        return Err(format!("the chart type \"{kind}\" is not exportable"));
    }
    let scatter = kind == "scatter";
    // A scatter chart may leave the X range empty (Excel then plots 1, 2, 3, ...);
    // every other kind needs its category range.
    let categories = absolute_ref(&chart.categories, sheet_name);
    if categories.is_none() && !(scatter && chart.categories.trim().is_empty()) {
        return Err("the category range could not be read".to_string());
    }
    let categories = categories.unwrap_or_default();
    if chart.series.is_empty() {
        return Err("the chart has no data series".into());
    }
    let (scatter_lines, scatter_markers, scatter_smooth) = scatter_flavour(chart.scatter_style.as_deref());

    let mut series_xml = String::new();
    for (index, series) in chart.series.iter().enumerate() {
        let values = absolute_ref(&series.range, sheet_name)
            .ok_or_else(|| format!("the range for series \"{}\" could not be read", series.name))?;
        series_xml.push_str(&format!(
            "<c:ser><c:idx val=\"{index}\"/><c:order val=\"{index}\"/><c:tx><c:v>{}</c:v></c:tx>",
            escape_text(&series.name)
        ));
        // Cached labels/values from an imported ChartML part are written back
        // so a chart whose source range lives outside the package still renders
        // and a second import sees the same model.
        let value_cache =
            chart.series_values_cache.get(index).map(|values| chart_num_cache_xml(values)).unwrap_or_default();
        if scatter {
            series_xml.push_str(&scatter_series_look_xml(series.color.as_deref(), scatter_lines, scatter_markers));
            if !categories.is_empty() {
                series_xml.push_str(&scatter_x_values_xml(&categories, &chart.categories_cache));
            }
            series_xml.push_str(&format!(
                "<c:yVal><c:numRef><c:f>{}</c:f>{value_cache}</c:numRef></c:yVal><c:smooth val=\"{}\"/></c:ser>",
                escape_text(&values),
                u8::from(scatter_smooth)
            ));
            continue;
        }
        if let Some(color) = series.color.as_deref() {
            let argb = argb_of(color);
            series_xml.push_str(&format!(
                "<c:spPr><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill></c:spPr>",
                argb.get(2..).unwrap_or("000000")
            ));
        }
        let category_cache = if index == 0 { chart_str_cache_xml(&chart.categories_cache) } else { String::new() };
        series_xml.push_str(&format!(
            "<c:cat><c:strRef><c:f>{}</c:f>{category_cache}</c:strRef></c:cat><c:val><c:numRef><c:f>{}</c:f>{value_cache}</c:numRef></c:val></c:ser>",
            // A sheet name can contain `&` or `<`; the reference has to be
            // XML-escaped or the chart part stops being well-formed XML.
            escape_text(&categories),
            escape_text(&values)
        ));
    }

    let labels = if chart.show_labels {
        "<c:dLbls><c:showLegendKey val=\"0\"/><c:showVal val=\"1\"/><c:showCatName val=\"0\"/><c:showSerName val=\"0\"/><c:showPercent val=\"0\"/><c:showBubbleSize val=\"0\"/></c:dLbls>"
    } else {
        ""
    };

    let mut plot = String::new();
    let mut axes = String::new();
    match kind {
        "column" | "bar" => {
            let direction = if kind == "bar" { "bar" } else { "col" };
            let grouping = if chart.stacked { "stacked" } else { "clustered" };
            let overlap = if chart.stacked { "<c:overlap val=\"100\"/>" } else { "" };
            plot.push_str(&format!(
                "<c:barChart><c:barDir val=\"{direction}\"/><c:grouping val=\"{grouping}\"/><c:varyColors val=\"0\"/>{series_xml}{labels}<c:gapWidth val=\"150\"/>{overlap}<c:axId val=\"{cat}\"/><c:axId val=\"{val}\"/></c:barChart>",
                cat = CHART_CATEGORY_AXIS,
                val = CHART_VALUE_AXIS,
            ));
            axes = chart_axes_xml(chart);
        }
        "line" => {
            plot.push_str(&format!(
                "<c:lineChart><c:grouping val=\"standard\"/><c:varyColors val=\"0\"/>{series_xml}{labels}<c:marker val=\"1\"/><c:axId val=\"{cat}\"/><c:axId val=\"{val}\"/></c:lineChart>",
                cat = CHART_CATEGORY_AXIS,
                val = CHART_VALUE_AXIS,
            ));
            axes = chart_axes_xml(chart);
        }
        "area" => {
            let grouping = if chart.stacked { "stacked" } else { "standard" };
            plot.push_str(&format!(
                "<c:areaChart><c:grouping val=\"{grouping}\"/><c:varyColors val=\"0\"/>{series_xml}{labels}<c:axId val=\"{cat}\"/><c:axId val=\"{val}\"/></c:areaChart>",
                cat = CHART_CATEGORY_AXIS,
                val = CHART_VALUE_AXIS,
            ));
            axes = chart_axes_xml(chart);
        }
        "pie" => {
            plot.push_str(&format!(
                "<c:pieChart><c:varyColors val=\"1\"/>{series_xml}{labels}<c:firstSliceAng val=\"0\"/></c:pieChart>"
            ));
        }
        "doughnut" => {
            let hole = chart.hole_size.unwrap_or(DOUGHNUT_DEFAULT_HOLE).clamp(10, 90);
            plot.push_str(&format!(
                "<c:doughnutChart><c:varyColors val=\"1\"/>{series_xml}{labels}<c:firstSliceAng val=\"0\"/><c:holeSize val=\"{hole}\"/></c:doughnutChart>"
            ));
        }
        "scatter" => {
            // Excel spells every drawn flavour `lineMarker` (or `smoothMarker`);
            // whether lines and markers show is decided per series above.
            let style = if scatter_smooth { "smoothMarker" } else { "lineMarker" };
            plot.push_str(&format!(
                "<c:scatterChart><c:scatterStyle val=\"{style}\"/><c:varyColors val=\"0\"/>{series_xml}{labels}<c:axId val=\"{cat}\"/><c:axId val=\"{val}\"/></c:scatterChart>",
                cat = CHART_CATEGORY_AXIS,
                val = CHART_VALUE_AXIS,
            ));
            axes = scatter_axes_xml(chart);
        }
        _ => return Err(format!("the chart type \"{kind}\" is not exportable")),
    }

    let title = if chart.title.is_empty() { String::new() } else { chart_title_xml(&chart.title) };
    let legend = if chart.legend { "<c:legend><c:legendPos val=\"b\"/><c:overlay val=\"0\"/></c:legend>" } else { "" };
    Ok(format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><c:roundedCorners val=\"0\"/><c:chart>{title}<c:plotArea><c:layout/>{plot}{axes}</c:plotArea>{legend}<c:plotVisOnly val=\"1\"/><c:dispBlanksAs val=\"gap\"/></c:chart><c:printSettings><c:headerFooter/><c:pageMargins b=\"0.75\" l=\"0.7\" r=\"0.7\" t=\"0.75\" header=\"0.3\" footer=\"0.3\"/><c:pageSetup/></c:printSettings></c:chartSpace>"
    ))
}

/// The drawing part that anchors a sheet's charts and pictures at their model
/// positions. Chart relationships come first so the rIds stay stable when only
/// one of the two kinds is present.
fn drawing_xml(charts: &[PlannedChart], images: &[PlannedImage]) -> String {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<xdr:wsDr xmlns:xdr=\"http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">",
    );
    for (index, chart) in charts.iter().enumerate() {
        let (row, column) = crate::address::parse(&chart.anchor).unwrap_or((0, 0));
        let cx = (chart.width_px.max(64.0) * 9525.0).round() as i64;
        let cy = (chart.height_px.max(64.0) * 9525.0).round() as i64;
        let rid = format!("rId{}", index + 1);
        xml.push_str(&format!(
            "<xdr:oneCellAnchor><xdr:from><xdr:col>{column}</xdr:col><xdr:colOff>190500</xdr:colOff><xdr:row>{row}</xdr:row><xdr:rowOff>95250</xdr:rowOff></xdr:from><xdr:ext cx=\"{cx}\" cy=\"{cy}\"/><xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"{id}\" name=\"Chart {id}\"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/></xdr:xfrm><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" r:id=\"{rid}\"/></a:graphicData></a:graphic></xdr:graphicFrame><xdr:clientData/></xdr:oneCellAnchor>",
            id = index + 1
        ));
    }
    for (index, planned) in images.iter().enumerate() {
        let image = &planned.image;
        let (row, column) = crate::address::parse(&image.anchor.address).unwrap_or((0, 0));
        let cx = (image.width_px.max(8.0) * 9525.0).round() as i64;
        let cy = (image.height_px.max(8.0) * 9525.0).round() as i64;
        let rid = format!("rId{}", charts.len() + index + 1);
        // `rot` is stored in 60000ths of a degree, mirrored by the importer.
        let rotation = if image.rotation_deg.abs() > 0.001 {
            format!(" rot=\"{}\"", (image.rotation_deg * 60_000.0).round() as i64)
        } else {
            String::new()
        };
        let alt = escape_attr(&image.image.alt);
        let id = charts.len() + index + 1;
        let picture = format!(
            "<xdr:pic><xdr:nvPicPr><xdr:cNvPr id=\"{id}\" name=\"Image {id}\" descr=\"{alt}\"/><xdr:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></xdr:cNvPicPr></xdr:nvPicPr><xdr:blipFill><a:blip xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" r:embed=\"{rid}\"/><a:stretch><a:fillRect/></a:stretch></xdr:blipFill><xdr:spPr><a:xfrm{rotation}><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic>"
        );
        let from = format!(
            "<xdr:from><xdr:col>{column}</xdr:col><xdr:colOff>{}</xdr:colOff><xdr:row>{row}</xdr:row><xdr:rowOff>{}</xdr:rowOff></xdr:from>",
            image.anchor.col_off_emu, image.anchor.row_off_emu
        );
        // A picture imported with a two-cell anchor keeps both corners; a model
        // picture without one is written as a one-cell anchor with its extent.
        match image.anchor.to_address.as_deref().and_then(crate::address::parse) {
            Some((to_row, to_column)) => {
                xml.push_str(&format!(
                    "<xdr:twoCellAnchor editAs=\"oneCell\">{from}<xdr:to><xdr:col>{to_column}</xdr:col><xdr:colOff>{}</xdr:colOff><xdr:row>{to_row}</xdr:row><xdr:rowOff>{}</xdr:rowOff></xdr:to>{picture}<xdr:clientData/></xdr:twoCellAnchor>",
                    image.anchor.to_col_off_emu, image.anchor.to_row_off_emu
                ));
            }
            None => {
                xml.push_str(&format!(
                    "<xdr:oneCellAnchor>{from}<xdr:ext cx=\"{cx}\" cy=\"{cy}\"/>{picture}<xdr:clientData/></xdr:oneCellAnchor>"
                ));
            }
        }
    }
    xml.push_str("</xdr:wsDr>");
    xml
}

/// The `<sheetProtection>` element for a sheet, or `None` when unprotected.
///
/// Every attribute is written verbatim so the SHA-512 verifier Excel wrote
/// comes back unchanged; the editor never tries to crack or bypass it. The
/// legacy `password` hash is emitted for files written before V3.1, whose
/// model only carried `sheet_protection`.
fn sheet_protection_xml(sheet: &Sheet) -> Option<String> {
    let protection = &sheet.protection;
    let legacy = sheet.sheet_protection.trim();
    let enabled = protection.enabled || !legacy.is_empty();
    if !enabled {
        return None;
    }
    let mut xml = String::from("<sheetProtection");
    xml.push_str(" sheet=\"1\"");
    if !legacy.is_empty() {
        xml.push_str(&format!(" password=\"{}\"", escape_attr(legacy)));
    } else if let Some(hash) = protection.password_hash.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        xml.push_str(&format!(" password=\"{}\"", escape_attr(hash)));
    }
    if !protection.algorithm_name.trim().is_empty() {
        xml.push_str(&format!(" algorithmName=\"{}\"", escape_attr(protection.algorithm_name.trim())));
    }
    if !protection.hash_value.trim().is_empty() {
        xml.push_str(&format!(" hashValue=\"{}\"", escape_attr(protection.hash_value.trim())));
    }
    if !protection.salt_value.trim().is_empty() {
        xml.push_str(&format!(" saltValue=\"{}\"", escape_attr(protection.salt_value.trim())));
    }
    if protection.spin_count > 0 {
        xml.push_str(&format!(" spinCount=\"{}\"", protection.spin_count));
    }
    let mut options: Vec<&str> =
        protection.options.iter().map(String::as_str).filter(|name| SHEET_PROTECTION_OPTIONS.contains(name)).collect();
    options.sort_unstable();
    options.dedup();
    for name in options {
        xml.push_str(&format!(" {name}=\"1\""));
    }
    xml.push_str("/>");
    Some(xml)
}

/// Paper size, orientation, margins, header/footer and page breaks.
///
/// The element order is the CT_Worksheet sequence (printOptions, pageMargins,
/// pageSetup, headerFooter, rowBreaks, colBreaks); writing them out of order
/// produces a package Excel refuses to open. Every stored field is emitted so
/// the settings survive an XLSX round trip.
fn print_settings_xml(sheet: &Sheet) -> String {
    let print = &sheet.print;
    let mut out = String::new();
    out.push_str(&format!(
        "<printOptions horizontalCentered=\"{}\" verticalCentered=\"{}\" gridLines=\"{}\" headings=\"{}\"/>",
        u8::from(print.center_horizontally),
        u8::from(print.center_vertically),
        u8::from(print.print_gridlines),
        u8::from(print.print_headings)
    ));
    out.push_str(&format!(
        "<pageMargins left=\"{:.2}\" right=\"{:.2}\" top=\"{:.2}\" bottom=\"{:.2}\" header=\"{:.2}\" footer=\"{:.2}\"/>",
        print.margin_left, print.margin_right, print.margin_top, print.margin_bottom, print.margin_header, print.margin_footer
    ));
    // Excel only honours `fitToWidth`/`fitToHeight` when `fitToPage` is on;
    // the model always stores the values, so the flag is derived from them.
    let fit_to_page = if print.fit_to_height > 0 || print.fit_to_width > 1 { " fitToPage=\"1\"" } else { "" };
    out.push_str(&format!(
        "<pageSetup paperSize=\"{}\" orientation=\"{}\" scale=\"{}\" fitToWidth=\"{}\" fitToHeight=\"{}\"{fit_to_page}/>",
        print.paper_size,
        if print.landscape { "landscape" } else { "portrait" },
        print.scale.clamp(10, 400),
        print.fit_to_width,
        print.fit_to_height
    ));
    let header_footer = |tag: &str, text: &str| -> String {
        if text.is_empty() {
            return String::new();
        }
        // Excel's format string carries its own section codes (`&L`, `&C`,
        // `&R`). Text that already opens with one is written verbatim; plain
        // text is centred, which is what the model means by `header`/`footer`.
        let formatted = if text.starts_with("&L") || text.starts_with("&C") || text.starts_with("&R") {
            text.to_string()
        } else {
            format!("&C{text}")
        };
        format!("<{tag}>{}</{tag}>", escape_text(&formatted))
    };
    let mut header = String::new();
    header.push_str(&header_footer("oddHeader", &print.header));
    header.push_str(&header_footer("oddFooter", &print.footer));
    if print.different_first_page {
        header.push_str(&header_footer("firstHeader", &print.first_header));
        header.push_str(&header_footer("firstFooter", &print.first_footer));
    }
    if print.different_odd_even {
        header.push_str(&header_footer("evenHeader", &print.even_header));
        header.push_str(&header_footer("evenFooter", &print.even_footer));
    }
    out.push_str(&format!(
        "<headerFooter differentFirst=\"{}\" differentOddEven=\"{}\">{header}</headerFooter>",
        u8::from(print.different_first_page),
        u8::from(print.different_odd_even)
    ));
    if !print.row_breaks.is_empty() {
        let breaks: Vec<u32> = {
            let mut breaks = print.row_breaks.clone();
            breaks.sort_unstable();
            breaks.dedup();
            breaks
        };
        out.push_str(&format!("<rowBreaks count=\"{}\" manualBreakCount=\"{}\">", breaks.len(), breaks.len()));
        for index in breaks {
            out.push_str(&format!("<brk id=\"{index}\" max=\"16383\" man=\"1\"/>"));
        }
        out.push_str("</rowBreaks>");
    }
    if !print.col_breaks.is_empty() {
        let breaks: Vec<u32> = {
            let mut breaks = print.col_breaks.clone();
            breaks.sort_unstable();
            breaks.dedup();
            breaks
        };
        out.push_str(&format!("<colBreaks count=\"{}\" manualBreakCount=\"{}\">", breaks.len(), breaks.len()));
        for index in breaks {
            out.push_str(&format!("<brk id=\"{index}\" max=\"1048575\" man=\"1\"/>"));
        }
        out.push_str("</colBreaks>");
    }
    out
}

/// True when a structured table on the sheet already owns this filter range.
///
/// The worksheet-level `<autoFilter>` must not be written for such a range: the
/// table part carries its own filter and Excel treats two filters over the same
/// range as a corrupt file.
fn filter_owned_by_table(sheet: &Sheet, range: &str) -> bool {
    sheet
        .tables
        .iter()
        .any(|table| table.filter.as_ref().map(|filter| filter.range.eq_ignore_ascii_case(range)).unwrap_or(false))
}

/// The range Excel filters for a table: the full range minus the totals row.
fn table_filter_range(table: &SpreadsheetTable) -> Option<String> {
    let ((start_row, start_col), (end_row, end_col)) = crate::address::parse_range(&table.range)?;
    let last_row = if table.has_totals { end_row.saturating_sub(1) } else { end_row };
    if last_row < start_row || end_col < start_col {
        return None;
    }
    Some(format!("{}:{}", crate::address::format(start_row, start_col), crate::address::format(last_row, end_col)))
}

/// A valid workbook-unique table name; OOXML names allow only a restricted set
/// of characters and may not look like a cell reference.
fn table_part_name(table: &SpreadsheetTable, number: usize) -> String {
    let mut name: String =
        table
            .name
            .trim()
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || character == '_' || character == '.' {
                    character
                } else {
                    '_'
                }
            })
            .collect();
    if name.is_empty() {
        name = format!("Table{number}");
    }
    let starts_well =
        name.chars().next().map(|character| character.is_ascii_alphabetic() || character == '_').unwrap_or(false);
    if !starts_well || crate::address::parse(&name).is_some() {
        name.insert(0, '_');
    }
    name
}

/// Number of columns a table part declares; falls back to the range width so a
/// table whose model columns were never filled in still exports a valid part.
fn table_column_count(table: &SpreadsheetTable) -> usize {
    let width = crate::address::parse_range(&table.range)
        .map(|((_, start_col), (_, end_col))| end_col.saturating_sub(start_col) as usize + 1)
        .unwrap_or(0);
    table.columns.len().max(width).max(1)
}

/// One `xl/tables/tableN.xml` part.
fn table_xml(table: &SpreadsheetTable, number: usize, name: &str) -> String {
    let header_rows = u8::from(table.has_headers);
    let totals_rows = u8::from(table.has_totals);
    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<table xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" id=\"{number}\" name=\"{name}\" displayName=\"{name}\" ref=\"{range}\" headerRowCount=\"{header_rows}\" totalsRowCount=\"{totals_rows}\">",
        name = escape_attr(name),
        range = escape_attr(&table.range),
    );
    if let Some(filter) = table.filter.as_ref() {
        let range = if filter.range.is_empty() {
            table_filter_range(table).unwrap_or_else(|| table.range.clone())
        } else {
            filter.range.clone()
        };
        if !range.is_empty() {
            if filter.values.is_empty() {
                xml.push_str(&format!("<autoFilter ref=\"{}\"/>", escape_attr(&range)));
            } else {
                xml.push_str(&format!(
                    "<autoFilter ref=\"{}\"><filterColumn colId=\"{}\"><filters>",
                    escape_attr(&range),
                    filter.column
                ));
                for value in filter.values.iter().take(MAX_FILTER_VALUES) {
                    xml.push_str(&format!("<filter val=\"{}\"/>", escape_attr(value)));
                }
                xml.push_str("</filters></filterColumn></autoFilter>");
            }
        }
    }
    let columns = table_column_count(table);
    xml.push_str(&format!("<tableColumns count=\"{columns}\">"));
    for index in 0..columns {
        let column = table.columns.get(index);
        let name = column
            .map(|column| column.name.trim())
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("Column{}", index + 1));
        match column.and_then(|column| column.formula.as_deref()).map(str::trim).filter(|formula| !formula.is_empty()) {
            Some(formula) => xml.push_str(&format!(
                "<tableColumn id=\"{}\" name=\"{}\"><calculatedColumnFormula>{}</calculatedColumnFormula></tableColumn>",
                index + 1,
                escape_attr(&name),
                escape_text(formula.trim_start_matches('='))
            )),
            None => xml.push_str(&format!("<tableColumn id=\"{}\" name=\"{}\"/>", index + 1, escape_attr(&name))),
        }
    }
    xml.push_str("</tableColumns>");
    // Excel needs a style blob for the banding to render; a table without an
    // explicit style falls back to the medium banded style.
    let style_name = if table.style_name.trim().is_empty() { "TableStyleMedium2" } else { table.style_name.trim() };
    xml.push_str(&format!(
        "<tableStyleInfo name=\"{}\" showFirstColumn=\"0\" showLastColumn=\"0\" showRowStripes=\"{}\" showColumnStripes=\"{}\"/>",
        escape_attr(style_name),
        u8::from(table.banded_rows),
        u8::from(table.banded_columns)
    ));
    xml.push_str("</table>");
    xml
}

pub fn write_xlsx_package(workbook: &Workbook) -> OfficeResult<SheetWrite> {
    let mut warnings = Vec::new();
    let mut styles = StyleTable::new();
    let mut shared: Vec<String> = Vec::new();
    let mut shared_index: BTreeMap<String, usize> = BTreeMap::new();

    // A preserved pivot without its raw parts cannot become a valid Excel part;
    // it stays in `.oswk` with a warning instead of producing a broken package.
    let preserved_pivots: Vec<&PreservedPivot> = workbook
        .preserved_pivots
        .iter()
        .filter(|pivot| !pivot.definition_xml.trim().is_empty() && !pivot.table_xml.trim().is_empty())
        .collect();
    if preserved_pivots.len() < workbook.preserved_pivots.len() {
        warnings.push(format!(
            "{} pivot table(s) had no preserved parts and were kept in the .oswk file only.",
            workbook.preserved_pivots.len() - preserved_pivots.len()
        ));
    }

    // Chart references use the sheet name, so names are resolved first.
    let sheet_names: Vec<String> = workbook
        .sheets
        .iter()
        .enumerate()
        .map(|(index, sheet)| {
            let mut name = sheet.name.clone();
            if name.is_empty() {
                name = format!("Sheet{}", index + 1);
            }
            name.truncate(31);
            name
        })
        .collect();

    // Charts: a supported chart becomes a real ChartML part; anything the
    // exporter cannot represent stays in `.oswk` and is reported instead of
    // silently producing a package Excel would reject.
    let mut chart_number = 0usize;
    let mut sheet_charts: Vec<Vec<PlannedChart>> = Vec::new();
    for (index, sheet) in workbook.sheets.iter().enumerate() {
        let mut planned = Vec::new();
        for placement in &sheet.charts {
            match chart_xml(placement, &sheet_names[index]) {
                Ok(xml) => {
                    chart_number += 1;
                    planned.push(PlannedChart {
                        chart_number,
                        xml,
                        anchor: if placement.anchor.is_empty() { "A1".into() } else { placement.anchor.clone() },
                        width_px: placement.width_px.max(64.0),
                        height_px: placement.height_px.max(64.0),
                    });
                }
                Err(reason) => warnings
                    .push(format!("Chart \"{}\" was kept in the .oswk file only: {reason}.", placement.chart.title)),
            }
        }
        sheet_charts.push(planned);
    }

    // Pictures: every non-empty image becomes an `xl/media` part referenced
    // from the sheet's drawing. A picture without bytes cannot be written as a
    // valid media part and stays in `.oswk` with a warning.
    let mut media_number = 0usize;
    let mut sheet_images: Vec<Vec<PlannedImage>> = Vec::new();
    for sheet in &workbook.sheets {
        let mut planned = Vec::new();
        for image in &sheet.images {
            if image.image.is_empty() {
                warnings
                    .push(format!("Image \"{}\" has no data and was kept in the .oswk file only.", image.image.name));
                continue;
            }
            media_number += 1;
            let part_name = format!("image{media_number}.{}", image.image.extension());
            planned.push(PlannedImage { part_name, bytes: image.image.bytes(), image: image.clone() });
        }
        sheet_images.push(planned);
    }

    // Structured tables: numbered workbook-wide so the worksheet tableParts,
    // the relationship targets and the content-type overrides agree. A table
    // with an unreadable range is skipped with a warning instead of producing a
    // package Excel would reject; names are deduplicated workbook-wide.
    let mut table_number = 0usize;
    let mut used_table_names: Vec<String> = Vec::new();
    let mut sheet_table_numbers: Vec<Vec<usize>> = Vec::new();
    let mut table_parts: Vec<(usize, String)> = Vec::new();
    for sheet in &workbook.sheets {
        let mut numbers = Vec::new();
        for table in &sheet.tables {
            if crate::address::parse_range(&table.range).is_none() {
                warnings.push(format!(
                    "Table \"{}\" has a range that could not be read and was kept in the .oswk file only.",
                    table.name
                ));
                continue;
            }
            table_number += 1;
            let mut name = table_part_name(table, table_number);
            while used_table_names.iter().any(|used| used.eq_ignore_ascii_case(&name)) {
                name = format!("{name}_{table_number}");
            }
            used_table_names.push(name.clone());
            numbers.push(table_number);
            table_parts.push((table_number, table_xml(table, table_number, &name)));
        }
        sheet_table_numbers.push(numbers);
    }

    let mut sheet_parts: Vec<SheetPart> = Vec::new();
    let mut sheet_drawings: Vec<Option<usize>> = Vec::new();
    let mut drawing_number = 0usize;
    let mut comment_number = 0usize;
    let mut pivots_materialized = 0usize;
    for (index, sheet) in workbook.sheets.iter().enumerate() {
        let drawing = if sheet_charts[index].is_empty() && sheet_images[index].is_empty() {
            None
        } else {
            drawing_number += 1;
            Some(drawing_number)
        };
        sheet_drawings.push(drawing);
        let comments = sheet_comments(sheet);
        let comment = if comments.is_empty() {
            None
        } else {
            comment_number += 1;
            Some(comment_number)
        };
        let pivot_cells = crate::pivot::materialize(workbook, sheet);
        if !pivot_cells.is_empty() {
            pivots_materialized += 1;
        }
        sheet_parts.push(sheet_xml(
            sheet,
            drawing,
            comment,
            comments,
            &pivot_cells,
            &sheet_table_numbers[index],
            &mut styles,
            &mut shared,
            &mut shared_index,
        ));
    }
    if pivots_materialized > 0 {
        warnings.push(format!(
            "Pivot tables are written as their computed values (one sheet so far: {pivots_materialized}); the live pivot definition stays in the .oswk file."
        ));
    }

    // Worksheet -> pivot table relationships. A pivot table has no element of
    // its own in the worksheet XML; Excel finds it through the sheet's rels.
    if !workbook.sheets.is_empty() {
        for (pivot_index, pivot) in preserved_pivots.iter().enumerate() {
            let number = pivot_index + 1;
            let sheet_index = workbook.sheets.iter().position(|sheet| sheet.name == pivot.sheet).unwrap_or(0);
            sheet_parts[sheet_index].rels.push((
                format!("rIdPivot{number}"),
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotTable".into(),
                format!("../pivotTables/pivotTable{number}.xml"),
            ));
        }
    }

    let mut zip = ZipWriter::new();
    let mut content_types = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">",
    );
    content_types.push_str(
        "<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>",
    );
    content_types.push_str("<Default Extension=\"xml\" ContentType=\"application/xml\"/>");
    content_types.push_str("<Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>");
    for index in 0..sheet_parts.len() {
        content_types.push_str(&format!(
            "<Override PartName=\"/xl/worksheets/sheet{}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>",
            index + 1
        ));
    }
    content_types.push_str("<Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>");
    content_types.push_str("<Override PartName=\"/xl/sharedStrings.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml\"/>");
    content_types.push_str("<Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/>");
    content_types.push_str("<Override PartName=\"/docProps/app.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.extended-properties+xml\"/>");
    // `</Types>` is appended after every optional part below: comments and
    // charts add overrides, and appending them after the closing tag produced
    // a package no other reader would open.

    let mut root_rels = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
    );
    root_rels.push_str("<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/>");
    root_rels.push_str("<Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/>");
    root_rels.push_str("<Relationship Id=\"rId3\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties\" Target=\"docProps/app.xml\"/>");
    root_rels.push_str("</Relationships>");

    let mut workbook_xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheets>",
    );
    for (index, name) in sheet_names.iter().enumerate() {
        workbook_xml.push_str(&format!(
            "<sheet name=\"{}\" sheetId=\"{}\" r:id=\"rId{}\"/>",
            escape_attr(name),
            index + 1,
            index + 1
        ));
    }
    workbook_xml.push_str("</sheets>");
    // Defined names: workbook-level ones first, then each sheet's own. Excel
    // scopes a name with `localSheetId`, which is the sheet's position.
    let mut defined = String::new();
    let mut defined_count = 0usize;
    for entry in &workbook.names {
        if !entry.is_workbook_scope() {
            continue;
        }
        if let Some(xml) = defined_name_xml(entry, None) {
            defined.push_str(&xml);
            defined_count += 1;
        }
    }
    for (index, sheet_name) in sheet_names.iter().enumerate() {
        for entry in &workbook.names {
            if entry.is_workbook_scope() || entry.sheet.as_deref() != Some(sheet_name.as_str()) {
                continue;
            }
            if let Some(xml) = defined_name_xml(entry, Some(index)) {
                defined.push_str(&xml);
                defined_count += 1;
            }
        }
    }
    // A sheet's AutoFilter range is itself a defined name in the OOXML spec.
    // A range a structured table filters is skipped for the same reason as the
    // worksheet `<autoFilter>` above.
    for (index, sheet) in workbook.sheets.iter().enumerate() {
        if let Some(filter) = &sheet.filter {
            if !filter.range.is_empty() && !filter_owned_by_table(sheet, &filter.range) {
                defined.push_str(&format!(
                    "<definedName name=\"_xlnm._FilterDatabase\" localSheetId=\"{}\" hidden=\"1\">{}!{}</definedName>",
                    index,
                    escape_text(&sheet_names[index]),
                    escape_text(&filter.range)
                ));
                defined_count += 1;
            }
        }
    }
    // Print area and repeating titles are `_xlnm` names scoped to the sheet.
    for (index, sheet) in workbook.sheets.iter().enumerate() {
        if let Some(area) = sheet.print.print_area.as_deref() {
            match absolute_ref(area, &sheet_names[index]) {
                Some(reference) => {
                    defined.push_str(&format!(
                        "<definedName name=\"_xlnm.Print_Area\" localSheetId=\"{index}\">{}</definedName>",
                        escape_text(&reference)
                    ));
                    defined_count += 1;
                }
                None => warnings.push(format!(
                    "The print area of sheet \"{}\" could not be read and was kept in the .oswk file only.",
                    sheet_names[index]
                )),
            }
        }
        let mut titles: Vec<String> = Vec::new();
        if let Some(rows) = sheet.print.print_titles_rows.as_deref() {
            if let Some(reference) = print_titles_ref(rows, &sheet_names[index], true) {
                titles.push(reference);
            } else {
                warnings.push(format!(
                    "The repeating rows of sheet \"{}\" could not be read and were kept in the .oswk file only.",
                    sheet_names[index]
                ));
            }
        }
        if let Some(columns) = sheet.print.print_titles_cols.as_deref() {
            if let Some(reference) = print_titles_ref(columns, &sheet_names[index], false) {
                titles.push(reference);
            } else {
                warnings.push(format!(
                    "The repeating columns of sheet \"{}\" could not be read and were kept in the .oswk file only.",
                    sheet_names[index]
                ));
            }
        }
        if !titles.is_empty() {
            defined.push_str(&format!(
                "<definedName name=\"_xlnm.Print_Titles\" localSheetId=\"{index}\">{}</definedName>",
                escape_text(&titles.join(","))
            ));
            defined_count += 1;
        }
    }
    if defined_count > 0 {
        workbook_xml.push_str(&format!("<definedNames>{defined}</definedNames>"));
    }
    // Pivot caches that were imported raw keep their original `cacheId`; the
    // workbook only has to point Excel at the definition part.
    let pivot_rid_base = sheet_parts.len() + 3;
    if !preserved_pivots.is_empty() {
        workbook_xml.push_str("<pivotCaches>");
        for (index, pivot) in preserved_pivots.iter().enumerate() {
            let cache_id = if pivot.cache_id > 0 { pivot.cache_id } else { index as u32 + 1 };
            workbook_xml
                .push_str(&format!("<pivotCache cacheId=\"{cache_id}\" r:id=\"rId{}\"/>", pivot_rid_base + index));
        }
        workbook_xml.push_str("</pivotCaches>");
    }
    workbook_xml.push_str("</workbook>");

    let mut workbook_rels = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
    );
    for (index, _) in sheet_parts.iter().enumerate() {
        workbook_rels.push_str(&format!(
            "<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet{}.xml\"/>",
            index + 1,
            index + 1
        ));
    }
    let styles_rid = sheet_parts.len() + 1;
    let shared_rid = sheet_parts.len() + 2;
    workbook_rels.push_str(&format!(
        "<Relationship Id=\"rId{styles_rid}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>"
    ));
    workbook_rels.push_str(&format!(
        "<Relationship Id=\"rId{shared_rid}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings\" Target=\"sharedStrings.xml\"/>"
    ));
    for (index, _) in preserved_pivots.iter().enumerate() {
        workbook_rels.push_str(&format!(
            "<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotCacheDefinition\" Target=\"pivotCache/pivotCacheDefinition{}.xml\"/>",
            pivot_rid_base + index,
            index + 1
        ));
    }
    workbook_rels.push_str("</Relationships>");

    let mut shared_xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<sst xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"",
    );
    shared_xml.push_str(&format!(" count=\"{}\" uniqueCount=\"{}\">", shared.len(), shared.len()));
    for text in &shared {
        shared_xml.push_str(&format!("<si><t xml:space=\"preserve\">{}</t></si>", escape_text(text)));
    }
    shared_xml.push_str("</sst>");

    let core = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:title>{}</dc:title><dc:creator>{}</dc:creator></cp:coreProperties>",
        escape_text(&workbook.title),
        escape_text(&workbook.metadata.author)
    );
    let app = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\"><Application>OmniOffice</Application></Properties>";

    let with_comments = sheet_parts.iter().any(|part| !part.comments.is_empty());
    if with_comments {
        let mut comment_number = 0usize;
        for part in &sheet_parts {
            if part.comments.is_empty() {
                continue;
            }
            comment_number += 1;
            content_types.push_str(&format!(
                "<Override PartName=\"/xl/comments{comment_number}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.comments+xml\"/>"
            ));
        }
        content_types.push_str(
            "<Default Extension=\"vml\" ContentType=\"application/vnd.openxmlformats-officedocument.vmlDrawing\"/>",
        );
    }

    for (index, charts) in sheet_charts.iter().enumerate() {
        if charts.is_empty() && sheet_images[index].is_empty() {
            continue;
        }
        if let Some(drawing) = sheet_drawings[index] {
            content_types.push_str(&format!(
                "<Override PartName=\"/xl/drawings/drawing{drawing}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.drawing+xml\"/>"
            ));
        }
        for chart in charts {
            content_types.push_str(&format!(
                "<Override PartName=\"/xl/charts/chart{}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.drawingml.chart+xml\"/>",
                chart.chart_number
            ));
        }
    }

    // One `Default` per image extension the workbook actually uses.
    let mut media_extensions: BTreeMap<&str, &str> = BTreeMap::new();
    for images in &sheet_images {
        for image in images {
            media_extensions.insert(image.image.image.extension(), image.image.image.mime.as_str());
        }
    }
    for (extension, mime) in media_extensions {
        content_types.push_str(&format!(
            "<Default Extension=\"{}\" ContentType=\"{}\"/>",
            escape_attr(extension),
            escape_attr(mime)
        ));
    }

    for (number, _) in &table_parts {
        content_types.push_str(&format!(
            "<Override PartName=\"/xl/tables/table{number}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml\"/>"
        ));
    }
    for (index, _) in preserved_pivots.iter().enumerate() {
        let number = index + 1;
        content_types.push_str(&format!(
            "<Override PartName=\"/xl/pivotTables/pivotTable{number}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.pivotTable+xml\"/>"
        ));
        content_types.push_str(&format!(
            "<Override PartName=\"/xl/pivotCache/pivotCacheDefinition{number}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheDefinition+xml\"/>"
        ));
        if preserved_pivots[index].records_base64.is_some() {
            content_types.push_str(&format!(
                "<Override PartName=\"/xl/pivotCache/pivotCacheRecords{number}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheRecords+xml\"/>"
            ));
        }
    }

    content_types.push_str("</Types>");
    zip.add_text("[Content_Types].xml", &content_types);
    zip.add_text("_rels/.rels", &root_rels);
    zip.add_text("docProps/core.xml", &core);
    zip.add_text("docProps/app.xml", app);
    zip.add_text("xl/workbook.xml", &workbook_xml);
    zip.add_text("xl/_rels/workbook.xml.rels", &workbook_rels);
    zip.add_text("xl/styles.xml", &styles.xml());
    zip.add_text("xl/sharedStrings.xml", &shared_xml);
    for (index, part) in sheet_parts.iter().enumerate() {
        zip.add_text(&format!("xl/worksheets/sheet{}.xml", index + 1), &part.xml);
        if !part.rels.is_empty() {
            let mut rels = String::from(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
            );
            for (id, kind, target) in &part.rels {
                let extra = if kind.ends_with("/hyperlink") { " TargetMode=\"External\"" } else { "" };
                rels.push_str(&format!(
                    "<Relationship Id=\"{id}\" Type=\"{kind}\" Target=\"{}\"{extra}/>",
                    escape_attr(target)
                ));
            }
            rels.push_str("</Relationships>");
            zip.add_text(&format!("xl/worksheets/_rels/sheet{}.xml.rels", index + 1), &rels);
        }
    }
    for (index, charts) in sheet_charts.iter().enumerate() {
        let Some(drawing) = sheet_drawings[index] else { continue };
        let images = &sheet_images[index];
        zip.add_text(&format!("xl/drawings/drawing{drawing}.xml"), &drawing_xml(charts, images));
        let mut drawing_rels = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
        );
        for (position, chart) in charts.iter().enumerate() {
            drawing_rels.push_str(&format!(
                "<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart\" Target=\"../charts/chart{}.xml\"/>",
                position + 1,
                chart.chart_number
            ));
        }
        for (position, image) in images.iter().enumerate() {
            drawing_rels.push_str(&format!(
                "<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"../media/{}\"/>",
                charts.len() + position + 1,
                escape_attr(&image.part_name)
            ));
        }
        drawing_rels.push_str("</Relationships>");
        zip.add_text(&format!("xl/drawings/_rels/drawing{drawing}.xml.rels"), &drawing_rels);
        for chart in charts {
            zip.add_text(&format!("xl/charts/chart{}.xml", chart.chart_number), &chart.xml);
        }
        for image in images {
            zip.add(&format!("xl/media/{}", image.part_name), &image.bytes);
        }
    }
    for (number, xml) in &table_parts {
        zip.add_text(&format!("xl/tables/table{number}.xml"), xml);
    }
    // Preserved pivots: raw parts plus the two relationship files Excel needs
    // to find the records and the cache definition.
    for (index, pivot) in preserved_pivots.iter().enumerate() {
        let number = index + 1;
        zip.add_text(&format!("xl/pivotCache/pivotCacheDefinition{number}.xml"), &pivot.definition_xml);
        zip.add_text(&format!("xl/pivotTables/pivotTable{number}.xml"), &pivot.table_xml);
        zip.add_text(
            &format!("xl/pivotTables/_rels/pivotTable{number}.xml.rels"),
            &format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotCacheDefinition\" Target=\"../pivotCache/pivotCacheDefinition{number}.xml\"/></Relationships>"
            ),
        );
        if let Some(records) = pivot.records_base64.as_deref().filter(|value| !value.is_empty()) {
            let bytes = base64::engine::general_purpose::STANDARD.decode(records.as_bytes()).unwrap_or_default();
            zip.add(&format!("xl/pivotCache/pivotCacheRecords{number}.xml"), &bytes);
            zip.add_text(
                &format!("xl/pivotCache/_rels/pivotCacheDefinition{number}.xml.rels"),
                &format!(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotCacheRecords\" Target=\"pivotCacheRecords{number}.xml\"/></Relationships>"
                ),
            );
        } else {
            zip.add_text(
                &format!("xl/pivotCache/_rels/pivotCacheDefinition{number}.xml.rels"),
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"/>",
            );
        }
    }

    if with_comments {
        // One comments part plus one VML shape set per commented sheet, all
        // pointed at by that sheet's own relationships (audit C11).
        let mut comment_number = 0usize;
        for part in &sheet_parts {
            if part.comments.is_empty() {
                continue;
            }
            comment_number += 1;
            zip.add_text(&format!("xl/comments{comment_number}.xml"), &comments_xml(part));
            zip.add_text(&format!("xl/drawings/vmlDrawing{comment_number}.vml"), &comments_vml(part));
        }
    }
    Ok(SheetWrite { bytes: zip.finish(), warnings })
}

/// Emits one `<definedName>` element, or `None` when the entry is unusable.
///
/// A name has to start with a letter or underscore and may not look like a
/// cell reference, otherwise Excel refuses to open the file - so a bad entry is
/// dropped rather than written out.
fn defined_name_xml(entry: &NamedRange, local_sheet_id: Option<usize>) -> Option<String> {
    let name = entry.name.trim();
    if name.is_empty() || entry.definition.trim().is_empty() {
        return None;
    }
    if !name.chars().next()?.is_alphabetic() && !name.starts_with('_') {
        return None;
    }
    if name.chars().any(|character| !(character.is_alphanumeric() || character == '_' || character == '.')) {
        return None;
    }
    if crate::address::parse(name).is_some() {
        return None;
    }
    let scope = match local_sheet_id {
        Some(index) => format!(" localSheetId=\"{index}\""),
        None => String::new(),
    };
    let mut definition = entry.definition.trim().to_string();
    if !definition.starts_with('=') {
        definition = format!("={definition}");
    }
    Some(format!("<definedName name=\"{}\"{}>{}</definedName>", escape_attr(name), scope, escape_text(&definition)))
}

/// The `<comments>` part for one sheet, carrying only that sheet's notes.
fn comments_xml(part: &SheetPart) -> String {
    let mut comments_xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<comments xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><authors><author>OmniOffice</author></authors><commentList>",
    );
    for (address, text) in &part.comments {
        comments_xml.push_str(&format!(
            "<comment ref=\"{address}\" authorId=\"0\"><text><r><rPr><sz val=\"9\"/></rPr><t xml:space=\"preserve\">{text}</t></r></text></comment>",
            text = escape_text(text)
        ));
    }
    comments_xml.push_str("</commentList></comments>");
    comments_xml
}

/// The VML drawing Excel needs in order to show a comment marker on a cell.
///
/// OOXML stores comment *text* in the comments part but the little red triangle
/// and the hover box are legacy VML shapes, so a comments part without this
/// file opens with invisible notes. One drawing per commented sheet.
fn comments_vml(part: &SheetPart) -> String {
    let mut vml = String::from(
        "<xml xmlns:v=\"urn:schemas-microsoft-com:vml\" xmlns:o=\"urn:schemas-microsoft-com:office:office\" xmlns:x=\"urn:schemas-microsoft-com:office:excel\">",
    );
    vml.push_str("<o:shapelayout v:ext=\"edit\"><o:idmap v:ext=\"edit\" data=\"1\"/></o:shapelayout>");
    vml.push_str(
        "<v:shapetype id=\"_x0000_t202\" coordsize=\"21600,21600\" o:spt=\"202\" path=\"m,l,21600r21600,l21600,xe\"><v:stroke joinstyle=\"miter\"/><v:path gradientshapeok=\"t\" o:connecttype=\"rect\"/></v:shapetype>",
    );
    for (shape_id, (address, _)) in (1025u32..).zip(part.comments.iter()) {
        // Column and row are zero based in the ClientData block.
        let (row, column) = crate::address::parse(address).unwrap_or((0, 0));
        vml.push_str(&format!(
            "<v:shape id=\"_x0000_s{shape_id}\" type=\"#_x0000_t202\" style=\"position:absolute;margin-left:59.25pt;margin-top:1.5pt;width:108pt;height:59.25pt;z-index:1;visibility:hidden\" fillcolor=\"#ffffe1\" o:insetmode=\"auto\"><v:fill color2=\"#ffffe1\"/><v:shadow on=\"t\" color=\"black\" obscured=\"t\"/><v:path o:connecttype=\"none\"/><v:textbox style=\"mso-direction-alt:auto\"><div style=\"text-align:left\"/></v:textbox><x:ClientData ObjectType=\"Note\"><x:MoveWithCells/><x:SizeWithCells/><x:AutoFill>False</x:AutoFill><x:Row>{row}</x:Row><x:Column>{column}</x:Column></x:ClientData></v:shape>"
        ));
    }
    vml.push_str("</xml>");
    vml
}

/// `#RRGGBB` (or `RRGGBB`) to the `AARRGGBB` form XLSX attributes use.
fn argb_of(color: &str) -> String {
    let hex: String = color.chars().filter(|character| character.is_ascii_hexdigit()).collect();
    match hex.len() {
        6 => format!("FF{hex}"),
        8 => hex,
        _ => "FF000000".into(),
    }
}

pub fn write_xlsx(workbook: &Workbook) -> OfficeResult<Vec<u8>> {
    Ok(write_xlsx_package(workbook)?.bytes)
}

pub fn write_xlsx_file(path: &Path, workbook: &Workbook) -> OfficeResult<()> {
    let result = write_xlsx_package(workbook)?;
    write_atomic(path, &result.bytes)
}

// ---------------------------------------------------------------------------
// Import (XLSX / XLS / ODS via calamine)
// ---------------------------------------------------------------------------

fn contains_part(bytes: &[u8], needle: &str) -> bool {
    crate::zip::ZipReader::open(bytes.to_vec())
        .map(|reader| reader.names().any(|name| name.contains(needle)))
        .unwrap_or(false)
}

/// A copy of an OOXML package with `xl/styles.xml` dropped.
///
/// Used only as a fallback when `calamine` refuses to open a workbook because
/// that part is malformed; dropping it lets the values open and the detailed
/// pass reports the damaged styles.
fn package_without_styles(bytes: &[u8]) -> Option<Vec<u8>> {
    let zip = crate::zip::ZipReader::open(bytes.to_vec()).ok()?;
    if !zip.contains("xl/workbook.xml") {
        return None;
    }
    let names: Vec<String> = zip.names().map(str::to_string).collect();
    let mut writer = ZipWriter::new();
    for name in names {
        if name == "xl/styles.xml" {
            continue;
        }
        let data = zip.read(&name).ok()?;
        writer.add(&name, &data);
    }
    Some(writer.finish())
}

pub fn read_workbook_bytes(bytes: &[u8]) -> OfficeResult<SheetRead> {
    if bytes.len() < 8 {
        return Err(OfficeError::corrupt("The file is too small to be a spreadsheet."));
    }
    let mut warnings: Vec<String> = Vec::new();
    let mut workbook = Workbook::new_blank("Imported workbook");
    workbook.sheets.clear();

    let cursor = Cursor::new(bytes.to_vec());
    let mut sheets = match calamine::open_workbook_auto_from_rs(cursor) {
        Ok(sheets) => sheets,
        Err(error) => {
            // `calamine` reads `xl/styles.xml` for number formats and fails on a
            // malformed part, which would lose every value. Retry without the
            // damaged part; the OOXML layout pass below reports it as a warning.
            let Some(repaired) = package_without_styles(bytes) else {
                return Err(OfficeError::corrupt(format!("Could not read the spreadsheet: {error}")));
            };
            calamine::open_workbook_auto_from_rs(Cursor::new(repaired))
                .map_err(|error| OfficeError::corrupt(format!("Could not read the spreadsheet: {error}")))?
        }
    };
    let names: Vec<String> = sheets.sheet_names().to_vec();
    if names.is_empty() {
        return Err(OfficeError::corrupt("The spreadsheet does not contain any sheet."));
    }
    let mut used_names: Vec<String> = Vec::new();
    for name in &names {
        let mut sheet = Sheet::new(name);
        let range = sheets
            .worksheet_range(name)
            .map_err(|error| OfficeError::corrupt(format!("Could not read sheet {name}: {error}")))?;
        let formulas = sheets.worksheet_formula(name).ok();
        let (start_row, start_col) =
            (range.start().map(|(row, _)| row).unwrap_or(0), range.start().map(|(_, column)| column).unwrap_or(0));
        let mut max_row = 0u32;
        let mut max_col = 0u32;
        for (row_index, row) in range.rows().enumerate() {
            for (column_index, value) in row.iter().enumerate() {
                let row_number = start_row + row_index as u32;
                let column_number = start_col + column_index as u32;
                if row_number > 100_000 || column_number > 1_000 {
                    continue;
                }
                let cell_value = match value {
                    calamine::Data::Empty => continue,
                    calamine::Data::Int(number) => CellValue::Number(*number as f64),
                    calamine::Data::Float(number) => CellValue::Number(*number),
                    calamine::Data::String(text) => CellValue::Text(text.clone()),
                    calamine::Data::Bool(flag) => CellValue::Bool(*flag),
                    #[allow(unreachable_patterns)]
                    calamine::Data::DateTime(serial) => CellValue::Number(serial.as_f64()),
                    calamine::Data::DateTimeIso(text) => CellValue::Text(text.clone()),
                    calamine::Data::DurationIso(text) => CellValue::Text(text.clone()),
                    calamine::Data::Error(error) => CellValue::Error(format!("{error:?}")),
                };
                let formula = formulas
                    .as_ref()
                    .and_then(|range| range.get_value((row_number, column_number)))
                    .filter(|text| !text.is_empty())
                    .map(|text| {
                        let trimmed = text.trim().trim_start_matches('=');
                        format!("={trimmed}")
                    });
                if matches!(cell_value, CellValue::Empty) && formula.is_none() {
                    continue;
                }
                let address = crate::address::format(row_number, column_number);
                sheet.set(&address, Cell { value: cell_value, formula, ..Default::default() });
                max_row = max_row.max(row_number);
                max_col = max_col.max(column_number);
            }
        }
        sheet.row_count = (max_row + 51).max(200);
        sheet.col_count = (max_col + 6).max(26);
        if used_names.iter().any(|existing| existing == &sheet.name) {
            let unique = {
                let mut index = 2;
                loop {
                    let candidate = format!("{} ({index})", sheet.name);
                    if !used_names.contains(&candidate) {
                        break candidate;
                    }
                    index += 1;
                }
            };
            sheet.name = unique;
        }
        used_names.push(sheet.name.clone());
        workbook.sheets.push(sheet);
    }

    if workbook.sheets.is_empty() {
        workbook.sheets.push(Sheet::new("Sheet1"));
    }
    // Second pass: read the OOXML parts calamine does not expose (styles,
    // layout, validations, conditional rules, defined names, hyperlinks,
    // comments, structured tables, drawings, print settings, protection and
    // pivot caches). It is best effort: every failure is a warning and the
    // values from the first pass stay untouched.
    if !import_ooxml_layout(bytes, &mut workbook, &mut warnings) {
        warnings.push("Formatting, layout and comments are imported from XLSX/XLSM workbooks only; this file opened with values and formulas.".into());
    }
    // Only warn about charts/images/pivots that are present in the package but
    // could not be attached to a sheet; imported ones are reported by the
    // importer itself when they need a caveat.
    let imported_charts: usize = workbook.sheets.iter().map(|sheet| sheet.charts.len()).sum();
    if imported_charts == 0 && contains_part(bytes, "xl/charts") {
        warnings.push(
            "Charts are present in the package but no sheet drawing could be linked, so they were not imported.".into(),
        );
    }
    let imported_images: usize = workbook.sheets.iter().map(|sheet| sheet.images.len()).sum();
    if imported_images == 0 && contains_part(bytes, "xl/media/") {
        warnings.push(
            "Images are present in the package but no sheet drawing could be linked, so they were not imported.".into(),
        );
    }
    if contains_part(bytes, "xl/charts/colors") {
        warnings.push(
            "Chart colour overrides (xl/charts/colorsN.xml) are not imported; series colours come from the chart part."
                .into(),
        );
    }
    if contains_part(bytes, "Object ") {
        warnings.push("Embedded OLE objects are not imported; they stay in the original file only.".into());
    }
    if contains_part(bytes, "xl/vbaProject.bin") {
        warnings.push("Macros were not loaded. Spreadsheets always open with macros disabled.".into());
    }
    let title = String::new();
    if title.is_empty() {
        workbook.metadata.title = workbook.title.clone();
    }
    Ok(SheetRead { workbook, warnings })
}

pub fn read_workbook_file(path: &Path) -> OfficeResult<SheetRead> {
    let bytes = crate::io::read_bytes(path)?;
    let mut result = read_workbook_bytes(&bytes)?;
    if result.workbook.title == "Imported workbook" || result.workbook.title.is_empty() {
        result.workbook.title = crate::io::file_stem(path);
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// Import: OOXML styles, layout, validation, names and tables
// ---------------------------------------------------------------------------

/// Same row/column budgets as the calamine pass.
const MAX_IMPORT_ROWS: u32 = 100_000;
const MAX_IMPORT_COLS: u32 = 1_000;
/// Worksheet parts bigger than this are skipped by the detailed pass so a
/// hostile file cannot make the XML tree explode; calamine still returns the
/// values. 64 MiB is far above any sheet the 100k-row budget can produce.
const MAX_DETAIL_PART_BYTES: usize = 64 * 1024 * 1024;
const MAX_FILTER_VALUES: usize = 1_024;
/// Cache points read from a ChartML part; well above any readable chart.
const MAX_CHART_CACHE_POINTS: usize = 100_000;
/// Pivot cache fields kept per cache definition.
const MAX_PIVOT_FIELDS: usize = 4_096;

/// The style catalogue resolved from `xl/styles.xml`, indexed by cell xf.
#[derive(Debug, Clone, Default)]
struct ImportedStyles {
    cell_styles: Vec<CellStyle>,
    /// `dxf` fill and font colours, indexed by `dxfId` in conditional rules.
    dxf_fills: Vec<Option<String>>,
    dxf_colors: Vec<Option<String>>,
}

#[derive(Debug, Clone, Default)]
struct ImportFont {
    name: Option<String>,
    size_pt: Option<f64>,
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    color: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct ImportedXf {
    number_format: String,
    font: Option<ImportFont>,
    fill: Option<String>,
    borders: CellBorders,
    align: String,
    valign: String,
    wrap: bool,
    rotation: i32,
}

/// One relationship from a `.rels` part.
struct Relationship {
    kind: String,
    target: String,
}

/// Reads the OOXML parts calamine does not expose and maps them onto the model.
///
/// Returns `true` when the input is an OOXML workbook (so the caller knows the
/// detailed pass applies); every parse failure is recorded as a warning and the
/// calamine values stay untouched.
fn import_ooxml_layout(bytes: &[u8], workbook: &mut Workbook, warnings: &mut Vec<String>) -> bool {
    let Ok(zip) = crate::zip::ZipReader::open(bytes.to_vec()) else { return false };
    if !zip.contains("xl/workbook.xml") {
        return false;
    }
    let info = match import_workbook_sheets(&zip) {
        Ok(parts) => parts,
        Err(error) => {
            warnings.push(format!("The workbook part could not be read ({error}); sheet layout was not imported."));
            return true;
        }
    };
    let WorkbookSheetInfo {
        sheets: sheet_refs,
        names,
        filters: filter_databases,
        print_areas,
        print_titles_rows,
        print_titles_cols,
    } = info;
    let styles = match zip.read_text("xl/styles.xml") {
        Ok(xml) => match parse_styles(&xml) {
            Ok(styles) => styles,
            Err(error) => {
                warnings.push(format!(
                    "The styles of the workbook could not be read ({error}); cell formatting was not imported."
                ));
                ImportedStyles::default()
            }
        },
        Err(error) => {
            warnings.push(format!(
                "The styles of the workbook could not be read ({error}); cell formatting was not imported."
            ));
            ImportedStyles::default()
        }
    };

    let mut table_number = 0usize;
    let mut preserved_pivots: Vec<PreservedPivot> = Vec::new();
    for (index, sheet) in workbook.sheets.iter_mut().enumerate() {
        // Sheet name -> part mapping comes from workbook.xml + its rels, so the
        // worksheet order in the package is never assumed.
        let part = sheet_refs
            .iter()
            .find(|(name, _)| name == &sheet.name)
            .map(|(_, part)| part.clone())
            .or_else(|| sheet_refs.get(index).map(|(_, part)| part.clone()));
        let Some(part) = part else { continue };
        if let Err(error) =
            apply_worksheet_part(&zip, &part, sheet, &styles, &mut table_number, &mut preserved_pivots, warnings)
        {
            warnings.push(format!("The layout of sheet \"{}\" could not be imported ({error}).", sheet.name));
        }
    }
    for entry in names {
        workbook.names.push(entry);
    }
    // A hidden `_xlnm._FilterDatabase` name is a sheet's AutoFilter range.
    for (sheet_name, range) in filter_databases {
        let index = workbook
            .sheets
            .iter()
            .position(|sheet| sheet.name == sheet_name)
            .or_else(|| workbook.sheets.iter().position(|sheet| sheet.name.eq_ignore_ascii_case(&sheet_name)));
        if let Some(index) = index {
            if workbook.sheets[index].filter.is_none() {
                workbook.sheets[index].filter = Some(FilterState { range, column: 0, values: Vec::new() });
            }
        }
    }
    apply_print_names(workbook, print_areas, print_titles_rows, print_titles_cols, warnings);
    if !preserved_pivots.is_empty() {
        warnings.push(format!(
            "{} pivot cache(s) were preserved and will be re-exported, but the pivot grid is not recomputed from the cache; the visible values come from the sheet cells.",
            preserved_pivots.len()
        ));
    }
    workbook.preserved_pivots.extend(preserved_pivots);
    true
}

/// Applies `_xlnm.Print_Area` / `_xlnm.Print_Titles` names to their sheets.
///
/// A sheet can only store one print area and one row/column title span, so a
/// package that carries several ranges keeps the first and reports the rest
/// instead of silently dropping them.
fn apply_print_names(
    workbook: &mut Workbook,
    print_areas: Vec<(String, String)>,
    print_titles_rows: Vec<(String, String)>,
    print_titles_cols: Vec<(String, String)>,
    warnings: &mut Vec<String>,
) {
    for (sheet_name, range) in print_areas {
        let Some(index) = sheet_index_by_name(workbook, &sheet_name) else { continue };
        let area = &mut workbook.sheets[index].print.print_area;
        if area.is_none() {
            *area = Some(range);
        } else {
            warnings.push(format!(
                "Sheet \"{}\" has more than one print area; only the first one was imported.",
                workbook.sheets[index].name
            ));
        }
    }
    for (sheet_name, range) in print_titles_rows {
        let Some(index) = sheet_index_by_name(workbook, &sheet_name) else { continue };
        let rows = &mut workbook.sheets[index].print.print_titles_rows;
        if rows.is_none() {
            *rows = Some(range);
        }
    }
    for (sheet_name, range) in print_titles_cols {
        let Some(index) = sheet_index_by_name(workbook, &sheet_name) else { continue };
        let columns = &mut workbook.sheets[index].print.print_titles_cols;
        if columns.is_none() {
            *columns = Some(range);
        }
    }
}

fn sheet_index_by_name(workbook: &Workbook, name: &str) -> Option<usize> {
    workbook
        .sheets
        .iter()
        .position(|sheet| sheet.name == name)
        .or_else(|| workbook.sheets.iter().position(|sheet| sheet.name.eq_ignore_ascii_case(name)))
}

/// Sheet names, their worksheet parts, the defined names read from
/// `xl/workbook.xml` and its rels, and the `_xlnm` ranges that belong to
/// printing rather than to the user's name list.
struct WorkbookSheetInfo {
    sheets: Vec<(String, String)>,
    names: Vec<NamedRange>,
    filters: Vec<(String, String)>,
    print_areas: Vec<(String, String)>,
    print_titles_rows: Vec<(String, String)>,
    print_titles_cols: Vec<(String, String)>,
}

fn import_workbook_sheets(zip: &crate::zip::ZipReader) -> OfficeResult<WorkbookSheetInfo> {
    let root = parse_xml(&zip.read_text("xl/workbook.xml")?)?;
    let relationships = read_relationships(zip, "xl/workbook.xml");
    let mut sheets: Vec<(String, String)> = Vec::new();
    if let Some(list) = root.child("sheets") {
        for sheet in list.children_of("sheet") {
            let name = sheet.attr("name").unwrap_or("").to_string();
            let part = sheet
                .attr_any_ns("id")
                .and_then(|id| relationships.get(id))
                .map(|relationship| resolve_part("xl/workbook.xml", &relationship.target))
                .unwrap_or_else(|| format!("xl/worksheets/sheet{}.xml", sheets.len() + 1));
            sheets.push((name, part));
        }
    }
    let mut names = Vec::new();
    let mut filters = Vec::new();
    let mut print_areas = Vec::new();
    let mut print_titles_rows = Vec::new();
    let mut print_titles_cols = Vec::new();
    if let Some(defined) = root.child("definedNames") {
        for entry in defined.children_of("definedName") {
            let name = entry.attr("name").unwrap_or("").trim().to_string();
            if name.is_empty() {
                continue;
            }
            let definition = entry.deep_text().trim().trim_start_matches('=').to_string();
            if definition.is_empty() {
                continue;
            }
            let scope = entry
                .attr("localSheetId")
                .and_then(|value| value.trim().parse::<usize>().ok())
                .and_then(|index| sheets.get(index))
                .map(|(sheet_name, _)| sheet_name.clone());
            if name == "_xlnm._FilterDatabase" {
                let from_definition = split_sheet_reference(&definition);
                let (sheet_name, range) = match from_definition {
                    Some((sheet_name, range)) => (sheet_name, range),
                    None => match scope {
                        Some(sheet_name) => (sheet_name, definition.clone()),
                        None => continue,
                    },
                };
                filters.push((sheet_name, range));
                continue;
            }
            if name == "_xlnm.Print_Area" {
                for part in definition.split(',') {
                    let (sheet_name, range) = match split_sheet_reference(part) {
                        Some(pair) => pair,
                        None => match scope.clone() {
                            Some(sheet_name) => (sheet_name, strip_absolute_marks(part)),
                            None => continue,
                        },
                    };
                    if !range.is_empty() {
                        print_areas.push((sheet_name, range));
                    }
                }
                continue;
            }
            if name == "_xlnm.Print_Titles" {
                for part in definition.split(',') {
                    let (sheet_name, range) = match split_sheet_reference(part) {
                        Some(pair) => pair,
                        None => match scope.clone() {
                            Some(sheet_name) => (sheet_name, strip_absolute_marks(part)),
                            None => continue,
                        },
                    };
                    if range.is_empty() {
                        continue;
                    }
                    // `$1:$3` prints rows, `$A:$B` prints columns.
                    if range.chars().next().map(|character| character.is_ascii_digit()).unwrap_or(false) {
                        print_titles_rows.push((sheet_name, range));
                    } else {
                        print_titles_cols.push((sheet_name, range));
                    }
                }
                continue;
            }
            // Other `_xlnm` internals are not user names.
            if name.starts_with("_xlnm.") {
                continue;
            }
            names.push(NamedRange { name, definition, sheet: scope, comment: String::new() });
        }
    }
    Ok(WorkbookSheetInfo { sheets, names, filters, print_areas, print_titles_rows, print_titles_cols })
}

/// `$A$1:$D$10` to `A1:D10` (the model stores relative references).
fn strip_absolute_marks(range: &str) -> String {
    range.trim().replace('$', "")
}

fn apply_worksheet_part(
    zip: &crate::zip::ZipReader,
    part: &str,
    sheet: &mut Sheet,
    styles: &ImportedStyles,
    table_number: &mut usize,
    preserved_pivots: &mut Vec<PreservedPivot>,
    warnings: &mut Vec<String>,
) -> OfficeResult<()> {
    let text = zip.read_text(part)?;
    if text.len() > MAX_DETAIL_PART_BYTES {
        warnings.push(format!(
            "Sheet \"{}\" is too large for the layout pass; values and formulas were imported and its formatting was skipped.",
            sheet.name
        ));
        return Ok(());
    }
    let root = parse_xml(&text)?;

    if let Some(tab_color) = root.child("sheetPr").and_then(|sheet_pr| sheet_pr.child("tabColor")).and_then(color_of) {
        sheet.tab_color = Some(tab_color);
    }
    apply_columns(&root, sheet);
    apply_sheet_cells(&root, sheet, styles);
    apply_merges(&root, sheet);
    apply_freeze_panes(&root, sheet);
    apply_sheet_filter(&root, sheet);
    apply_validations(&root, sheet, warnings);
    apply_conditional(&root, sheet, styles, warnings);
    apply_hyperlinks(zip, part, &root, sheet);
    apply_comments(zip, part, sheet, warnings);
    apply_tables(zip, part, &root, sheet, table_number, warnings);
    apply_print_settings(&root, sheet);
    apply_sheet_protection(&root, sheet);
    apply_drawings(zip, part, &root, sheet, warnings);
    apply_pivot_tables(zip, part, sheet, preserved_pivots, warnings);
    Ok(())
}

// ---------------------------------------------------------------------------
// Import: print settings and sheet protection
// ---------------------------------------------------------------------------

/// `<pageSetup>`, `<printOptions>`, `<pageMargins>`, `<headerFooter>` and the
/// manual page breaks, mapped onto [`PrintSettings`].
fn apply_print_settings(root: &XmlNode, sheet: &mut Sheet) {
    let print = &mut sheet.print;
    if let Some(setup) = root.child("pageSetup") {
        if let Some(value) = parse_u32_attr(setup, "paperSize") {
            print.paper_size = value;
        }
        if let Some(value) = setup.attr("orientation") {
            print.landscape = value.trim().eq_ignore_ascii_case("landscape");
        }
        if let Some(value) = parse_u32_attr(setup, "scale") {
            print.scale = value.clamp(10, 400);
        }
        if let Some(value) = parse_u32_attr(setup, "fitToWidth") {
            print.fit_to_width = value;
        }
        if let Some(value) = parse_u32_attr(setup, "fitToHeight") {
            print.fit_to_height = value;
        }
    }
    if let Some(options) = root.child("printOptions") {
        print.center_horizontally = attr_on(options, "horizontalCentered");
        print.center_vertically = attr_on(options, "verticalCentered");
        print.print_gridlines = attr_on(options, "gridLines");
        print.print_headings = attr_on(options, "headings");
    }
    if let Some(margins) = root.child("pageMargins") {
        let number = |name: &str, target: &mut f64| {
            if let Some(value) = margins.attr(name).and_then(|value| value.trim().parse::<f64>().ok()) {
                *target = value;
            }
        };
        number("left", &mut print.margin_left);
        number("right", &mut print.margin_right);
        number("top", &mut print.margin_top);
        number("bottom", &mut print.margin_bottom);
        number("header", &mut print.margin_header);
        number("footer", &mut print.margin_footer);
    }
    if let Some(header) = root.child("headerFooter") {
        print.different_first_page = attr_on(header, "differentFirst");
        print.different_odd_even = attr_on(header, "differentOddEven");
        print.header = header_format_text(header, "oddHeader");
        print.footer = header_format_text(header, "oddFooter");
        print.first_header = header_format_text(header, "firstHeader");
        print.first_footer = header_format_text(header, "firstFooter");
        print.even_header = header_format_text(header, "evenHeader");
        print.even_footer = header_format_text(header, "evenFooter");
    }
    for breaks in root.children_of("rowBreaks") {
        for brk in breaks.children_of("brk") {
            if let Some(id) = parse_u32_attr(brk, "id") {
                if id <= MAX_IMPORT_ROWS {
                    sheet.print.row_breaks.push(id);
                }
            }
        }
    }
    for breaks in root.children_of("colBreaks") {
        for brk in breaks.children_of("brk") {
            if let Some(id) = parse_u32_attr(brk, "id") {
                if id <= MAX_IMPORT_COLS {
                    sheet.print.col_breaks.push(id);
                }
            }
        }
    }
    sheet.print.row_breaks.sort_unstable();
    sheet.print.row_breaks.dedup();
    sheet.print.col_breaks.sort_unstable();
    sheet.print.col_breaks.dedup();
}

/// One header/footer format string. The model stores plain text; the `&C`
/// prefix the exporter adds means "centred", so exactly one is stripped.
fn header_format_text(header: &XmlNode, tag: &str) -> String {
    header
        .child(tag)
        .map(XmlNode::deep_text)
        .map(|text| text.strip_prefix("&C").unwrap_or(&text).to_string())
        .unwrap_or_default()
}

/// `<sheetProtection>`: the verifier is preserved exactly, never cracked.
fn apply_sheet_protection(root: &XmlNode, sheet: &mut Sheet) {
    let Some(node) = root.child("sheetProtection") else { return };
    let mut options: Vec<String> =
        SHEET_PROTECTION_OPTIONS.iter().filter(|name| attr_on(node, name)).map(|name| (*name).to_string()).collect();
    options.sort();
    let password = node.attr("password").map(str::trim).filter(|value| !value.is_empty()).map(str::to_string);
    let enabled = attr_on(node, "sheet")
        || password.is_some()
        || node.attr("hashValue").is_some()
        || node.attr("algorithmName").is_some();
    sheet.protection = SheetProtection {
        enabled,
        password_hash: password.clone(),
        algorithm_name: node.attr("algorithmName").unwrap_or("").to_string(),
        hash_value: node.attr("hashValue").unwrap_or("").to_string(),
        salt_value: node.attr("saltValue").unwrap_or("").to_string(),
        spin_count: parse_u32_attr(node, "spinCount").unwrap_or(0),
        options,
    };
    // Keep the legacy single-hash field in sync for older units and the UI.
    if let Some(password) = password {
        sheet.sheet_protection = password;
    }
}

/// `<cols>` widths: Excel character width to pixels (`width * 7 + 5`).
fn apply_columns(root: &XmlNode, sheet: &mut Sheet) {
    let Some(cols) = root.child("cols") else { return };
    for col in cols.children_of("col") {
        let Some(width) = col.attr("width").and_then(|value| value.trim().parse::<f64>().ok()) else { continue };
        let min = parse_u32_attr(col, "min").unwrap_or(1).saturating_sub(1);
        let max = parse_u32_attr(col, "max").unwrap_or(min + 1).saturating_sub(1).max(min);
        if min > MAX_IMPORT_COLS {
            continue;
        }
        let pixels = (width * 7.0 + 5.0).max(24.0);
        for column in min..=max.min(MAX_IMPORT_COLS) {
            sheet.col_widths.insert(column, pixels);
        }
    }
}

/// `<sheetData>` row heights (points -> pixels) and per-cell styles. Values and
/// formulas are not touched: they came from the calamine pass.
fn apply_sheet_cells(root: &XmlNode, sheet: &mut Sheet, styles: &ImportedStyles) {
    let Some(data) = root.child("sheetData") else { return };
    let styles_available = !styles.cell_styles.is_empty();
    for row in data.children_of("row") {
        let Some(row_number) = parse_u32_attr(row, "r").map(|value| value.saturating_sub(1)) else { continue };
        if row_number > MAX_IMPORT_ROWS {
            continue;
        }
        if let Some(height) = row.attr("ht").and_then(|value| value.trim().parse::<f64>().ok()) {
            sheet.row_heights.insert(row_number, height * PT_TO_PX);
        }
        if !styles_available {
            continue;
        }
        for cell in row.children_of("c") {
            let Some((cell_row, cell_column)) = cell.attr("r").and_then(crate::address::parse) else { continue };
            if cell_row > MAX_IMPORT_ROWS || cell_column > MAX_IMPORT_COLS {
                continue;
            }
            let Some(style_index) = parse_u32_attr(cell, "s").map(|value| value as usize) else { continue };
            let Some(style) = styles.cell_styles.get(style_index) else { continue };
            let address = crate::address::format(cell_row, cell_column);
            match sheet.cells.get_mut(&address) {
                Some(existing) => existing.style = style.clone(),
                // A style-only cell is only materialised when it carries
                // something visible; a plain "General" xf must not turn every
                // empty row into a million model cells.
                None if style_worth_a_cell(style) => {
                    sheet.cells.insert(address, Cell { style: style.clone(), ..Default::default() });
                }
                None => {}
            }
        }
    }
}

fn apply_merges(root: &XmlNode, sheet: &mut Sheet) {
    let Some(merges) = root.child("mergeCells") else { return };
    for merge in merges.children_of("mergeCell") {
        let Some(reference) = merge.attr("ref") else { continue };
        let Some(((start_row, start_col), (end_row, end_col))) = crate::address::parse_range(reference) else {
            continue;
        };
        if start_row > MAX_IMPORT_ROWS || start_col > MAX_IMPORT_COLS {
            continue;
        }
        sheet.merges.push(MergeRange {
            start: crate::address::format(start_row, start_col),
            end: crate::address::format(end_row, end_col),
        });
    }
}

fn apply_freeze_panes(root: &XmlNode, sheet: &mut Sheet) {
    let Some(pane) =
        root.child("sheetViews").and_then(|views| views.child("sheetView")).and_then(|view| view.child("pane"))
    else {
        return;
    };
    if !matches!(pane.attr("state").map(str::trim), Some("frozen") | Some("frozenSplit")) {
        return;
    }
    let x_split = pane.attr("xSplit").and_then(|value| value.trim().parse::<f64>().ok()).unwrap_or(0.0);
    let y_split = pane.attr("ySplit").and_then(|value| value.trim().parse::<f64>().ok()).unwrap_or(0.0);
    if x_split > 0.0 || y_split > 0.0 {
        sheet.freeze_cols = x_split.max(0.0).round() as u32;
        sheet.freeze_rows = y_split.max(0.0).round() as u32;
    } else if let Some((row, column)) = pane.attr("topLeftCell").and_then(crate::address::parse) {
        sheet.freeze_rows = row;
        sheet.freeze_cols = column;
    }
}

fn parse_filter_state(node: &XmlNode, fallback_range: &str) -> Option<FilterState> {
    let range = node.attr("ref").unwrap_or("").trim();
    let range = if range.is_empty() { fallback_range.trim() } else { range };
    if range.is_empty() {
        return None;
    }
    let mut state = FilterState { range: range.to_string(), column: 0, values: Vec::new() };
    if let Some(column) = node.child("filterColumn") {
        state.column = parse_u32_attr(column, "colId").unwrap_or(0);
        if let Some(filters) = column.child("filters") {
            for entry in filters.children_of("filter") {
                if state.values.len() >= MAX_FILTER_VALUES {
                    break;
                }
                if let Some(value) = entry.attr("val") {
                    state.values.push(value.to_string());
                }
            }
        }
    }
    Some(state)
}

fn apply_sheet_filter(root: &XmlNode, sheet: &mut Sheet) {
    let Some(node) = root.child("autoFilter") else { return };
    if let Some(state) = parse_filter_state(node, "") {
        sheet.filter = Some(state);
    }
}

fn apply_validations(root: &XmlNode, sheet: &mut Sheet, warnings: &mut Vec<String>) {
    let Some(block) = root.child("dataValidations") else { return };
    let mut unsupported = 0usize;
    let mut range_lists = 0usize;
    for node in block.children_of("dataValidation") {
        let range = node.attr("sqref").unwrap_or("").trim().to_string();
        if range.is_empty() {
            continue;
        }
        let kind = node.attr("type").unwrap_or("none").trim();
        let allow_blank = attr_on(node, "allowBlank");
        let message = node.attr("error").or_else(|| node.attr("prompt")).unwrap_or("").to_string();
        let formula1 = node.child("formula1").map(XmlNode::deep_text).unwrap_or_default();
        let formula2 = node.child("formula2").map(XmlNode::deep_text).unwrap_or_default();
        let validation = match kind {
            "list" => {
                let mut values = parse_list_values(&formula1);
                if values.is_empty() && is_list_reference(formula1.trim()) {
                    // The editor resolves a single `=reference` entry from the cells.
                    values = vec![format!("={}", formula1.trim())];
                } else if values.is_empty() && !formula1.trim().is_empty() {
                    range_lists += 1;
                }
                Validation {
                    id: format!("v{}", sheet.validations.len() + 1),
                    range,
                    kind: "list".into(),
                    values,
                    min: None,
                    max: None,
                    message,
                    allow_blank,
                }
            }
            "decimal" | "whole" => {
                let (min, max) = numeric_bounds(node.attr("operator").unwrap_or("between"), &formula1, &formula2);
                Validation {
                    id: format!("v{}", sheet.validations.len() + 1),
                    range,
                    kind: "number".into(),
                    values: Vec::new(),
                    min,
                    max,
                    message,
                    allow_blank,
                }
            }
            _ => {
                unsupported += 1;
                continue;
            }
        };
        sheet.validations.push(validation);
    }
    if unsupported > 0 {
        warnings.push(format!(
            "{unsupported} data validation rule(s) on sheet \"{}\" use types this editor cannot import and were dropped.",
            sheet.name
        ));
    }
    if range_lists > 0 {
        warnings.push(format!(
            "{range_lists} list validation(s) on sheet \"{}\" take their values from a formula or named range, which is not imported; the rule is kept without its list.",
            sheet.name
        ));
    }
}

/// The bare reference of a list rule whose only entry is `=A1:A5` or
/// `='My Sheet'!$A$1:$A$9`, or None for an inline list.
fn list_reference(values: &[String]) -> Option<String> {
    let [only] = values else { return None };
    let reference = only.trim().strip_prefix('=')?.trim();
    is_list_reference(reference).then(|| reference.to_string())
}

/// `A1`, `$A$1:$B$9`, `Sheet!A1:A9` or `'My Sheet'!A1:A9`; formulas, named
/// ranges and whole columns are not.
fn is_list_reference(text: &str) -> bool {
    let range = match text.rfind('!') {
        Some(bang) => {
            let sheet = &text[..bang];
            let quoted = sheet.len() >= 2 && sheet.starts_with('\'') && sheet.ends_with('\'');
            let plain = !sheet.is_empty()
                && sheet.chars().all(|character| character.is_alphanumeric() || matches!(character, '_' | '.'));
            if !quoted && !plain {
                return false;
            }
            &text[bang + 1..]
        }
        None => text,
    };
    let cell = |part: &str| {
        let bare = part.replace('$', "");
        !bare.is_empty()
            && bare.chars().all(|character| character.is_ascii_alphanumeric())
            && crate::address::parse(&bare).is_some()
    };
    let mut parts = range.split(':');
    let first = parts.next().is_some_and(cell);
    let second = parts.next().is_none_or(cell);
    first && second && parts.next().is_none()
}

fn parse_list_values(formula: &str) -> Vec<String> {
    let text = formula.trim();
    let Some(inner) = text.strip_prefix('"').and_then(|value| value.strip_suffix('"')) else { return Vec::new() };
    if inner.is_empty() {
        return Vec::new();
    }
    inner
        .split(',')
        .map(|value| value.trim().replace("\"\"", "\""))
        .filter(|value| !value.is_empty())
        .take(MAX_FILTER_VALUES)
        .collect()
}

fn numeric_bounds(operator: &str, first: &str, second: &str) -> (Option<f64>, Option<f64>) {
    let number = |text: &str| text.trim().parse::<f64>().ok();
    match operator {
        "greaterThan" | "greaterThanOrEqual" => (number(first), None),
        "lessThan" | "lessThanOrEqual" => (None, number(first)),
        "equal" => {
            let value = number(first);
            (value, value)
        }
        _ => (number(first), number(second)),
    }
}

fn apply_conditional(root: &XmlNode, sheet: &mut Sheet, styles: &ImportedStyles, warnings: &mut Vec<String>) {
    let mut unsupported = 0usize;
    for block in root.children_of("conditionalFormatting") {
        let range = block.attr("sqref").unwrap_or("").trim().to_string();
        if range.is_empty() {
            continue;
        }
        for rule in block.children_of("cfRule") {
            let kind = rule.attr("type").unwrap_or("").trim();
            let stop_if_true = attr_on(rule, "stopIfTrue");
            let dxf = parse_u32_attr(rule, "dxfId").map(|value| value as usize);
            let mut fill = dxf.and_then(|index| styles.dxf_fills.get(index).cloned()).flatten();
            let color = dxf.and_then(|index| styles.dxf_colors.get(index).cloned()).flatten();
            let formulas = rule.children_of("formula");
            let formula =
                |index: usize| formulas.get(index).map(|node| node.deep_text().trim().to_string()).unwrap_or_default();
            let (model_kind, values, top_n) = match kind {
                "cellIs" => match rule.attr("operator").unwrap_or("") {
                    "greaterThan" => ("greater", vec![strip_operator(&formula(0))], None),
                    "lessThan" => ("less", vec![strip_operator(&formula(0))], None),
                    "equal" => ("equal", vec![strip_operator(&formula(0))], None),
                    "between" => {
                        let first = formula(0);
                        let second = formula(1);
                        // This writer encodes `between` as one formula with a
                        // `~` separator; Excel uses two formula elements.
                        let (low, high) = match first.split_once('~') {
                            Some((low, high)) => (low.to_string(), high.to_string()),
                            None => (first, second),
                        };
                        ("between", vec![strip_operator(&low), strip_operator(&high)], None)
                    }
                    _ => {
                        unsupported += 1;
                        continue;
                    }
                },
                "containsText" => {
                    let text =
                        rule.attr("text").map(str::to_string).unwrap_or_else(|| contains_text_value(&formula(0)));
                    ("textContains", vec![text], None)
                }
                "duplicateValues" => ("duplicate", Vec::new(), None),
                "top10" => {
                    let rank = parse_u32_attr(rule, "rank").unwrap_or(10);
                    if attr_on(rule, "bottom") {
                        ("bottom", Vec::new(), Some(rank))
                    } else {
                        ("top", Vec::new(), Some(rank))
                    }
                }
                "dataBar" => {
                    if let Some(bar_color) = rule.child("dataBar").and_then(|bar| bar.child("color")).and_then(color_of)
                    {
                        fill = Some(bar_color);
                    }
                    ("dataBar", Vec::new(), None)
                }
                _ => {
                    unsupported += 1;
                    continue;
                }
            };
            sheet.conditional.push(CondRule {
                id: format!("cf{}", sheet.conditional.len() + 1),
                range: range.clone(),
                kind: model_kind.into(),
                values,
                fill,
                color,
                top_n,
                stop_if_true,
            });
        }
    }
    if unsupported > 0 {
        warnings.push(format!(
            "{unsupported} conditional formatting rule(s) on sheet \"{}\" use types this editor cannot import and were dropped.",
            sheet.name
        ));
    }
}

/// `>10`, `<=5` and the like become the bare value the editor stores.
fn strip_operator(formula: &str) -> String {
    formula.trim().trim_start_matches(['>', '<', '=']).trim().to_string()
}

fn contains_text_value(formula: &str) -> String {
    const SEARCH: &str = "SEARCH(\"";
    if let Some(start) = formula.find(SEARCH) {
        let rest = &formula[start + SEARCH.len()..];
        if let Some(end) = rest.find('"') {
            return rest[..end].to_string();
        }
    }
    String::new()
}

fn apply_hyperlinks(zip: &crate::zip::ZipReader, part: &str, root: &XmlNode, sheet: &mut Sheet) {
    let Some(links) = root.child("hyperlinks") else { return };
    let relationships = read_relationships(zip, part);
    for link in links.children_of("hyperlink") {
        let Some((row, column)) = link.attr("ref").and_then(crate::address::parse) else { continue };
        let address = crate::address::format(row, column);
        // Only external targets have a relationship; internal `location` links
        // cannot be expressed as a cell link in the model.
        let Some(target) = link
            .attr_any_ns("id")
            .and_then(|id| relationships.get(id))
            .filter(|relationship| relationship.kind.ends_with("/hyperlink"))
            .map(|relationship| relationship.target.clone())
        else {
            continue;
        };
        match sheet.cells.get_mut(&address) {
            Some(cell) => cell.link = Some(target),
            None => {
                // `Cell::is_empty` ignores links, so `Sheet::set` would drop
                // this cell; insert it directly instead.
                sheet.cells.insert(address, Cell { link: Some(target), ..Default::default() });
            }
        }
    }
}

fn apply_comments(zip: &crate::zip::ZipReader, part: &str, sheet: &mut Sheet, warnings: &mut Vec<String>) {
    let relationships = read_relationships(zip, part);
    let Some(target) = relationships
        .values()
        .find(|relationship| relationship.kind.ends_with("/comments"))
        .map(|relationship| resolve_part(part, &relationship.target))
    else {
        return;
    };
    let parsed = zip.read_text(&target).and_then(|xml| parse_xml(&xml));
    let root = match parsed {
        Ok(root) => root,
        Err(_) => {
            warnings.push(format!("The comments of sheet \"{}\" could not be read.", sheet.name));
            return;
        }
    };
    let Some(list) = root.child("commentList") else { return };
    for comment in list.children_of("comment") {
        let Some((row, column)) = comment.attr("ref").and_then(crate::address::parse) else { continue };
        let text = comment.child("text").map(XmlNode::deep_text).unwrap_or_default().trim().to_string();
        if text.is_empty() {
            continue;
        }
        let address = crate::address::format(row, column);
        match sheet.cells.get_mut(&address) {
            Some(cell) => cell.comment = Some(text),
            None => {
                sheet.cells.insert(address, Cell { comment: Some(text), ..Default::default() });
            }
        }
    }
}

fn apply_tables(
    zip: &crate::zip::ZipReader,
    part: &str,
    root: &XmlNode,
    sheet: &mut Sheet,
    table_number: &mut usize,
    warnings: &mut Vec<String>,
) {
    let Some(table_parts) = root.child("tableParts") else { return };
    let relationships = read_relationships(zip, part);
    for table_part in table_parts.children_of("tablePart") {
        let Some(relationship) = table_part.attr_any_ns("id").and_then(|id| relationships.get(id)) else { continue };
        if !relationship.kind.ends_with("/table") {
            continue;
        }
        let target = resolve_part(part, &relationship.target);
        *table_number += 1;
        match parse_table_part(zip, &target, *table_number, sheet) {
            Ok(table) => sheet.tables.push(table),
            Err(error) => warnings
                .push(format!("A structured table in sheet \"{}\" could not be imported ({error}).", sheet.name)),
        }
    }
}

fn parse_table_part(
    zip: &crate::zip::ZipReader,
    target: &str,
    number: usize,
    sheet: &Sheet,
) -> OfficeResult<SpreadsheetTable> {
    let root = parse_xml(&zip.read_text(target)?)?;
    let mut table = SpreadsheetTable { id: format!("table{number}"), ..Default::default() };
    table.name = root.attr("displayName").or_else(|| root.attr("name")).unwrap_or("").trim().to_string();
    if table.name.is_empty() {
        table.name = format!("Table{number}");
    }
    table.range = root.attr("ref").unwrap_or("").trim().to_string();
    if table.range.is_empty() {
        return Err(OfficeError::corrupt("the table range is missing"));
    }
    table.has_headers = root.attr("headerRowCount").map(|value| value.trim() != "0").unwrap_or(true);
    table.has_totals = parse_u32_attr(&root, "totalsRowCount").map(|count| count > 0).unwrap_or(false);
    if let Some(columns) = root.child("tableColumns") {
        for column in columns.children_of("tableColumn") {
            let name = column.attr("name").unwrap_or("").to_string();
            let formula = column
                .child("calculatedColumnFormula")
                .map(|node| node.deep_text().trim().trim_start_matches('=').to_string())
                .filter(|text| !text.is_empty())
                .map(|text| format!("={text}"));
            table.columns.push(TableColumn { name, formula });
        }
    }
    if let Some(style) = root.child("tableStyleInfo") {
        table.style_name = style.attr("name").unwrap_or("").to_string();
        table.banded_rows =
            style.attr("showRowStripes").map(|value| matches!(value.trim(), "1" | "true")).unwrap_or(true);
        table.banded_columns =
            style.attr("showColumnStripes").map(|value| matches!(value.trim(), "1" | "true")).unwrap_or(false);
    }
    if let Some(filter) = root.child("autoFilter") {
        table.filter = parse_filter_state(filter, &table.range);
    }
    // Header styling is inferred from the cells the styles pass just decorated.
    if table.has_headers {
        if let Some(((row, column), _)) = crate::address::parse_range(&table.range) {
            if let Some(cell) = sheet.get(&crate::address::format(row, column)) {
                table.header_fill = cell.style.fill.clone();
                table.header_bold = cell.style.bold;
            }
        }
    }
    Ok(table)
}

// ---------------------------------------------------------------------------
// Import: drawings (charts and pictures)
// ---------------------------------------------------------------------------

/// First descendant with a matching local name. `XmlNode::find_all` also
/// matches the local name, which is what chart/drawing XML needs.
fn descendant<'a>(node: &'a XmlNode, name: &str) -> Option<&'a XmlNode> {
    let mut found: Vec<&XmlNode> = Vec::new();
    node.find_all(name, &mut found);
    found.into_iter().next()
}

/// The text of a `c:title`/`c:tx` block: rich text, a cached string reference
/// or a literal `c:v`, whichever the writer used.
fn chart_text_block(node: &XmlNode) -> String {
    if let Some(value) = node.child("v") {
        return value.deep_text();
    }
    if let Some(reference) = node.child("strRef") {
        if let Some(value) = reference.child("v") {
            return value.deep_text();
        }
        if let Some(value) = descendant(reference, "strCache")
            .and_then(|cache| cache.children_of("pt").first().and_then(|point| point.child("v")))
            .map(XmlNode::deep_text)
        {
            if !value.is_empty() {
                return value;
            }
        }
        return reference.child("f").map(XmlNode::deep_text).unwrap_or_default();
    }
    if let Some(rich) = node.child("rich") {
        let mut out = String::new();
        for paragraph in rich.children_of("p") {
            if let Some(value) = paragraph.child("t") {
                out.push_str(&value.deep_text());
            }
            for run in paragraph.children_of("r") {
                if let Some(value) = run.child("t") {
                    out.push_str(&value.deep_text());
                }
            }
        }
        if !out.is_empty() {
            return out;
        }
    }
    node.deep_text()
}

/// `'My Sheet'!$A$1:$B$5` to the model's relative `A1:B5`.
fn relative_ref(formula: &str) -> String {
    let text = formula.trim();
    let text = match text.rfind('!') {
        Some(index) => &text[index + 1..],
        None => text,
    };
    text.trim().replace('$', "")
}

/// Cached `c:strCache`/`c:numCache` points as text, ordered by `c:pt/@idx`.
fn cache_text_values(reference: &XmlNode) -> Vec<String> {
    let Some(cache) = descendant(reference, "strCache").or_else(|| descendant(reference, "numCache")) else {
        return Vec::new();
    };
    let mut points: Vec<(u32, String)> = Vec::new();
    for point in cache.children_of("pt") {
        let index = parse_u32_attr(point, "idx").unwrap_or(points.len() as u32);
        let value = point.child("v").map(XmlNode::deep_text).unwrap_or_default();
        points.push((index, value));
    }
    points.sort_by_key(|(index, _)| *index);
    points.into_iter().map(|(_, value)| value).take(MAX_CHART_CACHE_POINTS).collect()
}

/// Cached `c:numCache` points as numbers, ordered by `c:pt/@idx`.
fn cache_number_values(reference: &XmlNode) -> Vec<f64> {
    let Some(cache) = descendant(reference, "numCache") else { return Vec::new() };
    let mut points: Vec<(u32, f64)> = Vec::new();
    for point in cache.children_of("pt") {
        let index = parse_u32_attr(point, "idx").unwrap_or(points.len() as u32);
        let value = point.child("v").and_then(|value| value.deep_text().trim().parse::<f64>().ok());
        if let Some(value) = value {
            points.push((index, value));
        }
    }
    points.sort_by_key(|(index, _)| *index);
    points.into_iter().map(|(_, value)| value).take(MAX_CHART_CACHE_POINTS).collect()
}

/// Reads one ChartML part back into the model. This is the inverse of
/// [`chart_xml`]: the exporter writes one cache-free `c:ser` per series with a
/// literal `c:tx`, so the round trip through our own files is exact.
fn read_xlsx_chart(xml: &str, warnings: &mut Vec<String>) -> Option<ChartData> {
    let root = parse_xml(xml).ok()?;
    let plot = descendant(&root, "plotArea")?;
    let plot_child = plot.children.iter().find(|child| child.local_name().ends_with("Chart"))?;
    let kind = match plot_child.local_name() {
        "barChart" => {
            if descendant(plot_child, "barDir").and_then(|node| node.attr("val")) == Some("bar") {
                "bar".to_string()
            } else {
                "column".to_string()
            }
        }
        "lineChart" => "line".to_string(),
        "pieChart" => "pie".to_string(),
        "areaChart" => "area".to_string(),
        "scatterChart" => "scatter".to_string(),
        "doughnutChart" => "doughnut".to_string(),
        other => {
            let raw = other.trim_end_matches("Chart").to_ascii_lowercase();
            if !raw.is_empty() {
                warnings.push(format!(
                    "A \"{raw}\" chart was imported with limited support; it is kept in the native .oswk file."
                ));
            }
            raw
        }
    };
    let chart_node = root.child("chart");
    let title = chart_node.and_then(|node| node.child("title")).map(chart_text_block).unwrap_or_default();
    let legend_node = chart_node.and_then(|node| node.child("legend"));
    let legend = legend_node.is_some();
    // The model stores only whether a legend exists; a foreign position cannot
    // be kept, and the exporter always writes a bottom legend.
    let legend_position = legend_node
        .and_then(|node| descendant(node, "legendPos"))
        .and_then(|node| node.attr("val"))
        .map(str::to_string);
    if let Some(position) = legend_position.as_deref().filter(|position| !position.is_empty() && *position != "b") {
        warnings.push(format!(
            "An imported chart has its legend at \"{position}\"; the editor renders charts with a bottom legend."
        ));
    }
    let scatter = kind == "scatter";
    let mut series = Vec::new();
    let mut categories = String::new();
    let mut categories_cache = Vec::new();
    let mut series_values_cache = Vec::new();
    for ser in plot_child.children_named("ser") {
        let name = ser.child("tx").map(chart_text_block).unwrap_or_default();
        // A scatter series carries its X values in `xVal` and its Y values in
        // `yVal`; every other kind uses `cat` and `val`.
        let (value_node, category_node) =
            if scatter { (ser.child("yVal"), ser.child("xVal")) } else { (ser.child("val"), ser.child("cat")) };
        let range = value_node
            .and_then(|val| descendant(val, "f"))
            .map(XmlNode::deep_text)
            .map(|value| relative_ref(&value))
            .unwrap_or_default();
        // A markers-only scatter series puts its colour on the marker, not on
        // the (hidden) line.
        let colored = |props: Option<&XmlNode>| {
            props
                .and_then(|props| descendant(props, "srgbClr"))
                .and_then(|color| color.attr("val"))
                .map(|value| format!("#{}", value.trim_start_matches('#').to_ascii_uppercase()))
        };
        let color = if scatter {
            colored(ser.child("spPr")).or_else(|| colored(ser.child("marker").and_then(|marker| marker.child("spPr"))))
        } else {
            colored(descendant(ser, "spPr"))
        };
        if let Some(cat) = category_node {
            let reference =
                descendant(cat, "f").map(XmlNode::deep_text).map(|value| relative_ref(&value)).unwrap_or_default();
            if categories.is_empty() {
                categories = reference;
                categories_cache = cache_text_values(cat);
            } else if scatter && !reference.is_empty() && reference != categories {
                // The model has one X range per chart; a series with its own X
                // values keeps the first range and reports the difference.
                warnings.push(format!(
                    "Scatter series \"{name}\" has its own X values ({reference}); the chart uses {categories} for every series."
                ));
            }
        }
        series_values_cache.push(value_node.map(cache_number_values).unwrap_or_default());
        series.push(ChartSeries { name, range, color });
    }
    let stacked = descendant(plot_child, "grouping")
        .and_then(|node| node.attr("val"))
        .map(|value| value == "stacked")
        .unwrap_or(false);
    // Empty caches are dropped so an export/import cycle of cache-free charts
    // stays byte-identical; a chart with at least one cache keeps the aligned
    // per-series vectors the model documents.
    if series_values_cache.iter().all(Vec::is_empty) {
        series_values_cache.clear();
    }
    let show_labels = descendant(&root, "dLbls")
        .and_then(|labels| descendant(labels, "showVal"))
        .and_then(|node| node.attr("val"))
        .map(|value| value == "1")
        .unwrap_or(false);
    let axis_title =
        |axis: Option<&XmlNode>| axis.and_then(|axis| axis.child("title")).map(chart_text_block).unwrap_or_default();
    // A scatter chart has two value axes: the one along the bottom (or top) is
    // X, the other is Y.
    let (x_title, y_title) = if scatter {
        let axes = plot.children_of("valAx");
        let horizontal =
            |axis: &XmlNode| matches!(axis.child("axPos").and_then(|node| node.attr("val")), Some("b") | Some("t"));
        let x_index = axes.iter().position(|axis| horizontal(axis)).unwrap_or(0);
        let y_index = usize::from(x_index == 0);
        (axis_title(axes.get(x_index).copied()), axis_title(axes.get(y_index).copied()))
    } else {
        (axis_title(descendant(plot, "catAx")), axis_title(descendant(plot, "valAx")))
    };
    let hole_size = if kind == "doughnut" {
        descendant(plot_child, "holeSize")
            .and_then(|node| parse_u32_attr(node, "val"))
            .map(|hole| hole.clamp(10, 90))
            .filter(|hole| *hole != DOUGHNUT_DEFAULT_HOLE)
    } else {
        None
    };
    let scatter_style = if scatter { read_scatter_style(plot_child) } else { None };
    Some(ChartData {
        kind,
        title,
        categories,
        series,
        legend,
        x_title,
        y_title,
        stacked,
        show_labels,
        categories_cache,
        series_values_cache,
        hole_size,
        scatter_style,
    })
}

/// The scatter flavour of a `c:scatterChart`, in the model's spelling. Excel
/// writes `lineMarker` for plain scatter too, so the look comes from what the
/// series draw: a hidden line (`a:ln/a:noFill`) or a `none` marker symbol.
/// Markers only is the default and reads as `None`.
fn read_scatter_style(plot_child: &XmlNode) -> Option<String> {
    let declared = plot_child.child("scatterStyle").and_then(|node| node.attr("val")).unwrap_or("lineMarker");
    let series = plot_child.children_named("ser").collect::<Vec<_>>();
    let line_hidden = |ser: &XmlNode| {
        ser.child("spPr")
            .and_then(|props| props.child("ln"))
            .map(|line| line.child("noFill").is_some())
            .unwrap_or(false)
    };
    let marker_hidden = |ser: &XmlNode| {
        ser.child("marker").and_then(|marker| marker.child("symbol")).and_then(|symbol| symbol.attr("val"))
            == Some("none")
    };
    // `c:smooth` without a value means on (CT_Boolean defaults to true).
    let smooth = declared.starts_with("smooth")
        || series
            .iter()
            .filter_map(|ser| ser.child("smooth"))
            .any(|node| node.attr("val").map(|value| matches!(value.trim(), "1" | "true")).unwrap_or(true));
    let lines = !series.iter().all(|ser| line_hidden(ser)) && !matches!(declared, "marker" | "none");
    let markers =
        !(!series.is_empty() && series.iter().all(|ser| marker_hidden(ser))) && !matches!(declared, "line" | "smooth");
    match (lines, markers, smooth) {
        (false, ..) => None,
        (true, true, false) => Some("lineMarker".into()),
        (true, false, false) => Some("line".into()),
        (true, true, true) => Some("smoothMarker".into()),
        (true, false, true) => Some("smooth".into()),
    }
}

/// The cell anchor of a `oneCellAnchor`/`twoCellAnchor`, in the model's terms.
struct DrawingAnchor {
    address: String,
    col_off_emu: i64,
    row_off_emu: i64,
    to_address: Option<String>,
    to_col_off_emu: i64,
    to_row_off_emu: i64,
    width_px: f64,
    height_px: f64,
}

/// 96 dpi pixels per EMU, the same factor the exporter uses.
const EMU_PER_PX: f64 = 9525.0;

fn parse_drawing_anchor(anchor: &XmlNode) -> Option<DrawingAnchor> {
    let from = anchor.child("from")?;
    let child_number = |node: &XmlNode, name: &str| -> Option<u32> {
        node.child(name).map(XmlNode::deep_text).and_then(|value| value.trim().parse::<u32>().ok())
    };
    let column = child_number(from, "col")?;
    let row = child_number(from, "row")?;
    if row > MAX_IMPORT_ROWS || column > MAX_IMPORT_COLS {
        return None;
    }
    let offset = |node: Option<&XmlNode>, name: &str| -> i64 {
        node.and_then(|node| node.child(name))
            .map(XmlNode::deep_text)
            .and_then(|value| value.trim().parse::<i64>().ok())
            .unwrap_or(0)
    };
    let col_off_emu = offset(Some(from), "colOff");
    let row_off_emu = offset(Some(from), "rowOff");
    let to = anchor.child("to");
    let to_address = to.and_then(|to| {
        let to_column = child_number(to, "col")?;
        let to_row = child_number(to, "row")?;
        Some(crate::address::format(to_row, to_column))
    });
    let to_col_off_emu = offset(to, "colOff");
    let to_row_off_emu = offset(to, "rowOff");
    // A one-cell anchor carries `ext`; a two-cell anchor's extent is derived
    // from its corners using Excel's default column (64 px) and row (20 px).
    let (width_px, height_px) = if let Some(ext) = anchor.child("ext") {
        let cx = ext.attr("cx").and_then(|value| value.trim().parse::<f64>().ok()).unwrap_or(0.0);
        let cy = ext.attr("cy").and_then(|value| value.trim().parse::<f64>().ok()).unwrap_or(0.0);
        (cx / EMU_PER_PX, cy / EMU_PER_PX)
    } else {
        let to = to?;
        let to_column = child_number(to, "col").unwrap_or(column);
        let to_row = child_number(to, "row").unwrap_or(row);
        let width = to_column.saturating_sub(column) as f64 * 64.0 + (to_col_off_emu - col_off_emu) as f64 / EMU_PER_PX;
        let height = to_row.saturating_sub(row) as f64 * 20.0 + (to_row_off_emu - row_off_emu) as f64 / EMU_PER_PX;
        (width, height)
    };
    if !width_px.is_finite() || !height_px.is_finite() || width_px <= 0.0 || height_px <= 0.0 {
        return None;
    }
    Some(DrawingAnchor {
        address: crate::address::format(row, column),
        col_off_emu,
        row_off_emu,
        to_address,
        to_col_off_emu,
        to_row_off_emu,
        width_px,
        height_px,
    })
}

/// Reads the sheet's drawing part and maps charts and pictures onto the model.
///
/// A drawing is optional: a sheet without one simply keeps no floating
/// objects. Every failure is a warning; the calamine values are never touched.
fn apply_drawings(
    zip: &crate::zip::ZipReader,
    part: &str,
    root: &XmlNode,
    sheet: &mut Sheet,
    warnings: &mut Vec<String>,
) {
    let Some(drawing) = root.child("drawing") else { return };
    let Some(relationship_id) = drawing.attr_any_ns("id") else { return };
    let relationships = read_relationships(zip, part);
    let Some(relationship) =
        relationships.get(relationship_id).filter(|relationship| relationship.kind.ends_with("/drawing"))
    else {
        return;
    };
    let target = resolve_part(part, &relationship.target);
    let text = match zip.read_text(&target) {
        Ok(text) => text,
        Err(error) => {
            warnings.push(format!("The drawing of sheet \"{}\" could not be read ({error}).", sheet.name));
            return;
        }
    };
    if text.len() > MAX_DETAIL_PART_BYTES {
        warnings.push(format!(
            "The drawing of sheet \"{}\" is too large to inspect; charts and images were skipped.",
            sheet.name
        ));
        return;
    }
    let parsed = match parse_xml(&text) {
        Ok(parsed) => parsed,
        Err(_) => {
            warnings.push(format!(
                "The drawing of sheet \"{}\" could not be parsed; charts and images were skipped.",
                sheet.name
            ));
            return;
        }
    };
    let drawing_relationships = read_relationships(zip, &target);
    for anchor in parsed.children.iter().filter(|child| matches!(child.local_name(), "oneCellAnchor" | "twoCellAnchor"))
    {
        let Some(geometry) = parse_drawing_anchor(anchor) else { continue };
        // Charts: `xdr:graphicFrame` wrapping a `c:chart` relationship.
        if let Some(frame) = anchor.child("graphicFrame") {
            let Some(chart_ref) = descendant(frame, "chart") else {
                warnings.push(format!(
                    "A non-chart graphic frame in the drawing of sheet \"{}\" was not imported.",
                    sheet.name
                ));
                continue;
            };
            let Some(embed) = chart_ref.attr_any_ns("id") else {
                warnings.push(format!("A chart on sheet \"{}\" has no relationship and was skipped.", sheet.name));
                continue;
            };
            let Some(relationship) =
                drawing_relationships.get(embed).filter(|relationship| relationship.kind.ends_with("/chart"))
            else {
                warnings.push(format!(
                    "A chart relationship on sheet \"{}\" could not be resolved and was skipped.",
                    sheet.name
                ));
                continue;
            };
            let chart_part = resolve_part(&target, &relationship.target);
            let chart_text = match zip.read_text(&chart_part) {
                Ok(text) => text,
                Err(_) => {
                    warnings.push(format!("Chart part {chart_part} could not be read and was skipped."));
                    continue;
                }
            };
            if chart_text.len() > MAX_DETAIL_PART_BYTES {
                warnings.push(format!("Chart part {chart_part} is too large to inspect and was skipped."));
                continue;
            }
            match read_xlsx_chart(&chart_text, warnings) {
                Some(chart) => sheet.charts.push(ChartPlacement {
                    id: format!("chart{}", sheet.charts.len() + 1),
                    chart,
                    anchor: geometry.address,
                    width_px: geometry.width_px,
                    height_px: geometry.height_px,
                }),
                None => warnings.push(format!("Chart part {chart_part} could not be parsed and was skipped.")),
            }
            continue;
        }
        // Pictures: `xdr:pic` with an `a:blip/@r:embed` relationship.
        let Some(picture) = anchor.child("pic") else { continue };
        let Some(embed) = descendant(picture, "blip").and_then(|blip| blip.attr_any_ns("embed")) else {
            warnings.push(format!("An image on sheet \"{}\" has no relationship and was skipped.", sheet.name));
            continue;
        };
        let Some(relationship) =
            drawing_relationships.get(embed).filter(|relationship| relationship.kind.ends_with("/image"))
        else {
            warnings.push(format!(
                "An image relationship on sheet \"{}\" could not be resolved and was skipped.",
                sheet.name
            ));
            continue;
        };
        let media_part = resolve_part(&target, &relationship.target);
        let bytes = match zip.read(&media_part) {
            Ok(bytes) if !bytes.is_empty() => bytes,
            _ => {
                warnings.push(format!("Image part {media_part} could not be read and was skipped."));
                continue;
            }
        };
        let name = media_part.rsplit('/').next().unwrap_or("image.png").to_string();
        let mut image = ImageData::from_bytes(&name, &bytes);
        image.alt = descendant(picture, "cNvPr").and_then(|props| props.attr("descr")).unwrap_or("").to_string();
        if descendant(picture, "srcRect").is_some() {
            warnings.push(format!(
                "A cropped image on sheet \"{}\" was imported without its crop; the full picture is shown.",
                sheet.name
            ));
        }
        let rotation_deg = picture
            .child("spPr")
            .and_then(|props| props.child("xfrm"))
            .and_then(|xfrm| xfrm.attr("rot"))
            .and_then(|value| value.trim().parse::<f64>().ok())
            .map(|value| value / 60_000.0)
            .unwrap_or(0.0);
        sheet.images.push(SheetImage {
            image,
            anchor: CellAnchor {
                address: geometry.address,
                col_off_emu: geometry.col_off_emu,
                row_off_emu: geometry.row_off_emu,
                to_address: geometry.to_address,
                to_col_off_emu: geometry.to_col_off_emu,
                to_row_off_emu: geometry.to_row_off_emu,
            },
            width_px: geometry.width_px,
            height_px: geometry.height_px,
            rotation_deg,
        });
    }
    let absolute = parsed.children_of("absoluteAnchor").len();
    if absolute > 0 {
        warnings.push(format!(
            "{absolute} absolutely anchored drawing(s) on sheet \"{}\" have no cell anchor and were not imported.",
            sheet.name
        ));
    }
    let shapes = parsed.children.iter().filter(|child| matches!(child.local_name(), "sp" | "grpSp")).count();
    if shapes > 0 {
        warnings.push(format!(
            "{shapes} shape(s) in the drawing of sheet \"{}\" were not imported; the sheet model has no floating shapes.",
            sheet.name
        ));
    }
}

// ---------------------------------------------------------------------------
// Import: pivot caches and tables
// ---------------------------------------------------------------------------

/// Reads the pivot tables linked from a worksheet and preserves their raw
/// parts. The rendered grid is already in the sheet cells from the calamine
/// pass; nothing here recomputes an aggregation.
fn apply_pivot_tables(
    zip: &crate::zip::ZipReader,
    part: &str,
    sheet: &Sheet,
    preserved: &mut Vec<PreservedPivot>,
    warnings: &mut Vec<String>,
) {
    let relationships = read_relationships(zip, part);
    for relationship in relationships.values() {
        if !relationship.kind.ends_with("/pivotTable") {
            continue;
        }
        let target = resolve_part(part, &relationship.target);
        if let Some(pivot) = parse_pivot_table(zip, &target, &sheet.name, warnings) {
            // Two sheets can share one cache; the table part itself is unique.
            if !preserved.iter().any(|existing| existing.table_xml == pivot.table_xml) {
                preserved.push(pivot);
            }
        }
    }
}

fn parse_pivot_table(
    zip: &crate::zip::ZipReader,
    target: &str,
    sheet_name: &str,
    warnings: &mut Vec<String>,
) -> Option<PreservedPivot> {
    let table_xml = match zip.read_text(target) {
        Ok(text) => text,
        Err(error) => {
            warnings.push(format!("A pivot table part could not be read ({error})."));
            return None;
        }
    };
    if table_xml.len() > MAX_DETAIL_PART_BYTES {
        warnings.push("A pivot table part is too large to inspect and was kept in the original file only.".to_string());
        return None;
    }
    let root = match parse_xml(&table_xml) {
        Ok(root) => root,
        Err(_) => {
            warnings.push("A pivot table part could not be parsed and was kept in the original file only.".to_string());
            return None;
        }
    };
    let mut name = root.attr("name").unwrap_or("").trim().to_string();
    if name.is_empty() {
        name = target.rsplit('/').next().unwrap_or("PivotTable").trim_end_matches(".xml").to_string();
    }
    let cache_id = parse_u32_attr(&root, "cacheId").unwrap_or(0);
    let relationships = read_relationships(zip, target);
    let Some(definition_relationship) =
        relationships.values().find(|relationship| relationship.kind.ends_with("/pivotCacheDefinition"))
    else {
        warnings
            .push(format!("Pivot table \"{name}\" has no cache definition and was kept in the original file only."));
        return None;
    };
    let definition_part = resolve_part(target, &definition_relationship.target);
    let definition_xml = match zip.read_text(&definition_part) {
        Ok(text) if text.len() <= MAX_DETAIL_PART_BYTES => text,
        _ => {
            warnings.push(format!(
                "The pivot cache definition for \"{name}\" could not be read and was kept in the original file only."
            ));
            return None;
        }
    };
    let Ok(definition_root) = parse_xml(&definition_xml) else {
        warnings.push(format!(
            "The pivot cache definition for \"{name}\" could not be parsed and was kept in the original file only."
        ));
        return None;
    };
    let source = definition_root
        .child("cacheSource")
        .and_then(|source| source.child("worksheetSource"))
        .map(|worksheet| {
            let reference = worksheet.attr("ref").unwrap_or("").trim().replace('$', "");
            let source_sheet = worksheet.attr("sheet").unwrap_or("").trim();
            if source_sheet.is_empty() || reference.is_empty() {
                reference
            } else {
                format!("{source_sheet}!{reference}")
            }
        })
        .unwrap_or_default();
    let fields: Vec<String> = definition_root
        .child("cacheFields")
        .map(|block| {
            block
                .children_of("cacheField")
                .into_iter()
                .take(MAX_PIVOT_FIELDS)
                .map(|field| field.attr("name").unwrap_or("").to_string())
                .collect()
        })
        .unwrap_or_default();
    let definition_relationships = read_relationships(zip, &definition_part);
    let records_part = definition_relationships
        .values()
        .find(|relationship| relationship.kind.ends_with("/pivotCacheRecords"))
        .map(|relationship| resolve_part(&definition_part, &relationship.target));
    let records_base64 = records_part
        .as_ref()
        .and_then(|part| zip.read(part).ok())
        .filter(|bytes| !bytes.is_empty())
        .map(|bytes| base64::engine::general_purpose::STANDARD.encode(bytes));
    Some(PreservedPivot {
        name,
        sheet: sheet_name.to_string(),
        cache_id,
        definition_xml,
        records_base64,
        table_xml,
        records_part,
        source,
        fields,
    })
}

// ---------------------------------------------------------------------------
// Import: styles.xml
// ---------------------------------------------------------------------------

fn parse_styles(xml: &str) -> OfficeResult<ImportedStyles> {
    let root = parse_xml(xml)?;
    let mut custom: BTreeMap<u32, String> = BTreeMap::new();
    if let Some(node) = root.child("numFmts") {
        for entry in node.children_of("numFmt") {
            if custom.len() >= 4_096 {
                break;
            }
            if let (Some(id), Some(code)) = (parse_u32_attr(entry, "numFmtId"), entry.attr("formatCode")) {
                custom.insert(id, code.to_string());
            }
        }
    }
    let fonts: Vec<ImportFont> = root
        .child("fonts")
        .map(|node| node.children_of("font").into_iter().map(parse_font).collect())
        .unwrap_or_default();
    let fills: Vec<Option<String>> = root
        .child("fills")
        .map(|node| node.children_of("fill").into_iter().map(parse_fill).collect())
        .unwrap_or_default();
    let borders: Vec<CellBorders> = root
        .child("borders")
        .map(|node| node.children_of("border").into_iter().map(parse_border).collect())
        .unwrap_or_default();
    let cell_styles: Vec<CellStyle> = root
        .child("cellXfs")
        .map(|node| {
            node.children_of("xf")
                .into_iter()
                .map(|xf| style_from_xf(&parse_xf(xf, &fonts, &fills, &borders, &custom)))
                .collect()
        })
        .unwrap_or_default();
    let mut dxf_fills = Vec::new();
    let mut dxf_colors = Vec::new();
    if let Some(node) = root.child("dxfs") {
        for dxf in node.children_of("dxf") {
            dxf_fills.push(dxf.child("fill").and_then(parse_fill));
            dxf_colors.push(dxf.child("font").and_then(|font| font.child("color")).and_then(color_of));
        }
    }
    Ok(ImportedStyles { cell_styles, dxf_fills, dxf_colors })
}

fn parse_xf(
    xf: &XmlNode,
    fonts: &[ImportFont],
    fills: &[Option<String>],
    borders: &[CellBorders],
    custom: &BTreeMap<u32, String>,
) -> ImportedXf {
    let font_id = parse_u32_attr(xf, "fontId").unwrap_or(0) as usize;
    let fill_id = parse_u32_attr(xf, "fillId").unwrap_or(0) as usize;
    let border_id = parse_u32_attr(xf, "borderId").unwrap_or(0) as usize;
    let mut out = ImportedXf {
        number_format: imported_number_format(parse_u32_attr(xf, "numFmtId").unwrap_or(0), custom),
        font: fonts.get(font_id).cloned(),
        fill: fills.get(fill_id).cloned().flatten(),
        borders: borders.get(border_id).cloned().unwrap_or_default(),
        ..Default::default()
    };
    if let Some(alignment) = xf.child("alignment") {
        if let Some(horizontal) = alignment.attr("horizontal") {
            if horizontal != "general" {
                out.align = horizontal.to_string();
            }
        }
        if let Some(vertical) = alignment.attr("vertical") {
            out.valign = vertical.to_string();
        }
        out.wrap = attr_on(alignment, "wrapText");
        out.rotation = alignment.attr("textRotation").and_then(|value| value.trim().parse::<i32>().ok()).unwrap_or(0);
    }
    out
}

fn parse_font(font: &XmlNode) -> ImportFont {
    let mut out = ImportFont::default();
    for child in &font.children {
        match child.local_name() {
            "b" => out.bold = font_flag(child),
            "i" => out.italic = font_flag(child),
            "u" => out.underline = child.attr("val").map(|value| value != "none").unwrap_or(true),
            "strike" => out.strike = font_flag(child),
            "sz" => out.size_pt = child.attr("val").and_then(|value| value.trim().parse::<f64>().ok()),
            "name" => out.name = child.attr("val").map(str::to_string),
            "color" => out.color = color_of(child),
            _ => {}
        }
    }
    out
}

/// A `<b/>`/`<i/>`-style flag: present without `val` means on.
fn font_flag(node: &XmlNode) -> bool {
    node.attr("val").map(|value| !matches!(value.trim(), "0" | "false" | "off")).unwrap_or(true)
}

fn parse_fill(fill: &XmlNode) -> Option<String> {
    let pattern = fill.child("patternFill")?;
    if pattern.attr("patternType").map(|kind| kind.trim() == "none").unwrap_or(false) {
        return None;
    }
    pattern.child("fgColor").and_then(color_of).or_else(|| pattern.child("bgColor").and_then(color_of))
}

fn parse_border(border: &XmlNode) -> CellBorders {
    let side = |name: &str| -> Option<BorderStyle> {
        let node = border.child(name)?;
        let style = node.attr("style")?.trim();
        if style.is_empty() || style == "none" {
            return None;
        }
        let color = node.child("color").and_then(color_of).unwrap_or_else(|| "#000000".into());
        Some(BorderStyle { style: style.to_string(), color })
    };
    CellBorders { top: side("top"), right: side("right"), bottom: side("bottom"), left: side("left") }
}

/// `rgb="FFRRGGBB"` (or `RRGGBB`) to the model's `#RRGGBB`. Theme and indexed
/// colours have no portable value in the model and resolve to `None`.
fn color_of(node: &XmlNode) -> Option<String> {
    if attr_on(node, "auto") {
        return None;
    }
    let rgb = node.attr("rgb")?;
    let hex: String = rgb.chars().filter(|character| character.is_ascii_hexdigit()).collect();
    match hex.len() {
        8 => normalize_hex(&hex[2..]),
        6 => normalize_hex(&hex),
        _ => None,
    }
}

fn style_from_xf(xf: &ImportedXf) -> CellStyle {
    let mut style = CellStyle {
        number_format: xf.number_format.clone(),
        fill: xf.fill.clone(),
        align: xf.align.clone(),
        valign: xf.valign.clone(),
        wrap: xf.wrap,
        rotation: xf.rotation,
        borders: xf.borders.clone(),
        ..Default::default()
    };
    if let Some(font) = &xf.font {
        style.font = font.name.clone();
        style.size_pt = font.size_pt;
        style.bold = font.bold;
        style.italic = font.italic;
        style.underline = font.underline;
        style.strike = font.strike;
        style.color = font.color.clone();
    }
    style
}

/// True when a style-only cell is worth keeping in the model. `General` is the
/// model's default number format, so an xf that only says `General` must not
/// materialise empty cells.
fn style_worth_a_cell(style: &CellStyle) -> bool {
    let mut probe = style.clone();
    if probe.number_format == "General" {
        probe.number_format.clear();
    }
    probe != CellStyle::default()
}

/// The built-in number formats from ECMA-376; ids without a portable code map
/// to `General` rather than an invented format string.
fn builtin_number_format(id: u32) -> Option<&'static str> {
    Some(match id {
        0 => "General",
        1 => "0",
        2 => "0.00",
        3 => "#,##0",
        4 => "#,##0.00",
        5 => "$#,##0_);($#,##0)",
        6 => "$#,##0_);[Red]($#,##0)",
        7 => "$#,##0.00_);($#,##0.00)",
        8 => "$#,##0.00_);[Red]($#,##0.00)",
        9 => "0%",
        10 => "0.00%",
        11 => "0.00E+00",
        12 => "# ?/?",
        13 => "# ??/??",
        14 => "mm-dd-yy",
        15 => "d-mmm-yy",
        16 => "d-mmm",
        17 => "mmm-yy",
        18 => "h:mm AM/PM",
        19 => "h:mm:ss AM/PM",
        20 => "h:mm",
        21 => "h:mm:ss",
        22 => "m/d/yy h:mm",
        37 => "#,##0 ;(#,##0)",
        38 => "#,##0 ;[Red](#,##0)",
        39 => "#,##0.00;(#,##0.00)",
        40 => "#,##0.00;[Red](#,##0.00)",
        45 => "mm:ss",
        46 => "[h]:mm:ss",
        47 => "mmss.0",
        48 => "##0.0E+0",
        49 => "@",
        _ => return None,
    })
}

fn imported_number_format(id: u32, custom: &BTreeMap<u32, String>) -> String {
    if let Some(code) = custom.get(&id) {
        return code.clone();
    }
    builtin_number_format(id).unwrap_or("General").to_string()
}

// ---------------------------------------------------------------------------
// Import: package relationships and small attribute helpers
// ---------------------------------------------------------------------------

/// `.rels` path for a part: `xl/workbook.xml` -> `xl/_rels/workbook.xml.rels`.
fn rels_path_for(part: &str) -> String {
    match part.rsplit_once('/') {
        Some((directory, name)) => format!("{directory}/_rels/{name}.rels"),
        None => format!("_rels/{part}.rels"),
    }
}

fn read_relationships(zip: &crate::zip::ZipReader, part: &str) -> BTreeMap<String, Relationship> {
    let mut map = BTreeMap::new();
    let Ok(xml) = zip.read_text(&rels_path_for(part)) else { return map };
    let Ok(root) = parse_xml(&xml) else { return map };
    for relationship in root.children_of("Relationship") {
        let id = relationship.attr("Id").unwrap_or("").to_string();
        if id.is_empty() {
            continue;
        }
        map.insert(
            id,
            Relationship {
                kind: relationship.attr("Type").unwrap_or("").to_string(),
                target: relationship.attr("Target").unwrap_or("").to_string(),
            },
        );
    }
    map
}

/// Resolves a relationship target against the part that owns it.
fn resolve_part(part: &str, target: &str) -> String {
    if let Some(absolute) = target.strip_prefix('/') {
        return absolute.to_string();
    }
    let base = part.rsplit_once('/').map(|(directory, _)| directory).unwrap_or("");
    let mut segments: Vec<&str> = if base.is_empty() { Vec::new() } else { base.split('/').collect() };
    for segment in target.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }
    segments.join("/")
}

/// Splits `'My Sheet'!$A$1:$D$10` into the sheet name and the bare range.
fn split_sheet_reference(definition: &str) -> Option<(String, String)> {
    let definition = definition.trim().trim_start_matches('=').trim();
    let bang = definition.rfind('!')?;
    let (sheet, range) = definition.split_at(bang);
    let sheet = sheet.trim();
    let sheet = if sheet.len() >= 2 && sheet.starts_with('\'') && sheet.ends_with('\'') {
        sheet[1..sheet.len() - 1].replace("''", "'")
    } else {
        sheet.to_string()
    };
    let range = range[1..].trim().replace('$', "");
    if sheet.is_empty() || range.is_empty() {
        return None;
    }
    Some((sheet, range))
}

fn parse_u32_attr(node: &XmlNode, name: &str) -> Option<u32> {
    node.attr(name).and_then(|value| value.trim().parse::<u32>().ok())
}

fn attr_on(node: &XmlNode, name: &str) -> bool {
    node.attr(name).map(|value| matches!(value.trim(), "1" | "true" | "on")).unwrap_or(false)
}

#[cfg(test)]
mod tests {
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
        workbook.sheets[0].set(
            "E1",
            Cell { value: CellValue::Text("note".into()), comment: Some("check".into()), ..Default::default() },
        );
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
}
