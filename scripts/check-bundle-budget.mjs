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

const ENTRY_BUDGET_KB = 180;
const CHUNK_BUDGET_KB = 100;
const TOTAL_BUDGET_KB = 450;

const failures = [];
if (!sizes.has(entry)) failures.push(`entry chunk ${entry} is missing from dist/assets`);
if (entryKb > ENTRY_BUDGET_KB) failures.push(`entry ${entry} is ${entryKb.toFixed(1)} KB gzip (budget ${ENTRY_BUDGET_KB} KB)`);
if (largestKb > CHUNK_BUDGET_KB) failures.push(`largest chunk ${largestName} is ${largestKb.toFixed(1)} KB gzip (budget ${CHUNK_BUDGET_KB} KB)`);
if (totalKb > TOTAL_BUDGET_KB) failures.push(`total JS is ${totalKb.toFixed(1)} KB gzip (budget ${TOTAL_BUDGET_KB} KB)`);

if (failures.length > 0) {
  for (const failure of failures) console.error(`bundle budget: ${failure}`);
  console.error("Split the growing screen with React.lazy or shrink the dependency that got in.");
  process.exit(1);
}
console.log(
  `Bundle budget OK: entry ${entryKb.toFixed(1)} KB, largest ${largestName} ${largestKb.toFixed(1)} KB, total ${totalKb.toFixed(1)} KB gzip.`,
);
