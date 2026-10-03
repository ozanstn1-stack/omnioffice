#!/usr/bin/env node
/**
 * Desktop smoke test through tauri-driver: a real Tauri binary driven by a
 * real WebDriver (msedgedriver/WebView2 on Windows, WebKitWebDriver on Linux).
 *
 *   node e2e/smoke.mjs                          # home + settings navigation
 *   node e2e/smoke.mjs --pdf samples/sample-1.pdf   # also opens the Reader
 *
 * Environment:
 *   TAURI_DRIVER_PORT     WebDriver port (default 4444)
 *   TAURI_NATIVE_DRIVER   path to msedgedriver (Windows; must match WebView2)
 *
 * The app is launched by tauri-driver with the dev launch context
 * (PDFSAK_START_SCREEN / PDFSAK_DEV_FILES) so no file dialog is involved.
 * The PDF step needs the native engines next to the binary; CI runs the
 * engine-free smoke on Linux, the Windows run can add --pdf.
 */
import { spawn } from "node:child_process";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "..");
const artifacts = join(here, "artifacts");
const argv = process.argv.slice(2);
const valueOf = (flag) => {
  const index = argv.indexOf(flag);
  return index >= 0 ? argv[index + 1] : undefined;
};
const pdf = valueOf("--pdf");
const port = Number(process.env.TAURI_DRIVER_PORT ?? 4444);

const exeName = process.platform === "win32" ? "pdf-swiss-army-knife.exe" : "pdf-swiss-army-knife";
const exe = valueOf("--exe") ?? join(root, "target", "debug", exeName);
if (!existsSync(exe)) {
  console.error(`app binary not found: ${exe}\nrun: cargo build -p pdf-swiss-army-knife`);
  process.exit(1);
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function webdriver(path, method = "GET", body) {
  const response = await fetch(`http://127.0.0.1:${port}${path}`, {
    method,
    headers: { "content-type": "application/json" },
    body: body ? JSON.stringify(body) : undefined,
  });
  const json = await response.json().catch(() => ({}));
  if (json.value && json.value.error) throw new Error(`${json.value.error}: ${json.value.message}`);
  return json.value;
}

const execute = (sessionId, script) => webdriver(`/session/${sessionId}/execute/sync`, "POST", { script, args: [] });

async function waitForScript(sessionId, script, timeoutMs, label) {
  const started = Date.now();
  let last;
  while (Date.now() - started < timeoutMs) {
    last = await execute(sessionId, script);
    if (last) return last;
    await sleep(300);
  }
  // On failure, report what the page actually shows - a blank window and a
  // stuck app are otherwise indistinguishable from a wrong selector.
  const diagnostic = await execute(
    sessionId,
    "return JSON.stringify({ready: document.readyState, href: location.href, text: (document.body?.innerText||'').slice(0,300)})",
  ).catch(() => "<no diagnostic>");
  throw new Error(`timed out waiting for ${label} (last value: ${JSON.stringify(last)}; page: ${diagnostic})`);
}

async function waitForDriver() {
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
  throw new Error("tauri-driver did not answer /status within 30 s");
}

async function main() {
  mkdirSync(artifacts, { recursive: true });
  const env = { ...process.env };
  if (pdf) {
    env.PDFSAK_START_SCREEN = "reader";
    env.PDFSAK_DEV_FILES = resolve(pdf);
  }
  const driverArgs = [];
  if (process.env.TAURI_NATIVE_DRIVER) driverArgs.push("--native-driver", process.env.TAURI_NATIVE_DRIVER);
  const driver = spawn("tauri-driver", driverArgs, { env, stdio: ["ignore", "pipe", "pipe"] });
  let driverLog = "";
  driver.stdout.on("data", (data) => (driverLog += data));
  driver.stderr.on("data", (data) => (driverLog += data));

  let sessionId = null;
  try {
    await waitForDriver();
    const session = await webdriver("/session", "POST", {
      capabilities: { alwaysMatch: { "tauri:options": { application: exe } } },
    });
    sessionId = session.sessionId;

    if (pdf) {
      // The dev launch context routed straight to the Reader; a page raster
      // must appear once pdfium has rendered the first page.
      await waitForScript(
        sessionId,
        "return document.querySelectorAll('.reader-scroll').length",
        30000,
        "reader screen",
      );
      const images = await waitForScript(
        sessionId,
        "return document.querySelectorAll('.reader-scroll img').length",
        90000,
        "rendered reader page",
      );
      const shot = await webdriver(`/session/${sessionId}/screenshot`);
      writeFileSync(join(artifacts, "reader.png"), Buffer.from(shot, "base64"));
      console.log(`PASS: reader rendered ${images} page(s)`);
      return;
    }

    // The home screen renders the tool grid once settings and recents loaded.
    const cards = await waitForScript(
      sessionId,
      "return document.querySelectorAll('.tool-card').length",
      30000,
      "tool grid",
    );
    if (cards < 10) throw new Error(`expected the full tool grid, found ${cards} cards`);

    // Navigation: click Settings in the sidebar (or in the drawer when the
    // window is narrow) and wait for its form.
    await execute(
      sessionId,
      "const menu=document.querySelector('header button[aria-label]'); if(menu && !document.querySelector('.nav-item')) menu.click(); return true;",
    );
    await sleep(500);
    const clickResult = await execute(
      sessionId,
      "const item=[...document.querySelectorAll('.nav-item')].find((el)=>/Settings|Ayarlar/i.test(el.textContent||'')); if(item) item.click(); return JSON.stringify({clicked: Boolean(item), navItems: document.querySelectorAll('.nav-item').length});",
    );
    try {
      await waitForScript(
        sessionId,
        "return document.querySelectorAll('.input').length > 0 && /Settings|Ayarlar/i.test(document.querySelector('h1')?.textContent || '')",
        15000,
        "settings screen",
      );
    } catch (error) {
      throw new Error(`${error.message}; click: ${clickResult}`);
    }
    const shot = await webdriver(`/session/${sessionId}/screenshot`);
    writeFileSync(join(artifacts, "settings.png"), Buffer.from(shot, "base64"));
    console.log(`PASS: home grid (${cards} cards), settings navigation`);
  } finally {
    if (sessionId) await webdriver(`/session/${sessionId}`, "DELETE").catch(() => undefined);
    driver.kill();
    const tail = driverLog.trim().split("\n").slice(-4).join("\n");
    if (tail) console.log(`driver: ${tail}`);
  }
}

main().catch((error) => {
  console.error(`E2E FAILED: ${error.message}`);
  process.exit(1);
});
