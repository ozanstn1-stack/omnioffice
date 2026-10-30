/**
 * Template fields: the `{{alan}}` placeholders a Writer template can carry.
 *
 * The rules mirror `crates/officecore/src/mailmerge.rs` (the file-based mail
 * merge): a field name is one or more letters, digits or underscores between
 * double braces, Turkish letters included, and placeholders are collected
 * from the body, the header and the footer, table cells included. Filling is
 * text-level and keeps the paragraph's first run formatting:
 *
 * - a placeholder inside one run keeps every other run untouched;
 * - a placeholder split across runs is filled by rebuilding that paragraph as
 *   a single run with the first run's formatting, because replacing text that
 *   spans styled runs cannot keep both styles' boundaries meaningful.
 */
import { defaultRun, type Block, type Run, type TableData, type TextDocument } from "./office-types";

/** A field name between double braces, e.g. `{{ad_soyad}}`. */
const PLACEHOLDER = /\{\{([\p{L}\p{N}_]+)\}\}/gu;

export interface TemplateField {
  name: string;
  /** How many placeholders carry this name (a name used twice counts twice). */
  count: number;
}

function scanText(text: string, counts: Map<string, number>): void {
  for (const match of text.matchAll(PLACEHOLDER)) {
    const name = match[1];
    counts.set(name, (counts.get(name) ?? 0) + 1);
  }
}

function scanBlocks(blocks: Block[], counts: Map<string, number>): void {
  for (const block of blocks) {
    if (block.type === "paragraph") {
      // Scan the joined text so a placeholder split across runs is found.
      scanText(block.runs.map((run) => run.text).join(""), counts);
    } else if (block.type === "table") {
      for (const row of block.table.rows) {
        for (const cell of row.cells) {
          scanBlocks(cell.blocks, counts);
        }
      }
    }
  }
}

/** Every placeholder in the document, with its occurrence count, sorted by name. */
export function templateFields(document: TextDocument): TemplateField[] {
  const counts = new Map<string, number>();
  scanBlocks(document.blocks, counts);
  scanBlocks(document.header ?? [], counts);
  scanBlocks(document.footer ?? [], counts);
  return [...counts.entries()]
    .map(([name, count]) => ({ name, count }))
    .sort((left, right) => left.name.localeCompare(right.name));
}

/** Fields whose value is missing or blank. */
export function missingTemplateValues(fields: TemplateField[], values: Record<string, string>): string[] {
  return fields.filter((field) => (values[field.name] ?? "").trim() === "").map((field) => field.name);
}

/** Replaces every complete placeholder in one run's text. */
function fillRunText(text: string, values: Record<string, string>): string {
  return text.replace(PLACEHOLDER, (placeholder, name: string) => {
    const value = (values[name] ?? "").trim();
    return value === "" ? placeholder : value;
  });
}

function fillRuns(runs: Run[], values: Record<string, string>): Run[] {
  const singleRun = runs.map((run) => ({ ...run, text: fillRunText(run.text, values) }));
  const joined = singleRun.map((run) => run.text).join("");
  if (!PLACEHOLDER.test(joined)) {
    PLACEHOLDER.lastIndex = 0;
    return singleRun;
  }
  PLACEHOLDER.lastIndex = 0;
  // At least one placeholder spans a run boundary; collapse the paragraph
  // into one run so the replacement cannot cut a placeholder in half.
  const format = runs[0] ?? defaultRun();
  return [{ ...format, text: fillRunText(joined, values) }];
}

function fillTable(table: TableData, values: Record<string, string>): TableData {
  return {
    ...table,
    rows: table.rows.map((row) => ({
      ...row,
      cells: row.cells.map((cell) => ({
        ...cell,
        blocks: fillBlocks(cell.blocks, values),
      })),
    })),
  };
}

function fillBlocks(blocks: Block[], values: Record<string, string>): Block[] {
  return blocks.map((block) => {
    if (block.type === "paragraph") return { ...block, runs: fillRuns(block.runs, values) };
    if (block.type === "table") return { ...block, table: fillTable(block.table, values) };
    return block;
  });
}

/** A copy of the document with every placeholder that has a value replaced. */
export function fillTemplate(document: TextDocument, values: Record<string, string>): TextDocument {
  const filled = structuredClone(document);
  filled.blocks = fillBlocks(filled.blocks, values);
  filled.header = fillBlocks(filled.header ?? [], values);
  filled.footer = fillBlocks(filled.footer ?? [], values);
  return filled;
}

/** True when the document carries at least one placeholder. */
export function hasTemplateFields(document: TextDocument): boolean {
  return templateFields(document).length > 0;
}
