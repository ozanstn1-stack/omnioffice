// Unit tests for the "open selected text" context-menu hand-off: the fragment
// written by background.js must turn into a readable PDF, Turkish letters
// included, and a hostile or empty fragment must produce nothing.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { PDFDocument } from "pdf-lib";

import {
  MAX_SELECTION_CHARS,
  fileNameForSelection,
  parseSelectedText,
  selectionToPdf,
  wrapText,
} from "../src/lib/selection";

const fragment = (text: string, src?: string) => {
  const params = new URLSearchParams({ text });
  if (src) params.set("src", src);
  params.set("n", "1");
  return `#${params.toString()}`;
};

test("selected text fragments are parsed", () => {
  assert.deepEqual(parseSelectedText(fragment("Merhaba dünya", "https://example.com/a?b=1")), {
    text: "Merhaba dünya",
    source: "example.com",
  });
  // Only an http(s) page is shown as the source.
  assert.equal(parseSelectedText(fragment("x", "javascript:alert(1)"))?.source, null);
  assert.equal(parseSelectedText(fragment("x", "not a url"))?.source, null);
  assert.equal(parseSelectedText(fragment("   ")), null);
  assert.equal(parseSelectedText(`#url=${encodeURIComponent("https://example.com/a.pdf")}`), null);
  assert.equal(parseSelectedText(""), null);
  assert.equal(parseSelectedText(fragment("a".repeat(MAX_SELECTION_CHARS + 10)))?.text.length, MAX_SELECTION_CHARS);
});

test("selected text file names are safe", () => {
  assert.equal(fileNameForSelection("example.com"), "example.com - selected text.pdf");
  assert.equal(fileNameForSelection("localhost:8080"), "localhost_8080 - selected text.pdf");
  assert.equal(fileNameForSelection(null), "selected text.pdf");
});

test("text wraps to the line width and breaks long words", () => {
  const measure = (value: string) => value.length;
  assert.deepEqual(wrapText("aa bb cc", 5, measure), ["aa bb", "cc"]);
  assert.deepEqual(wrapText("abcdefghij", 4, measure), ["abcd", "efgh", "ij"]);
  assert.deepEqual(wrapText("one\n\ntwo", 10, measure), ["one", "", "two"]);
});

test("selected text becomes a PDF with the Unicode font", async () => {
  const fontPath = fileURLToPath(new URL("../../crates/pdfcore/assets/fonts/PT_Sans-Web-Regular.ttf", import.meta.url));
  const font = new Uint8Array(readFileSync(fontPath));
  const text = Array.from({ length: 120 }, (_, i) => `Satır ${i}: Çağrı, şöyle ığdır ÖÜ.`).join("\n");
  const file = await selectionToPdf({ text, source: "example.com" }, font);
  assert.equal(file.name, "example.com - selected text.pdf");
  const doc = await PDFDocument.load(new Uint8Array(await file.arrayBuffer()));
  assert.ok(doc.getPageCount() >= 3, `expected several pages, got ${doc.getPageCount()}`);
  assert.equal(doc.getTitle(), "Selected text from example.com");
});

test("without the font, letters Helvetica cannot encode do not fail the PDF", async () => {
  const file = await selectionToPdf({ text: "Çağrı şöyle ığdır 🙂", source: null });
  const doc = await PDFDocument.load(new Uint8Array(await file.arrayBuffer()));
  assert.equal(doc.getPageCount(), 1);
});
