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
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

/** True for the whitespace and control characters a URL may be padded with. */
function isIgnorableHrefChar(code: number): boolean {
  return code <= 0x20 || code === 0x7f;
}

/**
 * Screens a link target before it reaches an `href` attribute.
 *
 * Escaping alone cannot make an `href` safe: `javascript:alert(1)` needs no
 * character escaped, and `data:`/`vbscript:` are just as readable. Only the
 * schemes the Writer knows how to open, plus same-document `#` references, are
 * allowed through; anything else is dropped so the run renders as plain text.
 */
export function safeHref(value: string | null | undefined): string | null {
  if (typeof value !== "string") return null;
  let start = 0;
  let end = value.length;
  while (start < end && isIgnorableHrefChar(value.charCodeAt(start))) start += 1;
  while (end > start && isIgnorableHrefChar(value.charCodeAt(end - 1))) end -= 1;
  const trimmed = value.slice(start, end);
  if (trimmed === "") return null;
  if (trimmed.startsWith("#")) return trimmed;
  return /^(?:https?:|mailto:)/i.test(trimmed) ? trimmed : null;
}

/**
 * Style values are validated, not escaped.
 *
 * `&quot;` inside a `style` attribute still decodes to a quote before the CSS
 * parser runs, so escaping buys nothing here: a value has to look like a colour,
 * a font family or a font size, or it is dropped.
 */
const CSS_HEX_COLOR = /^#(?:[0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/i;
const CSS_COLOR_KEYWORD = /^[a-z]+$/i;
const CSS_RGB_COLOR = /^rgba?\(([^()]*)\)$/i;
const CSS_RGB_CHANNEL = /^\d{1,3}(?:\.\d+)?$/;
const CSS_ALPHA = /^(?:0|1|0?\.\d+|\d{1,3}%)$/;
const CSS_FONT_FAMILY = /^[A-Za-z0-9 ,_-]+$/;
/** Longest accepted `font-family` value. */
const FONT_FAMILY_MAX_LENGTH = 100;

function sanitizeColor(value: string | null | undefined): string | null {
  if (typeof value !== "string") return null;
  const candidate = value.trim();
  if (candidate === "") return null;
  if (CSS_HEX_COLOR.test(candidate) || CSS_COLOR_KEYWORD.test(candidate)) return candidate;
  const rgb = CSS_RGB_COLOR.exec(candidate);
  if (!rgb) return null;
  const parts = rgb[1].split(",").map((part) => part.trim());
  if (parts.length !== 3 && parts.length !== 4) return null;
  if (!parts.slice(0, 3).every((part) => CSS_RGB_CHANNEL.test(part) && Number(part) <= 255)) return null;
  if (parts[3] !== undefined && !CSS_ALPHA.test(parts[3])) return null;
  return candidate;
}

function sanitizeFontFamily(value: string | null | undefined): string | null {
  if (typeof value !== "string") return null;
  const candidate = value.trim();
  if (candidate === "" || candidate.length > FONT_FAMILY_MAX_LENGTH) return null;
  return CSS_FONT_FAMILY.test(candidate) ? candidate : null;
}

function sanitizeFontSize(value: number | null | undefined): number | null {
  if (typeof value !== "number" || !Number.isFinite(value)) return null;
  return value >= 1 && value <= 1000 ? value : null;
}

export interface RunRenderOptions {
  /** Footnote/endnote id -> displayed number. */
  noteNumbers?: Record<string, number>;
  /** Field values to display instead of the cached text (page/pages). */
  fieldValues?: Record<string, string>;
  /** Show tracked changes (deletions struck through, insertions underlined). */
  showRevisions?: boolean;
}

/** Context for field values the renderer derives instead of reading the run. */
export interface DocumentFieldContext {
  page?: number;
  pages?: number;
  title?: string;
  author?: string;
  /** Injectable clock so tests are deterministic. */
  now?: Date;
}

/**
 * Builds the `kind:target` -> value map `runsToHtml` expects. Page numbers come
 * from the pagination result; title/author from the document metadata; date and
 * time from the clock, so those fields refresh instead of showing the value
 * cached at insertion time (audit M16).
 */
export function fieldValuesFor(context: DocumentFieldContext = {}): Record<string, string> {
  const now = context.now ?? new Date();
  const values: Record<string, string> = {
    "date:": now.toLocaleDateString(),
    "time:": now.toLocaleTimeString(),
  };
  if (context.page && context.page > 0) values["page:"] = String(context.page);
  if (context.pages && context.pages > 0) values["pages:"] = String(context.pages);
  if (context.title) values["title:"] = context.title;
  if (context.author) values["author:"] = context.author;
  return values;
}

