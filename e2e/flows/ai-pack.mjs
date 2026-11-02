/**
 * The AI task-pack pipeline, end to end, against the local mock provider.
 *
 * A pack is: pick a document -> prompt -> provider answer -> strict JSON
 * verification. This flow configures the app's provider through Settings (the
 * base URL points at `scripts/mock-deepseek.mjs`), launches the AI packs screen
 * with a document through the dev context, lets the screen's auto-run do the
 * round trip, and requires the verification card to report a verified answer.
 *
 * Nothing leaves the machine: the provider is the mock server the flow starts
 * and stops itself.
 */
import { spawn } from "node:child_process";
import { join } from "node:path";
import {
  clearElement,
  clickElement,
  execute,
  findElement,
  root,
  screenshot,
  sendKeys,
  sleep,
  waitForScript,
  withSession,
} from "../lib/driver.mjs";
import { elementKey } from "../lib/files.mjs";

const MOCK_PORT = 8899;
const MOCK_URL = `http://127.0.0.1:${MOCK_PORT}/v1`;
const MARKER_KEY = "e2e-ai-pack-key";

/** Starts the mock provider and waits until it answers. */
async function startMock() {
  const server = spawn(process.execPath, [join(root, "scripts", "mock-deepseek.mjs"), String(MOCK_PORT)], {
    stdio: ["ignore", "pipe", "pipe"],
  });
  let log = "";
  server.stdout.on("data", (chunk) => (log += chunk));
  server.stderr.on("data", (chunk) => (log += chunk));
  const started = Date.now();
  while (Date.now() - started < 15000) {
    try {
      const response = await fetch(`http://127.0.0.1:${MOCK_PORT}/chat/completions`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ messages: [{ role: "user", content: "single word: ready" }] }),
      });
      if (response.ok) return server;
    } catch {
      // not listening yet
    }
    await sleep(200);
  }
  server.kill();
  throw new Error(`the mock provider did not start: ${log.slice(-400)}`);
}

/** Points the app at the mock provider through the Settings UI. */
export async function configureProviderFlow(session) {
  await waitForScript(
    session,
    "return document.querySelector('[data-testid=\"ai-base-url\"]') && document.querySelector('[data-testid=\"ai-key-input\"]') ? 1 : 0",
    45000,
    "the provider fields",
  );
  const baseUrl = await findElement(session, '[data-testid="ai-base-url"]');
  await clearElement(session, elementKey(baseUrl));
  await sendKeys(session, elementKey(baseUrl), MOCK_URL);

  const key = await findElement(session, '[data-testid="ai-key-input"]');
  await clearElement(session, elementKey(key));
  await sendKeys(session, elementKey(key), MARKER_KEY);

  const save = await findElement(session, '[data-testid="ai-save"]');
  await clickElement(session, elementKey(save));
  await waitForScript(
    session,
    "const el=document.querySelector('[data-testid=\"ai-key-storage\"]'); return el && el.dataset.configured === 'true' ? 1 : 0",
    30000,
    "the provider to be saved",
  );
  await screenshot(session, "ai-pack-provider");
  console.log(`PASS: the provider points at the mock (${MOCK_URL})`);
}

/** Waits for the pack run and requires a verified answer. */
export async function packFlow(session) {
  // The dev launch hands the document to the screen; the flow then does what a
  // user does - tick the consent box and press Run - instead of relying on the
  // development auto-run, so the same path is exercised.
  const document = await waitForScript(
    session,
    // The row shows a placeholder until the file lands, and waitForScript
    // resolves on the first truthy value - so the placeholder must not count.
    "const el=document.querySelector('[data-testid=\"ai-pack-doc\"]'); return el && Number(el.dataset.files) > 0 ? el.textContent.trim() : '';",
    45000,
    "the launched document",
  );
  if (!/sample-1/.test(document)) {
    const detail = await execute(
      session,
      "const el=document.querySelector('[data-testid=\"ai-pack-doc\"]'); const dev=window.__PDFSAK_DEV__; return JSON.stringify({text:el?el.textContent.trim():null, files:el?el.dataset.files:null, handed:el?el.dataset.handedOver:null});",
    ).catch(() => "{}");
    throw new Error(`the screen did not receive the document: "${document}" (${detail})`);
  }

  const consent = await findElement(session, '[data-testid="ai-pack-consent"]');
  await clickElement(session, elementKey(consent));
  const runButton = await findElement(session, '[data-testid="ai-pack-run"]');
  await clickElement(session, elementKey(runButton));

  let state;
  try {
    state = await waitForScript(
      session,
      "const el=document.querySelector('[data-testid=\"ai-pack-result\"]'); return el ? el.dataset.parsed + '|' + el.dataset.ok : '';",
      90000,
      "the task-pack verdict",
    );
  } catch (error) {
    // The screen owns the reason (a refused provider, a parse error, ...): quote
    // it and the run button's state instead of a bare timeout.
    const detail = await execute(
      session,
      "const err=document.querySelector('[data-testid=\"ai-pack-error\"]'); const run=document.querySelector('[data-testid=\"ai-pack-run\"]'); return JSON.stringify({error:err?err.textContent.trim():null, runDisabled:run?run.disabled:null, runLabel:run?run.textContent.trim():null});",
    ).catch(() => "{}");
    throw new Error(`no verdict: ${error.message} (${detail})`);
  }
  const [parsed, ok] = state.split("|");
  if (parsed !== "true" || ok !== "true") {
    const shown = await waitForScript(
      session,
      "const el=document.querySelector('[data-testid=\"ai-pack-error\"]'); return el ? el.textContent.trim() : '';",
      5000,
      "the screen error",
    ).catch(() => "");
    throw new Error(`the pack did not verify (${state})${shown ? `: ${shown}` : ""}`);
  }
  const text = await waitForScript(
    session,
    "const el=document.querySelector('[data-testid=\"ai-pack-result\"]'); return el ? el.innerText.slice(0, 400) : '';",
    15000,
    "the verdict text",
  );
  await screenshot(session, "ai-pack");
  console.log(`PASS: the task pack verified its JSON answer (${text.replace(/\\s+/g, " ").slice(0, 120)})`);
}

/** Whole flow: mock up, provider configured, pack run, mock down. */
export async function aiPackFlow() {
  const server = await startMock();
  try {
    await withSession("ai-pack-settings", { PDFSAK_START_SCREEN: "settings" }, configureProviderFlow);
    const sample = join(root, "samples", "sample-1.pdf");
    await withSession(
      "ai-pack-run",
      { PDFSAK_START_SCREEN: "aiPacks", PDFSAK_DEV_FILES: sample, PDFSAK_DEV_RUN: "1" },
      packFlow,
    );
  } finally {
    server.kill();
  }
}
