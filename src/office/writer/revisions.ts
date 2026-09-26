/**
 * Writer tracked changes (V3).
 *
 * Mirrors the Rust `officecore::revisions` rules so the editor and the file
 * engines agree: deleted text stays in the run list until the revision is
 * accepted or rejected, insertions are underlined, deletions struck through.
 */
import type { Block, DocComment, RevisionMark, Run, RunFormat, TextDocument } from "../../lib/office-types";

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
      for (const row of block.table.rows) for (const cell of row.cells) cell.blocks.forEach((inner) => visit(inner, index));
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
 * unrelated text.
 */
export function trackRunChanges(previous: Run[], next: Run[], author: string): Run[] {
  const previousText = previous.map((run) => run.text).join("");
  const nextText = next.map((run) => run.text).join("");
  if (previousText === nextText && previous.length === next.length) return next;

  let prefix = 0;
  while (prefix < previousText.length && prefix < nextText.length && previousText[prefix] === nextText[prefix]) prefix += 1;
  let suffix = 0;
  while (
    suffix < previousText.length - prefix &&
    suffix < nextText.length - prefix &&
    previousText[previousText.length - 1 - suffix] === nextText[nextText.length - 1 - suffix]
  ) {
    suffix += 1;
  }
  const removed = previousText.slice(prefix, previousText.length - suffix);
  const added = nextText.slice(prefix, nextText.length - suffix);

  const style = next[0] ?? previous[0] ?? { text: "" };
  const base = { ...(style as Run) };
  delete (base as Partial<Run>).revision;

  const result: Run[] = [];
  if (prefix > 0) result.push({ ...base, text: nextText.slice(0, prefix) });
  if (removed.length > 0) {
    result.push({ ...base, text: removed, revision: newRevision("delete", author) });
  }
  if (added.length > 0) {
    result.push({ ...base, text: added, revision: newRevision("insert", author) });
  }
  if (suffix > 0) result.push({ ...base, text: nextText.slice(nextText.length - suffix) });
  if (result.length === 0) result.push({ ...base, text: "" });
  return result;
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
  const index = position < 0 ? (forward ? 0 : list.length - 1) : (position + (forward ? 1 : list.length - 1)) % list.length;
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
