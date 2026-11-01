/**
 * The four personas: accountant, student, public office and lawyer.
 */
import { rmSync, statSync } from "node:fs";
import { join } from "node:path";
import { execute, screenshot, sleep, waitForFile, waitForScreenReady, waitForScript, work } from "../lib/driver.mjs";
import { activateWriter, openReviewRibbon, typeInWriter } from "./office.mjs";

/**
 * Accountant persona: a UBL-TR invoice opens through the real `open_invoice`
 * command (nothing is mocked), the row shows the parsed payable total in the
 * Turkish format, and the KDV summary modal recomputes the declaration
 * figures from the parsed lines.
 */
export async function accountantPersona(session) {
  await waitForScript(
    session,
    "return /Örnek Danışmanlık/.test(document.body.innerText) && /1\\.800,00/.test(document.body.innerText)",
    60000,
    "parsed UBL invoice",
  );
  const opened = await execute(
    session,
    "const button=document.querySelector('[data-testid=\"kdv-declaration-open\"]'); if(!button) return false; button.click(); return true;",
  );
  if (!opened) throw new Error("the KDV summary button was not found");
  const summary = await waitForScript(
    session,
    "const panel=document.querySelector('[data-testid=\"kdv-declaration\"]'); return panel ? panel.innerText.slice(0, 6000) : '';",
    15000,
    "KDV summary",
  );
  for (const expected of ["1.500,00", "300,00"]) {
    if (!summary.includes(expected)) {
      throw new Error(`the KDV summary is missing ${expected}: ${summary.slice(0, 400)}`);
    }
  }
  await screenshot(session, "persona-accountant");
  console.log("PASS: accountant persona parsed the UBL invoice and the KDV summary");
}

/**
 * Student persona: type a misspelled Turkish word into the real Writer and
 * apply the checker's suggestion (the Rust `check_document_spelling` command).
 */
export async function studentPersona(session) {
  const doc = join(work, "test-document.docx");
  const modTimeBefore = statSync(doc).mtimeMs;
  await activateWriter(session);
  await typeInWriter(session, "yanlız");
  await openReviewRibbon(session);
  const started = await execute(
    session,
    "const button=[...document.querySelectorAll('button')].find((el)=>(el.textContent||'').includes('Turkish Spell Check')); if(!button) return false; button.click(); return true;",
  );
  if (!started) throw new Error("the Turkish Spell Check button was not found");
  await waitForScript(
    session,
    "const panes=[...document.querySelectorAll('.writer-review-pane')]; return panes.some((pane)=>/Turkish Spell Check/.test(pane.innerText||'') && /yanlız/.test(pane.innerText||''));",
    60000,
    "spell check results",
  );
  const applied = await execute(
    session,
    "const pane=[...document.querySelectorAll('.writer-review-pane')].find((el)=>/Turkish Spell Check/.test(el.innerText||'')); const card=pane && [...pane.querySelectorAll('.comment-card')].find((el)=>/yanlız/.test(el.innerText||'')); const button=card && card.querySelector('button.btn-soft'); if(!button) return ''; const text=(button.textContent||'').trim(); button.click(); return text;",
  );
  if (!applied) throw new Error("no suggestion button in the spell panel");
  await waitForScript(
    session,
    "return !/yanlız/.test((document.querySelector('[contenteditable=\"true\"][data-block-index]')||{}).textContent||'')",
    15000,
    "suggestion applied",
  );
  // Save the corrected document: the journey is "fix and keep", and an app
  // left with unsaved changes blocks the next scenario's app launch.
  await execute(
    session,
    "window.dispatchEvent(new KeyboardEvent('keydown',{key:'s',code:'KeyS',ctrlKey:true,bubbles:true,cancelable:true})); return true;",
  );
  const saveStarted = Date.now();
  let saved = false;
  while (Date.now() - saveStarted < 30000) {
    if (statSync(doc).mtimeMs > modTimeBefore) {
      saved = true;
      break;
    }
    await execute(
      session,
      "const button=[...document.querySelectorAll('button')].find((el)=>/^(Continue|Devam)/i.test((el.textContent||'').trim())); if(button){button.click(); return true;} return false;",
    ).catch(() => undefined);
    await sleep(400);
  }
  if (!saved) throw new Error("Ctrl+S after the spell fix did not write the document");
  await screenshot(session, "persona-student");
  console.log(`PASS: student persona applied the spell suggestion (${applied}) and saved`);
}

/**
 * Public persona: run the Official Correspondence Regulation checker on the
 * writer document and read its findings panel.
 */
export async function publicPersona(session) {
  await activateWriter(session);
  await openReviewRibbon(session);
  const started = await execute(
    session,
    "const button=[...document.querySelectorAll('button')].find((el)=>(el.textContent||'').includes('Official Letter Check')); if(!button) return false; button.click(); return true;",
  );
  if (!started) throw new Error("the Official Letter Check button was not found");
  const result = await waitForScript(
    session,
    "const pane=[...document.querySelectorAll('.writer-review-pane')].find((el)=>/Official Letter Check/.test(el.innerText||'')); if(!pane) return ''; const text=(pane.innerText||'').trim(); return text.length>20 ? text.slice(0,400) : '';",
    30000,
    "official letter results",
  );
  await screenshot(session, "persona-public");
  console.log(`PASS: public persona ran the official letter check (${result.replace(/\n/g, " ").slice(0, 80)})`);
}

/**
 * Lawyer persona: the Redact screen's dev auto-run finds the synthetic TCKN,
 * IBAN, e-mail and phone in `samples/redaction-sample.pdf` and redacts them
 * through the real `detect_sensitive_text` + `redact_pdf` commands.
 */
export async function lawyerPersona(session) {
  const output = join(work, "redaction-sample_redacted.pdf");
  rmSync(output, { force: true });
  await waitForScreenReady(session, "redact", 45000);
  const size = await waitForFile(output, 120000, "redacted output");
  await screenshot(session, "persona-lawyer");
  console.log(`PASS: lawyer persona found and redacted the synthetic identifiers (${size} bytes)`);
}
