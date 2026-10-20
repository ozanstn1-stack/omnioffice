#!/usr/bin/env node
/**
 * Translation table audit for src/lib/i18n/{en,tr}.json.
 *
 * The dictionaries are flat JSON assets (fetched at runtime), so the only
 * way to notice a key that exists in English but was never translated is to
 * compare the two blocks.
 *
 * The script also scans the source tree for translation calls with literal
 * keys (`t("…")`, `label("…")` and `tr("…")`) and reports keys that are used
 * in code but missing from the table, so a screen can no longer fall back to
 * hardcoded English strings unnoticed.
 *
 * Usage:
 *   node scripts/i18n-audit.mjs            # summary
 *   node scripts/i18n-audit.mjs --check    # exit 1 when anything is out of sync
 *   node scripts/i18n-audit.mjs --tsv      # key<TAB>english source table
 */
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const enPath = fileURLToPath(new URL("../src/lib/i18n/en.json", import.meta.url));
const trPath = fileURLToPath(new URL("../src/lib/i18n/tr.json", import.meta.url));

// The dictionaries are JSON assets the app fetches at runtime; they are the
// source of truth for this audit.
const en = JSON.parse(readFileSync(enPath, "utf8"));
const tr = JSON.parse(readFileSync(trPath, "utf8"));

const enKeys = Object.keys(en);
const trKeys = new Set(Object.keys(tr));
const missing = enKeys.filter((key) => !trKeys.has(key));
const extra = [...trKeys].filter((key) => !enKeys.includes(key));

if (process.argv.includes("--tsv")) {
  const out = enKeys
    .map((key) => {
      return key + "\t" + (en[key] ?? "?");
    })
    .join("\n");
  console.log(out);
  process.exit(0);
}

// Keys used in code with a literal dotted name, e.g. t("ai.run"), label("nav.redact", …).
const USAGE = /(?:\bt|\blabel|\btr)\(\s*"([a-z][A-Za-z0-9_-]*(?:\.[A-Za-z0-9_-]+)+)"/g;
const srcRoot = fileURLToPath(new URL("../src", import.meta.url));

function walk(dir, out = []) {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) walk(full, out);
    else if (/\.(ts|tsx)$/.test(entry) && !/\.test\./.test(entry) && entry !== "i18n.ts")
      out.push(full);
  }
  return out;
}

const enKeySet = new Set(enKeys);
const usedMissing = new Map();
for (const file of walk(srcRoot)) {
  const body = readFileSync(file, "utf8");
  for (const match of body.matchAll(USAGE)) {
    const key = match[1];
    if (enKeySet.has(key)) continue;
    if (!usedMissing.has(key)) usedMissing.set(key, new Set());
    usedMissing.get(key).add(file.slice(srcRoot.length + 1).replace(/\\/g, "/"));
  }
}
const usedMissingSorted = [...usedMissing.entries()].sort(([a], [b]) => a.localeCompare(b));

console.log(
  "i18n: en=" +
    enKeys.length +
    " tr=" +
    trKeys.size +
    " missing-in-tr=" +
    missing.length +
    " extra-in-tr=" +
    extra.length +
    " used-but-missing=" +
    usedMissingSorted.length,
);
if (missing.length) {
  console.log("  missing (first 20): " + missing.slice(0, 20).join(", "));
}
if (extra.length) {
  console.log("  extra (first 20): " + extra.slice(0, 20).join(", "));
}
for (const [key, files] of usedMissingSorted.slice(0, 40)) {
  console.log("  used but missing: " + key + " (" + [...files].join(", ") + ")");
}

if (process.argv.includes("--check") && (missing.length || extra.length || usedMissingSorted.length)) {
  console.error("i18n-audit: the translation tables are out of sync");
  process.exit(1);
}
process.exit(0);
