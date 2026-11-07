#!/usr/bin/env node
/**
 * The release guard: a version may not go out without its release notes.
 *
 * A release is built from three pieces of text that are easy to forget in a
 * version-bump pull request, and nothing used to notice until the tag was
 * already pushed:
 *
 *   1. the `## [X.Y.Z]` section in CHANGELOG.md, which becomes the release
 *      body and the Android changelog;
 *   2. `fastlane/metadata/android/en-US/changelogs/<versionCode>.txt`, which
 *      is the text the stores show;
 *   3. the version files agreeing with each other, so the tag can exist at all.
 *
 * On 2026-11-07 v7.12.0 was tagged without (1) and (2): the Android publish
 * job failed with `changelog-section: no '## [7.12.0]' section`, the release
 * page was never created, and the only recovery was moving the tag with a
 * force push. This guard runs in pull-request CI, in the auto-tag workflow
 * before the tag is created, and at the very start of the release and Android
 * builds, so the same mistake costs a red pull request instead.
 *
 * Usage:
 *   node scripts/release-guard.mjs [--root <dir>] [--tag vX.Y.Z] [--json]
 *
 * Exit codes: 0 the version is releasable, 1 a check failed, 2 bad arguments.
 */
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

import { extractSection } from "./changelog-section.mjs";

/** Where the version lives. A listed file that is missing is an error, not a
 * skipped check: the list is meant to stay explicit. */
export const VERSION_SOURCES = [
  { file: "package.json", read: (text) => JSON.parse(text).version },
  { file: "src-tauri/tauri.conf.json", read: (text) => JSON.parse(text).version },
  { file: "Cargo.toml", read: cargoPackageVersion },
];

/** Android metadata written by `tauri android init`/the build script. */
export const ANDROID_PROPERTIES = "src-tauri/gen/android/app/tauri.properties";

const CHANGELOG = "CHANGELOG.md";

/** `[Unreleased]`-style placeholders that are not release notes. */
const PLACEHOLDER = /^(\[?tbd\]?|todo|wip|\.\.\.|…|-\s*)$/i;

/**
 * The version a Cargo manifest declares, without a TOML parser: the root
 * manifest is a virtual workspace, so the value lives in `[workspace.package]`
 * (and a crate that carries its own `[package]` version wins).
 */
export function cargoPackageVersion(text) {
  const found = {};
  let section = "";
  for (const line of text.split(/\r?\n/)) {
    const header = line.match(/^\s*\[([^\]]+)\]/);
    if (header) {
      section = header[1].trim();
      continue;
    }
    const version = line.match(/^\s*version\s*=\s*"([^"]+)"/);
    if (version && (section === "package" || section === "workspace.package")) {
      found[section] = version[1];
    }
  }
  return found.package || found["workspace.package"] || null;
}

/**
 * The integer code the stores need. The same formula as
 * `scripts/build-android.ps1` and `scripts/fastlane-changelogs.mjs`; both are
 * scripts, not modules, so it is repeated here and pinned by the tests.
 */
export function versionCode(version) {
  const match = /^(\d+)\.(\d+)\.(\d+)$/.exec(String(version || ""));
  if (!match) {
    return null;
  }
  const [major, minor, patch] = match.slice(1).map((part) => Number.parseInt(part, 10));
  if (minor >= 1000 || patch >= 1000) {
    return null;
  }
  return major * 1_000_000 + minor * 1_000 + patch;
}

/** The section body without its heading, or `null` when the version has none. */
export function sectionBody(changelogText, version) {
  const section = extractSection(changelogText, version);
  if (section === null) {
    return null;
  }
  return section.split(/\r?\n/).slice(1).join("\n");
}

/**
 * True when the section says something. Subheadings, blank lines, HTML
 * comments and `TBD`/`TODO` placeholders do not count: a `### Added` heading
 * on its own is exactly the shape the v7.12.0 section would have had.
 */
