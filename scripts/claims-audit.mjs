/**
 * The "claim = test" rule.
 *
 * RELEASE_READINESS.md is what the release notes are based on, so every
 * `Implemented` claim names the test that proves it with an inline marker:
 *
 *     - **Reader text layer** (issue #118): ... `test:src/lib/reader-text.test.ts#text layer geometry`
 *
 * The marker points at a file plus a test name (the `it(...)`/`test(...)`
 * title for TypeScript, the `fn <name>(` for Rust). A claim whose test was
 * deleted or renamed fails here instead of shipping as an unverified promise.
 *
 *   npm run claims:check
 *
 * A claim that genuinely cannot be tested yet must say so explicitly with
 * `(no test: reason)` so the gap is visible in the document.
 */
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const documentPath = join(root, "RELEASE_READINESS.md");
const document = readFileSync(documentPath, "utf8");

// All sections from the latest cycle down to 5.0.1 are enforced; older
// sections (5.0.0 and earlier) are the historical record and are not re-tagged.
const cycleStart = document.search(/^## Implemented \(this cycle,/m);
const cycleEnd = document.search(/^## Implemented \(this cycle, 5\.0\.0\)/m);
if (cycleStart < 0 || cycleEnd < 0 || cycleEnd <= cycleStart) {
  console.error("claims:check: could not locate the current cycle's section.");
  process.exit(1);
}
const section = document.slice(cycleStart, cycleEnd);

// A claim is a bullet **block**: `- **…` plus its wrapped continuation lines,
// so a marker on a wrapped line still belongs to the claim.
const claims = section
  .split(/\n(?=- )/)
  .map((block) => block.trim())
  .filter((block) => block.startsWith("- **"));

const markerPattern = /`test:([^#`]+)#([^`]+)`/g;
const noTestPattern = /\(no test:[^)]*\)/;

const problems = [];
let tagged = 0;

for (const claim of claims) {
  const markers = [...claim.matchAll(markerPattern)];
  if (markers.length === 0) {
    if (!noTestPattern.test(claim)) {
      problems.push(`no marker (add \`test:<file>#<name>\` or \`(no test: reason)\`): ${claim.slice(0, 90)}…`);
    }
    continue;
  }
  tagged += 1;
  for (const [, file, name] of markers) {
    const trimmedFile = file.trim();
    const trimmedName = name.trim();
    let source;
    try {
      source = readFileSync(join(root, trimmedFile), "utf8");
    } catch {
      problems.push(`marker points at a missing file: ${trimmedFile}`);
      continue;
    }
    if (trimmedFile.endsWith(".kt")) {
      if (!source.includes(`fun ${trimmedName}(`) && !source.includes(`fun \`${trimmedName}\`(`)) {
        problems.push(`marker's test is not in ${trimmedFile}: ${trimmedName}`);
      }
      continue;
    }
    const needle = trimmedFile.endsWith(".rs") ? `fn ${trimmedName}(` : `("${trimmedName}"`;
    if (!source.includes(needle)) {
      problems.push(`marker's test is not in ${trimmedFile}: ${trimmedName}`);
    }
  }
}

if (problems.length > 0) {
  console.error(`claims:check: ${problems.length} problem(s):`);
  for (const problem of problems) console.error(`  - ${problem}`);
  process.exit(1);
}
console.log(`claims:check: ${claims.length} claim(s) in the 5.0.1–6.0.0 cycles, ${tagged} with a resolved test marker.`);
