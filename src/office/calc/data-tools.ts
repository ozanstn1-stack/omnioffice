/**
 * Data tools for the Calc editor: text to columns, remove duplicates and the
 * choices of a list validation.
 *
 * Everything here works on plain strings and scalars so it can be unit tested
 * without mounting the editor; `CalcEditor` turns the results into a single
 * undoable sheet edit.
 */
import { parseAddress } from "./addresses";
import { isError, toText, type Scalar } from "./formula";

// ---------------------------------------------------------------------------
// Text to columns
// ---------------------------------------------------------------------------

export type SplitDelimiter = "comma" | "semicolon" | "tab" | "space" | "custom";

export interface SplitOptions {
  delimiter: SplitDelimiter;
  /** The delimiter text when `delimiter` is "custom"; may be several characters. */
  custom?: string;
  /** Runs of delimiters split once, and leading/trailing ones add no empty piece. */
  mergeConsecutive?: boolean;
  /**
   * Text qualifier: a piece that starts with it runs to the matching closing
   * qualifier and may contain the delimiter; a doubled qualifier inside is a
   * literal one. Defaults to `"`, null turns quoting off.
   */
  quote?: string | null;
}

const DELIMITERS: Record<Exclude<SplitDelimiter, "custom">, string> = {
  comma: ",",
  semicolon: ";",
  tab: "\t",
  space: " ",
};

/** The literal text one delimiter option splits on ("" for an empty custom one). */
export function delimiterText(options: Pick<SplitOptions, "delimiter" | "custom">): string {
  return options.delimiter === "custom" ? (options.custom ?? "") : DELIMITERS[options.delimiter];
}

/** Reads a qualified piece starting after its opening quote, or null when it never closes. */
function readQuoted(text: string, start: number, quote: string): { text: string; end: number } | null {
  let out = "";
  let index = start;
  while (index < text.length) {
    if (text[index] === quote) {
      if (text[index + 1] === quote) {
        out += quote;
        index += 2;
        continue;
      }
      return { text: out, end: index + 1 };
    }
    out += text[index];
    index += 1;
  }
  return null;
}

/** Splits one cell's text into its pieces. */
export function splitDelimited(text: string, options: SplitOptions): string[] {
  const delimiter = delimiterText(options);
  if (delimiter === "") return [text];
  const quote = options.quote === undefined ? '"' : options.quote;
  const pieces: Array<{ text: string; quoted: boolean }> = [];
  let current = "";
  let quoted = false;
  let atStart = true;
  let index = 0;
  while (index < text.length) {
    if (atStart && quote && text[index] === quote) {
      const read = readQuoted(text, index + 1, quote);
      // An unterminated qualifier is ordinary text.
      if (read) {
        current = read.text;
        quoted = true;
        atStart = false;
        index = read.end;
        continue;
      }
    }
    if (text.startsWith(delimiter, index)) {
      pieces.push({ text: current, quoted });
      current = "";
      quoted = false;
      atStart = true;
      index += delimiter.length;
      continue;
    }
    current += text[index];
    atStart = false;
    index += 1;
  }
  pieces.push({ text: current, quoted });
  // A quoted empty piece ("") is explicit, so merging keeps it.
  const kept = options.mergeConsecutive ? pieces.filter((piece) => piece.quoted || piece.text !== "") : pieces;
  return kept.map((piece) => piece.text);
}

export interface ColumnSplitPlan {
  /** Pieces per source row; null leaves the row untouched (nothing to split). */
  rows: Array<string[] | null>;
  /** Columns the widest row fills, the source column included. */
  width: number;
}

/**
 * Splits a column of texts. A row whose split is a no-op (one piece equal to
 * the source, or nothing at all) is left alone, so numbers and formulas
 * without the delimiter keep their type.
 */
export function planTextToColumns(texts: readonly string[], options: SplitOptions): ColumnSplitPlan {
  let width = 1;
  const rows = texts.map((text) => {
    const pieces = splitDelimited(text, options);
    if (pieces.length === 0 || (pieces.length === 1 && pieces[0] === text)) return null;
    width = Math.max(width, pieces.length);
    return pieces;
  });
  return { rows, width };
}

// ---------------------------------------------------------------------------
// Remove duplicates
// ---------------------------------------------------------------------------

export interface DuplicateOptions {
  /** Column offsets inside the range that decide whether two rows are equal. */
  columns: readonly number[];
  /** The first row is a header: it is always kept and never compared. */
  hasHeaders: boolean;
}