export function isMeaningful(text) {
  if (!text) {
    return false;
  }
  return stripComments(text)
    .split(/\r?\n/)
    .some((line) => {
      const trimmed = line.trim();
      if (trimmed === "") {
        return false;
      }
      if (/^#{1,6}\s/.test(trimmed) || /^-{3,}$/.test(trimmed)) {
        return false;
      }
      return !PLACEHOLDER.test(trimmed.replace(/^[-*]\s*/, "").replace(/[*_`]/g, ""));
    });
}

/**
 * Removes HTML comments by scanning, not by one regular-expression pass: a
 * single pass can leave a fresh `<!--` behind and CodeQL is right to flag it.
 * An unterminated comment hides the rest of the section.
 */
function stripComments(text) {
  let kept = "";
  let rest = text;
  for (;;) {
    const open = rest.indexOf("<!--");
    if (open < 0) {
      return kept + rest;
    }
    const close = rest.indexOf("-->", open + 4);
    if (close < 0) {
      return kept;
    }
    kept += rest.slice(0, open);
    rest = rest.slice(close + 3);
  }
}

function readAndroidProperties(text) {
  return {
    versionName: (text.match(/versionName=([^\r\n]+)/) || [])[1],
    versionCode: (text.match(/versionCode=([0-9]+)/) || [])[1],
  };
}

function read(root, relative) {
  const path = join(root, relative);
  return existsSync(path) ? readFileSync(path, "utf8") : null;
}

/**
 * Every check, collected rather than stopping at the first: one run should tell
 * the whole story of what the version is missing.
 */
export function checkRelease({ root = process.cwd(), tag = "" } = {}) {
  const errors = [];
  const versions = [];

  for (const source of VERSION_SOURCES) {
    const text = read(root, source.file);
    if (text === null) {
      errors.push(`${source.file}: file is missing`);
      continue;
    }
    let version = null;
    try {
      version = source.read(text) || null;
    } catch {
      errors.push(`${source.file}: is not readable JSON`);
      continue;
    }
    if (!version) {
      errors.push(`${source.file}: no version found`);
      continue;
    }
    versions.push({ file: source.file, version });
  }

  const distinct = [...new Set(versions.map((entry) => entry.version))];
  if (distinct.length > 1) {
    errors.push(`version files disagree: ${versions.map((entry) => `${entry.file}=${entry.version}`).join(", ")}`);
  }
  const version = distinct[0] || "";
  const code = versionCode(version);
  if (version && code === null) {
    errors.push(`${version}: not a releasable X.Y.Z version (minor and patch must stay below 1000)`);
  }

  const properties = read(root, ANDROID_PROPERTIES);
  if (properties === null) {
    errors.push(`${ANDROID_PROPERTIES}: file is missing`);
  } else {
    const android = readAndroidProperties(properties);
    if (android.versionName !== version) {
      errors.push(`${ANDROID_PROPERTIES}: versionName=${android.versionName}, expected ${version}`);
    }
    if (code !== null && android.versionCode !== String(code)) {
      errors.push(`${ANDROID_PROPERTIES}: versionCode=${android.versionCode}, expected ${code}`);
    }
  }

  const changelog = read(root, CHANGELOG);
  if (changelog === null) {
    errors.push(`${CHANGELOG}: file is missing`);
  } else {
    const body = sectionBody(changelog, version);
    if (body === null) {
      errors.push(`${CHANGELOG}: no '## [${version}]' section in ${CHANGELOG}`);
    } else if (!isMeaningful(body)) {
      errors.push(`${CHANGELOG}: the '## [${version}]' section is empty`);
    }
  }

  if (code !== null) {
    const store = `fastlane/metadata/android/en-US/changelogs/${code}.txt`;
    const text = read(root, store);
    if (text === null) {
      errors.push(`${store}: file is missing`);
    } else if (text.trim() === "") {
      errors.push(`${store}: file is empty`);
    }
  }

  if (tag && tag.startsWith("v") && tag.slice(1) !== version) {
    errors.push(`tag ${tag} does not match the version ${version}`);
  }

  return { version, versionCode: code, errors };
}

function parseArguments(argv) {
  const options = { root: process.cwd(), tag: "", json: false };
  for (let i = 0; i < argv.length; i += 1) {
    const argument = argv[i];
    if (argument === "--json") {
      options.json = true;
    } else if (argument === "--root") {
      options.root = argv[++i] || "";
    } else if (argument.startsWith("--root=")) {
      options.root = argument.slice("--root=".length);
    } else if (argument === "--tag") {
      options.tag = argv[++i] || "";
    } else if (argument.startsWith("--tag=")) {
      options.tag = argument.slice("--tag=".length);
    } else {
      return null;
    }
  }
  return options.root ? options : null;
}

function main(argv) {
  const options = parseArguments(argv);
  if (options === null) {
    console.error("release-guard: usage: node scripts/release-guard.mjs [--root <dir>] [--tag vX.Y.Z] [--json]");
    return 2;
  }
  const result = checkRelease(options);
  if (options.json) {
    console.log(JSON.stringify(result));
    return result.errors.length === 0 ? 0 : 1;
  }
  if (result.errors.length === 0) {
    console.log(`release-guard: ${result.version} (versionCode ${result.versionCode}) OK`);
    return 0;
  }
  const inActions = process.env.GITHUB_ACTIONS === "true";
  for (const message of result.errors) {
    console.error(inActions ? `::error::${message}` : `release-guard: ${message}`);
  }
  return 1;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  process.exit(main(process.argv.slice(2)));
}
