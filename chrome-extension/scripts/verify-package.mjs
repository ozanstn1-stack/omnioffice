// Static verification of the packaged extension (runs in CI and before a
// release). Chrome 137+ removed the `--load-extension` flag, so an unpacked
// extension can only be loaded through chrome://extensions by hand; this
// script checks everything that *can* be verified automatically:
//
//   1. manifest.json is MV3, complete, requests no standing host powers and
//      only optional (per-origin, user-consented) host permissions
//   2. every referenced file exists in dist/
//   3. the bundle contains no remote code or remote network references
//   4. the worker, fonts, cmaps and standard fonts are present
//
//   node scripts/verify-package.mjs
import { existsSync, readFileSync, statSync } from "node:fs";
import { readdir } from "node:fs/promises";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const dist = join(here, "..", "dist");
const problems = [];
const notes = [];

function fail(message) {
  problems.push(message);
}

async function walk(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const full = join(directory, entry.name);
    if (entry.isDirectory()) files.push(...(await walk(full)));
    else files.push(full);
  }
  return files;
}

if (!existsSync(join(dist, "manifest.json"))) {
  console.error("dist/manifest.json missing — run `npm run build` first.");
  process.exit(2);
}

const manifest = JSON.parse(readFileSync(join(dist, "manifest.json"), "utf8"));

// 1. manifest sanity
if (manifest.manifest_version !== 3) fail("manifest_version must be 3");
if (!manifest.name || !manifest.version) fail("name/version missing");
if (manifest.host_permissions?.length) fail("host_permissions must stay empty (local-first guarantee)");
if (manifest.permissions?.some((permission) => permission !== "contextMenus")) {
  fail(`unexpected permissions: ${manifest.permissions.join(", ")}`);
}
if (!manifest.background?.service_worker) fail("background service worker missing");
if (!manifest.action) fail("action (toolbar button) missing");
const csp = manifest.content_security_policy?.extension_pages ?? "";
if (!csp.includes("script-src 'self'")) fail("CSP must restrict scripts to 'self'");
if (/script-src[^;]*https?:/.test(csp)) fail("CSP must not allow remote scripts");
// Remote connect-src is only acceptable when it is gated by optional host
// permissions (the context-menu download asks for one origin at a time).
if (/connect-src[^;]*https?:/.test(csp) && !manifest.optional_host_permissions?.length) {
  fail("remote connect-src requires optional_host_permissions");
}
if (/connect-src[^;]*https?:/.test(csp) && manifest.host_permissions?.length) {
  fail("remote connect-src must not come with standing host permissions");
}
if (!manifest.optional_host_permissions?.length) {
  fail("optional_host_permissions missing: the context-menu download cannot request its origin");
}

// 2. referenced files exist
const referenced = [
  manifest.background.service_worker,
  ...(manifest.content_scripts ?? []).flatMap((entry) => entry.js ?? []),
  ...Object.values(manifest.icons ?? {}),
  ...Object.values(manifest.action?.default_icon ?? {}),
  "app.html",
];
for (const file of referenced) {
  if (!file) continue;
  if (!existsSync(join(dist, file))) fail(`referenced file missing: ${file}`);
}
for (const required of ["assets/pdf.worker.min.mjs", "vendor/cmaps/Adobe-Japan1-0.bcmap", "fonts/PT_Sans-Web-Regular.ttf"]) {
  if (!existsSync(join(dist, required))) fail(`expected asset missing: ${required}`);
}

// 3. no remote code or network endpoints
//
// Namespace identifiers such as http://www.w3.org/2000/svg or XML namespaces
// inside pdf.js are data, not fetches. The strict URL scan therefore applies
// to our own bundles, and every bundle is checked for remote fetch/import
// patterns.
const namespaceAllowList = [
  "http://www.w3.org/",
  "http://ns.adobe.com/",
  "http://www.xfa.org/",
  "http://www.apache.org/",
  "https://github.com/",
  "https://opensource.org/",
  "https://creativecommons.org/",
  "https://www.gnu.org/",
  "https://developer.mozilla.org/",
  "https://react.dev/",
  "https://fonts.google.com/",
  // RFC 2606 reserved domains: the self test uses them as inert fixtures
  // (the fake fetch never leaves the page and the domain cannot resolve).
  "https://example.com",
  "http://example.com",
  "https://example.invalid",
  "http://example.invalid",
];
const remoteFetchPatterns = [
  /\bfetch\(\s*["'`]https?:\/\//i,
  /\bnew\s+Worker\(\s*["'`]https?:\/\//i,
  /\bimport\(\s*["'`]https?:\/\//i,
  /(?:src|href)\s*=\s*["'`]https?:\/\//i,
  /["'`]https?:\/\/(?!www\.w3\.org|ns\.adobe\.com|www\.xfa\.org)[a-z0-9.-]+\.[a-z]{2,}[^"'`]*["'`]\s*,\s*\{\s*mode/i,
];
const ownBundles = ["app.js", "selftest.js", "demo.js"];
const files = await walk(dist);
let remoteHits = 0;
for (const file of files) {
  const extension = file.split(".").pop()?.toLowerCase();
  if (!["js", "mjs", "html", "css", "json"].includes(extension ?? "")) continue;
  const text = readFileSync(file, "utf8");
  const name = relative(dist, file);
  for (const pattern of remoteFetchPatterns) {
    const match = text.match(pattern);
    if (match) {
      remoteHits += 1;
      fail(`remote fetch pattern in ${name}: ${match[0].slice(0, 80)}`);
    }
  }
  // Strict scan for our own code only.
  if (ownBundles.some((bundle) => name.endsWith(bundle))) {
    const urls = text.match(/https?:\/\/[a-z0-9.-]+/gi) ?? [];
    for (const url of urls) {
      if (namespaceAllowList.some((allowed) => url.startsWith(allowed.replace(/\/$/, "")))) continue;
      remoteHits += 1;
      fail(`remote endpoint in ${name}: ${url}`);
    }
  }
}

// 4. size report
let total = 0;
for (const file of files) total += statSync(file).size;
notes.push(`files: ${files.length}`);
notes.push(`size: ${(total / 1024 / 1024).toFixed(2)} MB`);
notes.push(`remote references found: ${remoteHits}`);
// Budget: the extension bundles pdf.js, its worker, cmaps and fonts; 9 MB is
// well above today's ~6.4 MB and still catches a dependency that doubles the
// package by accident.
const MAX_PACKAGE_BYTES = 9 * 1024 * 1024;
if (total > MAX_PACKAGE_BYTES) {
  fail(`package is ${(total / 1024 / 1024).toFixed(2)} MB (budget ${(MAX_PACKAGE_BYTES / 1024 / 1024).toFixed(0)} MB)`);
}

console.log("Extension package verification");
for (const note of notes) console.log(`  · ${note}`);
if (problems.length) {
  console.error("\nProblems:");
  for (const problem of problems) console.error(`  ✖ ${problem}`);
  process.exit(1);
}
console.log("\nAll checks passed: MV3 manifest, no host permissions, no remote code, all assets bundled.");