export interface DuplicateResult {
  /** Row offsets that stay, in their original order (the header included). */
  keep: number[];
  /** Row offsets that repeat an earlier row. */
  removed: number[];
}

/**
 * The comparison key of one value. Text compares case-insensitively (like
 * Excel's Remove Duplicates); the type tag keeps the number 1 and the text
 * "1" apart.
 */
function duplicateKey(value: Scalar | undefined): string {
  if (value === undefined || value === "") return "";
  if (isError(value)) return `e:${value.code}`;
  if (typeof value === "number") return `n:${value}`;
  if (typeof value === "boolean") return `b:${value}`;
  return `s:${value.toLowerCase()}`;
}

/** Finds the rows whose chosen columns repeat an earlier row. */
export function findDuplicateRows(rows: readonly (readonly Scalar[])[], options: DuplicateOptions): DuplicateResult {
  const keep: number[] = [];
  const removed: number[] = [];
  const seen = new Set<string>();
  rows.forEach((row, index) => {
    if ((options.hasHeaders && index === 0) || options.columns.length === 0) {
      keep.push(index);
      return;
    }
    const key = options.columns.map((column) => duplicateKey(row[column])).join("\u0000");
    if (seen.has(key)) {
      removed.push(index);
    } else {
      seen.add(key);
      keep.push(index);
    }
  });
  return { keep, removed };
}

/** The rows and columns Remove Duplicates rearranges (0-based, inclusive). */
export interface MovedBlock {
  top: number;
  bottom: number;
  left: number;
  right: number;
}

/**
 * Rewrites a formula for Remove Duplicates, which deletes rows inside a block
 * and shifts the rest up (Excel's delete-and-shift semantics, not a fill): a
 * reference to a cell of the block follows that cell's row through `rowMap`
 * (old row -> new row, 0-based). References outside the block, to other
 * sheets, and to removed rows are left as written, so `=C3*2` beside the
 * block keeps reading C3.
 */
export function remapMovedRows(formula: string | null, block: MovedBlock, rowMap: ReadonlyMap<number, number>) {
  if (!formula) return formula;
  return formula.replace(
    /(?<![A-Za-z0-9_$!'.])(\$?)([A-Za-z]{1,3})(\$?)(\d{1,7})(?![A-Za-z0-9_(!])/g,
    (match, dollarCol: string, letters: string, dollarRow: string, digits: string) => {
      const address = parseAddress(`${letters}${digits}`);
      if (!address) return match;
      const inside =
        address.row >= block.top &&
        address.row <= block.bottom &&
        address.col >= block.left &&
        address.col <= block.right;
      const row = inside ? rowMap.get(address.row) : undefined;
      return row === undefined ? match : `${dollarCol}${letters}${dollarRow}${row + 1}`;
    },
  );
}

// ---------------------------------------------------------------------------
// List validation
// ---------------------------------------------------------------------------

const LIST_REFERENCE =
  /^=\s*(?:(?:'((?:[^']|'')+)'|([^!'"=\s]+))!)?(\$?[A-Za-z]{1,3}\$?\d{1,7}(?::\$?[A-Za-z]{1,3}\$?\d{1,7})?)\s*$/;

/**
 * The choices of a list validation, trimmed, without blanks and duplicates.
 *
 * The model stores an inline list, one value per entry (what the validation
 * dialog and the XLSX importer write). A single entry written as a reference
 * (`=A1:A5`, `=Lists!$A$1:$A$9`) is read through `resolve`, which gets the
 * sheet name (null for the validation's own sheet) and the bare range.
 */
export function listValidationItems(
  values: readonly string[],
  resolve: (sheet: string | null, range: string) => readonly Scalar[] | null,
): string[] {
  const reference = values.length === 1 ? LIST_REFERENCE.exec(values[0].trim()) : null;
  const raw = reference
    ? (resolve(reference[1]?.replace(/''/g, "'") ?? reference[2] ?? null, reference[3]) ?? []).map((value) =>
        toText(value),
      )
    : values;
  const items: string[] = [];
  const seen = new Set<string>();
  for (const entry of raw) {
    const item = entry.trim();
    const key = item.toLowerCase();
    if (item === "" || seen.has(key)) continue;
    seen.add(key);
    items.push(item);
  }
  return items;
}
