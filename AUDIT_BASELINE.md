# AUDIT BASELINE — v3.2.1 (audit start)

Recorded before any code change, on the working tree at commit `50161db`
(`fix(android): use an existing PluginErrorPayload constructor in the mobile fallback`).

Environment: Windows (win32), Node.js + npm from the local install, Rust toolchain
invoked via `%USERPROFILE%\.cargo\bin\cargo.exe` (cargo was not on `PATH` in the
audit shell). No Android SDK/emulator run was attempted in this session, and no
NSIS/Tauri release build was produced (SDK download + signing are environment
dependent); frontend production build and the full Rust workspace test suite were
run.

## Results

| Gate | Command | Result |
|---|---|---|
| Frontend tests | `npm test` | PASS — 29 files, 587 passed, 1 skipped (`OSAK_PERF_HEAVY=1` heavy case), 0 failed |
| TypeScript | `npx tsc --noEmit` | PASS — no diagnostics |
| ESLint | `npm run lint` | PASS — 0 errors, 0 warnings |
| Frontend build | `npm run build` | PASS — vite production build, entry chunk 509 kB (gzip 155 kB) |
| Rust tests | `cargo test --workspace` | PASS — 503 passed, 0 failed, 3 ignored (heavy/perf + opt-in live WebDAV), 0 measured across all workspace targets |
| Rust clippy | `cargo clippy --workspace --all-targets -- -D warnings` | PASS — exit 0 |
| Rust fmt | `cargo fmt --all -- --check` | **FAIL (pre-existing)** — repository-wide formatting backlog; CI only enforces files added by a change (`npm run format:gate:rust`). Not caused by this audit. |
| Prettier | `npm run format:check` | **FAIL (pre-existing)** — 87 files fail on this tree; untouched files such as `src/screens/Home.tsx`, `src/screens/Merge.tsx` and `src/App.tsx` fail too, so this is the same staged-backlog situation as rustfmt. CI enforces `format:gate` (files a change adds), not this repo-wide check. Not caused by this audit. |

## Known environment limitations at baseline

- `cargo` is not on `PATH`; commands must use `%USERPROFILE%\.cargo\bin\cargo.exe`.
- Android Gradle/emulator build (`npm run android:build`, `connectedAndroidTest`)
  was not run in this environment; it remains a CI-only gate.
- The Tauri release build/`package` step was not run (needs SDK/NSIS/signing);
  `vite build` plus `cargo test`/`clippy` cover the code compiled here.
- The `#[ignore]`d heavy performance cases and the opt-in live WebDAV test are
  excluded by design.

## Pre-existing failures to not attribute to this change

1. `cargo fmt --all -- --check` fails on the pre-existing tree (documented in
   README "Quality gates": the formatting gate is staged and the repo-wide
   backlog is printed but not enforced).
2. `npm run format:check` (Prettier over all of `src/`) fails on 87 files on
   the pre-existing tree; untouched files reproduce it, so it is the same
   staged backlog (README: "Neither rustfmt nor Prettier had ever run over
   this code base, so the formatting gate is staged").
   No other pre-existing test, lint or type failure was observed.
