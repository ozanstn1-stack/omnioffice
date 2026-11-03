/**
 * The scanner round trip: two handed-over photos, the on-device corner detector,
 * a filter choice, and a real PDF written next to the first page.
 *
 * The screen is engine-free - `pdfcore`'s page detector (Canny/Hough) and the
 * lopdf writer do the work - so this flow runs in the Linux tauri-driver job
 * with the rest of the engine-free flows.
 *
 * Every assertion reads a data attribute or the file on disk, never a
 * translated sentence: a language change must not break the flow.
 */
import { join } from "node:path";
import { readFileSync, rmSync } from "node:fs";

import { execute, screenshot, waitForFile, waitForScreenReady, waitForScript, withSession, work } from "../lib/driver.mjs";
import { prepareScanPages } from "../lib/scan-fixtures.mjs";

/** The PDF path the app derives in a development auto-run (src/screens/scanner-dev.ts). */
const outputFor = (photo) => join(work, `${photo.slice(photo.lastIndexOf("/") + 1).replace(/\.[^.]+$/, "")}_scan.pdf`);

export async function scannerFlow(session) {
  const { photo } = prepareScanPages();
  const output = outputFor(photo);
  rmSync(output, { force: true });

  // The ready signal carries the page count, so "rendered" and "holds both
  // photos" cannot be confused (the placeholder trap from the AI-pack flow).
  await waitForScreenReady(session, "scanner", 90000);
  const detail = await execute(session, "return document.documentElement.dataset.screenReadyDetail || '';");
  if (String(detail) !== "2") {
    const rows = await execute(
      session,
      "return [...document.querySelectorAll('[data-testid=\"scanner-page\"]')].map((row)=>[row.dataset.auto, (row.textContent||'').slice(0,40)]);",
    );
    throw new Error(`expected both handed-over photos on the scanner, saw ${JSON.stringify(rows)} (detail ${detail})`);
  }

  // The detector found the page on the photo and nothing on the blank wall.
  const auto = await execute(
    session,
    "return [...document.querySelectorAll('[data-testid=\"scanner-page\"]')].map((row)=>row.dataset.auto);",
  );
  const found = Array.isArray(auto) ? auto.map(String) : [];
  if (found.join(",") !== "true,false") {
    throw new Error(
      `the on-device detector should find the page and not the blank wall, saw ${JSON.stringify(found)} ` +
        "(a refused path and a missed page look the same here: check frontend.log)",
    );
  }

  // A real click through the DOM, then the state the screen keeps.
  await execute(
    session,
    "const el=document.querySelector('[data-testid=\"scanner-filter-greyscale\"]'); if (!el) return false; el.click(); return true;",
  );
  await waitForScript(
    session,
    "return document.querySelector('[data-testid=\"scanner-filter-greyscale\"]')?.getAttribute('aria-pressed') === 'true'",
    10000,
    "the grey filter to stay selected",
  );

  const clicked = await execute(
    session,
    "const el=document.querySelector('[data-testid=\"scanner-build\"]'); if (!el || el.disabled) return false; el.click(); return true;",
  );
  if (clicked !== true) throw new Error("the build button was missing or disabled");

  const size = await waitForFile(output, 90000, "the scanned PDF");
  const result = await waitForScript(
    session,
    "const el=document.querySelector('[data-testid=\"scanner-result\"]'); return el ? `${el.dataset.pages}:${el.dataset.corrected}` : '';",
    30000,
    "the scanner result to reach the screen",
  );
  if (String(result) !== "2:1") throw new Error(`expected two pages and one corrected page, saw ${result}`);

  // The PDF on disk, not just the toast: a two-page document, and the OCR
  // hand-off offered only after a build.
  const bytes = readFileSync(output);
  const text = bytes.toString("latin1");
  if (!text.startsWith("%PDF-")) throw new Error("the output does not start with %PDF-");
  if (!text.includes("%%EOF")) throw new Error("the output has no %%EOF trailer");
  const pages = (text.match(/\/Type\s*\/Page\b/g) ?? []).length;
  if (pages !== 2) throw new Error(`expected 2 pages in the PDF, found ${pages} (${size} bytes)`);
  const ocrButton = await execute(session, "return Boolean(document.querySelector('[data-testid=\"scanner-send-ocr\"]'));");
  if (ocrButton !== true) throw new Error("the OCR hand-off did not appear after a build");

  await screenshot(session, "scanner");
  console.log(`PASS: the scanner wrote ${size} bytes with ${pages} pages; detector auto=${found.join(",")}`);
}

/**
 * The launch context for this flow: both photos are handed over and the
 * development auto-run supplies the save target, so no dialog is involved.
 */
export async function scannerScenario() {
  const { photo, blank } = prepareScanPages();
  await withSession(
    "scanner",
    { PDFSAK_START_SCREEN: "scanner", PDFSAK_DEV_FILES: `${photo};${blank}`, PDFSAK_DEV_RUN: "1" },
    scannerFlow,
  );
}
