/**
 * Writer tracked changes (V3).
 *
 * Mirrors the Rust `officecore::revisions` rules so the editor and the file
 * engines agree: deleted text stays in the run list until the revision is
 * accepted or rejected, insertions are underlined, deletions struck through.
 */
import type { Block, DocComment, RevisionMark, Run, RunFormat, TextDocument } from "../../lib/office-types";
import { normalizeParagraphRuns } from "./runs";

export interface RevisionSummary {
  id: string;
  kind: string;
  author: string;
  date: string;
  text: string;
  blockIndex: number;
}

export function newRevision(kind: string, author: string, original?: RunFormat | null): RevisionMark {
  return {
    id: `rev-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`,
    kind,
    author: author.trim() || "Anonymous",
    date: new Date().toISOString(),
    original: original ?? null,
  };
}

/** Every pending revision in document order (body, tables and notes). */
export function revisionList(document: TextDocument): RevisionSummary[] {
  const out: RevisionSummary[] = [];
  const seen = new Set<string>();
  const visitRuns = (runs: Run[], blockIndex: number) => {
    for (const run of runs) {
      if (run.revision && !seen.has(run.revision.id)) {
        seen.add(run.revision.id);
        out.push({
          id: run.revision.id,
          kind: run.revision.kind,
          author: run.revision.author,
          date: run.revision.date,
          text: run.text.slice(0, 120),
          blockIndex,
        });
      }
    }
  };
  const visit = (block: Block, index: number) => {
    if (block.type === "paragraph") visitRuns(block.runs, index);
    if (block.type === "table") {
      for (const row of block.table.rows)
        for (const cell of row.cells) cell.blocks.forEach((inner) => visit(inner, index));
    }
  };
  document.blocks.forEach(visit);
  return out;
}

export function revisionCount(document: TextDocument): number {
  return revisionList(document).length;
}

/**
 * Diff helper used while suggest mode is on: text that appeared since
 * `previous` is marked as inserted, text that disappeared is kept in the list
 * with a delete mark so it can be accepted or rejected later.
 *
 * The diff is position-based (longest common prefix/suffix), which is what
 * typing and backspacing actually produce; it never invents revision marks for
 * unrelated text. It works on runs rather than on flattened text, so unchanged
 * runs keep their own formatting, links, fields and note anchors, an existing
 * insertion keeps its revision id, and deleting text the same suggestion
 * inserted cancels the insertion instead of producing a delete mark that
 * "reject all" would resurrect.
 */

/** Runs that carry no visible text (pending deletions, note/field anchors).
 *
 * They are invisible to the text diff but must survive the synchronisation, so
 * they are carried across separately by key rather than by character offset. */
function isTransparent(run: Run): boolean {
  return run.text.length === 0 || run.revision?.kind === "delete";
}

function anchorKey(run: Run): string | null {
  if (run.revision?.kind === "delete") return `delete:${run.revision.id}`;
  if (run.footnote) return `footnote:${run.footnote}`;
  if (run.endnote) return `endnote:${run.endnote}`;
  if (run.field) return `field:${run.field.kind}:${run.field.target}`;
  return null;
}

/** Visible runs only: anchors and deletions are handled out of band. */
function visibleRuns(runs: Run[]): Run[] {
  return runs.filter((run) => !isTransparent(run));
}

/** Clones the visible runs overlapping `[from, to)` in plain-text offsets. */
function sliceVisibleRuns(runs: Run[], from: number, to: number): Run[] {
  const out: Run[] = [];
  if (to <= from) return out;
  let cursor = 0;
  for (const run of runs) {
    if (isTransparent(run)) continue;
    const start = cursor;
    const end = cursor + run.text.length;
    cursor = end;
    if (end <= from) continue;
    if (start >= to) break;
    const sliceStart = Math.max(from, start) - start;
    const sliceEnd = Math.min(to, end) - start;
    if (sliceEnd > sliceStart) {
      out.push({ ...run, text: run.text.slice(sliceStart, sliceEnd), revision: run.revision ?? null });
    }
  }
  return out;
}

