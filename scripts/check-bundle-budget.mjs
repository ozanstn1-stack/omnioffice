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
// Raised from 450 in 4.5.0: the Writer ruler/table tools and the Impress
// rich-text, connectors, crop and sorter features add real UI to the chunks.
// Raised to 485 in 4.7.0 for Belge Asistanı 2.0 (Document Assistant 2.0:
// document chat with citations, invoice extraction, and batch queue).
// Raised to 500 in 4.9.0 for "Resmî Belgeler": the UYAP `.udf` reader/writer,
// the UBL-TR e-Fatura screen with its VAT summary and GİB karekod panel, and
// the Reader's e-Devlet verification button. Each is a lazy route; the total
// moves because they all ship, not because one chunk lost its split.
// Raised to 505 in 5.0.0 for the plugin API v2 (the pdf capability and the
// batch-step manifest surface) and the Writer document comparison; both ride
// the existing lazy routes, so the per-chunk budgets are unchanged.
// Raised to 515 in 5.4.0 for the KVKK package (Document Health dashboard,
// Redaction Certificate generator, and official compliance tools).
// Raised to 525 in 6.0.1: the petition, KVKK and document-health screens that
// shipped in 5.3-5.4 had no routes, so their chunks were tree-shaken out of
// the total. Wiring them up (A2 routing split) makes ~8.5 KB gzip of real,
// already-written UI part of the bundle again.
// Raised to 530 in 6.0.1 for the Writer proofing panel (the offline Turkish
// spell check and the Official Correspondence Regulation checker, the second
// half of the same A2 split). This is the last raise of the cycle: the next
// budget change must be the dedicated reduction PR towards the <=480 KB
// target in the v6.0.1 audit plan.
const TOTAL_BUDGET_KB = 530;

const failures = [];
if (!sizes.has(entry)) failures.push(`entry chunk ${entry} is missing from dist/assets`);
if (entryKb > ENTRY_BUDGET_KB)
  failures.push(`entry ${entry} is ${entryKb.toFixed(1)} KB gzip (budget ${ENTRY_BUDGET_KB} KB)`);
if (largestKb > CHUNK_BUDGET_KB)
  failures.push(`largest chunk ${largestName} is ${largestKb.toFixed(1)} KB gzip (budget ${CHUNK_BUDGET_KB} KB)`);
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
