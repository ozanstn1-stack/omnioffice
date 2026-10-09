/**
 * Pure run-level helpers for the Impress text frame editor.
 *
 * A slide paragraph mirrors the Writer's run model, but the paragraph itself
 * also carries bold/italic/underline/size/color. Runs are the run-level truth
 * whenever they are present, so these helpers render a paragraph to HTML for
 * the contentEditable surface, read the DOM back and apply the structural edits
 * (split, merge, indent) without dropping formatting. Writer's run and DOM
 * helpers are reused instead of duplicated.
 */
import type { Run, TextParagraph } from "../../lib/office-types";
import { emptyRun, formatAtOffset, joinRuns, normalizeParagraphRuns, runsText, splitRuns } from "../writer/runs";
import { domToRuns, runsToHtml } from "../writer/writerDom";

/** The run-level format a toolbar control changes; `undefined` leaves a field. */
export interface RunFormat {
  bold?: boolean;
  italic?: boolean;
  underline?: boolean;
  color?: string | null;
  sizePt?: number | null;
}

/** The formatting a paragraph contributes when it has no runs of its own. */
function paragraphRun(paragraph: TextParagraph): Run {
  return {
    ...emptyRun(paragraph.text),
    bold: paragraph.bold,
    italic: paragraph.italic,
    underline: paragraph.underline,
    color: paragraph.color,
    sizePt: paragraph.sizePt,
  };
}

/** Runs to render and edit: the stored runs when present, otherwise one run
 * built from the paragraph's own formatting. */
export function paragraphRuns(paragraph: TextParagraph): Run[] {
  return paragraph.runs.length > 0 ? paragraph.runs : [paragraphRun(paragraph)];
}

/** Plain text of a paragraph (runs win over the cached text when present). */
export function paragraphText(paragraph: TextParagraph): string {
  return paragraph.runs.length > 0 ? runsText(paragraph.runs) : paragraph.text;
}

/** Renders a paragraph as the inner HTML of a contentEditable surface. */
export function paragraphHtml(paragraph: TextParagraph): string {
  return runsToHtml(paragraphRuns(paragraph));
}

/** Reads the runs back out of a contentEditable paragraph. */
export function domToParagraphRuns(element: HTMLElement): Run[] {
  return domToRuns(element);
}

function patchRun(run: Run, format: RunFormat): Run {
  const next = { ...run };
  if (format.bold !== undefined) next.bold = format.bold;
  if (format.italic !== undefined) next.italic = format.italic;
  if (format.underline !== undefined) next.underline = format.underline;
  if (format.color !== undefined) next.color = format.color;
  if (format.sizePt !== undefined) next.sizePt = format.sizePt;
  return next;
}

/**
 * Applies a format to the runs' `[from, to)` plain-text range. The run that
 * straddles a boundary is split so both halves keep their own formatting, and
 * an empty range is returned untouched (the caret case is handled by the
 * caller through `withParagraphFormat`).
 */
export function applyRunFormat(runs: Run[], from: number, to: number, format: RunFormat): Run[] {
  if (to <= from) return runs;
  const [left, tail] = splitRuns(runs, from);
  const [middle, right] = splitRuns(tail, to - from);
  const formatted = middle.map((run) => patchRun(run, format));
  return joinRuns(joinRuns(left, formatted), right);
}

/**
 * Applies a format to the paragraph itself and to every run it owns, for a
 * collapsed caret where there is no range to paint.
 */
export function withParagraphFormat(paragraph: TextParagraph, format: RunFormat): TextParagraph {
  const next: TextParagraph = { ...paragraph };
  if (format.bold !== undefined) next.bold = format.bold;
  if (format.italic !== undefined) next.italic = format.italic;
  if (format.underline !== undefined) next.underline = format.underline;
  if (format.color !== undefined) next.color = format.color;
  if (format.sizePt !== undefined) next.sizePt = format.sizePt;
  if (next.runs.length > 0) next.runs = next.runs.map((run) => patchRun(run, format));
  return next;
}

/** Splits a paragraph at a plain-text offset. Both halves keep the paragraph's
 * level, bullet and alignment; the runs are divided without losing a format. */
export function splitParagraphAt(paragraph: TextParagraph, offset: number): [TextParagraph, TextParagraph] {
  const [left, right] = splitRuns(paragraphRuns(paragraph), offset);
  return [
    { ...paragraph, text: runsText(left), runs: left },
    { ...paragraph, text: runsText(right), runs: right },
  ];
}

/** Merges a paragraph into the one before it, keeping both formats. */
export function mergeParagraphs(previous: TextParagraph, current: TextParagraph): TextParagraph {
  const runs = joinRuns(paragraphRuns(previous), paragraphRuns(current));
  return { ...previous, text: runsText(runs), runs };
}

/** Toggles the bullet flag; turning bullets on keeps the current indent level. */
export function withBullet(paragraph: TextParagraph, bullet: boolean): TextParagraph {
  return { ...paragraph, bullet };
}

/** Tab / Shift+Tab: the indent level shift, clamped to 0..8. */
export function withLevel(paragraph: TextParagraph, delta: number): TextParagraph {
  const level = Math.max(0, Math.min(8, paragraph.level + delta));
  return level === paragraph.level ? paragraph : { ...paragraph, level };
}

/**
 * The formatting that typing at `offset` should inherit, based on the runs
 * around the caret.
 */
export function formatAt(paragraph: TextParagraph, offset: number): Run {
  return formatAtOffset(paragraphRuns(paragraph), offset);
}

/**
 * Re-maps runs onto a paragraph whose cached text changed in place (line-level
 * mapping). Each run keeps its slice of the new text at the same offsets; any
 * text past the old length extends the last run, extra old runs are dropped.
 */
export function remapRuns(runs: Run[], oldText: string, newText: string): Run[] {
  if (oldText === newText || runs.length === 0) return runs;
  const out: Run[] = [];
  let oldPos = 0;
  for (const run of runs) {
    const start = Math.min(oldPos, newText.length);
    const end = Math.min(oldPos + run.text.length, newText.length);
    if (end > start) out.push({ ...run, text: newText.slice(start, end) });
    oldPos += run.text.length;
  }
  if (oldPos < newText.length) {
    const template = out[out.length - 1] ?? runs[runs.length - 1];
    if (out.length > 0) {
      out[out.length - 1] = { ...template, text: template.text + newText.slice(oldPos) };
    } else {
      out.push({ ...template, text: newText });
    }
  }
  return normalizeParagraphRuns(out);
}
