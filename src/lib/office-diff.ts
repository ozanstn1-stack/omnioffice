/**
 * Model-level office document diff (C13) - the facade the UI imports.
 *
 * The comparison runs on the same models the editors hold, so a large document
 * is compared structurally (paragraphs and table cells, cells and sheets,
 * slides) instead of being rendered and compared page by page. The engines are
 * separate modules so each stays small and independently testable:
 *
 * - `office-diff-writer.ts`  - Writer paragraphs and table cells
 * - `office-diff-sheets.ts`  - Calc sheets and cells
 * - `office-diff-slides.ts`  - Impress slides
 * - `text-diff.ts`           - the word-level segments shared by all three
 */
import { diffTextDocuments } from "./office-diff-writer";
import { diffWorkbooks } from "./office-diff-sheets";
import { diffDecks } from "./office-diff-slides";
import type { OfficeDiff } from "./office-diff-types";

export type {
  CalcCellChange,
  CalcDiff,
  DiffKind,
  DiffSegment,
  DiffSummary,
  ImpressDiff,
  OfficeDiff,
  SlideChange,
  WriterChange,
  WriterDiff,
} from "./office-diff-types";
export { MAX_WRITER_CHANGES, diffTextDocuments } from "./office-diff-writer";
export { MAX_CALC_CHANGES, diffWorkbooks } from "./office-diff-sheets";
export { MAX_SLIDE_CHANGES, diffDecks } from "./office-diff-slides";
export { diffSegments } from "./text-diff";

/** One entry point for the UI: the document kind decides the engine. */
export function diffOfficeDocuments(kind: "writer" | "calc" | "impress", base: unknown, current: unknown): OfficeDiff {
  if (kind === "writer") return diffTextDocuments(base as never, current as never);
  if (kind === "calc") return diffWorkbooks(base as never, current as never);
  return diffDecks(base as never, current as never);
}

/** True when the document pair has no differences. */
export function diffIsEmpty(diff: OfficeDiff): boolean {
  if (diff.kind === "writer") return diff.changes.length === 0;
  if (diff.kind === "calc")
    return diff.cells.length === 0 && diff.sheetsAdded.length === 0 && diff.sheetsRemoved.length === 0;
  return diff.slides.length === 0;
}
