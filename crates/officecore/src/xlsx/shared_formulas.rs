//! Shared-formula expansion for XLSX imports (D6).
//!
//! Excel stores a filled-down formula once: the first cell carries
//! `<f t="shared" si="0" ref="A1:A10">A1*2</f>` and the cells below carry
//! `<f t="shared" si="0"/>` with no text. `calamine` exposes the base formula
//! but the followers arrive empty, so a workbook imported by the editor lost
//! nine of ten formulas. This module reads the raw sheet XML, keeps the base
//! formula per `si` and expands each follower by shifting the relative A1
//! references by the row/column offset from the base cell.
//!
//! Absolute references (`$A$1`), mixed references (`$A1`, `A$1`), ranges
//! (`A1:B2`) and references in other sheets are handled; text inside quoted
//! strings is never treated as a reference.
use std::collections::HashMap;

use crate::error::OfficeResult;
use crate::model::Sheet;
use crate::xml::parse_xml;
use crate::zip::ZipReader;

/// Path of the sheet part for the `index`-th `<sheet>` entry in the workbook.
fn sheet_part(reader: &ZipReader, index: usize) -> Option<String> {
    let workbook = parse_xml(&reader.read_text("xl/workbook.xml").ok()?).ok()?;
    let mut collections = Vec::new();
    workbook.find_all("sheets", &mut collections);
    let collection = collections.first()?;
    let sheet = collection.children_named("sheet").nth(index)?;
    let relation = sheet.attr_any_ns("id")?.to_string();
    let rels = parse_xml(&reader.read_text("xl/_rels/workbook.xml.rels").ok()?).ok()?;
    let target = rels
        .children_named("Relationship")
        .find(|node| node.attr("Id") == Some(relation.as_str()))
        .and_then(|node| node.attr("Target"))?;
    let cleaned = target.trim_start_matches('/');
    let path = if cleaned.starts_with("xl/") { cleaned.to_string() } else { format!("xl/{cleaned}") };
    Some(path)
}

/// Converts an A1-address to (row, column); both 0-based.
fn coordinates(address: &str) -> Option<(i64, i64)> {
    let mut column = 0i64;
    let mut row = String::new();
    let mut letters = String::new();
    for character in address.chars() {
        if character.is_ascii_alphabetic() {
            if !row.is_empty() {
                return None;
            }
            letters.push(character);
        } else if character.is_ascii_digit() {
            row.push(character);
        } else if character == '$' {
            continue;
        } else {
            return None;
        }
    }
    if letters.is_empty() || row.is_empty() {
        return None;
    }
    for (position, character) in letters.chars().enumerate() {
        column = column * 26 + (character.to_ascii_uppercase() as i64 - 'A' as i64 + 1);
        if position > 3 {
            return None;
        }
    }
    Some((row.parse::<i64>().ok()? - 1, column - 1))
}

fn column_name(mut column: i64) -> String {
    let mut out = String::new();
    column += 1;
    while column > 0 {
        let remainder = ((column - 1) % 26) as u8;
        out.insert(0, (b'A' + remainder) as char);
        column = (column - 1) / 26;
    }
    out
}

/// One A1 reference found in a formula, with the byte range it occupies.
struct Reference {
    start: usize,
    end: usize,
    column: i64,
    row: i64,
    absolute_column: bool,
    absolute_row: bool,
}

/// The A1 references in `formula`, skipping quoted strings and names.
fn references(formula: &str) -> Vec<Reference> {
    let bytes = formula.as_bytes();
    let mut out = Vec::new();
    let mut index = 0usize;
    let mut quoted = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'"' {
            quoted = !quoted;
            index += 1;
            continue;
        }
        if quoted {
            index += 1;
            continue;
        }
        if byte.is_ascii_alphabetic() || byte == b'$' {
            let start = index;
            if index > 0
                && (bytes[index - 1].is_ascii_alphanumeric() || bytes[index - 1] == b'_' || bytes[index - 1] == b'.')
            {
                index += 1;
                continue;
            }
            let mut cursor = index;
            let mut absolute_column = false;
            if bytes.get(cursor) == Some(&b'$') {
                absolute_column = true;
                cursor += 1;
            }
            let letters_start = cursor;
            while cursor < bytes.len() && bytes[cursor].is_ascii_alphabetic() {
                cursor += 1;
            }
            let letters = &formula[letters_start..cursor];
            if letters.is_empty() || letters.len() > 3 {
                index += 1;
                continue;
            }
            let mut absolute_row = false;
            if bytes.get(cursor) == Some(&b'$') {
                absolute_row = true;
                cursor += 1;
            }
            let digits_start = cursor;
            while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
                cursor += 1;
            }
            let digits = &formula[digits_start..cursor];
            // A function name (SUM( ) or a sheet qualifier (Sheet1!A1) is not a
            // reference: the first needs "(" after it, the second needs "!".
            let next = formula[cursor..].chars().next();
            if digits.is_empty() || next == Some('(') || next == Some('!') {
                index += 1;
                continue;
            }
            let text = &formula[start..cursor];
            if let Some((row, column)) = coordinates(text) {
                out.push(Reference { start, end: cursor, column, row, absolute_column, absolute_row });
            }
            index = cursor;
            continue;
        }
        index += 1;
    }
    out
}

