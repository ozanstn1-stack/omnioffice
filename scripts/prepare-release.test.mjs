import assert from "node:assert/strict";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, test } from "node:test";

import { checkRelease } from "./release-guard.mjs";
import {
  computeVersionCode,
  prepareRelease,
  resolveNextVersion,
  updateReadmeVersion,
  updateWorkspaceCargoLock,
} from "./prepare-release.mjs";

const sandboxes = [];
afterEach(() => {
  for (const dir of sandboxes.splice(0)) {
    rmSync(dir, { recursive: true, force: true });
  }
});

function createRepoFixture(version = "8.2.0") {
  const root = mkdtempSync(join(tmpdir(), "prepare-release-"));
  sandboxes.push(root);
  writeFileSync(join(root, "package.json"), JSON.stringify({ name: "app", version }, null, 2));
  writeFileSync(
    join(root, "package-lock.json"),
    JSON.stringify({ name: "app", version, packages: { "": { version } } }, null, 2),
  );
  writeFileSync(
    join(root, "Cargo.toml"),
    `[workspace]\nmembers = ["crates/pdfcore"]\n\n[workspace.package]\nversion = "${version}"\nedition = "2021"\n`,
  );
  writeFileSync(
    join(root, "Cargo.lock"),
    [
      "[[package]]",
      'name = "officecore"',
      `version = "${version}"`,
      "",
      "[[package]]",
      'name = "objc2-encode"',
      'version = "4.1.0"',
      "",
      "[[package]]",
      'name = "pdf-swiss-army-knife"',
      `version = "${version}"`,
      "",
    ].join("\n"),
  );
  mkdirSync(join(root, "src-tauri", "gen", "android", "app"), { recursive: true });
  writeFileSync(join(root, "src-tauri", "tauri.conf.json"), JSON.stringify({ version }, null, 2));
  writeFileSync(
    join(root, "src-tauri", "gen", "android", "app", "tauri.properties"),
    `// generated\ntauri.android.versionName=${version}\ntauri.android.versionCode=8002000\n`,
  );
  writeFileSync(
    join(root, "README.md"),
    [
      `**Version ${version}**`,
      "",
      `## What's new in ${version}`,
      "",
      `**Downloads (v${version})** · [Release v${version}](https://github.com/ozanstn1-stack/omnioffice/releases/tag/v${version})`,
      `[Setup](https://github.com/ozanstn1-stack/omnioffice/releases/download/v${version}/OmniOffice-Setup-${version}.exe)`,
      `[APK](https://github.com/ozanstn1-stack/omnioffice/releases/download/v${version}/OmniOffice-Android-${version}-arm64-v8a.apk)`,
      "",
    ].join("\n"),
  );
  writeFileSync(
    join(root, "CHANGELOG.md"),
    `# Changelog\n\n## [Unreleased]\n\n## [${version}] - 2026-11-23\n\n- Old release\n`,
  );
  mkdirSync(join(root, "fastlane", "metadata", "android", "en-US", "changelogs"), { recursive: true });
  return root;
}

test("resolveNextVersion and computeVersionCode compute expected SemVer and store codes", () => {
  assert.equal(resolveNextVersion("8.2.0", "patch"), "8.2.1");
  assert.equal(resolveNextVersion("8.2.0", "minor"), "8.3.0");
  assert.equal(resolveNextVersion("8.2.0", "major"), "9.0.0");
  assert.equal(resolveNextVersion("8.2.0", "v8.4.2"), "8.4.2");
  assert.equal(computeVersionCode("8.3.0"), 8003000);
});

test("updateWorkspaceCargoLock updates only workspace crates and leaves third-party crates untouched", () => {
  const lock = [
    "[[package]]",
    'name = "officecore"',
    'version = "8.2.0"',
    "",
    "[[package]]",
    'name = "objc2-encode"',
    'version = "4.1.0"',
    "",
    "[[package]]",
    'name = "turkish"',
    'version = "8.2.0"',
  ].join("\n");
  const { text, updated } = updateWorkspaceCargoLock(lock, "8.3.0");
  assert.equal(updated, 2);
  assert.ok(text.includes('name = "officecore"\nversion = "8.3.0"'));
  assert.ok(text.includes('name = "objc2-encode"\nversion = "4.1.0"'));
  assert.ok(text.includes('name = "turkish"\nversion = "8.3.0"'));
});

test("updateReadmeVersion keeps download URLs and headers aligned", () => {
  const input =
    "**Version 8.0.0**\n## What's new in 8.0.0\n**Downloads (v7.50.0)**\n" +
    "https://github.com/ozanstn1-stack/omnioffice/releases/download/v7.50.0/OmniOffice-Android-7.50.0-arm64-v8a.apk\n";
  const output = updateReadmeVersion(input, "8.3.0");
  assert.ok(output.includes("**Version 8.3.0**"));
  assert.ok(output.includes("## What's new in 8.3.0"));
  assert.ok(output.includes("**Downloads (v8.3.0)**"));
  assert.ok(output.includes("releases/download/v8.3.0/OmniOffice-Android-8.3.0-arm64-v8a.apk"));
});

test("prepareRelease updates all manifests so release-guard passes cleanly", () => {
  const root = createRepoFixture("8.2.0");
  const result = prepareRelease({
    root,
    bump: "minor",
    date: "2026-11-26",
    notes: "Hybrid UI polish and automated release verification.",
  });

  assert.equal(result.version, "8.3.0");
  assert.equal(result.versionCode, 8003000);
  assert.ok(existsSync(join(root, "fastlane", "metadata", "android", "en-US", "changelogs", "8003000.txt")));
  const guard = checkRelease({ root, tag: "v8.3.0" });
  assert.deepEqual(guard.errors, []);

  const lock = readFileSync(join(root, "Cargo.lock"), "utf8");
  assert.ok(lock.includes('name = "objc2-encode"\nversion = "4.1.0"'));
  assert.ok(lock.includes('name = "pdf-swiss-army-knife"\nversion = "8.3.0"'));
});
