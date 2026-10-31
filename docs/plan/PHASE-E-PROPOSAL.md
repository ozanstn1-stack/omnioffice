# Phase E proposal — after v7.1.0

**Status: approved; E0 delivered in v7.1.1.**

## Progress

| Release | Items | Status |
|---|---|---|
| v7.1.1 | E0 quick wins | Delivered (#551) |
| v7.2.0 | E1 security, E2.2–E2.4 | In progress: CSP + prototype freeze + `forbid(unsafe_code)` landed; raw-path, LAN MITM, secrets, ACL and plugin confirmation are next |
| v7.3.0 | E4, E5, E2.5 | — |
| v7.4.0 | E3 | — |
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
2. **LAN pairing MITM.** `RecordingServerCert` accepts every certificate
   (`transport.rs:120-131`), and the six-digit code is sent inside that TLS
   session. Fix: SPAKE2 (`spake2`) binds the code to a PAKE key and to the two
   certificate fingerprints, or a short authentication string
   `SAS = H(fp_host‖fp_client‖code)` compared on both screens. Also cache seen
   nonces for the replay window (`lan.rs:659,887`) and serve connections on
   bounded worker threads instead of one at a time.
3. **Secrets on desktop.** `secret.rs` writes `plain:` when the platform cipher
   is unavailable and does not set `0600`. Use the already-present `keyring`
   dependency behind `SecretCipher`, fall back to `plain:` with `0600` only
   when the keyring is missing, migrate legacy values on read, and extend the
   existing cipher tests.
4. **Command ACL.** `build.rs` calls plain `tauri_build::build()`; all 249
   commands are reachable from the main window. Map the eight `ipc.rs` domains
   to capabilities/app-manifest permissions.
5. **CSP.** Current policy has `default-src 'self'`, `img`, `style`, `font`,
   `worker-src blob:` and `connect-src 'self' ipc: …`; it lacks
   `object-src 'none'; base-uri 'none'; form-action 'none';
   frame-ancestors 'none'`. Add those and `freezePrototype: true`, and prove the
   plugin worker cannot invoke IPC without the key.
6. **Plugin install.** `plugin_install` trusts the webview's manifest and
   source. Require an explicit Rust-side confirmation (`dialog::ask`) and record
   the installed SHA-256; a signed manifest is a later step.
7. **Rust hygiene.** Add `#![forbid(unsafe_code)]` to the six library crates,
   warn on `clippy::unwrap_used`/`expect_used` (biggest offenders:
   `officecore/src/docx/writer.rs`, `xml.rs`), and schedule the
   `RUSTSEC-2023-0071` review (2027-01-01).

Acceptance: a regression test per item (raw-path refusal, simulated MITM,
secret migration, ACL capability list, CSP header, install confirmation), and
`cargo deny check` still green.

## E2. Isolation and resilience (v7.2.0–v7.3.0)

1. **PDF parsing/rendering into the worker.** Today only `repair` runs in the
   worker; lopdf and pdfium run in-process (`pdfcore/src/render.rs`,
   `textbox.rs`). Step 1: a persistent worker pool for preview/text-extraction/
   info commands (`worker.rs` `RunKind`). Step 2: the remaining PDF operations.
2. **Worker limits.** Windows: create the child suspended and add it to the job
   object before it runs (currently spawn-then-add, `worker.rs:159`); Linux/macOS:
   `setrlimit(RLIMIT_AS, RLIMIT_CPU)`; clear the environment (allow-list);
   recognise `--worker` only as the first argument (`cli.rs:154`).
3. **UI stalls.** `ocr_languages` runs tesseract synchronously in the command
   thread (`commands.rs:124`); make it async + `spawn_blocking`, then audit the
   ~103 sync commands for file/process work.
4. **Typed errors.** The 13 commands returning `Result<_, String>` (jobs, files,
   file_dirs, links, update, android_update, android_share) move to
   `PdfError`/`ErrorCode`.
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

1. **Coverage.** Raise vitest thresholds to the measured values and publish the
   report as a CI artifact; add a `cargo llvm-cov` job (Rust coverage is not
   measured today).
2. **Thinly tested large screens.** `PdfStudio` (1,901), `Reader` (1,415), `Ai`
   (1,915, 6 tests), `Vault` (4), `Annotate`, `PageTools`; screens with no tests:
   `PetitionWizard`, `lan-sync-plan`, `scanner-corners`, `scanner-page-list`.
3. **E2E.** Split `e2e/smoke.mjs` (696 lines) into scenario modules; add Calc,
   Impress, Scanner, Vault, signature/verification and AI-pack flows against
   `scripts/mock-deepseek.mjs`; make the Android emulator run label-triggerable
   (`android-e2e`) on PRs.
4. **Benchmarks.** Make the criterion gate blocking with a two-consecutive-runs
   rule (or median of two), instead of advisory.
5. **i18n.** Remove hardcoded Turkish JSX literals (`PetitionWizard` 16,
   `KvkkWizard` 10, `Invoices` 8, `DocumentHealthScreen` 7) and English
   `aria-label`/`alt` strings (`office-ui.tsx:285`, `Organize.tsx:581`,
   `pages.tsx:77,378`, `Reader.tsx:487`); extend `i18n-audit.mjs` to check
   hardcoded JSX text, placeholder parity, unused-key candidates and en=tr equal
   values; replace the 68 `{count}`/“(s)” fake plurals with `Intl.PluralRules` /
   `Intl.NumberFormat`.

## E5. CI/CD efficiency and supply chain (v7.3.0)

- **Caching:** cache engine downloads (`engines:fetch` runs in 5 workflows) by
  `engines.lock.json` hash; replace `cargo install tauri-driver` with
  `cargo-binstall`/cache; use `Swatinem/rust-cache` for Android instead of
  `actions/cache`.
- **Android build:** split the six sequential `tauri android build` calls into a
  parallel ABI matrix.
- **Pinning:** add `rust-toolchain.toml` (reproducibility report lists its
  absence), `.nvmrc` + `engines.node` (CI 22, README says 20+), pin runners to
  `ubuntu-24.04`.
- **Security:** CodeQL (JS/TS + Actions), OpenSSF Scorecard, `SECURITY.md`
  reporting channel.
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
