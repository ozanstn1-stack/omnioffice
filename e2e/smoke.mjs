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
 *   node e2e/smoke.mjs --persona accountant  # UBL invoice -> KDV summary (real backend)
 *   node e2e/smoke.mjs --persona student     # Writer spell check + suggestion
 *   node e2e/smoke.mjs --persona public      # Official letter check
 *   node e2e/smoke.mjs --persona lawyer      # KVKK redaction of synthetic identifiers
 *   node e2e/smoke.mjs --persona all         # the four personas in sequence
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
import { spawn, spawnSync } from "node:child_process";
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

const exeName = process.platform === "win32" ? "OmniOffice.exe" : "OmniOffice";
const exe = valueOf("--exe") ?? join(root, "target", "debug", exeName);
if (!existsSync(exe)) {
  console.error(`app binary not found: ${exe}\nrun: npm run tauri -- build --debug --no-bundle`);
  process.exit(1);
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// Every WebDriver call is bounded: a stalled native driver used to leave the
// CI step waiting with no output until the job-level limit (one deep-flows
// run sat for 36 minutes). Session creation launches the app on a loaded CI
// runner, so the bound is generous (and createSession retries once); a timeout
// surfaces as a normal failure with a screenshot.
const WEBDRIVER_TIMEOUT_MS = 150_000;

function webdriver(port, path, method = "GET", body) {
  return fetch(`http://127.0.0.1:${port}${path}`, {
    method,
    headers: { "content-type": "application/json" },
    body: body ? JSON.stringify(body) : undefined,
    signal: AbortSignal.timeout(WEBDRIVER_TIMEOUT_MS),
  }).then(async (response) => {
    const json = await response.json().catch(() => ({}));
    if (json.value && json.value.error) throw new Error(`${json.value.error}: ${json.value.message}`);
    return json.value;
  });
}

const execute = (session, script) =>
  webdriver(session.port, `/session/${session.id}/execute/sync`, "POST", { script, args: [] });

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
    "const root=document.documentElement.dataset; return JSON.stringify({ready: document.readyState, href: location.href, screenReady: root.screenReady||null, screenReadyDetail: root.screenReadyDetail||null, text: (document.body?.innerText||'').slice(0,300)})",
  ).catch(() => "<no diagnostic>");
  throw new Error(`timed out waiting for ${label} (last value: ${JSON.stringify(last)}; page: ${diagnostic})`);
}

/**
 * Waits until a screen reports that it holds its input and can run
 * (`<html data-screen-ready>`, see src/lib/screen-ready.ts). This replaces
 * waits on button counts, which could not tell "rendered" from "has its
 * file" (#407).
 */
async function waitForScreenReady(session, screen, timeoutMs) {
  return waitForScript(
    session,
    `return document.documentElement.dataset.screenReady === ${JSON.stringify(screen)} ? (document.documentElement.dataset.screenReadyDetail || true) : false`,
    timeoutMs,
    `${screen} ready signal (data-screen-ready)`,
  );
}

async function waitForDriver(port) {
  const started = Date.now();
  while (Date.now() - started < 60000) {
    try {
      const response = await fetch(`http://127.0.0.1:${port}/status`);
      if (response.ok) return;
    } catch {
      // driver not up yet
    }
    await sleep(250);
  }
  throw new Error(`tauri-driver did not answer /status on port ${port} within 60 s`);
}

/**
 * Creates the WebDriver session, retrying once. A loaded CI runner can take
 * longer than the call timeout to start the native driver and the app, which
 * used to fail a whole release on the third scenario.
 */
