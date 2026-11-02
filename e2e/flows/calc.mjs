/**
 * Calc round trip: open a real XLSX in the office editor, prove the formula
 * engine evaluated it, and prove the formula bar follows the selection.
 *
 * The cells carry `data-cell="row:col"`, so nothing here depends on a translated
 * label or on pixel coordinates. Row 1 / column G holds `=SUM(D2:D101)` in
 * `samples/test-spreadsheet.xlsx`, and D is `B*C` per row, so a non-zero total
 * means the real engine ran rather than the cached 0 stored in the file.
 */
import { screenshot, waitForScript } from "../lib/driver.mjs";

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
  const value = Number(total.replace(/[^\d.,-]/g, "").replace(",", "."));
  if (!Number.isFinite(value) || value <= 0) {
    throw new Error(`the SUM cell should hold a positive total, got "${total}"`);
  }

  // Selecting the cell must put its formula in the formula bar.
  const formula = await waitForScript(
    session,
    `const cell=document.querySelector('[data-cell="1:6"]');
     if (!cell) return '';
     cell.dispatchEvent(new MouseEvent('mousedown',{bubbles:true,cancelable:true,view:window,button:0}));
     cell.dispatchEvent(new MouseEvent('mouseup',{bubbles:true,cancelable:true,view:window,button:0}));
     const input=document.querySelector('.formula-input');
     return input ? String(input.value||'') : '';`,
    20000,
    "the formula bar",
  );
  if (!formula.replace(/\s/g, "").toUpperCase().startsWith("=SUM")) {
    throw new Error(`the formula bar should show the SUM formula, got "${formula}"`);
  }

  await screenshot(session, "calc");
  console.log(`PASS: calc evaluated SUM(D2:D101) = ${total} and showed "${formula}" in the formula bar`);
}
