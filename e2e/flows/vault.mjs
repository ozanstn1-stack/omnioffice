/**
 * The vault round trip: a handed-over file seeds a folder, the real backend
 * indexes it, a keyword search finds it, and the preview shows the text.
 *
 * No engine and no AI: `vault_search_hybrid` falls back to keyword search when
 * no embedding client is configured, so this runs in the Linux tauri-driver job
 * with the other engine-free flows.
 *
 * The folder is a scratch directory under e2e/artifacts/work so the checked-in
 * samples never end up in an index.
 */
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { execute, screenshot, waitForScreenReady, waitForScript, withSession, work } from "../lib/driver.mjs";

const NOTES = "The quarterly kestrelwind audit closed on Tuesday.";
const OTHER = "Nothing to see here.";

/** A fresh folder with two text files; the notes one is the file we hand over. */
function prepareVaultFolder() {
  const folder = join(work, "vault-e2e");
  rmSync(folder, { recursive: true, force: true });
  mkdirSync(folder, { recursive: true });
  const notes = join(folder, "field-notes.md");
  writeFileSync(notes, NOTES, "utf8");
  writeFileSync(join(folder, "other.txt"), OTHER, "utf8");
  return { folder, notes };
}

export async function vaultFlow(session) {
  const { folder } = prepareVaultFolder();

  // The ready signal carries the indexed count, so "rendered" cannot be
  // mistaken for "the seeded scan finished".
  await waitForScreenReady(session, "vault", 120000);
  const detail = await execute(session, "return document.documentElement.dataset.screenReadyDetail || '';");
  if (String(detail) !== "2") {
    const folders = await execute(
      session,
      "return [...document.querySelectorAll('[data-testid=\"vault-folder\"]')].map((row)=>row.dataset.path);",
    );
    throw new Error(`expected two indexed documents, saw ${detail} (folders: ${JSON.stringify(folders)})`);
  }

  const seeded = await execute(
    session,
    "return [...document.querySelectorAll('[data-testid=\"vault-folder\"]')].map((row)=>row.dataset.path);",
  );
  if (!Array.isArray(seeded) || !seeded.some((path) => String(path).endsWith("vault-e2e"))) {
    throw new Error(`the seeded folder is not on screen: ${JSON.stringify(seeded)}`);
  }

  // A controlled React input only updates through the native setter.
  await execute(
    session,
    `const input = document.querySelector('[data-testid="vault-search"] input');
     if (!input) return false;
     const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
     setter.call(input, "kestrelwind");
     input.dispatchEvent(new Event("input", { bubbles: true }));
     return true;`,
  );
  await execute(session, "document.querySelector('[data-testid=\"vault-search\"]').requestSubmit(); return true;");

  const found = await waitForScript(
    session,
    `const box = document.querySelector('[data-testid="vault-hits"]');
     return box ? \`\${box.dataset.total}:\${box.dataset.mode}\` : '';`,
    60000,
    "the vault search to return a hit",
  );
  if (String(found) !== "1:keyword") {
    throw new Error(`expected one keyword hit, saw ${found} (no embedding client is configured, so keyword is the mode)`);
  }

  const row = await execute(
    session,
    `const hit = document.querySelector('[data-testid="vault-hit"]');
     if (!hit) return null;
     const mark = hit.querySelector("mark");
     return JSON.stringify({ name: hit.dataset.fileName, mark: (mark?.textContent || "").toLowerCase() });`,
  );
  const parsed = JSON.parse(String(row ?? "null"));
  if (parsed?.name !== "field-notes.md") throw new Error(`wrong document in the hit: ${row}`);
  if (parsed?.mark !== "kestrelwind") throw new Error(`the match is not highlighted: ${row}`);

  await execute(session, "document.querySelector('[data-testid=\"vault-hit\"]').click(); return true;");
  const preview = await waitForScript(
    session,
    `const pre = document.querySelector('[data-testid="vault-preview"]');
     return pre && pre.textContent.includes("kestrelwind") ? pre.textContent.length : 0;`,
    30000,
    "the preview to show the document text",
  );
  if (!preview) throw new Error("the preview did not show the document text");

  await screenshot(session, "vault");
  rmSync(folder, { recursive: true, force: true });
  console.log(`PASS: the vault indexed ${detail} documents and found the seeded one by keyword (${preview} chars previewed)`);
}

/** Hands the notes file over and lets the development auto-run seed the folder. */
export async function vaultScenario() {
  const { notes } = prepareVaultFolder();
  await withSession(
    "vault",
    { PDFSAK_START_SCREEN: "vault", PDFSAK_DEV_FILES: notes, PDFSAK_DEV_RUN: "1" },
    vaultFlow,
  );
}
