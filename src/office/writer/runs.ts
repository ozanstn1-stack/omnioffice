/**
 * Pure run-level text operations for the Writer engine.
 *
 * The editor edits paragraphs natively in the DOM and then synchronises the
 * runs back into the model. Structural keys (Enter, Backspace at the start,
 * Tab, arrow keys at a paragraph edge) need to slice and re-join runs, so that
 * logic lives here as plain functions: no DOM, no React, fully unit tested.
 */
import type { ParaProps, Run } from "../../lib/office-types";

export function emptyRun(text = ""): Run {
  return {
    text,
    bold: false,
    italic: false,
    underline: false,
    strike: false,
    color: null,
    highlight: null,
    font: null,
    sizePt: null,
    link: null,
    comment: null,
    superscript: false,
    subscript: false,
  };
}

function formatOf(run: Run): string {
  const { text: _text, ...format } = run;
  return JSON.stringify(format);
}

export function sameFormat(a: Run, b: Run): boolean {
  return formatOf(a) === formatOf(b);
}

export function runsText(runs: Run[]): string {
  return runs.map((run) => run.text).join("");
}

/** True for runs that exist only as an anchor (footnote/endnote/field). Their
 * text is empty by design; dropping them deletes the note or field from the
 * document on the next synchronisation. */
function isStructuralRun(run: Run): boolean {
  return Boolean(run.footnote || run.endnote || run.field);
}

/** Drops empty runs and merges neighbours that share the same formatting.
 * Empty structural runs (notes, fields) are kept. */
export function normalizeRuns(runs: Run[]): Run[] {
  const out: Run[] = [];
  for (const run of runs) {
    if (!run.text && !isStructuralRun(run)) continue;
    const previous = out[out.length - 1];
    if (previous && sameFormat(previous, run)) {
      out[out.length - 1] = { ...previous, text: previous.text + run.text };
    } else {
      out.push({ ...run });
    }
  }
  return out;
}

/** Normalised runs, but never an empty list - an empty paragraph has one run. */
export function normalizeParagraphRuns(runs: Run[]): Run[] {
  const normalized = normalizeRuns(runs);
  return normalized.length > 0 ? normalized : [emptyRun()];
}

/**
 * Splits runs at a plain-text offset. Returns the text before and after; the
 * run that straddles the offset is divided so both halves keep their
 * formatting. Out-of-range offsets are clamped.
 */
export function splitRuns(runs: Run[], offset: number): [Run[], Run[]] {
  const left: Run[] = [];
  const right: Run[] = [];
  let position = 0;
  for (const run of runs) {
    const start = position;
    const end = position + run.text.length;
    if (end <= offset) {
      left.push({ ...run });
    } else if (start >= offset) {
      right.push({ ...run });
    } else {
      const cut = offset - start;
      left.push({ ...run, text: run.text.slice(0, cut) });
      right.push({ ...run, text: run.text.slice(cut) });
    }
    position = end;
  }
  return [normalizeParagraphRuns(left), normalizeParagraphRuns(right)];
}

/** Concatenates two run lists, keeping each side's own formatting. */
export function joinRuns(left: Run[], right: Run[]): Run[] {
  return normalizeParagraphRuns([...left, ...right]);
}

/** Inserts plain text at an offset, inheriting the formatting of the run before it. */
export function insertText(runs: Run[], offset: number, text: string): Run[] {
  const [left, right] = splitRuns(runs, offset);
  // `formatAtOffset` looks at the real preceding run, so typing at offset 0 of
  // an italic paragraph keeps the italics - `splitRuns` returns a synthetic empty
  // run for the left side there and would otherwise drop every attribute.
  const template = formatAtOffset(runs, offset);
  return normalizeParagraphRuns([...left, { ...template, text }, ...right]);
}

/** Replaces a character range with plain text (used by the browser's own edits). */
export function replaceRange(runs: Run[], from: number, to: number, text: string): Run[] {
  if (to <= from) return insertText(runs, from, text);
  const [left, tail] = splitRuns(runs, from);
  const [, right] = splitRuns(tail, to - from);
  const template = formatAtOffset(runs, from);
  return normalizeParagraphRuns([...left, ...(text ? [{ ...template, text }] : []), ...right]);
}

