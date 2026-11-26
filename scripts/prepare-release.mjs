#!/usr/bin/env node
/**
 * Single-command release version bumper for OmniOffice.
 *
 * Keeps every version file, workspace Cargo.lock entry, Android versionCode,
 * README download links, and CHANGELOG section synchronized in one atomic run
 * so `auto-tag.yml` -> `release.yml` (EXE) + `android.yml` (APK) never drift.
 *
 * Usage:
 *   node scripts/prepare-release.mjs <patch|minor|major|X.Y.Z>
 *     [--root <dir>] [--date YYYY-MM-DD] [--notes "<bullet text>"] [--dry-run]
 */
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const WORKSPACE_CRATES = [
  "aicore",
  "compat",
  "officecore",
  "pdf-swiss-army-knife",
  "pdfcore",
  "synccore",
  "turkish",
];

/** Computes the next X.Y.Z version from `current` and a bump kind or explicit SemVer. */
export function resolveNextVersion(current, spec) {
  const curMatch = /^(\d+)\.(\d+)\.(\d+)$/.exec(String(current || "").trim());
  if (!curMatch) {
    throw new Error(`Invalid current version "${current}"`);
  }
  const [major, minor, patch] = curMatch.slice(1).map((part) => Number.parseInt(part, 10));
  const raw = String(spec || "").trim().replace(/^v/, "");
  if (raw === "patch") return `${major}.${minor}.${patch + 1}`;
  if (raw === "minor") return `${major}.${minor + 1}.0`;
  if (raw === "major") return `${major + 1}.0.0`;
  if (/^\d+\.\d+\.\d+$/.test(raw)) return raw;
  throw new Error(`Unsupported version bump "${spec}" (expected patch, minor, major, or X.Y.Z)`);
}

/** Android store integer code matching `scripts/release-guard.mjs`. */
export function computeVersionCode(version) {
  const match = /^(\d+)\.(\d+)\.(\d+)$/.exec(version);
  if (!match) throw new Error(`Invalid SemVer "${version}"`);
  const [major, minor, patch] = match.slice(1).map((part) => Number.parseInt(part, 10));
  if (minor >= 1000 || patch >= 1000) {
    throw new Error(`Minor/patch must stay below 1000 for Android versionCode (${version})`);
  }
  return major * 1_000_000 + minor * 1_000 + patch;
}

function writePreservingEol(filePath, nextUnixText, originalText) {
  const eol = originalText.includes("\r\n") ? "\r\n" : "\n";
  const output = eol === "\n" ? nextUnixText : nextUnixText.replace(/\n/g, eol);
  writeFileSync(filePath, output, "utf8");
}

function updateJsonFile(filePath, mutator, dryRun) {
  if (!existsSync(filePath)) return false;
  const original = readFileSync(filePath, "utf8");
  const data = JSON.parse(original);
  mutator(data);
  const next = `${JSON.stringify(data, null, 2)}\n`;
  if (!dryRun) writePreservingEol(filePath, next, original);
  return true;
}

/** Updates ONLY the 7 OmniOffice workspace crates inside Cargo.lock. */
export function updateWorkspaceCargoLock(lockText, nextVersion) {
  const crateSet = new Set(WORKSPACE_CRATES);
  const lines = lockText.replace(/\r\n/g, "\n").split("\n");
  let activePackage = null;
  let updated = 0;

  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i];
    if (line === "[[package]]") {
      activePackage = null;
      continue;
    }
    const nameMatch = /^name = "([^"]+)"$/.exec(line);
    if (nameMatch) {
      activePackage = nameMatch[1];
      continue;
    }
    if (activePackage && crateSet.has(activePackage) && /^version = "[^"]+"$/.test(line)) {
      lines[i] = `version = "${nextVersion}"`;
      updated += 1;
      activePackage = null;
    }
  }
  return { text: lines.join("\n"), updated };
}

