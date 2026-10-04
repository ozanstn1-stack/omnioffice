// Integration tests for the browser build's PDF operations.
//
// They run in Node against the same framework-free modules the extension
// uses, generate their own sample documents (pdf-lib) and verify the results
// by re-parsing them with pdf.js - the same library the UI renders with.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { PDFDocument, StandardFonts, rgb } from "pdf-lib";
import * as pdfjs from "pdfjs-dist";

import {
  addPageNumbers,
  addTextWatermark,
  applyPagePlan,
  clearMetadata,
  compressLossless,
  deletePages,
  editMetadata,
  extractPages,
  imagesToPdf,
  loadPdf,
  mergePdfs,
  planSplit,
  readInfo,
  rotatePages,
  splitPdf,
} from "../src/lib/pdf";
import { parsePageSelection } from "../src/lib/format";

const here = dirname(fileURLToPath(import.meta.url));

async function samplePdf(pages: number, label = "Sample"): Promise<Uint8Array> {
  const doc = await PDFDocument.create();
  const font = await doc.embedFont(StandardFonts.Helvetica);
  for (let index = 1; index <= pages; index += 1) {
    const page = doc.addPage([595.28, 841.89]);
    page.drawText(`${label} page ${index}`, { x: 72, y: 700, size: 24, font, color: rgb(0.1, 0.1, 0.2) });
    page.drawRectangle({ x: 80, y: 500, width: 300, height: 120, borderColor: rgb(0.2, 0.3, 0.8), borderWidth: 2 });
  }
  doc.setTitle("Original title");
  doc.setAuthor("Test author");
  return doc.save({ useObjectStreams: false });
}

/** Extracts the text of one page using pdf.js (verifies real content). */
async function pageText(bytes: Uint8Array, pageNumber: number): Promise<string> {
  const doc = await pdfjs.getDocument({ data: bytes.slice(), isEvalSupported: false }).promise;
  try {
    const page = await doc.getPage(pageNumber);
    const content = await page.getTextContent();
    return content.items.map((item) => ("str" in item ? item.str : "")).join(" ");
  } finally {
    await doc.destroy();
  }
}

// A tiny valid 8x8 red PNG (kept inline so no fixtures are needed).
const RED_PNG = Uint8Array.from(
  Buffer.from(
    "iVBORw0KGgoAAAANSUhEUgAAAAgAAAAIAQMAAAD+wSzIAAAABlBMVEX/AAD///9BHTQRAAAACXBIWXMAAAsTAAALEwEAmpwYAAAAEklEQVQI12P4z8DwHwyBLBAJBAB2nQ3w0DQ9rQAAAABJRU5ErkJggg==",
    "base64",
  ),
);

test("merge combines documents in order", async () => {
  const a = await samplePdf(3, "Alpha");
  const b = await samplePdf(2, "Beta");
  const merged = await mergePdfs([
    { name: "a.pdf", bytes: a },
    { name: "b.pdf", bytes: b },
  ]);
  const info = await readInfo("merged.pdf", merged);
  assert.equal(info.pageCount, 5);
  assert.equal(info.title, "Original title");
  const first = await pageText(merged, 1);
  const fourth = await pageText(merged, 4);
  assert.match(first, /Alpha page 1/);
  assert.match(fourth, /Beta page 1/);
});

test("extract, delete and reorder pages", async () => {
  const source = await samplePdf(6, "Doc");

  const extracted = await extractPages(source, [1, 3, 5]);
  assert.equal((await readInfo("x.pdf", extracted)).pageCount, 3);
  assert.match(await pageText(extracted, 1), /Doc page 1/);
  assert.match(await pageText(extracted, 2), /Doc page 3/);
  assert.match(await pageText(extracted, 3), /Doc page 5/);

  const cleaned = await deletePages(source, [2, 4]);
  assert.equal((await readInfo("x.pdf", cleaned)).pageCount, 4);
  assert.match(await pageText(cleaned, 2), /Doc page 3/);

  const reordered = await applyPagePlan(source, [
    { source: 3, rotation: 0 },
    { source: 1, rotation: 90 },
    { source: 3, rotation: 180 },
    { source: 6, rotation: 0 },
  ]);
  const reorderedText = await pageText(reordered, 1);
  assert.match(reorderedText, /Doc page 3/);
  assert.match(await pageText(reordered, 2), /Doc page 1/);
  assert.match(await pageText(reordered, 3), /Doc page 3/);
  assert.match(await pageText(reordered, 4), /Doc page 6/);
  const parsed = await loadPdf(reordered);
  assert.equal(parsed.getPage(1).getRotation().angle, 90);
  assert.equal(parsed.getPage(2).getRotation().angle, 180);
});

test("rotate applies to the selection only", async () => {
  const source = await samplePdf(3, "Rot");
  const rotated = await rotatePages(source, [1, 3], 90);
  const doc = await loadPdf(rotated);
  assert.equal(doc.getPage(0).getRotation().angle, 90);
  assert.equal(doc.getPage(1).getRotation().angle, 0);
  assert.equal(doc.getPage(2).getRotation().angle, 90);
  const all = await loadPdf(await rotatePages(source, [], 180));
  assert.equal(all.getPage(0).getRotation().angle, 180);
  assert.equal(all.getPage(2).getRotation().angle, 180);
});

