#!/usr/bin/env node
// Patches the version files for a **beta** build. CI-only: this is what makes
// the beta channel a real channel instead of a runtime toggle.
//
//   node scripts/set-update-channel.mjs beta 6.1.0-beta.12
//
// What it does:
//   - package.json version -> the beta version (make-latest-json reads it),
//   - src-tauri/tauri.conf.json `version` -> the beta version (the bundle
//     version, which is also what the app reports as its own version),
//   - src-tauri/tauri.conf.json updater endpoint -> the rolling `beta`
//     prerelease manifest, so a beta install never follows the stable
//     `releases/latest` endpoint (and vice versa).
//
// The beta version must be a *prerelease of the next* version
// (e.g. 6.1.0-beta.12 while stable is 6.0.0): semver then says the beta is
// newer than the installed stable, and the later stable 6.1.0 is newer than
// the beta, so beta users are pulled back onto the stable train when it ships.
//
// The workflow never commits these edits; they live only in the runner.

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const channel = process.argv[2];
const version = process.argv[3];

if (channel !== "beta") {
  console.error(
    `set-update-channel: unknown channel "${channel}" (only "beta" is patchable; stable is the repo state)`,
  );
  process.exit(2);
}
if (!/^\d+\.\d+\.\d+-beta\.\d+$/.test(version ?? "")) {
  console.error(`set-update-channel: "${version}" is not a X.Y.Z-beta.N version`);
  process.exit(2);
}

const BETA_ENDPOINT = "https://github.com/ozanstn1-stack/omnioffice/releases/download/beta/latest.json";

/** Writes JSON back with the file's original line endings and trailing newline. */
function writeJson(path, value) {
  const original = readFileSync(path, "utf8");
  const eol = original.includes("\r\n") ? "\r\n" : "\n";
  const text = `${JSON.stringify(value, null, 2)}\n`;
  writeFileSync(path, eol === "\n" ? text : text.replace(/\n/g, eol));
}

const packagePath = join(root, "package.json");
const packageJson = JSON.parse(readFileSync(packagePath, "utf8"));
packageJson.version = version;
writeJson(packagePath, packageJson);

const confPath = join(root, "src-tauri", "tauri.conf.json");
const conf = JSON.parse(readFileSync(confPath, "utf8"));
conf.version = version;
if (!conf.plugins?.updater) {
  console.error("set-update-channel: tauri.conf.json has no updater plugin section");
  process.exit(1);
}
conf.plugins.updater.endpoints = [BETA_ENDPOINT];
writeJson(confPath, conf);

console.log(`set-update-channel: beta ${version}, updater endpoint ${BETA_ENDPOINT}`);
