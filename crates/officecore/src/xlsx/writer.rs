//! XLSX export entry points, the package assembly and the per-sheet parts it writes.

use super::*;

/// True when a structured table on the sheet already owns this filter range.
///
/// The worksheet-level `<autoFilter>` must not be written for such a range: the
/// table part carries its own filter and Excel treats two filters over the same
/// range as a corrupt file.
pub(super) fn filter_owned_by_table(sheet: &Sheet, range: &str) -> bool {
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
                escape_text(&crate::formula::to_xlsx(formula))
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
            &mut warnings,
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
    // Each distinct author is listed once; a comment points at it by position.
    let mut authors: Vec<&str> = Vec::new();
    for note in &part.comments {
        if !authors.contains(&note.author.as_str()) {
            authors.push(&note.author);
        }
    }
    let mut comments_xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<comments xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><authors>",
    );
    for author in &authors {
        comments_xml.push_str(&format!("<author>{}</author>", escape_text(author)));
    }
    comments_xml.push_str("</authors><commentList>");
    for note in &part.comments {
        let author_id = authors.iter().position(|author| *author == note.author).unwrap_or(0);
        comments_xml.push_str(&format!(
            "<comment ref=\"{}\" authorId=\"{author_id}\"><text><r><rPr><sz val=\"9\"/></rPr><t xml:space=\"preserve\">{text}</t></r></text></comment>",
            note.address,
            text = escape_text(&note.text)
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
    for (shape_id, note) in (1025u32..).zip(part.comments.iter()) {
        // Column and row are zero based in the ClientData block.
        let (row, column) = crate::address::parse(&note.address).unwrap_or((0, 0));
        // A note that stays on screen says so twice, the way Excel does.
        let (visibility, visible) = if note.visible { ("visible", "<x:Visible/>") } else { ("hidden", "") };
        vml.push_str(&format!(
            "<v:shape id=\"_x0000_s{shape_id}\" type=\"#_x0000_t202\" style=\"position:absolute;margin-left:59.25pt;margin-top:1.5pt;width:108pt;height:59.25pt;z-index:1;visibility:{visibility}\" fillcolor=\"#ffffe1\" o:insetmode=\"auto\"><v:fill color2=\"#ffffe1\"/><v:shadow on=\"t\" color=\"black\" obscured=\"t\"/><v:path o:connecttype=\"none\"/><v:textbox style=\"mso-direction-alt:auto\"><div style=\"text-align:left\"/></v:textbox><x:ClientData ObjectType=\"Note\"><x:MoveWithCells/><x:SizeWithCells/><x:AutoFill>False</x:AutoFill><x:Row>{row}</x:Row><x:Column>{column}</x:Column>{visible}</x:ClientData></v:shape>"
        ));
    }
    vml.push_str("</xml>");
    vml
}

pub fn write_xlsx(workbook: &Workbook) -> OfficeResult<Vec<u8>> {
    Ok(write_xlsx_package(workbook)?.bytes)
}

pub fn write_xlsx_file(path: &Path, workbook: &Workbook) -> OfficeResult<()> {
    let result = write_xlsx_package(workbook)?;
    write_atomic(path, &result.bytes)
}
