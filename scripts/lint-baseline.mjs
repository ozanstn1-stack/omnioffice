#!/usr/bin/env node
// Category-based ESLint gate.
//
// The previous gate (`eslint . --max-warnings 143`) only froze the *total*
// warning count: a new accessibility violation could be offset by removing an
// unrelated warning elsewhere. This script freezes every rule separately
// (a per-rule baseline), always fails on errors, and fails when a rule that
// was not in the baseline reports anything, so a new category cannot slip in.
//
//   node scripts/lint-baseline.mjs          # enforce the baseline
//   node scripts/lint-baseline.mjs --update # rewrite the baseline deliberately

import { ESLint } from "eslint";
import { readFile, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const baselinePath = join(root, "scripts", "lint-baseline.json");
const update = process.argv.includes("--update");

const eslint = new ESLint({ cwd: root });
const results = await eslint.lintFiles(["."]);
const formatter = await eslint.loadFormatter("stylish");
const output = formatter.format(results);
if (output.trim()) console.log(output);

const errors = [];
const counts = new Map();
for (const result of results) {
  for (const message of result.messages) {
    if (!message.ruleId) continue;
    if (message.severity === 2) {
      errors.push(`${result.filePath}:${message.line}:${message.column} ${message.ruleId}`);
      continue;
    }
    counts.set(message.ruleId, (counts.get(message.ruleId) ?? 0) + 1);
  }
}

if (errors.length > 0) {
  console.error(`\n${errors.length} ESLint error(s) - errors are never allowed:`);
  for (const error of errors.slice(0, 40)) console.error(`  ${error}`);
  process.exit(1);
}

const current = Object.fromEntries([...counts.entries()].sort((a, b) => a[0].localeCompare(b[0])));
const total = [...counts.values()].reduce((sum, count) => sum + count, 0);

if (update) {
  const payload = {
    comment:
      "Per-rule warning baseline for scripts/lint-baseline.mjs. Counts may only go down; refreshing it is a deliberate, reviewed change.",
    updated: new Date().toISOString().slice(0, 10),
    rules: current,
  };
  await writeFile(baselinePath, `${JSON.stringify(payload, null, 2)}\n`, "utf8");
  console.log(`Baseline updated: ${Object.keys(current).length} rules, ${total} warnings.`);
  process.exit(0);
}

const baseline = JSON.parse(await readFile(baselinePath, "utf8")).rules ?? {};
const regressions = [];
for (const [rule, count] of Object.entries(current)) {
  const allowed = baseline[rule] ?? 0;
  if (count > allowed) regressions.push(`${rule}: ${count} recorded (baseline allows ${allowed})`);
}
const removed = Object.keys(baseline).filter((rule) => !(rule in current));

if (regressions.length > 0) {
  console.error("\nESLint warning budget exceeded:");
  for (const regression of regressions) console.error(`  ${regression}`);
  console.error(
    "Fix the new warnings, or update scripts/lint-baseline.json deliberately (node scripts/lint-baseline.mjs --update).",
  );
  process.exit(1);
}

const improved = Object.entries(current).filter(([rule, count]) => count < (baseline[rule] ?? 0));
console.log(`ESLint gate passed: ${total} warnings within the per-rule baseline (${Object.keys(baseline).length} rules tracked).`);
for (const [rule, count] of improved) console.log(`  improved: ${rule}: ${baseline[rule]} -> ${count}`);
if (removed.length > 0) console.log(`  cleared: ${removed.join(", ")}`);
