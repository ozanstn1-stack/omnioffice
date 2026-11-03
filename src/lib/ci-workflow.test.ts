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

/** The jobs of android.yml, in file order (jobBlocks() reads desktop.yml). */
function androidJobs(): { name: string; body: string }[] {
  const text = android.slice(android.indexOf("\njobs:\n") + 1);
  const matches = [...text.matchAll(/^ {2}([a-z0-9-]+):\n/gm)];
  return matches.map((match, index) => ({
    name: match[1],
    body: text.slice(match.index ?? 0, index + 1 < matches.length ? matches[index + 1].index : text.length),
  }));
}

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
    const key = "hashFiles('scripts/engines.lock.json', 'scripts/fetch-engines.ps1')";
    const rust = jobBlocks().find((entry) => entry.name === "rust");
    expect(rust, "rust job").toBeDefined();
    expect(rust?.body).toContain(key);
    expect(rust?.body).toContain("path: src-tauri/resources/engines");
    const windowsE2e = jobBlocks().find((entry) => entry.name === "e2e-windows");
    expect(windowsE2e, "e2e-windows job").toBeDefined();
    expect(windowsE2e?.body).toContain(key);
  });

  it("restores the engine cache before fetching them, everywhere but the nightly", () => {
    // ~100 MB of pinned third-party artifacts were downloaded again on every
    // release and beta run. The key carries the lock *and* the fetch script, so
    // either one changing invalidates the entry; a cache restored after the
    // fetch would cache nothing, which is why the order is asserted.
    const key =
      "key: engines-${{ runner.os }}-${{ hashFiles('scripts/engines.lock.json', 'scripts/fetch-engines.ps1') }}";
    const restorers = [
      ["desktop.yml", desktop],
      ["release.yml", release],
      ["beta.yml", beta],
    ] as const;
    for (const [name, workflow] of restorers) {
      expect(workflow, name).toContain(key);
      expect(workflow, name).toContain("path: src-tauri/resources/engines");
      const cache = workflow.search(/Restore (cached )?engine binaries|Cache engine binaries/);
      expect(cache, name).toBeGreaterThan(-1);
      expect(cache, name).toBeLessThan(workflow.indexOf("npm run engines:fetch"));
    }
  });

  it("publishing workflows only read the engine cache", () => {
    // Trust boundary: the entry is written by the Desktop run on the default
    // branch and only read by the jobs that publish. A publishing run that
    // stores its own entry would seed a cache other runs then trust.
    for (const [name, workflow] of [
      ["release.yml", release],
      ["beta.yml", beta],
    ] as const) {
      expect(workflow, name).toMatch(/uses: actions\/cache\/restore@[0-9a-f]{40}/);
      expect(workflow, name).not.toContain("uses: actions/cache@");
      expect(workflow, name).not.toContain("actions/cache/save@");
    }
  });

  it("pins one actions/cache commit for the engine cache", () => {
    // Only the steps that touch the engine directory count: the same workflow
    // caches other things (the bench baseline) on its own action version.
    const shas = new Set<string>();
    for (const name of readdirSync(path.join(root, ".github", "workflows"))) {
      const text = read(name);
      if (!text.includes("path: src-tauri/resources/engines")) continue;
      for (const step of text.split(/\n(?= {6}- )/)) {
        if (!step.includes("path: src-tauri/resources/engines")) continue;
        for (const match of step.matchAll(/actions\/cache(?:\/restore|\/save)?@([0-9a-f]{40})/g)) {
          shas.add(match[1]);
        }
      }
    }
    // Dependabot bumping one file is exactly what this catches: a split would
    // leave one workflow writing entries the others cannot see.
    expect([...shas]).toHaveLength(1);
  });

  it("nightly stays the uncached upstream-drift canary", () => {
    // Several pins point at mutable `main` URLs; caching the nightly would hide
    // the drift it exists to catch.
    const nightly = read("nightly.yml");
    expect(nightly).toContain("npm run engines:fetch");
    expect(nightly).toContain("Deliberately uncached");
    expect(nightly).not.toContain("path: src-tauri/resources/engines");
    expect(nightly).not.toContain("actions/cache");
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
      attributes: Record<string, number>;
      props: Record<string, number>;
      objects: Record<string, number>;
    };
    expect(baseline.version).toBe(4);
    const total = (section: Record<string, number>) => Object.values(section).reduce((sum, count) => sum + count, 0);
    expect(total(baseline.files)).toBeLessThanOrEqual(77);
    // 2026-11-03: template attributes count now (`alt={`Page ${page}`}`), so the
    // ceiling rose from 9 to 11 - the two shortcut hints are the addition.
    expect(total(baseline.attributes)).toBeLessThanOrEqual(11);
    expect(total(baseline.props)).toBeLessThanOrEqual(6);
    expect(total(baseline.objects)).toBe(0);
  });
});

