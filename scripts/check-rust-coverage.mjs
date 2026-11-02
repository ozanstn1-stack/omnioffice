#!/usr/bin/env node
// Enforces the Rust coverage floors (E4.1, the ratchet half).
//
// The `rust-coverage` job measures the library crates with `cargo llvm-cov` and
// writes `coverage-summary.txt` (`cargo llvm-cov report --summary-only`). This
// script reads the TOTAL row and compares it with scripts/coverage-baseline.json.
//
// The floors are a ratchet: a measurement may raise them with `--update`, never
// lower them, so a regression (a lost test, a dead path) fails the job instead
// of quietly shrinking the number. The first baseline was set one point under
// the 2026-11-02 measurement so run-to-run noise cannot redden a green PR.
//
// Usage:
//   node scripts/check-rust-coverage.mjs <coverage-summary.txt>
//   node scripts/check-rust-coverage.mjs <coverage-summary.txt> --update
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const baselinePath = fileURLToPath(new URL("./coverage-baseline.json", import.meta.url));
const METRICS = ["regions", "functions", "lines"];

const [summaryPath, ...flags] = process.argv.slice(2);
if (!summaryPath) {
  console.error("usage: node scripts/check-rust-coverage.mjs <coverage-summary.txt> [--update]");
  process.exit(2);
}
if (!existsSync(summaryPath)) {
  console.error(`coverage summary not found: ${summaryPath}`);
  process.exit(2);
}

/** Percentages of the TOTAL row, in the order llvm-cov prints them. */
function parseTotal(summary) {
  const total = summary.split(/\r?\n/).find((line) => line.trim().startsWith("TOTAL"));
  if (!total) return null;
  const percentages = [...total.matchAll(/(\d+(?:\.\d+)?)%/g)].map((match) => Number(match[1]));
  // Regions cover, functions executed, lines cover. A newer llvm-cov may append
  // a branches column; the first three positions have stayed stable.
  if (percentages.length < 3) return null;
  return { regions: percentages[0], functions: percentages[1], lines: percentages[2] };
}

const baseline = existsSync(baselinePath) ? JSON.parse(readFileSync(baselinePath, "utf8")) : { version: 1, rust: {} };

const measured = parseTotal(readFileSync(summaryPath, "utf8"));
if (!measured) {
  console.error(`could not read the TOTAL row from ${summaryPath}`);
  process.exit(2);
}

const floors = baseline.rust ?? {};

if (flags.includes("--update")) {
  const lowered = METRICS.filter((metric) => floors[metric] !== undefined && measured[metric] < floors[metric]);
  if (lowered.length) {
    for (const metric of lowered) {
      console.error(`coverage:update: ${metric} would drop ${floors[metric]} -> ${measured[metric]}`);
    }
    console.error("coverage:update: the floors may only rise; subtract a test and this refuses to move.");
    process.exit(1);
  }
  const next = { ...baseline, version: 1, rust: {} };
  for (const metric of METRICS) {
    next.rust[metric] = Math.floor(Math.min(measured[metric], 100));
  }
  writeFileSync(baselinePath, `${JSON.stringify(next, null, 2)}\n`);
  console.log(`coverage:update: floors ${METRICS.map((metric) => `${metric} ${next.rust[metric]}`).join(", ")}`);
  process.exit(0);
}

const problems = [];
for (const metric of METRICS) {
  const floor = floors[metric];
  if (floor === undefined) continue;
  const value = measured[metric];
  if (value < floor) problems.push(`${metric} ${value.toFixed(2)}% is below the floor ${floor}%`);
}

console.log(
  `coverage: ${METRICS.map((metric) => `${metric} ${measured[metric].toFixed(2)}% (floor ${floors[metric] ?? "-"}%)`).join(", ")}`,
);
if (problems.length) {
  for (const problem of problems) console.error(`  ${problem}`);
  console.error(
    "coverage: the measurement dropped under the ratchet; add the missing tests instead of lowering the floor.",
  );
  process.exit(1);
}
process.exit(0);
