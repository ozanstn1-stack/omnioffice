#!/usr/bin/env node
// Emits a CycloneDX 1.5 SBOM for the Rust dependency graph from Cargo.lock.
//
// The release workflow generates the same file with anchore/sbom-action; this
// local generator keeps the artifact available for offline release builds
// without pulling a container tool. It reads names, versions and SHA-256
// checksums straight from the lockfile, so it describes exactly what was
// compiled.
//
// Usage: node scripts/sbom-rust.mjs [output-file]

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const output = process.argv[2] ?? join(root, "release-artifacts", "sbom-rust.cyclonedx.json");

const lock = readFileSync(join(root, "Cargo.lock"), "utf8");
const pkg = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));

/** Minimal Cargo.lock parser: `[[package]]` blocks with name/version/checksum. */
function parsePackages(text) {
  const packages = [];
  let current = null;
  for (const rawLine of text.split(/\r?\n/)) {
    const line = rawLine.trim();
    if (line === "[[package]]") {
      current = {};
      packages.push(current);
      continue;
    }
    if (!current) continue;
    const match = /^([a-zA-Z_]+)\s*=\s*"([^"]*)"$/.exec(line);
    if (!match) continue;
    if (match[1] === "name") current.name = match[2];
    else if (match[1] === "version") current.version = match[2];
    else if (match[1] === "checksum") current.checksum = match[2];
  }
  return packages.filter((entry) => entry.name && entry.version);
}

const packages = parsePackages(lock);
const components = packages.map((entry) => {
  const component = {
    type: "library",
    name: entry.name,
    version: entry.version,
    purl: `pkg:cargo/${entry.name}@${entry.version}`,
  };
  if (entry.checksum) {
    component.hashes = [{ alg: "SHA-256", content: entry.checksum }];
  }
  return component;
});

const bom = {
  bomFormat: "CycloneDX",
  specVersion: "1.5",
  serialNumber: `urn:uuid:${crypto.randomUUID()}`,
  version: 1,
  metadata: {
    timestamp: new Date().toISOString(),
    tools: [{ vendor: "OmniOffice", name: "sbom-rust.mjs", version: pkg.version }],
    component: {
      type: "application",
      name: "pdf-swiss-army-knife",
      version: pkg.version,
    },
  },
  components,
};

writeFileSync(output, `${JSON.stringify(bom, null, 2)}\n`, "utf8");
console.log(`sbom-rust: wrote ${output} (${components.length} components)`);
