//! XLSX import entry points and the first (calamine) pass over cell values.

use super::*;

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

/// Why the `calamine` pass could not read the cell values.
enum ValuesFailure {
    /// A `t="e"` cell holds a code `calamine` has no name for (`#CALC!`).
    UnknownErrorCode,
    Failed(OfficeError),
}

impl From<OfficeError> for ValuesFailure {
    fn from(error: OfficeError) -> Self {
        Self::Failed(error)
    }
}

/// The first pass: values and formulas for every sheet, through `calamine`.
///
/// Wrapped in `std::panic::catch_unwind` so third-party parser panics on corrupted
/// or fuzzed inputs (e.g. integer multiplication overflows) are safely caught and
/// converted into errors rather than crashing the process.
fn read_cell_values(bytes: &[u8], workbook: &mut Workbook, warnings: &mut Vec<String>) -> Result<(), ValuesFailure> {
    let mut temp_wb = Workbook::new_blank("Imported workbook");
    temp_wb.sheets.clear();
    let mut temp_warnings = Vec::new();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        read_cell_values_inner(bytes, &mut temp_wb, &mut temp_warnings)
    }));
    match result {
        Ok(Ok(())) => {
            *workbook = temp_wb;
            *warnings = temp_warnings;
            Ok(())
        }
        Ok(Err(err)) => Err(err),
        Err(_) => Err(OfficeError::corrupt("Spreadsheet parser panicked on malformed input").into()),
    }
}

