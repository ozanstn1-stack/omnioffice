#!/usr/bin/env node
/**
 * Cross-platform engine & font fetcher for OmniOffice.
 *
 * Mirrors scripts/fetch-engines.ps1 in pure Node.js so `--verify-lock` and
 * asset/font fetching work identically on Linux, macOS, and Windows without
 * requiring PowerShell.
 *
 * Usage:
 *   node scripts/fetch-engines.mjs --verify-lock
 *   node scripts/fetch-engines.mjs [--force] [--update-lock] [--fonts-only]
 */

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { pipeline } from "node:stream/promises";
import { Readable } from "node:stream";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const root = path.resolve(__dirname, "..");

const enginesDir = path.join(root, "src-tauri", "resources", "engines");
const fontsDir = path.join(root, "crates", "pdfcore", "assets", "fonts");
const cacheBase = process.env.LOCALAPPDATA || process.env.XDG_CACHE_HOME || "/tmp";
const cacheDir = path.join(cacheBase, "pdf-sak-cache");
const lockPath = path.join(__dirname, "engines.lock.json");

const args = new Set(process.argv.slice(2));
const force = args.has("--force") || args.has("-Force");
const updateLock = args.has("--update-lock") || args.has("-UpdateLock");
const verifyLock = args.has("--verify-lock") || args.has("-VerifyLock");
const fontsOnly = args.has("--fonts-only");

if (args.has("--help") || args.has("-h")) {
  console.log(
    "Usage: node scripts/fetch-engines.mjs [--verify-lock] [--force] [--update-lock] [--fonts-only]",
  );
  process.exit(0);
}

const PINNED = {
  pdfiumUrl:
    "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F8057/pdfium-win-x64.tgz",
  qpdfUrl: "https://github.com/qpdf/qpdf/releases/download/v12.4.1/qpdf-12.4.1-msvc64.zip",
  tessUrl:
    "https://github.com/UB-Mannheim/tesseract/releases/download/v5.4.0.20240606/tesseract-ocr-w64-setup-5.4.0.20240606.exe",
  tessLangs: ["eng", "tur", "deu", "nld", "fra", "spa", "ita", "bul", "osd"],
  tessDataBase: "https://raw.githubusercontent.com/tesseract-ocr/tessdata_fast/main",
  tessdataLicenseUrl: "https://raw.githubusercontent.com/tesseract-ocr/tessdata_fast/main/LICENSE",
  pdfTtfUrl: "https://raw.githubusercontent.com/tesseract-ocr/tesseract/main/tessdata/pdf.ttf",
  sevenZrUrl: "https://github.com/ip7z/7zip/releases/download/23.01/7zr.exe",
  sevenZipUrl: "https://github.com/ip7z/7zip/releases/download/23.01/7z2301-x64.exe",
  ptSansBase: "https://raw.githubusercontent.com/google/fonts/main/ofl/ptsans",
  liberationUrl:
    "https://github.com/liberationfonts/liberation-fonts/files/7261482/liberation-fonts-ttf-2.1.5.tar.gz",
};

const ALL_ARTIFACT_URLS = [
  PINNED.pdfiumUrl,
  PINNED.qpdfUrl,
  PINNED.tessUrl,
  PINNED.tessdataLicenseUrl,
  PINNED.pdfTtfUrl,
  PINNED.sevenZrUrl,
  PINNED.sevenZipUrl,
  PINNED.liberationUrl,
  ...PINNED.tessLangs.map((lang) => `${PINNED.tessDataBase}/${lang}.traineddata`),
  ...["PT_Sans-Web-Regular.ttf", "PT_Sans-Web-Bold.ttf", "OFL.txt"].map(
    (file) => `${PINNED.ptSansBase}/${file}`,
  ),
];

function readLock() {
  if (!fs.existsSync(lockPath)) {
    return { comment: "", artifacts: {} };
  }
  return JSON.parse(fs.readFileSync(lockPath, "utf8"));
}

function writeLock(lock) {
  const sorted = {};
  for (const key of Object.keys(lock.artifacts).sort()) {
    sorted[key] = lock.artifacts[key];
  }
  const next = { comment: lock.comment ?? "", artifacts: sorted };
  fs.writeFileSync(lockPath, JSON.stringify(next, null, 2) + "\n", "utf8");
}

function sha256File(filePath) {
  const hash = crypto.createHash("sha256");
  const data = fs.readFileSync(filePath);
  hash.update(data);
  return hash.digest("hex");
}

function assertArtifact(lock, url, filePath) {
  const actual = sha256File(filePath);
  const expected = lock.artifacts?.[url]?.sha256;
  if (!expected) {
    if (updateLock) {
      lock.artifacts[url] = { sha256: actual };
      writeLock(lock);
      console.log(`  pinned now: ${actual}`);
      return;
    }
    throw new Error(
      `No pinned SHA-256 for ${url}\nRefusing to use an unverified download. Review the source, then record it with --update-lock.`,
    );
  }
  if (actual === expected) return;
  if (updateLock) {
    lock.artifacts[url] = { sha256: actual };
    writeLock(lock);
    console.log(`  re-pinned: ${actual}`);
    return;
  }
  throw new Error(
    `SHA-256 mismatch for ${url}\n  expected ${expected}\n  actual   ${actual}\nRefusing to use the file.`,
  );
}

