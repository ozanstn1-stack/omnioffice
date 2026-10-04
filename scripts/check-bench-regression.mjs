#!/usr/bin/env node
// Compares criterion benchmark means against a stored baseline and fails on a
// relative regression.
//
// The CI bench job (master only) restores the previous run's baseline file,
// runs the benchmarks, calls this script, and saves the updated baseline only
// when the comparison passed. Locally the baseline is created on first run:
//
//   cargo bench -p officecore --bench parse -p pdfcore --bench pdf_ops
//   node scripts/check-bench-regression.mjs
//
// Thresholds: 15 % mean regression fails, 7.5 % warns. GitHub runners are not
// a fixed-performance machine (neighbour noise, CPU model drift), so the gate
// is deliberately coarse: it catches an order-of-magnitude or structural
// regression, not a 5 % wobble. Use the criterion report artifact for the
// detailed trend.
//
// The criterion directory layout is `<group>/<benchmark>/new/estimates.json`
// (the group level is optional), so the tree is walked and each file ID is its
// path relative to target/criterion.

import { readFileSync, readdirSync, statSync, writeFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const criterionDir = join(root, "target", "criterion");
const defaultBaseline = join(root, ".bench-baseline.json");

function usage() {
  console.error(
    "usage: node scripts/check-bench-regression.mjs [--baseline <file>] [--threshold <percent>]",
  );
  process.exit(2);
}

const args = process.argv.slice(2);
let baselinePath = defaultBaseline;
let failPercent = 15;
for (let index = 0; index < args.length; index += 1) {
  if (args[index] === "--baseline" && args[index + 1]) {
    baselinePath = args[index + 1];
    index += 1;
  } else if (args[index] === "--threshold" && args[index + 1]) {
    failPercent = Number(args[index + 1]);
    if (!Number.isFinite(failPercent) || failPercent <= 0) usage();
    index += 1;
  } else {
    usage();
  }
}
const warnPercent = failPercent / 2;

function walkEstimates(dir) {
  const found = [];
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (!statSync(path).isDirectory()) continue;
    if (entry === "new" && existsSync(join(path, "estimates.json"))) {
      found.push(join(path, "estimates.json"));
    } else if (entry !== "report" && entry !== "base" && entry !== "change") {
      found.push(...walkEstimates(path));
    }
  }
  return found;
}

function meanOf(file) {
  const estimates = JSON.parse(readFileSync(file, "utf8"));
  const mean = estimates?.mean?.point_estimate;
  if (typeof mean !== "number" || !Number.isFinite(mean)) return null;
  return mean;
}

if (!existsSync(criterionDir)) {
  console.error(`benchmark regression: ${criterionDir} is missing; run cargo bench first.`);
  process.exit(1);
}

const current = new Map();
for (const file of walkEstimates(criterionDir)) {
  const id = relative(criterionDir, dirname(dirname(file))).split(sep).join("/");
  const mean = meanOf(file);
  if (mean !== null) current.set(id, mean);
}

if (current.size === 0) {
  console.error("benchmark regression: no criterion estimates were found.");
  process.exit(1);
}

if (!existsSync(baselinePath)) {
  mkdirSync(dirname(baselinePath), { recursive: true });
  writeFileSync(baselinePath, `${JSON.stringify(Object.fromEntries([...current].sort()), null, 2)}\n`);
  console.log(
    `benchmark regression: no baseline at ${baselinePath}; stored ${current.size} benchmarks from this run.`,
  );
  process.exit(0);
}

const baseline = JSON.parse(readFileSync(baselinePath, "utf8"));
const failures = [];
const warnings = [];
const notes = [];

for (const [id, mean] of current) {
  const previous = baseline[id];
  if (typeof previous !== "number" || previous <= 0) {
    notes.push(`new benchmark ${id}`);
    continue;
  }
  const change = ((mean - previous) / previous) * 100;
  const label = `${id} ${previous.toFixed(0)} ns -> ${mean.toFixed(0)} ns (${change >= 0 ? "+" : ""}${change.toFixed(1)} %)`;
  if (change > failPercent) failures.push(label);
  else if (change > warnPercent) warnings.push(label);
}
for (const id of Object.keys(baseline)) {
  if (!current.has(id)) notes.push(`benchmark disappeared: ${id}`);
}

for (const warning of warnings) console.warn(`benchmark warning: ${warning}`);
for (const note of notes) console.log(`benchmark note: ${note}`);

if (failures.length > 0) {
  for (const failure of failures) console.error(`benchmark regression: ${failure}`);
  console.error(
    `A mean over ${failPercent} % slower than the stored baseline failed the gate. ` +
      "Fix the regression or, when the slowdown is understood and accepted, delete the " +
      "baseline cache entry to re-baseline deliberately.",
  );
  process.exit(1);
}

writeFileSync(baselinePath, `${JSON.stringify(Object.fromEntries([...current].sort()), null, 2)}\n`);
console.log(
  `Benchmark regression check OK: ${current.size} benchmarks within ${failPercent} % of the baseline ` +
    `(${warnings.length} warning${warnings.length === 1 ? "" : "s"}).`,
);
