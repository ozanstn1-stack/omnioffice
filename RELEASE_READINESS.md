# Release Readiness — Office Swiss Army Knife 3.4.0

This file states what is actually implemented, tested and benchmarked, and
what is not. It is deliberately conservative: nothing is claimed as released,
built or verified unless it was reproduced in this environment or is produced
by CI.

## Implemented (this cycle)

- **Command platform** (`src/lib/commands.ts`, `src/App.tsx`): every screen is
  a palette command; `matchKeybinding` now drives the global shortcuts and the
  former hard-coded key handler is gone. `file.open` routes office documents
  to the workspace.
- **Settings completion** (`src/screens/Settings.tsx`): midnight/paper themes,
  office default formats, version history and import-warning toggles; the
  defaults reach `useOfficeSession.ts` and `openOfficePath`.
- **Home tool directory** (`src/screens/Home.tsx`): multi-file suggestion fix,
  searchable grouped grid over all screens, recent PDFs open in the Reader.
- **Accessibility** (`src/components/ui.tsx`, `src/App.tsx`, `src/styles.css`):
  focus trap/restore, `aria-current`, roving tabindex + arrow keys, live job
  region, language attribute, translated labels, reduced-motion rule.
- **Calc `CUMIPMT`/`CUMPRINC`** (`src/office/calc/functions/financial.ts`) with
  Excel reference values and validation.
- **Docs/contract**: `compat.rs` signature note matches `pdfcore::ltv`, and
  `readme_contract_test.rs` keeps it honest; `docs/roadmap.md` added.

## Tested in this environment

| Gate | Command | Result |
|---|---|---|
| Frontend unit/integration | `npm test` | 31 files, 626 passed, 1 skipped |
| TypeScript | `npx tsc --noEmit` | clean |
| ESLint | `npm run lint` | 0 errors, 0 warnings |
| i18n parity | `npm run i18n:audit` | en = tr = 1537 keys |
| Rust tests | `cargo test --workspace` | 528 passed, 3 ignored |
| Rust contract | `cargo test -p officecore --test readme_contract_test` | 4 passed |

New regression tests: modal focus trap/restore and `Segmented` keyboard
navigation (`src/components/ui.test.tsx`), `CUMIPMT`/`CUMPRINC` values and
argument errors (`functions.test.ts`), and the compat-matrix signature wording.

## Benchmarked

- Existing cargo/frontend perf guards run in CI: large Writer pagination,
  50k/100k-cell workbooks, 20k/100k dependency chains, PDF compression.
- No new large-PDF (100/500/1000-page) or 20k-file vault benchmark was added
  in this pass; those remain scheduled (Phase 2/3 of `docs/roadmap.md`).

## Not done in this pass (honest)

- **Coverage is informational.** `npm run test:coverage` runs in CI but there
  is no blocking threshold yet.
- **Desktop E2E, fuzzing, benchmark trends**: not started (Phase 2).
- **Jobs "Retry"** still reports `unavailable` because no screen registers a
  retry handler; `vault_clear` still has no UI caller.
- **PDF deep audit** (coordinates, render cache/LRU, signed-PDF golden
  fixtures): unchanged, scheduled for Phase 3.
- **Android foreground service / WorkManager and Keystore `SecretStore`**:
  not started. The README limitations remain the source of truth.
- **Vault 2.0 ranking/filters and the 20k-file benchmark**: not started.
- **Fuzzing expansion, OAuth sync providers, auto-update**: not started.

## Platform support and artifacts

- **Windows**: source builds and tests locally (`cargo test`, `vite build`);
  the release installer and portable ZIP are produced by
  `npm run release:local` / CI (`.github/workflows/release.yml`), together
  with `build-info.json`, CycloneDX SBOMs, `SHA256SUMS.txt` and a provenance
  attestation.
- **Android**: built by CI and by `npm run release:local` on this machine
  (Android SDK present); APK/AAB artifacts are named
  `PDF-Swiss-Army-Knife-Android-3.4.0-<abi>.apk`.
- **Linux/macOS desktop**: built and tested by CI.
- **Chrome extension**: unchanged from 3.1.1; CI runs its self test.

## Applying a release locally

`npm run release:local` (or `scripts/release-local.ps1`) builds Windows and
Android, writes `build-info.json` + CycloneDX SBOMs + `SHA256SUMS.txt`, and
applies the new build to this machine **per-user** via
`scripts/install-local.ps1` (portable ZIP into
`%LOCALAPPDATA%\Programs\Office Swiss Army Knife`). No administrator rights
are needed. Add `-Publish` to also create/update the GitHub release for the
current version.

## Release artifacts expected from CI

Windows: `Office-Swiss-Army-Knife-Setup-3.4.0.exe`,
`Office Swiss Army Knife_3.4.0_x64-setup.exe`,
`Office-Swiss-Army-Knife-Portable-3.4.0.zip`, `SHA256SUMS.txt`,
`sbom-rust.cyclonedx.json`, `sbom-npm.cyclonedx.json`, `build-info.json`.
Android: `PDF-Swiss-Army-Knife-Android-3.4.0-arm64-v8a.apk`,
`…-armeabi-v7a.apk`, and the AABs. Extension: package ZIP.

Only artifacts actually produced are published; this file does not claim they
exist yet.

## Security status

- No silent overwrite: saves/imports are guarded by the compatibility report
  and the file fingerprint check; WebDAV conflicts are manual.
- Signatures report `trust: unknown` offline — never "trusted". DSS archiving
  (offline PAdES B-LT) is implemented; RFC 3161 timestamps are not requested.
- Plugin network: HTTPS-only with private/metadata refusal; Web Worker is not
  an OS sandbox (documented).
- Android `allowBackup=false`; AI keys use DPAPI on Windows with a documented
  plaintext fallback on Android (Keystore migration is future work).

## Known limitations

Unchanged from the README "Known limitations" section, which remains the single
source of truth. In particular: no Android foreground service (long jobs stop
on process death and come back `interrupted`), Type0 PDF/A fonts skipped,
PDF content-stream objects not editable, DOCX bookmark anchors not emitted,
WebDAV only, plugin install folder-based.
