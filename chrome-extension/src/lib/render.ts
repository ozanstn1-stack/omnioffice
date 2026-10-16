// pdf.js integration: rendering, text extraction and search.
// Everything runs locally in the browser; no network access is performed
// (the worker and the optional wasm module are bundled with the extension).

import * as pdfjs from "pdfjs-dist";
import type { PDFDocumentProxy } from "pdfjs-dist";
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
import { fileStem } from "./format";
import { PdfOpError } from "./pdf";

pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;

/** Base URL for assets served next to the app (extension or dev server). */
function assetBase(): string {
  const runtime = (globalThis as { chrome?: typeof chrome }).chrome;
  if (runtime?.runtime?.getURL) return runtime.runtime.getURL("");
  return new URL(".", location.href).href;
}

export interface RenderedPage {
  canvas: HTMLCanvasElement;
  width: number;
  height: number;
  /** Page size in points (72 dpi). */
  widthPt: number;
  heightPt: number;
}

/**
 * Runs `work` with an open document and tears the loading task down afterwards.
 *
 * pdf.js 6 removed `PDFDocumentProxy.destroy`; the loading task owns teardown,
 * so every call site goes through here instead of destroying the proxy.
 */
export async function withDocument<T>(bytes: Uint8Array, work: (doc: PDFDocumentProxy) => Promise<T>): Promise<T> {
  // pdf.js takes ownership of the buffer, so hand it a copy.
  const data = bytes.slice();
  const base = assetBase();
  const task = pdfjs.getDocument({
    data,
    useSystemFonts: true,
    cMapUrl: `${base}vendor/cmaps/`,
    cMapPacked: true,
    standardFontDataUrl: `${base}vendor/standard_fonts/`,
  });
  let doc: PDFDocumentProxy;
  try {
    doc = await task.promise;
  } catch (error) {
    await task.destroy().catch(() => undefined);
    const name = (error as { name?: string })?.name;
    if (name === "PasswordException") {
      throw new PdfOpError("password_required", "This PDF is password protected.");
    }
    throw new PdfOpError("invalid_pdf", "This file could not be read as a PDF. It may be damaged.");
  }
  try {
    return await work(doc);
  } finally {
    await task.destroy().catch(() => undefined);
  }
}

export async function renderPage(bytes: Uint8Array, pageNumber: number, dpi: number): Promise<RenderedPage> {
  return withDocument(bytes, async (doc) => {
    const page = await doc.getPage(pageNumber);
    const base = page.getViewport({ scale: 1 });
    const scale = dpi / 72;
    const viewport = page.getViewport({ scale });
    const canvas = document.createElement("canvas");
    canvas.width = Math.max(1, Math.round(viewport.width));
    canvas.height = Math.max(1, Math.round(viewport.height));
    const context = canvas.getContext("2d");
    if (!context) throw new PdfOpError("internal", "Canvas 2D is not available in this browser.");
    context.fillStyle = "#ffffff";
    context.fillRect(0, 0, canvas.width, canvas.height);
    // pdf.js 6 requires the canvas itself alongside the context.
    await page.render({ canvas, canvasContext: context, viewport }).promise;
    return {
      canvas,
      width: canvas.width,
      height: canvas.height,
      widthPt: base.width,
      heightPt: base.height,
    };
  });
}

function canvasToBytes(canvas: HTMLCanvasElement, type: string, quality: number): Promise<Uint8Array> {
  return new Promise((resolve, reject) => {
    canvas.toBlob(
      async (blob) => {
        if (!blob) {
          reject(new PdfOpError("conversion_failed", "The browser could not encode this image."));
          return;
        }
        resolve(new Uint8Array(await blob.arrayBuffer()));
      },
      type,
      quality,
    );
  });
}

export async function renderPageToJpeg(bytes: Uint8Array, pageNumber: number, dpi: number, quality = 75): Promise<Uint8Array> {
  const rendered = await renderPage(bytes, pageNumber, dpi);
  return canvasToBytes(rendered.canvas, "image/jpeg", quality / 100);
}

