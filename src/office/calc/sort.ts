/**
 * Multi-level sort for the Calc model.
 *
 * Rows of a range are rearranged by up to a few keys, each ascending or
 * descending. The order is Excel's: numbers, then text (without regard to
 * case), then logical values, then errors; empty cells go last whichever the
 * direction. The sort is stable, so equal rows keep their order. A moved
 * formula is shifted like a copy: its relative references follow it to the new
 * row, absolute ones stay. Hidden rows keep their place and the visible rows
 * are sorted among themselves, as a filtered list is in Excel.
 */
import type { Cell, Sheet } from "../../lib/office-types";
import { shiftFormulaRows, usedRange } from "./cells";
import { formatAddress, isError, parseAddress, parseRange, type Scalar } from "./formula";
import { isSheetProtected } from "./protection";

export interface SortRange {
  top: number;
  left: number;
  bottom: number;
  right: number;
}

export interface SortLevel {
  /** Absolute column index; it must lie inside the range. */
  column: number;
  ascending: boolean;
}

const collator = new Intl.Collator(undefined, { sensitivity: "accent" });

const BLANK = 5;

function rank(value: Scalar | undefined): number {
  if (value === undefined || value === "") return BLANK;
  if (typeof value === "number") return 0;
  if (typeof value === "string") return 1;
  if (typeof value === "boolean") return 2;
  return isError(value) ? 3 : 4;
}

/**
 * Negative when `left` sorts before `right`. Empty values compare after every
 * other value in both directions; descending reverses the other types and the
 * order inside each of them.
 */
export function compareValues(left: Scalar | undefined, right: Scalar | undefined, ascending: boolean): number {
  const leftRank = rank(left);
  const rightRank = rank(right);
  if (leftRank === BLANK || rightRank === BLANK) return leftRank === rightRank ? 0 : leftRank === BLANK ? 1 : -1;
  const direction = ascending ? 1 : -1;
  if (leftRank !== rightRank) return (leftRank - rightRank) * direction;
  let order = 0;
  if (typeof left === "number" && typeof right === "number") order = left - right;
  else if (typeof left === "string" && typeof right === "string") order = collator.compare(left, right);
  else if (typeof left === "boolean" && typeof right === "boolean") order = Number(left) - Number(right);
  else if (isError(left) && isError(right)) order = left.code < right.code ? -1 : left.code > right.code ? 1 : 0;
  return order * direction;
}

/**
 * The stable order of `rows` by the given keys: element `i` is the index of
 * the row that belongs at position `i`. `offset` indexes into a row.
 */
export function sortOrder(
  rows: readonly (readonly Scalar[])[],
  levels: readonly { offset: number; ascending: boolean }[],
): number[] {
  const order = rows.map((_, index) => index);
  order.sort((a, b) => {
    for (const level of levels) {
      const result = compareValues(rows[a][level.offset], rows[b][level.offset], level.ascending);
      if (result !== 0) return result;
    }
    return a - b;
  });
  return order;
}

/**
 * Whether the first row of a range looks like a header: every cell is
 * non-empty text and the row below holds a number or logical value somewhere.
 * An all-text block cannot tell header from data, so it stays "no header" and
 * the user decides.
 */
export function guessHeaders(first: readonly Scalar[], second: readonly Scalar[] | undefined): boolean {
  if (!second || first.length === 0) return false;
  if (!first.every((value) => typeof value === "string" && value !== "")) return false;
  return second.some((value) => typeof value === "number" || typeof value === "boolean");
}

/** True when a merged range touches the sort range: Excel refuses to sort those too. */
export function rangeHasMerges(sheet: Sheet, range: SortRange): boolean {
  return sheet.merges.some((merge) => {
    const parts = parseRange(`${merge.start}:${merge.end}`);
    return (
      parts !== null &&
      parts.start.row <= range.bottom &&
      parts.end.row >= range.top &&
      parts.start.col <= range.right &&
      parts.end.col >= range.left
    );
  });
}

export interface SortPlan {
  /** The sheet's cells after the sort. */
  cells: Record<string, Cell>;
  /** Addresses whose content changed, for incremental recalculation. */
  changed: string[];
}