/** The formatting that a character inserted at `offset` should receive.
 * Structural anchors (notes/fields) are not inherited: typing next to a note
 * marker must produce plain text, never a second note reference. */
export function formatAtOffset(runs: Run[], offset: number): Run {
  const text = runsText(runs);
  const at = Math.max(0, Math.min(offset, text.length));
  let position = 0;
  let previous: Run | null = null;
  for (const run of runs) {
    if (at > position) previous = run;
    position += run.text.length;
  }
  const template = previous ?? runs[0] ?? null;
  if (!template) return emptyRun();
  const { footnote: _footnote, endnote: _endnote, field: _field, revision: _revision, ...format } = template;
  return { ...emptyRun(), ...format, text: "" };
}

/**
 * Splits on the last hard line break at or before `offset`, which is what
 * Shift+Enter needs: everything up to the break stays above, the rest moves
 * into a new paragraph.
 */
export function splitAtLineBreak(runs: Run[], offset: number): [Run[], Run[], number] {
  const text = runsText(runs);
  const at = Math.max(0, Math.min(offset, text.length));
  const before = text.lastIndexOf("\n", Math.max(0, at - 1));
  const cut = before < 0 ? at : before + 1;
  const [left, right] = splitRuns(runs, cut);
  return [left, right, cut];
}

/** Strips a leading list marker so Enter inside a list does not duplicate it. */
export function nextParagraphProps(props: ParaProps, text: string): ParaProps {
  // A heading only continues on the next line when there is text after the
  // caret; an empty Heading must fall back to Normal like every word processor.
  if (text.trim() === "" && props.style !== "Normal") return { ...props, style: "Normal", list: null };
  if (text.trim() === "" && props.list) return { ...props, list: null };
  return { ...props };
}

/** The list level shift requested by Tab / Shift+Tab, clamped to 0..8. */
export function nextListLevel(props: ParaProps, delta: number): ParaProps {
  if (!props.list) return props;
  return { ...props, list: { ...props.list, level: Math.max(0, Math.min(8, props.list.level + delta)) } };
}

/** Counts the visible lines of a paragraph so arrow keys know the last one. */
export function lineCount(runs: Run[]): number {
  const text = runsText(runs);
  if (text === "") return 1;
  return text.split("\n").length;
}

/** The plain-text offset of the start of a visual line (0-based line index). */
export function lineStartOffset(runs: Run[], line: number): number {
  const text = runsText(runs);
  if (line <= 0) return 0;
  let count = 0;
  let index = 0;
  while (count < line && index < text.length) {
    const next = text.indexOf("\n", index);
    if (next < 0) return text.length;
    index = next + 1;
    count += 1;
  }
  return index;
}

export function lineEndOffset(runs: Run[], line: number): number {
  const text = runsText(runs);
  const start = lineStartOffset(runs, line);
  const next = text.indexOf("\n", start);
  return next < 0 ? text.length : next;
}

/** Word boundaries around an offset, for Ctrl+Backspace / Ctrl+Delete. */
export function wordRangeAt(runs: Run[], offset: number, direction: "backward" | "forward"): [number, number] {
  const text = runsText(runs);
  const at = Math.max(0, Math.min(offset, text.length));
  const isWord = (character: string) => /[\p{L}\p{N}_]/u.test(character);
  if (direction === "backward") {
    let index = at;
    while (index > 0 && /\s/.test(text[index - 1])) index -= 1;
    while (index > 0 && isWord(text[index - 1])) index -= 1;
    return [index, at];
  }
  let index = at;
  while (index < text.length && /\s/.test(text[index])) index += 1;
  while (index < text.length && isWord(text[index])) index += 1;
  return [at, index];
}

/** Plain text of a paragraph, with tabs and line breaks preserved. */
export function paragraphText(runs: Run[]): string {
  return runsText(runs);
}
