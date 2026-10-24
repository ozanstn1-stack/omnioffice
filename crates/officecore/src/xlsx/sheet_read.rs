//! Import of one worksheet part: print settings, cells, merges, filters, links and tables.

use super::*;

// ---------------------------------------------------------------------------
// Import: print settings and sheet protection
// ---------------------------------------------------------------------------

/// `<pageSetup>`, `<printOptions>`, `<pageMargins>`, `<headerFooter>` and the
/// manual page breaks, mapped onto [`PrintSettings`].
pub(super) fn apply_print_settings(root: &XmlNode, sheet: &mut Sheet) {
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
pub(super) fn apply_sheet_protection(root: &XmlNode, sheet: &mut Sheet) {
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
pub(super) fn apply_columns(root: &XmlNode, sheet: &mut Sheet) {
    let Some(cols) = root.child("cols") else { return };
    for col in cols.children_of("col") {
        let width = col.attr("width").and_then(|value| value.trim().parse::<f64>().ok());
        // Excel hides a column with a flag and keeps its width; a zero width
        // is invisible too. The editor models both as width 0.
        let hidden = attr_on(col, "hidden") || width.is_some_and(|width| width <= 0.0);
        if width.is_none() && !hidden {
            continue;
        }
        let min = parse_u32_attr(col, "min").unwrap_or(1).saturating_sub(1);
        let max = parse_u32_attr(col, "max").unwrap_or(min + 1).saturating_sub(1).max(min);
        if min > MAX_IMPORT_COLS {
            continue;
        }
        let pixels = if hidden { 0.0 } else { (width.unwrap_or(0.0) * 7.0 + 5.0).max(24.0) };
        // "Hide everything right of Z" is one `<col min="27" max="16384">`; only
        // the columns of the imported grid are recorded.
        let last = if hidden { max.min(sheet.col_count.saturating_sub(1)) } else { max };
        for column in min..=last.min(MAX_IMPORT_COLS) {
            sheet.col_widths.insert(column, pixels);
        }
    }
}

/// Puts the error code of a `t="e"` cell back when the first pass could only
/// read it as `#VALUE!` (see `neutralise_error_codes`).
fn restore_error_code(cell: &XmlNode, sheet: &mut Sheet, row: u32, column: u32) {
    if cell.attr("t") != Some("e") {
        return;
    }
    let code = cell.child("v").map(|value| value.deep_text().trim().to_string()).unwrap_or_default();
    if code.is_empty() || CALAMINE_ERRORS.contains(&code.as_str()) {
        return;
    }
    let address = crate::address::format(row, column);
    match sheet.cells.get_mut(&address) {
        Some(existing) => existing.value = CellValue::Error(code),
        None => {
            sheet.cells.insert(address, Cell { value: CellValue::Error(code), ..Default::default() });
        }
    }
}

/// `<sheetData>` row heights (points -> pixels), error codes `calamine` could
/// not read and per-cell styles. Other values and formulas are not touched: they
/// came from the calamine pass.
pub(super) fn apply_sheet_cells(root: &XmlNode, sheet: &mut Sheet, styles: &ImportedStyles) {
    let Some(data) = root.child("sheetData") else { return };
    let styles_available = !styles.cell_styles.is_empty();
    for row in data.children_of("row") {
        let Some(row_number) = parse_u32_attr(row, "r").map(|value| value.saturating_sub(1)) else { continue };
        if row_number > MAX_IMPORT_ROWS {
            continue;
        }
        if attr_on(row, "hidden") {
            // Excel keeps a hidden row's height; the editor models hidden as 0.
            // Rows below the imported grid are not recorded (a sheet that hides
            // everything under its data would add a hundred thousand entries).
            if row_number < sheet.row_count {
                sheet.row_heights.insert(row_number, 0.0);
            }
        } else if let Some(height) = row.attr("ht").and_then(|value| value.trim().parse::<f64>().ok()) {
            sheet.row_heights.insert(row_number, height * PT_TO_PX);
        }
        for cell in row.children_of("c") {
            let Some((cell_row, cell_column)) = cell.attr("r").and_then(crate::address::parse) else { continue };
            if cell_row > MAX_IMPORT_ROWS || cell_column > MAX_IMPORT_COLS {
                continue;
            }
            restore_error_code(cell, sheet, cell_row, cell_column);
            if !styles_available {
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

pub(super) fn apply_merges(root: &XmlNode, sheet: &mut Sheet) {
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

pub(super) fn apply_freeze_panes(root: &XmlNode, sheet: &mut Sheet) {
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

pub(super) fn apply_sheet_filter(root: &XmlNode, sheet: &mut Sheet) {
    let Some(node) = root.child("autoFilter") else { return };
    if let Some(state) = parse_filter_state(node, "") {
        sheet.filter = Some(state);
    }
}

pub(super) fn apply_validations(root: &XmlNode, sheet: &mut Sheet, warnings: &mut Vec<String>) {
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
pub(super) fn list_reference(values: &[String]) -> Option<String> {
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

/// Cells one hyperlink may cover (`ref="A1:C300"`), and the cells all of a
/// sheet's links may cover, so a hostile range cannot build millions of cells.
const MAX_LINK_RANGE_CELLS: usize = 10_000;
const MAX_LINKED_CELLS: usize = 100_000;

/// Reads `<hyperlinks>`: an external target comes through the sheet's
/// relationships, an internal one through `location` (kept as `#Sheet2!A1`).
///
/// Every target is checked with [`safe_link_target`]; links to files, scripts
/// and network paths are dropped and counted in one warning.
pub(super) fn apply_hyperlinks(
    zip: &crate::zip::ZipReader,
    part: &str,
    root: &XmlNode,
    sheet: &mut Sheet,
    warnings: &mut Vec<String>,
) {
    let Some(links) = root.child("hyperlinks") else { return };
    let relationships = read_relationships(zip, part);
    let mut dropped = 0usize;
    let mut linked = 0usize;
    for link in links.children_of("hyperlink") {
        let Some(reference) = link.attr("ref").map(str::trim).filter(|reference| !reference.is_empty()) else {
            continue;
        };
        let external = link
            .attr_any_ns("id")
            .and_then(|id| relationships.get(id))
            .filter(|relationship| relationship.kind.ends_with("/hyperlink"))
            .map(|relationship| relationship.target.trim().to_string());
        let location = link.attr("location").map(str::trim).filter(|location| !location.is_empty());
        let target = match (external, location) {
            (Some(url), None) => url,
            (Some(url), Some(location)) => format!("{url}#{location}"),
            (None, Some(location)) => format!("#{location}"),
            (None, None) => continue,
        };
        let Some(target) = safe_link_target(&target) else {
            dropped += 1;
            continue;
        };
        let display = link.attr("display").filter(|text| !text.is_empty()).map(str::to_string);
        let tooltip = link.attr("tooltip").filter(|text| !text.is_empty()).map(str::to_string);
        for address in crate::address::expand_range(reference, MAX_LINK_RANGE_CELLS) {
            if linked >= MAX_LINKED_CELLS {
                break;
            }
            linked += 1;
            let cell = sheet.cells.entry(address).or_default();
            // The cell text is the label; `display` is only worth keeping when
            // it says something else (or the cell has no text at all).
            let differs = match &cell.value {
                CellValue::Text(text) => display.as_deref().is_some_and(|display| display != text),
                CellValue::Empty => display.is_some(),
                _ => false,
            };
            cell.link = Some(target.clone());
            cell.link_display = if differs { display.clone() } else { None };
            cell.link_tooltip = tooltip.clone();
        }
    }
    if dropped > 0 {
        warnings.push(format!(
            "{dropped} hyperlink(s) on sheet \"{}\" were dropped: only http, https, mailto and internal references are allowed.",
            sheet.name
        ));
    }
}

/// The note text of a `<comment>`: Excel starts a rich note with the author's
/// name in a bold run ("Ada:"), which belongs to the author field and not to the
/// text; the legacy copy of a threaded comment carries a notice that is dropped
/// to leave the comment itself.
fn note_text(text: Option<&XmlNode>, author: Option<&str>) -> String {
    let Some(text) = text else { return String::new() };
    let runs = text.children_of("r");
    let mut out = String::new();
    if runs.is_empty() {
        out = text.deep_text();
    }
    for (index, run) in runs.iter().enumerate() {
        let run_text = run.child("t").map(XmlNode::deep_text).unwrap_or_default();
        let bold = run.child("rPr").is_some_and(|properties| properties.child("b").is_some());
        let names_the_author = author.is_some_and(|author| run_text.trim().trim_end_matches(':').trim() == author);
        if index == 0 && bold && names_the_author {
            continue;
        }
        out.push_str(&run_text);
    }
    let out = out.trim();
    if let Some((_, comment)) = out.strip_prefix("[Threaded comment]").and_then(|rest| rest.split_once("Comment:")) {
        return comment
            .lines()
            .map(|line| line.strip_prefix("    ").unwrap_or(line))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();
    }
    out.to_string()
}

/// The cells whose note the legacy VML drawing keeps open (`<x:Visible/>` or
/// `visibility:visible`), as zero-based (row, column).
fn read_visible_notes(zip: &crate::zip::ZipReader, target: &str) -> std::collections::HashSet<(u32, u32)> {
    let mut visible = std::collections::HashSet::new();
    let Ok(root) = zip.read_text(target).and_then(|xml| parse_xml(&xml)) else { return visible };
    let mut shapes = Vec::new();
    root.find_all("shape", &mut shapes);
    for shape in shapes {
        let Some(client) = descendant(shape, "ClientData").filter(|client| client.attr("ObjectType") == Some("Note"))
        else {
            continue;
        };
        let number = |name: &str| client.child(name).and_then(|node| node.deep_text().trim().parse::<u32>().ok());
        let open = client.child("Visible").is_some()
            || shape.attr("style").is_some_and(|style| style.replace(' ', "").contains("visibility:visible"));
        if let (true, Some(row), Some(column)) = (open, number("Row"), number("Column")) {
            visible.insert((row, column));
        }
    }
    visible
}

pub(super) fn apply_comments(zip: &crate::zip::ZipReader, part: &str, sheet: &mut Sheet, warnings: &mut Vec<String>) {
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
    let authors: Vec<String> = root
        .child("authors")
        .map(|authors| {
            authors.children_of("author").iter().map(|author| author.deep_text().trim().to_string()).collect()
        })
        .unwrap_or_default();
    let open_notes = relationships
        .values()
        .find(|relationship| relationship.kind.ends_with("/vmlDrawing"))
        .map(|relationship| read_visible_notes(zip, &resolve_part(part, &relationship.target)))
        .unwrap_or_default();
    for comment in list.children_of("comment") {
        let Some((row, column)) = comment.attr("ref").and_then(crate::address::parse) else { continue };
        // A threaded comment's legacy author is "tc={guid}", which names no one.
        let author = comment
            .attr("authorId")
            .and_then(|id| id.trim().parse::<usize>().ok())
            .and_then(|index| authors.get(index))
            .filter(|author| !author.is_empty() && !author.starts_with("tc="))
            .cloned();
        let text = note_text(comment.child("text"), author.as_deref());
        if text.is_empty() {
            continue;
        }
        let address = crate::address::format(row, column);
        let cell = sheet.cells.entry(address).or_default();
        cell.comment = Some(text);
        cell.comment_author = author.filter(|author| author != DEFAULT_NOTE_AUTHOR);
        cell.comment_visible = open_notes.contains(&(row, column));
    }
}

pub(super) fn apply_tables(
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
                .map(|node| crate::formula::unprefix_xlsx(&node.deep_text()))
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
