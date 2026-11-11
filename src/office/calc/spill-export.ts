/**
 * The workbook as it is saved to XLSX: every formula that spills carries the
 * range it spills into (4.2b), which the writer turns into Excel's dynamic
 * array markup. The open document is never changed; only the saved copy is.
 */
import type { Workbook } from "../../lib/office-types";
import { formatAddress, parseAddress } from "./addresses";
import { spillTargetsOf } from "./cells";

/** The `A1:B2` range a spilled formula covers, or null when it does not spill. */
export function spillRange(workbook: Workbook, sheetName: string, address: string): string | null {
  const targets = spillTargetsOf(workbook, sheetName, address);
  const anchor = parseAddress(address);
  if (!anchor || targets.length === 0) return null;
  let bottom = anchor.row;
  let right = anchor.col;
  for (const key of targets) {
    const target = parseAddress(key.slice(key.lastIndexOf("!") + 1));
    if (!target) continue;
    bottom = Math.max(bottom, target.row);
    right = Math.max(right, target.col);
  }
  return `${address}:${formatAddress(bottom, right)}`;
}

/** A copy of the workbook whose spilling formulas carry their spill range. */
export function withSpillRanges(workbook: Workbook): Workbook {
  let changed = false;
  const sheets = workbook.sheets.map((sheet) => {
    let cells: typeof sheet.cells | null = null;
    for (const [address, cell] of Object.entries(sheet.cells)) {
      if (!cell.formula) continue;
      const spill = spillRange(workbook, sheet.name, address);
      if (!spill && !cell.spill) continue;
      cells ??= { ...sheet.cells };
      cells[address] = { ...cell, spill };
    }
    if (!cells) return sheet;
    changed = true;
    return { ...sheet, cells };
  });
  return changed ? { ...workbook, sheets } : workbook;
}
