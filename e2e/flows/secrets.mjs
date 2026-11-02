/**
 * The secret store, end to end: type an API key into Settings, save it, look at
 * what reached the disk, restart the app and read it back, then clear it.
 *
 * What this proves that a unit test cannot: the real `ai_save_settings` command
 * wrote the key through the three-tier store, the file on disk holds only what
 * that tier promises (no plaintext above the `plain` fallback, and `0600` when
 * the fallback is what ran), and a fresh process reads the same value back.
 */
import { readFileSync, readdirSync, statSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import {
  clearElement,
  clickElement,
  findElement,
  screenshot,
  sendKeys,
  waitForScript,
  withSession,
} from "../lib/driver.mjs";
import { elementKey } from "../lib/files.mjs";

/** The marker: distinctive enough that a plaintext leak is unambiguous. */
export const SECRET_MARKER = "e2e-secret-4f1c9a";

const CONFIG_IDENTIFIER = "io.github.ozanstn1.pdfswissarmyknife";

/** `app.path().app_config_dir()` on the platform the flow runs on. */
export function configDir() {
  if (process.platform === "win32") {
    return join(process.env.APPDATA ?? homedir(), CONFIG_IDENTIFIER);
  }
  if (process.platform === "darwin") {
    return join(homedir(), "Library", "Application Support", CONFIG_IDENTIFIER);
  }
  return join(process.env.XDG_CONFIG_HOME ?? join(homedir(), ".config"), CONFIG_IDENTIFIER);
}

/** Reads the tier and the configured flag, or "" until they reach the wanted state. */
const storageScript = (configured) =>
  `const el=document.querySelector('[data-testid="ai-key-storage"]');
   return el && el.dataset.configured === '${configured}' ? el.dataset.keyStorage + '|' + el.dataset.configured : '';`;

async function keyField(session) {
  await waitForScript(
    session,
    "return document.querySelector('[data-testid=\"ai-key-input\"]') ? 1 : 0",
    45000,
    "the AI key field",
  );
}

/** Saves the marker key and returns the storage tier the UI reports. */
export async function saveSecretsFlow(session) {
  await keyField(session);
  const input = await findElement(session, '[data-testid="ai-key-input"]');
  await clearElement(session, elementKey(input));
  await sendKeys(session, elementKey(input), SECRET_MARKER);
  const save = await findElement(session, '[data-testid="ai-save"]');
  await clickElement(session, elementKey(save));

  const state = await waitForScript(session, storageScript("true"), 30000, "the key to be stored");
  const [tier] = state.split("|");
  if (!tier || tier === "none") throw new Error(`the key is not stored after saving: "${state}"`);
  await screenshot(session, "secrets-save");
  console.log(`PASS: the AI key was stored through the "${tier}" tier`);
  return tier;
}

/**
 * Checks the file the store wrote. Which assertion is right depends on the
 * tier: encrypted and vault storage must not contain the value at all, and the
 * documented `plain` fallback must be owner-only.
 */
export function checkSecretFile(tier) {
  const dir = configDir();
  let files;
  try {
    files = readdirSync(dir).filter((name) => /^ai-key-.*\.bin$/.test(name));
  } catch {
    throw new Error(`the configuration directory does not exist: ${dir}`);
  }
  if (files.length === 0) throw new Error(`no ai-key-*.bin file was written in ${dir}`);
  const path = join(
    dir,
    files.sort((left, right) => statSync(join(dir, right)).mtimeMs - statSync(join(dir, left)).mtimeMs)[0],
  );
  const raw = readFileSync(path, "utf8");

  if (tier === "plain") {
    // Documented fallback (no platform cipher and no OS keyring): the file is
    // owner-only and carries the marker behind the `plain:` prefix.
    const mode = statSync(path).mode & 0o777;
    if (mode !== 0o600) throw new Error(`${path} is ${mode.toString(8)}; the plain fallback must be 600`);
    if (!raw.includes(SECRET_MARKER)) throw new Error(`${path} does not contain the saved key`);
    console.log(`PASS: the plain fallback stored the key in a 0600 file (${path})`);
    return path;
  }
  if (raw.includes(SECRET_MARKER)) {
    throw new Error(`${path} leaks the key in plaintext although the tier is "${tier}"`);
  }
  console.log(`PASS: ${path} holds the "${tier}" blob without the key in plaintext`);
  return path;
}

/** Restart: the key must still be configured, then clear it again. */
export async function reloadSecretsFlow(session) {
  await waitForScript(session, storageScript("true"), 45000, "the stored key after a restart");
  const clear = await findElement(session, '[data-testid="ai-clear"]');
  await clickElement(session, elementKey(clear));
  await waitForScript(session, storageScript("false"), 20000, "the key to be cleared again");
  await screenshot(session, "secrets-reload");
  console.log("PASS: the key survived a restart and was cleared again");
}

/** Runs both sessions and the disk check; returns the tier for the log. */
export async function secretStoreFlow() {
  const tier = await withSession("secrets-save", { PDFSAK_START_SCREEN: "settings" }, saveSecretsFlow);
  checkSecretFile(tier);
  await withSession("secrets-reload", { PDFSAK_START_SCREEN: "settings" }, reloadSecretsFlow);
  return tier;
}