fn read_cell_values_inner(
    bytes: &[u8],
    workbook: &mut Workbook,
    warnings: &mut Vec<String>,
) -> Result<(), ValuesFailure> {
    let cursor = Cursor::new(bytes.to_vec());
    let mut sheets = match calamine::open_workbook_auto_from_rs(cursor) {
        Ok(sheets) => sheets,
        Err(error) => {
            // `calamine` reads `xl/styles.xml` for number formats and fails on a
            // malformed part, which would lose every value. Retry without the
            // damaged part; the OOXML layout pass below reports it as a warning.
            let Some(repaired) = package_without_styles(bytes) else {
                return Err(OfficeError::corrupt(format!("Could not read the spreadsheet: {error}")).into());
            };
            calamine::open_workbook_auto_from_rs(Cursor::new(repaired))
                .map_err(|error| OfficeError::corrupt(format!("Could not read the spreadsheet: {error}")))?
        }
    };
    let names: Vec<String> = sheets.sheet_names().to_vec();
    if names.is_empty() {
        return Err(OfficeError::corrupt("The spreadsheet does not contain any sheet.").into());
    }
    let mut used_names: Vec<String> = Vec::new();
    for name in &names {
        let mut sheet = Sheet::new(name);
        let range = match sheets.worksheet_range(name) {
            Ok(range) => range,
            Err(calamine::Error::Xlsx(calamine::XlsxError::CellError(_))) => {
                return Err(ValuesFailure::UnknownErrorCode)
            }
            Err(error) => return Err(OfficeError::corrupt(format!("Could not read sheet {name}: {error}")).into()),
        };
        let formulas = sheets.worksheet_formula(name).ok();
        let (start_row, start_col) =
            (range.start().map(|(row, _)| row).unwrap_or(0), range.start().map(|(_, column)| column).unwrap_or(0));
        let mut max_row = 0u32;
        let mut max_col = 0u32;
        let mut cut_off = false;
        for (row_index, row) in range.rows().enumerate() {
            for (column_index, value) in row.iter().enumerate() {
                let row_number = start_row + row_index as u32;
                let column_number = start_col + column_index as u32;
                if row_number > MAX_IMPORT_ROWS || column_number > MAX_IMPORT_COLS {
                    // Empty padding past the budget costs nothing; data does.
                    cut_off |= !matches!(value, calamine::Data::Empty);
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
                    calamine::Data::Error(error) => CellValue::Error(calamine_error_text(error)),
                };
                let formula = formulas
                    .as_ref()
                    .and_then(|range| range.get_value((row_number, column_number)))
                    .filter(|text| !text.is_empty())
                    .map(|text| crate::formula::from_xlsx(text));
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
        if cut_off {
            warnings.push(import_limit_warning(name, MAX_IMPORT_ROWS, MAX_IMPORT_COLS));
        }
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

    Ok(())
}

/// Error codes `calamine` parses; any other `t="e"` value fails the whole sheet.
pub(super) const CALAMINE_ERRORS: [&str; 7] = ["#DIV/0!", "#N/A", "#NAME?", "#NULL!", "#NUM!", "#REF!", "#VALUE!"];

fn calamine_error_text(error: &calamine::CellErrorType) -> String {
    match error {
        calamine::CellErrorType::GettingData => "#GETTING_DATA".into(),
        other => other.to_string(),
    }
}

/// A copy of the package whose worksheets only carry error codes `calamine`
/// can parse. Excel 365 writes `#SPILL!`, `#CALC!`, `#FIELD!` and others as
/// ordinary error cells, which `calamine` rejects; the codes become `#VALUE!`
/// here and `restore_error_codes` puts the real ones back from the original.
fn neutralise_error_codes(bytes: &[u8]) -> Option<Vec<u8>> {
    let zip = crate::zip::ZipReader::open(bytes.to_vec()).ok()?;
    let names: Vec<String> = zip.names().map(str::to_string).collect();
    let mut writer = ZipWriter::new();
    for name in names {
        let data = zip.read(&name).ok()?;
        let is_sheet = name.starts_with("xl/worksheets/") && name.ends_with(".xml");
        match String::from_utf8(data).map(|text| (is_sheet, text)) {
            Ok((true, text)) if text.contains("t=\"e\"") => writer.add_text(&name, &neutralise_sheet_errors(&text)),
            Ok((_, text)) => writer.add(&name, text.as_bytes()),
            Err(error) => writer.add(&name, error.as_bytes()),
        }
    }
    Some(writer.finish())
}

/// Rewrites the value of every `<c t="e">` cell that holds an unparsable code.
fn neutralise_sheet_errors(sheet: &str) -> String {
    let mut out = String::with_capacity(sheet.len());
    let mut rest = sheet;
    while let Some(at) = rest.find("<c ") {
        let tag_end = rest[at..].find('>').map_or(rest.len(), |end| at + end + 1);
        out.push_str(&rest[..tag_end]);
        let tag = &rest[at..tag_end];
        rest = &rest[tag_end..];
        if !tag.contains("t=\"e\"") || tag.ends_with("/>") {
            continue;
        }
        // The cell ends at its own `</c>`; its `<v>` is the value.
        let cell_end = rest.find("</c>").unwrap_or(rest.len());
        let cell = &rest[..cell_end];
        match (cell.find("<v>"), cell.find("</v>")) {
            (Some(open), Some(close)) if open + 3 <= close => {
                let code = cell[open + 3..close].trim();
                out.push_str(&cell[..open + 3]);
                out.push_str(if CALAMINE_ERRORS.contains(&code) { code } else { "#VALUE!" });
                out.push_str(&cell[close..]);
            }
            _ => out.push_str(cell),
        }
        rest = &rest[cell_end..];
    }
    out.push_str(rest);
    out
}

pub fn read_workbook_bytes(bytes: &[u8]) -> OfficeResult<SheetRead> {
    if bytes.len() < 8 {
        return Err(OfficeError::corrupt("The file is too small to be a spreadsheet."));
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| read_workbook_bytes_inner(bytes)));
    match result {
        Ok(inner) => inner,
        Err(_) => Err(OfficeError::corrupt("Spreadsheet parser panicked on malformed input")),
    }
}

fn read_workbook_bytes_inner(bytes: &[u8]) -> OfficeResult<SheetRead> {
    let mut warnings: Vec<String> = Vec::new();
    let mut workbook = Workbook::new_blank("Imported workbook");
    workbook.sheets.clear();

    match read_cell_values(bytes, &mut workbook, &mut warnings) {
        Ok(()) => {}
        Err(ValuesFailure::UnknownErrorCode) => {
            let repaired = neutralise_error_codes(bytes)
                .ok_or_else(|| OfficeError::corrupt("The spreadsheet holds an error code that cannot be read."))?;
            workbook.sheets.clear();
            warnings.clear();
            match read_cell_values(&repaired, &mut workbook, &mut warnings) {
                Ok(()) => {}
                Err(ValuesFailure::UnknownErrorCode) => {
                    return Err(OfficeError::corrupt("The spreadsheet holds an error code that cannot be read."));
                }
                Err(ValuesFailure::Failed(error)) => return Err(error),
            }
        }
        Err(ValuesFailure::Failed(error)) => return Err(error),
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
        // A legacy `.xls` is a CFB container, not an OOXML package: calamine
        // exposes the cached cell values but not the formula records, so say
        // exactly that instead of implying formulas were imported.
        if bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]) {
            warnings.push(
                "Legacy Excel (.xls) workbooks import the cached values only: formulas arrive as their last saved results and formatting, comments and layout are not imported."
                    .into(),
            );
        } else {
            warnings.push("Formatting, layout and comments are imported from XLSX/XLSM workbooks only; this file opened with values and formulas.".into());
        }
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
