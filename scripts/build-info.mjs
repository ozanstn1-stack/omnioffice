#!/usr/bin/env node
// Emits build-info.json describing exactly what produced a release artifact:
// app version, git SHA, toolchain versions and the Android build tool chain
// when present. CI attaches the file next to the installer, portable ZIP and
// APKs so a downloaded binary can be traced back to its source.
//
// Everything is best-effort: a missing tool (e.g. no Android SDK on a desktop
// runner) is recorded as null rather than failing the build.

import { execSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

function tryCommand(command) {
  try {
    return execSync(command, { cwd: root, stdio: ["ignore", "pipe", "ignore"] }).toString().trim() || null;
  } catch {
    return null;
  }
}

/** rustup installs under the user profile; not always on PATH in CI shells. */
function rustToolBin(name) {
  const home = process.env.USERPROFILE ?? process.env.HOME ?? "";
  const exe = process.platform === "win32" ? `${name}.exe` : name;
  const candidate = join(home, ".cargo", "bin", exe);
  return existsSync(candidate) ? `"${candidate}"` : name;
}

const cargoBin = () => rustToolBin("cargo");
const rustcBin = () => rustToolBin("rustc");

const pkg = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
const tauriConf = JSON.parse(readFileSync(join(root, "src-tauri", "tauri.conf.json"), "utf8"));

const info = {
  product: "Office Swiss Army Knife",
  version: pkg.version,
  tauriVersion: tauriConf.version ?? null,
  gitSha: tryCommand("git rev-parse HEAD"),
  gitShortSha: tryCommand("git rev-parse --short HEAD"),
  gitBranch: tryCommand("git rev-parse --abbrev-ref HEAD"),
  gitDirty: (() => {
    const status = tryCommand("git status --porcelain");
    return status === null ? null : status.length > 0;
  })(),
  node: process.version,
  npm: tryCommand("npm --version"),
  rustc: tryCommand(`${rustcBin()} --version`),
  cargo: tryCommand(`${cargoBin()} --version`),
  java: tryCommand("java -version 2>&1"),
  androidSdk: process.env.ANDROID_SDK_ROOT ?? process.env.ANDROID_HOME ?? null,
  androidNdk: process.env.NDK_HOME ?? null,
  gradle: existsSync(join(root, "src-tauri", "gen", "android"))
    ? tryCommand(
        process.platform === "win32"
          ? ".\\gradlew.bat --version"
          : "./gradlew --version",
      )?.split("\n")[0] ?? null
    : null,
  platform: process.platform,
  arch: process.arch,
  generatedAt: new Date().toISOString(),
};

const outDir = join(root, "release-artifacts");
if (!existsSync(outDir)) mkdirSync(outDir, { recursive: true });
const outPath = join(outDir, "build-info.json");
writeFileSync(outPath, `${JSON.stringify(info, null, 2)}\n`, "utf8");
console.log(`build-info: wrote ${outPath}`);