/**
 * Plans a sort of `range` (the header row, if any, stays on top). `values` are
 * the computed values keyed by bare address. Returns null when there is
 * nothing to do: fewer than two sortable rows, a key outside the range, or
 * rows that are already in order.
 */
export function planSort(
  sheet: Sheet,
  values: ReadonlyMap<string, Scalar>,
  range: SortRange,
  levels: readonly SortLevel[],
  options: { hasHeaders: boolean; isHidden?: (row: number) => boolean },
): SortPlan | null {
  if (levels.length === 0 || levels.some((level) => level.column < range.left || level.column > range.right)) {
    return null;
  }
  const first = range.top + (options.hasHeaders ? 1 : 0);
  const positions: number[] = [];
  for (let row = first; row <= range.bottom; row += 1) {
    if (!options.isHidden?.(row)) positions.push(row);
  }
  if (positions.length < 2) return null;

  const keys = positions.map((row) => levels.map((level) => values.get(formatAddress(row, level.column)) ?? ""));
  const order = sortOrder(
    keys,
    levels.map((_, offset) => ({ offset, ascending: levels[offset].ascending })),
  );
  if (order.every((source, target) => source === target)) return null;

  // Only the cells that exist are visited: a sheet is sparse and the range
  // may span a million rows.
  const wanted = new Set(positions);
  const byRow = new Map<number, Array<[number, Cell]>>();
  for (const [address, cell] of Object.entries(sheet.cells)) {
    const position = parseAddress(address);
    if (!position || !wanted.has(position.row) || position.col < range.left || position.col > range.right) continue;
    const row = byRow.get(position.row) ?? [];
    row.push([position.col, cell]);
    byRow.set(position.row, row);
  }

  const cells = { ...sheet.cells };
  const changed = new Set<string>();
  const moves: Array<{ target: number; source: number }> = [];
  order.forEach((sourceIndex, targetIndex) => {
    if (sourceIndex === targetIndex) return;
    moves.push({ target: positions[targetIndex], source: positions[sourceIndex] });
  });
  for (const { target } of moves) {
    for (const [col] of byRow.get(target) ?? []) {
      const address = formatAddress(target, col);
      delete cells[address];
      changed.add(address);
    }
  }
  for (const { target, source } of moves) {
    for (const [col, cell] of byRow.get(source) ?? []) {
      const address = formatAddress(target, col);
      cells[address] = cell.formula ? { ...cell, formula: shiftFormulaRows(cell.formula, target - source) } : cell;
      changed.add(address);
    }
  }
  return { cells, changed: [...changed] };
}

export type SortContext =
  { ok: true; range: SortRange; headerGuess: boolean } | { ok: false; reason: "protected" | "merged" | "tooSmall" };

/**
 * What a sort works on. A selection of several cells is sorted as it is,
 * clipped to the used area of the sheet; a single cell stands for the whole
 * list: the filter range if the sheet has one, else the used range. The
 * answer also carries whether the first row looks like a header, and why a
 * sort is not possible (protected sheet, merged cells, fewer than two rows).
 */
export function sortContext(sheet: Sheet, values: ReadonlyMap<string, Scalar>, selection: SortRange): SortContext {
  if (isSheetProtected(sheet)) return { ok: false, reason: "protected" };
  const used = parseRange(usedRange(sheet));
  const list = parseRange(sheet.filter?.range ?? usedRange(sheet));
  if (!used || !list) return { ok: false, reason: "tooSmall" };
  const single = selection.top === selection.bottom && selection.left === selection.right;
  const range: SortRange = single
    ? { top: list.start.row, left: list.start.col, bottom: list.end.row, right: list.end.col }
    : {
        top: selection.top,
        left: selection.left,
        bottom: Math.min(selection.bottom, used.end.row),
        right: Math.min(selection.right, used.end.col),
      };
  if (range.bottom <= range.top || range.right < range.left) return { ok: false, reason: "tooSmall" };
  if (rangeHasMerges(sheet, range)) return { ok: false, reason: "merged" };
  const rowValues = (row: number) => {
    const out: Scalar[] = [];
    for (let col = range.left; col <= range.right; col += 1) out.push(values.get(formatAddress(row, col)) ?? "");
    return out;
  };
  return { ok: true, range, headerGuess: guessHeaders(rowValues(range.top), rowValues(range.top + 1)) };
}
