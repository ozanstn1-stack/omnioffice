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
 *
 * `extractSection` is exported because the release guard
 * (`scripts/release-guard.mjs`) has to look for exactly the same section: one
 * function, so the guard can never pass a section the release body would not
 * have.
 */
import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

/**
 * The section for `version`, heading included, or `null` when the changelog
 * has none. A section ends at the next `## [` heading (the next release), not
 * at a `###` subheading.
 */
export function extractSection(text, version) {
  const wanted = String(version || "").replace(/^v/, "");
  const lines = text.split(/\r?\n/);
  const heading = new RegExp("^## \\[" + wanted.replace(/\./g, "\\.") + "\\]");
  const start = lines.findIndex((line) => heading.test(line));
  if (start < 0) {
    return null;
  }
  let end = lines.length;
  for (let i = start + 1; i < lines.length; i += 1) {
    if (/^## \[/.test(lines[i])) {
      end = i;
      break;
    }
  }
  return lines.slice(start, end).join("\n").trimEnd();
}

function main(argv) {
  const version = (argv[2] || "").replace(/^v/, "");
  if (!/^\d+\.\d+\.\d+/.test(version)) {
    console.error("changelog-section: pass a version like 4.7.1");
    return 2;
  }
  const path = argv[3] || "CHANGELOG.md";
  const section = extractSection(readFileSync(path, "utf8"), version);
  if (section === null) {
    console.error("changelog-section: no '## [" + version + "]' section in " + path);
    return 1;
  }
  console.log(section);
  return 0;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  process.exit(main(process.argv));
}
