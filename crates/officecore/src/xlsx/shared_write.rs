//! Shared-formula grouping for XLSX exports.
//!
//! Consecutive identical filled-down or filled-right formulas (`>= 2` cells)
//! are emitted as OOXML `<f t="shared" ref="A2:A5" si="0">...</f>` on the
//! top-left anchor cell and `<f t="shared" si="0"/>` on every follower cell,
//! matching what [`super::shared_formulas`] expands back on import.
//!
//! Dynamic-array spilling formulas (`Cell::spill`) keep plain `<f>` markup so
//! `spill_write::dynamic_array` upgrades them to `<f t="array" ref="...">`.

use super::shared_formulas::shift_formula;
use super::spill_write::spill_range;
use crate::model::{Cell, Sheet};
use crate::xml::escape_text;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
enum SharedEntry {
    Anchor { end_row: u32, end_col: u32, group: usize, xlsx_formula: String },
    Follower { group: usize },
}

/// Worksheet-local `t="shared"` plan mapping `(row, column)` to its XML role.
#[derive(Debug, Default)]
pub(super) struct SharedFormulaPlan {
    entries: BTreeMap<(u32, u32), SharedEntry>,
}

fn group_runs(
    candidates: &BTreeMap<(u32, u32), String>,
    claimed: &mut BTreeSet<(u32, u32)>,
    entries: &mut BTreeMap<(u32, u32), SharedEntry>,
    next_group: &mut usize,
    vertical: bool,
) {
    let mut keys: Vec<(u32, u32)> = candidates.keys().copied().collect();
    if vertical {
        keys.sort_by_key(|&(row, col)| (col, row));
    }
    for (start_row, start_col) in keys {
        if claimed.contains(&(start_row, start_col)) {
            continue;
        }
        let Some(anchor_formula) = candidates.get(&(start_row, start_col)) else {
            continue;
        };
        let mut len = 1u32;
        loop {
            let (next_row, next_col) = if vertical {
                let Some(row) = start_row.checked_add(len) else {
                    break;
                };
                (row, start_col)
            } else {
                let Some(col) = start_col.checked_add(len) else {
                    break;
                };
                (start_row, col)
            };
            if claimed.contains(&(next_row, next_col)) {
                break;
            }
            let Some(candidate) = candidates.get(&(next_row, next_col)) else {
                break;
            };
            let (row_delta, col_delta) = if vertical { (i64::from(len), 0) } else { (0, i64::from(len)) };
            if shift_formula(anchor_formula, row_delta, col_delta) != *candidate {
                break;
            }
            len += 1;
        }
        if len < 2 {
            continue;
        }
        let group = *next_group;
        *next_group += 1;
        let (end_row, end_col) =
            if vertical { (start_row + len - 1, start_col) } else { (start_row, start_col + len - 1) };
        claimed.insert((start_row, start_col));
        entries.insert(
            (start_row, start_col),
            SharedEntry::Anchor { end_row, end_col, group, xlsx_formula: anchor_formula.clone() },
        );
        for offset in 1..len {
            let pos = if vertical { (start_row + offset, start_col) } else { (start_row, start_col + offset) };
            claimed.insert(pos);
            entries.insert(pos, SharedEntry::Follower { group });
        }
    }
}

/// Finds contiguous vertical then horizontal shared-formula runs in `sheet`.
pub(super) fn plan(sheet: &Sheet) -> SharedFormulaPlan {
    let mut candidates: BTreeMap<(u32, u32), String> = BTreeMap::new();
    for (address, cell) in &sheet.cells {
        if cell.is_empty() || spill_range(cell).is_some() {
            continue;
        }
        let Some(raw) = cell.formula.as_deref().map(str::trim).filter(|text| !text.is_empty()) else {
            continue;
        };
        let Some((row, col)) = crate::address::parse(address) else {
            continue;
        };
        // Excel refuses the plain name of every function added after 2007.
        candidates.insert((row, col), crate::formula::to_xlsx(raw));
    }
    let mut entries = BTreeMap::new();
    if candidates.len() < 2 {
        return SharedFormulaPlan { entries };
    }
    let mut claimed = BTreeSet::new();
    let mut next_group = 0usize;
    group_runs(&candidates, &mut claimed, &mut entries, &mut next_group, true);
    group_runs(&candidates, &mut claimed, &mut entries, &mut next_group, false);
    SharedFormulaPlan { entries }
}

