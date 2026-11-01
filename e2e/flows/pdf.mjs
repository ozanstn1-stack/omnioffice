/**
 * PDF Studio flows: merging two documents and running the sanitizer through the UI.
 */
import { rmSync } from "node:fs";
import { join } from "node:path";
import { execute, screenshot, sleep, waitForFile, waitForScreenReady, waitForScript, work } from "../lib/driver.mjs";

/** Merges two PDFs through the Merge screen's auto-run hook. */
export async function mergeFlow(session) {
  const output = join(work, "sample-1_merged.pdf");
  rmSync(output, { force: true });

  await waitForScript(session, "return document.querySelectorAll('.tool-card, button').length", 20000, "merge screen");
  const size = await waitForFile(output, 60000, "merged output");
  await screenshot(session, "merge");
  console.log(`PASS: merge wrote ${output} (${size} bytes)`);
}

/** Runs the PDF Studio sanitizer through its real card button. */
export async function sanitizeFlow(session) {
  const output = join(work, "sample-1-clean.pdf");
  rmSync(output, { force: true });

  // The dev launch screen opens the Studio with the file already selected;
  // clicking a navigation item would clear that selection, so the route is
  // preferred. A slow WebView2 start gets a generous deadline; if it still
  // misses, the sidebar fallback can only prove the screen renders (#407).
  let onStudio = false;
  try {
    await waitForScript(
      session,
      "return /PDF St/i.test(document.querySelector('h1')?.textContent || '')",
      45000,
      "PDF Studio screen from the dev launch context",
    );
    onStudio = true;
  } catch {
    onStudio = false;
  }
  if (!onStudio) {
    await execute(
      session,
      "const menu=document.querySelector('header button[aria-label]'); if(menu && !document.querySelector('.nav-item')) menu.click(); return true;",
    );
    await sleep(500);
    const navigated = await execute(
      session,
      "const item=[...document.querySelectorAll('.nav-item')].find((el)=>/PDF St/i.test(el.textContent||'')); if(item){item.click(); return true;} return false;",
    );
    if (!navigated) throw new Error("the dev launch screen missed PDF Studio and the navigation item was not found");
    await waitForScript(
      session,
      "return /PDF St/i.test(document.querySelector('h1')?.textContent || '')",
      20000,
      "PDF Studio screen after navigation",
    );
  }

  // The Studio says when it holds the dev-selected file; only then is the
  // run button the sanitizer's. After a navigation fallback the file is gone
  // and the signal never comes, so fail fast and let the caller relaunch.
  // The run button is the last enabled primary button on the screen (the file
  // picker is the first); requiring two keeps a native dialog from opening.
  let clicked;
  try {
    await waitForScreenReady(session, "pdfStudio", onStudio ? 30000 : 10000);
    clicked = await waitForScript(
      session,
      "const buttons=[...document.querySelectorAll('button.btn.btn-primary')].filter((button)=>!button.disabled); if(buttons.length>=2){buttons[buttons.length-1].click(); return true;} return false;",
      onStudio ? 30000 : 10000,
      `sanitize run button (${onStudio ? "dev route" : "nav fallback, the dev-selected file was cleared"})`,
    );
  } catch (error) {
    if (!onStudio) {
      throw new Error(
        `the dev launch screen missed PDF Studio and navigating clears the selected file, so the sanitizer cannot run (relaunch required): ${error.message}`,
      );
    }
    throw error;
  }
  if (!clicked) throw new Error("the sanitize run button could not be clicked");
  const size = await waitForFile(output, 60000, "sanitized output");
  await screenshot(session, "sanitize");
  console.log(`PASS: sanitizer wrote ${output} (${size} bytes${onStudio ? "" : ", nav fallback"})`);
}
