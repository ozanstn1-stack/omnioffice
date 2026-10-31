import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

// The desktop workflow is the release gate for every PR; its structure is
// pinned here because a silently removed concurrency block or timeout turns a
// cheap mistake into a full matrix run, and a misplaced artifact upload hides
// the only copy of a fuzz crash (E0).
const root = path.resolve(__dirname, "..", "..");
const read = (name: string) => readFileSync(path.join(root, ".github", "workflows", name), "utf8");
const desktop = read("desktop.yml");
const release = read("release.yml");
const linux = read("linux-package.yml");
const android = read("android.yml");
const beta = read("beta.yml");

function jobBlocks(): { name: string; body: string }[] {
  const jobs: { name: string; body: string }[] = [];
  const start = desktop.indexOf("\njobs:\n");
  const text = desktop.slice(start + 1);
  const matches = [...text.matchAll(/^ {2}([a-z-]+):\n/gm)];
  matches.forEach((match, index) => {
    const bodyStart = match.index ?? 0;
    const bodyEnd = index + 1 < matches.length ? matches[index + 1].index : text.length;
    jobs.push({ name: match[1], body: text.slice(bodyStart, bodyEnd) });
  });
  return jobs;
}

describe("desktop workflow structure (E0)", () => {
  it("cancels superseded PR runs but never a master run", () => {
    expect(desktop).toMatch(/\nconcurrency:\n {2}group: desktop-\$\{\{ github\.ref \}\}\n/);
    expect(desktop).toContain("cancel-in-progress: ${{ github.event_name == 'pull_request' }}");
  });

  it("gives every job a wall-clock timeout", () => {
    for (const job of jobBlocks()) {
      expect(job.body, job.name).toMatch(/\n {4}timeout-minutes: \d+/);
    }
  });

  it("uploads fuzz crash artifacts after the last fuzz target", () => {
    const upload = desktop.indexOf("Upload crash artifacts");
    const pdfParser = desktop.indexOf("name: PDF parser");
    expect(upload).toBeGreaterThan(0);
    expect(pdfParser).toBeGreaterThan(0);
    expect(upload).toBeGreaterThan(pdfParser);
  });
});

describe("release builds are lock-pinned (E0)", () => {
  it("runs a cargo metadata --locked preflight before every release tauri build", () => {
    for (const [name, text] of [
      ["release.yml", release],
      ["linux-package.yml", linux],
      ["android.yml", android],
      ["beta.yml", beta],
    ] as const) {
      expect(text, name).toContain("cargo metadata --locked");
      const lines = text.split(/\r?\n/);
      const buildIndex = lines.findIndex(
        (line) => !line.trim().startsWith("#") && /tauri\s+(android\s+)?build/.test(line) && !/--debug/.test(line),
      );
      expect(buildIndex, `${name} release build line`).toBeGreaterThan(-1);
      const preflightIndex = lines.findIndex((line) => line.includes("cargo metadata --locked"));
      expect(preflightIndex, name).toBeLessThan(buildIndex);
    }
  });

  it("keeps a single advisory policy (no cargo audit --ignore duplicate)", () => {
    expect(desktop).not.toContain("cargo audit \\");
    expect(desktop).toContain("cargo deny check");
  });
});
