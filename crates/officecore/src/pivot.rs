//! Pivot table materialization.
//!
//! The editor computes pivots live from their definitions; the exporter cannot
//! run that code, so this module implements the same aggregation over the
//! cached cell values and produces the grid the XLSX writer materialises at
//! the pivot's anchor. Keeping the two engines aligned is covered by tests on
//! both sides with the same fixture.
use crate::address::{format, parse, parse_range};
use crate::model::*;

struct Record {
    row: Vec<String>,
    column: Vec<String>,
    values: Vec<CellValue>,
}

fn cell_text(value: &CellValue) -> String {
    match value {
        CellValue::Empty => String::new(),
        CellValue::Text(text) => text.clone(),
        CellValue::Bool(flag) => if *flag { "TRUE" } else { "FALSE" }.into(),
        CellValue::Error(code) => code.clone(),
        CellValue::Number(number) => {
            if number.fract() == 0.0 && number.abs() < 1e15 {
                format!("{}", *number as i64)
            } else {
                format!("{number}")
            }
        }
    }
}

fn numeric(value: &CellValue) -> Option<f64> {
    match value {
        CellValue::Number(number) => Some(*number),
        CellValue::Bool(flag) => Some(if *flag { 1.0 } else { 0.0 }),
        CellValue::Text(text) => {
            let cleaned = text.trim().replace(',', ".");
            if cleaned.is_empty() {
                return None;
            }
            cleaned.parse::<f64>().ok().filter(|value| value.is_finite())
        }
        _ => None,
    }
}

/// Numeric-aware, case-insensitive ordering, mirroring the TS `compareScalars`.
fn compare_keys(left: &str, right: &str) -> std::cmp::Ordering {
    let left_number = left.parse::<f64>();
    let right_number = right.parse::<f64>();
    if let (Ok(a), Ok(b)) = (left_number, right_number) {
        return a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal);
    }
    left.to_uppercase().cmp(&right.to_uppercase())
}

fn distinct_keys(keys: &[Vec<String>]) -> Vec<Vec<String>> {
    let mut seen: Vec<Vec<String>> = Vec::new();
    for key in keys {
        if !seen.iter().any(|existing| existing == key) {
            seen.push(key.clone());
        }
    }
    seen.sort_by(|left, right| {
        for index in 0..left.len().max(right.len()) {
            let comparison = compare_keys(left.get(index).map(String::as_str).unwrap_or(""), right.get(index).map(String::as_str).unwrap_or(""));
            if comparison != std::cmp::Ordering::Equal {
                return comparison;
            }
        }
        std::cmp::Ordering::Equal
    });
    seen
}

fn aggregate(records: &[&Record], index: usize, aggregation: &str) -> CellValue {
    if aggregation == "count" {
        return CellValue::Number(records.iter().filter(|record| !matches!(record.values.get(index), None | Some(CellValue::Empty))).count() as f64);
    }
    let numbers: Vec<f64> = records.iter().filter_map(|record| record.values.get(index).and_then(numeric)).collect();
    if numbers.is_empty() {
        return CellValue::Empty;
    }
    let total: f64 = numbers.iter().sum();
    match aggregation {
        "sum" => CellValue::Number(total),
        "average" => CellValue::Number(total / numbers.len() as f64),
        "min" => CellValue::Number(numbers.iter().copied().fold(f64::INFINITY, f64::min)),
        "max" => CellValue::Number(numbers.iter().copied().fold(f64::NEG_INFINITY, f64::max)),
        _ => CellValue::Empty,
    }
}

