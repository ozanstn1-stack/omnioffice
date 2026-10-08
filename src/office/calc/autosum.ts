/**
 * AutoSum: where a SUM goes and what it adds up.
 *
 * With one cell selected it sums the run of numbers directly above, else the
 * run directly to the left. With a range selected it writes the totals just
 * outside it: below each column, or to the right of a single row. When there
 * is nothing to add up it hands back an empty `=SUM()` for the user to finish.
 */
import type { CellTextEdit } from "./cells";
import { formatAddress, type Scalar } from "./formula";

export type AutoSumPlan =
  | { kind: "write"; edits: CellTextEdit[] }
  /** Open the cell editor on `text` with the caret `caret` characters in. */
  | { kind: "edit"; row: number; col: number; text: string; caret: number };

interface Bounds {
  top: number;
  left: number;
  bottom: number;
  right: number;
}

function reference(topRow: number, leftCol: number, bottomRow: number, rightCol: number): string {
  const start = formatAddress(topRow, leftCol);
  return topRow === bottomRow && leftCol === rightCol ? start : `${start}:${formatAddress(bottomRow, rightCol)}`;
}

export function planAutoSum(
  values: ReadonlyMap<string, Scalar>,
  selection: Bounds,
  active: { row: number; col: number },
): AutoSumPlan {
  const isNumber = (row: number, col: number) => typeof values.get(formatAddress(row, col)) === "number";
  const sum = (text: string, row: number, col: number): CellTextEdit => ({ row, col, text: `=SUM(${text})` });

  const { top, left, bottom, right } = selection;
  if (top !== bottom || left !== right) {
    if (top === bottom) return { kind: "write", edits: [sum(reference(top, left, top, right), top, right + 1)] };
    const edits: CellTextEdit[] = [];
    for (let col = left; col <= right; col += 1) edits.push(sum(reference(top, col, bottom, col), bottom + 1, col));
    return { kind: "write", edits };
  }

  const { row, col } = active;
  let first = row;
  while (first > 0 && isNumber(first - 1, col)) first -= 1;
  if (first < row) return { kind: "write", edits: [sum(reference(first, col, row - 1, col), row, col)] };
  let start = col;
  while (start > 0 && isNumber(row, start - 1)) start -= 1;
  if (start < col) return { kind: "write", edits: [sum(reference(row, start, row, col - 1), row, col)] };
  return { kind: "edit", row, col, text: "=SUM()", caret: "=SUM(".length };
}