async function downloadFile(lock, url, outFile) {
  if (fs.existsSync(outFile) && !force) {
    assertArtifact(lock, url, outFile);
    console.log(`  cached: ${outFile}`);
    return;
  }
  console.log(`  downloading: ${url}`);
  const tmp = `${outFile}.part`;
  const response = await fetch(url, {
    headers: { "User-Agent": "pdf-sak-build" },
    redirect: "follow",
  });
  if (!response.ok || !response.body) {
    throw new Error(`Failed to download ${url}: HTTP ${response.status}`);
  }
  fs.mkdirSync(path.dirname(outFile), { recursive: true });
  await pipeline(Readable.fromWeb(response.body), fs.createWriteStream(tmp));
  try {
    assertArtifact(lock, url, tmp);
  } catch (err) {
    fs.rmSync(tmp, { force: true });
    throw err;
  }
  fs.renameSync(tmp, outFile);
}

async function main() {
  const lock = readLock();

  if (verifyLock) {
    const missing = ALL_ARTIFACT_URLS.filter((url) => !lock.artifacts?.[url]?.sha256);
    if (missing.length > 0) {
      throw new Error(
        `engines.lock.json is missing SHA-256 entries for:\n  ${missing.join("\n  ")}`,
      );
    }
    console.log(
      `engines.lock.json covers all ${ALL_ARTIFACT_URLS.length} artifacts this script can download.`,
    );
    return;
  }

  fs.mkdirSync(enginesDir, { recursive: true });
  fs.mkdirSync(fontsDir, { recursive: true });
  fs.mkdirSync(cacheDir, { recursive: true });

  // Fonts (PT Sans, OFL)
  for (const font of ["PT_Sans-Web-Regular.ttf", "PT_Sans-Web-Bold.ttf"]) {
    const target = path.join(fontsDir, font);
    if (force || !fs.existsSync(target)) {
      console.log(`==> font ${font}`);
      await downloadFile(lock, `${PINNED.ptSansBase}/${font}`, target);
    }
  }
  const ofl = path.join(fontsDir, "OFL.txt");
  if (!fs.existsSync(ofl)) {
    await downloadFile(lock, `${PINNED.ptSansBase}/OFL.txt`, ofl);
  }

  // Fonts (Liberation Sans, OFL)
  const liberationFiles = [
    "LiberationSans-Regular.ttf",
    "LiberationSans-Italic.ttf",
    "LiberationSans-Bold.ttf",
    "LiberationSans-BoldItalic.ttf",
  ];
  const liberationMissing = liberationFiles.filter(
    (file) => force || !fs.existsSync(path.join(fontsDir, file)),
  );
  if (liberationMissing.length > 0) {
    const vendorDir = path.join(root, "chrome-extension", "public", "vendor", "standard_fonts");
    const stillMissing = [];
    for (const file of liberationMissing) {
      const local = path.join(vendorDir, file);
      if (fs.existsSync(local)) {
        console.log(`==> font ${file} (copy from chrome-extension vendor)`);
        fs.copyFileSync(local, path.join(fontsDir, file));
      } else {
        stillMissing.push(file);
      }
    }
    if (stillMissing.length > 0) {
      const archive = path.join(cacheDir, "liberation-fonts-ttf-2.1.5.tar.gz");
      await downloadFile(lock, PINNED.liberationUrl, archive);
      const tmpDir = path.join(cacheDir, "liberation-x");
      fs.rmSync(tmpDir, { recursive: true, force: true });
      fs.mkdirSync(tmpDir, { recursive: true });
      execFileSync("tar", ["-xzf", archive, "-C", tmpDir], { stdio: "inherit" });
      for (const file of stillMissing) {
        const found = findFileRecursive(tmpDir, file);
        if (!found) throw new Error(`liberation archive did not contain ${file}`);
        fs.copyFileSync(found, path.join(fontsDir, file));
      }
    }
  }

  if (fontsOnly) {
    console.log("Done (fonts only).");
    return;
  }

  // Tessdata models (platform-independent)
  const tessDataDir = path.join(enginesDir, "tesseract", "tessdata");
  fs.mkdirSync(tessDataDir, { recursive: true });
  for (const lang of PINNED.tessLangs) {
    const target = path.join(tessDataDir, `${lang}.traineddata`);
    if (force || !fs.existsSync(target)) {
      await downloadFile(lock, `${PINNED.tessDataBase}/${lang}.traineddata`, target);
    }
  }
  const lic = path.join(tessDataDir, "LICENSE.tessdata_fast.txt");
  if (!fs.existsSync(lic)) {
    await downloadFile(lock, PINNED.tessdataLicenseUrl, lic);
  }
  const pdfFont = path.join(tessDataDir, "pdf.ttf");
  if (!fs.existsSync(pdfFont)) {
    await downloadFile(lock, PINNED.pdfTtfUrl, pdfFont);
  }

  if (process.platform === "win32") {
    console.log(
      "Note: on Windows, run scripts/fetch-engines.ps1 to extract pdfium.dll, qpdf.exe, and tesseract.exe.",
    );
  }
  console.log("Done.");
}

function findFileRecursive(dir, targetName) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      const hit = findFileRecursive(full, targetName);
      if (hit) return hit;
    } else if (entry.name === targetName) {
      return full;
    }
  }
  return null;
}

main().catch((err) => {
  console.error(err?.message || err);
  process.exit(1);
});
