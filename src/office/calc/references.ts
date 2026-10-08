/**
 * References as values.
 *
 * Most functions only ever see the cell values a reference points at, but
 * OFFSET and INDIRECT produce a *location*, and CELL asks about one. A
 * `CellReference` is that location: a sheet and a rectangle of cells, kept
 * apart from the values so a chain such as `OFFSET(INDIRECT(...), 1, 0)` can
 * keep moving it before anything is read.
 */
import { MAX_COLS, MAX_ROWS, columnLabel, type CellAddress } from "./addresses";
import { ERR, isError, type FormulaError, type Scalar } from "./scalars";

export interface CellReference {
  kind: "reference";
  /** Workbook spelling of the sheet name; null means the formula's own sheet. */
  sheet: string | null;
  /** Top-left and bottom-right corners, 0-based and inclusive. */
  start: CellAddress;
  end: CellAddress;
}

export function makeReference(sheet: string | null, start: CellAddress, end: CellAddress = start): CellReference {
  return { kind: "reference", sheet, start, end };
}

export function isCellReference(value: unknown): value is CellReference {
  return typeof value === "object" && value !== null && (value as { kind?: unknown }).kind === "reference";
}

export function referenceSize(reference: CellReference): { rows: number; cols: number } {
  return {
    rows: reference.end.row - reference.start.row + 1,
    cols: reference.end.col - reference.start.col + 1,
  };
}

/** The first cell of a reference, as a one-cell reference of its own. */
export function firstCell(reference: CellReference): CellReference {
  return makeReference(reference.sheet, reference.start);
}

/**
 * OFFSET: the reference moved by `rows`/`cols`, optionally resized.
 *
 * `height`/`width` default to the size of the base. A reference that ends up
 * with no cells, or that leaves the sheet, is `#REF!`, as in Excel.
 */
export function offsetReference(
  base: CellReference,
  rows: number,
  cols: number,
  height?: number,
  width?: number,
): CellReference | FormulaError {
  const size = referenceSize(base);
  const nextHeight = height ?? size.rows;
  const nextWidth = width ?? size.cols;
  if (nextHeight < 1 || nextWidth < 1) return ERR.ref();
  const top = base.start.row + rows;
  const left = base.start.col + cols;
  if (top < 0 || left < 0 || top + nextHeight > MAX_ROWS || left + nextWidth > MAX_COLS) return ERR.ref();
  return makeReference(base.sheet, { row: top, col: left }, { row: top + nextHeight - 1, col: left + nextWidth - 1 });
}

/** A sheet name as it must be written in front of `!`. */
function sheetPrefix(sheet: string): string {
  return /^[A-Za-z_][A-Za-z0-9_.]*$/.test(sheet) ? `${sheet}!` : `'${sheet.replace(/'/g, "''")}'!`;
}

/**
 * CELL("address"): the absolute address of the first cell. A cell on another
 * sheet carries its sheet name; Excel also prefixes the workbook name, which is
 * not known here.
 */
export function referenceAddress(reference: CellReference, currentSheet: string): string {
  const address = `$${columnLabel(reference.start.col)}$${reference.start.row + 1}`;
  const foreign = reference.sheet !== null && reference.sheet !== currentSheet;
  return foreign ? `${sheetPrefix(reference.sheet!)}${address}` : address;
}

/** CELL("type"): "b" for blank, "l" for a text label, "v" for any other value. */
export function cellTypeLetter(value: Scalar): "b" | "l" | "v" {
  if (isError(value)) return "v";
  if (typeof value === "string") return value === "" ? "b" : "l";
  return "v";
}
