/**
 * Calc diff (C13): sheets added/removed and one entry per changed cell. A cell
 * is compared by its formula text (prefixed with `=`) or, when it has no
 * formula, by its displayed text, so a formula edit is reported even when the
 * cached value happens to be unchanged.
 */
import { cellText, type Sheet, type Workbook } from "./office-types";
import {
  countKind,
  emptySummary,
  type CalcCellChange,
  type CalcDiff,
  type DiffKind,
  type DiffSummary,
} from "./office-diff-types";

/** Bound so a pathological pair cannot exhaust memory in the webview. */
export const MAX_CALC_CHANGES = 5_000;

function cellDisplay(sheet: Sheet, address: string): string {
  const cell = sheet.cells[address];
  if (!cell) return "";
  if (cell.formula) return `=${cell.formula}`;
  return cellText(cell);
}

function sheetAddresses(sheet: Sheet): string[] {
  return Object.keys(sheet.cells).filter((address) => cellDisplay(sheet, address) !== "");
}

/** Sheet and cell diff; a sheet present on only one side is added/removed. */
export function diffWorkbooks(base: Workbook, current: Workbook): CalcDiff {
  const baseSheets = new Map(base.sheets.map((sheet) => [sheet.name, sheet]));
  const currentSheets = new Map(current.sheets.map((sheet) => [sheet.name, sheet]));
  const sheetsAdded = [...currentSheets.keys()].filter((name) => !baseSheets.has(name));
  const sheetsRemoved = [...baseSheets.keys()].filter((name) => !currentSheets.has(name));
  const cells: CalcCellChange[] = [];
  const summary: DiffSummary = emptySummary();
  let truncated = false;
  const push = (cell: CalcCellChange) => {
    if (cells.length >= MAX_CALC_CHANGES) {
      truncated = true;
      return;
    }
    cells.push(cell);
  };
  for (const added of sheetsAdded) {
    summary.added += 1;
    const sheet = currentSheets.get(added)!;
    for (const address of sheetAddresses(sheet)) {
      push({ sheet: added, address, kind: "added", before: "", after: cellDisplay(sheet, address) });
    }
  }
  for (const removed of sheetsRemoved) {
    summary.removed += 1;
    const sheet = baseSheets.get(removed)!;
    for (const address of sheetAddresses(sheet)) {
      push({ sheet: removed, address, kind: "removed", before: cellDisplay(sheet, address), after: "" });
    }
  }
  for (const [name, baseSheet] of baseSheets) {
    const currentSheet = currentSheets.get(name);
    if (!currentSheet) continue;
    const addresses = new Set([...sheetAddresses(baseSheet), ...sheetAddresses(currentSheet)]);
    for (const address of [...addresses].sort()) {
      const before = cellDisplay(baseSheet, address);
      const after = cellDisplay(currentSheet, address);
      if (before === after) continue;
      const kind: DiffKind = before === "" ? "added" : after === "" ? "removed" : "changed";
      push({ sheet: name, address, kind, before, after });
      countKind(summary, kind);
    }
  }
  return { kind: "calc", summary, sheetsAdded, sheetsRemoved, cells, truncated };
}
