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
      literal: number;
    };
    // 2026-11-04: the baseline grew a fifth field, the count of deliberate
    // literals a marker silences, so the schema is v5.
    expect(baseline.version).toBe(5);
    const total = (section: Record<string, number>) => Object.values(section).reduce((sum, count) => sum + count, 0);
    // 2026-11-04: conditionals, logical fallbacks, JSX child expressions and the
    // strings inside a template's interpolations count now, so every ceiling
    // rose on purpose (50 -> 73, 11 -> 20, 6 -> 8). Translating what that
    // measurement found, marking the structured-reference placeholder and then
    // translating the last four leaks (an Impress hint, the autosave units, a
    // form-field default and a page-number example) brought it to 58/12/6.
    expect(total(baseline.files)).toBeLessThanOrEqual(58);
    expect(total(baseline.attributes)).toBeLessThanOrEqual(12);
    expect(total(baseline.props)).toBeLessThanOrEqual(6);
    // The object scan reached zero and stays there.
    expect(total(baseline.objects)).toBe(0);
  });

  it("caps the deliberate literals a marker may silence", () => {
    // A marker is a reviewed decision, not a way to pass: it needs a reason,
    // it silences exactly one finding, and the total is both ratcheted and
    // hard-capped so "just mark it" never becomes the easy path.
    const baseline = JSON.parse(readFileSync(path.join(root, "scripts", "i18n-baseline.json"), "utf8")) as {
      literal: number;
    };
    const module = readFileSync(path.join(root, "scripts", "lib", "i18n-jsx.mjs"), "utf8");
    const cap = Number(/export const LITERAL_HARD_CAP = (\d+);/.exec(module)?.[1]);
    expect(cap).toBeGreaterThan(0);
    // Three markers today: the Calc structured-reference placeholder, the link
    // dialog's sample address and a filename example's extensions. Each one is
    // a conversation, not a commit.
    expect(baseline.literal).toBe(3);
    expect(baseline.literal).toBeLessThanOrEqual(cap);
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

describe("OpenSSF Scorecard (E5)", () => {
  const scorecard = read("scorecard.yml");

  it("grades the repository on a schedule, on master and on a protection change", () => {
    // A score nobody refreshes is a claim, not a measurement: the schedule and
    // the push to master keep it current, and a branch-protection change is
    // exactly when one of the graded checks moves.
    expect(scorecard).toMatch(/\n {2}branch_protection_rule:\n/);
    expect(scorecard).toMatch(/\n {2}schedule:\n {4}- cron: "[0-9 ]+ \* \* [0-9]"\n/);
    expect(scorecard).toMatch(/\n {2}push:\n {4}branches: \["master"\]\n/);
    expect(scorecard).toMatch(/\n {2}workflow_dispatch:\n/);
  });

  it("only runs on a pull request when this workflow itself changes", () => {
    // The action rejects any other event off the default branch, so a filtered
    // pull_request trigger is the only way to exercise it before a merge. The
    // path filter is what keeps that from adding a check to every PR.
    expect(scorecard).toMatch(/\n {2}pull_request:\n {4}paths: \["\.github\/workflows\/scorecard\.yml"\]\n/);
  });

  it("keeps the analysis least-privilege and pinned", () => {
    // The publisher validates the workflow: top-level permissions read-only,
    // no `run:` step, no top-level `env`/`defaults`, allowed actions only.
    expect(scorecard).toMatch(/\npermissions:\n {2}contents: read\n/);
    expect(scorecard).not.toMatch(/^env:/m);
    expect(scorecard).not.toMatch(/^defaults:/m);
    expect(scorecard).not.toMatch(/^\s+-? ?run:/m);
    for (const permission of ["security-events: write", "id-token: write", "contents: read", "actions: read"]) {
      expect(scorecard, permission).toContain(permission);
    }
    expect(scorecard).toContain("runs-on: ubuntu-24.04");
    expect(scorecard).toMatch(/\n {4}timeout-minutes: \d+/);
    // A checkout that leaves the token in .git/config hands the analysis a
    // credential it does not need.
    expect(scorecard).toContain("persist-credentials: false");
    for (const line of scorecard.split("\n").filter((entry) => entry.includes("uses:"))) {
      // A 40-character commit SHA, never a tag: hex digests start with digits,
      // so only the length of the whole ref tells a pin from `@v3`.
      expect(line, line).toMatch(/@[0-9a-f]{40}(\s|$)/);
    }
  });

  it("publishes the score and uploads the report", () => {
    expect(scorecard).toContain("publish_results: true");
    expect(scorecard).toContain("results_format: sarif");
    expect(scorecard).toContain("results_file: results.sarif");
    expect(scorecard).toContain("github/codeql-action/upload-sarif@");
    expect(scorecard).toContain("sarif_file: results.sarif");
  });

  it("uses one codeql-action commit across code scanning and scorecard", () => {
    // Dependabot updates one file at a time; a split would leave the two SARIF
    // uploads on different versions of the same action.
    const shas = new Set<string>();
    for (const text of [read("codeql.yml"), scorecard]) {
      for (const match of text.matchAll(/github\/codeql-action\/[\w-]+@([0-9a-f]{40})/g)) {
        shas.add(match[1]);
      }
    }
    expect([...shas]).toHaveLength(1);
  });
});

describe("auto-tag reads the tagged commit without checking it out (E5)", () => {
  const autoTag = read("auto-tag.yml");

  it("never checks out the triggering run's commit", () => {
    // A `workflow_run` checkout is the shape of the fork-code-with-secrets
    // attack and OpenSSF Scorecard flags it (Dangerous-Workflow). The job only
    // needs four files, so it must not have a working tree at all.
    expect(autoTag).not.toContain("actions/checkout@");
    expect(autoTag).toContain("Dangerous-Workflow");
  });

  it("reads each version file from the commit it is about to tag", () => {
    // The version gate and the tag have to describe one commit: fetching the
    // files at TARGET_SHA is what keeps them from drifting apart.
    expect(autoTag).toContain("Accept: application/vnd.github.raw");
    expect(autoTag).toContain("repos/$REPO/contents/$1?ref=$TARGET_SHA");
    for (const file of [
      "package.json",
      "src-tauri/tauri.conf.json",
      "Cargo.toml",
      "src-tauri/gen/android/app/tauri.properties",
    ]) {
      expect(autoTag, file).toContain(file);
    }
  });

  it("keeps the gate on a push from this repository", () => {
    // A fork's pull-request run reports `pull_request`, and the repository
    // comparison states the boundary even if that changed.
    expect(autoTag).toContain("github.event.workflow_run.event == 'push'");
    expect(autoTag).toContain("github.event.workflow_run.head_repository.full_name == github.repository");
  });
});

describe("GitHub token permissions (E5)", () => {
  const workflows = readdirSync(path.join(root, ".github", "workflows")).filter((name) => name.endsWith(".yml"));

  type JobPermissions = Record<string, { permissions?: Record<string, string> }>;

  function parsed(name: string): { permissions: Record<string, string>; jobs: JobPermissions } {
    // The workflows are simple enough for a YAML-free read: the two blocks that
    // matter are the top-level `permissions:` mapping and each job's.
    const text = read(name);
    // Comment lines inside the block are skipped: they explain the narrowing.
    const top = text.match(/\npermissions:\n((?:(?: {2}#.*| {2}[\w-]+: \w+)\n)+)/);
    const permissions: Record<string, string> = {};
    for (const line of (top?.[1] ?? "").split("\n")) {
      const match = line.match(/^ {2}([\w-]+): (\w+)$/);
      if (match) permissions[match[1]] = match[2];
    }
    const jobs: JobPermissions = {};
    for (const block of text.split(/\n(?= {2}[\w-]+:\n)/)) {
      const name = block.match(/^ {2}([\w-]+):\n/)?.[1];
      if (!name) continue;
      const jobPerms: Record<string, string> = {};
      for (const line of block.split("\n")) {
        const match = line.match(/^ {6}([\w-]+): (\w+)$/);
        if (match) jobPerms[match[1]] = match[2];
      }
      if (block.includes("permissions:")) jobs[name] = { permissions: jobPerms };
    }
    return { permissions, jobs };
  }

  it("never grants a write permission at the workflow level", () => {
    // Scorecard's Token-Permissions check, and the reason it matters: a
    // workflow-level `contents: write` hands every job the repository token's
    // write scope, including the ones that only read files.
    for (const name of workflows) {
      const { permissions } = parsed(name);
      expect(Object.keys(permissions).length, name).toBeGreaterThan(0);
      const writes = Object.entries(permissions).filter(([, level]) => level === "write");
      expect(writes, `${name} grants ${JSON.stringify(writes)} at the workflow level`).toHaveLength(0);
    }
  });

  it("grants each job the permission its steps actually call", () => {
    // "Resource not accessible by integration" only appears at tag time, when
    // the publishing steps run. This reads the steps instead: a marker in a
    // job body means the effective permissions of that job must cover it.
    // [marker, permission, level]: SignPath only reads the artifact back, so
    // its need is `actions: read`, while a dispatch call is a write.
    const markers: [RegExp, string, string][] = [
      [/gh release (create|upload|edit|delete)/, "contents", "write"],
      [/gh workflow run/, "actions", "write"],
      [/git\/refs/, "contents", "write"],
      [/actions\/attest-build-provenance@/, "id-token", "write"],
      [/actions\/attest-build-provenance@/, "attestations", "write"],
      [/SignPath\/github-action-submit-signing-request@/, "actions", "read"],
      [/scripts\/pr-size\.mjs/, "issues", "write"],
      [/scripts\/pr-size\.mjs/, "pull-requests", "write"],
      [/codeql-action\/(analyze|upload-sarif)@/, "security-events", "write"],
    ];
    for (const name of workflows) {
      const text = read(name);
      const { permissions } = parsed(name);
      for (const block of text.split(/\n(?= {2}[\w-]+:\n)/)) {
        const job = block.match(/^ {2}([\w-]+):\n/)?.[1];
        if (!job) continue;
        // The job body minus its comment lines: a comment that names a command
        // must not demand a permission.
        const body = block
          .split("\n")
          .filter((line) => !line.trim().startsWith("#"))
          .join("\n");
        const own = block.includes("\n    permissions:") ? parsed(name).jobs[job]?.permissions : undefined;
        const effective = own ?? permissions;
        for (const [marker, permission, level] of markers) {
          if (!marker.test(body)) continue;
          expect(effective?.[permission], `${name} ${job} needs ${permission}: ${level}`).toBe(level);
        }
      }
    }
  });

  it("keeps the called workflow inside its caller's ceiling", () => {
    // A reusable workflow may keep or lower the caller's scopes, never raise
    // them; a violation is a startup failure on the release tag.
    const caller = parsed("release.yml").jobs?.linux?.permissions ?? {};
    const callee = parsed("linux-package.yml").jobs?.linux?.permissions ?? {};
    for (const [permission, level] of Object.entries(callee)) {
      if (level !== "write") continue;
      expect(caller[permission], `linux-package wants ${permission}: write`).toBe("write");
    }
  });

  it("keeps the write permissions the publishing jobs need", () => {
    // Narrowing the workflow level must not silently take a permission away
    // from the one job that publishes; that failure only shows at tag time.
    const needs: [string, string, Record<string, string>][] = [
      ["auto-tag.yml", "tag", { contents: "write", actions: "write" }],
      ["beta.yml", "beta", { contents: "write" }],
      ["linux-package.yml", "linux", { contents: "write" }],
      ["pr-size.yml", "size", { issues: "write", "pull-requests": "write" }],
      ["release.yml", "windows", { contents: "write", "id-token": "write", attestations: "write" }],
      ["release.yml", "linux", { contents: "write" }],
    ];
    for (const [name, job, permissions] of needs) {
      const { jobs } = parsed(name);
      expect(jobs[job], `${name} ${job}`).toBeDefined();
      for (const [permission, level] of Object.entries(permissions)) {
        expect(jobs[job]?.permissions?.[permission], `${name} ${job} ${permission}`).toBe(level);
      }
    }
  });
});