/** Number of visible characters before `target` in `runs`. */
function visibleOffset(runs: Run[], target: Run): number {
  let offset = 0;
  for (const run of runs) {
    if (run === target) return offset;
    if (!isTransparent(run)) offset += run.text.length;
  }
  return offset;
}

function commonEdges(previousText: string, nextText: string): { prefix: number; suffix: number } {
  let prefix = 0;
  while (prefix < previousText.length && prefix < nextText.length && previousText[prefix] === nextText[prefix])
    prefix += 1;
  let suffix = 0;
  while (
    suffix < previousText.length - prefix &&
    suffix < nextText.length - prefix &&
    previousText[previousText.length - 1 - suffix] === nextText[nextText.length - 1 - suffix]
  ) {
    suffix += 1;
  }
  return { prefix, suffix };
}

/** Inserts a zero-width (or deletion) run at a visible-text offset. */
function insertAnchorAt(runs: Run[], offset: number, anchor: Run): Run[] {
  const out: Run[] = [];
  let cursor = 0;
  let inserted = false;
  for (const run of runs) {
    if (!inserted && offset <= cursor) {
      out.push({ ...anchor });
      inserted = true;
    }
    if (isTransparent(run)) {
      out.push({ ...run });
      continue;
    }
    const end = cursor + run.text.length;
    if (!inserted && offset < end) {
      const cut = offset - cursor;
      if (cut > 0) out.push({ ...run, text: run.text.slice(0, cut) });
      out.push({ ...anchor });
      if (cut < run.text.length) out.push({ ...run, text: run.text.slice(cut) });
      inserted = true;
    } else {
      out.push({ ...run });
    }
    cursor = end;
  }
  if (!inserted) out.push({ ...anchor });
  return out;
}

/**
 * Merges the pre-assembled visible runs with the anchors of `previous` and
 * `next`.
 *
 * Anchors that existed before are taken from `previous` (preserving their
 * revision marks); anchors that only the DOM has are new and placed inside the
 * added region.
 */
function assembleWithAnchors(visible: Run[], previous: Run[], next: Run[]): Run[] {
  const previousText = visibleRuns(previous)
    .map((run) => run.text)
    .join("");
  const nextText = visibleRuns(next)
    .map((run) => run.text)
    .join("");
  const { prefix, suffix } = commonEdges(previousText, nextText);
  const removedLength = previousText.length - prefix - suffix;
  const addedLength = nextText.length - prefix - suffix;

  const remaining = new Map<string, number>();
  for (const anchor of previous) {
    if (!isTransparent(anchor)) continue;
    const key = anchorKey(anchor);
    if (key) remaining.set(key, (remaining.get(key) ?? 0) + 1);
  }

  const placements: { offset: number; order: number; run: Run }[] = [];
  let order = 0;
  for (const anchor of previous) {
    if (!isTransparent(anchor)) continue;
    const position = visibleOffset(previous, anchor);
    const offset =
      position <= prefix
        ? position
        : position <= prefix + removedLength
          ? prefix
          : prefix + removedLength + addedLength + (position - prefix - removedLength);
    placements.push({ offset, order: order++, run: { ...anchor } });
  }
  for (const anchor of next) {
    if (!isTransparent(anchor)) continue;
    const key = anchorKey(anchor);
    if (key) {
      const count = remaining.get(key) ?? 0;
      if (count > 0) {
        remaining.set(key, count - 1);
        continue;
      }
    }
    const position = visibleOffset(next, anchor);
    const offset =
      position <= prefix
        ? position
        : position <= prefix + addedLength
          ? prefix + removedLength + (position - prefix)
          : prefix + removedLength + addedLength + (position - prefix - addedLength);
    placements.push({ offset, order: order++, run: { ...anchor } });
  }

  placements.sort((a, b) => a.offset - b.offset || a.order - b.order);
  let result = visible;
  for (const placement of placements) {
    result = insertAnchorAt(result, placement.offset, placement.run);
  }
  return result;
}

