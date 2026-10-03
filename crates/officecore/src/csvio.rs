//! CSV import/export with delimiter detection and encoding tolerance.

use crate::error::{OfficeError, OfficeResult};
use crate::io::write_atomic;
use crate::model::*;
use crate::xlsx::SheetRead;
use std::path::Path;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CsvOptions {
    /// "," | ";" | "\t" | "auto"
    pub delimiter: String,
    pub quote: String,
    pub has_header: bool,
    /// "utf-8" | "windows-1252" | "auto"
    pub encoding: String,
}

impl Default for CsvOptions {
    fn default() -> Self {
        Self { delimiter: "auto".into(), quote: "\"".into(), has_header: true, encoding: "auto".into() }
    }
}

fn delimiter_char(value: &str) -> Option<char> {
    match value {
        "comma" | "," => Some(','),
        "semicolon" | ";" => Some(';'),
        "tab" | "\\t" | "\t" => Some('\t'),
        "pipe" | "|" => Some('|'),
        _ => None,
    }
}

pub fn detect_delimiter(text: &str) -> char {
    let mut counts = [0usize; 3];
    let candidates = [',', ';', '\t'];
    for line in text.lines().take(20) {
        let mut in_quotes = false;
        for ch in line.chars() {
            if ch == '"' {
                in_quotes = !in_quotes;
                continue;
            }
            if in_quotes {
                continue;
            }
            for (index, candidate) in candidates.iter().enumerate() {
                if ch == *candidate {
                    counts[index] += 1;
                }
            }
        }
    }
    let mut best = 0;
    for index in 1..candidates.len() {
        if counts[index] > counts[best] {
            best = index;
        }
    }
    if counts[best] == 0 {
        ','
    } else {
        candidates[best]
    }
}

fn decode_text(bytes: &[u8], encoding: &str) -> String {
    let trimmed = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match encoding {
        "utf-8" | "utf8" => String::from_utf8_lossy(trimmed).into_owned(),
        // The declared code page still wins; the fallback is Windows-1254/1252
        // rather than a Latin-1 byte cast (see `crate::encoding`).
        "windows-1252" | "latin1" | "ansi" => crate::encoding::decode_legacy(trimmed),
        _ => String::from_utf8(trimmed.to_vec()).unwrap_or_else(|_| crate::encoding::decode_legacy(trimmed)),
    }
}

/// RFC 4180-style parser: quoted fields, embedded delimiters and newlines.
fn parse_records(text: &str, delimiter: char, quote: char) -> Vec<Vec<String>> {
    let mut records: Vec<Vec<String>> = Vec::new();
    let mut record: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == quote {
                if chars.peek() == Some(&quote) {
                    field.push(quote);
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(ch);
            }
            continue;
        }
        if ch == quote && field.is_empty() {
            in_quotes = true;
            continue;
        }
        if ch == delimiter {
            record.push(std::mem::take(&mut field));
            continue;
        }
        if ch == '\r' {
            continue;
        }
        if ch == '\n' {
            record.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut record));
            continue;
        }
        field.push(ch);
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    records
}

fn parse_number(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let normalized = if trimmed.contains(',') && !trimmed.contains('.') && trimmed.matches(',').count() == 1 {
        // "1,5" style decimals are common in European exports.
        trimmed.replacen(',', ".", 1)
    } else {
        trimmed.replace(',', "")
    };
    normalized.parse::<f64>().ok()
}

pub fn parse_csv(bytes: &[u8], options: &CsvOptions) -> OfficeResult<SheetRead> {
    let text = decode_text(bytes, &options.encoding);
    let delimiter = delimiter_char(&options.delimiter).unwrap_or_else(|| detect_delimiter(&text));
    let quote = options.quote.chars().next().unwrap_or('"');
    let records = parse_records(&text, delimiter, quote);
    if records.is_empty() {
        return Err(OfficeError::corrupt("The CSV file is empty."));
    }
    let mut sheet = Sheet::new("Sheet1");
    let mut max_row = 0u32;
    let mut max_col = 0u32;
    for (row_index, record) in records.iter().enumerate() {
        if row_index > 100_000 {
            break;
        }
        for (column_index, value) in record.iter().enumerate() {
            if column_index > 1_000 {
                break;
            }
            if value.is_empty() {
                continue;
            }
            let address = crate::address::format(row_index as u32, column_index as u32);
            let cell_value = match parse_number(value) {
                Some(number) => CellValue::Number(number),
                None => {
                    if value.eq_ignore_ascii_case("true") {
                        CellValue::Bool(true)
                    } else if value.eq_ignore_ascii_case("false") {
                        CellValue::Bool(false)
                    } else {
                        CellValue::Text(value.clone())
                    }
                }
            };
            sheet.set(&address, Cell { value: cell_value, ..Default::default() });
            max_row = max_row.max(row_index as u32);
            max_col = max_col.max(column_index as u32);
        }
    }
    sheet.row_count = (max_row + 51).max(200);
    sheet.col_count = (max_col + 6).max(26);
    let mut workbook = Workbook::new_blank("Imported CSV");
    workbook.sheets = vec![sheet];
    let mut warnings = Vec::new();
    if !options.has_header {
        warnings.push("The file was imported without a header row.".into());
    }
    warnings.push(format!(
        "Delimiter: {}",
        match delimiter {
            '\t' => "tab".to_string(),
            other => other.to_string(),
        }
    ));
    Ok(SheetRead { workbook, warnings })
}

