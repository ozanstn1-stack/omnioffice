/**
 * A1-style cell addressing for the Calc engine.
 *
 * Shared by the formula parser, the grid, the pivot/table builders and the
 * formula bar, so there is exactly one place that knows how `AB123` maps to a
 * row/column pair.
 */
import type { Scalar } from "./scalars";
import { isError, toNumber } from "./scalars";

/** Excel's hard limits: 1 048 576 rows and 16 384 columns. */
export const MAX_ROWS = 1_048_576;
export const MAX_COLS = 16_384;

export interface CellAddress {
  row: number;
  col: number;
}

export function parseAddress(address: string): CellAddress | null {
  const match = /^\$?([A-Za-z]{1,3})\$?(\d{1,7})$/.exec(address.trim());
  if (!match) return null;
  const col = columnIndex(match[1].toUpperCase());
  const row = Number(match[2]);
  if (row < 1 || col === null || col >= MAX_COLS) return null;
  return { row: row - 1, col };
}

export function columnIndex(letters: string): number | null {
  let value = 0;
  for (const character of letters.toUpperCase()) {
    const digit = character.charCodeAt(0) - 64;
    if (digit < 1 || digit > 26) return null;
    value = value * 26 + digit;
  }
  return value - 1;
}

export function formatAddress(row: number, col: number): string {
  return `${columnLabel(col)}${row + 1}`;
}

export function columnLabel(col: number): string {
  let remaining = Math.max(0, Math.floor(col));
  let label = "";
  while (true) {
    label = String.fromCharCode(65 + (remaining % 26)) + label;
    remaining = Math.floor(remaining / 26) - 1;
    if (remaining < 0) break;
  }
  return label;
}

export interface RangeParts {
  start: CellAddress;
  end: CellAddress;
}

export function parseRange(range: string): RangeParts | null {
  const cleaned = range.replace(/\$/g, "").trim();
  const [first, second] = cleaned.split(":");
  const start = parseAddress(first ?? "");
  if (!start) return null;
  if (!second) return { start, end: start };
  const end = parseAddress(second);
  if (!end) return null;
  return {
    start: { row: Math.min(start.row, end.row), col: Math.min(start.col, end.col) },
    end: { row: Math.max(start.row, end.row), col: Math.max(start.col, end.col) },
  };
}

/**
 * Expands a range into addresses in row-major order, capped at `limit`.
 *
 * The cap is what stops `=SUM(A1:AMJ1048576)` from allocating a billion
 * strings; the formula evaluator applies the same bound before calling this.
 */
export function addressesInRange(range: string, limit = 500_000): string[] {
  const parts = parseRange(range);
  if (!parts) return [];
  const out: string[] = [];
  for (let row = parts.start.row; row <= parts.end.row && out.length < limit; row += 1) {
    for (let col = parts.start.col; col <= parts.end.col && out.length < limit; col += 1) {
      out.push(formatAddress(row, col));
    }
  }
  return out;
}

/** True when `address` falls inside `range` (both may use `$`). */
export function addressInRange(address: string, range: string): boolean {
  const parts = parseRange(range);
  const position = parseAddress(address);
  if (!parts || !position) return false;
  return (
    position.row >= parts.start.row &&
    position.row <= parts.end.row &&
    position.col >= parts.start.col &&
    position.col <= parts.end.col
  );
}

/** Row/column count of a range, without materialising every address. */
export function rangeSize(range: string): { rows: number; cols: number } {
  const parts = parseRange(range);
  if (!parts) return { rows: 0, cols: 0 };
  return { rows: parts.end.row - parts.start.row + 1, cols: parts.end.col - parts.start.col + 1 };
}

/** Numeric value of a scalar, or NaN when it is not usable in arithmetic. */
export function numericOrNaN(value: Scalar): number {
  const number = toNumber(value);
  return isError(number) ? Number.NaN : number;
}
