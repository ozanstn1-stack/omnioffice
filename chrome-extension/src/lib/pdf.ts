// PDF operations for the browser build, implemented with pdf-lib.
//
// Everything here is framework-free and runs both in the extension page and in
// Node (the unit tests execute these functions directly), which keeps the
// browser toolkit honest: the same code path is tested without a browser.

import {
  degrees,
  PDFDocument,
  rgb,
  StandardFonts,
  type PDFImage,
  type PDFPage,
} from "pdf-lib";
import type {
  CompressOptions,
  ImageToPdfOptions,
  Metadata,
  NumberingOptions,
  PageBox,
  PagePlanItem,
  PdfInfo,
  SplitMode,
  WatermarkOptions,
} from "./types";
import { fileStem, hasEncryptDictionary, pdfVersionFromBytes } from "./format";

export class PdfOpError extends Error {
  code: string;
  constructor(code: string, message: string) {
    super(message);
    this.code = code;
  }
}

const PAGE_SIZES: Record<string, [number, number]> = {
  a3: [841.89, 1190.55],
  a4: [595.28, 841.89],
  a5: [419.53, 595.28],
  letter: [612, 792],
  legal: [612, 1008],
};

/** Loads a document, mapping pdf-lib failures to friendly error codes. */
export async function loadPdf(bytes: Uint8Array): Promise<PDFDocument> {
  // The browser build has no decryption support: refuse files that declare an
  // /Encrypt dictionary instead of writing a corrupt "unlocked" copy.
  if (hasEncryptDictionary(bytes)) {
    throw new PdfOpError("password_required", "This PDF is encrypted. Use the desktop app to unlock it.");
  }
  try {
    return await PDFDocument.load(bytes, { ignoreEncryption: true, updateMetadata: false });
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    if (message.toLowerCase().includes("encrypt")) {
      throw new PdfOpError("password_required", "This PDF is password protected.");
    }
    throw new PdfOpError("invalid_pdf", "This file could not be read as a PDF. It may be damaged.");
  }
}

export function pageCount(doc: PDFDocument): number {
  return doc.getPageCount();
}

export function pageBox(doc: PDFDocument, index: number): PageBox {
  const page = doc.getPage(index);
  const { width, height } = page.getSize();
  return { width, height, rotation: page.getRotation().angle };
}

export async function readInfo(fileName: string, bytes: Uint8Array): Promise<PdfInfo> {
  const encrypted = hasEncryptDictionary(bytes);
  let doc: PDFDocument;
  try {
    doc = await loadPdf(bytes);
  } catch (error) {
    if (error instanceof PdfOpError && error.code === "password_required") {
      return {
        fileName,
        fileSize: bytes.length,
        pageCount: 0,
        version: pdfVersionFromBytes(bytes),
        encrypted: true,
        title: "",
        author: "",
        subject: "",
        keywords: "",
        creator: "",
        producer: "",
        creationDate: "",
        modificationDate: "",
        pageSizes: [],
      };
    }
    throw error;
  }
  const sizes = new Map<string, { width: number; height: number; count: number }>();
  doc.getPages().forEach((page) => {
    const { width, height } = page.getSize();
    const key = `${Math.round(width)}x${Math.round(height)}`;
    const existing = sizes.get(key);
    if (existing) existing.count += 1;
    else sizes.set(key, { width, height, count: 1 });
  });
  return {
    fileName,
    fileSize: bytes.length,
    pageCount: doc.getPageCount(),
    version: pdfVersionFromBytes(bytes),
    encrypted,
    title: doc.getTitle() ?? "",
    author: doc.getAuthor() ?? "",
    subject: doc.getSubject() ?? "",
    keywords: doc.getKeywords() ?? "",
    creator: doc.getCreator() ?? "",
    producer: doc.getProducer() ?? "",
    creationDate: doc.getCreationDate()?.toISOString() ?? "",
    modificationDate: doc.getModificationDate()?.toISOString() ?? "",
    pageSizes: Array.from(sizes.values()),
  };
}

async function save(doc: PDFDocument, withObjectStreams = true): Promise<Uint8Array> {
  return doc.save({ useObjectStreams: withObjectStreams, addDefaultPage: false });
}

function normalizeSelection(pages: number[], total: number): number[] {
  const unique = Array.from(new Set(pages.filter((page) => page >= 1 && page <= total)));
  if (!unique.length) throw new PdfOpError("invalid_input", "Select at least one valid page.");
  return unique.sort((a, b) => a - b);
}