async function createSession(port) {
  let lastError;
  for (let attempt = 1; attempt <= 2; attempt += 1) {
    try {
      return await webdriver(port, "/session", "POST", {
        capabilities: { alwaysMatch: { "tauri:options": { application: exe } } },
      });
    } catch (error) {
      lastError = error;
      if (attempt < 2) {
        console.log(`[driver] session attempt ${attempt} failed (${error.message}); retrying`);
        await sleep(4000);
      }
    }
  }
  throw lastError;
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
  // A stale app from a previous scenario would win the single-instance race
  // and forward the new launch to itself; the session would then attach to the
  // old window (usually Home) and time out. Reap before launching.
  killAppInstances();
  await sleep(500);
  // A unique native port per session: killing tauri-driver does not always
  // reap the WebDriver child on Windows, and a second session's msedgedriver
  // cannot bind a port the old one still holds.
  const driverArgs = ["--port", String(port), "--native-port", String(port + 100)];
  if (process.env.TAURI_NATIVE_DRIVER) driverArgs.push("--native-driver", process.env.TAURI_NATIVE_DRIVER);
  const driver = spawn("tauri-driver", driverArgs, {
    env: { ...process.env, ...env },
    stdio: ["ignore", "pipe", "pipe"],
  });
  let driverLog = "";
  driver.stdout.on("data", (data) => (driverLog += data));
  driver.stderr.on("data", (data) => (driverLog += data));
  let session = null;
  try {
    await waitForDriver(port);
    const created = await createSession(port);
    session = { id: created.sessionId, port };
    try {
      return await steps(session);
    } catch (error) {
      // A failure screenshot is the fastest way to see what the app showed.
      await screenshot(session, `${name}-failure`).catch(() => undefined);
      const diagnostic = await execute(
        session,
        "return JSON.stringify({h1:(document.querySelector('h1')?.textContent||'').trim(), buttons:document.querySelectorAll('button').length})",
      ).catch(() => "<no diagnostic>");
      error.message = `${error.message}; screen: ${diagnostic}`;
      throw error;
    }
  } finally {
    if (session) await webdriver(session.port, `/session/${session.id}`, "DELETE").catch(() => undefined);
    driver.kill();
    // An app left with unsaved changes answers the close request with a
    // dialog and survives; the next session's launch would then forward to it
    // (single-instance) and time out. Reap the app between scenarios.
    killAppInstances();
    // Do not wait for EOF on pipes that a surviving grandchild may hold.
    driver.stdout.destroy();
    driver.stderr.destroy();
    const tail = driverLog.trim().split("\n").slice(-3).join("\n");
    if (tail) console.log(`[${name}] driver: ${tail}`);
  }
}

/**
 * Runs a scenario, and on failure relaunches it once with a fresh session.
 *
 * The dev launch context (PDFSAK_START_SCREEN / PDFSAK_DEV_FILES) is applied
 * only at launch, so a flow that missed its start screen cannot recover by
 * navigating - the sanitize fallback clears the selected file and then times
 * out (#407). A relaunch is the only recovery; one retry keeps a slow WebView2
 * start from blocking the release tag, while a real regression still fails
 * both attempts.
 */
async function withSessionRetry(name, env, steps) {
  try {
    return await withSession(name, env, steps);
  } catch (error) {
    console.warn(`RETRY: ${name} failed (${error.message}); relaunching once with a fresh session`);
    return withSession(`${name}-retry`, env, steps);
  }
}

