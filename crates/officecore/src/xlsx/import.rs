//! The detailed OOXML import pass: package relationships, workbook parts and dispatch.

use super::*;

// ---------------------------------------------------------------------------
// Import: OOXML styles, layout, validation, names and tables
// ---------------------------------------------------------------------------

/// Row/column budgets of the import: cells past them are not read.
pub(super) const MAX_IMPORT_ROWS: u32 = 100_000;
pub(super) const MAX_IMPORT_COLS: u32 = 1_000;

/// Import warning for a sheet with cells outside the imported area. The UI
/// recognises this wording (`src/lib/importWarnings.ts`) to show it translated,
/// so change both together.
pub(crate) fn import_limit_warning(sheet: &str, max_rows: u32, max_cols: u32) -> String {
    format!(
        "Import limit: sheet \"{sheet}\" has cells beyond row {max_rows} or column {max_cols}; they were not imported."
    )
}
/// Worksheet parts bigger than this are skipped by the detailed pass so a
/// hostile file cannot make the XML tree explode; calamine still returns the
/// values. 64 MiB is far above any sheet the 100k-row budget can produce.
pub(super) const MAX_DETAIL_PART_BYTES: usize = 64 * 1024 * 1024;
pub(super) const MAX_FILTER_VALUES: usize = 1_024;
/// Cache points read from a ChartML part; well above any readable chart.
pub(super) const MAX_CHART_CACHE_POINTS: usize = 100_000;
/// Pivot cache fields kept per cache definition.
pub(super) const MAX_PIVOT_FIELDS: usize = 4_096;

/// One relationship from a `.rels` part.
pub(super) struct Relationship {
    pub(super) kind: String,
    pub(super) target: String,
}

/// Reads the OOXML parts calamine does not expose and maps them onto the model.
///
/// Returns `true` when the input is an OOXML workbook (so the caller knows the
/// detailed pass applies); every parse failure is recorded as a warning and the
/// calamine values stay untouched.
pub(super) fn import_ooxml_layout(bytes: &[u8], workbook: &mut Workbook, warnings: &mut Vec<String>) -> bool {
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
    apply_hyperlinks(zip, part, &root, sheet, warnings);
    apply_comments(zip, part, sheet, warnings);
    apply_tables(zip, part, &root, sheet, table_number, warnings);
    apply_print_settings(&root, sheet);
    apply_sheet_protection(&root, sheet);
    apply_drawings(zip, part, &root, sheet, warnings);
    apply_pivot_tables(zip, part, sheet, preserved_pivots, warnings);
    Ok(())
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

pub(super) fn read_relationships(zip: &crate::zip::ZipReader, part: &str) -> BTreeMap<String, Relationship> {
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
pub(super) fn resolve_part(part: &str, target: &str) -> String {
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

pub(super) fn parse_u32_attr(node: &XmlNode, name: &str) -> Option<u32> {
    node.attr(name).and_then(|value| value.trim().parse::<u32>().ok())
}

pub(super) fn attr_on(node: &XmlNode, name: &str) -> bool {
    node.attr(name).map(|value| matches!(value.trim(), "1" | "true" | "on")).unwrap_or(false)
}
