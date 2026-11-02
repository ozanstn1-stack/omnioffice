/**
 * Impress round trip: open a real PPTX in the office editor, move between
 * slides, add one, save with Ctrl+S and prove the file on disk changed.
 *
 * The slide rail carries `.slide-thumb` / `.is-active` and the add button keeps
 * the `.slide-add` class, so the flow counts and navigates without matching a
 * translated label. The save goes through the shared office shortcut handler
 * (`useEditorShortcuts`, a window capture listener), which is exactly the path
 * the user's own Ctrl+S takes.
 */
import { statSync } from "node:fs";
import { join } from "node:path";
import { clickElement, execute, findElement, screenshot, sleep, waitForScript, work } from "../lib/driver.mjs";
import { elementKey } from "../lib/files.mjs";

const RAIL = `const thumbs=[...document.querySelectorAll('.slide-thumb')];
  const active=thumbs.findIndex((el)=>el.classList.contains('is-active'));
  return thumbs.length ? thumbs.length + '|' + active : '';`;

export async function impressFlow(session) {
  const deck = join(work, "test-presentation.pptx");
  const modTimeBefore = statSync(deck).mtimeMs;

  const first = await waitForScript(session, RAIL, 45000, "the slide rail");
  const [initialCount, initialActive] = first.split("|").map(Number);
  if (initialCount < 2) throw new Error(`the sample deck should have several slides, saw ${initialCount}`);
  if (initialActive !== 0) throw new Error(`the first slide should be active, saw index ${initialActive}`);

  // Navigation: a real pointer click on the second slide.
  const second = await findElement(session, ".slide-thumb:nth-of-type(2)");
  await clickElement(session, elementKey(second));
  await waitForScript(
    session,
    `const thumbs=[...document.querySelectorAll('.slide-thumb')];
     return thumbs[1] && thumbs[1].classList.contains('is-active') ? 1 : 0;`,
    15000,
    "the second slide to become active",
  );

  // A change to save: the add button appends a slide and selects it.
  const add = await findElement(session, ".slide-add");
  await clickElement(session, elementKey(add));
  await waitForScript(
    session,
    `const thumbs=[...document.querySelectorAll('.slide-thumb')];
     return thumbs.length === ${initialCount + 1} ? 1 : 0;`,
    15000,
    "the new slide",
  );

  // Real save shortcut; the shared handler listens on window in the capture phase.
  await execute(
    session,
    "window.dispatchEvent(new KeyboardEvent('keydown',{key:'s',code:'KeyS',ctrlKey:true,bubbles:true,cancelable:true})); return true;",
  );
  const started = Date.now();
  let saved = false;
  while (Date.now() - started < 45000) {
    if (statSync(deck).mtimeMs > modTimeBefore) {
      saved = true;
      break;
    }
    // A loss-protection dialog, if the exporter reports losses, asks to continue.
    await execute(
      session,
      "const button=[...document.querySelectorAll('button')].find((el)=>/^(Continue|Devam)/i.test((el.textContent||'').trim())); if(button){button.click(); return true;} return false;",
    ).catch(() => undefined);
    await sleep(400);
  }
  if (!saved) throw new Error("Ctrl+S did not write the presentation within 45 s");

  const after = await waitForScript(session, RAIL, 10000, "the slide rail after saving");
  await screenshot(session, "impress");
  console.log(
    `PASS: impress navigated to slide 2, added a slide (${initialCount} -> ${after.split("|")[0]}) and saved ${deck}`,
  );
}
