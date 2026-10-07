/**
 * Pure helpers for the Calc AI actions: what is read from the sheet and sent
 * to the model, and how a text result is written back.
 */
import { columnLabel, formatAddress, parseAddress, toText, type Scalar } from "./formula";
import type { Sheet } from "../../lib/office-types";

/** Values of one column (first..last row), blanks skipped, one per line. */
export function columnValueLines(
  computed: Map<string, Scalar>,
  col: number,
  firstRow: number,
  lastRow: number,
): string[] {
  const lines: string[] = [];
  for (let row = firstRow; row <= lastRow; row += 1) {
    const value = toText(computed.get(formatAddress(row, col)) ?? "").trim();
    if (value) lines.push(value.replace(/\s*\n\s*/g, " "));
  }
  return lines;
}

/** Last row of a column that holds a cell with content (-1 for none). */
export function lastRowOfColumn(sheet: Sheet, col: number): number {
  let last = -1;
  for (const [address, cell] of Object.entries(sheet.cells)) {
    const position = parseAddress(address);
    if (!position || position.col !== col) continue;
    if (cell.value.kind === "empty" && !cell.formula) continue;
    last = Math.max(last, position.row);
  }
  return last;
}

/** Most header cells, and longest header text, sent with a formula request. */
export const MAX_HEADER_CELLS = 30;
export const MAX_HEADER_CHARS = 40;

/** "A: Name; B: Amount" from the first row, for the formula prompt (bounded: 30 headers of 40 characters). */
export function headerContext(computed: Map<string, Scalar>, colCount: number): string {
  const parts: string[] = [];
  for (let col = 0; col < colCount && parts.length < MAX_HEADER_CELLS; col += 1) {
    const header = toText(computed.get(formatAddress(0, col)) ?? "")
      .replace(/\s+/g, " ")
      .trim()
      .slice(0, MAX_HEADER_CHARS);
    if (header) parts.push(`${columnLabel(col)}: ${header}`);
  }
  return parts.join("; ");
}

/** A summary as one cell value: single line, never read back as a formula. */
export function summaryCellText(text: string): string {
  const line = text.replace(/\s*\n\s*/g, " ").trim();
  return /^[=+\-@]/.test(line) ? ` ${line}` : line;
}