/** Builds a new document from a list of (source page, rotation) items. */
export async function applyPagePlan(
  bytes: Uint8Array,
  plan: PagePlanItem[],
): Promise<Uint8Array> {
  if (!plan.length) throw new PdfOpError("invalid_input", "The page plan is empty.");
  const source = await loadPdf(bytes);
  const total = source.getPageCount();
  const output = await PDFDocument.create();
  output.setTitle(source.getTitle() ?? "");
  output.setAuthor(source.getAuthor() ?? "");
  output.setSubject(source.getSubject() ?? "");
  output.setKeywords(source.getKeywords() ? source.getKeywords()!.split(/[;,]\s*/).filter(Boolean) : []);
  output.setCreator(source.getCreator() ?? "OmniOffice (extension)");
  output.setProducer("OmniOffice (extension)");

  for (const item of plan) {
    if (item.source < 1 || item.source > total) {
      throw new PdfOpError("range_out_of_bounds", `Page ${item.source} does not exist.`);
    }
    const [copied] = await output.copyPages(source, [item.source - 1]);
    if (item.rotation % 360 !== 0) {
      copied.setRotation(degrees(((item.rotation % 360) + 360) % 360));
    }
    output.addPage(copied);
  }
  return save(output);
}

export async function mergePdfs(inputs: { name: string; bytes: Uint8Array }[]): Promise<Uint8Array> {
  if (inputs.length < 2) throw new PdfOpError("invalid_input", "Select at least two PDF files.");
  const output = await PDFDocument.create();
  let first = true;
  for (const input of inputs) {
    const source = await loadPdf(input.bytes);
    const pages = await output.copyPages(source, source.getPageIndices());
    pages.forEach((page) => output.addPage(page));
    if (first) {
      output.setTitle(source.getTitle() ?? "");
      output.setAuthor(source.getAuthor() ?? "");
      output.setSubject(source.getSubject() ?? "");
      output.setCreator(source.getCreator() ?? "");
      first = false;
    }
  }
  output.setProducer("OmniOffice (extension)");
  output.setModificationDate(new Date());
  return save(output);
}

export async function extractPages(bytes: Uint8Array, pages: number[]): Promise<Uint8Array> {
  const doc = await loadPdf(bytes);
  const selection = normalizeSelection(pages, doc.getPageCount());
  return applyPagePlan(
    bytes,
    selection.map((page) => ({ source: page, rotation: 0 })),
  );
}

export async function deletePages(bytes: Uint8Array, pages: number[]): Promise<Uint8Array> {
  const doc = await loadPdf(bytes);
  const total = doc.getPageCount();
  const drop = new Set(normalizeSelection(pages, total));
  const keep = Array.from({ length: total }, (_, index) => index + 1).filter((page) => !drop.has(page));
  if (!keep.length) throw new PdfOpError("invalid_input", "You cannot delete every page.");
  return applyPagePlan(
    bytes,
    keep.map((page) => ({ source: page, rotation: 0 })),
  );
}

export async function rotatePages(bytes: Uint8Array, pages: number[], delta: number): Promise<Uint8Array> {
  const doc = await loadPdf(bytes);
  const total = doc.getPageCount();
  const target = pages.length ? new Set(normalizeSelection(pages, total)) : new Set(Array.from({ length: total }, (_, i) => i + 1));
  const plan: PagePlanItem[] = [];
  for (let index = 0; index < total; index += 1) {
    const page = doc.getPage(index);
    const current = page.getRotation().angle;
    plan.push({
      source: index + 1,
      rotation: target.has(index + 1) ? current + delta : current,
    });
  }
  return applyPagePlan(bytes, plan);
}

export interface SplitPart {
  fileName: string;
  bytes: Uint8Array;
  firstPage: number;
  lastPage: number;
}

export function planSplit(mode: SplitMode, total: number): [number, number][] {
  if (mode.mode === "ranges") {
    let groups: [number, number][] = [];
    for (const range of mode.ranges) {
      const parts = range.split("-");
      const start = Number(parts[0]?.trim());
      const end = parts[1] ? Number(parts[1].trim()) : start;
      if (!Number.isFinite(start) || !Number.isFinite(end) || start < 1 || end < start || end > total) {
        throw new PdfOpError("range_out_of_bounds", `Invalid range "${range}" for a ${total}-page document.`);
      }
      groups.push([start, end]);
    }
    // Ranges may cover the document in several chunks; each range is a part.
    groups = groups.filter((group) => group[0] <= total);
    if (!groups.length) throw new PdfOpError("invalid_input", "No valid ranges given.");
    return groups;
  }
  if (mode.mode === "every_n") {
    if (mode.n < 1) throw new PdfOpError("invalid_input", "Pages per file must be at least 1.");
    const groups: [number, number][] = [];
    for (let start = 1; start <= total; start += mode.n) {
      groups.push([start, Math.min(total, start + mode.n - 1)]);
    }
    return groups;
  }
  if (mode.mode === "at_pages") {
    const cuts = Array.from(new Set(mode.pages.filter((page) => page > 1 && page <= total))).sort((a, b) => a - b);
    const groups: [number, number][] = [];
    let start = 1;
    for (const cut of cuts) {
      groups.push([start, cut - 1]);
      start = cut;
    }
    groups.push([start, total]);
    return groups;
  }
  return Array.from({ length: total }, (_, index) => [index + 1, index + 1] as [number, number]);
}