function killAppInstances() {
  if (process.platform === "win32") {
    spawnSync("taskkill", ["/IM", exeName, "/F"], { stdio: "ignore" });
  } else {
    spawnSync("pkill", ["-f", exeName], { stdio: "ignore" });
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

/**
 * Accountant persona: a UBL-TR invoice opens through the real `open_invoice`
 * command (nothing is mocked), the row shows the parsed payable total in the
 * Turkish format, and the KDV summary modal recomputes the declaration
 * figures from the parsed lines.
 */
async function accountantPersona(session) {
  await waitForScript(
    session,
    "return /Örnek Danışmanlık/.test(document.body.innerText) && /1\\.800,00/.test(document.body.innerText)",
    60000,
    "parsed UBL invoice",
  );
  const opened = await execute(
    session,
    "const button=[...document.querySelectorAll('button')].find((el)=>(el.textContent||'').includes('KDV Beyan')); if(!button) return false; button.click(); return true;",
  );
  if (!opened) throw new Error("the KDV summary button was not found");
  const summary = await waitForScript(
    session,
    "return /KDV1 Beyannamesi/.test(document.body.innerText) ? document.body.innerText.slice(0, 6000) : ''",
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

/** Opens the office document and focuses its first paragraph. */
async function activateWriter(session) {
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
async function typeInWriter(session, text) {
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
async function openReviewRibbon(session) {
  const clicked = await execute(
    session,
    "const tab=[...document.querySelectorAll('button')].find((el)=>(el.textContent||'').trim()==='Review'); if(!tab) return false; tab.click(); return true;",
  );
  if (!clicked) throw new Error("the Review ribbon tab was not found");
}

/**
 * Student persona: type a misspelled Turkish word into the real Writer and
 * apply the checker's suggestion (the Rust `check_document_spelling` command).
 */
async function studentPersona(session) {
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
async function publicPersona(session) {
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
async function lawyerPersona(session) {
  const output = join(work, "redaction-sample_redacted.pdf");
  rmSync(output, { force: true });
  await waitForScreenReady(session, "redact", 45000);
  const size = await waitForFile(output, 120000, "redacted output");
  await screenshot(session, "persona-lawyer");
  console.log(`PASS: lawyer persona found and redacted the synthetic identifiers (${size} bytes)`);
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
  const persona = valueOf("--persona");
  if (persona) {
    const personas = {
      accountant: { screen: "invoices", file: "sample-invoice.xml", run: accountantPersona },
      student: { screen: "office", file: "test-document.docx", run: studentPersona },
      public: { screen: "office", file: "test-document.docx", run: publicPersona },
      lawyer: { screen: "redact", file: "redaction-sample.pdf", run: lawyerPersona, autoRun: true },
    };
    const names =
      persona === "all"
        ? Object.keys(personas)
        : persona
            .split(",")
            .map((name) => name.trim())
            .filter(Boolean);
    for (const name of names) {
      const spec = personas[name];
      if (!spec) throw new Error(`unknown persona: ${name} (known: ${Object.keys(personas).join(", ")})`);
      const file = prepareWorkFile(spec.file);
      await withSession(
        `persona-${name}`,
        {
          PDFSAK_START_SCREEN: spec.screen,
          PDFSAK_DEV_FILES: file,
          ...(spec.autoRun ? { PDFSAK_DEV_RUN: "1" } : {}),
        },
        spec.run,
      );
    }
    return;
  }
  if (scenarios.length === 0) {
    await withSession("smoke", {}, homeAndSettings);
    return;
  }
  for (const scenario of scenarios) {
    if (scenario === "office") {
      const doc = prepareWorkFile("test-document.docx");
      await withSessionRetry("office", { PDFSAK_START_SCREEN: "office", PDFSAK_DEV_FILES: doc }, officeRoundtrip);
    } else if (scenario === "merge") {
      const first = prepareWorkFile("sample-1.pdf");
      const second = prepareWorkFile("sample-2.pdf");
      await withSessionRetry(
        "merge",
        { PDFSAK_START_SCREEN: "merge", PDFSAK_DEV_FILES: `${first};${second}`, PDFSAK_DEV_RUN: "1" },
        mergeFlow,
      );
    } else if (scenario === "sanitize") {
      const input = prepareWorkFile("sample-1.pdf");
      await withSessionRetry("sanitize", { PDFSAK_START_SCREEN: "pdfStudio", PDFSAK_DEV_FILES: input }, sanitizeFlow);
    }
  }
}

// Exit explicitly: helper processes started under tauri-driver (e.g. WebKit's
// network process on Linux) can inherit its stdout/stderr pipes and outlive
// it, which kept Node's event loop - and the CI step - alive long after every
// scenario had passed.
main().then(
  () => process.exit(0),
  (error) => {
    console.error(`E2E FAILED: ${error.message}`);
    process.exit(1);
  },
);