fn value_to_csv(cell: &Cell) -> String {
    if let Some(formula) = &cell.formula {
        return formula.clone();
    }
    match &cell.value {
        CellValue::Empty => String::new(),
        CellValue::Number(number) => {
            if number.fract() == 0.0 && number.abs() < 1e15 {
                format!("{}", *number as i64)
            } else {
                format!("{number}")
            }
        }
        CellValue::Text(text) => text.clone(),
        CellValue::Bool(value) => if *value { "TRUE" } else { "FALSE" }.to_string(),
        CellValue::Error(error) => error.clone(),
    }
}

fn escape_field(value: &str, delimiter: char, quote: char) -> String {
    if value.contains(delimiter) || value.contains(quote) || value.contains('\n') || value.contains('\r') {
        format!("{quote}{}{quote}", value.replace(quote, &format!("{quote}{quote}")))
    } else {
        value.to_string()
    }
}

pub fn write_csv(workbook: &Workbook, sheet_index: usize, options: &CsvOptions) -> OfficeResult<Vec<u8>> {
    let sheet =
        workbook.sheets.get(sheet_index).ok_or_else(|| OfficeError::invalid("The requested sheet does not exist."))?;
    let delimiter = delimiter_char(&options.delimiter).unwrap_or(',');
    let quote = options.quote.chars().next().unwrap_or('"');
    let mut max_row = 0u32;
    let mut max_col = 0u32;
    for (address, cell) in &sheet.cells {
        if cell.is_empty() {
            continue;
        }
        if let Some((row, column)) = crate::address::parse(address) {
            max_row = max_row.max(row);
            max_col = max_col.max(column);
        }
    }
    let mut out = String::new();
    for row in 0..=max_row {
        let mut fields = Vec::new();
        for column in 0..=max_col {
            let address = crate::address::format(row, column);
            let value = sheet.cells.get(&address).map(value_to_csv).unwrap_or_default();
            fields.push(escape_field(&value, delimiter, quote));
        }
        out.push_str(&fields.join(&delimiter.to_string()));
        out.push_str("\r\n");
    }
    Ok(out.into_bytes())
}

pub fn write_csv_file(path: &Path, workbook: &Workbook, sheet_index: usize, options: &CsvOptions) -> OfficeResult<()> {
    write_atomic(path, &write_csv(workbook, sheet_index, options)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_delimiter() {
        assert_eq!(detect_delimiter("a,b,c\n1,2,3"), ',');
        assert_eq!(detect_delimiter("a;b;c\n1;2;3"), ';');
        assert_eq!(detect_delimiter("a\tb\tc"), '\t');
    }

    #[test]
    fn parses_quotes_and_newlines() {
        let text = "name,note\n\"Doe, John\",\"line1\nline2\"\n";
        let records = parse_records(text, ',', '"');
        assert_eq!(records.len(), 2);
        assert_eq!(records[0], vec!["name".to_string(), "note".to_string()]);
        assert_eq!(records[1][0], "Doe, John");
        assert_eq!(records[1][1], "line1\nline2");
    }

    #[test]
    fn csv_roundtrip() {
        let mut workbook = Workbook::new_blank("CSV");
        let sheet = &mut workbook.sheets[0];
        sheet.set("A1", Cell { value: CellValue::Text("Name".into()), ..Default::default() });
        sheet.set("B1", Cell { value: CellValue::Number(12.5), ..Default::default() });
        sheet.set("A2", Cell { value: CellValue::Text("With, comma".into()), ..Default::default() });
        sheet.set("B2", Cell { value: CellValue::Bool(true), ..Default::default() });
        let bytes = write_csv(&workbook, 0, &CsvOptions::default()).unwrap();
        let text = String::from_utf8(bytes.clone()).unwrap();
        assert!(text.contains("\"With, comma\""));
        let read = parse_csv(&bytes, &CsvOptions::default()).unwrap();
        let sheet = &read.workbook.sheets[0];
        assert_eq!(sheet.get("B1").map(|cell| cell.value.clone()), Some(CellValue::Number(12.5)));
        assert_eq!(sheet.get("A2").map(|cell| cell.value.clone()), Some(CellValue::Text("With, comma".into())));
    }

    #[test]
    fn semicolon_and_decimal_comma() {
        let bytes = b"Name;Price\nPen;1,50\n";
        let options = CsvOptions { delimiter: "semicolon".into(), ..Default::default() };
        let read = parse_csv(bytes, &options).unwrap();
        assert_eq!(read.workbook.sheets[0].get("B2").map(|cell| cell.value.clone()), Some(CellValue::Number(1.5)));
    }

    #[test]
    fn rejects_empty() {
        assert!(parse_csv(b"", &CsvOptions::default()).is_err());
    }
}
