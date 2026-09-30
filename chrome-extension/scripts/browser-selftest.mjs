// Automated browser validation for the extension build.
//
//   node scripts/browser-selftest.mjs [baseUrl]
//
// Launches headless Chrome against the built app, waits for the in-page self
// test to finish (`?selftest=1`), prints the report and saves screenshots for
// the README. Requires `npm run build` and a running preview server
// (`npm run preview`).

import { spawn } from "node:child_process";
import { mkdir, writeFile, rm } from "node:fs/promises";
import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");
const outDir = join(root, "..", "docs", "screenshots");
const baseUrl = process.argv[2] ?? "http://localhost:4173/app.html";
const profile = join(process.env.TEMP ?? "/tmp", `pdfsak-chrome-${Date.now()}`);

const chromeCandidates = [
  "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
  "C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe",
  "C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe",
  "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
  "/usr/bin/google-chrome",
  "/usr/bin/chromium",
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
];
const chrome = chromeCandidates.find((candidate) => existsSync(candidate));
if (!chrome) {
  console.error("No Chrome/Edge binary found; skipping browser validation.");
  process.exit(2);
}

const port = 9333;
const chromeProcess = spawn(
  chrome,
  [
    "--headless=new",
    "--disable-gpu",
    "--no-first-run",
    "--no-default-browser-check",
    `--user-data-dir=${profile}`,
    `--remote-debugging-port=${port}`,
    "--window-size=1400,900",
    "--hide-scrollbars",
    "about:blank",
  ],
  { stdio: "ignore" },
);

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function findTarget() {
  for (let attempt = 0; attempt < 60; attempt += 1) {
    try {
      const response = await fetch(`http://127.0.0.1:${port}/json/list`);
      const targets = await response.json();
      const page = targets.find((target) => target.type === "page");
      if (page?.webSocketDebuggerUrl) return page.webSocketDebuggerUrl;
    } catch {
      // Chrome is not up yet.
    }
    await sleep(250);
  }
  throw new Error("DevTools endpoint did not appear");
}

class Cdp {
  constructor(url) {
    this.socket = new WebSocket(url);
    this.nextId = 1;
    this.pending = new Map();
    this.socket.addEventListener("message", (event) => {
      const message = JSON.parse(event.data);
      const resolver = this.pending.get(message.id);
      if (resolver) {
        this.pending.delete(message.id);
        resolver(message);
      }
    });
  }

  ready() {
    return new Promise((resolve) => this.socket.addEventListener("open", () => resolve(), { once: true }));
  }

  send(method, params = {}) {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, (message) => {
        if (message.error) reject(new Error(`${method}: ${message.error.message}`));
        else resolve(message.result);
      });
      this.socket.send(JSON.stringify({ id, method, params }));
    });
  }

  async evaluate(expression) {
    const result = await this.send("Runtime.evaluate", {
      expression,
      awaitPromise: true,
      returnByValue: true,
    });
    return result.result?.value;
  }

  async screenshot(path) {
    const result = await this.send("Page.captureScreenshot", { format: "png" });
    await writeFile(path, Buffer.from(result.data, "base64"));
  }

  close() {
    this.socket.close();
  }
}

async function main() {
  const socketUrl = await findTarget();
  const cdp = new Cdp(socketUrl);
  await cdp.ready();
  await cdp.send("Page.enable");

  // 1. Self test (real operations in the browser, incl. pdf.js rendering).
  await cdp.send("Page.navigate", { url: `${baseUrl}?selftest=1` });
  const report = await cdp.evaluate(`
    new Promise((resolve) => {
      const started = Date.now();
      const timer = setInterval(() => {
        const element = document.getElementById('selftest');
        const text = element ? element.textContent || '' : '';
        if (text.includes('SELFTEST_RESULT')) {
          clearInterval(timer);
          resolve(text);
        } else if (Date.now() - started > 90000) {
          clearInterval(timer);
          resolve('TIMEOUT: ' + text.slice(0, 400));
        }
      }, 400);
    })
  `);
  const text = String(report ?? "no result");
  console.log(text);
  const selfTestOk = text.includes("SELFTEST_RESULT=OK");

  // 2. Screenshots for the README (home + reader with a demo document).
  await mkdir(outDir, { recursive: true });
  await cdp.send("Page.navigate", { url: baseUrl });
  await sleep(2500);
  await cdp.screenshot(join(outDir, "20-extension-home.png"));
  await cdp.send("Page.navigate", { url: `${baseUrl}?demo=1&screen=reader` });
  await sleep(6000);
  await cdp.screenshot(join(outDir, "21-extension-reader.png"));
  await cdp.send("Page.navigate", { url: `${baseUrl}?demo=1&screen=organize` });
  await sleep(6000);
  await cdp.screenshot(join(outDir, "22-extension-organize.png"));

  // 3. Context-menu deep link: the consent prompt must show the linked host,
  // and a hostile `javascript:` URL must be ignored instead of prompting.
  const encoded = encodeURIComponent("https://example.com/docs/report.pdf");
  await cdp.send("Page.navigate", { url: `${baseUrl}#url=${encoded}` });
  const bannerText = await cdp.evaluate(`
    new Promise((resolve) => {
      const started = Date.now();
      const timer = setInterval(() => {
        const element = document.querySelector('[data-testid="remote-link-banner"]');
        if (element) {
          clearInterval(timer);
          resolve(element.textContent || '');
        } else if (Date.now() - started > 15000) {
          clearInterval(timer);
          resolve('');
        }
      }, 200);
    })
  `);
  const deepLinkOk = String(bannerText).includes("example.com");
  console.log(deepLinkOk ? "[PASS] deep link consent prompt shows the linked host" : `[FAIL] deep link consent prompt: ${bannerText}`);

  await cdp.send("Page.navigate", { url: `${baseUrl}#url=javascript:alert(1)` });
  await sleep(1500);
  const hostilePrompt = await cdp.evaluate(`Boolean(document.querySelector('[data-testid="remote-link-banner"]'))`);
  const hostileOk = hostilePrompt === false;
  console.log(hostileOk ? "[PASS] hostile deep link is ignored" : "[FAIL] hostile deep link opened a prompt");

  const ok = selfTestOk && deepLinkOk && hostileOk;
  cdp.close();
  await rm(profile, { recursive: true, force: true }).catch(() => undefined);
  chromeProcess.kill();
  process.exit(ok ? 0 : 1);
}

main().catch(async (error) => {
  console.error(error);
  chromeProcess.kill();
  process.exit(1);
});
