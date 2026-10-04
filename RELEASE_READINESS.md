# Release Readiness — Office Swiss Army Knife 3.5.6

This file states what is actually implemented, tested and benchmarked, and
what is not. It is deliberately conservative: nothing is claimed as released,
built or verified unless it was reproduced in this environment or is produced
by CI.

## Implemented (this cycle)

- **Home opens a single picked/dropped file** through the same routing as
  open-with (office → workspace, PDF → reader, image → image-to-PDF); the
  picker accepts every format the suite opens and the Home presentation leads
  with the office suite. `src/lib/open-route.ts` is the pure, tested routing.
- **Multi-output exports are reliable**: the overwrite default matches the UI
  ("create new") and PDF → images writes into `<document>_images/`, so a
  second export cannot collide (verified live on Windows: 5 pages written,
  then a rerun produced `page_001 (1).jpg` names without error).
- **Android staging folder is created** before the first native tool run; this
  is what made merge work on Windows but fail on Android.
- **Converter and cleaner use the SAF bridge on Android** (the desktop dialog
  plugin cannot open the Android picker), stage in the app cache and publish
  results to the chosen folder or Downloads. PDF input offers JPG/PNG; PDF →
  Word/Excel layout reconstruction is not implemented.

## Implemented in 3.5.5 (unchanged)

- **XLSX comments are per sheet (audit C11).** `officecore` writes one
  comments part and one VML shape set per commented sheet, wired through the
  sheet's own relationships and content-type overrides; the regression test
  writes two sheets with a comment on A1 and re-reads the produced file.
- **Writer tracked changes are run-preserving (audit M12).**
  `src/office/writer/revisions.ts` diffs runs instead of flattened text:
  surrounding formatting, links, footnote/field anchors and existing revision
  ids survive, deleting freshly suggested text cancels the insertion, and a
  pending deletion is no longer accepted when revisions are hidden.
- **qpdf repair and Fast Web View.** `pdfcore::repair` runs the bundled qpdf
  (recovery rewrite or `--linearize`), re-opens the produced file, reports its
  real page count and the engine diagnostics; `pdf_repair`/`pdf_linearize` are
  registered Tauri commands with Jobs-screen retry and a PDF Studio tab.
- **Benchmark regression gate.** `scripts/check-bench-regression.mjs`
  compares criterion means against a baseline (`npm run bench:check`); the CI
  bench job restores the baseline from the action cache, fails above 15 %
  (warns above 7.5 %) and only saves the new baseline when the check passed.
- **Deep desktop E2E.** `npm run e2e:flows` (= `node e2e/smoke.mjs --all`)
  drives Writer type → Ctrl+S (the DOCX on disk must change), Merge auto-run
  and the Studio sanitizer through its real button; CI runs it on Linux next
  to the smoke test and failure screenshots are saved.
- **Formatting is enforced repository-wide.** Prettier and rustfmt ran over
  the whole tree once (one no-behaviour commit); CI checks all changed files
  and the full tree.
- **Dependencies**: `cbc` 0.2.1 + `des` 0.9 (the versions lopdf already uses;
  the cipher 0.5 `BlockMode` traits), unused `aes` dev-dependency removed.
  Deferred majors are ignored in `dependabot.yml` with their blocker.

## Tested in this environment

| Gate | Command | Result |
|---|---|---|
| Rust workspace | `cargo test --workspace` | 536 passed, 3 ignored (the heavy perf cases) |
| Rust clippy | `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| Rust formatting | `cargo fmt --all -- --check` | clean |
| Frontend unit/integration | `npm test` | 41 files, 687 passed, 1 skipped |
| Frontend coverage | `npm run test:coverage` | 64.4 / 71.6 / 48.8 / 64.4, floors 62 / 69 / 46 / 62 pass |
| Typecheck / lint / i18n | `npx tsc --noEmit`, `npm run lint`, `npm run i18n:audit` | clean, en=tr=1552 |
| Frontend formatting | `npm run format:check` | clean |
| E2E smoke (Windows, msedgedriver 154) | `npm run e2e:smoke` | passed: 38 tool cards, Settings navigation |
| E2E Reader (Windows) | `npm run e2e:smoke:pdf` | passed: reader rendered 1 page |
| E2E flows (Windows) | `npm run e2e:flows` | passed: Writer typed+saved, merge wrote output, sanitizer wrote output |
| qpdf repair tests | `cargo test -p pdfcore --test repair_test` | 2 passed (damaged file repaired, linearized) |
| Benchmark regression script | `npm run bench:check` | stores a baseline, fails a synthetic regression |
| Dependency audit | `npm audit --audit-level=high` | passes (three moderate dev-only advisories in vitest 3's mocker are documented in `dependabot.yml`) |

## Benchmarked

- Five criterion benchmarks: DOCX/XLSX/PPTX import and lossless PDF
  compression. The new gate compares means against the cached master baseline;
  the uploaded report stays the detailed comparison point.

## Not done in this pass (honest)

- **The vitest/vite majors are deferred, not adopted.** Vitest 4/5 change the
  coverage instrumentation completely (the same suite measures
  64.6/71.5/48.7/64.6 percent on 3.2.7 and 53.0/46.4/41.4/57.0 percent on
  4.1.11), so moving would re-baseline the enforcement floors and needs a
  deliberate change with new tests. Vitest 3.2.7 keeps the high-severity npm
  audit gate green; the moderate `@vitest/mocker` advisory
  (GHSA-82fw-gwwq-j7x9) is dev-only and fixed by that same move.
- **eslint 10, TypeScript 7, der 0.8 and rand 0.10 are blocked upstream**
  (jsx-a11y peer range, typescript-eslint `<6.1.0`, cms/x509-cert pin der 0.7,
  rsa uses rand_core 0.6) and documented in `.github/dependabot.yml`.
- **E2E still runs on Linux only in CI.** The Windows flows were run locally;
  the Windows CI matrix does not have the WebDriver wiring, and the Reader
  step needs the native engines.
- **The repair UI is desktop-only** (qpdf is not shipped on Android) and
  encrypted documents must be decrypted before qpdf can rewrite them.

## Platform support and artifacts

- **Windows**: source builds and tests locally; installer/portable ZIP via
  `npm run release:local` or CI, with SBOMs, checksums and provenance.
- **Android**: built by CI and by `npm run release:local`; artifacts are named
  `PDF-Swiss-Army-Knife-Android-3.5.5-<abi>.apk`.
- **Linux/macOS desktop**: built and tested by CI; the E2E job runs on Linux.
- **Chrome extension**: unchanged.

## Security status

Unchanged from 3.5.4. The qpdf invocation runs the bundled, SHA-256-pinned
binary with a null stdin and a captured, length-capped stderr; the repair
module never shells out to a user-controlled program. A cancelled repair kills
the child and removes the partial output.
