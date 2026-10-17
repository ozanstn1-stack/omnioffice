#!/usr/bin/env node
// Runs the compatibility corpus scorer and writes the report
// (target/compat/report.json), the nightly artifact.
//
//   node scripts/compat-score.mjs                # score, exit 1 on review
//   node scripts/compat-score.mjs --update-golden
//
// When LibreOffice is available (`PDFSAK_SOFFICE` or `soffice` on PATH) every
// corpus sample is also converted to PDF with it and the report carries the
// external reference score; without LibreOffice the golden (regression) score
// stands alone. The reference score is informational: it depends on the
// LibreOffice version and is not calibrated, so it never drives the exit code.

import { spawnSync } from "node:child_process";
import { appendFileSync, existsSync, mkdirSync, readFileSync, renameSync, rmSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const corpus = path.join(root, "tests", "corpus");
const outDir = path.join(root, "target", "compat");
const reportPath = path.join(outDir, "report.json");
const referenceDir = path.join(outDir, "reference");

const isWindows = process.platform === "win32";

function run(command, args, { shell = false, timeout = 0 } = {}) {
  // soffice.com only works through cmd.exe on Windows; its path contains a
  // space, so it must be quoted for the shell.
  const result = spawnSync(shell ? `"${command}"` : command, args, {
    cwd: root,
    stdio: "inherit",
    shell,
    timeout,
  });
  return result.status ?? 2;
}

function sleepSync(milliseconds) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, milliseconds);
}

function findSoffice() {
  const override = process.env.PDFSAK_SOFFICE;
  if (override && existsSync(override)) return override;
  const probe = spawnSync("soffice", ["--version"], { stdio: "ignore", timeout: 20000 });
  if (probe.status === 0) return "soffice";
  // The Windows installer does not put soffice on PATH, and soffice.exe is the
  // GUI launcher that never returns for a console call; soffice.com is the
  // console one.
  if (isWindows) {
    for (const name of ["soffice.com", "soffice.exe"]) {
      const installed = path.join("C:\\Program Files\\LibreOffice\\program", name);
      if (existsSync(installed)) return installed;
    }
  }
  return null;
}

// LibreOffice writes `<stem>.pdf`, which would make writer-basic.docx and
// writer-basic.odt collide; convert into a scratch directory and rename the
// result to `<file>.pdf` so every sample keeps its own reference.
function buildReferences(soffice) {
  const entries = JSON.parse(readFileSync(path.join(corpus, "manifest.json"), "utf8"));
  rmSync(referenceDir, { recursive: true, force: true });
  mkdirSync(referenceDir, { recursive: true });
  // A unique scratch root per run: soffice keeps profile handles open for a
  // moment after exiting, and a locked directory must not fail the run.
  const scratchRoot = path.join(outDir, "reference-scratch", `run-${Date.now()}`);
  mkdirSync(scratchRoot, { recursive: true });
  let converted = 0;
  for (const [index, entry] of entries.entries()) {
    if (entry.format === "udf") continue; // LibreOffice cannot open UDF
    const scratch = path.join(scratchRoot, String(index));
    mkdirSync(scratch, { recursive: true });
    const convert = () =>
      run(
        soffice,
        [
          "--headless",
          "--convert-to",
          "pdf",
          "--outdir",
          `"${scratch}"`,
          `"${path.join(corpus, entry.file)}"`,
        ],
        { shell: isWindows, timeout: 180000 },
      );
    let status = convert();
    let produced = path.join(scratch, entry.file.replace(/\.[^.]+$/, ".pdf"));
    if (status !== 0 || !existsSync(produced)) {
      // A previous instance that is still shutting down makes the call a
      // no-op; one retry after a short pause recovers.
      sleepSync(3000);
      status = convert();
      produced = path.join(scratch, entry.file.replace(/\.[^.]+$/, ".pdf"));
    }
    if (status !== 0 || !existsSync(produced)) {
      console.warn(`referans üretilemedi: ${entry.file}`);
      continue;
    }
    renameSync(produced, path.join(referenceDir, `${entry.file}.pdf`));
    converted += 1;
  }
  console.log(`LibreOffice referansı: ${converted} PDF`);
  return converted > 0 ? referenceDir : null;
}

if (process.argv.includes("--update-golden")) {
  process.exit(run("cargo", ["run", "-q", "-p", "compat", "--", "update-golden"]));
}

mkdirSync(outDir, { recursive: true });
let reference = null;
const soffice = findSoffice();
if (soffice) {
  reference = buildReferences(soffice);
} else {
  console.log("LibreOffice yok: referans puanı atlanıyor, golden puanı raporlanıyor");
}

const args = ["run", "-q", "-p", "compat", "--", "score", "--out", reportPath];
if (reference) args.push("--reference", reference);
const status = run("cargo", args);

const report = JSON.parse(readFileSync(reportPath, "utf8"));
if (process.env.GITHUB_STEP_SUMMARY) {
  const lines = [
    "## Uyumluluk puanı",
    "",
    `Eşik: yapısal ${report.structural_floor}%, görsel ${report.threshold}%`,
    "",
    "| Biçim | Yapısal | Görsel | Referans | Örnek |",
    "| --- | --- | --- | --- | --- |",
  ];
  const cell = (value) => (value === null || value === undefined ? "-" : `${value.toFixed(1)}%`);
  for (const [format, score] of Object.entries(report.formats)) {
    lines.push(
      `| ${format} | ${cell(score.structural)} | ${cell(score.visual)} | ${cell(score.reference)} | ${score.samples} |`,
    );
  }
  if (report.review.length > 0) {
    lines.push("", `**İnceleme listesi:** ${report.review.join(", ")}`);
  }
  if (report.missing_goldens.length > 0) {
    lines.push("", `**Golden eksik:** ${report.missing_goldens.join(", ")}`);
  }
  lines.push("", reference ? "Referans: LibreOffice" : "Referans: yok (LibreOffice kurulu değil)");
  appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${lines.join("\n")}\n`);
}

process.exit(status);
