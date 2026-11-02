import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";

// The Rust coverage floors are a ratchet (E4.1): a measurement may raise them,
// never lower them. This pins the script that enforces it against a synthetic
// `cargo llvm-cov report --summary-only` table, so the rule is tested without a
// coverage run.
const root = path.resolve(__dirname, "..", "..");
const script = path.join(root, "scripts", "check-rust-coverage.mjs");
const baseline = JSON.parse(readFileSync(path.join(root, "scripts", "coverage-baseline.json"), "utf8")) as {
  version: number;
  rust: Record<string, number>;
};
const cleanups: string[] = [];

afterEach(() => {
  for (const directory of cleanups.splice(0)) rmSync(directory, { recursive: true, force: true });
});

/** One TOTAL row in the shape llvm-cov prints it. */
function summaryPath(regions: number, functions: number, lines: number): string {
  const directory = mkdtempSync(path.join(tmpdir(), "coverage-"));
  cleanups.push(directory);
  const header =
    "Filename                                       Regions    Missed Regions     Cover   Functions  Missed Functions  Executed       Lines      Missed Lines     Cover";
  const row = `TOTAL                                           ${1000}              ${100}    ${regions.toFixed(2)}%         ${100}                ${10}    ${functions.toFixed(2)}%        ${1000}              ${100}    ${lines.toFixed(2)}%`;
  const file = path.join(directory, "coverage-summary.txt");
  writeFileSync(file, `${header}\n${row}\n`);
  return file;
}

function run(file: string, ...flags: string[]): { status: number; output: string } {
  try {
    const output = execFileSync(process.execPath, [script, file, ...flags], { encoding: "utf8" });
    return { status: 0, output };
  } catch (caught) {
    const error = caught as { status?: number; stdout?: string; stderr?: string };
    return { status: error.status ?? 1, output: `${error.stdout ?? ""}${error.stderr ?? ""}` };
  }
}

describe("Rust coverage ratchet", () => {
  it("records three floors under a hundred", () => {
    expect(baseline.version).toBe(1);
    for (const metric of ["regions", "functions", "lines"]) {
      expect(baseline.rust[metric], metric).toBeGreaterThan(0);
      expect(baseline.rust[metric], metric).toBeLessThan(100);
    }
  });

  it("passes a measurement above every floor", () => {
    const result = run(summaryPath(99, 99, 99));
    expect(result.status).toBe(0);
    expect(result.output).toContain("floor");
  });

  it("fails a measurement below a floor and names the metric", () => {
    const floor = baseline.rust.lines;
    const result = run(summaryPath(99, 99, Math.max(0, floor - 5)));
    expect(result.status).toBe(1);
    expect(result.output).toContain("lines");
    expect(result.output).toContain("below the floor");
  });

  it("refuses to lower a floor through --update", () => {
    // --update writes the *repository* baseline, so a low measurement must be
    // rejected before anything is written: the file has to stay identical.
    const before = readFileSync(path.join(root, "scripts", "coverage-baseline.json"), "utf8");
    const result = run(summaryPath(1, 1, 1), "--update");
    expect(result.status).toBe(1);
    expect(result.output).toContain("may only rise");
    expect(readFileSync(path.join(root, "scripts", "coverage-baseline.json"), "utf8")).toBe(before);
  });

  it("reports a summary without a TOTAL row instead of passing it", () => {
    const directory = mkdtempSync(path.join(tmpdir(), "coverage-"));
    cleanups.push(directory);
    const file = path.join(directory, "coverage-summary.txt");
    writeFileSync(file, "no table here\n");
    const result = run(file);
    expect(result.status).toBe(2);
    expect(result.output).toContain("could not read the TOTAL row");
  });
});
