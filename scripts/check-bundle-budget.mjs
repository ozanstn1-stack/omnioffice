#!/usr/bin/env node
// Enforces the production bundle budget. Run after `npx vite build`.
//
// The entry chunk must stay small because every startup pays for it; the
// per-chunk and total budgets catch a screen that stops being tree-shakeable
// or a dependency that creeps into the shared graph.

import { readFileSync, readdirSync } from "node:fs";
import { gzipSync } from "node:zlib";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const dist = join(root, "dist");
const assets = join(dist, "assets");

const html = readFileSync(join(dist, "index.html"), "utf8");
const entryMatch = html.match(/src="[^"]*\/assets\/(index-[^"]+\.js)"/);
if (!entryMatch) {
  console.error("bundle budget: the entry chunk is not referenced from dist/index.html");
  process.exit(1);
}
const entry = entryMatch[1];

const sizes = new Map();
for (const file of readdirSync(assets).filter((name) => name.endsWith(".js"))) {
  sizes.set(file, gzipSync(readFileSync(join(assets, file))).length);
}

const entryKb = (sizes.get(entry) ?? 0) / 1024;
const totalKb = [...sizes.values()].reduce((sum, value) => sum + value, 0) / 1024;
const [largestName, largestBytes] = [...sizes.entries()]
  .filter(([name]) => name !== entry)
  .sort((a, b) => b[1] - a[1])[0] ?? ["", 0];
const largestKb = largestBytes / 1024;

const ENTRY_BUDGET_KB = 60;
const CHUNK_BUDGET_KB = 100;
// The v6.0.0 audit asked for a budget per route, not just one global number: a
// screen can load slowly even while the total stays flat. These caps cover the
// chunks that carry an editor or a large route (measured sizes plus a small
// margin at 6.0.1); anything not listed falls back to CHUNK_BUDGET_KB.
const ROUTE_BUDGETS = {
  react: 72,
  CalcEditor: 52,
  WriterEditor: 35,
  ImpressEditor: 30,
  "calc-functions": 27,
  ToolsScreens: 24,
  Ai: 14,
  Reader: 12,
  PdfStudio: 12,
  Invoices: 10,
};
const baseName = (file) => file.replace(/\.js$/, "").replace(/-[A-Za-z0-9_-]{8}$/, "");
// History: 450 → 485 (4.7.0) → 500 (4.9.0) → 505 (5.0.0) → 515 (5.4.0) →
// 525 → 530 (6.1 routing/proofing wiring, see 6.0.1 notes).
// Reduced to 480 in 6.0.1 (B5): the English and Turkish dictionaries moved
// from JS chunks to JSON assets (`src/lib/i18n/*.json`, fetched at runtime),
// which took ~71 KB gzip out of the total. The cap now tracks the audit's
// <=480 KB target; a future raise needs its own justified PR.
const TOTAL_BUDGET_KB = 480;

const failures = [];
if (!sizes.has(entry)) failures.push(`entry chunk ${entry} is missing from dist/assets`);
if (entryKb > ENTRY_BUDGET_KB)
  failures.push(`entry ${entry} is ${entryKb.toFixed(1)} KB gzip (budget ${ENTRY_BUDGET_KB} KB)`);
if (largestKb > CHUNK_BUDGET_KB)
  failures.push(`largest chunk ${largestName} is ${largestKb.toFixed(1)} KB gzip (budget ${CHUNK_BUDGET_KB} KB)`);
for (const [file, bytes] of sizes) {
  const cap = ROUTE_BUDGETS[baseName(file)];
  if (cap !== undefined && bytes / 1024 > cap) {
    failures.push(`${file} is ${(bytes / 1024).toFixed(1)} KB gzip (${baseName(file)} budget ${cap} KB)`);
  }
}
if (totalKb > TOTAL_BUDGET_KB)
  failures.push(`total JS is ${totalKb.toFixed(1)} KB gzip (budget ${TOTAL_BUDGET_KB} KB)`);

if (failures.length > 0) {
  for (const failure of failures) console.error(`bundle budget: ${failure}`);
  console.error("Split the growing screen with React.lazy or shrink the dependency that got in.");
  process.exit(1);
}
console.log(
  `Bundle budget OK: entry ${entryKb.toFixed(1)} KB, largest ${largestName} ${largestKb.toFixed(1)} KB, total ${totalKb.toFixed(1)} KB gzip.`,
);
