/**
 * The WebDriver plumbing behind the end-to-end flows: one session per scenario,
 * bounded calls, a screenshot on failure and the dev launch context.
 *
 * Everything here is exported and side-effect free at import time - the binary
 * check lives in requireAppBinary() so tools can import this module without a
 * built app.
 */
import { spawn, spawnSync } from "node:child_process";
import { existsSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const e2eDir = resolve(here, "..");
export const root = resolve(e2eDir, "..");
export const artifacts = join(e2eDir, "artifacts");
export const work = join(artifacts, "work");
export const argv = process.argv.slice(2);
export const valueOf = (flag) => {
  const index = argv.indexOf(flag);
  return index >= 0 ? argv[index + 1] : undefined;
};
export const pdf = valueOf("--pdf");
export const basePort = Number(process.env.TAURI_DRIVER_PORT ?? 4444);

const exeName = process.platform === "win32" ? "OmniOffice.exe" : "OmniOffice";
export const exe = valueOf("--exe") ?? join(root, "target", "debug", exeName);

/** Fails before the first session when the debug binary is missing. */
export function requireAppBinary() {
  if (!existsSync(exe)) {
    console.error(`app binary not found: ${exe}\nrun: npm run tauri -- build --debug --no-bundle`);
    process.exit(1);
  }
}

export const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// Every WebDriver call is bounded: a stalled native driver used to leave the
// CI step waiting with no output until the job-level limit (one deep-flows
// run sat for 36 minutes). Session creation launches the app on a loaded CI
// runner, so the bound is generous (and createSession retries once); a timeout
// surfaces as a normal failure with a screenshot.
const WEBDRIVER_TIMEOUT_MS = 150_000;

export function webdriver(port, path, method = "GET", body) {
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

export const execute = (session, script) =>
  webdriver(session.port, `/session/${session.id}/execute/sync`, "POST", { script, args: [] });

// `execute/sync` cannot return a promise, and `invoke` is async: WebDriver's
// async endpoint hands the script a callback as its last argument.
export const executeAsync = (session, script) =>
  webdriver(session.port, `/session/${session.id}/execute/async`, "POST", { script, args: [] });

export function findElement(session, selector) {
  return webdriver(session.port, `/session/${session.id}/element`, "POST", { using: "css selector", value: selector });
}

export function clickElement(session, elementId) {
  return webdriver(session.port, `/session/${session.id}/element/${elementId}/click`, "POST", {});
}

export async function waitForScript(session, script, timeoutMs, label) {
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
export async function waitForScreenReady(session, screen, timeoutMs) {
  return waitForScript(
    session,
    `return document.documentElement.dataset.screenReady === ${JSON.stringify(screen)} ? (document.documentElement.dataset.screenReadyDetail || true) : false`,
    timeoutMs,
    `${screen} ready signal (data-screen-ready)`,
  );
}

export async function waitForDriver(port) {
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
export async function createSession(port) {
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

export async function waitForFile(path, timeoutMs, label) {
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

export function screenshot(session, name) {
  return webdriver(session.port, `/session/${session.id}/screenshot`).then((shot) =>
    writeFileSync(join(artifacts, `${name}.png`), Buffer.from(shot, "base64")),
  );
}

/** Launches tauri-driver + one session, runs `steps`, and always tears down. */
export async function withSession(name, env, steps) {
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
export async function withSessionRetry(name, env, steps) {
  try {
    return await withSession(name, env, steps);
  } catch (error) {
    console.warn(`RETRY: ${name} failed (${error.message}); relaunching once with a fresh session`);
    return withSession(`${name}-retry`, env, steps);
  }
}

export function killAppInstances() {
  if (process.platform === "win32") {
    spawnSync("taskkill", ["/IM", exeName, "/F"], { stdio: "ignore" });
  } else {
    spawnSync("pkill", ["-f", exeName], { stdio: "ignore" });
  }
}
