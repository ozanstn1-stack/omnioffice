/**
 * Small DOM, footnote, and MIME helpers used by WriterEditor.
 */
import type { Footnote, Run, TextDocument } from "../../lib/office-types";

export function applyVisualStyle(element: HTMLElement, property: string, value: string) {
  element.style.setProperty(property, value);
}

/** True when the selection overlaps `box` vertically, or when there is no layout to tell (jsdom). */
export function selectionShownIn(box: HTMLElement): boolean {
  const selection = window.getSelection();
  if (!selection || selection.rangeCount === 0) return true;
  const range = selection.getRangeAt(0);
  const rect = typeof range.getBoundingClientRect === "function" ? range.getBoundingClientRect() : null;
  const bounds = box.getBoundingClientRect();
  if (!rect || rect.height === 0 || bounds.height === 0) return true;
  return rect.bottom > bounds.top && rect.top < bounds.bottom;
}

export function mimeFromName(name: string): string {
  const extension = name.split(".").pop()?.toLowerCase() ?? "";
  switch (extension) {
    case "jpg":
    case "jpeg":
      return "image/jpeg";
    case "gif":
      return "image/gif";
    case "bmp":
      return "image/bmp";
    case "webp":
      return "image/webp";
    case "svg":
      return "image/svg+xml";
    default:
      return "image/png";
  }
}

/** Footnote area reservation at 8.5pt with a 1.0 line height. */
export function estimateNoteHeight(note: Footnote, contentWidthPx: number): number {
  const text = note.runs.map((run) => run.text).join("");
  const charsPerLine = Math.max(40, Math.floor(contentWidthPx / 4.6));
  const lines = Math.max(1, Math.ceil((text.length + 4) / charsPerLine));
  return lines * 13 + 6;
}

/** Footnote number in reference order (1-based). */
export function noteNumber(document: TextDocument, id: string): number {
  const order: string[] = [];
  const collect = (runs: Run[]) => {
    for (const run of runs) {
      const reference = run.footnote ?? run.endnote;
      if (reference && !order.includes(reference)) order.push(reference);
    }
  };
  for (const block of document.blocks) {
    if (block.type === "paragraph") collect(block.runs);
    if (block.type === "table")
      for (const row of block.table.rows)
        for (const cell of row.cells)
          for (const inner of cell.blocks) if (inner.type === "paragraph") collect(inner.runs);
  }
  return order.indexOf(id) + 1;
}
