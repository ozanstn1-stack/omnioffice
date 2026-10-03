#!/usr/bin/env node
/**
 * Desktop E2E through tauri-driver: a real Tauri binary driven by a real
 * WebDriver (msedgedriver/WebView2 on Windows, WebKitWebDriver on Linux).
 *
 *   node e2e/smoke.mjs                    # home grid + settings navigation
 *   node e2e/smoke.mjs --pdf <file>       # opens <file> in the Reader
 *   node e2e/smoke.mjs --office           # Writer type -> Ctrl+S -> file written
 *   node e2e/smoke.mjs --merge            # two PDFs merged through the real UI
 *   node e2e/smoke.mjs --sanitize         # PDF Studio sanitizer run through the UI
 *   node e2e/smoke.mjs --all              # office + merge + sanitize
 *
 * Environment:
 *   TAURI_DRIVER_PORT     first WebDriver port (default 4444; scenarios take the next port)
 *   TAURI_NATIVE_DRIVER   path to msedgedriver (Windows; must match WebView2)
 *
 * The app is launched by tauri-driver with the dev launch context
 * (PDFSAK_START_SCREEN / PDFSAK_DEV_FILES / PDFSAK_DEV_RUN) so no file dialog
 * is involved. The Reader step needs the native engines next to the binary;
 * the office/merge/sanitize flows are engine-free (officecore + lopdf) and run
 * in the Linux CI job. Working copies live under e2e/artifacts/work/ so the
 * checked-in samples are never modified.
 */
import { spawn } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "..");
const artifacts = join(here, "artifacts");
const work = join(artifacts, "work");
const argv = process.argv.slice(2);
const valueOf = (flag) => {
  const index = argv.indexOf(flag);
  return index >= 0 ? argv[index + 1] : undefined;
};
const pdf = valueOf("--pdf");
const basePort = Number(process.env.TAURI_DRIVER_PORT ?? 4444);

const exeName = process.platform === "win32" ? "pdf-swiss-army-knife.exe" : "pdf-swiss-army-knife";
const exe = valueOf("--exe") ?? join(root, "target", "debug", exeName);
if (!existsSync(exe)) {
  console.error(`app binary not found: ${exe}\nrun: npm run tauri -- build --debug --no-bundle`);
  process.exit(1);
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

function webdriver(port, path, method = "GET", body) {
  return fetch(`http://127.0.0.1:${port}${path}`, {
    method,
    headers: { "content-type": "application/json" },
    body: body ? JSON.stringify(body) : undefined,
  }).then(async (response) => {
    const json = await response.json().catch(() => ({}));
    if (json.value && json.value.error) throw new Error(`${json.value.error}: ${json.value.message}`);
    return json.value;
  });
}

const execute = (session, script) => webdriver(session.port, `/session/${session.id}/execute/sync`, "POST", { script, args: [] });

function findElement(session, selector) {
  return webdriver(session.port, `/session/${session.id}/element`, "POST", { using: "css selector", value: selector });
}

function clickElement(session, elementId) {
  return webdriver(session.port, `/session/${session.id}/element/${elementId}/click`, "POST", {});
}

async function waitForScript(session, script, timeoutMs, label) {
  const started = Date.now();
  let last;
  while (Date.now() - started < timeoutMs) {
    last = await execute(session, script);
    if (last) return last;
    await sleep(300);
  }
  // On failure, report what the page actually shows - a blank window and a
  // stuck app are otherwise indistinguishable from a wrong selector.
  const diagnostic = await execute(
    session,
    "return JSON.stringify({ready: document.readyState, href: location.href, text: (document.body?.innerText||'').slice(0,300)})",
  ).catch(() => "<no diagnostic>");
  throw new Error(`timed out waiting for ${label} (last value: ${JSON.stringify(last)}; page: ${diagnostic})`);
}

async function waitForDriver(port) {
  const started = Date.now();
  while (Date.now() - started < 30000) {
    try {
      const response = await fetch(`http://127.0.0.1:${port}/status`);
      if (response.ok) return;
    } catch {
      // driver not up yet
    }
    await sleep(250);
  }
  throw new Error(`tauri-driver did not answer /status on port ${port} within 30 s`);
}

async function waitForFile(path, timeoutMs, label) {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    if (existsSync(path)) {
      const size = statSync(path).size;
      if (size > 100) return size;
    }
    await sleep(400);
  }
  throw new Error(`timed out waiting for ${label}: ${path}`);
}

