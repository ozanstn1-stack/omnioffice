# Contributing to OmniOffice

Thank you for helping. This file says how a change gets from your branch into a release. The
architecture is described in [`docs/architecture.md`](docs/architecture.md), and security reports
go through [`SECURITY.md`](SECURITY.md), never a public issue.

## Before you start

- Open or pick an issue first for anything larger than a small fix, so the approach can be agreed
  before you write it.
- Keep a pull request small: under about 400 lines of non-generated code. Split larger work into
  steps that each leave `master` releasable.
- Nothing is claimed that is not shipped. A limitation is written down, not hidden.

## Setting up

```sh
npm ci                 # frontend dependencies
npm run engines:fetch  # pdfium, qpdf, Tesseract and fonts the PDF tools need (PowerShell)
npm run app:dev        # the desktop app with hot reload
```

Rust is pinned by `rust-toolchain.toml`. Android builds need the SDK and NDK; see
[`docs/android.md`](docs/android.md).

## The checks a pull request must pass

Run these before you push; CI runs the same ones on Windows, Linux and macOS.

| What                                                               | Command                    |
| ------------------------------------------------------------------ | -------------------------- |
| Lint, i18n audit, type check, unit tests                           | `npm run check`            |
| Everything above plus formatting, Clippy, Rust tests and the build | `npm run check:all`        |
| Formatting of the files you changed                                | `npm run format:gate`      |
| File-size ratchet                                                  | `npm run files:check`      |
| Every RELEASE_READINESS claim points at a real test                | `npm run claims:check`     |
| README statistics are current                                      | `npm run docs:stats:check` |
| Bundle budget (after `npm run build`)                              | `npm run bundle:check`     |

Notes that save a CI round trip:

- **i18n:** every UI string exists in both `src/lib/i18n/en.json` and `src/lib/i18n/tr.json`; run
  `npm run i18n:audit -- --check`.
- **File sizes only shrink.** New code goes into new files instead of growing the large editors. If
  a file must grow, raise its budget on purpose with
  `node scripts/check-file-sizes.mjs --update --allow-raise` and say why in the pull request.
- **New test files** change the README statistics: run `npm run docs:stats`.
- **Generated files** (`src/lib/generated/`, `src-tauri/gen/`, lockfiles) are never edited by hand;
  regenerate them (`npm run bindings:specta`, `npm install`, `cargo update -p ...`).
- **Line endings:** a few large files use CRLF. Keep the ending a file already has.
- Rust library crates forbid `unsafe` and panicking shortcuts (`unwrap`, `expect`) in production
  code; return an error instead.

## Pull requests

Fill in `.github/pull_request_template.md`: what changed and why, the linked issue, and which test
or E2E scenario proves it. Pull requests are squash-merged once CI is green.

## Releases

A release is its own pull request that bumps the version everywhere it is written:
`package.json` and `package-lock.json`, `Cargo.toml` and `Cargo.lock`, `src-tauri/tauri.conf.json`,
`src-tauri/gen/android/app/tauri.properties` (`versionName` and `versionCode`), the README version
and downloads block, a `CHANGELOG.md` section, a new "Implemented (this cycle, X)" section in
`RELEASE_READINESS.md` whose claims name their tests, and the store notes under
`fastlane/metadata/android/` (`npm run fastlane:sync` writes en-US; tr-TR is written by hand).
`node scripts/release-guard.mjs` checks that they agree.

Once the release pull request is merged and the desktop CI passes on `master`, the version is
tagged automatically and the Windows, Linux and Android packages are built and attached to the
GitHub release with their checksums and provenance.
