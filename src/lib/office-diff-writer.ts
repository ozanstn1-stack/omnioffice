/**
 * Writer diff (C13): paragraphs from the body, header and footer plus the
 * paragraphs inside table cells, aligned with an LCS on their exact text. The
 * paragraphs between two anchors are paired positionally and reported as
 * changed with word-level segments; the extras are added/removed.
 */
import type { Block, TextDocument } from "./office-types";
import { diffSegments } from "./text-diff";
import { countKind, emptySummary, type DiffSummary, type WriterChange, type WriterDiff } from "./office-diff-types";

/** Bound so a pathological pair cannot exhaust memory in the webview. */
export const MAX_WRITER_CHANGES = 2_000;

interface FlatParagraph {
  area: WriterChange["area"];
  blockIndex: number;
  cell?: { row: number; col: number };
  text: string;
}

function flattenBlocks(blocks: Block[], area: WriterChange["area"], out: FlatParagraph[]): void {
  blocks.forEach((block, offset) => {
    if (block.type === "paragraph") {
      out.push({ area, blockIndex: offset + 1, text: block.runs.map((run) => run.text).join("") });
    } else if (block.type === "table") {
      block.table.rows.forEach((row, rowIndex) => {
        row.cells.forEach((cell, colIndex) => {
          cell.blocks.forEach((inner) => {
            if (inner.type === "paragraph") {
              out.push({
                area,
                blockIndex: offset + 1,
                cell: { row: rowIndex + 1, col: colIndex + 1 },
                text: inner.runs.map((run) => run.text).join(""),
              });
            }
          });
        });
      });
    }
  });
}

function flattenWriter(document: TextDocument): FlatParagraph[] {
  const out: FlatParagraph[] = [];
  flattenBlocks(document.blocks, "body", out);
  flattenBlocks(document.header ?? [], "header", out);
  flattenBlocks(document.footer ?? [], "footer", out);
  return out;
}

function changeFromPair(base: FlatParagraph, current: FlatParagraph): WriterChange {
  const change: WriterChange = {
    kind: "changed",
    area: current.area,
    blockIndex: current.blockIndex,
    text: current.text,
    segments: diffSegments(base.text, current.text),
  };
  if (current.cell) change.cell = current.cell;
  return change;
}

function flattened(index: FlatParagraph, kind: "added" | "removed"): WriterChange {
  const change: WriterChange = { kind, area: index.area, blockIndex: index.blockIndex, text: index.text };
  if (index.cell) change.cell = index.cell;
  return change;
}

/**
 * LCS over whole strings. Kept local (not the token LCS) because a paragraph
 * must match as a unit; the token LCS is what produces the word-level detail.
 */
function lcsByText(base: string[], current: string[]): [number, number][] {
  let prefix = 0;
  while (prefix < base.length && prefix < current.length && base[prefix] === current[prefix]) prefix += 1;
  let suffix = 0;
  while (
    suffix < base.length - prefix &&
    suffix < current.length - prefix &&
    base[base.length - 1 - suffix] === current[current.length - 1 - suffix]
  ) {
    suffix += 1;
  }
  const matches: [number, number][] = [];
  for (let index = 0; index < prefix; index += 1) matches.push([index, index]);
  const baseMid = base.slice(prefix, base.length - suffix);
  const currentMid = current.slice(prefix, current.length - suffix);
  if (baseMid.length > 0 && currentMid.length > 0 && baseMid.length * currentMid.length <= 250_000) {
    const rows = baseMid.length;
    const cols = currentMid.length;
    const table: number[][] = Array.from({ length: rows + 1 }, () => new Array<number>(cols + 1).fill(0));
    for (let i = rows - 1; i >= 0; i -= 1) {
      for (let j = cols - 1; j >= 0; j -= 1) {
        table[i][j] =
          baseMid[i] === currentMid[j] ? table[i + 1][j + 1] + 1 : Math.max(table[i + 1][j], table[i][j + 1]);
      }
    }
    let i = 0;
    let j = 0;
    while (i < rows && j < cols) {
      if (baseMid[i] === currentMid[j]) {
        matches.push([prefix + i, prefix + j]);
        i += 1;
        j += 1;
      } else if (table[i + 1][j] >= table[i][j + 1]) {
        i += 1;
      } else {
        j += 1;
      }
    }
  }
  for (let index = 0; index < suffix; index += 1) {
    matches.push([base.length - suffix + index, current.length - suffix + index]);
  }
  return matches;
}

/** Writer paragraph and table-cell diff. */
export function diffTextDocuments(base: TextDocument, current: TextDocument): WriterDiff {
  const baseItems = flattenWriter(base);
  const currentItems = flattenWriter(current);
  const matches = lcsByText(
    baseItems.map((item) => item.text),
    currentItems.map((item) => item.text),
  );
  const changes: WriterChange[] = [];
  const summary: DiffSummary = emptySummary();
  let truncated = false;
  const emit = (change: WriterChange) => {
    if (changes.length >= MAX_WRITER_CHANGES) {
      truncated = true;
      return;
    }
    changes.push(change);
    countKind(summary, change.kind);
  };
  let baseIndex = 0;
  let currentIndex = 0;
  const flushGap = (baseEnd: number, currentEnd: number) => {
    const baseGap = baseItems.slice(baseIndex, baseEnd);
    const currentGap = currentItems.slice(currentIndex, currentEnd);
    const paired = Math.min(baseGap.length, currentGap.length);
    for (let position = 0; position < paired; position += 1)
      emit(changeFromPair(baseGap[position], currentGap[position]));
    for (const item of baseGap.slice(paired)) emit(flattened(item, "removed"));
    for (const item of currentGap.slice(paired)) emit(flattened(item, "added"));
  };
  for (const [baseMatch, currentMatch] of matches) {
    flushGap(baseMatch, currentMatch);
    baseIndex = baseMatch + 1;
    currentIndex = currentMatch + 1;
  }
  flushGap(baseItems.length, currentItems.length);
  return { kind: "writer", summary, changes, truncated };
}