function screenshot(session, name) {
  return webdriver(session.port, `/session/${session.id}/screenshot`).then((shot) =>
    writeFileSync(join(artifacts, `${name}.png`), Buffer.from(shot, "base64")),
  );
}

/** Launches tauri-driver + one session, runs `steps`, and always tears down. */
async function withSession(name, env, steps) {
  const port = basePort + withSession.index++;
  // A unique native port per session: killing tauri-driver does not always
  // reap the WebDriver child on Windows, and a second session's msedgedriver
  // cannot bind a port the old one still holds.
  const driverArgs = ["--port", String(port), "--native-port", String(port + 100)];
  if (process.env.TAURI_NATIVE_DRIVER) driverArgs.push("--native-driver", process.env.TAURI_NATIVE_DRIVER);
  const driver = spawn("tauri-driver", driverArgs, { env: { ...process.env, ...env }, stdio: ["ignore", "pipe", "pipe"] });
  let driverLog = "";
  driver.stdout.on("data", (data) => (driverLog += data));
  driver.stderr.on("data", (data) => (driverLog += data));
  let session = null;
  try {
    await waitForDriver(port);
    const created = await webdriver(port, "/session", "POST", {
      capabilities: { alwaysMatch: { "tauri:options": { application: exe } } },
    });
    session = { id: created.sessionId, port };
    try {
      return await steps(session);
    } catch (error) {
      // A failure screenshot is the fastest way to see what the app showed.
      await screenshot(session, `${name}-failure`).catch(() => undefined);
      throw error;
    }
  } finally {
    if (session) await webdriver(session.port, `/session/${session.id}`, "DELETE").catch(() => undefined);
    driver.kill();
    const tail = driverLog.trim().split("\n").slice(-3).join("\n");
    if (tail) console.log(`[${name}] driver: ${tail}`);
  }
}
withSession.index = 0;

async function homeAndSettings(session) {
  // The home screen renders the tool grid once settings and recents loaded.
  const cards = await waitForScript(
    session,
    "return document.querySelectorAll('.tool-card').length",
    30000,
    "tool grid",
  );
  if (cards < 10) throw new Error(`expected the full tool grid, found ${cards} cards`);

  // Navigation: click Settings in the sidebar (or in the drawer when the
  // window is narrow) and wait for its form.
  await execute(
    session,
    "const menu=document.querySelector('header button[aria-label]'); if(menu && !document.querySelector('.nav-item')) menu.click(); return true;",
  );
  await sleep(500);
  const clickResult = await execute(
    session,
    "const item=[...document.querySelectorAll('.nav-item')].find((el)=>/Settings|Ayarlar/i.test(el.textContent||'')); if(item) item.click(); return JSON.stringify({clicked: Boolean(item), navItems: document.querySelectorAll('.nav-item').length});",
  );
  try {
    await waitForScript(
      session,
      "return document.querySelectorAll('.input').length > 0 && /Settings|Ayarlar/i.test(document.querySelector('h1')?.textContent || '')",
      15000,
      "settings screen",
    );
  } catch (error) {
    throw new Error(`${error.message}; click: ${clickResult}`);
  }
  await screenshot(session, "settings");
  console.log(`PASS: home grid (${cards} cards), settings navigation`);
}

