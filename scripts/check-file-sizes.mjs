#!/usr/bin/env node
// The line-budget ratchet (v6.0.0 audit, B4).
//
// A file that is already long may not grow: the checked-in baseline is its
// ceiling and `--update` only lowers it (`--allow-raise` is the deliberate
// escape hatch, and the diff then shows exactly which budget moved). Files
// below the floor are not tracked; a new file above `NEW_FILE_LIMIT` fails
// `--check` until it is added through `--update`, so nothing is born at two
// thousand lines without the reviewer seeing the budget entry.
//
// The editors still carry the biggest budgets. The audit's split work moves
// code into the existing `src/office/{writer,calc,impress}/` seams and
// `odf/`, `xlsx/` modules; every split lowers the baseline and the gate makes
// the win permanent.

import { existsSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const baselinePath = join(root, "scripts", "file-size-baseline.json");
const FLOOR = 200;
const NEW_FILE_LIMIT = 400;
const TRACKED = ["src", "src-tauri/src", "crates"];
const EXTENSIONS = [".ts", ".tsx", ".rs"];

function walk(directory, out = []) {
  for (const entry of readdirSync(directory)) {
    const full = join(directory, entry);
    if (statSync(full).isDirectory()) {
      if (entry === "node_modules" || entry === "target" || entry === "dist") continue;
      walk(full, out);
    } else if (EXTENSIONS.some((extension) => entry.endsWith(extension)) && !entry.includes(".test.")) {
      out.push(full);
    }
  }
  return out;
}

function linesOf(text) {
  if (text.length === 0) return 0;
  const count = text.split("\n").length;
  return text.endsWith("\n") ? count - 1 : count;
}

const files = TRACKED.flatMap((entry) => walk(join(root, entry)));
const current = new Map();
for (const file of files) {
  const lines = linesOf(readFileSync(file, "utf8"));
  if (lines >= FLOOR) current.set(relative(root, file).split("\\").join("/"), lines);
}
const sorted = [...current.entries()].sort(([a], [b]) => a.localeCompare(b));
const total = sorted.reduce((sum, [, lines]) => sum + lines, 0);

const baseline = existsSync(baselinePath)
  ? JSON.parse(readFileSync(baselinePath, "utf8"))
  : { version: 1, floor: FLOOR, files: {} };
const known = baseline.files ?? {};

if (process.argv.includes("--update")) {
  const raises = sorted.filter(([file, lines]) => known[file] !== undefined && lines > known[file]);
  if (raises.length > 0 && !process.argv.includes("--allow-raise")) {
    for (const [file, lines] of raises) {
      console.error(`files:update: ${file} would grow ${known[file]} -> ${lines}`);
    }
    console.error("files:update: a budget may only shrink; pass --allow-raise to move it on purpose.");
    process.exit(1);
  }
  const next = { version: 1, floor: FLOOR, files: Object.fromEntries(sorted) };
  writeFileSync(baselinePath, `${JSON.stringify(next, null, 2)}\n`);
  const added = sorted.filter(([file]) => known[file] === undefined);
  const removed = Object.keys(known).filter((file) => !current.has(file));
  console.log(
    `files:update: ${sorted.length} files tracked (>=${FLOOR} lines), ${total} lines total` +
      (added.length ? `, ${added.length} added` : "") +
      (removed.length ? `, ${removed.length} removed` : ""),
  );
  process.exit(0);
}

const failures = [];
for (const [file, lines] of sorted) {
  const budget = known[file];
  if (budget === undefined) {
    if (lines > NEW_FILE_LIMIT) {
      failures.push(`${file} is a new file of ${lines} lines (new files stay <= ${NEW_FILE_LIMIT})`);
    }
  } else if (lines > budget) {
    failures.push(`${file} grew to ${lines} lines (budget ${budget})`);
  }
}
if (failures.length > 0) {
  for (const failure of failures) console.error(`file size: ${failure}`);
  console.error("file size: split the file along an existing seam, or lower another budget in the same PR.");
  process.exit(1);
}
const stale = Object.keys(known).filter((file) => !current.has(file));
if (stale.length > 0) {
  console.log(
    `file size: ${stale.length} stale baseline entr${stale.length === 1 ? "y" : "ies"} (run npm run files:update)`,
  );
}
const top = [...sorted].sort((a, b) => b[1] - a[1]).slice(0, 5);
console.log(
  `File size OK: ${sorted.length} files tracked (>=${FLOOR} lines), ${total} lines total. ` +
    `Largest: ${top.map(([file, lines]) => `${file} ${lines}`).join(", ")}.`,
);
