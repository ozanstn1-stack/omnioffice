/**
 * Signature verification of an unsigned document.
 *
 * The file arrives through the dev launch context and the screen runs the
 * offline check itself (`pdf_verify_signatures`), so this flow proves the real
 * command ran and that the UI reports "nothing was signed" instead of an empty
 * panel. The verdict is read from a data attribute rather than a translated
 * sentence, so the assertion survives a language change.
 */
import { screenshot, waitForScreenReady, waitForScript, withSession } from "../lib/driver.mjs";
import { prepareWorkFile } from "../lib/files.mjs";

export async function verifyFlow(session) {
  await waitForScreenReady(session, "verify", 45000);
  const verdict = await waitForScript(
    session,
    "const el=document.querySelector('[data-testid=\"signature-verdict\"]'); return el ? (el.dataset.verdict||'')+'|'+el.innerText.replace(/\\s+/g,' ').trim() : ''",
    60000,
    "signature verdict",
  );
  const [kind, text] = verdict.split("|");
  if (kind !== "unsigned") {
    throw new Error(`expected an unsigned verdict for the sample PDF, got "${kind}"`);
  }
  if (!text) throw new Error("the verdict card rendered without text");
  // The screen must still list the file it verified.
  const listed = await waitForScript(
    session,
    "return /sample-1/.test(document.body.innerText)",
    30000,
    "the verified file in the list",
  );
  if (!listed) throw new Error("the verified file is not listed on the screen");
  await screenshot(session, "verify");
  console.log(`PASS: verify reported an unsigned document (${text.slice(0, 60)})`);
}

/** The launch context for this flow: the sample arrives through the dev context. */
export async function verifyScenario() {
  const sample = prepareWorkFile("sample-1.pdf");
  await withSession("verify", { PDFSAK_START_SCREEN: "verify", PDFSAK_DEV_FILES: sample }, verifyFlow);
}
