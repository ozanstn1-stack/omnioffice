#!/usr/bin/env node
/**
 * Automatic local/CI storage cleanup for OmniOffice.
 *
 * Keeps the workspace and runner disks lean during and after builds, tests,
 * packaging, and agent sessions without touching source files, git history,
 * or committed fixtures.
 *
 * Usage:
 *   node scripts/cleanup-storage.mjs [--root <dir>] [--dry-run] [--keep-target]
 *                                    [--deep] [--include-node-modules] [--all-tmp] [--json]
 */
import { existsSync, lstatSync, readdirSync, rmSync, statSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const DEFAULT_WORK_ARTIFACTS = [
  "dist",
  "coverage",
  "junit.xml",
  "rust-tests.log",
  "release-artifacts",
  "e2e/artifacts",
  "src-tauri/gen/android/app/build",
  "src-tauri/gen/android/.gradle",
  "chrome-extension/dist",
  "android-logcat.txt",
  "updater-off.json",
  "release-notes.md",
];

const RUST_TARGETS = ["target", "fuzz/target"];
const DEEP_REPO_TARGETS = ["src-tauri/resources/engines", "src-tauri/resources/engines-android"];
const TMP_PREFIXES = ["pdfsak-", "omnioffice-", "vitest-"];
const TMP_MAX_AGE_MS = 5 * 60 * 1000;

/** Formats byte counts into human-readable units. */
export function formatBytes(bytes) {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let idx = 0;
  while (value >= 1024 && idx < units.length - 1) {
    value /= 1024;
    idx += 1;
  }
  return `${value >= 100 || idx === 0 ? value.toFixed(0) : value.toFixed(1)} ${units[idx]}`;
}

/** Computes recursive disk footprint in bytes (skipping symlinks). */
export function measurePathBytes(targetPath) {
  let total = 0;
  let stat;
  try {
    stat = lstatSync(targetPath);
  } catch {
    return 0;
  }
  if (stat.isSymbolicLink()) return 0;
  if (!stat.isDirectory()) return stat.size;
  let entries = [];
  try {
    entries = readdirSync(targetPath, { withFileTypes: true });
  } catch {
    return 0;
  }
  for (const entry of entries) {
    const child = join(targetPath, entry.name);
    if (entry.isSymbolicLink()) continue;
    if (entry.isDirectory()) {
      total += measurePathBytes(child);
    } else {
      try {
        total += statSync(child).size;
      } catch {
        // Ignore vanished transient files.
      }
    }
  }
  return total;
}

/** Collects safe cleanup targets inside `root`, `tmpDir`, and optional deep caches. */
export function collectCleanupTargets({
  root = process.cwd(),
  keepTarget = false,
  deep = false,
  includeNodeModules = false,
  tmpDir = tmpdir(),
  homeDir = homedir(),
  allTmp = false,
  nowMs = Date.now(),
} = {}) {
  const resolvedRoot = resolve(root);
  const candidates = [...DEFAULT_WORK_ARTIFACTS];
  if (!keepTarget) candidates.push(...RUST_TARGETS);
  if (deep) candidates.push(...DEEP_REPO_TARGETS);
  if (includeNodeModules) candidates.push("node_modules", "chrome-extension/node_modules");

  const targets = [];
  const seen = new Set();

  const pushTarget = (fullPath, label, scope) => {
    const normalized = resolve(fullPath);
    if (seen.has(normalized) || !existsSync(normalized)) return;
    seen.add(normalized);
    targets.push({
      path: normalized,
      label,
      scope,
      bytes: measurePathBytes(normalized),
    });
  };

  for (const rel of candidates) {
    pushTarget(join(resolvedRoot, rel), rel, "repo");
  }

  if (deep && homeDir) {
    pushTarget(join(homeDir, ".npm", "_cacache"), "~/.npm/_cacache", "cache");
    pushTarget(join(homeDir, ".cargo", "registry", "cache"), "~/.cargo/registry/cache", "cache");
  }

  if (tmpDir && existsSync(tmpDir)) {
    let entries = [];
    try {
      entries = readdirSync(tmpDir, { withFileTypes: true });
    } catch {
      entries = [];
    }
    for (const entry of entries) {
      if (!TMP_PREFIXES.some((prefix) => entry.name.startsWith(prefix))) continue;
      const fullPath = join(tmpDir, entry.name);
      try {
        const stat = lstatSync(fullPath);
        if (!allTmp && nowMs - stat.mtimeMs < TMP_MAX_AGE_MS) continue;
      } catch {
        continue;
      }
      pushTarget(fullPath, `tmp/${entry.name}`, "tmp");
    }
  }

  return targets;
}

/** Removes collected targets unless `dryRun` is true, returning freed byte totals. */
export function cleanStorage(options = {}) {
  const dryRun = Boolean(options.dryRun);
  const targets = collectCleanupTargets(options);
  const removed = [];
  const errors = [];
  let freedBytes = 0;

  for (const target of targets) {
    if (dryRun) {
      freedBytes += target.bytes;
      removed.push(target);
      continue;
    }
    try {
      rmSync(target.path, { recursive: true, force: true });
      freedBytes += target.bytes;
      removed.push(target);
    } catch (error) {
      errors.push({ path: target.path, label: target.label, error: String(error) });
    }
  }

  return {
    dryRun,
    removedCount: removed.length,
    freedBytes,
    entries: removed,
    errors,
  };
}

function parseArgs(argv) {
  const options = {
    root: join(import.meta.dirname ?? process.cwd(), ".."),
    dryRun: false,
    keepTarget: false,
    deep: false,
    includeNodeModules: false,
    allTmp: false,
    json: false,
  };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--dry-run") options.dryRun = true;
    else if (arg === "--keep-target") options.keepTarget = true;
    else if (arg === "--deep") options.deep = true;
    else if (arg === "--include-node-modules") options.includeNodeModules = true;
    else if (arg === "--all-tmp") options.allTmp = true;
    else if (arg === "--json") options.json = true;
    else if (arg === "--root") options.root = resolve(argv[++i] || "");
    else if (arg.startsWith("--root=")) options.root = resolve(arg.slice("--root=".length));
    else return null;
  }
  return options;
}

function main(argv) {
  const options = parseArgs(argv);
  if (!options) {
    console.error(
      "clean:storage: usage: node scripts/cleanup-storage.mjs [--root <dir>] [--dry-run] [--keep-target] [--deep] [--include-node-modules] [--all-tmp] [--json]",
    );
    return 2;
  }
  const result = cleanStorage(options);
  if (options.json) {
    console.log(JSON.stringify(result, null, 2));
  } else {
    const verb = result.dryRun ? "Would free" : "Freed";
    if (result.removedCount === 0) {
      console.log("clean:storage: workspace is already clean (0 B).");
    } else {
      for (const entry of result.entries) {
        console.log(`  - ${entry.label} (${formatBytes(entry.bytes)})`);
      }
      console.log(`clean:storage: ${verb} ${formatBytes(result.freedBytes)} across ${result.removedCount} item(s).`);
    }
    for (const err of result.errors) {
      console.error(`clean:storage: failed to remove ${err.label}: ${err.error}`);
    }
  }
  return result.errors.length > 0 ? 1 : 0;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  process.exit(main(process.argv.slice(2)));
}