export function trackRunChanges(previous: Run[], next: Run[], author: string): Run[] {
  const previousText = visibleRuns(previous)
    .map((run) => run.text)
    .join("");
  const nextText = visibleRuns(next)
    .map((run) => run.text)
    .join("");

  if (previousText === nextText) {
    return normalizeParagraphRuns(
      assembleWithAnchors(sliceVisibleRuns(previous, 0, previousText.length), previous, next),
    );
  }

  const { prefix, suffix } = commonEdges(previousText, nextText);
  const insertMark = newRevision("insert", author);
  const deleteMark = newRevision("delete", author);

  const result: Run[] = sliceVisibleRuns(previous, 0, prefix);
  for (const run of sliceVisibleRuns(previous, prefix, previousText.length - suffix)) {
    // Deleting text the same suggestion inserted cancels the insertion; a fresh
    // delete mark would make "reject all" resurrect the typed text.
    if (run.revision?.kind === "insert") continue;
    result.push({ ...run, revision: deleteMark });
  }
  for (const run of sliceVisibleRuns(next, prefix, nextText.length - suffix)) {
    result.push({ ...run, revision: run.revision?.kind === "insert" ? run.revision : insertMark });
  }
  result.push(...sliceVisibleRuns(previous, previousText.length - suffix, previousText.length));

  return normalizeParagraphRuns(assembleWithAnchors(result, previous, next));
}

/** Removes one revision: accept keeps the change, reject rolls it back. */
export function resolveRevision(document: TextDocument, id: string, accept: boolean): TextDocument {
  const resolveRuns = (runs: Run[]): Run[] => {
    const out: Run[] = [];
    for (const run of runs) {
      if (run.revision?.id === id) {
        if (run.revision.kind === "insert") {
          if (accept) out.push({ ...run, revision: null });
          continue;
        }
        if (run.revision.kind === "delete") {
          if (accept) continue;
          out.push({ ...run, revision: null });
          continue;
        }
        if (run.revision.kind === "format") {
          const restored = accept ? run : { ...run, ...run.revision.original, revision: null };
          out.push({ ...restored, revision: null });
          continue;
        }
      }
      out.push(run);
    }
    return out;
  };
  const resolveBlocks = (blocks: Block[]): Block[] =>
    blocks.map((block) => {
      if (block.type === "paragraph") return { ...block, runs: resolveRuns(block.runs) };
      if (block.type === "table") {
        return {
          ...block,
          table: {
            ...block.table,
            rows: block.table.rows.map((row) => ({
              ...row,
              cells: row.cells.map((cell) => ({ ...cell, blocks: resolveBlocks(cell.blocks) })),
            })),
          },
        };
      }
      return block;
    });
  return { ...document, blocks: resolveBlocks(document.blocks) };
}

export function acceptRevision(document: TextDocument, id: string): TextDocument {
  return resolveRevision(document, id, true);
}

export function rejectRevision(document: TextDocument, id: string): TextDocument {
  return resolveRevision(document, id, false);
}

export function resolveAll(document: TextDocument, accept: boolean): TextDocument {
  let current = document;
  for (const summary of revisionList(document)) {
    current = resolveRevision(current, summary.id, accept);
  }
  return { ...current, trackChanges: false };
}

export function acceptAll(document: TextDocument): TextDocument {
  return resolveAll(document, true);
}

export function rejectAll(document: TextDocument): TextDocument {
  return resolveAll(document, false);
}

export function nextRevision(document: TextDocument, after: string | null, forward: boolean): string | null {
  const list = revisionList(document);
  if (list.length === 0) return null;
  const position = after ? list.findIndex((summary) => summary.id === after) : -1;
  const index =
    position < 0 ? (forward ? 0 : list.length - 1) : (position + (forward ? 1 : list.length - 1)) % list.length;
  return list[index].id;
}

/** Runs as they should be displayed: hide or style deleted/inserted text. */
export function displayRuns(document: TextDocument, runs: Run[]): Run[] {
  if (document.showRevisions === false) return runs.filter((run) => run.revision?.kind !== "delete");
  return runs;
}

export function commentById(document: TextDocument, id: string | null): DocComment | undefined {
  if (!id) return undefined;
  return document.comments.find((comment) => comment.id === id);
}