export async function splitPdf(baseName: string, bytes: Uint8Array, mode: SplitMode): Promise<SplitPart[]> {
  const doc = await loadPdf(bytes);
  const total = doc.getPageCount();
  const groups = planSplit(mode, total);
  const stem = fileStem(baseName);
  const parts: SplitPart[] = [];
  for (const [first, last] of groups) {
    const pages = Array.from({ length: last - first + 1 }, (_, index) => first + index);
    const partBytes = await extractPages(bytes, pages);
    parts.push({
      fileName: groups.length === 1 ? `${stem}_part.pdf` : `${stem}_${String(first).padStart(4, "0")}-${String(last).padStart(4, "0")}.pdf`,
      bytes: partBytes,
      firstPage: first,
      lastPage: last,
    });
  }
  return parts;
}

export async function editMetadata(bytes: Uint8Array, metadata: Metadata): Promise<Uint8Array> {
  const doc = await loadPdf(bytes);
  doc.setTitle(metadata.title);
  doc.setAuthor(metadata.author);
  doc.setSubject(metadata.subject);
  doc.setKeywords(metadata.keywords ? metadata.keywords.split(/[;,]\s*/).filter(Boolean) : []);
  doc.setCreator(metadata.creator);
  doc.setProducer(metadata.producer);
  return save(doc);
}

export async function clearMetadata(bytes: Uint8Array): Promise<Uint8Array> {
  const doc = await loadPdf(bytes);
  doc.setTitle("");
  doc.setAuthor("");
  doc.setSubject("");
  doc.setKeywords([]);
  doc.setCreator("");
  doc.setProducer("");
  return save(doc);
}

// ---------------------------------------------------------------------------
// Drawing helpers (watermarks, page numbers)
// ---------------------------------------------------------------------------

function hexToRgb(hex: string): [number, number, number] {
  const value = hex.replace("#", "").trim();
  const parse = (part: string) => parseInt(part, 16) / 255;
  if (value.length === 6) return [parse(value.slice(0, 2)), parse(value.slice(2, 4)), parse(value.slice(4, 6))];
  return [0, 0, 0];
}

async function embedUiFont(doc: PDFDocument, bold: boolean, bytes?: Uint8Array): Promise<Awaited<ReturnType<PDFDocument["embedFont"]>>> {
  if (bytes && bytes.length) {
    const { default: fontkit } = await import("@pdf-lib/fontkit");
    doc.registerFontkit(fontkit);
    try {
      return await doc.embedFont(bytes, { subset: true });
    } catch {
      // fall through to the standard font below
    }
  }
  return doc.embedFont(bold ? StandardFonts.HelveticaBold : StandardFonts.Helvetica);
}

async function drawTextStamp(
  doc: PDFDocument,
  page: PDFPage,
  text: string,
  options: {
    sizePt: number;
    color: string;
    opacity: number;
    rotationDeg: number;
    position: WatermarkOptions["position"];
    marginPt: number;
  },
  bold: boolean,
  fontBytes?: Uint8Array,
): Promise<void> {
  const font = await embedUiFont(doc, bold, fontBytes);
  const { width, height } = page.getSize();
  const textWidth = font.widthOfTextAtSize(text, options.sizePt);
  const textHeight = font.heightAtSize(options.sizePt);
  const margin = options.marginPt;
  let x: number;
  let y: number;
  switch (options.position) {
    case "top_left":
      x = margin;
      y = height - margin - textHeight;
      break;
    case "top_center":
      x = (width - textWidth) / 2;
      y = height - margin - textHeight;
      break;
    case "top_right":
      x = width - margin - textWidth;
      y = height - margin - textHeight;
      break;
    case "bottom_left":
      x = margin;
      y = margin;
      break;
    case "bottom_right":
      x = width - margin - textWidth;
      y = margin;
      break;
    case "bottom_center":
      x = (width - textWidth) / 2;
      y = margin;
      break;
    default:
      x = (width - textWidth) / 2;
      y = (height - textHeight) / 2;
  }
  const [r, g, b] = hexToRgb(options.color);
  page.drawText(text, {
    x,
    y,
    size: options.sizePt,
    font,
    color: rgb(r, g, b),
    opacity: options.opacity,
    rotate: degrees(-options.rotationDeg),
  });
}

