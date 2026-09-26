/**
 * Conversion between the Writer's DOM and the model's inline runs.
 *
 * The editor lets the browser own the contentEditable surface, so every render
 * writes runs to HTML and every keystroke reads the DOM back. Keeping both
 * directions in one module makes the round trip testable, which matters because
 * a lossy round trip silently drops formatting on save.
 */
import type { Block, ParaProps, Run } from "../../lib/office-types";
import { defaultParaProps } from "../../lib/office-types";
import { emptyRun, normalizeParagraphRuns } from "./runs";

export function escapeHtml(value: string): string {
  return value.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

export interface RunRenderOptions {
  /** Footnote/endnote id -> displayed number. */
  noteNumbers?: Record<string, number>;
  /** Field values to display instead of the cached text (page/pages). */
  fieldValues?: Record<string, string>;
  /** Show tracked changes (deletions struck through, insertions underlined). */
  showRevisions?: boolean;
}

function revisionAttributes(run: Run): string {
  const revision = run.revision;
  if (!revision) return "";
  return ` data-revision-id="${escapeHtml(revision.id)}" data-revision-kind="${escapeHtml(revision.kind)}" data-revision-author="${escapeHtml(revision.author)}" data-revision-date="${escapeHtml(revision.date)}"`;
}

/** Writes runs as the inner HTML of a contentEditable paragraph. */
export function runsToHtml(runs: Run[], options: RunRenderOptions = {}): string {
  const showRevisions = options.showRevisions !== false;
  return runs
    .map((run) => {
      const noteId = run.footnote ?? run.endnote;
      if (noteId) {
        const number = options.noteNumbers?.[noteId] ?? 1;
        const kind = run.footnote ? "footnote" : "endnote";
        return `<sup class="writer-note-ref" data-note-kind="${kind}" data-note-id="${escapeHtml(noteId)}"${revisionAttributes(run)}>${number}</sup>`;
      }
      if (run.field) {
        const field = run.field;
        const value = options.fieldValues?.[`${field.kind}:${field.target}`] ?? field.cached ?? "";
        return `<span class="writer-field" data-field-kind="${escapeHtml(field.kind)}" data-field-target="${escapeHtml(field.target)}" data-field-cached="${escapeHtml(field.cached ?? "")}"${revisionAttributes(run)}>${escapeHtml(value)}</span>`;
      }
      const text = escapeHtml(run.text).replace(/\t/g, "&emsp;");
      if (text === "") return "";
      let html = text;
      if (run.bold) html = `<strong>${html}</strong>`;
      if (run.italic) html = `<em>${html}</em>`;
      if (run.underline) html = `<u>${html}</u>`;
      if (run.strike) html = `<s>${html}</s>`;
      if (run.superscript) html = `<sup>${html}</sup>`;
      if (run.subscript) html = `<sub>${html}</sub>`;
      const styles: string[] = [];
      if (run.color) styles.push(`color:${run.color}`);
      if (run.highlight) styles.push(`background-color:${run.highlight}`);
      if (run.sizePt) styles.push(`font-size:${run.sizePt}pt`);
      if (run.font) styles.push(`font-family:'${run.font.replace(/'/g, "")}'`);
      if (styles.length) html = `<span style="${styles.join(";")}">${html}</span>`;
      if (run.link) html = `<a href="${escapeHtml(run.link)}" target="_blank" rel="noreferrer">${html}</a>`;
      if (run.revision && showRevisions) {
        const className = run.revision.kind === "delete" ? "writer-rev-delete" : run.revision.kind === "insert" ? "writer-rev-insert" : "writer-rev-format";
        html = `<span class="writer-rev ${className}"${revisionAttributes(run)} title="${escapeHtml(run.revision.author)}">${html}</span>`;
      } else if (run.revision) {
        html = `<span${revisionAttributes(run)}>${html}</span>`;
      }
      return html;
    })
    .join("");
}

/**
 * Converts a CSS length back into points.
 *
 * The editor writes `font-size:18pt` but the browser hands back whatever unit it
 * normalised to, so blindly treating the number as pixels used to turn 18pt into
 * 13.5pt on the first keystroke. Handle both units.
 */
export function cssLengthToPt(value: string): number | null {
  const match = /^\s*(-?[\d.]+)\s*(px|pt|em|rem|%)?\s*$/i.exec(value);
  if (!match) return null;
  const number = Number.parseFloat(match[1]);
  if (!Number.isFinite(number) || number <= 0) return null;
  switch ((match[2] ?? "px").toLowerCase()) {
    case "pt":
      return Math.round(number * 10) / 10;
    case "em":
    case "rem":
      return Math.round(number * 16 * 0.75 * 10) / 10;
    case "%":
      return null;
    default:
      return Math.round(number * 0.75 * 10) / 10;
  }
}

/**
 * Reads the runs back out of a contentEditable paragraph.
 *
 * Formatting is inherited down the tree, so a `<strong>` inside a coloured span
 * keeps both. `font-family` used to be written by `runsToHtml` but never read
 * back, which quietly lost the font on every save; it is read now.
 */
export function domToRuns(element: HTMLElement): Run[] {
  const runs: Run[] = [];
  const walk = (node: Node, inherited: Partial<Run>) => {
    if (node.nodeType === Node.TEXT_NODE) {
      const text = node.textContent ?? "";
      if (text) runs.push({ ...emptyRun(text), ...inherited });
      return;
    }
    if (node.nodeType !== Node.ELEMENT_NODE) return;
    const el = node as HTMLElement;
    const tag = el.tagName.toLowerCase();
    const next: Partial<Run> = { ...inherited };
    if (tag === "strong" || tag === "b") next.bold = true;
    if (tag === "em" || tag === "i") next.italic = true;
    if (tag === "u") next.underline = true;
    if (tag === "s" || tag === "strike" || tag === "del") next.strike = true;
    if (tag === "sup") next.superscript = true;
    if (tag === "sub") next.subscript = true;
    // Footnote / endnote references are atomic: they carry no editable text.
    const noteId = el.getAttribute("data-note-id");
    if (noteId) {
      const footnote = el.getAttribute("data-note-kind") !== "endnote";
      runs.push({ ...emptyRun(""), ...next, footnote: footnote ? noteId : null, endnote: footnote ? null : noteId, revision: null });
      return;
    }
    // Fields survive a DOM round trip through their data attributes.
    const fieldKind = el.getAttribute("data-field-kind");
    if (fieldKind) {
      runs.push({
        ...emptyRun(""),
        ...next,
        field: {
          kind: fieldKind,
          target: el.getAttribute("data-field-target") ?? "",
          cached: el.getAttribute("data-field-cached") ?? (el.textContent ?? ""),
        },
        revision: null,
      });
      return;
    }
    // Tracked revisions are inherited by their children so typing inside an
    // insertion keeps recording into the same revision.
    const revisionId = el.getAttribute("data-revision-id");
    if (revisionId) {
      next.revision = {
        id: revisionId,
        kind: el.getAttribute("data-revision-kind") ?? "insert",
        author: el.getAttribute("data-revision-author") ?? "Unknown",
        date: el.getAttribute("data-revision-date") ?? "",
        original: null,
      };
    }
    if (tag === "a") {
      const href = el.getAttribute("href");
      if (href) next.link = href;
    }
    if (el.style?.color) next.color = el.style.color;
    if (el.style?.backgroundColor) next.highlight = el.style.backgroundColor;
    if (el.style?.fontSize) {
      const sizePt = cssLengthToPt(el.style.fontSize);
      if (sizePt !== null) next.sizePt = sizePt;
    }
    if (el.style?.fontFamily) {
      const family = el.style.fontFamily.split(",")[0]?.trim().replace(/^['"]|['"]$/g, "");
      if (family) next.font = family;
    }
    if (tag === "br") {
      runs.push({ ...emptyRun("\n"), ...next });
      return;
    }
    el.childNodes.forEach((child) => walk(child, next));
  };
  element.childNodes.forEach((child) => walk(child, {}));
  return normalizeParagraphRuns(runs);
}

export function wrapCellRuns(runs: Run[]): Block {
  return { type: "paragraph", props: defaultParaProps(), runs: normalizeParagraphRuns(runs) };
}

/** Splits a paragraph's plain text into a list of blocks (used by import). */
export function textToBlocks(text: string, props: ParaProps = defaultParaProps()): Block[] {
  return text
    .split(/\r?\n/)
    .map((line) => ({ type: "paragraph" as const, props: { ...props }, runs: [emptyRun(line)] }));
}
