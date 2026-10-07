/**
 * Column filters for the Calc grid.
 *
 * A filter hides rows the way the grid already understands: a hidden row has
 * height 0 in `sheet.rowHeights`, and the sheet (or the table) remembers the
 * checked values in its `filter` state. These functions are pure over the
 * model so the dialog and the tests share one implementation.
 */
import type { Sheet, SpreadsheetTable } from "../../lib/office-types";
import { usedRange } from "./cells";
import { addressesInRange, formatAddress, parseAddress, parseRange, type Scalar } from "./formula";
import { tableColumnBodyRange } from "./structured";

/** What the filter dialog edits: one column and its distinct values. */
export interface FilterDraft {
  col: number;
  values: Array<{ value: string; checked: boolean }>;
  tableId?: string;
  tableName?: string;
}

function draftValues(texts: Iterable<string>): FilterDraft["values"] {
  const values = new Map<string, boolean>();
  for (const text of texts) if (!values.has(text)) values.set(text, true);
  return [...values.entries()].map(([value, checked]) => ({ value, checked }));
}

/** The value list of a sheet column below the header row of the used range. */
export function sheetFilterDraft(sheet: Sheet, computed: ReadonlyMap<string, Scalar>, col: number): FilterDraft | null {
  const parts = parseRange(usedRange(sheet));
  if (!parts) return null;
  const texts: string[] = [];
  for (let row = parts.start.row + 1; row <= parts.end.row; row += 1) {
    texts.push(String(computed.get(formatAddress(row, col)) ?? ""));
  }
  return { col, values: draftValues(texts) };
}

/** The value list of one structured table column (its body rows only). */
export function tableFilterDraft(
  table: SpreadsheetTable,
  computed: ReadonlyMap<string, Scalar>,
  columnName: string,
): FilterDraft | null {
  const parts = parseRange(table.range);
  const columnIndex = table.columns.findIndex((column) => column.name === columnName);
  if (!parts || columnIndex < 0) return null;
  const body = tableColumnBodyRange(table, columnName);
  const texts = body ? addressesInRange(`${body.start}:${body.end}`).map((a) => String(computed.get(a) ?? "")) : [];
  return {
    col: parts.start.col + columnIndex,
    tableId: table.id,
    tableName: table.name,
    values: draftValues(texts),
  };
}

/**
 * Hides the rows whose value in the draft's column is not checked and shows
 * the others again. A structured table filters its body only; the plain sheet
 * filter keeps its original behaviour of scanning the whole used range.
 */
export function applyFilterDraft(sheet: Sheet, draft: FilterDraft, computed: ReadonlyMap<string, Scalar>): Sheet {
  const allowed = new Set(draft.values.filter((entry) => entry.checked).map((entry) => entry.value));
  const table = draft.tableId ? (sheet.tables ?? []).find((candidate) => candidate.id === draft.tableId) : undefined;
  const parts = parseRange(table ? table.range : usedRange(sheet));
  if (!parts) return sheet;
  const startRow = table ? parts.start.row + (table.hasHeaders ? 1 : 0) : parts.start.row;
  const endRow = table ? parts.end.row - (table.hasTotals ? 1 : 0) : parts.end.row;
  let rowHeights = sheet.rowHeights;
  for (let row = startRow; row <= endRow; row += 1) {
    const address = formatAddress(row, draft.col);
    const value = String(computed.get(address) ?? "");
    const position = parseAddress(address);
    if (!position) continue;
    const hidden = !allowed.has(value);
    if (hidden) {
      rowHeights = { ...rowHeights, [position.row]: 0 };
    } else if (rowHeights[position.row] === 0) {
      rowHeights = { ...rowHeights };
      delete rowHeights[position.row];
    }
  }
  if (table) {
    return {
      ...sheet,
      rowHeights,
      tables: (sheet.tables ?? []).map((candidate) =>
        candidate.id === table.id
          ? { ...candidate, filter: { range: table.range, column: draft.col, values: [...allowed] } }
          : candidate,
      ),
    };
  }
  return { ...sheet, rowHeights, filter: { range: usedRange(sheet), column: draft.col, values: [...allowed] } };
}

/** Removes the filter (of one table, or of the sheet) and shows every row. */
export function clearFilter(sheet: Sheet, tableId?: string): Sheet {
  if (!tableId) return { ...sheet, rowHeights: {}, filter: null };
  return {
    ...sheet,
    rowHeights: {},
    tables: (sheet.tables ?? []).map((table) => (table.id === tableId ? { ...table, filter: null } : table)),
  };
}