/** Updates README version badge, What's new heading, and GitHub download links. */
export function updateReadmeVersion(readmeText, nextVersion) {
  let out = readmeText.replace(/\r\n/g, "\n");
  out = out.replace(/\*\*Version \d+\.\d+\.\d+\*\*/, `**Version ${nextVersion}**`);
  out = out.replace(/^## What's new in \d+\.\d+\.\d+/m, `## What's new in ${nextVersion}`);
  out = out.replace(/\*\*Downloads \(v\d+\.\d+\.\d+\)\*\*/, `**Downloads (v${nextVersion})**`);
  out = out.replace(/\[Release v\d+\.\d+\.\d+\]/g, `[Release v${nextVersion}]`);
  out = out.replace(/releases\/tag\/v\d+\.\d+\.\d+/g, `releases/tag/v${nextVersion}`);
  out = out.replace(/releases\/download\/v\d+\.\d+\.\d+\//g, `releases/download/v${nextVersion}/`);
  out = out.replace(/OmniOffice-Setup-\d+\.\d+\.\d+\.exe/g, `OmniOffice-Setup-${nextVersion}.exe`);
  out = out.replace(/OmniOffice-Portable-\d+\.\d+\.\d+\.zip/g, `OmniOffice-Portable-${nextVersion}.zip`);
  out = out.replace(/OmniOffice-Linux-\d+\.\d+\.\d+-amd64\.(deb|AppImage)/g, `OmniOffice-Linux-${nextVersion}-amd64.$1`);
  out = out.replace(
    /OmniOffice-Android-\d+\.\d+\.\d+-(arm64-v8a|armeabi-v7a)\.apk/g,
    `OmniOffice-Android-${nextVersion}-$1.apk`,
  );
  return out;
}

/** Inserts a new release section under `## [Unreleased]` if missing. */
export function upsertChangelogSection(changelogText, nextVersion, date, notes) {
  const unix = changelogText.replace(/\r\n/g, "\n");
  const marker = `## [${nextVersion}]`;
  if (unix.includes(marker)) {
    return { text: unix, added: false };
  }
  const bullet = notes?.trim() || `- Release ${nextVersion}.`;
  const block = [
    "## [Unreleased]",
    "",
    `## [${nextVersion}] - ${date}`,
    "",
    "### Changed",
    "",
    bullet.startsWith("-") ? bullet : `- ${bullet}`,
  ].join("\n");

  if (unix.includes("## [Unreleased]")) {
    return { text: unix.replace("## [Unreleased]", block), added: true };
  }
  return { text: `${block}\n\n${unix}`, added: true };
}

/** Applies the version bump across all OmniOffice manifests and metadata files. */
export function prepareRelease({
  root = join(dirname(fileURLToPath(import.meta.url)), ".."),
  bump,
  date = new Date().toISOString().slice(0, 10),
  notes = "",
  dryRun = false,
} = {}) {
  const resolvedRoot = resolve(root);
  const pkgPath = join(resolvedRoot, "package.json");
  const pkg = JSON.parse(readFileSync(pkgPath, "utf8"));
  const nextVersion = resolveNextVersion(pkg.version, bump);
  const code = computeVersionCode(nextVersion);
  const changed = [];

  if (updateJsonFile(pkgPath, (doc) => { doc.version = nextVersion; }, dryRun)) {
    changed.push("package.json");
  }

  const lockJsonPath = join(resolvedRoot, "package-lock.json");
  if (
    updateJsonFile(
      lockJsonPath,
      (doc) => {
        doc.version = nextVersion;
        if (doc.packages && doc.packages[""]) {
          doc.packages[""].version = nextVersion;
        }
      },
      dryRun,
    )
  ) {
    changed.push("package-lock.json");
  }

  const tauriConfPath = join(resolvedRoot, "src-tauri", "tauri.conf.json");
  if (updateJsonFile(tauriConfPath, (doc) => { doc.version = nextVersion; }, dryRun)) {
    changed.push("src-tauri/tauri.conf.json");
  }

  const cargoTomlPath = join(resolvedRoot, "Cargo.toml");
  if (existsSync(cargoTomlPath)) {
    const original = readFileSync(cargoTomlPath, "utf8");
    const next = original
      .replace(/\r\n/g, "\n")
      .replace(/(\[workspace\.package\][\s\S]*?^\s*version\s*=\s*")([^"]+)(")/m, `$1${nextVersion}$3`);
    if (!dryRun) writePreservingEol(cargoTomlPath, next, original);
    changed.push("Cargo.toml");
  }

  const cargoLockPath = join(resolvedRoot, "Cargo.lock");
  if (existsSync(cargoLockPath)) {
    const original = readFileSync(cargoLockPath, "utf8");
    const { text } = updateWorkspaceCargoLock(original, nextVersion);
    if (!dryRun) writePreservingEol(cargoLockPath, text, original);
    changed.push("Cargo.lock");
  }

  const androidPropsPath = join(resolvedRoot, "src-tauri", "gen", "android", "app", "tauri.properties");
  if (existsSync(androidPropsPath)) {
    const original = readFileSync(androidPropsPath, "utf8");
    const next = original
      .replace(/\r\n/g, "\n")
      .replace(/^tauri\.android\.versionName=.*$/m, `tauri.android.versionName=${nextVersion}`)
      .replace(/^tauri\.android\.versionCode=.*$/m, `tauri.android.versionCode=${code}`);
    if (!dryRun) writePreservingEol(androidPropsPath, next, original);
    changed.push("src-tauri/gen/android/app/tauri.properties");
  }

  const readmePath = join(resolvedRoot, "README.md");
  if (existsSync(readmePath)) {
    const original = readFileSync(readmePath, "utf8");
    const next = updateReadmeVersion(original, nextVersion);
    if (!dryRun && next !== original.replace(/\r\n/g, "\n")) {
      writePreservingEol(readmePath, next, original);
    }
    changed.push("README.md");
  }

  const changelogPath = join(resolvedRoot, "CHANGELOG.md");
  if (existsSync(changelogPath)) {
    const original = readFileSync(changelogPath, "utf8");
    const { text, added } = upsertChangelogSection(original, nextVersion, date, notes);
    if (added) {
      if (!dryRun) writePreservingEol(changelogPath, text, original);
      changed.push("CHANGELOG.md");
    }
  }

  const enFastlaneDir = join(resolvedRoot, "fastlane", "metadata", "android", "en-US", "changelogs");
  if (existsSync(enFastlaneDir)) {
    const storeFile = join(enFastlaneDir, `${code}.txt`);
    if (!existsSync(storeFile)) {
      const text = `${nextVersion}: ${(notes || "Quality, release automation, and UI improvements.").trim()}\n`;
      if (!dryRun) writeFileSync(storeFile, text, "utf8");
      changed.push(`fastlane/metadata/android/en-US/changelogs/${code}.txt`);
    }
  }

  return {
    previousVersion: pkg.version,
    version: nextVersion,
    versionCode: code,
    dryRun,
    changed,
  };
}

function parseArgs(argv) {
  const options = {
    bump: "",
    root: join(dirname(fileURLToPath(import.meta.url)), ".."),
    date: new Date().toISOString().slice(0, 10),
    notes: "",
    dryRun: false,
  };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--dry-run") options.dryRun = true;
    else if (arg === "--root") options.root = argv[++i] || "";
    else if (arg.startsWith("--root=")) options.root = arg.slice("--root=".length);
    else if (arg === "--date") options.date = argv[++i] || "";
    else if (arg.startsWith("--date=")) options.date = arg.slice("--date=".length);
    else if (arg === "--notes") options.notes = argv[++i] || "";
    else if (arg.startsWith("--notes=")) options.notes = arg.slice("--notes=".length);
    else if (!arg.startsWith("-") && !options.bump) options.bump = arg;
    else return null;
  }
  return options.bump && options.root ? options : null;
}

function main(argv) {
  const options = parseArgs(argv);
  if (!options) {
    console.error(
      "release:bump: usage: node scripts/prepare-release.mjs <patch|minor|major|X.Y.Z> [--root <dir>] [--date YYYY-MM-DD] [--notes <text>] [--dry-run]",
    );
    return 2;
  }
  const result = prepareRelease(options);
  console.log(
    `release:bump${result.dryRun ? " (dry-run)" : ""}: ${result.previousVersion} -> ${result.version} (versionCode ${result.versionCode})`,
  );
  for (const file of result.changed) {
    console.log(`  - ${file}`);
  }
  return 0;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  process.exit(main(process.argv.slice(2)));
}