export async function convertToPng(bytes: Uint8Array, type: string): Promise<Uint8Array> {
  const blob = new Blob([bytes as BlobPart], { type: type || "image/png" });
  const bitmap = await createImageBitmap(blob).catch(() => null);
  if (!bitmap) throw new PdfOpError("invalid_image", "This image could not be decoded by the browser.");
  const canvas = document.createElement("canvas");
  canvas.width = bitmap.width;
  canvas.height = bitmap.height;
  const context = canvas.getContext("2d");
  if (!context) throw new PdfOpError("internal", "Canvas 2D is not available in this browser.");
  context.drawImage(bitmap, 0, 0);
  bitmap.close();
  return canvasToBytes(canvas, "image/png", 1);
}

export interface PageText {
  page: number;
  text: string;
}

export async function extractPageText(bytes: Uint8Array, pageNumber: number): Promise<string> {
  return withDocument(bytes, async (doc) => {
    const page = await doc.getPage(pageNumber);
    const content = await page.getTextContent();
    return content.items
      .map((item) => ("str" in item ? item.str : ""))
      .join(" ")
      .replace(/\s+/g, " ")
      .trim();
  });
}

export async function documentHasText(bytes: Uint8Array, samplePages = 4): Promise<boolean> {
  return withDocument(bytes, async (doc) => {
    const total = doc.numPages;
    const step = Math.max(1, Math.floor(total / samplePages));
    for (let pageNumber = 1; pageNumber <= total; pageNumber += step) {
      const page = await doc.getPage(pageNumber);
      const content = await page.getTextContent();
      const text = content.items.map((item) => ("str" in item ? item.str : "")).join("");
      if (text.trim().length > 3) return true;
    }
    return false;
  });
}

export interface PdfSearchMatch {
  page: number;
  snippet: string;
}

export interface PdfSearchResult {
  matches: PdfSearchMatch[];
  pagesWithMatches: number;
  totalMatches: number;
  truncated: boolean;
  hasTextLayer: boolean;
}

/** Case-insensitive full-text search over the document's text layer. */
export async function searchDocument(
  bytes: Uint8Array,
  query: string,
  maxResults: number,
  onProgress?: (page: number, total: number) => void,
  cancelled?: () => boolean,
): Promise<PdfSearchResult> {
  const needle = query.trim().toLowerCase();
  if (!needle) throw new PdfOpError("invalid_input", "Enter the text to search for.");
  return withDocument(bytes, async (doc) => {
    const matches: PdfSearchMatch[] = [];
    let pagesWithMatches = 0;
    let hasTextLayer = false;
    let truncated = false;
    for (let pageNumber = 1; pageNumber <= doc.numPages; pageNumber += 1) {
      if (cancelled?.()) throw new PdfOpError("cancelled", "Search cancelled.");
      onProgress?.(pageNumber, doc.numPages);
      const page = await doc.getPage(pageNumber);
      const content = await page.getTextContent();
      const text = content.items
        .map((item) => ("str" in item ? item.str : ""))
        .join(" ")
        .replace(/\s+/g, " ")
        .trim();
      if (text.length > 3) hasTextLayer = true;
      const haystack = text.toLowerCase();
      let from = 0;
      let pageMatches = 0;
      while (true) {
        const position = haystack.indexOf(needle, from);
        if (position === -1) break;
        const start = Math.max(0, position - 48);
        const end = Math.min(text.length, position + needle.length + 48);
        matches.push({
          page: pageNumber,
          snippet: `${start > 0 ? "…" : ""}${text.slice(start, end)}${end < text.length ? "…" : ""}`,
        });
        pageMatches += 1;
        from = position + needle.length;
        if (matches.length >= maxResults) {
          truncated = true;
          break;
        }
      }
      if (pageMatches > 0) pagesWithMatches += 1;
      if (truncated) break;
    }
    return { matches, pagesWithMatches, totalMatches: matches.length, truncated, hasTextLayer };
  });
}

export function suggestImageFileName(pdfName: string, page: number, format: "png" | "jpeg"): string {
  return `${fileStem(pdfName)}_page_${String(page).padStart(3, "0")}.${format === "jpeg" ? "jpg" : "png"}`;
}

export { canvasToBytes };
