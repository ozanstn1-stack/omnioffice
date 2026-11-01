import { readFileSync, readdirSync } from "node:fs";
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
  // Job keys may contain digits (`e2e`, `e2e-windows`); the old character class
  // silently skipped them, so the timeout rule never reached the E2E jobs.
  const matches = [...text.matchAll(/^ {2}([a-z0-9-]+):\n/gm)];
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

  it("runs the Android clippy gate in review, not only at tag time (v7.2.1)", () => {
    // The release job runs `cargo clippy -- -D warnings` for the Android
    // target; when the PR job only ran `cargo check`, a dead-code warning that
    // exists on Android alone (v7.2.0's unused keyring constant) shipped in the
    // review and failed the tagged build.
    const job = jobBlocks().find((entry) => entry.name === "android-target");
    expect(job, "android-target job").toBeDefined();
    expect(job?.body).toContain(
      "cargo clippy -p pdf-swiss-army-knife --lib --target aarch64-linux-android -- -D warnings",
    );
    expect(job?.body).not.toMatch(/cargo check -p pdf-swiss-army-knife --target aarch64-linux-android/);
  });

  it("keeps a single advisory policy (no cargo audit --ignore duplicate)", () => {
    expect(desktop).not.toContain("cargo audit \\");
    expect(desktop).toContain("cargo deny check");
  });
});

describe("coverage is measured, published and only raised (E4)", () => {
  it("publishes the coverage report from the frontend job", () => {
    const job = jobBlocks().find((entry) => entry.name === "frontend");
    expect(job, "frontend job").toBeDefined();
    expect(job?.body).toContain("npm run test:coverage");
    expect(job?.body).toContain("name: coverage-frontend");
    expect(job?.body).toContain("path: coverage/");
  });

  it("never lowers a vitest coverage floor", () => {
    // The floors are a ratchet: this pins the values measured on 2026-11-01
    // (statements 78.4, branches 80.2, functions 65.9, lines 78.4), so a
    // failing build cannot be made green by relaxing the gate.
    const source = readFileSync(path.join(root, "vite.config.ts"), "utf8");
    const block = source.slice(source.indexOf("thresholds: {"));
    const floor = (name: string) => {
      const match = new RegExp(`${name}: (\\d+)`).exec(block);
      expect(match, `${name} threshold`).not.toBeNull();
      return Number(match?.[1] ?? 0);
    };
    expect(floor("statements")).toBeGreaterThanOrEqual(77);
    expect(floor("branches")).toBeGreaterThanOrEqual(79);
    expect(floor("functions")).toBeGreaterThanOrEqual(64);
    expect(floor("lines")).toBeGreaterThanOrEqual(77);
  });
});

describe("Linux runners are pinned (E5)", () => {
  it("no workflow falls back to ubuntu-latest", () => {
    const dir = path.join(root, ".github", "workflows");
    for (const file of readdirSync(dir).filter((name) => name.endsWith(".yml"))) {
      const text = readFileSync(path.join(dir, file), "utf8");
      expect(text, file).not.toContain("runs-on: ubuntu-latest");
    }
  });
});

describe("supply chain and toolchain pinning (E5)", () => {
  it("pins the toolchain without letting the file mask the MSRV job", () => {
    const toolchain = readFileSync(path.join(root, "rust-toolchain.toml"), "utf8");
    expect(toolchain).toContain('channel = "stable"');
    // A toolchain file takes precedence over the installed default: without the
    // override the MSRV job would test stable and the gate would be a lie.
    const msrv = jobBlocks().find((entry) => entry.name === "msrv");
    expect(msrv, "msrv job").toBeDefined();
    expect(msrv?.body).toContain('RUSTUP_TOOLCHAIN: "1.89.0"');
  });

  it("keeps the fuzz jobs on nightly despite the toolchain file", () => {
    const fuzz = jobBlocks().find((entry) => entry.name === "fuzz");
    expect(fuzz, "fuzz job").toBeDefined();
    expect(fuzz?.body).toContain("RUSTUP_TOOLCHAIN: nightly");
    expect(read("fuzz-pr.yml")).toContain("RUSTUP_TOOLCHAIN: nightly");
  });

  it("caches the engine downloads by lock hash", () => {
    const key = "hashFiles('scripts/engines.lock.json')";
    const rust = jobBlocks().find((entry) => entry.name === "rust");
    expect(rust, "rust job").toBeDefined();
    expect(rust?.body).toContain(key);
    expect(rust?.body).toContain("path: src-tauri/resources/engines");
    const windowsE2e = jobBlocks().find((entry) => entry.name === "e2e-windows");
    expect(windowsE2e, "e2e-windows job").toBeDefined();
    expect(windowsE2e?.body).toContain(key);
  });

  it("analyzes the frontend and the workflows themselves", () => {
    const codeql = read("codeql.yml");
    expect(codeql).toContain("javascript-typescript");
    expect(codeql).toContain("actions");
    expect(codeql).toMatch(/github\/codeql-action\/init@[0-9a-f]{40}/);
    expect(codeql).toMatch(/github\/codeql-action\/analyze@[0-9a-f]{40}/);
    expect(codeql).toContain("security-events: write");
  });

  it("declares the node version the suite is built and tested with", () => {
    const nvmrc = readFileSync(path.join(root, ".nvmrc"), "utf8").trim();
    const pkg = JSON.parse(readFileSync(path.join(root, "package.json"), "utf8")) as {
      engines?: { node?: string };
    };
    expect(nvmrc).toBe("22");
    expect(pkg.engines?.node).toBe(">=20");
  });
});

describe("i18n audit is a blocking gate (E4.5)", () => {
  it("runs the audit with --check in the lint job", () => {
    const lint = jobBlocks().find((entry) => entry.name === "lint");
    expect(lint, "lint job").toBeDefined();
    expect(lint?.body).toContain("npm run i18n:audit -- --check");
  });

  it("records a hardcoded-JSX baseline the audit compares against", () => {
    // The baseline is a ratchet: moving a string into the dictionary lowers it
    // with `npm run i18n:audit -- --update`, and the audit fails above it.
    const baseline = JSON.parse(readFileSync(path.join(root, "scripts", "i18n-baseline.json"), "utf8")) as {
      version: number;
      files: Record<string, number>;
    };
    expect(baseline.version).toBe(1);
    const total = Object.values(baseline.files).reduce((sum, count) => sum + count, 0);
    expect(total).toBeLessThanOrEqual(80);
  });
});
