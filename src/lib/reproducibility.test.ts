import { execFileSync } from "node:child_process";
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";

const root = path.resolve(__dirname, "..", "..");
const script = path.join(root, "scripts", "check-reproducibility-inputs.mjs");
const cleanups: string[] = [];

afterEach(() => {
  for (const directory of cleanups.splice(0)) rmSync(directory, { recursive: true, force: true });
});

function run(directory: string): { status: number; output: string } {
  try {
    const output = execFileSync(process.execPath, [script], {
      encoding: "utf8",
      env: { ...process.env, REPRO_ROOT: directory },
    });
    return { status: 0, output };
  } catch (caught) {
    const error = caught as { status?: number; stdout?: string; stderr?: string };
    return { status: error.status ?? 1, output: `${error.stdout ?? ""}${error.stderr ?? ""}` };
  }
}

/** A copy of the pinned inputs, so a negative case can break one file. */
function copyInputs(): string {
  const directory = mkdtempSync(path.join(tmpdir(), "repro-"));
  cleanups.push(directory);
  for (const entry of [".github", "crates", "src-tauri", "scripts", "docs"]) {
    cpSync(path.join(root, entry), path.join(directory, entry), { recursive: true });
  }
  for (const file of ["Cargo.toml", "Cargo.lock", "package-lock.json", ".gitignore"]) {
    cpSync(path.join(root, file), path.join(directory, file));
  }
  return directory;
}

describe("reproducibility inputs (D11)", () => {
  it("passes the repository as it stands", () => {
    const result = run(root);
    expect(result.status).toBe(0);
    expect(result.output).toContain("reproducibility inputs: OK");
    expect(result.output).toContain("7 manifests");
  });

  it("fails when the Cargo lockfile disappears", () => {
    const directory = copyInputs();
    rmSync(path.join(directory, "Cargo.lock"));
    const result = run(directory);
    expect(result.status).toBe(1);
    expect(result.output).toContain("Cargo.lock is missing");
  });

  it("fails when an action loses its commit pin", () => {
    const directory = copyInputs();
    const workflow = path.join(directory, ".github", "workflows", "release.yml");
    const text = readFileSync(workflow, "utf8").replace(
      /uses: (actions\/checkout@[0-9a-f]{40})/,
      "uses: actions/checkout@v7",
    );
    writeFileSync(workflow, text);
    const result = run(directory);
    expect(result.status).toBe(1);
    expect(result.output).toContain("not pinned to a commit SHA");
  });

  it("fails when a tauri release build loses its lockfile preflight", () => {
    const directory = copyInputs();
    const workflow = path.join(directory, ".github", "workflows", "release.yml");
    const text = readFileSync(workflow, "utf8").replace(
      /cargo metadata --locked --format-version 1 \| Out-Null/,
      "cargo metadata",
    );
    writeFileSync(workflow, text);
    const result = run(directory);
    expect(result.status).toBe(1);
    expect(result.output).toContain("cargo metadata --locked");
  });

  it("fails when the reproducibility report loses its gaps section", () => {
    const directory = copyInputs();
    const report = path.join(directory, "docs", "reproducibility.md");
    writeFileSync(report, readFileSync(report, "utf8").replace("## What is not reproducible yet", "## Gaps"));
    const result = run(directory);
    expect(result.status).toBe(1);
    expect(result.output).toContain("What is not reproducible yet");
  });
});
