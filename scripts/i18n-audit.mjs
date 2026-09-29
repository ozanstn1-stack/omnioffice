#!/usr/bin/env node
/**
 * Translation table audit for src/lib/i18n.ts.
 *
 * The dictionaries are a flat TypeScript table (one entry per line, on
 * purpose: src/lib/i18n-encoding.test.ts scans it line by line), so the only
 * way to notice a key that exists in English but was never translated is to
 * compare the two blocks.
 *
 * Usage:
 *   node scripts/i18n-audit.mjs            # summary
 *   node scripts/i18n-audit.mjs --check    # exit 1 when Turkish is missing keys
 *   node scripts/i18n-audit.mjs --tsv      # key<TAB>english source table
 */
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const sourcePath = fileURLToPath(new URL("../src/lib/i18n.ts", import.meta.url));
const source = readFileSync(sourcePath, "utf8");

const enStart = source.indexOf("const en: Dict = {");
const trStart = source.indexOf("const tr: Dict = {");
const dictStart = source.indexOf("const dictionaries");
if (enStart < 0 || trStart < 0 || dictStart < 0) {
  console.error("i18n-audit: could not find the en/tr dictionary blocks in " + sourcePath);
  process.exit(2);
}

const enBody = source.slice(enStart, trStart);
const trBody = source.slice(trStart, dictStart);

const keysOf = (body) => [...body.matchAll(/^\s*"([^"]+)":/gm)].map((match) => match[1]);

const enKeys = keysOf(enBody);
const trKeys = new Set(keysOf(trBody));
const missing = enKeys.filter((key) => !trKeys.has(key));
const extra = [...trKeys].filter((key) => !enKeys.includes(key));

if (process.argv.includes("--tsv")) {
  const out = enKeys
    .map((key) => {
      const line = enBody.split("\n").find((entry) => entry.trim().startsWith('"' + key + '":'));
      const value = line
        ? line.trim().slice(line.indexOf(":") + 1).trim().replace(/,$/, "").replace(/^"|"$/g, "")
        : "?";
      return key + "\t" + value;
    })
    .join("\n");
  console.log(out);
  process.exit(0);
}

console.log(
  "i18n: en=" + enKeys.length + " tr=" + trKeys.size + " missing-in-tr=" + missing.length + " extra-in-tr=" + extra.length,
);
if (missing.length) {
  console.log("  missing (first 20): " + missing.slice(0, 20).join(", "));
}
if (extra.length) {
  console.log("  extra (first 20): " + extra.slice(0, 20).join(", "));
}

if (process.argv.includes("--check") && (missing.length || extra.length)) {
  console.error("i18n-audit: the en and tr tables are out of sync");
  process.exit(1);
}
process.exit(0);