test("split modes produce the expected parts", async () => {
  const source = await samplePdf(10, "Split");

  const everyTwo = await splitPdf("doc.pdf", source, { mode: "every_n", n: 4 });
  assert.equal(everyTwo.length, 3);
  assert.equal((await readInfo("x", everyTwo[0].bytes)).pageCount, 4);
  assert.equal((await readInfo("x", everyTwo[2].bytes)).pageCount, 2);
  assert.match(everyTwo[0].fileName, /0001-0004/);

  const individual = await splitPdf("doc.pdf", source, { mode: "individual" });
  assert.equal(individual.length, 10);

  const atPages = await splitPdf("doc.pdf", source, { mode: "at_pages", pages: [4, 8] });
  assert.deepEqual(atPages.map((part) => [part.firstPage, part.lastPage]), [[1, 3], [4, 7], [8, 10]]);

  const ranges = await splitPdf("doc.pdf", source, { mode: "ranges", ranges: ["1-2", "3-10"] });
  assert.equal(ranges.length, 2);
  assert.match(await pageText(ranges[1].bytes, 1), /Split page 3/);

  assert.throws(() => planSplit({ mode: "every_n", n: 0 }, 10));
  assert.throws(() => planSplit({ mode: "ranges", ranges: ["20-30"] }, 10));
});

test("metadata can be edited and cleared", async () => {
  const source = await samplePdf(1, "Meta");
  const edited = await editMetadata(source, {
    title: "Rapor: şğüöç 2026",
    author: "Test Yazarı",
    subject: "Konu",
    keywords: "pdf, test",
    creator: "Unit test",
    producer: "OmniOffice",
  });
  const info = await readInfo("edited.pdf", edited);
  assert.equal(info.title, "Rapor: şğüöç 2026");
  assert.equal(info.author, "Test Yazarı");
  assert.match(info.keywords, /pdf/);

  const cleared = await clearMetadata(source);
  const clearedInfo = await readInfo("cleared.pdf", cleared);
  assert.equal(clearedInfo.title, "");
  assert.equal(clearedInfo.author, "");
});

test("page numbers and watermarks add real text", async () => {
  const source = await samplePdf(2, "Stamp");

  const numbered = await addPageNumbers(source, {
    position: "bottom_center",
    format: "page_n_of_total",
    startNumber: 1,
    sizePt: 12,
    color: "#333333",
    marginPt: 28,
    countFromStart: true,
    pages: [],
  });
  assert.match(await pageText(numbered, 1), /Page 1 of 2/);
  assert.match(await pageText(numbered, 2), /Page 2 of 2/);

  const fonts = {
    sans: new Uint8Array(await readFile(join(here, "..", "public", "fonts", "PT_Sans-Web-Regular.ttf"))),
  };
  const watermarked = await addTextWatermark(source, {
    text: "GİZLİ — CONFIDENTIAL",
    fontFamily: "sans",
    fontBytes: fonts.sans,
    sizePt: 40,
    color: "#9aa0a6",
    opacity: 0.3,
    rotationDeg: 45,
    position: "center",
    marginPt: 24,
    tile: false,
    pages: [1],
  });
  const firstText = await pageText(watermarked, 1);
  const secondText = await pageText(watermarked, 2);
  assert.match(firstText, /GİZLİ/);
  assert.doesNotMatch(secondText, /GİZLİ/);
});

test("images become a single PDF", async () => {
  const doc = await imagesToPdf(
    [
      { name: "a.png", bytes: RED_PNG, type: "image/png", rotation: 0 },
      { name: "b.png", bytes: RED_PNG, type: "image/png", rotation: 90 },
    ],
    {
      pageSize: "a4",
      customWidthPt: 595,
      customHeightPt: 842,
      orientation: "auto",
      fit: "fit",
      marginPt: 12,
      assumedDpi: 96,
    },
  );
  const info = await readInfo("album.pdf", info0(doc));
  assert.equal(info.pageCount, 2);
});

function info0(bytes: Uint8Array): Uint8Array {
  return bytes;
}

test("lossless compression keeps the document intact", async () => {
  const source = await samplePdf(4, "Compress");
  const compressed = await compressLossless(source);
  const info = await readInfo("compressed.pdf", compressed);
  assert.equal(info.pageCount, 4);
  assert.match(await pageText(compressed, 2), /Compress page 2/);
  assert.ok(compressed.length > 1000);
});

test("page selection parser matches the desktop rules", () => {
  assert.deepEqual(parsePageSelection("1,3,5-8,12", 20), [1, 3, 5, 6, 7, 8, 12]);
  assert.deepEqual(parsePageSelection("*", 3), [1, 2, 3]);
  assert.equal(parsePageSelection("end", 5), null);
  assert.equal(parsePageSelection("9-12", 10), null);
  assert.equal(parsePageSelection("", 10), null);
});

test("encrypted documents are reported, not mis-parsed", async () => {
  // Build a file with an /Encrypt reference in the trailer (no real crypto:
  // the loader must refuse instead of producing a broken document).
  const source = await samplePdf(1, "Enc");
  const text = new TextDecoder("latin1").decode(source);
  const patched = text.replace("trailer", "/Encrypt 1 0 R\ntrailer");
  const bytes = new TextEncoder().encode(patched);
  const info = await readInfo("enc.pdf", bytes);
  assert.equal(info.encrypted, true);
  assert.equal(info.pageCount, 0);
  await assert.rejects(() => loadPdf(bytes));
});
