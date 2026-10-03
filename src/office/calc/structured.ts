/**
 * Structured table references for the Calc engine.
 *
 * A structured reference names a spreadsheet table and one of its parts, as in
 * `Sales[Amount]`, `Sales[@Net]` or `Sales[#Totals]`. This module is pure: it
 * translates those references into ordinary A1 addresses and ranges that the
 * existing evaluator and dependency graph already understand, so the rest of
 * the engine never needs a second addressing scheme.
 *
 * `currentRow` is the 1-based row of the formula's own cell; it is what lets a
 * this-row reference (`Sales[@Amount]`) resolve to a single cell.
 */
import type { SpreadsheetTable } from "../../lib/office-types";
import { formatAddress, parseAddress, parseRange } from "./addresses";

/** The part of a table a structured reference names. */
export type StructuredReferenceKind = "data" | "headers" | "totals" | "all" | "thisRow";

export interface StructuredReference {
  table: SpreadsheetTable;
  /** Declared column name, or null for a whole-table specifier. */
  column: string | null;
  kind: StructuredReferenceKind;
}

/** A rectangular slice of a table, as A1 addresses. */
export interface TableRange {
  start: string;
  end: string;
}

const SPECIAL_KINDS: Record<string, StructuredReferenceKind> = {
  "#ALL": "all",
  "#HEADERS": "headers",
  "#DATA": "data",
  "#TOTALS": "totals",
  "#THIS ROW": "thisRow",
};

/** Finds a table by name, ignoring case and surrounding whitespace. */
export function tableByName(tables: readonly SpreadsheetTable[] | undefined, name: string): SpreadsheetTable | null {
  const needle = name.trim().toLowerCase();
  if (!needle) return null;
  return (tables ?? []).find((table) => table.name.trim().toLowerCase() === needle) ?? null;
}

/** Declared name of the column at `index`, or null when out of range. */
export function columnNameOf(table: SpreadsheetTable, index: number): string | null {
  if (!Number.isInteger(index) || index < 0 || index >= table.columns.length) return null;
  return table.columns[index]?.name ?? null;
}

function columnIndexOf(table: SpreadsheetTable, columnName: string): number | null {
  const needle = columnName.trim().toLowerCase();
  if (!needle) return null;
  const index = table.columns.findIndex((column) => column.name.trim().toLowerCase() === needle);
  return index >= 0 ? index : null;
}

/** The table's data body, without the header and totals rows. */
export function tableBodyRange(table: SpreadsheetTable): TableRange | null {
  const parts = parseRange(table.range);
  if (!parts) return null;
  const startRow = parts.start.row + (table.hasHeaders ? 1 : 0);
  const endRow = parts.end.row - (table.hasTotals ? 1 : 0);
  if (startRow > endRow) return null;
  return { start: formatAddress(startRow, parts.start.col), end: formatAddress(endRow, parts.end.col) };
}

/** The header row, or null when the table declares none. */
export function tableHeaderRange(table: SpreadsheetTable): TableRange | null {
  const parts = parseRange(table.range);
  if (!parts || !table.hasHeaders) return null;
  return { start: formatAddress(parts.start.row, parts.start.col), end: formatAddress(parts.start.row, parts.end.col) };
}

/** The totals row, or null when the table declares none. */
export function tableTotalsRange(table: SpreadsheetTable): TableRange | null {
  const parts = parseRange(table.range);
  if (!parts || !table.hasTotals) return null;
  return { start: formatAddress(parts.end.row, parts.start.col), end: formatAddress(parts.end.row, parts.end.col) };
}

/** One column's cells down the data body (no header, no totals). */
export function tableColumnBodyRange(table: SpreadsheetTable, columnName: string): TableRange | null {
  const body = tableBodyRange(table);
  if (!body) return null;
  const start = parseAddress(body.start);
  const end = parseAddress(body.end);
  if (!start || !end) return null;
  return columnSlice(table, columnName, start.row, end.row);
}

/** Absolute 0-based column index of a declared column inside the table range. */
function columnColumnOf(table: SpreadsheetTable, columnName: string): number | null {
  const parts = parseRange(table.range);
  const index = columnIndexOf(table, columnName);
  if (!parts || index === null) return null;
  const col = parts.start.col + index;
  return col <= parts.end.col ? col : null;
}

function columnSlice(table: SpreadsheetTable, columnName: string, startRow: number, endRow: number): TableRange | null {
  const col = columnColumnOf(table, columnName);
  if (col === null) return null;
  return { start: formatAddress(startRow, col), end: formatAddress(endRow, col) };
}

function rangeString(range: TableRange): string {
  return range.start === range.end ? range.start : `${range.start}:${range.end}`;
}

/** Strips one layer of `[...]`; returns null when the text is not bracketed. */
function unwrapBrackets(text: string): string | null {
  const trimmed = text.trim();
  if (trimmed.length >= 2 && trimmed.startsWith("[") && trimmed.endsWith("]")) return trimmed.slice(1, -1).trim();
  return null;
}

/** Splits `[#Data],[Amount]` on commas that are not inside brackets. */
function splitTopLevel(content: string): string[] | null {
  const parts: string[] = [];
  let depth = 0;
  let current = "";
  for (const character of content) {
    if (character === "[") depth += 1;
    else if (character === "]") {
      depth -= 1;
      if (depth < 0) return null;
    } else if (character === "," && depth === 0) {
      parts.push(current);
      current = "";
      continue;
    }
    current += character;
  }
  if (depth !== 0) return null;
  parts.push(current);
  return parts;
}

