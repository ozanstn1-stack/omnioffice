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
 * Checks that fail `--check`:
 *   * a key missing from `tr` (or present only there),
 *   * a key used with a literal name that the table does not have,
 *   * a `{placeholder}` set that differs between the two languages,
 *   * more user-visible JSX text than `i18n-baseline.json` records (a ratchet:
 *     moving a string into the table lowers the baseline with `--update`).
 *
 * Advisory (printed, never fails): values identical in both languages, and keys
 * no literal `t("…")` call reaches - both have legitimate cases the audit
 * cannot separate from real defects.
 *
 * Usage:
 *   node scripts/i18n-audit.mjs            # summary
 *   node scripts/i18n-audit.mjs --check    # exit 1 when anything is out of sync
 *   node scripts/i18n-audit.mjs --tsv      # key<TAB>english source table
 *   node scripts/i18n-audit.mjs --update   # re-record the JSX-text baseline
 */
import { existsSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";

const enPath = fileURLToPath(new URL("../src/lib/i18n/en.json", import.meta.url));
const trPath = fileURLToPath(new URL("../src/lib/i18n/tr.json", import.meta.url));
const baselinePath = fileURLToPath(new URL("./i18n-baseline.json", import.meta.url));

// The dictionaries are JSON assets the app fetches at runtime; they are the
// source of truth for this audit.
const en = JSON.parse(readFileSync(enPath, "utf8"));
const tr = JSON.parse(readFileSync(trPath, "utf8"));

const enKeys = Object.keys(en);
const trKeys = new Set(Object.keys(tr));
const missing = enKeys.filter((key) => !trKeys.has(key));
const extra = [...trKeys].filter((key) => !enKeys.includes(key));
const check = process.argv.includes("--check");

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
    else if (/\.(ts|tsx)$/.test(entry) && !/\.test\./.test(entry) && entry !== "i18n.ts") out.push(full);
  }
  return out;
}

const enKeySet = new Set(enKeys);
const usedMissing = new Map();
const usedKeys = new Set();
for (const file of walk(srcRoot)) {
  const body = readFileSync(file, "utf8");
  for (const match of body.matchAll(USAGE)) {
    const key = match[1];
    usedKeys.add(key);
    if (enKeySet.has(key)) continue;
    if (!usedMissing.has(key)) usedMissing.set(key, new Set());
    usedMissing.get(key).add(file.slice(srcRoot.length + 1).replace(/\\/g, "/"));
  }
}
const usedMissingSorted = [...usedMissing.entries()].sort(([a], [b]) => a.localeCompare(b));

// ---------------------------------------------------------------------------
// Placeholder parity: "{count} dosya" and "{count} files" must agree, or the
// Turkish string renders a literal `{count}` (or silently drops the number).
// ---------------------------------------------------------------------------
const PLACEHOLDER = /\{\s*([A-Za-z0-9_]+)\s*\}/g;
function placeholders(value) {
  return new Set([...String(value).matchAll(PLACEHOLDER)].map((match) => match[1]));
}
const placeholderMismatches = [];
for (const key of enKeys) {
  if (!trKeys.has(key)) continue;
  const source = placeholders(en[key]);
  const target = placeholders(tr[key]);
  const onlySource = [...source].filter((name) => !target.has(name));
  const onlyTarget = [...target].filter((name) => !source.has(name));
  if (onlySource.length || onlyTarget.length) {
    placeholderMismatches.push({ key, onlySource, onlyTarget });
  }
}

