/**
 * Documents handed to the app on its command line, the way Explorer does it
 * (double-click, "Open with", "Send to"): no dev launch context, only argv.
 *
 * 1. Forwarded: the app runs on Home and a second process is started with a
 *    document, exactly as Explorer starts it; the running window must open it.
 * 2. Cold start: the app is launched with a PDF; it must leave Home for the
 *    Reader.
 *
 * Both scenarios always run and report, so one failure does not hide the
 * other. Engine-free: the checks read the screen and the file name, not a
 * rendered page, so the flow runs in the Linux job as well as on Windows.
 */
import { spawn, spawnSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

import { executeAsync, exe, killAppInstances, screenshot, sleep, waitForScript, withSession } from "../lib/driver.mjs";
import { prepareWorkFile } from "../lib/files.mjs";

const onScreen = (text) =>
  // Home is the screen with the tool grid; the opened document's name shows
  // in the reader / workspace once it has left Home.
  `return !document.querySelector('.tool-card') && document.body.innerText.includes(${JSON.stringify(text)})`;

// What the Rust side sees on this command line; printed so a failure on the
// Windows runner says whether the file never reached the app or the UI.
const STARTUP_FILES = `const done = arguments[arguments.length - 1];
const started = Date.now();
const ask = () => {
  const ipc = window.__TAURI_INTERNALS__;
  if (!ipc) {
    if (Date.now() - started < 20000) return setTimeout(ask, 250);
    return done("no IPC");
  }
  ipc.invoke("office_startup_files")
    .then((refs) => done(JSON.stringify(refs.map((ref) => ref.name))))
    .catch((error) => done("error: " + String(error)));
};
ask();`;

/** The app's real command lines (Windows), to compare with Explorer's. */
function appCommandLines() {
  if (process.platform !== "win32") return "";
  const result = spawnSync(
    "powershell",
    [
      "-NoProfile",
      "-Command",
      "Get-CimInstance Win32_Process -Filter \"Name='OmniOffice.exe'\" | ForEach-Object { $_.CommandLine }",
    ],
    { encoding: "utf8" },
  );
  return (result.stdout ?? "").trim();
}

/** Newest DevToolsActivePort under the app's WebView2 data folder (E2E build). */
function devToolsPort(since) {
  const base = process.env.LOCALAPPDATA;
  if (!base) return null;
  const stack = [base];
  let found = null;
  while (stack.length) {
    const dir = stack.pop();
    let entries = [];
    try {
      entries = readdirSync(dir, { withFileTypes: true });
    } catch {
      continue;
    }
    for (const entry of entries) {
      const path = join(dir, entry.name);
      if (entry.isDirectory() && !entry.isSymbolicLink() && dir.split(/[\\/]/).length < 12) stack.push(path);
      else if (entry.name === "DevToolsActivePort") {
        const mtime = statSync(path).mtimeMs;
        if (mtime >= since && (!found || mtime > found.mtime)) found = { path, mtime };
      }
    }
  }
  if (!found || !existsSync(found.path)) return null;
  const port = Number(readFileSync(found.path, "utf8").split(/\r?\n/)[0]);
  return Number.isFinite(port) && port > 0 ? port : null;
}

/** Evaluates `expression` in the app page over the WebView2 DevTools port. */
async function evaluateOverCdp(port, expression) {
  const pages = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
  const page = pages.find((entry) => entry.type === "page" && entry.webSocketDebuggerUrl);
  if (!page) throw new Error("no page target");
  const socket = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    socket.onopen = resolve;
    socket.onerror = () => reject(new Error("DevTools socket failed"));
  });
  try {
    return await new Promise((resolve, reject) => {
      socket.onmessage = (event) => {
        const message = JSON.parse(String(event.data));
        if (message.id === 1) resolve(message.result?.result?.value);
      };
      socket.onerror = () => reject(new Error("DevTools socket failed"));
      socket.send(JSON.stringify({ id: 1, method: "Runtime.evaluate", params: { expression, returnByValue: true } }));
    });
  } finally {
    socket.close();
  }
}

/**
 * Windows cold start exactly as Explorer does it: OmniOffice.exe "<file>",
 * with no WebDriver in between (msedgedriver turns `args` into `--<arg>`
 * switches, which the app rightly ignores). The page is read over the
 * DevTools port the E2E build opens.
 */
async function windowsColdStart(pdfPath) {
  killAppInstances();
  await sleep(500);
  const started = Date.now() - 1000;
  const app = spawn(exe, [pdfPath], { stdio: "ignore" });
  app.on("error", () => undefined);
  try {
    let port = null;
    while (!port && Date.now() - started < 30000) {
      await sleep(500);
      port = devToolsPort(started);
    }
    if (!port) throw new Error("no DevTools port (is this the E2E build?)");
    const check = `(${onScreen("sample-1.pdf").replace(/^return /, "() => ")})()`;
    const files = `window.__TAURI_INTERNALS__?.invoke("office_startup_files").then((refs) => JSON.stringify(refs.map((r) => r.name)))`;
    let last = "";
    while (Date.now() - started < 60000) {
      last = String(
        await evaluateOverCdp(port, `(async () => (${check}) + " " + (await ${files}))()`).catch((e) => e.message),
      );
      if (last.startsWith("true")) {
        console.log(`launch-file: cold start (Explorer) -> ${last}`);
        return;
      }
      await sleep(1000);
    }
    throw new Error(`cold start stayed off the reader (last: ${last})`);
  } finally {
    killAppInstances();
  }
}

async function attempt(label, run) {
  try {
    await run();
    console.log(`PASS: ${label}`);
    return null;
  } catch (error) {
    console.log(`FAIL: ${label}: ${error.message}`);
    return `${label}: ${error.message}`;
  }
}

export async function launchFileFlow() {
  const pdfPath = prepareWorkFile("sample-1.pdf");
  const docPath = prepareWorkFile("test-document.docx");
  const failures = [];

  await withSession("launch-forwarded", {}, async (session) => {
    await waitForScript(session, "return document.querySelectorAll('.tool-card').length", 30000, "home grid");
    const failure = await attempt("a document from a second launch opens in the running window", async () => {
      // The second process finds the running instance, forwards its argv and
      // exits; the running window must open the document.
      const second = spawn(exe, [docPath], { stdio: "ignore" });
      second.on("error", () => undefined);
      await sleep(1000);
      try {
        await waitForScript(session, onScreen("test-document"), 30000, "forwarded document open in the workspace");
      } finally {
        await screenshot(session, "launch-forwarded").catch(() => undefined);
      }
    });
    if (failure) failures.push(failure);
  });

  if (process.platform === "win32") {
    const failure = await attempt("a PDF on the command line opens in the reader (Explorer cold start)", () =>
      windowsColdStart(pdfPath),
    );
    if (failure) failures.push(failure);
    if (failures.length) throw new Error(failures.join(" | "));
    return;
  }

  await withSession(
    "launch-cold",
    {},
    async (session) => {
      console.log(`launch-file: app command line(s): ${appCommandLines() || "(not Windows)"}`);
      console.log(`launch-file: office_startup_files -> ${await executeAsync(session, STARTUP_FILES)}`);
      const failure = await attempt("a PDF on the command line opens in the reader", async () => {
        try {
          await waitForScript(session, onScreen("sample-1.pdf"), 30000, "launched PDF open in the reader");
        } finally {
          await screenshot(session, "launch-cold").catch(() => undefined);
        }
      });
      if (failure) failures.push(failure);
    },
    [pdfPath],
  );

  if (failures.length) throw new Error(failures.join(" | "));
}
