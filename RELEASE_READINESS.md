# Release Readiness — Office Swiss Army Knife 3.5.0

This file states what is actually implemented, tested and benchmarked, and
what is not. It is deliberately conservative: nothing is claimed as released,
built or verified unless it was reproduced in this environment or is produced
by CI.

## Implemented (this cycle)

- **Coverage ratchet** (`vite.config.ts`, `.github/workflows/desktop.yml`):
  `npm run test:coverage` enforces statement/branch/function/line floors
  (58/68/46/58) and the frontend CI job runs it.
- **Screen tests** for the previously untested tool, library and productivity
  screens: `src/screens/tools-screens.test.tsx`, `src/screens/more-screens.test.tsx`,
  `src/office/tools-screens.test.tsx` — 24 tests that render the real screens
  and pin the request envelopes.
- **Templates contract** (`src/office/templates.test.ts`): unique ids,
  metadata and model kind.
- No production code changed in this release.

## Tested in this environment

| Gate | Command | Result |
|---|---|---|
| Frontend unit/integration | `npm test` | 35 files, 650 passed, 1 skipped |
| Frontend coverage | `npm run test:coverage` | 60.66 % statements, 71.17 % branches, 48.45 % functions (floors pass) |
| TypeScript | `npx tsc --noEmit` | clean |
| ESLint | `npm run lint` | 0 errors, 0 warnings |
| i18n parity | `npm run i18n:audit` | en = tr = 1537 keys |
| Frontend build | `npm run build` | success + bundle budget |
| Rust tests | `cargo test --workspace` | 528 passed, 3 ignored (unchanged) |

## Benchmarked

- Existing cargo/frontend perf guards run in CI: large Writer pagination,
  50k/100k-cell workbooks, 20k/100k dependency chains, PDF compression.
- No benchmark *trend* tracking yet; that stays on the Phase 2 list.

## Not done in this pass (honest)

- **Desktop E2E** (tauri-driver/WebDriver or a mocked-invoke browser suite)
  and **fuzzing** are still not started; they are the next Phase 2 items
  (`docs/roadmap.md`).
- **Coverage floors are conservative** (58/68/46/58) with a few points of
  headroom; they should be raised as the suite grows.
- Jobs "Retry" still reports `unavailable` for screens without a registered
  handler; `vault_clear` still has no UI caller.
- Android foreground service / WorkManager and Keystore `SecretStore`,
  PDF content-stream editing, PDF/A subsetting and RFC 3161 timestamps remain
  future work as in `docs/roadmap.md`.

## Platform support and artifacts

- **Windows**: source builds and tests locally; the release installer and
  portable ZIP are produced by `npm run release:local` / CI, together with
  `build-info.json`, CycloneDX SBOMs, `SHA256SUMS.txt` and a provenance
  attestation.
- **Android**: built by CI and by `npm run release:local` (Android SDK
  present); artifacts are named
  `PDF-Swiss-Army-Knife-Android-3.5.0-<abi>.apk`.
- **Linux/macOS desktop**: built and tested by CI.
- **Chrome extension**: unchanged; CI runs its self test.

## Applying a release locally

`npm run release:local` (or `scripts/release-local.ps1`) builds Windows and
Android, writes `build-info.json` + CycloneDX SBOMs + `SHA256SUMS.txt`, and
applies the new build to this machine **per-user** via
`scripts/install-local.ps1` (portable ZIP into
`%LOCALAPPDATA%\Programs\Office Swiss Army Knife`). No administrator rights
are needed. Add `-Publish` to also create/update the GitHub release for the
current version.

## Release artifacts expected

Windows: `Office-Swiss-Army-Knife-Setup-3.5.0.exe`,
`Office Swiss Army Knife_3.5.0_x64-setup.exe`,
`Office-Swiss-Army-Knife-Portable-3.5.0.zip`, `SHA256SUMS.txt`,
`sbom-rust.cyclonedx.json`, `sbom-npm.cyclonedx.json`, `build-info.json`.
Android: `PDF-Swiss-Army-Knife-Android-3.5.0-arm64-v8a.apk`,
`…-armeabi-v7a.apk`, and the AABs. Extension: package ZIP.

## Security status

Unchanged from 3.4.0: no silent overwrite (compatibility report + file
fingerprint check), signatures report `trust: unknown` offline with DSS
archiving (RFC 3161 timestamps not requested), plugin network HTTPS-only with
private/metadata refusal, Android `allowBackup=false` with a documented AI-key
plaintext fallback.

## Known limitations

Unchanged from the README "Known limitations" section, which remains the single
source of truth.
