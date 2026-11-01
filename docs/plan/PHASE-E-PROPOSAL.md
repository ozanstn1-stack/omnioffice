# Phase E proposal — after v7.1.0

**Status: approved; E0 delivered in v7.1.1.**

## Progress

| Release | Items | Status |
|---|---|---|
| v7.1.1 | E0 quick wins | Delivered (#551) |
| v7.2.x | E1 security | **Shipped.** E1 delivered in full: CSP/prototype freeze, unsafe-free crates, raw-path policy, LAN pairing PAKE, keyring secret tier, command ACL and plugin-install confirmation. E2.2–E2.4 moved to v7.3.0. The two E1 leftovers (configured-output token migration, clippy hygiene) were taken up in the v7.4.0 work |
| v7.3.0 | E2 (isolation) | **Shipped.** Worker containment (E2.2), async engine probe + command audit (E2.3) and typed command errors (E2.4). E4/E5/fuzz move to v7.4.0 |
| v7.4.0 | E1 leftovers + E4/E5 | **In progress.** Configured output destinations are persisted grants behind the strict policy (closed), the clippy unwrap/expect ratchet now denies panicking shortcuts in all seven crates (closed); E4/E5 and the fuzz extension follow |
| v7.5.0 | E3 | — |
| v8.0.0 | E2.1, E6 | — |

 Phase C is closed (C1–C15) and Phase D is delivered
(D1–D13; D10 closed as "unsigned by decision", D11's listing submission is the
owner's external step). This document defines **Phase E**: security hardening,
quality infrastructure, architectural debt and product depth, in versioned
trains of small PRs.

It is grounded in a review of `master` at `12c20d7` (v7.1.0, 2026-10-31). Every
finding below was checked in the repository; where a number from the review no
longer matched the tree it was corrected. Nothing here is committed until the
owner picks the items and a tracker issue is opened.

## Starting point (measured)

| Metric | Value |
|---|---|
| Frontend production TS/TSX | ~90,400 lines |
| TypeScript test files / vitest cases | 216 files / ~2,430 cases |
| Rust (7 crates + app) | ~129,800 lines, ~1,400 `#[test]` |
| Tauri commands | 249 registered (`ipc.rs`), 253 `#[tauri::command]` attributes |
| CI | lint, frontend, android-target, android-kotlin, Rust (3 OS), MSRV, extension, 2 E2E jobs, fuzz, bench, audit |
| Releases | Windows/Linux/Android, 18 assets, SHA-256 verification; Windows unsigned by decision |

Parent PR: #548 (v7.1.0), current commits after D13/D10/D11/D12.

## E0. Quick wins (v7.1.1, 1–2 days, one PR series)

| # | Finding (verified) | Fix |
|---|---|---|
| 1 | `issue_tokens` is marked `#[tauri::command]` but never registered; the attribute also inflates the command count (253 vs 249). | Remove the attribute, fix the comment; refresh the README command count. |
| 2 | `desktop.yml` uploads crash artifacts **before** the `pdf_parse` fuzz target, so a crash there loses its input. | Move the upload step after the last fuzz target. |
| 3 | `desktop.yml` has no `concurrency`, and none of its 12 jobs has `timeout-minutes` (only steps do). | Add workflow concurrency with cancel for PRs, and job-level timeouts. |
| 4 | Impress `undo`/`redo` call `setRedoStack` and `edit()` inside the `setUndoStack` updater (impure, double-invoked under StrictMode). | Move to a reducer/`useCalcUndoRedo`-style hook (see E3.3). |
| 5 | Release builds call `tauri build` / `tauri android build` without `--locked`; `check-reproducibility-inputs.mjs` only checks `cargo build` lines. | Add a `cargo metadata --locked` preflight before every tauri build (the CLI passthrough is version-dependent), extend the gate to require it, dry-run once. |
| 6 | Stale comments/docs: `lib.rs:104,231`, `crash_log.rs:3` (panic=abort claim), `aicore/src/lib.rs:1-6`, `deny.toml` zip note (calamine still pins zip 8; the review's "zip 9" claim is wrong), file-wide `dead_code` in `files.rs:15`, README format-gate text and test counts. | Fix each in place; no behaviour change. |
| 7 | `check` and `check:fast` are byte-identical. | Drop `check:fast`; `check:all` builds on `check`. |
| 8 | `cargo audit --ignore` list duplicates `deny.toml`. | Keep `deny.toml` as the only advisory list; let `cargo audit` read it or reduce the duplicate to a comment. |
| 9 | Generated `specta.ts` is inside the file-size ratchet and grows on every regeneration. | Exclude `src/lib/generated/` from the ratchet; rebuild the baseline. |

Acceptance: all nine items merged in one PR (or two), CI green, no user-visible
change, README counts refreshed.

## E1. Security hardening (v7.2.0, priority 1)

1. **Raw-path closes.** `paths::lexical()` and `paths::directory()` fall back to
   a raw path without calling `paths_policy::ensure_raw_allowed()`
   (`paths.rs:212,222`), while the other validators do call it
   (`paths.rs:149,189`). In a release build that would let a compromised webview
   use `ensure_dir`, `file_fingerprint`, `output_exists`, `ai_library_save`
   targets, `sync_forget`, batch output folders and **the LAN shared folder**
   with arbitrary paths. `lexical()` also accepts a Read token while writing.
   Fix: both validators pass through the policy with an explicit `FileAccess`;
   directory picks use a `file_token_pick_directory` token; regression tests
   prove a release-mode raw path is refused.
2. **LAN pairing MITM. Delivered (E1/3).** `RecordingServerCert` accepts every
   certificate during pairing (`transport.rs:120-131`) and the six-digit code
   used to travel inside that TLS session. Pairing now runs SPAKE2 (`spake2`)
   over the code and hashes both certificate fingerprints into the derived key
   (`lan/pairing.rs`); the SAS/QR variant was not needed because a relay
   derives two different keys and cannot confirm either side. A guessed
   confirmation spends one of the five attempts, accepted nonces are cached for
   the freshness window (`lan/replay.rs`), and four worker threads replace the
   one-at-a-time accept loop. Regression tests: `lan/pairing_tests.rs`.
3. **Secrets on desktop. Delivered (E1/4).** `secret.rs` used to write `plain:`
   when the platform cipher was unavailable and never set `0600`, so on Linux
   and macOS the API keys, the WebDAV password and the OAuth token landed in
   umask-permission files. The store now has three tiers: the platform cipher,
   then the OS credential vault (`src-tauri/src/secret/vault.rs`, behind an
   injectable `Vault` so the tests need no Keychain) with only a
   `keyring:<slot>` marker in the file, then a `plain:` file forced to `0600`.
   Legacy plain values migrate into the vault on read (with a read-back check
   before the plain file is replaced), old files are tightened when they are
   read, and every sealed blob store (LAN device key, person cards, signatures,
   trash copies) is written owner-only through `secret::write_sealed`. The UI
   names the tier it is in.
4. **Command ACL. Delivered (E1/4b).** `build.rs` called plain
   `tauri_build::build()`, so the webview could invoke all 249 registered
   commands. The build script now declares an app ACL manifest and
   `src-tauri/permissions/` carries one permission per `ipc.rs` domain,
   generated by `scripts/command-acl.mjs` so the manifest cannot drift from the
   registry; `capabilities/default.json` grants the eight permissions to the
   main window. `src-tauri/src/command_acl.rs` compares the compiled allow list
   with the registered surface, and `e2e/smoke.mjs --acl` proves the runtime
   refusal (see `docs/command-acl.md` for the limits).
5. **CSP.** Current policy has `default-src 'self'`, `img`, `style`, `font`,
   `worker-src blob:` and `connect-src 'self' ipc: …`; it lacks
   `object-src 'none'; base-uri 'none'; form-action 'none';
   frame-ancestors 'none'`. Add those and `freezePrototype: true`, and prove the
   plugin worker cannot invoke IPC without the key.
6. **Plugin install. Delivered (E1/5).** `plugin_install` used to trust the
   webview's manifest and source. The Rust side now validates the pair, shows
   the plan in a native dialog (permissions, capabilities, size, bundle SHA-256)
   and writes nothing without consent; `install.json` records the hash and
   `plugin_list` reports a plugin changed after install as `modified` (see
   `docs/plugins.md`). A signed manifest stays a later step.
7. **Rust hygiene. Delivered.** `#![forbid(unsafe_code)]` shipped in E1/1.
   `clippy::unwrap_used`/`expect_used` are now denied in production code
   (`cfg_attr(not(test), deny(...))`) in every crate: `turkish`, `pdfcore` and
   `aicore` in the first slice, `officecore`, `synccore` and the `compat` CLI in
   the second, where all 55 remaining offenders became typed errors, explicit
   fallbacks or `if let` guards. Two infallible-by-construction calls (the
   embedded PDF face, the HMAC key) stay as documented `#[expect]`. The
   `RUSTSEC-2023-0071` review stays scheduled for 2027-01-01.

Acceptance: a regression test per item (raw-path refusal, simulated MITM,
secret migration, ACL capability list, CSP header, install confirmation), and
`cargo deny check` still green.

## E2. Isolation and resilience (v7.2.0–v7.3.0)

1. **PDF parsing/rendering into the worker.** Today only `repair` runs in the
   worker; lopdf and pdfium run in-process (`pdfcore/src/render.rs`,
   `textbox.rs`). Step 1: a persistent worker pool for preview/text-extraction/
   info commands (`worker.rs` `RunKind`). Step 2: the remaining PDF operations.
2. **Worker limits. Delivered (E2.2).** The child is contained before it runs:
   on Windows it is created suspended, assigned to a job object with a
   process-memory limit and only then resumed (it used to be spawn-then-assign);
   Linux/macOS install `RLIMIT_AS` and `RLIMIT_CPU` between `fork` and `exec`; the
   environment is an explicit allow-list (engine paths, temp dir and the platform
   basics only); and `--worker` counts only as the first argument. The code lives
   in `worker/containment.rs` with its own tests (`worker_tests.rs`).
3. **UI stalls. Delivered (E2.3).** The proposal named `ocr_languages`, which
   only lists the tessdata directory; the command that spawns a process is
   `engine_status` (`tesseract --version`), and it now runs on the blocking pool
   with a "missing" fallback. The audit of the sync surface is in
   `docs/command-async-audit.md`: 50 sync commands touch the filesystem, all
   bounded small reads/writes of app state, plus the rule for new commands.
4. **Typed errors. Delivered (E2.4).** The commands that answered with a bare
   string - external links, the file/folder token picks, job status and retry,
   the updater, and the two Android bridges (behind thin validating wrappers so
   the platform code stays as it was) - now return `PdfError`, so the UI gets
   `{code, message}` and its existing `toAppError` maps the code to a localized
   message. A rejected link and an unknown job status are pinned as
   `invalid_input` on the wire.
5. **Fuzzing.** `pdf_parse` wraps the parser in `catch_unwind`, hiding panics;
   remove it. Add targets for forms, signature/CMS verification, redact, rebuild;
   add inputs with no target today (EML, ODS/ODP, WebDAV PROPFIND XML, LAN frame
   and `DiscoveryReply::decode`, AI response parsing). Add a mutation-driven
   round-trip fidelity fuzz for the office formats (open item in
   `docs/office-fidelity-report.md`).
6. **Audit leftovers** (`AUDIT_REPORT.md`): M15 (unchanged document copied into
   history on every save) and Android "dropped-future liveness".

## E3. Frontend architecture and debt (v7.3.0–v7.4.0)

1. **`App.tsx` (1,155 lines) split.** Screen identity is repeated four times
   (lazy import, `screens` memo, `NAV_ICONS`, `routes.ts`); move the component
   factory and icon into one screen registry. Overlay hosts are duplicated in
   the compact and desktop branches (with a `PresendDialog` difference) → one
   `<GlobalOverlays/>`. Six open/drop functions repeat file→screen routing → one
   `routeFile()`. Move `NAV_ICONS` to module scope and review the
   `key={screen-files.join}` remount.
2. **Editors split.** `WriterEditor.tsx` 3,252 lines (~2,980 in one function,
   40 `useState`), `ImpressEditor.tsx` 2,796, `CalcEditor.tsx` 2,541,
   `PdfStudio.tsx` 1,901 (47 `useState`), `Ai.tsx` 1,915 (41 `useState`). Extract
   field hooks (`useWriterSelection`, `useWriterTables`, …) and group state with
   `useReducer`; lower the ratchet with every split.
3. **One history implementation.** Writer `historyRef`, Calc
   `useCalcUndoRedo`, Impress full-deck copies: generalise `useCalcUndoRedo`
   into `useHistory<T>` (patch or snapshot, limit, coalescing) and adopt it,
   which also fixes E0.4.
4. **Complete IPC typing.** 38 `invokeTracked("cmd", …)` calls in
   `src/lib/api.ts` and 5 in `PdfStudio.tsx` move to a typed
   `trackedCall(c => c.mergePdfs, args)`; add the two Android update commands to
   specta; connect the 1,532-line `office-types.ts` (65 types) and 68 non-specta
   types in `types.ts` to generated types; merge the 17 ts-rs files into specta
   and delete `types-gen.mjs`.
5. **State.** Two `useJobs` stores (`store.ts`, `jobs.ts:258`) both listen to the
   same progress event and never clear `progress`; merge into one.
6. **Duplication.** `PageThumb` (Organize) vs `Thumb` (`components/pages.tsx`);
   14+ inline `split(/[\\/]/).pop()` instead of `fileBaseName`; `clamp` defined
   twice.
7. **Performance.** Reader/pages/Organize mount a DOM node per page → windowing;
   `startTransition` for heavy lists; the entry budget has ~0.7 KB headroom at
   62 KB → lazy-load `CommandPalette`/`GlobalSearch`/`files.tsx` and stop
   importing `package.json` just for the version.
8. **Quality.** 42 `eslint-disable`s (15 each for `set-state-in-effect` and
   `exhaustive-deps`), 79 silent `.catch(() => {})`, then
   `noImplicitOverride`, `noImplicitReturns`, and a folder-ratcheted
   `noUncheckedIndexedAccess`; enable `recommendedTypeChecked`.

## E4. Test, coverage and i18n (v7.3.0)

1. **Coverage.** *Frontend delivered:* vitest floors raised from 62/69/46/62 to
   77/79/64/77 against a measured 78.4/80.2/65.9/78.4 (2026-11-01), the report
   uploaded as the `coverage-frontend` artifact, and a test that refuses to let a
   floor drop. *Open:* the `cargo llvm-cov` job (Rust coverage is not measured
   today).
2. **Thinly tested large screens.** `PdfStudio` (1,901), `Reader` (1,415), `Ai`
   (1,915, 6 tests), `Vault` (4), `Annotate`, `PageTools`; screens with no tests:
   `PetitionWizard`, `lan-sync-plan`, `scanner-corners`, `scanner-page-list`.
3. **E2E.** Split `e2e/smoke.mjs` (696 lines) into scenario modules; add Calc,
   Impress, Scanner, Vault, signature/verification and AI-pack flows against
   `scripts/mock-deepseek.mjs`; make the Android emulator run label-triggerable
   (`android-e2e`) on PRs.
4. **Benchmarks.** Make the criterion gate blocking with a two-consecutive-runs
   rule (or median of two), instead of advisory.
5. **i18n.** *Partly delivered.* `i18n-audit.mjs` now fails on placeholder
   parity and on more hardcoded JSX text than `scripts/i18n-baseline.json`
   records (measured with the TypeScript parser, ratcheted); unused keys and
   en=tr identical values are advisory. `DocumentHealthScreen` (14),
   `PetitionWizard` (4), `Invoices` (23 of 42), `KvkkWizard` (9) and `Organize`
   (3) moved into the tables, and the five named `aria-label`/`alt` strings were
   translated (baseline 131 -> 80; `docs/i18n.md`). What remains is mostly
   literal by design (`TRY`, accounting-software names, paper sizes, units).
   The 22 `(s)` strings now use `t.plural` over `Intl.PluralRules` and the
   audit fails on a new one. *Open:* the `Redact` prose and attribute strings
   beyond the five fixed (the scanner counts JSX text nodes only).

## E5. CI/CD efficiency and supply chain (v7.3.0)

- **Caching:** *partly delivered* - the `desktop.yml` workspace-test and Windows
  E2E jobs cache `src-tauri/resources/engines` under the `engines.lock.json`
  hash, pinned by a test. *Open:* the release/beta/nightly workflows, replacing
  `cargo install tauri-driver` with `cargo-binstall`/cache, and
  `Swatinem/rust-cache` for Android.
- **Android build:** split the six sequential `tauri android build` calls into a
  parallel ABI matrix.
- **Pinning:** *delivered* - every workflow asks for `ubuntu-24.04` instead of
  `ubuntu-latest`, `rust-toolchain.toml` pins the stable channel with
  clippy/rustfmt (the MSRV and fuzz jobs override it explicitly so the file
  cannot mask them), and `.nvmrc` + `engines.node` state the Node version the
  suite is built and tested with. All four facts are pinned by tests.
- **Security:** *partly delivered* - `SECURITY.md` documents the private
  advisory channel, supported versions and the deliberate limits, and the
  `CodeQL` workflow analyses `javascript-typescript` plus the workflow files on
  push, PR and a weekly schedule. *Open:* OpenSSF Scorecard.
- **Dependency bot:** choose Renovate **or** Dependabot (currently only
  Dependabot); migrate the blocking notes if Renovate is chosen.
- **Deferred majors, one PR each:** vite 6→7/8 and `@vitejs/plugin-react` 5;
  vitest 3→4 (re-baseline coverage); eslint 10; TS 7 (extension already uses
  TS 7); evaluate open PRs #470/#471.
- **Cross-platform engines:** move `scripts/fetch-engines.ps1` logic to a Node
  script (`fetch-engines.mjs`) so Linux/macOS packages can include engines and
  the npm scripts stop depending on PowerShell.

## E6. Product depth (v7.4.0 → v8.0.0)

All items are already listed as open in the project docs.

1. **Office fidelity** (`docs/office-fidelity-report.md`): dynamic-array (spill)
   calculation; `t="shared"` output in the XLSX writer; add images, charts,
   comments, headers/footers, numbering and section breaks to the
   round-trip counters; multiple ODP master pages; Calc chart UI and Impress
   media.
2. **Writer-selection-scoped desk tasks** (the open D13 item).
3. **LAN:** resumable downloads (only uploads resume today).
4. **Türkiye-specific:** CMK rules in deadline calculation; the holiday table
   ends in 2029 → generator and tests for 2030+; enable the UDF reader/writer
   behind the fidelity test.
5. **Known UI defect:** the toast bar overlaps the bottom navigation area
   (RELEASE_READINESS 6.5.1).
6. **AI:** send embeddings in batches, not per chunk; persistent consent option.
7. **PDF:** orphan objects after annotation deletion; N-up drops links; tagged
   PDF export does not tag tables/headers/footers.
8. **Chrome extension:** frozen since 3.x; decide archive vs. bring into lint,
   i18n and the release flow.

## E7. Documentation and project hygiene

- `README.md` (52 KB): "What's new 6.2→7.1" (~190 lines) repeats CHANGELOG —
  shorten it; fix "Windows and Android" (Linux missing), the "staged" format
  gate, the 3.5.5-era roadmap section, and the architecture block missing
  `compat`/`turkish`.
- `RELEASE_READINESS.md` (174 KB) is a second changelog; keep one current
  **"Open items"** list and move release history to `docs/archive/`.
- `docs/roadmap.md` is stuck at 4.7.0; connect Phase C/D and make this Phase E
  document the live plan. Close `PHASE-D-PROPOSAL.md`'s status and open
  decisions.
- Missing files: `CONTRIBUTING.md`, `SECURITY.md`, `CODEOWNERS`,
  `.github/ISSUE_TEMPLATE/` (the in-app "report a problem" already produces the
  labelled URL) and `docs/architecture.md`.
- Name drift: npm package `pdf-swiss-army-knife`, crate `pdf_sak_lib`, env vars
  `PDFSAK_*`, README "Office Swiss Army Knife" path. Cosmetic names can change
  cheaply; **do not change the Tauri identifier, Android package or keystore
  name** (data dir, updater, signature continuity depend on them).
- Move `AUDIT_REPORT.md`/`AUDIT_BASELINE.md` (titles still say v3.2.1) to
  `docs/archive/` and write a current audit report aligned with E1.

## Owner actions (not solvable in code)

- SignPath Foundation or Certum OSS application (D10; SmartScreen).
- IzzyOnDroid Codeberg submission (#502).
- Install Renovate (or confirm Dependabot-only).
- Verify the real updater pubkey once (`PLACEHOLDER_UPDATER_PUBKEY` path).
- Physical-device passes: LAN S1–S14 matrix, 360 dp record sheet, WhatsApp
  end-to-end flow.

## Release trains

| Release | Contents |
|---|---|
| **v7.1.1** | E0 quick wins |
| **v7.2.0 "Güvenli sınırlar"** | E1 all, E2.2–E2.4 |
| **v7.3.0 "Ölçülü kalite"** | E4, E5, E2.5 fuzz expansion |
| **v7.4.0 "Sade kabuk"** | E3 (screen registry, shared history, IPC typing, first editor split) |
| **v8.0.0** | E2.1 PDF worker isolation, E6 fidelity/product items, Linux engines |

E7 runs alongside every release.

## Verification per PR

- Frontend: `npm run lint && npm run i18n:audit && npx tsc --noEmit &&
  npm run test:coverage && npm run build && npm run bundle:check &&
  npm run files:check`.
- Rust: `cargo fmt --all -- --check && cargo clippy --workspace --all-targets
  -- -D warnings && cargo test --workspace && cargo deny check`.
- Security items: one regression test each (raw-path refusal in release mode,
  simulated LAN MITM, secret migration, CSP header, install confirmation), then
  `npm run claims:check`.
- CI changes: `workflow_dispatch` dry-run on the branch; a deliberate crashing
  input proves fuzz artifacts upload.
- E2E: `npm run e2e:flows`, extended as scenarios land.

## First step

1. Approve this proposal and open a Phase E tracker issue with one checkbox per
   E0–E7 section and per E0 item.
2. Start **v7.1.1 (E0)** in a single branch; the security work (E1) follows as
   v7.2.0.