export async function addTextWatermark(bytes: Uint8Array, options: WatermarkOptions): Promise<Uint8Array> {
  const text = options.text.trim();
  if (!text) throw new PdfOpError("invalid_input", "Enter the watermark text.");
  const doc = await loadPdf(bytes);
  const total = doc.getPageCount();
  const target = options.pages.length ? new Set(normalizeSelection(options.pages, total)) : null;
  const font = await embedUiFont(doc, options.fontFamily === "sans-bold", options.fontBytes);
  const [r, g, b] = hexToRgb(options.color);

  for (let index = 0; index < total; index += 1) {
    if (target && !target.has(index + 1)) continue;
    const page = doc.getPage(index);
    const { width, height } = page.getSize();
    if (!options.tile) {
      await drawTextStamp(doc, page, text, options, options.fontFamily === "sans-bold", options.fontBytes);
      continue;
    }
    const size = options.sizePt;
    const stepX = font.widthOfTextAtSize(text, size) + size * 2.2;
    const stepY = size * 3.2;
    const diagonal = Math.sqrt(width * width + height * height);
    const cols = Math.ceil(diagonal / stepX) + 2;
    const rows = Math.ceil(diagonal / stepY) + 2;
    const startX = (width - (cols - 1) * stepX) / 2;
    const startY = (height - (rows - 1) * stepY) / 2;
    for (let row = 0; row < rows; row += 1) {
      for (let col = 0; col < cols; col += 1) {
        page.drawText(text, {
          x: startX + col * stepX,
          y: startY + row * stepY,
          size,
          font,
          color: rgb(r, g, b),
          opacity: options.opacity,
          rotate: degrees(-options.rotationDeg),
        });
      }
    }
  }
  return save(doc);
}

function helveticaWidth(text: string, size: number, bold = false): number {
  const widths: Record<string, number> = {
    " ": 278, "/": 278, ".": 278, ":": 278, "-": 333,
  };
  let total = 0;
  for (const char of text) {
    if (char >= "0" && char <= "9") total += 556;
    else if (char === " " || char === "/" || char === "." || char === ":") total += widths[char];
    else if (char === "-") total += 333;
    else if (char >= "A" && char <= "Z") total += char === "I" ? 278 : 667;
    else if (char >= "a" && char <= "z") total += 500;
    else total += 556;
  }
  return (total / 1000) * size * (bold ? 1.05 : 1);
}

export async function addPageNumbers(bytes: Uint8Array, options: NumberingOptions): Promise<Uint8Array> {
  const doc = await loadPdf(bytes);
  const total = doc.getPageCount();
  const target = options.pages.length ? new Set(normalizeSelection(options.pages, total)) : null;
  const font = await doc.embedFont(StandardFonts.Helvetica);
  const [r, g, b] = hexToRgb(options.color);

  for (let index = 0; index < total; index += 1) {
    if (target && !target.has(index + 1)) continue;
    const page = doc.getPage(index);
    const { width, height } = page.getSize();
    const display = options.countFromStart ? options.startNumber + index : index + 1;
    const label =
      options.format === "page_n"
        ? `Page ${display}`
        : options.format === "n_of_total"
          ? `${display} / ${total}`
          : options.format === "page_n_of_total"
            ? `Page ${display} of ${total}`
            : `${display}`;
    const textWidth = helveticaWidth(label, options.sizePt);
    const margin = options.marginPt;
    let x: number;
    let y: number;
    switch (options.position) {
      case "top_left":
        x = margin;
        y = height - margin - options.sizePt;
        break;
      case "top_center":
        x = (width - textWidth) / 2;
        y = height - margin - options.sizePt;
        break;
      case "top_right":
        x = width - margin - textWidth;
        y = height - margin - options.sizePt;
        break;
      case "bottom_right":
        x = width - margin - textWidth;
        y = margin;
        break;
      case "bottom_center":
        x = (width - textWidth) / 2;
        y = margin;
        break;
      default:
        x = margin;
        y = margin;
    }
    page.drawText(label, { x, y, size: options.sizePt, font, color: rgb(r, g, b) });
  }
  return save(doc);
}

