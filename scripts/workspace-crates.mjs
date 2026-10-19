#!/usr/bin/env node
// Prints the `-p <crate>` flags for a category of workspace crates, derived
// from `cargo metadata` instead of a hand-maintained list (v6.0.1 audit, B6).
//
//   node scripts/workspace-crates.mjs engine-free   # no native engines needed
//   node scripts/workspace-crates.mjs release       # libraries shipped by CI
//   node scripts/workspace-crates.mjs msrv          # crates checked on the MSRV
//
// Categories:
//   engine-free - crates whose tests do not need pdfium/tesseract/qpdf. Those
//                 engines only ship for Windows (see desktop.yml), so this is
//                 the set the Linux/macOS runners exercise.
//   release     - every library crate (everything except the two binaries:
//                 the Tauri app and the compat runner).
//   msrv        - same set as `release`; the MSRV job checks the libraries.
//
// The app crate and the compat runner are excluded from `release` because
// they are binaries built by the Tauri tooling, not published libraries.
// pdfcore, aicore and compat depend on the native engines in their tests.

import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

const ENGINE_DEPENDENT = new Set(["pdfcore", "aicore", "compat", "pdf-swiss-army-knife"]);

export function workspaceMembers(metadata) {
  const members = new Set(metadata.workspace_members);
  return metadata.packages.filter((entry) => members.has(entry.id)).map((entry) => entry.name);
}

export function cratesFor(members, kind) {
  switch (kind) {
    case "engine-free":
      return members.filter((name) => !ENGINE_DEPENDENT.has(name));
    case "release":
    case "msrv":
      return members.filter((name) => name !== "compat" && name !== "pdf-swiss-army-knife");
    default:
      throw new Error(`unknown category: ${kind} (engine-free | release | msrv)`);
  }
}

export function cargoFlags(members, kind) {
  return cratesFor(members, kind)
    .map((name) => `-p ${name}`)
    .join(" ");
}

function main() {
  const kind = process.argv[2];
  if (!kind) {
    console.error("usage: node scripts/workspace-crates.mjs <engine-free|release|msrv>");
    process.exit(2);
  }
  const metadata = JSON.parse(
    execFileSync("cargo", ["metadata", "--format-version", "1", "--no-deps"], { encoding: "utf8" }),
  );
  process.stdout.write(cargoFlags(workspaceMembers(metadata), kind));
}

const invoked = process.argv[1] ? path.resolve(process.argv[1]) : "";
if (invoked === fileURLToPath(import.meta.url)) {
  main();
}
