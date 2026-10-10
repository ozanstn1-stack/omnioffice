#!/usr/bin/env node
// Writes release-artifacts/latest.json, the Tauri v2 updater manifest for the
// Windows build, and copies the updater artifact and its signature into
// release-artifacts so the release contains the file the manifest points at.
// Release CI runs this after `npm run package` and before the checksum step,
// so latest.json, the artifact and the signature are listed in
// SHA256SUMS.txt and uploaded next to the installer.
//
// Updater artifact shapes:
//   - `createUpdaterArtifacts: true` (v2, what this repo uses): the NSIS
//     installer itself is re-used, `OmniOffice_<version>_x64-setup.exe` plus
//     its `.exe.sig` next to it in bundle/nsis.
//   - `createUpdaterArtifacts: "v1Compatible"` (legacy): a
//     `..._x64-setup.nsis.zip` plus `.nsis.zip.sig` is produced instead.
// Both are handled (the zip wins when both exist). The manifest's URL uses
// the artifact's real file name, which is the name under which the release
// upload publishes it.
//
// The artifacts only exist when the build ran with a signing key
// (TAURI_SIGNING_PRIVATE_KEY). When none is found this script prints why and
// exits 0: the release then has no in-app update manifest and the
// GitHub-releases check remains the fallback.

import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const version = JSON.parse(readFileSync(join(root, "package.json"), "utf8")).version;
// The workspace target dir is the repo root's target/, so that is where the
// Tauri bundler writes (package.ps1 reads it from there as well); the
// src-tauri/target location is the fallback for a build with a crate-local
// target dir.
const bundleDirs = [
  join(root, "target", "release", "bundle", "nsis"),
  join(root, "src-tauri", "target", "release", "bundle", "nsis"),
];
const outDir = join(root, "release-artifacts");

function skip(reason) {
  console.log(`make-latest-json: ${reason}`);
  console.log("make-latest-json: no updater manifest was written.");
  process.exit(0);
}

/** Picks a file for this version: exact `_<version>_` first, then a loose match. */
function pick(names, suffix) {
  const candidates = names.filter((name) => name.endsWith(suffix));
  return candidates.find((name) => name.includes(`_${version}_`)) ?? candidates.find((name) => name.includes(version));
}

/** Returns the updater artifact + its signature inside `dir`, or null. */
function findUpdaterArtifact(dir) {
  const names = readdirSync(dir);
  for (const suffix of [".nsis.zip", "-setup.exe"]) {
    const name = pick(names, suffix);
    if (!name || !names.includes(`${name}.sig`)) continue;
    return { name, path: join(dir, name), sigPath: join(dir, `${name}.sig`) };
  }
  return null;
}

let artifact = null;
for (const dir of bundleDirs) {
  if (!existsSync(dir)) continue;
  artifact = findUpdaterArtifact(dir);
  if (artifact) break;
}
if (!artifact) {
  skip(
    `no signed updater artifact (*.nsis.zip or *-setup.exe with a .sig) found in: ${bundleDirs.join(", ")}; ` +
      "run tauri build first.",
  );
}

/** Release notes: the version's section of CHANGELOG.md, capped like the app does. */
function changelogNotes() {
  const path = join(root, "CHANGELOG.md");
  if (existsSync(path)) {
    const text = readFileSync(path, "utf8");
    const heading = new RegExp(`^##\\s+\\[?${version.replace(/\./g, "\\.")}\\]?.*$`, "m");
    const match = heading.exec(text);
    if (match) {
      const rest = text.slice(match.index + match[0].length);
      const next = rest.search(/^##\s/m);
      const section = (next === -1 ? rest : rest.slice(0, next)).trim();
      if (section) return section.slice(0, 4000);
    }
  }
  return `OmniOffice ${version}`;
}

const manifest = {
  version,
  notes: changelogNotes(),
  pub_date: new Date().toISOString(),
  platforms: {
    "windows-x86_64": {
      signature: readFileSync(artifact.sigPath, "utf8").trim(),
      url: `https://github.com/ozanstn1-stack/omnioffice/releases/download/v${version}/${artifact.name}`,
    },
  },
};

mkdirSync(outDir, { recursive: true });
copyFileSync(artifact.path, join(outDir, artifact.name));
copyFileSync(artifact.sigPath, join(outDir, `${artifact.name}.sig`));
const outPath = join(outDir, "latest.json");
writeFileSync(outPath, `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
console.log(`make-latest-json: wrote ${outPath} for ${artifact.name}`);