/// Renders the `<f ...>` XML element for one model cell using `plan`.
pub(super) fn formula_xml(plan: &SharedFormulaPlan, row: u32, column: u32, cell: &Cell) -> Option<String> {
    if let Some(entry) = plan.entries.get(&(row, column)) {
        return Some(match entry {
            SharedEntry::Anchor { end_row, end_col, group, xlsx_formula } => format!(
                "<f t=\"shared\" ref=\"{}:{}\" si=\"{group}\">{}</f>",
                crate::address::format(row, column),
                crate::address::format(*end_row, *end_col),
                escape_text(xlsx_formula)
            ),
            SharedEntry::Follower { group } => format!("<f t=\"shared\" si=\"{group}\"/>"),
        });
    }
    // Excel refuses the plain name of every function added after 2007.
    cell.formula.as_deref().map(|raw| format!("<f>{}</f>", escape_text(&crate::formula::to_xlsx(raw))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CellValue;

    fn formula_cell(formula: &str) -> Cell {
        Cell { value: CellValue::Number(1.0), formula: Some(formula.into()), ..Default::default() }
    }

    #[test]
    fn groups_vertical_and_horizontal_runs_sequentially() {
        let mut sheet = Sheet::new("Sheet1");
        sheet.set("B2", formula_cell("=A2*2"));
        sheet.set("B3", formula_cell("=A3*2"));
        sheet.set("B4", formula_cell("=A4*2"));
        sheet.set("D5", formula_cell("=SUM(D1:D4)"));
        sheet.set("E5", formula_cell("=SUM(E1:E4)"));
        sheet.set("G1", formula_cell("=99"));

        let shared_plan = plan(&sheet);
        assert_eq!(
            formula_xml(&shared_plan, 1, 1, sheet.get("B2").unwrap()).as_deref(),
            Some("<f t=\"shared\" ref=\"B2:B4\" si=\"0\">A2*2</f>")
        );
        assert_eq!(
            formula_xml(&shared_plan, 2, 1, sheet.get("B3").unwrap()).as_deref(),
            Some("<f t=\"shared\" si=\"0\"/>")
        );
        assert_eq!(
            formula_xml(&shared_plan, 3, 1, sheet.get("B4").unwrap()).as_deref(),
            Some("<f t=\"shared\" si=\"0\"/>")
        );
        assert_eq!(
            formula_xml(&shared_plan, 4, 3, sheet.get("D5").unwrap()).as_deref(),
            Some("<f t=\"shared\" ref=\"D5:E5\" si=\"1\">SUM(D1:D4)</f>")
        );
        assert_eq!(
            formula_xml(&shared_plan, 4, 4, sheet.get("E5").unwrap()).as_deref(),
            Some("<f t=\"shared\" si=\"1\"/>")
        );
        assert_eq!(formula_xml(&shared_plan, 0, 6, sheet.get("G1").unwrap()).as_deref(), Some("<f>99</f>"));
    }

    #[test]
    fn skips_spilling_formulas_and_empty_formulas() {
        let mut sheet = Sheet::new("Sheet1");
        sheet.set(
            "B2",
            Cell {
                value: CellValue::Number(1.0),
                formula: Some("=SEQUENCE(2)".into()),
                spill: Some("B2:B3".into()),
                ..Default::default()
            },
        );
        sheet.set(
            "B3",
            Cell {
                value: CellValue::Number(2.0),
                formula: Some("=SEQUENCE(2)".into()),
                spill: Some("B3:B4".into()),
                ..Default::default()
            },
        );
        sheet.set("C1", formula_cell("   "));
        sheet.set("C2", formula_cell("   "));

        let shared_plan = plan(&sheet);
        assert!(shared_plan.entries.is_empty());
        assert_eq!(
            formula_xml(&shared_plan, 1, 1, sheet.get("B2").unwrap()).as_deref(),
            Some("<f>_xlfn.SEQUENCE(2)</f>")
        );
    }
}