/** Parses the text between the outer brackets of a structured reference. */
function parseBracketedContent(content: string): { kind: StructuredReferenceKind; column: string | null } | null {
  const trimmed = content.trim();
  if (!trimmed) return null;
  if (trimmed.startsWith("@")) {
    const rest = trimmed.slice(1).trim();
    const column = (unwrapBrackets(rest) ?? rest).trim();
    if (!column) return null;
    return { kind: "thisRow", column };
  }
  const parts = splitTopLevel(trimmed);
  if (!parts || parts.length === 0) return null;
  let kind: StructuredReferenceKind = "data";
  let column: string | null = null;
  for (const raw of parts) {
    const part = (unwrapBrackets(raw.trim()) ?? raw.trim()).trim();
    if (!part) return null;
    if (part.startsWith("#")) {
      const mapped = SPECIAL_KINDS[part.toUpperCase()];
      if (!mapped) return null;
      kind = mapped;
      continue;
    }
    if (column !== null) return null;
    column = part;
  }
  if (kind === "thisRow" && column === null) return null;
  return { kind, column };
}

/**
 * Parses a structured reference against the declared tables.
 *
 * Returns null when the leading name is not a declared table (or the reference
 * is malformed). A declared table with an unknown column still parses; it is
 * `resolveStructuredReference` that then reports the range as unresolvable.
 */
export function parseStructuredReference(
  reference: string,
  tables: readonly SpreadsheetTable[] | undefined,
): StructuredReference | null {
  const text = reference.trim();
  const open = text.indexOf("[");
  if (open <= 0 || !text.endsWith("]")) return null;
  const table = tableByName(tables, text.slice(0, open));
  if (!table) return null;
  const parsed = parseBracketedContent(text.slice(open + 1, -1));
  if (!parsed) return null;
  const index = parsed.column === null ? null : columnIndexOf(table, parsed.column);
  return { table, column: index === null ? parsed.column : columnNameOf(table, index), kind: parsed.kind };
}

/**
 * Resolves a structured reference to an A1 address or range, or null when the
 * table, column or requested part does not exist (or a this-row reference has
 * no row to anchor to).
 */
export function resolveStructuredReference(
  reference: string,
  tables: readonly SpreadsheetTable[] | undefined,
  currentRow?: number,
): string | null {
  const parsed = parseStructuredReference(reference, tables);
  if (!parsed) return null;
  const { table, column, kind } = parsed;
  const parts = parseRange(table.range);
  if (!parts) return null;

  if (kind === "thisRow") {
    if (currentRow === undefined || column === null) return null;
    if (currentRow < parts.start.row + 1 || currentRow > parts.end.row + 1) return null;
    const col = columnColumnOf(table, column);
    return col === null ? null : formatAddress(currentRow - 1, col);
  }

  if (kind === "all") {
    const col = column === null ? null : columnColumnOf(table, column);
    if (column !== null && col === null) return null;
    return rangeString({
      start: formatAddress(parts.start.row, col ?? parts.start.col),
      end: formatAddress(parts.end.row, col ?? parts.end.col),
    });
  }

  const base =
    kind === "headers" ? tableHeaderRange(table) : kind === "totals" ? tableTotalsRange(table) : tableBodyRange(table);
  if (!base) return null;
  if (column === null) return rangeString(base);
  const col = columnColumnOf(table, column);
  if (col === null) return null;
  const start = parseAddress(base.start);
  const end = parseAddress(base.end);
  if (!start || !end) return null;
  return rangeString({ start: formatAddress(start.row, col), end: formatAddress(end.row, col) });
}

/** Consumes a balanced `[...]` body starting at `start`, or null when unclosed. */
function readBalancedBrackets(text: string, start: number): string | null {
  if (text[start] !== "[") return null;
  let depth = 0;
  for (let index = start; index < text.length; index += 1) {
    if (text[index] === "[") depth += 1;
    else if (text[index] === "]") {
      depth -= 1;
      if (depth === 0) return text.slice(start, index + 1);
    }
  }
  return null;
}

/**
 * Every `Name[...]` span in a formula, in order, ignoring quoted strings.
 *
 * This is deliberately independent of the formula tokenizer so the dependency
 * graph can ask which tables a formula touches without parsing it twice.
 */
export function collectStructuredReferences(formula: string): string[] {
  if (!formula.includes("[")) return [];
  const text = formula.startsWith("=") ? formula.slice(1) : formula;
  const references: string[] = [];
  let index = 0;
  while (index < text.length) {
    if (text[index] === '"') {
      index += 1;
      while (index < text.length) {
        if (text[index] === '"') {
          if (text[index + 1] === '"') {
            index += 2;
            continue;
          }
          index += 1;
          break;
        }
        index += 1;
      }
      continue;
    }
    if (!/[A-Za-z_$]/.test(text[index])) {
      index += 1;
      continue;
    }
    const start = index;
    while (index < text.length && /[A-Za-z0-9_$.]/.test(text[index])) index += 1;
    const name = text.slice(start, index);
    if (text[index] !== "[") continue;
    const brackets = readBalancedBrackets(text, index);
    if (!brackets) continue;
    references.push(name + brackets);
    index += brackets.length;
  }
  return references;
}

/**
 * The A1 ranges every resolvable structured reference in `formula` reads.
 *
 * Used to wire dependency edges: the body range of `Sales[Amount]` has to mark
 * the reading formula dirty when any of its cells changes.
 */
export function structuredReferenceRanges(
  formula: string,
  tables: readonly SpreadsheetTable[] | undefined,
  currentRow?: number,
): string[] {
  const ranges: string[] = [];
  for (const reference of collectStructuredReferences(formula)) {
    const resolved = resolveStructuredReference(reference, tables, currentRow);
    if (resolved) ranges.push(resolved);
  }
  return ranges;
}
