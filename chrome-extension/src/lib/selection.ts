// Context-menu "open selected text": the service worker opens the app with
// `app.html#text=<encoded selection>&src=<page URL>` and this module turns the
// text into a PDF the reader can open. Nothing is fetched: the text came from
// the page the user right-clicked, so no consent prompt or host permission is
// needed.

import { PDFDocument, StandardFonts, rgb, type PDFFont } from "pdf-lib";

/** Longest selection kept (the service worker trims to the same length). */
export const MAX_SELECTION_CHARS = 50_000;

export interface SelectedText {
  text: string;
  /** Host of the page the text came from, when it was an http(s) page. */
  source: string | null;
}

/** Parses the `#text=` fragment; null when there is no usable text. */
export function parseSelectedText(hash: string): SelectedText | null {
  const raw = hash.startsWith("#") ? hash.slice(1) : hash;
  if (!raw) return null;
  let params: URLSearchParams;
  try {
    params = new URLSearchParams(raw);
  } catch {
    return null;
  }
  const text = (params.get("text") ?? "").trim().slice(0, MAX_SELECTION_CHARS);
  if (!text) return null;
  let source: string | null = null;
  const src = params.get("src");
  if (src) {
    try {
      const parsed = new URL(src);
      if (parsed.protocol === "http:" || parsed.protocol === "https:") source = parsed.host;
    } catch {
      // A malformed source is just not shown.
    }
  }
  return { text, source };
}

/** "example.com - selected text.pdf", safe as a download name. */
export function fileNameForSelection(source: string | null): string {
  const host = (source ?? "").replace(/[\\/:*?"<>|\u0000-\u001f]/g, "_").trim();
  return host ? `${host} - selected text.pdf` : "selected text.pdf";
}

/** Splits text into lines that fit `width` (long words are broken). */
export function wrapText(text: string, width: number, measure: (value: string) => number): string[] {
  const lines: string[] = [];
  for (const paragraph of text.replace(/\r\n?/g, "\n").split("\n")) {
    const words = paragraph.split(/[ \t]+/).filter(Boolean);
    if (!words.length) {
      lines.push("");
      continue;
    }
    let line = "";
    for (const word of words) {
      const candidate = line ? `${line} ${word}` : word;
      if (measure(candidate) <= width) {
        line = candidate;
        continue;
      }
      if (line) lines.push(line);
      line = "";
      // A word wider than the line is cut into pieces that fit.
      let rest = word;
      while (measure(rest) > width && rest.length > 1) {
        let cut = rest.length - 1;
        while (cut > 1 && measure(rest.slice(0, cut)) > width) cut -= 1;
        lines.push(rest.slice(0, cut));
        rest = rest.slice(cut);
      }
      line = rest;
    }
    lines.push(line);
  }
  return lines;
}

/** Characters the font cannot encode become "?" instead of failing the PDF. */
function encodable(font: PDFFont, value: string): string {
  try {
    font.encodeText(value);
    return value;
  } catch {
    return Array.from(value)
      .map((char) => {
        try {
          font.encodeText(char);
          return char;
        } catch {
          return "?";
        }
      })
      .join("");
  }
}

/**
 * Lays the text out on A4 pages. `fontBytes` is the bundled Unicode font
 * (Turkish and other non-Latin-1 letters); without it Helvetica is used.
 */
export async function selectionToPdf(selection: SelectedText, fontBytes?: Uint8Array): Promise<File> {
  const doc = await PDFDocument.create();
  let font: PDFFont | null = null;
  if (fontBytes?.length) {
    try {
      const { default: fontkit } = await import("@pdf-lib/fontkit");
      doc.registerFontkit(fontkit);
      font = await doc.embedFont(fontBytes, { subset: true });
    } catch {
      font = null;
    }
  }
  font ??= await doc.embedFont(StandardFonts.Helvetica);

  const [pageWidth, pageHeight] = [595.28, 841.89];
  const margin = 56;
  const size = 12;
  const leading = 17;
  const usable = pageWidth - margin * 2;
  const measure = (value: string) => font.widthOfTextAtSize(encodable(font, value), size);
  const lines = wrapText(selection.text, usable, measure);

  let page = doc.addPage([pageWidth, pageHeight]);
  let y = pageHeight - margin;
  if (selection.source) {
    page.drawText(encodable(font, selection.source), { x: margin, y, size: 9, font, color: rgb(0.45, 0.45, 0.5) });
    y -= leading * 1.5;
  }
  for (const line of lines) {
    if (y < margin) {
      page = doc.addPage([pageWidth, pageHeight]);
      y = pageHeight - margin;
    }
    if (line) page.drawText(encodable(font, line), { x: margin, y, size, font, color: rgb(0.08, 0.08, 0.1) });
    y -= leading;
  }

  doc.setTitle(selection.source ? `Selected text from ${selection.source}` : "Selected text");
  doc.setCreator("OmniOffice");
  const bytes = await doc.save({ useObjectStreams: false });
  return new File([bytes as BlobPart], fileNameForSelection(selection.source), { type: "application/pdf" });
}
