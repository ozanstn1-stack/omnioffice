#!/usr/bin/env node
// Generates the fastlane changelog files the F-Droid ecosystem reads, from the
// one source of release notes: CHANGELOG.md.
//
//   node scripts/fastlane-changelogs.mjs          # write en-US changelogs
//   node scripts/fastlane-changelogs.mjs --check  # fail when they drift
//
// F-Droid and IzzyOnDroid show `fastlane/metadata/android/<locale>/changelogs/
// <versionCode>.txt`; the file name is the integer version code, not the
// version name. The version code formula is the one `auto-tag.yml` enforces:
// major * 1_000_000 + minor * 1_000 + patch.
//
// The English body comes from CHANGELOG.md. Other locales are hand-maintained
// (the listing text is translated; the changelog body follows the English
// changelog), so this script only checks that the current version has a file
// there, never overwrites one.

import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const SOURCE_LOCALE = "en-US";
// The catalogue metadata started with 6.6.0; older codes are not backfilled to
// keep the tree small. Bump this when old entries are pruned.
const SINCE_VERSION = "6.6.0";
// F-Droid truncates changelogs longer than this.
const MAX_CHANGELOG_CHARS = 500;

const check = process.argv.includes("--check");
const metadataDir = join(root, "fastlane", "metadata", "android");
const changelog = readFileSync(join(root, "CHANGELOG.md"), "utf8");

function versionCode(version) {
  const [major, minor, patch] = version.split(".").map((part) => Number.parseInt(part, 10));
  if (!Number.isFinite(major) || !Number.isFinite(minor) || !Number.isFinite(patch)) return null;
  return major * 1_000_000 + minor * 1_000 + patch;
}

function compareVersions(left, right) {
  const a = left.split(".").map(Number);
  const b = right.split(".").map(Number);
  for (let index = 0; index < 3; index += 1) {
    if (a[index] !== b[index]) return a[index] - b[index];
  }
  return 0;
}

function cleanLine(line) {
  return line
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1") // links -> text
    .replace(/`([^`]*)`/g, "$1") // code spans
    .replace(/\*\*([^*]+)\*\*/g, "$1") // bold
    .replace(/\*([^*]+)\*/g, "$1") // italic
    .replace(/\s+/g, " ")
    .trim();
}

/** Joins the wrapped lines of every bullet so a sentence is never cut. */
function bulletsOf(body) {
  const bullets = [];
  let current = null;
  for (const line of body.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (/^-\s+/.test(trimmed)) {
      if (current) bullets.push(current);
      current = trimmed.replace(/^-\s+/, "");
    } else if (current && trimmed !== "" && !trimmed.startsWith("#") && !trimmed.startsWith("|")) {
      current += ` ${trimmed}`;
    } else if (current && trimmed === "") {
      bullets.push(current);
      current = null;
    }
  }
  if (current) bullets.push(current);
  return bullets.map(cleanLine).filter(Boolean);
}

/** One plain-text changelog per version: "X.Y.Z: first bullet. Second bullet." */
function changelogText(version, body) {
  const bullets = bulletsOf(body);
  const sentences = (bullets.length > 0 ? bullets : [cleanLine(body.split(/\r?\n/)[0] ?? "")])
    .map((bullet) => bullet.replace(/[.;:]\s*$/, ""))
    .filter(Boolean)
    .map((bullet) => `${bullet}.`);
  let composed = `${version}:`;
  for (const sentence of sentences) {
    // Stop at a sentence boundary; a changelog never ends mid-word.
    if (composed.length + 1 + sentence.length > MAX_CHANGELOG_CHARS) {
      if (composed === `${version}:`) {
        // A single long first sentence is cut at a word boundary.
        const room = MAX_CHANGELOG_CHARS - composed.length - 2;
        composed += ` ${sentence.slice(0, room).replace(/\s+\S*$/, "")}…`;
      } else {
        composed = `${composed.replace(/\s+$/, "")}…`;
      }
      break;
    }
    composed += ` ${sentence}`;
  }
  if (!composed.endsWith("…") && !/[.!?]$/.test(composed)) composed += ".";
  return `${composed}\n`;
}

const sections = [];
const headingPattern = /^## \[(\d+\.\d+\.\d+)\]\s*-\s*(.*)$/gm;
const matches = [...changelog.matchAll(headingPattern)];
matches.forEach((heading, index) => {
  const start = heading.index + heading[0].length;
  const end = index + 1 < matches.length ? matches[index + 1].index : changelog.length;
  sections.push({ version: heading[1], body: changelog.slice(start, end) });
});

const wanted = sections
  .filter((section) => compareVersions(section.version, SINCE_VERSION) >= 0)
  .map((section) => ({ ...section, code: versionCode(section.version) }))
  .filter((section) => section.code !== null);

if (wanted.length === 0) {
  console.error("fastlane-changelogs: no CHANGELOG sections found");
  process.exit(2);
}

const problems = [];
const sourceDir = join(metadataDir, SOURCE_LOCALE, "changelogs");
for (const section of wanted) {
  const path = join(sourceDir, `${section.code}.txt`);
  const expected = changelogText(section.version, section.body);
  if (check) {
    const actual = existsSync(path) ? readFileSync(path, "utf8") : null;
    if (actual !== expected) problems.push(`${SOURCE_LOCALE}/${section.code}.txt is ${actual === null ? "missing" : "stale"}`);
  } else {
    writeFileSync(path, expected);
  }
}

const locales = readdirSync(metadataDir, { withFileTypes: true })
  .filter((entry) => entry.isDirectory())
  .map((entry) => entry.name);
const latest = wanted[wanted.length - 1];
for (const locale of locales) {
  if (locale === SOURCE_LOCALE) continue;
  const path = join(metadataDir, locale, "changelogs", `${latest.code}.txt`);
  if (!existsSync(path)) problems.push(`${locale}/changelogs/${latest.code}.txt is missing (hand-maintained)`);
}

if (problems.length > 0) {
  console.error(`fastlane-changelogs: ${problems.length} problem(s):`);
  for (const problem of problems) console.error(`  - ${problem}`);
  process.exit(1);
}
console.log(
  `fastlane-changelogs: ${check ? "checked" : "wrote"} ${wanted.length} changelog(s) for ${SOURCE_LOCALE}; ` +
    `${locales.length - 1} translated locale(s) carry the current version.`,
);
