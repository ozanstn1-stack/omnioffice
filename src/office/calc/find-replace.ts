/**
 * Pure find & replace for the Calc model.
 *
 * A search runs over the text of each cell: its formula or typed value when
 * looking in formulas, what the grid shows when looking in values. A match is
 * a whole cell (a replacement rewrites every occurrence inside it, like
 * Excel's Replace button), and cells come in reading order: sheet by sheet,
 * row by row, left to right. Replacing goes through the cell parser, so a
 * replaced formula is re-parsed and recalculated, and cells of protected
 * sheets are never written.
 */
import { cellText, type Workbook } from "../../lib/office-types";
import { expandReplacement, escapeRegExp, findInText, MATCH_LIMIT } from "../writer/find-replace";
import { applyCellTextEdits, cellValueToScalar, formatCellDisplay, type CellTextEdit } from "./cells";
import { MAX_COLS, parseAddress, type Scalar } from "./formula";
import { isSheetProtected } from "./protection";

export { MATCH_LIMIT };

export interface CalcFindOptions {
  /** The active sheet only, or every sheet of the workbook. */
  scope: "sheet" | "workbook";
  /** Formulas: the formula or typed text. Values: the text the grid displays. */
  lookIn: "formulas" | "values";
  matchCase: boolean;
  /** The search text must equal the whole cell. */
  wholeCell: boolean;
  /** Treat the search text as a JavaScript regular expression (`$1` groups in the replacement). */
  regex: boolean;
}

export const DEFAULT_FIND_OPTIONS: CalcFindOptions = {
  scope: "sheet",
  lookIn: "formulas",
  matchCase: false,
  wholeCell: false,
  regex: false,
};

export type CompiledFind = { ok: true; pattern: RegExp } | { ok: false; error: string };

/**
 * Builds the global search pattern, or null for an empty query. An invalid
 * regular expression comes back as an error value, so the panel can show it
 * while the user is still typing.
 */
export function compileFind(query: string, options: CalcFindOptions): CompiledFind | null {
  if (!query) return null;
  const flags = options.matchCase ? "gu" : "giu";
  const source = options.regex ? query : escapeRegExp(query);
  try {
    // Validated on its own first: wrapped in `(?:…)` an unbalanced `a)|(b`
    // would compile and silently mean something else.
    new RegExp(source, flags);
    const bounded = options.wholeCell ? `^(?:${source})$` : source;
    return { ok: true, pattern: new RegExp(bounded, flags) };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : String(error) };
  }
}

/** A cell the search looks at. */
export interface SearchCell {
  sheet: number;
  row: number;
  col: number;
  address: string;
  /** The text the pattern runs over. */
  text: string;
  /** False where a replacement would be refused: protected sheet, or a value that is not plain text. */
  replaceable: boolean;
}

export interface CellKey {
  sheet: number;
  row: number;
  col: number;
}

function keyOrder(key: CellKey): number {
  return key.row * MAX_COLS + key.col;
}

/** Reading order: sheet, then row, then column. */
export function compareKeys(left: CellKey, right: CellKey): number {
  return left.sheet - right.sheet || keyOrder(left) - keyOrder(right);
}

/**
 * The non-empty cells in scope, in reading order. `values` are the computed
 * values keyed `Sheet!A1` (see `computeWorkbookValues`); they are only read
 * when looking in values.
 */
export function searchCells(
  workbook: Workbook,
  activeSheet: number,
  options: CalcFindOptions,
  values: ReadonlyMap<string, Scalar>,
): SearchCell[] {
  const active = Math.min(Math.max(0, activeSheet), workbook.sheets.length - 1);
  const indexes = options.scope === "workbook" ? workbook.sheets.map((_, index) => index) : [active];
  const out: SearchCell[] = [];
  for (const sheetIndex of indexes) {
    const sheet = workbook.sheets[sheetIndex];
    if (!sheet) continue;
    const protectedSheet = isSheetProtected(sheet);
    const found: SearchCell[] = [];
    for (const address of Object.keys(sheet.cells)) {
      const cell = sheet.cells[address];
      const position = parseAddress(address);
      if (!cell || !position) continue;
      const text =
        options.lookIn === "formulas"
          ? cellText(cell)
          : formatCellDisplay(values.get(`${sheet.name}!${address}`) ?? cellValueToScalar(cell.value), cell.style);
      if (text === "") continue;
      const plainText = cell.formula === null && cell.value.kind === "text";
      found.push({
        sheet: sheetIndex,
        row: position.row,
        col: position.col,
        address,
        text,
        replaceable: !protectedSheet && (options.lookIn === "formulas" || plainText),
      });
    }
    found.sort(compareKeys);
    out.push(...found);
  }
  return out;
}

