/**
 * Compare two Writer documents and produce a tracked-changes document.
 *
 * The result is the **revised** document's structure with word-level
 * differences marked as revisions: text only in the revised document is an
 * `insert`, text only in the base is a `delete` (kept in the run list so the
 * existing accept/reject flow works). Paragraphs are aligned by position;
 * a paragraph only in one side is inserted or deleted as a whole. The diff is
 * case-sensitive, so Turkish `İ`/`i` and `I`/`ı` are distinguished.
 */
import { emptyRun } from "./runs";
import { newRevision } from "./revisions";
import type { Block, Run, TextDocument } from "../../lib/office-types";

/** Splits text into tokens, keeping whitespace runs so the text round-trips. */
function tokenize(text: string): string[] {
  return text.match(/\s+|\S+/g) ?? [];
}

/** Longest common subsequence of tokens; returns matched (base, revised) pairs. */
function commonTokens(base: string[], revised: string[]): [number, number][] {
  const rows = base.length;
  const cols = revised.length;
  const table: number[][] = Array.from({ length: rows + 1 }, () => new Array<number>(cols + 1).fill(0));
  for (let i = rows - 1; i >= 0; i -= 1) {
    for (let j = cols - 1; j >= 0; j -= 1) {
      table[i][j] = base[i] === revised[j] ? table[i + 1][j + 1] + 1 : Math.max(table[i + 1][j], table[i][j + 1]);
    }
  }
  const matches: [number, number][] = [];
  let i = 0;
  let j = 0;
  while (i < rows && j < cols) {
    if (base[i] === revised[j]) {
      matches.push([i, j]);
      i += 1;
      j += 1;
    } else if (table[i + 1][j] >= table[i][j + 1]) {
      i += 1;
    } else {
      j += 1;
    }
  }
  return matches;
}

function runsText(runs: Run[]): string {
  return runs.map((run) => run.text).join("");
}

function makeRun(template: Run | undefined, text: string, kind: "insert" | "delete" | null, author: string): Run {
  const run = template ? { ...template } : emptyRun();
  return { ...run, text, revision: kind ? newRevision(kind, author) : null };
}

/** Word-level diff of one paragraph's runs, returning runs with revision marks. */
export function diffRuns(base: Run[], revised: Run[], author: string): Run[] {
  const template = revised[0] ?? base[0];
  const baseTokens = tokenize(runsText(base));
  const revisedTokens = tokenize(runsText(revised));
  const matches = commonTokens(baseTokens, revisedTokens);
  const runs: Run[] = [];
  let baseIndex = 0;
  let revisedIndex = 0;
  const boundaries: [number, number][] = [...matches, [baseTokens.length, revisedTokens.length]];
  for (const [matchBase, matchRevised] of boundaries) {
    const deleted = baseTokens.slice(baseIndex, matchBase).join("");
    const inserted = revisedTokens.slice(revisedIndex, matchRevised).join("");
    if (deleted) runs.push(makeRun(template, deleted, "delete", author));
    if (inserted) runs.push(makeRun(template, inserted, "insert", author));
    if (matchBase < baseTokens.length) {
      runs.push(makeRun(template, baseTokens[matchBase], null, author));
    }
    baseIndex = matchBase + 1;
    revisedIndex = matchRevised + 1;
  }
  return runs.length ? runs : [emptyRun()];
}

/**
 * Returns the revised document with the differences against `base` marked as
 * tracked changes. Tables and other blocks are kept as they are in `revised`.
 */
export function diffDocuments(base: TextDocument, revised: TextDocument, author = "Comparison"): TextDocument {
  const document: TextDocument = structuredClone(revised);
  const blocks: Block[] = [];
  const max = Math.max(base.blocks.length, revised.blocks.length);
  for (let index = 0; index < max; index += 1) {
    const baseBlock = base.blocks[index];
    const revisedBlock = revised.blocks[index];
    if (baseBlock?.type === "paragraph" && revisedBlock?.type === "paragraph") {
      blocks.push({ ...revisedBlock, runs: diffRuns(baseBlock.runs, revisedBlock.runs, author) });
    } else if (revisedBlock?.type === "paragraph") {
      // Only in the revised document: every run is an insertion.
      blocks.push({ ...revisedBlock, runs: revisedBlock.runs.map((run) => makeRun(run, run.text, "insert", author)) });
    } else if (baseBlock?.type === "paragraph") {
      // Only in the base document: every run is a deletion.
      blocks.push({ ...baseBlock, runs: baseBlock.runs.map((run) => makeRun(run, run.text, "delete", author)) });
    } else if (revisedBlock) {
      blocks.push(structuredClone(revisedBlock));
    }
  }
  document.blocks = blocks;
  document.showRevisions = true;
  return document;
}
