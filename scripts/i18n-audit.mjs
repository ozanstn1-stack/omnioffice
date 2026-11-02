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
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { scanSource } from "./lib/i18n-jsx.mjs";

// Test hooks: a temporary tree can be audited without touching the repository
// (the same shape `check-reproducibility-inputs.mjs` uses with REPRO_ROOT).
const auditRoot = process.env.I18N_AUDIT_ROOT ? resolve(process.env.I18N_AUDIT_ROOT) : undefined;
const repoPath = (relative) =>
  auditRoot ? join(auditRoot, relative) : fileURLToPath(new URL(`../${relative}`, import.meta.url));
const enPath = repoPath("src/lib/i18n/en.json");
const trPath = repoPath("src/lib/i18n/tr.json");
const baselinePath = process.env.I18N_BASELINE
  ? resolve(process.env.I18N_BASELINE)
  : fileURLToPath(new URL("./i18n-baseline.json", import.meta.url));

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
const srcRoot = repoPath("src");

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
// Hardcoded strings, measured with a real parser (see lib/i18n-jsx.mjs) so
// generics and comparisons cannot be mistaken for prose. Each section - text
// nodes, attributes, props - has its own baseline and its own ratchet.
// ---------------------------------------------------------------------------
const scan = { files: {}, attributes: {}, props: {} };
const samples = new Map();
const propRows = [];
for (const file of walk(srcRoot)) {
  if (!file.endsWith(".tsx")) continue;
  const relative = file.slice(srcRoot.length + 1).replace(/\\/g, "/");
  const found = scanSource(file, readFileSync(file, "utf8"));
  if (found.texts.length) scan.files[relative] = found.texts.length;
  if (found.attributes.length) scan.attributes[relative] = found.attributes.length;
  if (found.props.length) scan.props[relative] = found.props.length;
  if (found.texts.length)
    samples.set(
      relative,
      found.texts.slice(0, 3).map((row) => row.text.slice(0, 48)),
    );
  for (const row of found.props) propRows.push(`${relative}:${row.line}\t${row.name}\t${row.text}`);
}

if (process.argv.includes("--props-report")) {
  // The translation backlog, cheapest source first: one line per prop string.
  console.log(propRows.sort().join("\n"));
  console.log(`i18n:props-report: ${propRows.length} prop string(s) across ${Object.keys(scan.props).length} file(s)`);
  process.exit(0);
}

const baseline = existsSync(baselinePath)
  ? JSON.parse(readFileSync(baselinePath, "utf8"))
  : { version: 3, files: {}, attributes: {}, props: {} };
if (process.argv.includes("--check") && baseline.props === undefined) {
  console.error("i18n: the baseline has no `props` section yet; run `npm run i18n:audit -- --update --allow-raise`.");
  process.exit(1);
}
const known = { files: baseline.files ?? {}, attributes: baseline.attributes ?? {}, props: baseline.props ?? {} };

if (process.argv.includes("--update")) {
  const grown = Object.entries(scan).flatMap(([section, counts]) =>
    Object.entries(counts)
      .filter(([file, count]) => known[section][file] !== undefined && count > known[section][file])
      .map(([file, count]) => `${section}: ${file} would grow ${known[section][file]} -> ${count}`),
  );
  if (grown.length && !process.argv.includes("--allow-raise")) {
    for (const line of grown) console.error(`i18n:update: ${line}`);
    console.error("i18n:update: the baseline may only shrink; pass --allow-raise to move it on purpose.");
    process.exit(1);
  }
  const sort = (counts) => Object.fromEntries(Object.entries(counts).sort(([a], [b]) => a.localeCompare(b)));
  const next = { version: 3 };
  for (const section of Object.keys(scan)) next[section] = sort(scan[section]);
  writeFileSync(baselinePath, `${JSON.stringify(next, null, 2)}\n`);
  const totals = Object.keys(scan).map(
    (section) => `${Object.values(scan[section]).reduce((sum, n) => sum + n, 0)} ${section}`,
  );
  console.log(`i18n:update: recorded ${totals.join(", ")}`);
  process.exit(0);
}

const LABELS = { files: "text node", attributes: "attribute string", props: "prop string" };
const problemsBySection = Object.entries(scan).map(([section, counts]) => ({
  section,
  lines: Object.entries(counts)
    .filter(([file, count]) => count > (known[section][file] ?? 0))
    .map(([file, count]) => `${file}: ${count} hardcoded ${LABELS[section]}(s), baseline ${known[section][file] ?? 0}`),
}));

const untranslated = enKeys.filter((key) => trKeys.has(key) && en[key] === tr[key]);
const unused = enKeys.filter((key) => !usedKeys.has(key));
const totals = Object.fromEntries(
  Object.entries(scan).map(([section, counts]) => [
    section,
    Object.values(counts).reduce((sum, count) => sum + count, 0),
  ]),
);

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
    totals.files +
    " hardcoded-attrs=" +
    totals.attributes +
    " hardcoded-props=" +
    totals.props,
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
for (const file of Object.keys(scan.files)
  .sort((a, b) => scan.files[b] - scan.files[a])
  .slice(0, 10)) {
  console.log(
    `  hardcoded text: ${file} has ${scan.files[file]} node(s)` +
      (known.files[file] !== undefined ? ` (baseline ${known.files[file]})` : " (not in the baseline)") +
      ` e.g. ${JSON.stringify(samples.get(file) ?? [])}`,
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
  problemsBySection.reduce((sum, entry) => sum + entry.lines.length, 0);
if (check && problems) {
  for (const entry of problemsBySection) {
    for (const line of entry.lines.slice(0, 20)) console.error("  " + line);
  }
  console.error("i18n-audit: the translation tables are out of sync");
  process.exit(1);
}
process.exit(0);