/** The cells whose text holds at least one non-empty match, up to `limit`. */
export function matchCells(cells: readonly SearchCell[], pattern: RegExp, limit = MATCH_LIMIT): SearchCell[] {
  const out: SearchCell[] = [];
  for (const cell of cells) {
    // Zero-length matches (`^`, `x*`) select nothing, so they are not matches.
    if (findInText(cell.text, pattern, 1).length === 0) continue;
    out.push(cell);
    if (out.length >= limit) break;
  }
  return out;
}

/**
 * The match after (`direction` 1) or before (-1) a position, wrapping around
 * at the ends. The position itself does not count, so stepping always moves.
 */
export function stepMatch(matches: readonly SearchCell[], from: CellKey, direction: 1 | -1): SearchCell | null {
  if (matches.length === 0) return null;
  if (direction === 1) {
    return matches.find((match) => compareKeys(match, from) > 0) ?? matches[0];
  }
  for (let index = matches.length - 1; index >= 0; index -= 1) {
    if (compareKeys(matches[index], from) < 0) return matches[index];
  }
  return matches[matches.length - 1];
}

/**
 * Replaces every non-empty match in a text. A regex replacement expands `$1`,
 * `$&` and `$<name>` the way `String.prototype.replace` does; otherwise the
 * replacement is taken literally.
 */
export function replaceInText(
  text: string,
  pattern: RegExp,
  replacement: string,
  regex: boolean,
): { text: string; count: number } {
  let out = "";
  let last = 0;
  let count = 0;
  for (const match of text.matchAll(pattern)) {
    if (match[0].length === 0) continue;
    const index = match.index ?? 0;
    out += text.slice(last, index) + (regex ? expandReplacement(replacement, match) : replacement);
    last = index + match[0].length;
    count += 1;
  }
  return count === 0 ? { text, count } : { text: out + text.slice(last), count };
}

export interface ReplaceResult {
  workbook: Workbook;
  /** Cells whose content changed. */
  replaced: number;
  /** Matching cells the replacement left as they were. */
  unchanged: number;
  /** Matching cells that may not be replaced: protected sheets, values that are not plain text. */
  skipped: number;
}

/**
 * Replaces the matches in the given cells in one model change (one undo step
 * for the caller). Texts are parsed like typed input, so a replaced formula
 * is re-parsed and its cached value, and everything reading it, recalculated.
 * The workbook comes back unchanged (same object) when no cell changes.
 */
export function replaceCells(
  workbook: Workbook,
  targets: readonly SearchCell[],
  pattern: RegExp,
  replacement: string,
  options: Pick<CalcFindOptions, "regex">,
): ReplaceResult {
  let replaced = 0;
  let unchanged = 0;
  let skipped = 0;
  const bySheet = new Map<number, CellTextEdit[]>();
  for (const target of targets) {
    if (!target.replaceable) {
      skipped += 1;
      continue;
    }
    const result = replaceInText(target.text, pattern, replacement, options.regex);
    if (result.count === 0 || result.text === target.text) {
      unchanged += 1;
      continue;
    }
    const edits = bySheet.get(target.sheet) ?? [];
    edits.push({ row: target.row, col: target.col, text: result.text });
    bySheet.set(target.sheet, edits);
    replaced += 1;
  }
  if (replaced === 0) return { workbook, replaced, unchanged, skipped };
  let next = workbook;
  for (const [sheetIndex, edits] of [...bySheet.entries()].sort((a, b) => a[0] - b[0])) {
    next = applyCellTextEdits(next, sheetIndex, edits);
  }
  return { workbook: next, replaced, unchanged, skipped };
}
