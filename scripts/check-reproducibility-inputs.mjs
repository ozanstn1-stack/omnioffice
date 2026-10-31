#!/usr/bin/env node
// Reproducibility inputs gate (D11).
//
// A release can only be reproduced by a third party if the inputs are pinned
// and visible: committed lockfiles, one workspace version, SHA-pinned actions,
// `npm ci`, and archives built from a sorted file list. This script pins the
// *inputs* the repository controls. It does not claim the artifacts are
// bit-reproducible — `docs/reproducibility.md` lists what still differs (ZIP
// timestamps/order, NSIS, AppImage, Android signing) and how to verify.
//
// Run with `npm run repro:check`; the vitest suite runs it too.
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = process.env.REPRO_ROOT ?? join(dirname(fileURLToPath(import.meta.url)), "..");
const read = (relative) => readFileSync(join(root, relative), "utf8");
const failures = [];
const check = (condition, message) => {
  if (!condition) failures.push(message);
};

// 1. Lockfiles are committed and not ignored.
check(existsSync(join(root, "Cargo.lock")), "Cargo.lock is missing");
check(existsSync(join(root, "package-lock.json")), "package-lock.json is missing");
const gitignore = read(".gitignore");
check(!/^Cargo\.lock$/m.test(gitignore), "Cargo.lock is git-ignored");
check(!/^package-lock\.json$/m.test(gitignore), "package-lock.json is git-ignored");

// 2. One workspace version: the crates must inherit it, not carry their own.
const workspaceVersion = /^version\s*=\s*"([^"]+)"/m.exec(read("Cargo.toml"))?.[1] ?? "";
check(workspaceVersion !== "", "the workspace Cargo.toml has no version");
const crates = readdirSync(join(root, "crates"), { withFileTypes: true })
  .filter((entry) => entry.isDirectory())
  .map((entry) => `crates/${entry.name}/Cargo.toml`)
  .filter((relative) => existsSync(join(root, relative)));
for (const relative of ["src-tauri/Cargo.toml", ...crates]) {
  const manifest = read(relative);
  check(/version\.workspace\s*=\s*true/.test(manifest), `${relative} does not inherit the workspace version`);
  check(!/^version\s*=\s*"/m.test(manifest), `${relative} carries its own version`);
}

// 3. Every GitHub action is pinned to a commit, and builds use the committed lockfiles.
const workflows = readdirSync(join(root, ".github", "workflows")).filter((name) => name.endsWith(".yml"));
for (const name of workflows) {
  const text = read(`.github/workflows/${name}`);
  for (const match of text.matchAll(/uses:\s*([^\s#]+)/g)) {
    const action = match[1];
    if (action.startsWith("./")) continue;
    check(/@[0-9a-f]{40}$/.test(action), `${name}: action ${action} is not pinned to a commit SHA`);
  }
  check(!/npm install(?!\s)/.test(text), `${name}: use npm ci instead of npm install`);
  // Every `cargo build` line must carry --locked: a lockfile that would change
  // during a release build must fail the job, not silently resolve new versions.
  for (const line of text.split(/\r?\n/)) {
    const command = line.trim();
    if (!command.startsWith("#") && /cargo\s+build\b/.test(command) && !/--locked/.test(command)) {
      failures.push(`${name}: cargo build without --locked: ${line.trim()}`);
    }
  }
}

// 4. Archives are built from a sorted list, not from directory enumeration order.
const packageScript = read("scripts/package.ps1");
check(/Sort-Object/.test(packageScript), "scripts/package.ps1 does not sort the files it archives");

// 5. The report exists and explains what is and is not reproducible.
const report = read("docs/reproducibility.md");
for (const heading of ["## Inputs that are pinned", "## What is not reproducible yet", "## How to verify"]) {
  check(report.includes(heading), `docs/reproducibility.md is missing "${heading}"`);
}
check(/Cargo\.lock/.test(report) && /package-lock\.json/.test(report), "the report does not list the lockfiles");
check(/Sha|SHA-256|checksum/i.test(report), "the report does not explain how a binary is compared");

if (failures.length > 0) {
  console.error("reproducibility inputs: FAILED");
  for (const failure of failures) console.error(`  - ${failure}`);
  process.exit(1);
}
console.log(
  `reproducibility inputs: OK (${workspaceVersion} workspace, ${crates.length + 1} manifests, ${workflows.length} workflows, sorted archives, report present)`,
);
