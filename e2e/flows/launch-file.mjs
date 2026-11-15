/**
 * Documents handed to the app on its command line, the way Explorer does it
 * (double-click, "Open with", "Send to"): no dev launch context, only argv.
 *
 * 1. A cold start with a PDF must leave Home for the Reader.
 * 2. A second launch with an office document while the app runs must be
 *    forwarded (single instance) and open it in the office workspace.
 *
 * Engine-free: the checks read the screen and the file name, not a rendered
 * page, so the flow runs in the Linux job as well as on Windows.
 */
import { spawn } from "node:child_process";

import { executeAsync, exe, screenshot, sleep, waitForScript, withSessionRetry } from "../lib/driver.mjs";
import { prepareWorkFile } from "../lib/files.mjs";

const onScreen = (text) =>
  // Home is the screen with the tool grid; the opened document's name shows
  // in the reader / workspace once it has left Home.
  `return !document.querySelector('.tool-card') && document.body.innerText.includes(${JSON.stringify(text)})`;

// What the Rust side sees on this command line; printed so a failure on the
// Windows runner says whether the file never reached the app or the UI.
const STARTUP_FILES = `const done = arguments[arguments.length - 1];
window.__TAURI_INTERNALS__.invoke("office_startup_files")
  .then((refs) => done(JSON.stringify(refs.map((ref) => ref.name))))
  .catch((error) => done("error: " + String(error)));`;

export async function launchFileFlow() {
  const pdfPath = prepareWorkFile("sample-1.pdf");
  const docPath = prepareWorkFile("test-document.docx");
  await withSessionRetry(
    "launch-file",
    {},
    async (session) => {
      console.log(`launch-file: office_startup_files -> ${await executeAsync(session, STARTUP_FILES)}`);
      await waitForScript(session, onScreen("sample-1.pdf"), 45000, "launched PDF open in the reader");
      await screenshot(session, "launch-file-pdf");
      console.log("PASS: a PDF on the command line opens in the reader");

      // The second process finds the running instance, forwards its argv and
      // exits; the running window must open the document.
      const second = spawn(exe, [docPath], { stdio: "ignore", detached: false });
      second.on("error", () => undefined);
      await sleep(1000);
      await waitForScript(session, onScreen("test-document"), 45000, "forwarded document open in the workspace");
      await screenshot(session, "launch-file-forwarded");
      console.log("PASS: a document from a second launch opens in the running window");
    },
    [pdfPath],
  );
}