describe("Rust coverage and Android build shape (E4.1, E5)", () => {
  it("measures Rust coverage and publishes the report instead of guessing a floor", () => {
    const coverage = jobBlocks().find((entry) => entry.name === "rust-coverage");
    expect(coverage, "rust-coverage job").toBeDefined();
    expect(coverage?.body).toContain("cargo llvm-cov --lcov");
    expect(coverage?.body).toContain("cargo llvm-cov report --summary-only");
    // Measuring without a floor would let a regression pass silently.
    expect(coverage?.body).toContain("node scripts/check-rust-coverage.mjs coverage-summary.txt");
    expect(coverage?.body).toContain("name: coverage-rust");
    expect(coverage?.body).toContain("node scripts/workspace-crates.mjs release");
  });

  it("builds both Android artifacts in one Gradle run per ABI", () => {
    // `--apk` and `--aab` are independent flags: one Gradle run per ABI produces
    // both, and the two ABIs are separate matrix jobs.
    const calls = [...android.matchAll(/tauri android build -- ([^\n]+)/g)].map((match) => match[1].trim());
    const release = calls.filter((call) => call.includes("--aab"));
    expect(release).toHaveLength(1);
    expect(release[0]).toContain("--apk");
    expect(release[0]).toContain("--target ${{ matrix.target }}");
    expect(android).toContain("OmniOffice-Android-$version-${{ matrix.abi }}.aab");
    expect(android).toContain("sha256sum *.apk *.aab");
  });

  it("splits the Android build by ABI and publishes from one assemble job", () => {
    const jobs = androidJobs();
    const build = jobs.find((entry) => entry.name === "build");
    const tests = jobs.find((entry) => entry.name === "tests");
    const assemble = jobs.find((entry) => entry.name === "assemble");
    expect(build && tests && assemble).toBeTruthy();

    // Both arms run even when one fails: in a two-run budget, the second
    // failure reason is worth more than a cancelled runner.
    expect(build?.body).toContain("fail-fast: false");
    expect(build?.body).toContain("abi: arm64-v8a");
    expect(build?.body).toContain("abi: armeabi-v7a");
    expect(build?.body).toContain("${{ matrix.abi }}");
    expect(build?.body).toContain("Require the release keystore");
    expect(build?.body).toContain("name: android-abi-${{ matrix.abi }}");

    // The emulator is x86_64; only that library is built, and no secret is used.
    expect(tests?.body).toContain("-PabiList=x86_64 -ParchList=x86_64 -PtargetList=x86_64");
    expect(tests?.body).not.toContain("secrets.");
    expect(tests?.body).not.toContain("ANDROID_KEYSTORE");
    expect(android).not.toContain("--target i686");

    // One place verifies and publishes, and only after the tests pass.
    expect(assemble?.body).toContain("needs: [build, tests]");
    expect(assemble?.body).toMatch(/actions\/download-artifact@[0-9a-f]{40}/);
    expect(assemble?.body).toContain("pattern: android-abi-*");
    expect(assemble?.body).toContain("merge-multiple: true");
    const order = ["sha256sum *.apk *.aab", "verify-android-abis.sh", "name: android-apks", "gh release upload"];
    const positions = order.map((needle) => assemble?.body.indexOf(needle) ?? -1);
    for (const position of positions) expect(position).toBeGreaterThan(-1);
    expect([...positions].sort((a, b) => a - b)).toEqual(positions);
    // The tag-gated steps live only here now.
    expect(assemble?.body).toContain("Verify release identity");
    expect(assemble?.body).toContain("Attach Android assets to the release");
    expect(build?.body).not.toContain("Attach Android assets");
    // Writing is needed only where the release is attached.
    expect(android).toContain("permissions:\n  contents: read");
  });

  it("runs the Android build on demand for a pull request, without touching the tag path", () => {
    // Only `labeled`: `opened`/`synchronize` would add skipped Android checks to
    // every PR and break the "19 checks" count.
    expect(android).toMatch(/pull_request:\s*\n\s*types: \[labeled\]/);
    // Comments may name the events that are deliberately absent; the triggers
    // themselves may not carry them.
    const triggers = android
      .slice(android.indexOf("on:"), android.indexOf("\npermissions:"))
      .split("\n")
      .filter((line) => !line.trim().startsWith("#"))
      .join("\n");
    for (const event of ["opened", "synchronize", "reopened"]) expect(triggers).not.toContain(event);
    expect(android).not.toContain("pull_request_target");
    // Both jobs stay dormant for any other label.
    const guard = "github.event_name != 'pull_request' || github.event.label.name == 'android-ci'";
    for (const job of ["build", "tests", "assemble", "version-consistency"]) {
      const block = androidJobs().find((entry) => entry.name === job);
      expect(block, `${job} job`).toBeDefined();
      expect(block?.body).toContain(guard);
    }
    // The release keystore must never reach a pull request: it builds debug-signed.
    expect(android).toContain("github.event_name != 'pull_request' && secrets.ANDROID_KEYSTORE_BASE64 || ''");
    expect(android).toContain("if: env.ANDROID_KEYSTORE_BASE64 != '' && github.event_name != 'pull_request'");
    // The tag-gated steps are untouched: their real test is the next release tag.
    expect(android.split("startsWith(github.ref, 'refs/tags/v')").length - 1).toBeGreaterThanOrEqual(3);
    // A pull request builds unsigned, so the identity is only demanded elsewhere.
    expect(android).toContain(
      "verify-android-abis.sh release-artifacts ${{ github.event_name != 'pull_request' && '--require-signature' || '' }}",
    );
  });

  it("checks the ABI matrix on the artifacts and publishes the checksum list", () => {
    // The per-ABI libraries and the signing identity are in the files, not in
    // the build log: the workflow has to read them before publishing.
    expect(android).toContain("bash .github/scripts/verify-android-abis.sh release-artifacts");
    // A checksum list the release attaches but the artifact omits cannot be
    // verified by anyone downloading the run.
    const upload = android.slice(android.indexOf("name: android-apks"));
    expect(upload.slice(0, 400)).toContain("release-artifacts/SHA256SUMS-android.txt");
    // The verification has to run before the artifacts are uploaded.
    expect(android.indexOf("verify-android-abis.sh")).toBeLessThan(android.indexOf("name: android-apks"));
  });
});
