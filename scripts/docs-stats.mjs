#!/usr/bin/env node
// Generates the README's project-stats block from the sources, so the numbers
// cannot drift from the code (v6.0.1 audit, B9).
//
//   node scripts/docs-stats.mjs           # rewrite the block
//   node scripts/docs-stats.mjs --check   # fail when it is stale (CI)
//
// The block lives between `<!-- stats:start -->` and `<!-- stats:end -->`.
// Counts:
//   screens  - ids in src/lib/routes.ts (`route("id", …)` entries)
//   commands - Tauri commands registered in src-tauri/src/lib.rs
//   rust     - #[test]/#[tokio::test] attributes under crates/ and src-tauri/
//   ts files - *.test.ts(x) files under src/
//   ts cases - it()/test() calls under src/ (approximate; vitest is the truth)

import { readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), "..");
const readmePath = path.join(root, "README.md");
const START = "<!-- stats:start -->";
const END = "<!-- stats:end -->";

const read = (file) => readFileSync(path.join(root, file), "utf8");

function walk(directory, accept) {
  const files = [];
  for (const entry of readdirSync(directory)) {
    if (entry === "node_modules" || entry === "target" || entry.startsWith(".")) continue;
    const absolute = path.join(directory, entry);
    if (statSync(absolute).isDirectory()) {
      files.push(...walk(absolute, accept));
    } else if (accept(absolute)) {
      files.push(absolute);
    }
  }
  return files;
}

function countMatches(text, pattern) {
  return (text.match(pattern) ?? []).length;
}

export function collectStats() {
  // Screens come from the route manifest (B1); before it lands, the ScreenId
  // union in nav.ts is the same list.
  let screens;
  try {
    screens = countMatches(read("src/lib/routes.ts"), /^ {2}route\("/gm);
  } catch {
    screens = countMatches(read("src/lib/nav.ts"), /^ {2}\| "/gm);
  }

  // Commands live in several modules (commands, office, pdf_v3, vault, ai…),
  // so count the attributes rather than the handler's `commands::` entries.
  const rustSources = walk(path.join(root, "src-tauri", "src"), (file) => file.endsWith(".rs"));
  const commands = rustSources.reduce(
    (total, file) => total + countMatches(readFileSync(file, "utf8"), /#\[tauri::command\]/g),
    0,
  );

  const allRustFiles = rustSources.concat(walk(path.join(root, "crates"), (file) => file.endsWith(".rs")));
  const rustTests = allRustFiles.reduce(
    (total, file) => total + countMatches(readFileSync(file, "utf8"), /#\[(?:tokio::)?test\]/g),
    0,
  );

  const testFiles = walk(path.join(root, "src"), (file) => /\.test\.(ts|tsx)$/.test(file));
  const tsCases = testFiles.reduce((total, file) => {
    const text = readFileSync(file, "utf8");
    return total + countMatches(text, /\b(?:it|test)\(/g);
  }, 0);

  return { screens, commands, rustTests, tsTestFiles: testFiles.length, tsCases };
}

export function renderBlock(stats) {
  return [
    START,
    "| Project statistics | Count |",
    "| --- | --- |",
    `| Screens | ${stats.screens} |`,
    `| Tauri commands | ${stats.commands} |`,
    `| Rust tests | ${stats.rustTests} |`,
    `| TypeScript test files | ${stats.tsTestFiles} |`,
    `| TypeScript test cases (approx.) | ${stats.tsCases} |`,
    END,
  ].join("\n");
}

function main() {
  const check = process.argv.includes("--check");
  const stats = collectStats();
  const block = renderBlock(stats);
  const readme = readFileSync(readmePath, "utf8");
  const start = readme.indexOf(START);
  const end = readme.indexOf(END);
  if (start < 0 || end < 0 || end < start) {
    console.error(`docs-stats: the README needs a ${START} … ${END} block`);
    process.exit(1);
  }
  const current = readme.slice(start, end + END.length);
  if (current === block) {
    console.log("docs-stats: up to date");
    return;
  }
  if (check) {
    console.error("docs-stats: the README stats block is stale; run `npm run docs:stats`");
    process.exit(1);
  }
  writeFileSync(readmePath, readme.slice(0, start) + block + readme.slice(end + END.length));
  console.log("docs-stats: README stats block updated");
}

const invoked = process.argv[1] ? path.resolve(process.argv[1]) : "";
if (invoked === fileURLToPath(import.meta.url)) {
  main();
}