async function reader(session) {
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

function prepareWorkFile(source) {
  mkdirSync(work, { recursive: true });
  const target = join(work, source);
  copyFileSync(join(root, "samples", source), target);
  return target;
}

const elementKey = (element) => element["element-6066-11e4-a52e-4f735466cecf"] ?? element.ELEMENT;

/**
 * Writer round trip: open a DOCX copy, type through the real editing surface,
 * save with Ctrl+S and prove the file on disk changed.
 */
async function officeRoundtrip(session) {
  const doc = join(work, "test-document.docx");
  const modTimeBefore = statSync(doc).mtimeMs;

  await waitForScript(
    session,
    "return document.querySelectorAll('.writer-fragment').length",
    30000,
    "writer editor",
  );
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

/** Merges two PDFs through the Merge screen's auto-run hook. */
async function mergeFlow(session) {
  const output = join(work, "sample-1_merged.pdf");
  rmSync(output, { force: true });

  await waitForScript(session, "return document.querySelectorAll('.tool-card, button').length", 20000, "merge screen");
  const size = await waitForFile(output, 60000, "merged output");
  await screenshot(session, "merge");
  console.log(`PASS: merge wrote ${output} (${size} bytes)`);
}

/** Runs the PDF Studio sanitizer through its real card button. */
async function sanitizeFlow(session) {
  const output = join(work, "sample-1-clean.pdf");
  rmSync(output, { force: true });

  await waitForScript(
    session,
    "const buttons=[...document.querySelectorAll('button.btn.btn-primary')]; return buttons.some((button)=>!button.disabled);",
    30000,
    "sanitize run button",
  );
  // The screen renders its file-picker ("Dosya seç"/"Choose file") as the
  // first primary button; the run button is the last one because the tool card
  // comes after the file card. A script click avoids msedgedriver's
  // "not interactable" behind the studio's scroll container, and clicking the
  // picker would open a native dialog the driver cannot dismiss.
  const clicked = await execute(
    session,
    "const buttons=[...document.querySelectorAll('button.btn.btn-primary')].filter((button)=>!button.disabled); const button=buttons[buttons.length-1]; if(button){button.click(); return true;} return false;",
  );
  if (!clicked) throw new Error("the sanitize run button could not be clicked");
  const size = await waitForFile(output, 60000, "sanitized output");
  await screenshot(session, "sanitize");
  console.log(`PASS: sanitizer wrote ${output} (${size} bytes)`);
}

async function main() {
  mkdirSync(artifacts, { recursive: true });
  const scenarios = [];
  if (argv.includes("--office") || argv.includes("--all")) scenarios.push("office");
  if (argv.includes("--merge") || argv.includes("--all")) scenarios.push("merge");
  if (argv.includes("--sanitize") || argv.includes("--all")) scenarios.push("sanitize");

  if (pdf) {
    await withSession("reader", { PDFSAK_START_SCREEN: "reader", PDFSAK_DEV_FILES: resolve(pdf) }, reader);
    return;
  }
  if (scenarios.length === 0) {
    await withSession("smoke", {}, homeAndSettings);
    return;
  }
  for (const scenario of scenarios) {
    if (scenario === "office") {
      const doc = prepareWorkFile("test-document.docx");
      await withSession("office", { PDFSAK_START_SCREEN: "office", PDFSAK_DEV_FILES: doc }, officeRoundtrip);
    } else if (scenario === "merge") {
      const first = prepareWorkFile("sample-1.pdf");
      const second = prepareWorkFile("sample-2.pdf");
      await withSession(
        "merge",
        { PDFSAK_START_SCREEN: "merge", PDFSAK_DEV_FILES: `${first};${second}`, PDFSAK_DEV_RUN: "1" },
        mergeFlow,
      );
    } else if (scenario === "sanitize") {
      const input = prepareWorkFile("sample-1.pdf");
      await withSession(
        "sanitize",
        { PDFSAK_START_SCREEN: "pdfStudio", PDFSAK_DEV_FILES: input },
        sanitizeFlow,
      );
    }
  }
}

main().catch((error) => {
  console.error(`E2E FAILED: ${error.message}`);
  process.exit(1);
});
