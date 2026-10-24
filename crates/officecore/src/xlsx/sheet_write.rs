//! Export of one worksheet part: cells, notes, layout, protection and print settings.

use super::*;

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

/// The editor's default column width, used for the width a hidden column keeps.
const DEFAULT_COLUMN_PX: f64 = 96.0;

fn column_width_units(pixels: f64) -> f64 {
    // Approximate Excel's character-based width.
    ((pixels - 5.0) / 7.0).max(2.0)
}

/// One worksheet plus the companion parts that hang off it.
///
/// XLSX keeps a worksheet's relationships in a sibling `.rels` file, so the
/// hyperlink targets, the comment part and its VML shapes cannot live in the
/// worksheet XML itself.
pub(super) struct SheetPart {
    pub(super) xml: String,
    pub(super) rels: Vec<(String, String, String)>,
    pub(super) comments: Vec<SheetNote>,
}

/// The author a note is filed under when the model does not know who wrote it.
pub(super) const DEFAULT_NOTE_AUTHOR: &str = "OmniOffice";

/// One cell note headed for `xl/commentsN.xml` and its VML shape.
pub(super) struct SheetNote {
    pub(super) address: String,
    pub(super) text: String,
    pub(super) author: String,
    pub(super) visible: bool,
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
    // Excel refuses the plain name of every function added after 2007.
    let formula = formula.map(crate::formula::to_xlsx);
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
            value_xml = format!("<v>{}</v>", crate::formula::excel_error(error));
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
pub(super) fn sheet_comments(sheet: &Sheet) -> Vec<SheetNote> {
    let mut comments = Vec::new();
    for (address, cell) in &sheet.cells {
        if cell.is_empty() {
            continue;
        }
        if let (Some(text), Some((row, column))) = (cell.comment.as_ref(), crate::address::parse(address)) {
            if !text.trim().is_empty() {
                comments.push(SheetNote {
                    address: crate::address::format(row, column),
                    text: text.clone(),
                    author: cell
                        .comment_author
                        .as_deref()
                        .map(str::trim)
                        .filter(|author| !author.is_empty())
                        .unwrap_or(DEFAULT_NOTE_AUTHOR)
                        .to_string(),
                    visible: cell.comment_visible,
                });
            }
        }
    }
    comments
}

pub(super) fn sheet_xml(
    sheet: &Sheet,
    drawing_number: Option<usize>,
    comment_number: Option<usize>,
    comments: Vec<SheetNote>,
    pivot_cells: &[(String, CellValue)],
    table_numbers: &[usize],
    styles: &mut StyleTable,
    shared: &mut Vec<String>,
    shared_index: &mut BTreeMap<String, usize>,
    warnings: &mut Vec<String>,
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
            // The editor hides a column by storing width 0. The file keeps the
            // flag and a width Excel shows again on "Unhide"; the clamp in
            // `column_width_units` would otherwise draw it 2 characters wide.
            let hidden = *width <= 0.0;
            writer.raw(&format!(
                "<col min=\"{}\" max=\"{}\" width=\"{:.2}\"{} customWidth=\"1\"/>",
                column + 1,
                column + 1,
                column_width_units(if hidden { DEFAULT_COLUMN_PX } else { *width }),
                if hidden { " hidden=\"1\"" } else { "" }
            ));
        }
        writer.raw("</cols>");
    }

    writer.raw("<sheetData>");
    let mut rows: BTreeMap<u32, Vec<(u32, &Cell)>> = BTreeMap::new();
    // (address, target, display, tooltip) of every link that passes the target check.
    let mut hyperlinks: Vec<(String, String, Option<String>, Option<String>)> = Vec::new();
    let mut unsafe_links = 0usize;
    for (address, cell) in &sheet.cells {
        if cell.is_empty() {
            continue;
        }
        if let (Some(target), Some((row, column))) = (cell.link.as_ref(), crate::address::parse(address)) {
            match safe_link_target(target) {
                Some(target) => hyperlinks.push((
                    crate::address::format(row, column),
                    target,
                    cell.link_display.clone().filter(|text| !text.is_empty()),
                    cell.link_tooltip.clone().filter(|text| !text.is_empty()),
                )),
                None => unsafe_links += 1,
            }
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

    // A row without cells is still a row when it is hidden or resized.
    for row in sheet.row_heights.keys() {
        rows.entry(*row).or_default();
    }
    for (row, mut cells) in rows {
        cells.sort_by_key(|(column, _)| *column);
        // The editor hides a row (or an AutoFilter does) by storing height 0.
        let row_open = match sheet.row_heights.get(&row) {
            Some(height) if *height <= 0.0 => format!("<row r=\"{}\" hidden=\"1\"", row + 1),
            Some(height) => format!("<row r=\"{}\" ht=\"{:.2}\" customHeight=\"1\"", row + 1, height * 0.75),
            None => format!("<row r=\"{}\"", row + 1),
        };
        let mut row_cells = String::new();
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
                row_cells.push_str(&xml);
            }
        }
        if row_cells.is_empty() {
            writer.raw(&format!("{row_open}/>"));
        } else {
            writer.raw(&format!("{row_open}>{row_cells}</row>"));
        }
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

    // Conditional formatting; a highlight rule points at its own dxf in
    // styles.xml. A rule that cannot be written stays in the .oswk file and is
    // reported, the way charts are.
    for (index, rule) in sheet.conditional.iter().enumerate() {
        match conditional_formatting_xml(rule, index, styles) {
            Ok(xml) => {
                writer.raw(&xml);
            }
            Err(reason) => warnings.push(format!(
                "A conditional formatting rule on sheet \"{}\" was kept in the .oswk file only: {reason}.",
                sheet.name
            )),
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

    // Hyperlinks: an external link is one relationship per target; an internal
    // one (`#Sheet2!A1`) is a `location` and needs none.
    let mut rels: Vec<(String, String, String)> = Vec::new();
    if unsafe_links > 0 {
        warnings.push(format!(
            "{unsafe_links} hyperlink(s) on sheet \"{}\" were not written: only http, https, mailto and internal references are allowed.",
            sheet.name
        ));
    }
    if !hyperlinks.is_empty() {
        writer.raw(&format!("<hyperlinks count=\"{}\">", hyperlinks.len()));
        for (address, target, display, tooltip) in &hyperlinks {
            let mut attributes = format!("ref=\"{}\"", escape_attr(address));
            if let Some(location) = target.strip_prefix('#') {
                attributes.push_str(&format!(" location=\"{}\"", escape_attr(location)));
            } else {
                let rid = format!("rId{}", rels.len() + 1);
                attributes.push_str(&format!(" r:id=\"{rid}\""));
                rels.push((
                    rid,
                    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink".into(),
                    target.clone(),
                ));
            }
            if let Some(display) = display {
                attributes.push_str(&format!(" display=\"{}\"", escape_attr(display)));
            }
            if let Some(tooltip) = tooltip {
                attributes.push_str(&format!(" tooltip=\"{}\"", escape_attr(tooltip)));
            }
            writer.raw(&format!("<hyperlink {attributes}/>"));
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
