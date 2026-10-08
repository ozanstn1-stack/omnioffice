/**
 * Formula assistance for the Calc editor: autocomplete suggestions for the
 * word at the caret and the argument hint of the call the caret is inside.
 * Pure functions over the draft text, so the popup can be tested without the
 * grid.
 */
import type { Translate } from "../../lib/i18n";
import type { Sheet, Workbook } from "../../lib/office-types";
import { suggestFunctions } from "./formula";
import { tableByName } from "./structured";

export type SuggestionKind = "function" | "name" | "sheet" | "table" | "column";

export interface FormulaSuggestion {
  kind: SuggestionKind;
  label: string;
  insert: string;
  detail: string;
  /** Caret offset inside `insert` after the insertion (defaults to the end). */
  caret?: number;
}

export interface SuggestionList {
  items: FormulaSuggestion[];
  /** Range inside the draft the selected item replaces. */
  start: number;
  end: number;
}

export interface ArgumentHint {
  name: string;
  parts: string[];
  active: number;
}

export interface CatalogueEntry {
  name: string;
  signature: string;
  category: string;
}

/** The identifier or partial structured reference ending at `caret`. */
function suggestionWord(text: string, caret: number): string {
  return /[A-Za-z_$][A-Za-z0-9_$.]*$/.exec(text.slice(0, caret))?.[0] ?? "";
}

/**
 * The suggestion popup contents for a draft.
 *
 * Inside `Table[...]` the items are the table's columns; otherwise functions
 * (prefix match), defined names, sheet names and table names are offered.
 * Returns null when the draft is not a formula or nothing matches.
 */
export function buildSuggestions(
  text: string,
  caret: number,
  workbook: Workbook,
  sheet: Sheet,
  catalogue: Map<string, CatalogueEntry>,
  t: Translate,
): SuggestionList | null {
  if (!text.startsWith("=") || caret < 1 || caret > text.length) return null;
  const before = text.slice(0, caret);
  const bracket = /([A-Za-z_][A-Za-z0-9_$. ]*)\[([^[\]]*)$/.exec(before);
  if (bracket) {
    const table = tableByName(sheet.tables, bracket[1]);
    if (!table) return null;
    const partial = bracket[2];
    const items = table.columns
      .filter((column) => column.name.toUpperCase().startsWith(partial.toUpperCase()))
      .map<FormulaSuggestion>((column) => ({
        kind: "column",
        label: column.name,
        insert: text[caret] === "]" ? column.name : `${column.name}]`,
        detail: `${table.name}[${column.name}]`,
      }));
    if (items.length === 0) return null;
    return { items, start: caret - partial.length, end: caret };
  }

  const word = suggestionWord(text, caret);
  if (!word) return null;
  const upper = word.toUpperCase();
  const items: FormulaSuggestion[] = [];
  for (const name of suggestFunctions(word, 6)) {
    const meta = catalogue.get(name);
    items.push({
      kind: "function",
      label: name,
      insert: `${name}(`,
      caret: name.length + 1,
      detail: meta ? `${meta.signature} · ${meta.category}` : name,
    });
  }
  for (const entry of workbook.names ?? []) {
    if (entry.name.toUpperCase().startsWith(upper)) {
      items.push({ kind: "name", label: entry.name, insert: entry.name, detail: entry.definition });
    }
  }
  for (const candidate of workbook.sheets) {
    if (candidate.name.toUpperCase().startsWith(upper)) {
      items.push({
        kind: "sheet",
        label: candidate.name,
        insert: `${candidate.name}!`,
        detail: t("calc.sheetReference"),
      });
    }
  }
  for (const table of sheet.tables ?? []) {
    if (table.name.toUpperCase().startsWith(upper)) {
      items.push({ kind: "table", label: table.name, insert: `${table.name}[`, detail: t("calc.tableReference") });
    }
  }
  if (items.length === 0) return null;
  return { items, start: caret - word.length, end: caret };
}

/** Splits a signature body on top-level commas (`VLOOKUP(a, [b], c)`). */
function splitSignature(signature: string): string[] {
  const open = signature.indexOf("(");
  const close = signature.lastIndexOf(")");
  if (open < 0 || close <= open) return [signature];
  const body = signature.slice(open + 1, close);
  const parts: string[] = [];
  let depth = 0;
  let current = "";
  for (const character of body) {
    if (character === "(" || character === "[") depth += 1;
    else if (character === ")" || character === "]") depth -= 1;
    if (character === "," && depth === 0) {
      parts.push(current);
      current = "";
      continue;
    }
    current += character;
  }
  parts.push(current);
  return parts.map((part) => part.trim());
}

/**
 * The argument hint for the call the caret is inside, or null.
 *
 * Scanning backwards from the caret, the innermost unmatched `(` names the
 * function and the separators at that depth count the current argument.
 */
export function argumentHintFor(
  text: string,
  caret: number,
  catalogue: Map<string, CatalogueEntry>,
): ArgumentHint | null {
  if (!text.startsWith("=") || caret < 1) return null;
  const before = text.slice(0, caret);
  let depth = 0;
  let separators = 0;
  for (let index = before.length - 1; index >= 0; index -= 1) {
    const character = before[index];
    if (character === '"') {
      // Skip a quoted string backwards; an unmatched quote just ends the scan.
      index -= 1;
      while (index >= 0 && before[index] !== '"') index -= 1;
      continue;
    }
    if (character === ")") {
      depth += 1;
      continue;
    }
    if (character === "(") {
      if (depth > 0) {
        depth -= 1;
        continue;
      }
      const match = /([A-Za-z_][A-Za-z0-9_.]*)$/.exec(before.slice(0, index));
      const name = match?.[1]?.toUpperCase() ?? "";
      const meta = catalogue.get(name);
      if (!meta) return null;
      const parts = splitSignature(meta.signature);
      if (parts.length === 0) return null;
      return { name, parts, active: Math.min(separators, parts.length - 1) };
    }
    if ((character === "," || character === ";") && depth === 0) separators += 1;
  }
  return null;
}
