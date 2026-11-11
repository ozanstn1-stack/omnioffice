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

// Raised to 61 in 6.4.5 (#316, group 1): the typed command wrappers and the
// generated specta table the command palette needs at startup add about
// 0.2 KB gzip to the entry, which the Linux CI measured at 60.1 KB against
// the old 60 KB cap. The total cap is unchanged.
//
// Raised to 62 in 7.1 (#531 D13): the desk-task palette commands and the
// disabled-reason rows: the catalogue itself is loaded as its own chunk, but
// the registry wiring and the row rendering the palette needs at startup push
// the entry to 61.3 KB gzip against the old 61 KB cap.
const ENTRY_BUDGET_KB = 62;
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
  templates: 14,
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
// Raised to 490 in 6.4 (Phase C, #341): the profession desks, the pre-send
// check, the notification deadline calculator, the period audit and the
// phone bottom bar add about 12 KB gzip in total. Each one is a lazy chunk or
// lazy-mounted dialog; the entry stays under its 60 KB cap.
// Raised to 500 in 6.5 (#341 C3 and C5): the evidence bundle screen and the
// official letter wizard add about 7 KB gzip, both as lazy chunks; the entry
// is unchanged (59.5 KB). That leaves room for C13 before the next review.
// Raised to 505 in 6.7 (#508/#509 C6): the template field form and the person
// card dialog (CSV/vCard import/export, encrypted-card UI) add about 4 KB gzip
// as a lazy route and a lazy dialog; the entry is unchanged.
// Raised to 510 in 6.8 (#341 C13): the model-level office diff engine and its
// viewer add about 3 KB gzip, used by the Compare route and the version-history
// dialog (both lazy); the entry chunk is unchanged.
// Raised to 515 in 6.9 (#341 C7): the Turkish task pack engine and the aiPacks
// route (four packs + verification, both lazy) add about 5 KB gzip; the entry
// chunk is unchanged.
// Raised to 520 in 6.11 (#341 C14): the LAN sync card and its plan list run
// on the Sync screen; the crypto and transport live in Rust.
// Raised to 524 in 7.31 (new interface): the hub sidebar, the New menu and
// the Home create row add about 3 KB gzip; the sidebar is its own lazy chunk,
// so the entry chunk stays inside its budget.
// Raised to 526 in 7.32 (Tools hub): the shared tool catalogue chunk
// (lib/tool-cards.tsx, ~2.3 KB gzip) gives fourteen more screens a card and
// the hub its tiles; the entry chunk is unchanged.
// Raised to 530 in 7.32.1 (office launchers out of App.tsx): the launcher is
// now a lazy screen, so useOfficeSession, save-destination, share and the
// recent table left the entry chunk (61.7 -> 58.1 KB) for small shared
// chunks; their per-chunk gzip overhead adds ~3.9 KB to the total.
// Raised to 532 in the 7.35 editor split (3.3a): Writer's Find & replace and
// four side panes moved into office/writer/panels; the props they now take
// keep their names under minification (~0.6 KB gzip in the Writer chunk).
// Raised to 533 in 3.5 (the command palette and global search load on first
// open): the entry chunk drops 58.7 -> 57.6 KB, the new palette chunk (1.8 KB)
// and its search icon add ~1 KB of per-chunk overhead to the total.
// Raised to 534 in 4.1a (Calc chart properties): editing a chart's title, kind,
// legend and ranges adds ~0.8 KB gzip to the CalcEditor chunk (50.7 / 52 KB).
// Raised to 535 in 4.1b (move and resize Calc charts): the drag layer and the
// measured head and legend add ~0.8 KB gzip to the CalcEditor chunk (51.5 / 52 KB).
// Raised to 536 in 4.1c (stacked Calc charts, value labels, axis titles): the
// cartesian plot and the three new chart properties add ~0.6 KB gzip; the
// CalcEditor chunk is 52,155 bytes (50.9 / 52 KB; the two notes above counted
// thousands of bytes, not KB).
const TOTAL_BUDGET_KB = 536;

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
