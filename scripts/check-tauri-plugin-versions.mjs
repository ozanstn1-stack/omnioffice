// Fails when an npm `@tauri-apps/plugin-*` package and its Rust crate resolve
// to different major.minor versions: `tauri build` refuses that combination
// ("Found version mismatched Tauri packages"), and a Dependabot bump that
// updated only the npm side shipped exactly that in 3.5.3.
//
// The Rust version is read from Cargo.lock (Cargo.toml keeps a loose "2"), the
// npm version from the installed package (falling back to the manifest range
// when node_modules is absent).
import { readFileSync } from "node:fs";

const lock = readFileSync("Cargo.lock", "utf8");
const manifest = JSON.parse(readFileSync("package.json", "utf8"));
const dependencies = { ...manifest.dependencies, ...manifest.devDependencies };

function rustCrateVersion(name) {
  const match = new RegExp(`name = "${name}"\\nversion = "([^"]+)"`).exec(lock);
  return match?.[1] ?? null;
}

function npmPackageVersion(name, spec) {
  try {
    return JSON.parse(readFileSync(`node_modules/${name}/package.json`, "utf8")).version;
  } catch {
    return String(spec).replace(/^[^\d]*/, "");
  }
}

function majorMinor(version) {
  const [major, minor] = String(version).split(".");
  return `${major}.${minor}`;
}

let failed = false;
let checked = 0;
for (const [npmName, spec] of Object.entries(dependencies)) {
  const match = /^@tauri-apps\/plugin-(.+)$/.exec(npmName);
  if (!match) continue;
  const rustName = `tauri-plugin-${match[1]}`;
  const rustVersion = rustCrateVersion(rustName);
  if (!rustVersion) continue; // Rust-side-only plugin integration
  const npmVersion = npmPackageVersion(npmName, spec);
  checked += 1;
  if (majorMinor(npmVersion) !== majorMinor(rustVersion)) {
    failed = true;
    console.error(
      `MISMATCH ${npmName} ${npmVersion} vs ${rustName} ${rustVersion}: keep the same major.minor on both sides`,
    );
  } else {
    console.log(`ok ${npmName} ${npmVersion} = ${rustName} ${rustVersion}`);
  }
}

if (checked === 0) {
  console.error("No Tauri plugin pairs found; the check did not run.");
  process.exit(1);
}
if (failed) {
  console.error("Bump the Rust crate (Cargo.lock) and the npm package together, or pin one side back.");
  process.exit(1);
}
