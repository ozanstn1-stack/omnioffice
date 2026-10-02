# Release Readiness — Office Swiss Army Knife 3.3.0

This file states what is actually implemented, tested and benchmarked, and
what is not. It is deliberately conservative: nothing is claimed as released,
built or verified unless it was reproduced in this environment or is produced
by CI.

## Implemented (this cycle)

- **Canonical `.oswk` format** (`crates/officecore/src/unit.rs`): versioned
  envelope with `documentType`, `applicationVersion`, a SHA-256 `checksum`,
  `featureManifest` and an `extensions` bag; unknown fields preserved;
  checksum mismatch ⇒ `corrupt_document`; newer schema refused.
- **Writer transaction history** (`src/office/writer/history.ts`): reversible
  operations, bounded checkpoint + delta undo/redo wired to Ctrl+Z / Ctrl+Y /
  Shift+Ctrl+Z and the toolbar.
- **Calc Excel compatibility**: approximate `MATCH`/`HLOOKUP`, `XLOOKUP` ±2,
  `*`/`?` wildcards in `COUNTIF`/`SUMIF`, `NUMBERVALUE` separators, `FILTER`
  blank mask, array-aware unary for `SUMPRODUCT(--(range>1))`.
- **External file-conflict guard**: SHA-256 fingerprint at open/save plus a
  Reload / Save-as-new / Cancel dialog.
- **Release metadata**: `build-info.json` (version, git SHA, toolchain).
- Security/data-loss items from the 3.2.1 audit (see `AUDIT_REPORT.md`).

## Tested in this environment

| Gate | Command | Result |
|---|---|---|
| Frontend unit/integration | `npm test` | 30 files, 617 passed, 1 skipped |
| TypeScript | `npx tsc --noEmit` | clean |
| ESLint | `npm run lint` | 0 errors, 0 warnings |
| i18n parity | `npm run i18n:audit` | en = tr = 1514 keys |
| Frontend build | `npm run build` | success |
| Rust tests | `cargo test --workspace` | all workspace targets pass |
| Rust clippy | `cargo clippy --workspace --all-targets -- -D warnings` | clean |

New regression tests: `.oswk` envelope (checksum/feature-manifest/migration/
future-schema), Writer history (undo/redo/branch/cap/header), Calc approximate
lookup/wildcards/`NUMBERVALUE`/`FILTER`/unary.

## Benchmarked

- Existing cargo/frontend perf guards run in CI: large Writer pagination,
  50k/100k-cell workbooks, 20k/100k dependency chains, PDF compression.
- No new large-PDF (100/500/1000-page) or 20k-file vault benchmark was added
  in this pass; those remain scheduled (see below).

## Not done in this pass (honest)

These were requested but are **not** implemented/verified here, and the README
and this file say so rather than implying coverage:

- **PDF deep audit** (coordinates, render cache/LRU, redaction independence,
  signed-PDF golden fixtures): the dedicated audit pass timed out; redaction
  and signature suites re-run green, but no new adversarial PDF review.
- **Impress polish and PPTX nested-transform golden math**: unchanged.
- **DOCX/XLSX XML-differential golden pipeline expansion**: partially covered
  by existing round-trip suites; the new fixtures focus on `.oswk`.
- **Android background work (WorkManager/Foreground Service)**, Android
  Keystore `SecretStore`, and Android UX passes: not started. The README's
  existing limitations still apply and were not weakened.
- **Vault 2.0 ranking/filters and 20k-file benchmark**: not started.
- **Command palette/home IA restructure**, localization/accessibility sweeps:
  not started.
- **Fuzzing expansion**: not started.
- **Performance budgets for APK/installer size and startup**: bundle budget
  exists; APK/installer budgets not added.

## Platform support and artifacts

- **Windows**: source builds and tests locally. A release installer/portable
  ZIP was **not** produced in this environment (kept out of the repo history).
  CI (`.github/workflows/release.yml`) builds NSIS installer, portable ZIP,
  SHA256SUMS, SBOMs, `build-info.json` and a provenance attestation.
- **Android**: **not built here** — no Android SDK/NDK in this environment
  (`ANDROID_SDK_ROOT` unset). The Gradle wrapper and CI workflow exist; the
  APK/AAB artifacts are CI-only. No artifact from this pass should be
  presented as an Android release.
- **Linux/macOS desktop**: built and tested by CI.
- **Chrome extension**: unchanged from 3.1.1; CI runs its self test.

## Applying a release locally

`npm run release:local` (or `scripts/release-local.ps1`) builds Windows and
Android, writes `build-info.json` + CycloneDX SBOMs + `SHA256SUMS.txt`, and
applies the new build to this machine **per-user** via
`scripts/install-local.ps1` (portable ZIP into
`%LOCALAPPDATA%\Programs\Office Swiss Army Knife`, Start Menu and Desktop
shortcuts refreshed). No administrator rights are needed. Add `-Publish` to
also create/update the GitHub release for the current version.

The per-machine copy in `C:\Program Files\Office Swiss Army Knife` (if one is
installed) can only be replaced by running the NSIS installer **elevated**;
the script deliberately never writes there.

## Release artifacts expected from CI

Windows: `Office-Swiss-Army-Knife-Setup-3.3.0.exe`,
`Office Swiss Army Knife_3.3.0_x64-setup.exe`,
`Office-Swiss-Army-Knife-Portable-3.3.0.zip`, `SHA256SUMS.txt`,
`sbom-rust.cyclonedx.json`, `sbom-npm.cyclonedx.json`, `build-info.json`.
Android: `PDF-Swiss-Army-Knife-Android-3.3.0-arm64-v8a.apk`,
`…-armeabi-v7a.apk`, and the AABs. Extension: package ZIP.

Only artifacts actually produced by CI are published; this file does not claim
they exist yet.

## Security status

- No silent overwrite: saves/imports are guarded by the compatibility report
  and (new) the file fingerprint check; WebDAV conflicts are manual.
- Signatures report `trust: unknown` offline — never "trusted".
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
