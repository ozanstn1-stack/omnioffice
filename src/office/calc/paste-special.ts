/**
 * Paste special for the Calc model.
 *
 * Copying keeps a snapshot of the source cells (formulas, formatting) next to
 * their computed values. Pasting picks what to bring along: everything,
 * formulas only (relative references shift by the distance, absolute ones
 * stay, constants stay constants), values only (what the cells show), or
 * formatting only; and may swap rows and columns. A paste goes through the
 * cell parser in one model change, so pasted formulas are evaluated and the
 * cells that read the target are recalculated. Text copied in another
 * application has no formulas or formatting: it pastes as values.
 */
import { cellText, defaultCellStyle, type Cell, type Sheet, type Workbook } from "../../lib/office-types";
import { applyCellTextEdits, shiftFormulaRows, type CellTextEdit } from "./cells";
import { formatAddress, isError, MAX_COLS, MAX_ROWS, type Scalar } from "./formula";
import { shiftFormulaColumns } from "./grid-math";
import type { CellPosition } from "./grid-types";

export type PasteContent = "all" | "formulas" | "values" | "formats";

export interface PasteOptions {
  content: PasteContent;
  transpose: boolean;
}

/** What a copy leaves on the in-app clipboard. */
export interface ClipboardData {
  /** The source cells row by row; undefined where the source cell does not exist. */
  cells: Array<Array<Cell | undefined>>;
  /** What the source cells show, in the same shape. */
  values: Scalar[][];
  /** Where the top-left source cell sits on its sheet. */
  origin: CellPosition;
}

export function snapshotClipboard(
  sheet: Sheet,
  values: ReadonlyMap<string, Scalar>,
  range: { top: number; left: number; bottom: number; right: number },
): ClipboardData {
  const cells: ClipboardData["cells"] = [];
  const shown: Scalar[][] = [];
  for (let row = range.top; row <= range.bottom; row += 1) {
    const cellRow: Array<Cell | undefined> = [];
    const valueRow: Scalar[] = [];
    for (let col = range.left; col <= range.right; col += 1) {
      const address = formatAddress(row, col);
      cellRow.push(sheet.cells[address]);
      valueRow.push(values.get(address) ?? "");
    }
    cells.push(cellRow);
    shown.push(valueRow);
  }
  return { cells, values: shown, origin: { row: range.top, col: range.left } };
}

/** Text copied elsewhere (tab separated columns, one row per line) as clipboard data. */
export function clipboardFromText(text: string): ClipboardData {
  const rows = text
    .split(/\r?\n/)
    .filter((line) => line.length > 0)
    .map((line) => line.split("\t"));
  return {
    cells: rows.map((row) => row.map(() => undefined)),
    values: rows,
    origin: { row: 0, col: 0 },
  };
}

/** The text other applications receive: the shown values, tab and newline separated. */
export function clipboardText(data: ClipboardData): string {
  return data.values.map((line) => line.map((value) => String(value ?? "")).join("\t")).join("\n");
}

function valueText(value: Scalar): string {
  return isError(value) ? value.code : String(value ?? "");
}

/** The edits that paste `data` with its top-left corner on `target`. */
export function planPaste(data: ClipboardData, target: CellPosition, options: PasteOptions): CellTextEdit[] {
  const edits: CellTextEdit[] = [];
  data.cells.forEach((cellRow, r) => {
    cellRow.forEach((cell, c) => {
      const row = target.row + (options.transpose ? c : r);
      const col = target.col + (options.transpose ? r : c);
      if (row >= MAX_ROWS || col >= MAX_COLS) return;
      const edit: CellTextEdit = { row, col };
      if (options.content === "values") {
        edit.text = valueText(data.values[r][c]);
      } else if (options.content !== "formats") {
        edit.text = cell?.formula
          ? (shiftFormulaColumns(
              shiftFormulaRows(cell.formula, row - (data.origin.row + r)),
              col - (data.origin.col + c),
            ) ?? cell.formula)
          : cell
            ? cellText(cell)
            : valueText(data.values[r][c]);
      }
      if (options.content === "all" || options.content === "formats") edit.style = cell?.style ?? defaultCellStyle();
      edits.push(edit);
    });
  });
  return edits;
}

/** Pastes as one model change; the workbook comes back unchanged for an empty clipboard. */
export function pasteSpecial(
  workbook: Workbook,
  sheetIndex: number,
  data: ClipboardData,
  target: CellPosition,
  options: PasteOptions,
): Workbook {
  return applyCellTextEdits(workbook, sheetIndex, planPaste(data, target, options));
}