/**
 * Numbers ordered-list paragraphs the way a word processor does: consecutive
 * numbered paragraphs at the same level increment, a deeper level starts its
 * own counter at the paragraph's `start`, and returning to a shallower level
 * continues that level's counter. Any non-numbered block ends the series, and
 * a numbered item whose `start` differs from the running list restarts its
 * level at that `start`, so two adjacent lists with different starts do not
 * continue each other (audit M16: ordered lists used to render every item as
 * "1"). A level's counter remembers the start it belongs to; an item with the
 * same start continues, one with a different start restarts.
 */
export function orderedListNumbers(blocks: Block[]): Map<number, number> {
  const numbers = new Map<number, number>();
  const counters: ({ value: number; start: number } | undefined)[] = [];
  blocks.forEach((block, index) => {
    if (block.type !== "paragraph" || !block.props.list || block.props.list.kind !== "number") {
      counters.length = 0;
      return;
    }
    const level = Math.max(0, Math.min(8, block.props.list.level));
    counters.length = level + 1;
    const start = Math.max(1, Math.round(block.props.list.start) || 1);
    const counter = counters[level];
    const next = counter && counter.start === start ? counter.value + 1 : start;
    counters[level] = { value: next, start };
    numbers.set(index, next);
  });
  return numbers;
}

/** The marker an ordered-list item shows, given its computed position. */
export function orderedListMarker(props: ParaProps, listNumber: number | undefined): string {
  if (!props.list) return "";
  if (props.list.kind === "number") return `${listNumber ?? props.list.start}.`;
  return ["•", "◦", "▪"][props.list.level % 3];
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
      // Final view: a tracked deletion is hidden, exactly like "All markup off"
      // in Word. Previously it was rendered as plain text, so deleted content
      // looked like final content.
      if (!showRevisions && run.revision?.kind === "delete") return "";
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
      // Hard line breaks (Shift+Enter stores "\n") must render as <br>, or HTML
      // collapses them to a space and the page preview disagrees with the model.
      const text = escapeHtml(run.text).replace(/\t/g, "&emsp;").replace(/\n/g, "<br>");
      if (text === "") return "";
      let html = text;
      if (run.bold) html = `<strong>${html}</strong>`;
      if (run.italic) html = `<em>${html}</em>`;
      if (run.underline) html = `<u>${html}</u>`;
      if (run.strike) html = `<s>${html}</s>`;
      if (run.superscript) html = `<sup>${html}</sup>`;
      if (run.subscript) html = `<sub>${html}</sub>`;
      const styles: string[] = [];
      const color = sanitizeColor(run.color);
      if (color) styles.push(`color:${color}`);
      const highlight = sanitizeColor(run.highlight);
      if (highlight) styles.push(`background-color:${highlight}`);
      const sizePt = sanitizeFontSize(run.sizePt);
      if (sizePt !== null) styles.push(`font-size:${sizePt}pt`);
      const font = sanitizeFontFamily(run.font);
      if (font) styles.push(`font-family:'${font}'`);
      if (styles.length) html = `<span style="${styles.join(";")}">${html}</span>`;
      const href = safeHref(run.link);
      if (href) html = `<a href="${escapeHtml(href)}" target="_blank" rel="noreferrer">${html}</a>`;
      if (run.revision && showRevisions) {
        const className =
          run.revision.kind === "delete"
            ? "writer-rev-delete"
            : run.revision.kind === "insert"
              ? "writer-rev-insert"
              : "writer-rev-format";
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
      runs.push({
        ...emptyRun(""),
        ...next,
        footnote: footnote ? noteId : null,
        endnote: footnote ? null : noteId,
        revision: null,
      });
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
          cached: el.getAttribute("data-field-cached") ?? el.textContent ?? "",
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
      // A link pasted from the clipboard is untrusted too, so it clears the
      // same scheme check before it can reach the model.
      const href = safeHref(el.getAttribute("href"));
      if (href) next.link = href;
    }
    if (el.style?.color) next.color = el.style.color;
    if (el.style?.backgroundColor) next.highlight = el.style.backgroundColor;
    if (el.style?.fontSize) {
      const sizePt = cssLengthToPt(el.style.fontSize);
      if (sizePt !== null) next.sizePt = sizePt;
    }
    if (el.style?.fontFamily) {
      const family = el.style.fontFamily
        .split(",")[0]
        ?.trim()
        .replace(/^['"]|['"]$/g, "");
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

export function wrapCellRuns(runs: Run[]): Extract<Block, { type: "paragraph" }> {
  return { type: "paragraph", props: defaultParaProps(), runs: normalizeParagraphRuns(runs) };
}

/** Splits a paragraph's plain text into a list of blocks (used by import). */
export function textToBlocks(text: string, props: ParaProps = defaultParaProps()): Block[] {
  return text
    .split(/\r?\n/)
    .map((line) => ({ type: "paragraph" as const, props: { ...props }, runs: [emptyRun(line)] }));
}
