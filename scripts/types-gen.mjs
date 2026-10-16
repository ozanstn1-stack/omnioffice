/**
 * Renders the TypeScript bindings for the IPC DTOs from the Rust structs.
 *
 *   npm run types:gen     regenerate src/lib/generated
 *   npm run types:check   regenerate and fail when the committed files drift
 *
 * The DTOs opt in with `#[cfg_attr(feature = "ts", ts(export))]`; enabling the
 * app crate's `ts` feature also enables pdfcore's, so every opted-in type
 * exports in one run. The generated files are committed and a test
 * (`src/lib/generated/contract.test.ts`) compares them with the hand-written
 * types the app still narrows.
 */
import { spawnSync } from "node:child_process";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const outDir = join(root, "src", "lib", "generated");
const check = process.argv.includes("--check");
const isWindows = process.platform === "win32";

// `cargo` is not always on PATH (the app's docs add ~/.cargo/bin); do it here
// so the npm script works from a fresh shell.
const cargoBin = join(process.env.USERPROFILE ?? process.env.HOME ?? "", ".cargo", "bin");
const pathValue = `${cargoBin}${isWindows ? ";" : ":"}${process.env.PATH ?? ""}`;
const env = { ...process.env, PATH: pathValue, TS_RS_EXPORT_DIR: outDir };

function run(command, args, extra = {}) {
  return spawnSync(command, args, { cwd: root, env, stdio: "inherit", shell: isWindows, ...extra });
}

const result = run("cargo", [
  "test",
  "--workspace",
  "--features",
  "pdf-swiss-army-knife/ts",
  "export_bindings",
]);
if (result.status !== 0) {
  console.error("types:gen: cargo failed; the bindings were not regenerated.");
  process.exit(1);
}

// A silently empty export must not pass: every opted-in type writes a file.
const expected = [
  "BatesOptions.ts",
  "CompressOptions.ts",
  "GibQr.ts",
  "HeaderFooterOptions.ts",
  "ImageRedaction.ts",
  "InvoiceQr.ts",
  "OcrOptions.ts",
  "OcrPreprocess.ts",
  "Recipe.ts",
  "RecipeStep.ts",
  "RedactionArea.ts",
  "RedactionOptions.ts",
];
const written = new Set(readdirSync(outDir));
const missing = expected.filter((name) => !written.has(name));
if (missing.length > 0) {
  console.error(`types:gen: expected files are missing: ${missing.join(", ")}`);
  process.exit(1);
}
for (const name of expected) {
  const text = readFileSync(join(outDir, name), "utf8");
  if (!text.includes("export type")) {
    console.error(`types:gen: ${name} carries no type export.`);
    process.exit(1);
  }
}
console.log(`types:gen: ${expected.length} bindings written to src/lib/generated.`);

if (check) {
  // `git status` catches a modified file and an untracked one; `git diff` alone
  // would silently accept a binding that was never committed.
  const status = run("git", ["status", "--porcelain", "--", "src/lib/generated"], {
    encoding: "utf8",
    stdio: "pipe",
  });
  const dirty = String(status.stdout ?? "").trim();
  if (dirty) {
    console.error("types:check: src/lib/generated is out of date:");
    console.error(dirty);
    console.error("Run `npm run types:gen` and commit the result.");
    process.exit(1);
  }
  console.log("types:check: the committed bindings match the Rust structs.");
}
