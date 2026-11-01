/**
 * Opens a PDF in the Reader and waits for a rendered page (needs the native engines).
 */
import { screenshot, waitForScript } from "../lib/driver.mjs";

export async function reader(session) {
  // The dev launch context routed straight to the Reader; a page raster must
  // appear once pdfium has rendered the first page.
  await waitForScript(session, "return document.querySelectorAll('.reader-scroll').length", 30000, "reader screen");
  const images = await waitForScript(
    session,
    "return document.querySelectorAll('.reader-scroll img').length",
    90000,
    "rendered reader page",
  );
  await screenshot(session, "reader");
  console.log(`PASS: reader rendered ${images} page(s)`);
}
