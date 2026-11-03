/**
 * Calc round trip: open a real XLSX in the office editor, prove the formula
 * engine evaluated it, and prove the formula bar follows the selection.
 *
 * The cells carry `data-cell="row:col"`, so nothing here depends on a translated
 * label or on pixel coordinates. Row 1 / column G holds `=SUM(D2:D101)` in
 * `samples/test-spreadsheet.xlsx`, and D is `B*C` per row, so a non-zero total
 * means the real engine ran rather than the cached 0 stored in the file.
 */
import { clickElement, findElement, screenshot, waitForScript, withSession } from "../lib/driver.mjs";
import { elementKey, prepareWorkFile } from "../lib/files.mjs";

const cellText = (row, col) =>
  `const el=document.querySelector('[data-cell="${row}:${col}"]'); return el ? (el.textContent||'').trim() : '';`;

export async function calcFlow(session) {
  await waitForScript(session, "return document.querySelectorAll('.calc-cell').length", 45000, "calc grid");

  // Header row comes from the workbook's shared strings.
  for (const [row, col, expected] of [
    [0, 0, "Name"],
    [0, 1, "Quantity"],
    [0, 3, "Total"],
  ]) {
    const text = await waitForScript(session, cellText(row, col), 15000, `header A${row + 1}`);
    if (!text.includes(expected)) {
      throw new Error(`cell ${row}:${col} should read "${expected}", got "${text}"`);
    }
  }

  // G2 = SUM(D2:D101) over D = B*C. The workbook caches 0, so a positive total
  // is the engine's answer, not the file's.
  const total = await waitForScript(
    session,
    "const el=document.querySelector('[data-cell=\"1:6\"]'); return el ? (el.textContent||'').trim() : '';",
    20000,
    "the computed SUM",
  );
  // The engine's value arrives locale-formatted ("9,820.00" or "9820,00"), so
  // only the magnitude is asserted: the file caches 0, so any non-zero digits
  // can only come from the formula being evaluated.
  const digits = total.replace(/[^\d]/g, "");
  if (!digits || Number(digits) <= 0) {
    throw new Error(`the SUM cell should hold a positive total, got "${total}"`);
  }

  // Selecting the cell must put its formula in the formula bar. A WebDriver
  // click is used rather than a synthetic MouseEvent: the grid maps the pointer
  // to a cell from the event's coordinates, which a hand-built event lacks.
  const cell = await findElement(session, '[data-cell="1:6"]');
  await clickElement(session, elementKey(cell));
  const formula = await waitForScript(
    session,
    "const input=document.querySelector('.formula-input'); return input ? String(input.value||'') : '';",
    20000,
    "the formula bar",
  );
  if (!formula.replace(/\s/g, "").toUpperCase().startsWith("=SUM")) {
    throw new Error(`the formula bar should show the SUM formula, got "${formula}"`);
  }

  await screenshot(session, "calc");
  console.log(`PASS: calc evaluated SUM(D2:D101) = ${total} and showed "${formula}" in the formula bar`);
}

/** The launch context for this flow: the workbook arrives through the dev context. */
export async function calcScenario() {
  const book = prepareWorkFile("test-spreadsheet.xlsx");
  await withSession("calc", { PDFSAK_START_SCREEN: "office", PDFSAK_DEV_FILES: book }, calcFlow);
}
