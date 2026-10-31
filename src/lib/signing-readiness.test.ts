import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";

const root = path.resolve(__dirname, "..", "..");
const script = path.join(root, "scripts", "check-signing-readiness.mjs");
const workflowPath = path.join(root, ".github", "workflows", "release.yml");
const docsPath = path.join(root, "docs", "signing.md");

const cleanups: string[] = [];
afterEach(() => {
  for (const directory of cleanups.splice(0)) rmSync(directory, { recursive: true, force: true });
});

/** A minimal copy of the signing inputs, so a negative case can break one line. */
function fixture(workflow: string, docs: string): string {
  const directory = mkdtempSync(path.join(tmpdir(), "signing-"));
  cleanups.push(directory);
  mkdirSync(path.join(directory, ".github", "workflows"), { recursive: true });
  mkdirSync(path.join(directory, "docs"), { recursive: true });
  writeFileSync(path.join(directory, ".github", "workflows", "release.yml"), workflow);
  writeFileSync(path.join(directory, "docs", "signing.md"), docs);
  return directory;
}

function run(directory?: string): { status: number; output: string } {
  try {
    const output = execFileSync(process.execPath, [script], {
      encoding: "utf8",
      env: { ...process.env, ...(directory ? { SIGNING_ROOT: directory } : {}) },
    });
    return { status: 0, output };
  } catch (caught) {
    const error = caught as { status?: number; stderr?: string; stdout?: string };
    return { status: error.status ?? 1, output: `${error.stdout ?? ""}${error.stderr ?? ""}` };
  }
}

describe("signing readiness (D10)", () => {
  it("passes the workflow and runbook invariants", () => {
    const result = run();
    expect(result.status).toBe(0);
    expect(result.output).toContain("signing readiness: OK");
    expect(result.output).toContain("3 providers");
  });

  it("pins the unsigned fallback, the preflight and the updater refresh", () => {
    const workflow = readFileSync(workflowPath, "utf8");
    expect(workflow).toContain("Check the Authenticode configuration");
    expect(workflow).toContain('provider="none"');
    expect(workflow).toContain("Refresh the updater signature over the signed installer");
    const docs = readFileSync(docsPath, "utf8");
    expect(docs).toContain("D10 runbook");
    expect(docs).toContain("Get-AuthenticodeSignature");
  });

  it("fails when a required SignPath secret disappears from the workflow", () => {
    const workflow = readFileSync(workflowPath, "utf8").replace(/SIGNPATH_ORGANIZATION_ID/g, "SIGNPATH_ORG");
    const directory = fixture(workflow, readFileSync(docsPath, "utf8"));
    const result = run(directory);
    expect(result.status).toBe(1);
    expect(result.output).toContain("SIGNPATH_ORGANIZATION_ID");
  });

  it("fails when the runbook drops the verification step", () => {
    const workflow = readFileSync(workflowPath, "utf8");
    const docs = readFileSync(docsPath, "utf8").replace(/signtool verify/g, "signtool");
    const directory = fixture(workflow, docs);
    const result = run(directory);
    expect(result.status).toBe(1);
    expect(result.output).toContain("signtool verify");
  });
});
