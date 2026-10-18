/**
 * The "claim = test" rule.
 *
 * RELEASE_READINESS.md is what the release notes are based on, so every
 * `Implemented` claim names the test that proves it with an inline marker:
 *
 *     - **Reader text layer** (issue #118): ... `test:src/lib/reader-text.test.ts#text layer geometry`
 *
 * The marker points at a file plus a test name (the `it(...)`/`test(...)`
 * title for TypeScript, the `fn <name>(` for Rust).
 *
 * Modes:
 *
 *   npm run claims:check
 *     Default: every marker in every `Implemented (this cycle, ...)` section
 *     resolves, and the `package.json` version has its own cycle section.
 *     Only existence is proven here - this runs in the Lint job.
 *
 *   node scripts/claims-audit.mjs --rust-log <file>
 *     Verification: every Rust marker's test appears as passed in a
 *     `cargo test --workspace` log. An ignored or missing test fails.
 *
 *   node scripts/claims-audit.mjs --junit <file>
 *     Verification: every TypeScript marker's test appears as passed in a
 *     vitest JUnit report. A failing or missing test fails.
 *
 * The verification modes run in the jobs that actually execute the suites
 * (Rust matrix on Windows, Frontend), so a claim cannot outlive its test.
 *
 * A claim that genuinely cannot be tested yet must say so explicitly with
 * `(no test: reason)` so the gap is visible in the document; that is required
 * for the current cycle's section.
 */
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const documentPath = join(root, "RELEASE_READINESS.md");
const document = readFileSync(documentPath, "utf8");
const version = JSON.parse(readFileSync(join(root, "package.json"), "utf8")).version;

const args = process.argv.slice(2);
const argumentValue = (flag) => {
  const index = args.indexOf(flag);
  return index >= 0 ? args[index + 1] : null;
};
const rustLogPath = argumentValue("--rust-log");
const junitPath = argumentValue("--junit");

const markerPattern = /`test:([^#`]+)#([^`]+)`/g;
const noTestPattern = /\(no test:[^)]*\)/;

// Every cycle section, oldest to newest in file order (newest first in the
// document). The current version's section is the one that must carry markers.
const sections = [...document.matchAll(/^## Implemented \(this cycle, ([^)]+)\)[^\n]*$/gm)].map((match) => {
  const start = match.index;
  const nextHeading = document.indexOf("\n## ", start + 1);
  return {
    version: match[1].trim(),
    text: document.slice(start, nextHeading < 0 ? document.length : nextHeading),
  };
});

const problems = [];
const current = sections.find((section) => section.version === version);
if (sections.length === 0) {
  problems.push("RELEASE_READINESS.md has no `Implemented (this cycle, ...)` sections");
} else if (!current) {
  problems.push(
    `RELEASE_READINESS.md has no \`Implemented (this cycle, ${version})\` section for the current package.json version`,
  );
}

// A claim is a bullet **block**: `- **…` plus its wrapped continuation lines,
// so a marker on a wrapped line still belongs to the claim.
const claimsOf = (text) =>
  text
    .split(/\n(?=- )/)
    .map((block) => block.trim())
    .filter((block) => block.startsWith("- **"));

let totalClaims = 0;
let totalMarkers = 0;
for (const section of sections) {
  const isCurrent = section === current;
  for (const claim of claimsOf(section.text)) {
    totalClaims += 1;
    const markers = [...claim.matchAll(markerPattern)];
    if (markers.length === 0) {
      if (isCurrent && !noTestPattern.test(claim)) {
        problems.push(`no marker (add \`test:<file>#<name>\` or \`(no test: reason)\`): ${claim.slice(0, 90)}…`);
      }
      continue;
    }
    totalMarkers += markers.length;
    for (const [, file, name] of markers) {
      try {
        const source = readFileSync(join(root, file.trim()), "utf8");
        const needle = file.trim().endsWith(".rs") ? `fn ${name.trim()}(` : `("${name.trim()}"`;
        if (!source.includes(needle)) {
          problems.push(`marker's test is not in ${file.trim()}: ${name.trim()}`);
        }
      } catch {
        problems.push(`marker points at a missing file: ${file.trim()}`);
      }
    }
  }
}

/** Every `test:<file>#<name>` marker across every section. */
function* markers() {
  for (const section of sections) {
    for (const claim of claimsOf(section.text)) {
      for (const match of claim.matchAll(markerPattern)) {
        yield { file: match[1].trim(), name: match[2].trim() };
      }
    }
  }
}

let mode = "resolve";
if (rustLogPath) {
  mode = "rust-log";
  const log = readFileSync(rustLogPath, "utf8");
  const passed = new Set();
  const ignored = new Set();
  for (const rawLine of log.split(/\r?\n/)) {
    const line = rawLine.trim();
    const ok = /^test (.+?) \.\.\. ok$/.exec(line);
    if (ok) passed.add(ok[1].split("::").pop());
    const skip = /^test (.+?) \.\.\. ignored$/.exec(line);
    if (skip) ignored.add(skip[1].split("::").pop());
  }
  if (passed.size === 0) {
    problems.push(`the Rust log ${rustLogPath} contains no passing tests; was the run captured?`);
  }
  for (const marker of markers()) {
    if (!marker.file.endsWith(".rs")) continue;
    if (ignored.has(marker.name)) {
      problems.push(`Rust test is ignored in the run: ${marker.file}#${marker.name}`);
    } else if (!passed.has(marker.name)) {
      problems.push(`Rust test did not pass in the run: ${marker.file}#${marker.name}`);
    }
  }
}

if (junitPath) {
  mode = "junit";
  const xml = readFileSync(junitPath, "utf8");
  const passed = new Set();
  const failed = new Set();
  const testcasePattern = /<testcase\b[^>]*>[\s\S]*?<\/testcase>|<testcase\b[^>]*\/>/g;
  const decode = (value) =>
    value
      .replace(/&quot;/g, '"')
      .replace(/&apos;/g, "'")
      .replace(/&lt;/g, "<")
      .replace(/&gt;/g, ">")
      .replace(/&amp;/g, "&");
  for (const match of xml.matchAll(testcasePattern)) {
    const chunk = match[0];
    const name = /\bname="([^"]*)"/.exec(chunk);
    if (!name) continue;
    // Vitest writes `"<describe> > <test title>"`; the marker names the title.
    const decoded = decode(name[1]);
    const short = decoded.includes(" > ") ? decoded.split(" > ").pop() : decoded;
    if (/<failure|<error/.test(chunk)) {
      failed.add(short);
    } else {
      passed.add(short);
    }
  }
  if (passed.size === 0 && failed.size === 0) {
    problems.push(`the JUnit report ${junitPath} contains no test cases; was the run captured?`);
  }
  for (const marker of markers()) {
    if (marker.file.endsWith(".rs")) continue;
    if (failed.has(marker.name)) {
      problems.push(`TypeScript test failed in the run: ${marker.file}#${marker.name}`);
    } else if (!passed.has(marker.name)) {
      problems.push(`TypeScript test did not run/pass: ${marker.file}#${marker.name}`);
    }
  }
}

if (problems.length > 0) {
  console.error(`claims:check (${mode}): ${problems.length} problem(s):`);
  for (const problem of problems) console.error(`  - ${problem}`);
  process.exit(1);
}
console.log(
  `claims:check (${mode}): ${totalClaims} claim(s) across ${sections.length} cycle section(s), ${totalMarkers} resolved marker(s); current cycle ${version}.`,
);