/// The pivot grid as rows of values, or `None` when the definition cannot be
/// read. Layout matches the TypeScript engine: header row(s), then body rows
/// with the row-field labels in front.
pub fn compute(workbook: &Workbook, pivot: &PivotTable) -> Option<Vec<Vec<CellValue>>> {
    let sheet = workbook.sheets.iter().find(|candidate| candidate.name == pivot.source_sheet)?;
    let ((start_row, start_col), (end_row, end_col)) = parse_range(&pivot.source)?;
    if pivot.values.is_empty() || (pivot.rows.is_empty() && pivot.columns.is_empty()) {
        return None;
    }
    let value_at = |row: u32, column: u32| -> CellValue {
        sheet.get(&format(row, column)).map(|cell| cell.value.clone()).unwrap_or(CellValue::Empty)
    };
    let field_text = |row: u32, column: u32| -> String {
        let text = cell_text(&value_at(row, column)).trim().to_string();
        if text.is_empty() { "(blank)".to_string() } else { text }
    };

    let fields: Vec<String> = (start_col..=end_col)
        .map(|column| {
            let text = cell_text(&value_at(start_row, column)).trim().to_string();
            if text.is_empty() { format!("Column {}", column + 1) } else { text }
        })
        .collect();
    let index_of = |field: &str| fields.iter().position(|candidate| candidate.eq_ignore_ascii_case(field));
    let value_indexes: Vec<Option<usize>> = pivot.values.iter().map(|entry| index_of(&entry.field)).collect();
    let row_indexes: Vec<Option<usize>> = pivot.rows.iter().map(|field| index_of(field)).collect();
    let column_indexes: Vec<Option<usize>> = pivot.columns.iter().map(|field| index_of(field)).collect();
    if value_indexes.iter().chain(row_indexes.iter()).chain(column_indexes.iter()).any(Option::is_none) {
        return None;
    }
    let value_indexes: Vec<usize> = value_indexes.into_iter().flatten().collect();
    let row_indexes: Vec<usize> = row_indexes.into_iter().flatten().collect();
    let column_indexes: Vec<usize> = column_indexes.into_iter().flatten().collect();

    let mut records: Vec<Record> = Vec::new();
    for row in start_row + 1..=end_row {
        let record = Record {
            row: row_indexes.iter().map(|offset| field_text(row, start_col + *offset as u32)).collect(),
            column: column_indexes.iter().map(|offset| field_text(row, start_col + *offset as u32)).collect(),
            values: value_indexes.iter().map(|offset| value_at(row, start_col + *offset as u32)).collect(),
        };
        let blank = record.row.iter().all(|value| value == "(blank)")
            && record.column.iter().all(|value| value == "(blank)")
            && record.values.iter().all(|value| cell_text(value).is_empty());
        if blank {
            continue;
        }
        let keep = pivot.filters.iter().all(|filter| {
            if filter.values.is_empty() {
                return true;
            }
            match index_of(&filter.field) {
                Some(offset) => {
                    let text = field_text(row, start_col + offset as u32);
                    filter.values.iter().any(|value| value.eq_ignore_ascii_case(&text))
                }
                None => true,
            }
        });
        if keep {
            records.push(record);
        }
    }

    let row_keys = distinct_keys(&records.iter().map(|record| record.row.clone()).collect::<Vec<_>>());
    let column_keys = if pivot.columns.is_empty() {
        vec![Vec::new()]
    } else {
        distinct_keys(&records.iter().map(|record| record.column.clone()).collect::<Vec<_>>())
    };
    let value_count = pivot.values.len();

    let mut grid: Vec<Vec<CellValue>> = Vec::new();
    let corner = if pivot.rows.is_empty() { "Pivot".to_string() } else { pivot.rows.join(" / ") };
    if !pivot.columns.is_empty() {
        let mut header: Vec<CellValue> = vec![CellValue::Text(corner)];
        for key in &column_keys {
            for _ in 0..value_count {
                header.push(CellValue::Text(key.join(" / ")));
            }
        }
        grid.push(header);
        if value_count > 1 {
            let mut labels: Vec<CellValue> = vec![CellValue::Empty];
            for _ in &column_keys {
                for entry in &pivot.values {
                    labels.push(CellValue::Text(format!("{} ({})", entry.field, entry.aggregation)));
                }
            }
            grid.push(labels);
        }
    } else {
        let mut header: Vec<CellValue> = vec![CellValue::Text(corner)];
        for entry in &pivot.values {
            header.push(CellValue::Text(format!("{} ({})", entry.field, entry.aggregation)));
        }
        grid.push(header);
    }

    for row_key in &row_keys {
        let mut line: Vec<CellValue> = row_key.iter().cloned().map(CellValue::Text).collect();
        for column_key in &column_keys {
            let matching: Vec<&Record> = records
                .iter()
                .filter(|record| &record.row == row_key && &record.column == column_key)
                .collect();
            for (value_index, entry) in pivot.values.iter().enumerate() {
                line.push(if matching.is_empty() { CellValue::Empty } else { aggregate(&matching, value_index, &entry.aggregation) });
            }
        }
        grid.push(line);
    }
    if grid.len() <= 1 {
        return None;
    }
    Some(grid)
}

