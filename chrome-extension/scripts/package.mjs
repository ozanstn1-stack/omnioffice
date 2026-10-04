// Packages the built extension for distribution:
//   release-artifacts/OmniOffice-Chrome-Extension-<version>.zip
// plus a SHA-256 checksum file. Run `npm run build` first.
import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, rmSync, existsSync } from "node:fs";
import { createHash } from "node:crypto";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");
const dist = join(root, "dist");
const releaseDir = join(root, "..", "release-artifacts");
const version = JSON.parse(readFileSync(join(root, "package.json"), "utf8")).version;
const zipPath = join(releaseDir, `OmniOffice-Chrome-Extension-${version}.zip`);

if (!existsSync(join(dist, "manifest.json"))) {
  console.error("dist/manifest.json not found — run `npm run build` first.");
  process.exit(1);
}

mkdirSync(releaseDir, { recursive: true });
rmSync(zipPath, { force: true });
execFileSync(
  "powershell",
  ["-NoProfile", "-Command", `Compress-Archive -Path '${dist}\\*' -DestinationPath '${zipPath}' -CompressionLevel Optimal`],
  { stdio: "inherit" },
);

const hash = createHash("sha256").update(readFileSync(zipPath)).digest("hex");
const checksums = join(releaseDir, "SHA256SUMS-extension.txt");
const line = `${hash}  ${zipPath.split(/[\\/]/).pop()}\n`;
execFileSync("powershell", ["-NoProfile", "-Command", `Set-Content -Path '${checksums}' -Value '${line.trim()}' -Encoding ascii`]);
console.log(`packaged: ${zipPath}`);
console.log(`checksum: ${hash}`);