// ---------------------------------------------------------------------------
// Images -> PDF
// ---------------------------------------------------------------------------

export interface ImageItem {
  name: string;
  bytes: Uint8Array;
  type: string;
  rotation: number;
}

export async function imagesToPdf(items: ImageItem[], options: ImageToPdfOptions): Promise<Uint8Array> {
  if (!items.length) throw new PdfOpError("invalid_input", "Add at least one image.");
  const doc = await PDFDocument.create();
  doc.setProducer("OmniOffice (extension)");
  doc.setCreator("OmniOffice (extension)");

  for (const item of items) {
    let image: PDFImage;
    const lower = item.name.toLowerCase();
    const isPng = item.type.includes("png") || lower.endsWith(".png");
    if (isPng) {
      image = await doc.embedPng(item.bytes);
    } else if (item.type.includes("jpeg") || /\.jpe?g$/i.test(lower)) {
      image = await doc.embedJpg(item.bytes);
    } else {
      // Convert anything else (webp/bmp) through a canvas.
      const { convertToPng } = await import("./render");
      const converted = await convertToPng(item.bytes, item.type);
      image = await doc.embedPng(converted);
    }
    const imageWidth = image.width;
    const imageHeight = image.height;
    const [baseWidth, baseHeight] =
      options.pageSize === "original"
        ? [(imageWidth * 72) / options.assumedDpi, (imageHeight * 72) / options.assumedDpi]
        : options.pageSize === "custom"
          ? [options.customWidthPt, options.customHeightPt]
          : PAGE_SIZES[options.pageSize];

    const landscapeImage = imageWidth > imageHeight;
    let pageWidth = baseWidth;
    let pageHeight = baseHeight;
    if (options.orientation === "landscape") {
      pageWidth = Math.max(baseWidth, baseHeight);
      pageHeight = Math.min(baseWidth, baseHeight);
    } else if (options.orientation === "portrait") {
      pageWidth = Math.min(baseWidth, baseHeight);
      pageHeight = Math.max(baseWidth, baseHeight);
    } else if (landscapeImage !== baseWidth > baseHeight) {
      pageWidth = baseHeight;
      pageHeight = baseWidth;
    }

    const margin = Math.max(0, options.marginPt);
    const availableWidth = Math.max(1, pageWidth - margin * 2);
    const availableHeight = Math.max(1, pageHeight - margin * 2);
    let drawWidth: number;
    let drawHeight: number;
    if (options.fit === "fill") {
      const scale = Math.max(availableWidth / imageWidth, availableHeight / imageHeight);
      drawWidth = imageWidth * scale;
      drawHeight = imageHeight * scale;
    } else if (options.fit === "actual") {
      drawWidth = (imageWidth * 72) / options.assumedDpi;
      drawHeight = (imageHeight * 72) / options.assumedDpi;
    } else {
      const scale = Math.min(availableWidth / imageWidth, availableHeight / imageHeight);
      drawWidth = imageWidth * scale;
      drawHeight = imageHeight * scale;
    }

    const page = doc.addPage([pageWidth, pageHeight]);
    const x = (pageWidth - drawWidth) / 2;
    const y = (pageHeight - drawHeight) / 2;
    page.drawImage(image, {
      x,
      y,
      width: drawWidth,
      height: drawHeight,
      rotate: degrees(((item.rotation % 360) + 360) % 360),
    });
  }
  return save(doc);
}

// ---------------------------------------------------------------------------
// Compression
// ---------------------------------------------------------------------------

export async function compressLossless(bytes: Uint8Array): Promise<Uint8Array> {
  const doc = await loadPdf(bytes);
  doc.setProducer("OmniOffice (extension)");
  return doc.save({ useObjectStreams: true, addDefaultPage: false, objectsPerTick: 200 });
}

export async function compressWithRenderer(
  bytes: Uint8Array,
  options: CompressOptions,
  render: (bytes: Uint8Array, pageNumber: number, dpi: number) => Promise<Uint8Array>,
): Promise<Uint8Array> {
  const source = await loadPdf(bytes);
  const total = source.getPageCount();
  const output = await PDFDocument.create();
  output.setProducer("OmniOffice (extension)");
  output.setCreator("OmniOffice (extension)");
  for (let index = 0; index < total; index += 1) {
    const { width, height } = source.getPage(index).getSize();
    const jpeg = await render(bytes, index + 1, options.dpi);
    const image = await output.embedJpg(jpeg);
    const page = output.addPage([width, height]);
    page.drawImage(image, { x: 0, y: 0, width, height });
  }
  return save(output);
}

export { PAGE_SIZES };
