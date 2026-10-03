# Release Readiness — Office Swiss Army Knife 3.5.4

This file states what is actually implemented, tested and benchmarked, and
what is not. It is deliberately conservative: nothing is claimed as released,
built or verified unless it was reproduced in this environment or is produced
by CI.

## Implemented (this cycle)

- **Desktop E2E** (`e2e/smoke.mjs`, CI job `Desktop E2E (tauri-driver)`):
  WebDriver against the real binary; home grid + Settings navigation on every
  PR, screenshots uploaded.
- **Fuzzing** (`fuzz/`): `zip_archive`, `xml_parse`, `office_readers`
  targets with a seeded corpus; nightly CI job on master (45/30/60 s).
- **Benchmark trends**: `crates/officecore/benches/parse.rs` and
  `crates/pdfcore/benches/pdf_ops.rs`; master CI job uploads the criterion
  report.
- **Dependency policy**: `deny.toml` gained `[licenses]` (permissive
  allow-list), `[bans]` (duplicate majors warn, wildcards deny, path
  wildcards allowed) and `[sources]`; `src-tauri` is `publish = false`, and
  the RUSTSEC-2024-0429 exception was retired.

## Tested in this environment

| Gate | Command | Result |
|---|---|---|
| E2E smoke (no engines) | `npm run e2e:smoke` (Windows, msedgedriver 154) | passed: 38 tool cards, Settings navigation |
| E2E Reader | `npm run e2e:smoke:pdf` | passed: reader rendered 1 page (screenshot in `e2e/artifacts/`) |
| Fuzz targets | `cargo +nightly check --manifest-path fuzz/Cargo.toml --bins` | compiles (libFuzzer linking is Linux/macOS-only, so runs are CI-only) |
| Benchmarks | `cargo bench -p officecore --bench parse -p pdfcore --bench pdf_ops` | docx 1.04 ms, xlsx 2.59 ms, pptx 1.53 ms, pdf lossless ~13.1 ms on this machine |
| Dependency policy | `cargo deny check` | advisories ok, bans ok, licenses ok, sources ok |
| Rust clippy | `cargo clippy -p officecore -p pdfcore --all-targets -- -D warnings` | clean |
| Frontend unit/integration | `npm test` | 38 files, 673 passed, 1 skipped (unchanged) |
| Rust workspace | `cargo test --workspace` | 533 passed, 3 ignored (unchanged) |

New CI jobs: `e2e` (PRs + master), `fuzz` (master/dispatch), `bench`
(master/dispatch). The audit job now runs `cargo deny check` (all four
sections) instead of advisories only.

## Benchmarked

- Five criterion benchmarks: DOCX/XLSX/PPTX import and lossless PDF
  compression. No thresholds are enforced yet; the uploaded report is the
  trend baseline (compare with the previous artifact).

## Not done in this pass (honest)

- **Fuzzing does not run on Windows** (libFuzzer links on Linux/macOS only);
  only the Linux CI job exercises it. Local runs here verified compilation,
  not the fuzzing itself.
- **No benchmark thresholds or automatic regression alerts** - trend review is
  manual against the previous artifact.
- **E2E coverage is a smoke test**: home grid, Settings navigation and (on
  Windows) one Reader render. Dialogs, editors and the save round trip are
  not driven yet.
- **C11 (XLSX cross-sheet comments)** and the red Dependabot majors are still
  open, as is the Phase 3/4 roadmap.

## Platform support and artifacts

- **Windows**: source builds and tests locally; installer/portable ZIP via
  `npm run release:local` or CI, with SBOMs, checksums and provenance.
- **Android**: built by CI and by `npm run release:local`; artifacts are named
  `PDF-Swiss-Army-Knife-Android-3.5.4-<abi>.apk`.
- **Linux/macOS desktop**: built and tested by CI; the E2E job runs on Linux.
- **Chrome extension**: unchanged.

## Security status

Unchanged from 3.5.3, plus: the dependency policy is now enforced on licenses,
duplicate versions, wildcards and registry sources; the fuzz targets add
continuous adversarial testing of the hardened ZIP/XML/reader layers.

## Known limitations

See the README "Known limitations" section (single source of truth).
