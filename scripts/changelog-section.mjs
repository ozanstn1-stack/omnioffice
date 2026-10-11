#!/usr/bin/env node
/**
 * Prints the CHANGELOG.md section for one version, so a GitHub release body
 * carries only that version's notes instead of the whole file.
 *
 * Usage:
 *   node scripts/changelog-section.mjs 4.7.1 [CHANGELOG.md]
 *
 * Exits 1 when the version has no section: a release without notes should fail
 * loudly instead of silently publishing the entire changelog.
 */
import { readFileSync } from "node:fs";

const version = (process.argv[2] || "").replace(/^v/, "");
if (!/^\d+\.\d+\.\d+/.test(version)) {
  console.error("changelog-section: pass a version like 4.7.1");
  process.exit(2);
}

const path = process.argv[3] || "CHANGELOG.md";
const lines = readFileSync(path, "utf8").split(/\r?\n/);
const heading = new RegExp("^## \\[" + version.replace(/\./g, "\\.") + "\\]");
const start = lines.findIndex((line) => heading.test(line));
if (start < 0) {
  console.error("changelog-section: no '## [" + version + "]' section in " + path);
  process.exit(1);
}

let end = lines.length;
for (let i = start + 1; i < lines.length; i += 1) {
  if (/^## \[/.test(lines[i])) {
    end = i;
    break;
  }
}

console.log(lines.slice(start, end).join("\n").trimEnd());