/// Shifts the relative references of `formula` by the given offsets.
pub fn shift_formula(formula: &str, row_delta: i64, column_delta: i64) -> String {
    let mut out = String::with_capacity(formula.len() + 16);
    let mut cursor = 0usize;
    for reference in references(formula) {
        out.push_str(&formula[cursor..reference.start]);
        let column = if reference.absolute_column { reference.column } else { reference.column + column_delta };
        let row = if reference.absolute_row { reference.row } else { reference.row + row_delta };
        if column < 0 || row < 0 {
            out.push_str(&formula[reference.start..reference.end]);
        } else {
            if reference.absolute_column {
                out.push('$');
            }
            out.push_str(&column_name(column));
            if reference.absolute_row {
                out.push('$');
            }
            out.push_str(&(row + 1).to_string());
        }
        cursor = reference.end;
    }
    out.push_str(&formula[cursor..]);
    out
}

/// Expands the shared formulas of one sheet into `sheet`, leaving explicit
/// formulas untouched. Returns how many formulas were expanded.
pub fn apply(workbook: &[u8], sheet_index: usize, sheet: &mut Sheet) -> OfficeResult<usize> {
    let reader = ZipReader::open(workbook.to_vec())?;
    let Some(path) = sheet_part(&reader, sheet_index) else {
        return Ok(0);
    };
    let Ok(text) = reader.read_text(&path) else {
        return Ok(0);
    };
    let Ok(root) = parse_xml(&text) else {
        return Ok(0);
    };
    // The base formula of every shared group, keyed by `si`.
    let mut bases: HashMap<String, (String, String)> = HashMap::new();
    let mut followers: Vec<(String, String)> = Vec::new();
    let mut nodes = Vec::new();
    root.find_all("c", &mut nodes);
    for cell in nodes {
        let Some(address) = cell.attr("r").map(str::to_string) else {
            continue;
        };
        let Some(formula) = cell.child("f") else {
            continue;
        };
        if formula.attr("t") != Some("shared") {
            continue;
        }
        let Some(group) = formula.attr("si").map(str::to_string) else {
            continue;
        };
        let text = formula.direct_text();
        if text.trim().is_empty() {
            followers.push((address, group));
        } else {
            bases.insert(group, (address, text));
        }
    }
    let mut expanded = 0usize;
    for (address, group) in followers {
        let Some((base_address, base_formula)) = bases.get(&group) else {
            continue;
        };
        let (Some((base_row, base_column)), Some((row, column))) = (coordinates(base_address), coordinates(&address))
        else {
            continue;
        };
        let shifted = shift_formula(base_formula, row - base_row, column - base_column);
        let mut cell = sheet.get(&address).cloned().unwrap_or_default();
        cell.formula = Some(crate::formula::from_xlsx(&shifted));
        sheet.set(&address, cell);
        expanded += 1;
    }
    Ok(expanded)
}

/// Expands the shared formulas of every sheet; the sheets must be in workbook
/// order, which is the order `calamine::sheet_names()` returns.
pub fn apply_all(workbook: &[u8], sheets: &mut [Sheet]) -> OfficeResult<usize> {
    let mut expanded = 0usize;
    for (index, sheet) in sheets.iter_mut().enumerate() {
        expanded += apply(workbook, index, sheet)?;
    }
    Ok(expanded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shifts_relative_references_only() {
        assert_eq!(shift_formula("A1*2", 3, 0), "A4*2");
        assert_eq!(shift_formula("B2+C3", 1, 1), "C3+D4");
        assert_eq!(shift_formula("$A$1+B2", 1, 1), "$A$1+C3");
        assert_eq!(shift_formula("$A1+A$1", 2, 1), "$A3+B$1");
        assert_eq!(shift_formula("SUM(A1:A10)", 5, 0), "SUM(A6:A15)");
        assert_eq!(shift_formula("Sheet2!A1", 1, 0), "Sheet2!A2");
    }

    #[test]
    fn leaves_quoted_text_and_function_names_alone() {
        assert_eq!(shift_formula("\"A1 text\"&B2", 1, 0), "\"A1 text\"&B3");
        assert_eq!(shift_formula("LOG10(B2)", 1, 0), "LOG10(B3)");
        assert_eq!(shift_formula("Sales[Amount]+B2", 1, 0), "Sales[Amount]+B3");
    }

    #[test]
    fn parses_coordinates() {
        assert_eq!(coordinates("A1"), Some((0, 0)));
        assert_eq!(coordinates("$AB$12"), Some((11, 27)));
        assert_eq!(coordinates("A"), None);
        assert_eq!(coordinates("1A"), None);
    }
}
