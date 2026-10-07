/**
 * Pure helpers for applying an AI suggestion to a Writer paragraph.
 *
 * The suggestion replaces the selected range (or the whole paragraph) as one
 * run that carries the formatting of the first selected run, so the
 * paragraph's style and its surrounding runs stay untouched.
 */
import type { Run } from "../../lib/office-types";
import { emptyRun, insertText, normalizeParagraphRuns, runsText, splitRuns } from "./runs";

/** Model replies come back with CRLF and blank-line paragraph gaps; a
 * paragraph only holds single line breaks. */
export function normalizeAiText(text: string): string {
  return text
    .replace(/\r\n?/g, "\n")
    .replace(/\n{2,}/g, "\n")
    .trim();
}

/** The formatting of the run that holds the first selected character. */
function firstSelectedFormat(runs: Run[], from: number): Run {
  let position = 0;
  for (const run of runs) {
    const end = position + run.text.length;
    if (from < end && !(run.footnote || run.endnote || run.field)) {
      const { footnote: _footnote, endnote: _endnote, field: _field, revision: _revision, ...format } = run;
      return { ...emptyRun(), ...format, text: "" };
    }
    position = end;
  }
  return emptyRun();
}

/** Replaces `[from, to)` of a paragraph with AI text, keeping the first run's formatting. */
export function applyAiText(runs: Run[], from: number, to: number, text: string): Run[] {
  const total = runsText(runs).length;
  const start = Math.max(0, Math.min(from, total));
  const end = Math.max(start, Math.min(to, total));
  const replacement = normalizeAiText(text);
  if (end <= start) return insertText(runs, start, replacement);
  const template = firstSelectedFormat(runs, start);
  const [left, tail] = splitRuns(runs, start);
  const [, right] = splitRuns(tail, end - start);
  return normalizeParagraphRuns([...left, ...(replacement ? [{ ...template, text: replacement }] : []), ...right]);
}
