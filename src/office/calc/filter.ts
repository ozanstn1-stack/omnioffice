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
import { addressesInRange, formatAddress, parseAddress, parseRange, toText, type Scalar } from "./formula";
import { tableColumnBodyRange } from "./structured";

export type ConditionOp =
  | "none"
  | "contains"
  | "begins"
  | "ends"
  | "equals"
  | "greater"
  | "less"
  | "between"
  | "top"
  | "bottom"
  | "blank"
  | "nonblank";

/**
 * A condition beside the value list. The inputs are kept as typed; `value`
 * is the text, the number, the lower bound or the item count depending on the
 * operator, `value2` the upper bound of "between".
 */
export interface FilterCondition {
  op: ConditionOp;
  value: string;
  value2: string;
}

export const NO_CONDITION: FilterCondition = { op: "none", value: "", value2: "" };

/** What the filter dialog edits: one column, its distinct values and a condition. */
export interface FilterDraft {
  col: number;
  values: Array<{ value: string; checked: boolean }>;
  condition: FilterCondition;
  tableId?: string;
  tableName?: string;
}

function parseNumber(text: string): number {
  const trimmed = text.trim().replace(",", ".");
  return trimmed === "" ? Number.NaN : Number(trimmed);
}

/**
 * The test a condition applies to one cell value. `column` holds every value
 * of the filtered column, which Top / Bottom N need to find their cut-off.
 * A condition whose inputs are missing or not numbers filters nothing, so the
 * dialog never hides rows for a half-typed condition. Text conditions ignore
 * case; number conditions match numbers only; ties at the cut-off stay in.
 */
export function conditionPredicate(condition: FilterCondition, column: readonly Scalar[]): (value: Scalar) => boolean {
  const all = () => true;
  const text = condition.value.toLowerCase();
  const isBlank = (value: Scalar) => value === "" || value === undefined;
  const asText = (value: Scalar) => toText(value).toLowerCase();
  switch (condition.op) {
    case "contains":
      return text === "" ? all : (value) => asText(value).includes(text);
    case "begins":
      return text === "" ? all : (value) => asText(value).startsWith(text);
    case "ends":
      return text === "" ? all : (value) => asText(value).endsWith(text);
    case "equals":
      return text === "" ? all : (value) => asText(value) === text;
    case "greater":
    case "less": {
      const limit = parseNumber(condition.value);
      if (Number.isNaN(limit)) return all;
      return condition.op === "greater"
        ? (value) => typeof value === "number" && value > limit
        : (value) => typeof value === "number" && value < limit;
    }
    case "between": {
      const first = parseNumber(condition.value);
      const second = parseNumber(condition.value2);
      if (Number.isNaN(first) || Number.isNaN(second)) return all;
      const low = Math.min(first, second);
      const high = Math.max(first, second);
      return (value) => typeof value === "number" && value >= low && value <= high;
    }
    case "top":
    case "bottom": {
      const count = Math.floor(parseNumber(condition.value));
      if (!(count >= 1)) return all;
      const numbers = column.filter((value): value is number => typeof value === "number");
      numbers.sort((a, b) => (condition.op === "top" ? b - a : a - b));
      const cutOff = numbers[Math.min(count, numbers.length) - 1];
      if (cutOff === undefined) return () => false;
      return condition.op === "top"
        ? (value) => typeof value === "number" && value >= cutOff
        : (value) => typeof value === "number" && value <= cutOff;
    }
    case "blank":
      return isBlank;
    case "nonblank":
      return (value) => !isBlank(value);
    default:
      return all;
  }
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
  return { col, values: draftValues(texts), condition: NO_CONDITION };
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
    condition: NO_CONDITION,
  };
}

/**
 * Hides the rows that fail the filter and shows the others again. A row stays
 * when its value is checked in the list AND it meets the condition. A
 * structured table filters its body only; the plain sheet filter takes the
 * first row of the used range as the header, which is never hidden (the value
 * list is built from the rows below it). The remembered filter
 * lists the values of the rows that stay (the condition itself is not stored).
 */
export function applyFilterDraft(sheet: Sheet, draft: FilterDraft, computed: ReadonlyMap<string, Scalar>): Sheet {
  const allowed = new Set(draft.values.filter((entry) => entry.checked).map((entry) => entry.value));
  const table = draft.tableId ? (sheet.tables ?? []).find((candidate) => candidate.id === draft.tableId) : undefined;
  const parts = parseRange(table ? table.range : usedRange(sheet));
  if (!parts) return sheet;
  const startRow = table ? parts.start.row + (table.hasHeaders ? 1 : 0) : parts.start.row + 1;
  const endRow = table ? parts.end.row - (table.hasTotals ? 1 : 0) : parts.end.row;
  const conditional = draft.condition.op !== "none";
  let passes: (value: Scalar) => boolean = () => true;
  if (conditional) {
    const column: Scalar[] = [];
    for (let row = startRow; row <= endRow; row += 1) column.push(computed.get(formatAddress(row, draft.col)) ?? "");
    passes = conditionPredicate(draft.condition, column);
  }
  const shown = new Set<string>();
  let rowHeights = sheet.rowHeights;
  for (let row = startRow; row <= endRow; row += 1) {
    const address = formatAddress(row, draft.col);
    const scalar = computed.get(address) ?? "";
    const value = String(scalar);
    const position = parseAddress(address);
    if (!position) continue;
    const hidden = !allowed.has(value) || !passes(scalar);
    if (hidden) {
      rowHeights = { ...rowHeights, [position.row]: 0 };
    } else {
      shown.add(value);
      if (rowHeights[position.row] === 0) {
        rowHeights = { ...rowHeights };
        delete rowHeights[position.row];
      }
    }
  }
  const values = conditional ? [...shown] : [...allowed];
  if (table) {
    return {
      ...sheet,
      rowHeights,
      tables: (sheet.tables ?? []).map((candidate) =>
        candidate.id === table.id
          ? { ...candidate, filter: { range: table.range, column: draft.col, values } }
          : candidate,
      ),
    };
  }
  return { ...sheet, rowHeights, filter: { range: usedRange(sheet), column: draft.col, values } };
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
