/**
 * Shared shapes of the model-level office diff (C13). The engines live in
 * `office-diff-writer.ts`, `office-diff-sheets.ts` and `office-diff-slides.ts`;
 * `office-diff.ts` is the facade the UI imports.
 */

export type DiffKind = "added" | "removed" | "changed";

export interface DiffSummary {
  added: number;
  removed: number;
  changed: number;
}

/** One piece of a changed text: unchanged text, an insertion or a deletion. */
export interface DiffSegment {
  kind: "same" | "added" | "removed";
  text: string;
}

export interface WriterChange {
  kind: DiffKind;
  area: "body" | "header" | "footer";
  /** 1-based index of the block in its area. */
  blockIndex: number;
  /** Set when the change is inside a table cell. */
  cell?: { row: number; col: number };
  /** The text after the change for added/changed, before it for removed. */
  text: string;
  /** Word-level detail for a changed paragraph. */
  segments?: DiffSegment[];
}

export interface WriterDiff {
  kind: "writer";
  summary: DiffSummary;
  changes: WriterChange[];
  truncated: boolean;
}

export interface CalcCellChange {
  sheet: string;
  /** `A1`, `B7`, ... */
  address: string;
  kind: DiffKind;
  before: string;
  after: string;
}

export interface CalcDiff {
  kind: "calc";
  summary: DiffSummary;
  sheetsAdded: string[];
  sheetsRemoved: string[];
  cells: CalcCellChange[];
  truncated: boolean;
}

export interface SlideChange {
  kind: DiffKind;
  /** 1-based slide number. */
  index: number;
  title: string;
  before: string;
  after: string;
  segments?: DiffSegment[];
}

export interface ImpressDiff {
  kind: "impress";
  summary: DiffSummary;
  slides: SlideChange[];
  truncated: boolean;
}

export type OfficeDiff = WriterDiff | CalcDiff | ImpressDiff;

export function emptySummary(): DiffSummary {
  return { added: 0, removed: 0, changed: 0 };
}

export function countKind(summary: DiffSummary, kind: DiffKind): void {
  summary[kind] += 1;
}
