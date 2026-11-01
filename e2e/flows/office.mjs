/**
 * Writer round trip and the writing helpers the personas share.
 */
import { statSync } from "node:fs";
import { join } from "node:path";
import { clickElement, execute, findElement, screenshot, sleep, waitForScript, work } from "../lib/driver.mjs";
import { elementKey } from "../lib/files.mjs";

/**
 * Writer round trip: open a DOCX copy, type through the real editing surface,
 * save with Ctrl+S and prove the file on disk changed.
 */
export async function officeRoundtrip(session) {
  const doc = join(work, "test-document.docx");
  const modTimeBefore = statSync(doc).mtimeMs;

  await waitForScript(session, "return document.querySelectorAll('.writer-fragment').length", 30000, "writer editor");
  const fragment = await findElement(session, ".writer-fragment");
  await clickElement(session, elementKey(fragment));

  await waitForScript(
    session,
    "return document.querySelectorAll('[contenteditable=\"true\"][data-block-index]').length",
    15000,
    "active writer paragraph",
  );
  const typed = await execute(
    session,
    `const el=document.querySelector('[contenteditable="true"][data-block-index]');
     el.focus();
     const range=document.createRange();
     range.selectNodeContents(el);
     range.collapse(false);
     const sel=window.getSelection();
     sel.removeAllRanges();
     sel.addRange(range);
     document.execCommand('insertText', false, ' E2E');
     return el.textContent;`,
  );
  if (!String(typed).includes("E2E")) throw new Error(`the typed text did not reach the paragraph: ${typed}`);
  await waitForScript(
    session,
    "return [...document.querySelectorAll('[data-block-index]')].some((el)=>(el.textContent||'').includes('E2E'))",
    10000,
    "typed text in the DOM",
  );

  // Real save shortcut. The office hook listens on window in the capture phase,
  // so a dispatched event is enough; a loss-protection dialog (if the exporter
  // reports losses) is answered with Continue while we wait.
  await execute(
    session,
    "window.dispatchEvent(new KeyboardEvent('keydown',{key:'s',code:'KeyS',ctrlKey:true,bubbles:true,cancelable:true})); return true;",
  );
  const started = Date.now();
  let saved = false;
  while (Date.now() - started < 30000) {
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
  if (!saved) throw new Error("Ctrl+S did not write the document within 30 s");
  await screenshot(session, "office");
  console.log(`PASS: writer typed and saved ${doc} (${statSync(doc).size} bytes)`);
}

/** Opens the office document and focuses its first paragraph. */
export async function activateWriter(session) {
  await waitForScript(session, "return document.querySelectorAll('.writer-fragment').length", 30000, "writer editor");
  const fragment = await findElement(session, ".writer-fragment");
  await clickElement(session, elementKey(fragment));
  await waitForScript(
    session,
    "return document.querySelectorAll('[contenteditable=\"true\"][data-block-index]').length",
    15000,
    "active writer paragraph",
  );
}

/** Types text at the end of the active Writer paragraph. */
export async function typeInWriter(session, text) {
  const typed = await execute(
    session,
    `const el=document.querySelector('[contenteditable="true"][data-block-index]');
     if(!el) return '';
     el.focus();
     const range=document.createRange();
     range.selectNodeContents(el);
     range.collapse(false);
     const sel=window.getSelection();
     sel.removeAllRanges();
     sel.addRange(range);
     document.execCommand('insertText', false, ${JSON.stringify(` ${text}`)});
     return el.textContent;`,
  );
  if (!String(typed).includes(text)) throw new Error(`the typed text did not reach the paragraph: ${typed}`);
}

/** Selects the Review ribbon tab, where the proofing tools live. */
export async function openReviewRibbon(session) {
  const clicked = await execute(
    session,
    "const tab=[...document.querySelectorAll('button')].find((el)=>(el.textContent||'').trim()==='Review'); if(!tab) return false; tab.click(); return true;",
  );
  if (!clicked) throw new Error("the Review ribbon tab was not found");
}