/// `(address, value)` pairs for every non-empty pivot cell, ready to be merged
/// into the worksheet the exporter writes.
pub fn materialize(workbook: &Workbook, sheet: &Sheet) -> Vec<(String, CellValue)> {
    let mut out = Vec::new();
    for pivot in &sheet.pivot_tables {
        let Some(grid) = compute(workbook, pivot) else { continue };
        let Some((anchor_row, anchor_col)) = parse(&pivot.anchor) else { continue };
        for (row_offset, line) in grid.iter().enumerate() {
            for (col_offset, value) in line.iter().enumerate() {
                if matches!(value, CellValue::Empty) {
                    continue;
                }
                out.push((format(anchor_row + row_offset as u32, anchor_col + col_offset as u32), value.clone()));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data_workbook() -> Workbook {
        let mut workbook = Workbook::default();
        workbook.sheets[0].name = "Data".into();
        let sheet = &mut workbook.sheets[0];
        let rows = [
            ["Department", "Year", "Sales", "Region"],
            ["Hardware", "2025", "100", "North"],
            ["Hardware", "2025", "150", "South"],
            ["Software", "2025", "200", "North"],
            ["Hardware", "2026", "50", "North"],
            ["Software", "2026", "300", "South"],
        ];
        for (row, line) in rows.iter().enumerate() {
            for (column, value) in line.iter().enumerate() {
                let numeric = value.parse::<f64>().is_ok();
                sheet.set(
                    &format(row as u32, column as u32),
                    Cell {
                        value: if numeric { CellValue::Number(value.parse().unwrap()) } else { CellValue::Text((*value).into()) },
                        ..Default::default()
                    },
                );
            }
        }
        workbook
    }

    fn pivot(overrides: impl FnOnce(&mut PivotTable)) -> PivotTable {
        let mut pivot = PivotTable {
            id: "p1".into(),
            name: "Pivot".into(),
            source_sheet: "Data".into(),
            source: "A1:D6".into(),
            rows: vec!["Department".into()],
            columns: vec!["Year".into()],
            values: vec![PivotValueField { field: "Sales".into(), aggregation: "sum".into() }],
            filters: vec![],
            anchor: "F1".into(),
        };
        overrides(&mut pivot);
        pivot
    }

    fn text_grid(grid: &[Vec<CellValue>]) -> Vec<Vec<String>> {
        grid.iter().map(|row| row.iter().map(cell_text).collect()).collect()
    }

    #[test]
    fn groups_rows_and_columns_like_the_editor() {
        let workbook = data_workbook();
        let grid = compute(&workbook, &pivot(|_| {})).unwrap();
        assert_eq!(
            text_grid(&grid),
            vec![
                vec!["Department", "2025", "2026"],
                vec!["Hardware", "250", "50"],
                vec!["Software", "200", "300"],
            ]
        );
    }

    #[test]
    fn applies_filters_and_aggregations() {
        let workbook = data_workbook();
        let filtered = compute(&workbook, &pivot(|pivot| {
            pivot.filters = vec![PivotFilter { field: "Region".into(), values: vec!["North".into()] }];
        }))
        .unwrap();
        assert_eq!(
            text_grid(&filtered),
            vec![
                vec!["Department", "2025", "2026"],
                vec!["Hardware", "100", "50"],
                vec!["Software", "200", ""],
            ]
        );
        let counted = compute(&workbook, &pivot(|pivot| {
            pivot.columns = vec![];
            pivot.values = vec![PivotValueField { field: "Sales".into(), aggregation: "count".into() }];
        }))
        .unwrap();
        assert_eq!(
            text_grid(&counted),
            vec![
                vec!["Department", "Sales (count)"],
                vec!["Hardware", "3"],
                vec!["Software", "2"],
            ]
        );
    }

    #[test]
    fn materializes_cells_at_the_anchor() {
        let workbook = data_workbook();
        let sheet = workbook.sheets[0].clone();
        let sheet = Sheet { pivot_tables: vec![pivot(|_| {})], ..sheet };
        let cells = materialize(&workbook, &sheet);
        assert!(cells.contains(&("F1".to_string(), CellValue::Text("Department".into()))));
        assert!(cells.contains(&("G2".to_string(), CellValue::Number(250.0))));
        assert!(cells.contains(&("H3".to_string(), CellValue::Number(300.0))));
        assert_eq!(cells.len(), 9);
    }

    #[test]
    fn rejects_unreadable_definitions() {
        let workbook = data_workbook();
        assert!(compute(&workbook, &pivot(|pivot| pivot.values.clear())).is_none());
        assert!(compute(&workbook, &pivot(|pivot| { pivot.rows.clear(); pivot.columns.clear(); })).is_none());
        assert!(compute(&workbook, &pivot(|pivot| pivot.source = "nonsense".into())).is_none());
    }
}