// ---------------------------------------------------------------------------
// Count-aware strings: `t.plural("key", n)` needs `key.one` and `key.other`, and
// no value may spell a plural as "(s)" - that is what the helper replaced.
// ---------------------------------------------------------------------------
const PLURAL_USAGE = /\bt\.plural\(\s*"([a-z][A-Za-z0-9_-]*(?:\.[A-Za-z0-9_-]+)+)"/g;
const pluralKeys = new Set();
for (const file of walk(srcRoot)) {
  const body = readFileSync(file, "utf8");
  for (const match of body.matchAll(PLURAL_USAGE)) pluralKeys.add(match[1]);
}
const pluralProblems = [];
for (const key of [...pluralKeys].sort()) {
  for (const [language, table] of [
    ["en", en],
    ["tr", tr],
  ]) {
    if (table[`${key}.one`] === undefined || table[`${key}.other`] === undefined) {
      pluralProblems.push(`${key} is used with t.plural but ${language}.json lacks .one/.other`);
    }
  }
}
const fakePlurals = enKeys.filter((key) => /\(s\)/.test(String(en[key])) || /\(s\)/.test(String(tr[key] ?? "")));

// ---------------------------------------------------------------------------
// Hardcoded JSX text, measured with a real parser so generics and comparisons
// (`a > b`, `Map<K, V>`) cannot be mistaken for prose. The baseline is a
// ratchet: it may only shrink, which is how the strings move into the table.
// ---------------------------------------------------------------------------
const PROSE = /[A-Za-zÇĞİÖŞÜçğıöşü]{2,}/;
// Attributes the user reads or hears. A model prompt example ("tr, en, de") or a
// format name ("pdf, docx, md") is literal on purpose and still counted - which
// is why the baseline exists instead of a zero target.
const ATTRIBUTES = ["alt", "aria-label", "title", "placeholder"];

function jsxStrings() {
  const counts = {};
  const attributes = {};
  const samples = new Map();
  for (const file of walk(srcRoot)) {
    if (!file.endsWith(".tsx")) continue;
    const body = readFileSync(file, "utf8");
    const source = ts.createSourceFile(file, body, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
    const relative = file.slice(srcRoot.length + 1).replace(/\\/g, "/");
    const visit = (node) => {
      if (ts.isJsxText(node)) {
        const text = node.text.replace(/\s+/g, " ").trim();
        // Letters are what makes text user-visible; "—", "·", "1" and "/" are
        // separators the table never needs to hold.
        if (PROSE.test(text)) {
          counts[relative] = (counts[relative] ?? 0) + 1;
          if (!samples.has(relative)) samples.set(relative, []);
          if (samples.get(relative).length < 3) samples.get(relative).push(text.slice(0, 48));
        }
      }
      if (ts.isJsxAttribute(node) && node.initializer && ts.isStringLiteral(node.initializer)) {
        const name = node.name.getText(source);
        if (ATTRIBUTES.includes(name) && PROSE.test(node.initializer.text.trim())) {
          attributes[relative] = (attributes[relative] ?? 0) + 1;
        }
      }
      ts.forEachChild(node, visit);
    };
    visit(source);
  }
  return { counts, attributes, samples };
}

const { counts: jsxCounts, attributes: attrCounts, samples: jsxSamples } = jsxStrings();
const baseline = existsSync(baselinePath)
  ? JSON.parse(readFileSync(baselinePath, "utf8"))
  : { version: 2, files: {}, attributes: {} };
const known = baseline.files ?? {};
const knownAttributes = baseline.attributes ?? {};

if (process.argv.includes("--update")) {
  const grown = [
    ...Object.entries(jsxCounts).filter(([file, count]) => known[file] !== undefined && count > known[file]),
    ...Object.entries(attrCounts).filter(
      ([file, count]) => knownAttributes[file] !== undefined && count > knownAttributes[file],
    ),
  ];
  if (grown.length && !process.argv.includes("--allow-raise")) {
    for (const [file, count] of grown) {
      console.error(`i18n:update: ${file} would grow ${known[file] ?? knownAttributes[file]} -> ${count}`);
    }
    console.error(
      "i18n:update: the hardcoded-text baseline may only shrink; pass --allow-raise to move it on purpose.",
    );
    process.exit(1);
  }
  const sort = (counts) => Object.fromEntries(Object.entries(counts).sort(([a], [b]) => a.localeCompare(b)));
  const sorted = sort(jsxCounts);
  const sortedAttributes = sort(attrCounts);
  writeFileSync(
    baselinePath,
    `${JSON.stringify({ version: 2, files: sorted, attributes: sortedAttributes }, null, 2)}
`,
  );
  const total = Object.values(sorted).reduce((sum, count) => sum + count, 0);
  const attrTotal = Object.values(sortedAttributes).reduce((sum, count) => sum + count, 0);
  console.log(`i18n:update: ${total} hardcoded JSX text nodes and ${attrTotal} attribute strings recorded`);
  process.exit(0);
}

const jsxProblems = Object.entries(jsxCounts)
  .filter(([file, count]) => count > (known[file] ?? 0))
  .map(([file, count]) => `${file}: ${count} hardcoded text node(s), baseline ${known[file] ?? 0}`);
const attrProblems = Object.entries(attrCounts)
  .filter(([file, count]) => count > (knownAttributes[file] ?? 0))
  .map(([file, count]) => `${file}: ${count} hardcoded attribute string(s), baseline ${knownAttributes[file] ?? 0}`);

const untranslated = enKeys.filter((key) => trKeys.has(key) && en[key] === tr[key]);
const unused = enKeys.filter((key) => !usedKeys.has(key));
const jsxTotal = Object.values(jsxCounts).reduce((sum, count) => sum + count, 0);
const attrTotal = Object.values(attrCounts).reduce((sum, count) => sum + count, 0);

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
    usedMissingSorted.length +
    " placeholder-mismatch=" +
    placeholderMismatches.length +
    " plural-usage=" +
    pluralKeys.size +
    " hardcoded-jsx=" +
    jsxTotal +
    " hardcoded-attrs=" +
    attrTotal,
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
for (const mismatch of placeholderMismatches.slice(0, 20)) {
  const detail = [
    mismatch.onlySource.length ? `only in en: ${mismatch.onlySource.join(", ")}` : "",
    mismatch.onlyTarget.length ? `only in tr: ${mismatch.onlyTarget.join(", ")}` : "",
  ]
    .filter(Boolean)
    .join("; ");
  console.log(`  placeholder mismatch: ${mismatch.key} (${detail})`);
}
for (const file of Object.keys(jsxCounts)
  .sort((a, b) => jsxCounts[b] - jsxCounts[a])
  .slice(0, 10)) {
  console.log(
    `  hardcoded text: ${file} has ${jsxCounts[file]} node(s)` +
      (known[file] !== undefined ? ` (baseline ${known[file]})` : " (not in the baseline)") +
      ` e.g. ${JSON.stringify(jsxSamples.get(file) ?? [])}`,
  );
}
console.log(`  advisory: identical-in-en-and-tr=${untranslated.length}, not-reached-by-a-literal-key=${unused.length}`);

const problems =
  missing.length +
  extra.length +
  usedMissingSorted.length +
  placeholderMismatches.length +
  pluralProblems.length +
  fakePlurals.length +
  jsxProblems.length +
  attrProblems.length;
if (check && problems) {
  for (const problem of [...jsxProblems, ...attrProblems].slice(0, 20)) console.error("  " + problem);
  console.error("i18n-audit: the translation tables are out of sync");
  process.exit(1);
}
process.exit(0);
