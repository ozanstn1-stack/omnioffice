#!/usr/bin/env node
/**
 * Formatting gate for the repository.
 *
 * Why this is not a plain `cargo fmt --check` / `prettier --check`:
 * neither rustfmt nor Prettier has ever run over this code base. A repo-wide
 * check reports 93 Rust files and 96 frontend files on day one, so the gate is
 * staged instead:
 *
 *   * blocked: files ADDED by the change must be formatted;
 *   * reported: repo-wide drift is printed so the backlog stays visible and
 *     can only shrink over time.
 *
 * Once `npm run format` + `npm run fmt:rust` have been run once (and the
 * result committed on its own), passing --all-changed - or a plain --repo
 * call - turns this into the strict version.
 *
 * Usage:
 *   node scripts/check-format.mjs frontend [--repo] [--all-changed] [--base=<ref>]
 *   node scripts/check-format.mjs rust     [--repo] [--all-changed] [--base=<ref>]
 *
 * The base defaults to HEAD~1; locally, with no --base and no --repo, the
 * working tree is compared against HEAD.
 */
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import path from "node:path";

const argv = process.argv.slice(2);
const mode = argv.find((arg) => arg === "rust" || arg === "frontend");
if (!mode) {
  console.error("usage: check-format.mjs <rust|frontend> [--repo] [--all-changed] [--base=<ref>]");
  process.exit(2);
}
const repoWide = argv.includes("--repo");
const allChanged = argv.includes("--all-changed");
const baseArg = argv.find((arg) => arg.startsWith("--base="));
const base = baseArg ? baseArg.slice("--base=".length) : process.env.FORMAT_BASE || null;

const root = execFileSync("git", ["rev-parse", "--show-toplevel"], { encoding: "utf8" }).trim();

function git(args) {
  return execFileSync("git", args, { cwd: root, encoding: "utf8" }).trim();
}

/** Files this change adds (or, with --all-changed, adds/modifies/renames). */
function changedFiles() {
  const filter = allChanged ? "ACMR" : "A";
  const attempts = base ? [[base + "...HEAD"], [base, "HEAD"]] : [["HEAD"], ["HEAD~1", "HEAD"]];
  for (const range of attempts) {
    try {
      const out = git(["diff", "--name-only", "--diff-filter=" + filter, ...range]);
      return out ? out.split(/\r?\n/).filter(Boolean) : [];
    } catch {
      // Shallow clone or unknown ref: try the next strategy.
    }
  }
  return null;
}

function repoFiles() {
  const out = git(["ls-files"]);
  return out ? out.split(/\r?\n/).filter(Boolean) : [];
}

/** Frontend: the same globs package.json formats. Rust: workspace sources. */
function wanted(file) {
  const normalised = file.split(path.sep).join("/");
  if (mode === "rust") return normalised.endsWith(".rs") && !normalised.startsWith("target/");
  // i18n.ts is one entry per line on purpose: the mojibake test parses it line
  // by line, and a wrapped value would silently leave the scan.
  return /^src\/.*\.(ts|tsx|css)$/.test(normalised) && !normalised.endsWith("src/lib/i18n.ts");
}

function run(command, args) {
  try {
    execFileSync(command, args, { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
    return true;
  } catch {
    return false;
  }
}

function unformatted(files) {
  if (files.length === 0) return [];
  let check;
  if (mode === "rust") {
    check = (file) => run("rustfmt", ["--edition", "2021", "--check", file]);
  } else {
    // Run the local CLI with the running Node. `npx` is a .cmd shim on Windows
    // and execFileSync cannot spawn it without a shell (ENOENT), which used to
    // report every file as unformatted on that platform.
    const cli = path.join(root, "node_modules", "prettier", "bin", "prettier.cjs");
    if (!existsSync(cli)) {
      console.error("prettier is not installed (expected " + cli + "); run npm ci first.");
      process.exit(2);
    }
    check = (file) => run(process.execPath, [cli, "--check", file]);
  }
  return files.filter((file) => !check(file));
}

const fixCommand = mode === "rust" ? "npm run fmt:rust" : "npm run format";

if (repoWide) {
  const files = repoFiles().filter(wanted).filter((file) => existsSync(path.join(root, file)));
  const drift = unformatted(files);
  console.log(mode + ": " + drift.length + " of " + files.length + " tracked files are not formatted");
  if (drift.length) {
    console.log("  (informational while the backlog is burned down; run '" + fixCommand + "' to clear it)");
    for (const file of drift.slice(0, 15)) console.log("  - " + file);
    if (drift.length > 15) console.log("  ... and " + (drift.length - 15) + " more");
  }
  process.exit(0);
}

const changed = changedFiles();
if (changed === null) {
  console.log(mode + ": the base revision is unavailable (shallow clone?); skipping the formatting gate");
  process.exit(0);
}

const files = changed.filter(wanted).filter((file) => existsSync(path.join(root, file)));
if (files.length === 0) {
  console.log(mode + ": no " + (allChanged ? "changed" : "added") + " files to check");
  process.exit(0);
}

const drift = unformatted(files);
if (drift.length === 0) {
  console.log(mode + ": " + files.length + " " + (allChanged ? "changed" : "added") + " file(s) are formatted");
  process.exit(0);
}

console.error(mode + ": these files are not formatted:");
for (const file of drift) console.error("  - " + file);
console.error("\nRun '" + fixCommand + "' (whole repository) or format just these files, then commit again.");
process.exit(1);
