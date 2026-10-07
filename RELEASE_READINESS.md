# Release Readiness — OmniOffice 4.1.0

This file states what is actually implemented, tested and benchmarked, and
what is not. It is deliberately conservative: nothing is claimed as released,
built or verified unless it was reproduced in this environment or is produced
by CI.

## Implemented (this cycle, 4.1.0) - PDF tools (plan phase 3)

- `render.rs` `render_page_region` + a two-document cache, `page_tile`
  command, `src/lib/tiles.ts` (tile math, LRU, request queue) and the Reader
  overlay above `MAX_PREVIEW_RASTER_WIDTH`.
- `rebuild.rs`: raw-object repair; `repair.rs` picks qpdf (Windows bundle,
  PATH, Homebrew) or the built-in engine and reports `method`.
- `ttfsubset.rs` (glyph-preserving subset), `fontusage.rs` (glyph usage
  through content, forms, patterns, soft masks, Type3, annotations),
  `fontembed.rs` subset tags and Type0 `CIDToGIDMap`.
- Revocation: OCSP/CRL request and response handling in pdfcore,
  `pdf_verify_signatures_online` with `netpolicy.rs` (public addresses only,
  filtered DNS, 20 requests / 45 s), setting `onlineRevocationCheck`.
- `pdf2doc.rs` layout recovery wired into PDF to DOCX.
- An independent review found seven issues, all fixed with regression tests
  in `crates/pdfcore/tests/hostile_inputs.rs` and the module tests: an
  unbounded allocation in PDF to Word (process abort), ToUnicode CMaps
  re-parsed per string, inherited attributes copied into every page during
  repair, revocation requests to private addresses and without a request
  cap, glyphs dropped from fonts used only in soft masks, and tiles shown on
  the wrong document.
- Tests: pdfcore lib 119, `repair_builtin` 19, `pdfa_fonts` 8, `revocation`
  31, `pdf2doc_test` 3, `hostile_inputs` 5, app crate 99, frontend 811.

### Not done in this pass (honest)

- pdfium is not available in the Linux test environment: the region-render
  comparison and the pdfium path of PDF to Word run only in the Windows CI
  job. The Android repair path was not run on a device.
- The revocation check was not tried against a live certificate authority;
  behind a proxy without local DNS the revocation URLs cannot be resolved.
- PDF to Word: no password support in the converter yet, no tables, and a
  second numbered list continues the first one's numbering in Word.
- Times and Courier still fall back to PT Sans (not metric-compatible) during
  PDF/A embedding.
- Repair peak memory is about three times the file size.

## Implemented in 4.0.0 - office formats and tools (plan phase 2)

The phase was re-scoped after checking the code: XLSX charts, PPTX
animations/groups and DOCX comments already round-tripped, so the work went
to the ODF/RTF gaps listed as "lost" in `compat.rs`.

- `odf.rs` / `rtf.rs`: ODT and RTF comments (export and import); ODP nested
  groups and SMIL animations; ODS charts as `Object N/` chart documents with
  manifest entries, and pivot output as values. `compat.rs` updated (ODT
  comments kept unless they have replies, RTF comments "transformed", ODP
  groups and ODS charts "unchanged", ODP animations "partial").
- Tests: `crates/officecore/tests/odf_rtf_comments.rs` (8) and
  `odf_objects.rs` (13), plus an XLSX list-reference test; 197 officecore
  tests pass. ODP and ODS output was also opened and re-saved with
  LibreOffice 24.2 during development.
- Calc `calc/data-tools.ts` (text to columns, duplicates, list items,
  formula remapping), Writer `writer/find-replace.ts`, `writer/regex-probe.ts`
  and `writer/StyleGallery.tsx`, Turkish templates in `templates.ts`; 784
  frontend tests pass.
- An independent review of the merged work found six issues, all fixed
  before release: unbounded memory for hostile ODS charts, quadratic comment
  import, regex freezes, Remove Duplicates shifting outside references, XLSX
  list references, charts anchored past row 100,000.

### Not done in this pass (honest)

- The ODT reader still ignores character styles on spans (bold/italic from
  an ODT are lost on import), and the RTF reader starts a new paragraph at
  every group. Both predate this release; comments are unaffected.
- Comment reply dates and initials are not kept in ODT/RTF; RTF comment ids
  are renumbered.
- ODP charts are still placeholders; scatter charts are not supported in any
  format.
- Animation details: grow/shrink use LibreOffice's 150 %/50 % presets and
  fly-in/out use one direction.
- Remove Duplicates updates references inside the moved block only;
  formulas elsewhere that point into the block are not adjusted.

## Implemented in 3.9.0 - user experience (plan phase 1)

- `src-tauri/src/update.rs`: `update_check` reads
  `api.github.com/repos/ozanstn1-stack/omnioffice/releases/latest` and picks
  the asset for the running OS/arch (`OmniOffice-Setup-*.exe`, or the arm64 /
  armv7 APK); `update_open` only opens URLs under this repository's releases.
  Weekly by default (`updateCheck`, `lastUpdateCheck`), dismissible per
  version (`releases/latest` already skips drafts and prereleases). Unit
  tests: version parsing and comparison, asset choice per platform, foreign
  download links dropped, bad tags rejected, the URL allow-list; frontend
  tests for the schedule and the banner rules.
- Android printing (`useOfficeSession.print`): `office_export_pdf` into the
  cache, then the system viewer. A native `PrintManager` bridge was not added;
  the viewer's own print menu covers it. Tested with a mocked backend.
- `src-tauri/src/diagnostics.rs`: report with version, platform, engines,
  the last 20 jobs (no titles/payloads) and the last 150 frontend log lines;
  quoted and unquoted paths are masked. It is a `.txt`, not a `.zip` as the
  plan said: one readable file is easier to check before attaching it.
- Recent files for office documents (`rememberOfficePath`), Home routing to
  the editor, welcome card (`onboardingDone`, skipped for upgrades).
- Calc touch: tap selects, one-finger drag pans, corner grips extend the
  selection; ribbon folding below 760 px (`firstRowHeight`). Unit tests cover
  both; checked visually at 390 x 844 in Chromium with a mocked backend.
  Not yet tried on a physical phone.

## Implemented in 3.8.3 - release and CI infrastructure

- `auto-tag.yml`: version change on master -> tag + `workflow_dispatch` of
  `release.yml`/`android.yml` on the tag (GITHUB_TOKEN tags do not fire
  `push: tags`). The same mechanism published v3.8.2 by hand.
- `check-bench-regression.mjs`: median-ratio machine factor (>= 3 comparable
  benchmarks); `--criterion <dir>` for local checks. Verified with synthetic
  trees: the real October drift passes (+10.9 % relative warning), a single
  +33.7 % regression fails, and a +40 % regression hidden in a +70 % runner
  shift still fails.
- `e2e/smoke.mjs`: the post-pass hang (driver pipes held by a surviving
  WebKit helper) is fixed by closing the pipes and exiting explicitly -
  reproduced locally with a fake driver (old: still running at 20 s, new:
  exits in 1 s); 90 s bound per WebDriver call; step time limits in CI.
- `e2e-windows` job: engines fetched, debug build with a test-only
  `--config` adding `--remote-debugging-port=0` to wry's WebView2 arguments
  (found with a process probe: the runtime ignored msedgedriver's
  environment arguments), msedgedriver matched to the WebView2 runtime;
  smoke + Reader (pdfium) + deep flows pass on the PR.
- `scripts/android-launch-smoke.sh`: the debug APK is started on the CI
  emulator and must log the engine status from the frontend. An
  instrumentation version was dropped: logcat showed the app starting fine
  and then Tauri's exit() on activity destruction (plus the emulator's EGL
  teardown abort) killing the shared test process.

## Implemented in 3.8.2

A feature-by-feature audit of the Android and Windows code paths found
screens that still used the desktop dialog plugin on Android and two desktop
save paths that could not succeed. Fixed:

- `pickFileBytes` / `saveFileBytes` (`src/lib/mobile.ts`): one read/write path
  for a user-chosen file - desktop dialog + plugin-fs, or the Android SAF
  picker + `content://` read/write. Used by Writer "Insert image", the Impress
  image placeholder, Data import/export and Draw SVG/PNG/PDF export.
- PDF Forms, the PDF Studio picker and Sync "Add file" pick through SAF on
  Android; PDF Forms output is staged in the cache and published to the chosen
  file. The AI library folder chooser is hidden on Android.
- Draw -> PDF is built in memory (`src/lib/image-pdf.ts`, JPEG page), so the
  desktop no longer writes a temporary PNG outside the fs scope.
- PDF Forms save and AI "Apply metadata" use the `replace` policy after the
  save dialog confirmed the overwrite (was `error`, which refused it).
- `mimeForName` returns real office/CSV/JSON/SVG types for published files.
- CSV import follows RFC 4180; JSON import accepts more shapes.

| Gate | Command | Result |
|---|---|---|
| Frontend | `npm test` | 44 files, 698 passed, 1 skipped (new: file bridge, CSV/JSON parsers, JPEG->PDF) |
| Typecheck / lint / i18n / format | `npx tsc --noEmit`, `npm run lint`, `npm run i18n:audit -- --check`, `npm run format:check` | clean, en=tr=1592 |
| Frontend build | `npm run build` | passes |
| Rust workspace (Linux) | `cargo test --workspace` | 564 passed, 3 ignored, with a Linux pdfium on `LD_LIBRARY_PATH` |

Not verified here: no Android device/emulator and no Windows machine were
available; the Android SAF calls are covered by unit tests with the plugin
mocked, and the APK/EXE are built by the tag workflows.

## Implemented in 3.8.1

- **Template click fix**: `TemplatesScreen` now receives an `onOpen` callback
  and the shell passes `navigate("office")`, so picking a template creates the
  tab *and* shows the Office workspace. Verified with tauri-driver against the
  debug build (clicked the "CV" template; `.office-workspace` opened with the
  "CV" tab and the template body) and by a unit test that asserts the
  navigation callback fires.

## Implemented in 3.8.0

- **New brand mark**: a gradient squircle with a white "O" ring and sparkle,
  generated by `scripts/make-icon.ps1` and fanned out with `tauri icon` to
  Windows/macOS icons, Android adaptive icons (mipmaps committed under
  `src-tauri/gen/android/...` and mirrored in `src-tauri/icons/android`), the
  Chrome extension icons and a new SVG favicon; the in-app `BrandMark` mirrors
  the same design in the sidebar, drawer and Home hero.
- **UI modernization**: refined light/dark palettes (deeper surfaces, softer
  shadows, 16 px radii), gradient primary buttons, branded active nav items,
  segmented controls, badges, tool cards, drop zones, progress bars and
  slimmer scrollbars; the sidebar privacy badge was shortened so it no longer
  wraps. No layout or workflow changes.
- Verified visually with tauri-driver screenshots (Home dark/light, Settings,
  PDF Studio, AI) from the debug build.

## Implemented in 3.7.0

- **Renamed to OmniOffice** across the Windows product name, window title,
  executable (`OmniOffice.exe`), installer/portable names, Android artifacts
  and app label, Chrome extension (`OmniOffice-Chrome-Extension-1.0.2.zip`),
  per-user install path (`%LOCALAPPDATA%\Programs\OmniOffice`), shortcuts,
  About dialog, docs and the generator/creator metadata written into produced
  PDF/DOCX/XLSX/PPTX/ODF files.
- **Upgrade safety**: the package identifier, `.oswk` format tag and OAuth
  keyring service name are unchanged; the AI library falls back to the legacy
  `Documents/PDF Swiss Army Knife AI` folder while `Documents/OmniOffice AI`
  does not exist; Android results go to `Downloads/OmniOffice`; new sync
  setups default to the "OmniOffice" remote folder.
- **Updater cleanup**: `install-local.ps1` removes the old per-user install
  folder and shortcuts after a successful update; per-machine installs need
  the elevated uninstaller (documented).

## Implemented in 3.6.1

- **Writer wrapping fix** (`src/styles.css`): `.para`/`.para-row` no longer
  keep a flexbox `min-width: auto`, so a paragraph (or an unbroken word/URL)
  shrinks to the A4 text column and wraps instead of running past the page
  edge.
- **AI assistant office input** (`src-tauri/src/ai.rs` plus the format
  plumbing): DOCX/DOCM/DOTX, ODT, RTF, legacy DOC/DOT, TXT/MD/Markdown/HTML,
  XLSX/XLSM/XLS/ODS, CSV/TSV, PPTX/PPTM, ODP, legacy PPT and native `.oswk`
  are extracted as text units (writer chunks, one unit per sheet, one unit per
  slide with notes) and accepted by the picker, drop zone and Android SAF
  import. Unsupported extensions fail with a clear message, and the PDF-only
  metadata "Apply" action is hidden for office inputs while the suggestion
  still shows.

## Implemented in 3.6.0

- **Content-stream text editing** (`pdfcore::content`): a page's decoded
  content is walked with graphics/text state (q/Q, Tf, Tm, Td/TD, T*, TL,
  Tc/Tw/Tz/Ts/Tr); runs are listed with font, size and approximate position;
  a run's text is replaced in place as an **incremental revision** (the
  original bytes and any signature stay intact). Replacements are encoded with
  the run's font and accepted only when the byte round trip reproduces the
  text; multi-string `TJ` and non-representable runs are reported read-only.
  PDF Studio gains the "Text" tab.
- **Reader bookmarks**: `pdf_info` carries the bounded outline (with `/Dest`
  page resolution); the Reader side panel lists it and jumps to pages.
- **RFC 3161 timestamps** (`pdfcore::timestamp` + the signing dialog): an
  optional TSA URL timestamps the signature value over HTTPS (loopback HTTP
  allowed for a local TSA) and embeds the token as `id-aa-timeStampToken`;
  verification reports the token's `genTime`. A requested timestamp is
  mandatory - a TSA failure fails signing.

## Tested in this environment

| Gate | Command | Result |
|---|---|---|
| Content editor | `cargo test -p pdfcore --lib content` | 6 passed (list, replace, incremental prefix, `TJ` refusal, font-encoding refusal, unknown page) |
| RFC 3161 core | `cargo test -p pdfcore --lib timestamp` | 4 passed (request shape, granted/refused responses, garbage) |
| Signature + timestamp | `cargo test -p pdfcore --test signature_test` | 17 passed (incl. embedded token verified and provider-failure path) |
| AI office extraction | `cargo test -p pdf-swiss-army-knife --lib ai` | 9 passed (DOCX/XLSX/PPTX units, unknown extension refused, password redaction) |
| Frontend | `npm test` | 42 files, 691 passed, 1 skipped |
| Typecheck / lint / i18n | `npx tsc --noEmit`, `npm run lint`, `npm run i18n:audit` | clean, en=tr=1592 |
| Rust clippy | `cargo clippy -p pdfcore -p pdf-swiss-army-knife --all-targets -- -D warnings` | clean |
| Rust workspace | `cargo test --workspace` | 565 passed, 3 ignored (the new suites included) |

## Not done in this pass (honest)

- **PDF/A font subsetting** (Type0/CID remapping) is not implemented; the
  existing PDF/A conversion still reports unembeddable fonts instead of
  guessing.
- **Tiled rendering** for extreme zoom remains open; the reader still upscales
  beyond the cached bitmap on very high-density screens.
- **OCSP/CRL revocation** is not fetched; signature trust stays `unknown`
  offline, and a timestamp's TSA chain is not validated.
- **Vector-object editing** inside content streams is read-only; only text
  runs can be edited.
- Text-run positions/widths are approximations (no glyph metrics); the UI and
  docs say so.

## Implemented in 3.5.7, 3.5.6 and 3.5.5 (unchanged)

- **Legacy `.doc`/`.ppt` import** (`officecore::legacy`): the Word FIB piece
  table (compressed + UTF-16 pieces) and the PowerPoint record-tree text atoms
  per `Slide` container are read from the OLE2 container; a locally installed
  LibreOffice is used first for full-fidelity conversion (`PDFSAK_SOFFICE` or
  `soffice` on PATH, fixed args, 120 s timeout, private temp dir). Legacy tabs
  have no save path: the first Ctrl+S asks for `.docx`/`.pptx`/`.oswk`.
  Compatibility matrix and README table updated to the honest flags.
- **OAuth 2.0 PKCE cloud sync** (`synccore::oauth`, `synccore::cloud`,
  `src-tauri::oauth`): S256 PKCE (RFC 7636 vector test), loopback listener with
  state verification, token exchange/refresh, Google Drive v3 and Microsoft
  Graph providers with streaming transfers and the existing conflict contract
  (Graph `If-Match`; Drive `sha256Checksum` comparison). Tokens are stored in
  the OS credential vault on desktop; the fallback store is reported in the UI.
- **Converter completion**: PDF → JPG/PNG/TXT/DOCX and image → PDF inside
  `office_convert`, with the targets list updated accordingly.

## Tested in this environment

| Gate | Command | Result |
|---|---|---|
| Rust workspace | `cargo test --workspace` | 552 passed, 3 ignored (incl. the legacy and OAuth suites) |
| Rust clippy | `cargo clippy -p synccore -p officecore -p pdf-swiss-army-knife --all-targets -- -D warnings` | clean |
| Frontend unit/integration | `npm test` | 42 files, 688 passed, 1 skipped |
| Frontend coverage | `npm run test:coverage` | 65.4 / 71.3 / 48.9 / 65.4, floors 62 / 69 / 46 / 62 pass |
| Legacy importer | `cargo test -p officecore --lib legacy` | 4 passed (piece table, PPT slides, corrupt input) |
| OAuth core | `cargo test -p synccore` | PKCE RFC vector, auth URLs, token parsing, loopback accept/reject, mock-server round trips for Graph and Drive, both conflict cases |
| Sync screen | `npx vitest run src/screens/sync-oauth.test.tsx` | connect flow + keychain note |

## Not tested here (honest)

- **No live OAuth sign-in was performed.** It needs real Google/Microsoft app
  client IDs and a browser account; the tests cover the protocol, loopback
  handling, token parsing and the provider HTTP surfaces against a mock
  server. A live run is the first thing to do with real client IDs.
- **LibreOffice conversion was not exercised** (no LibreOffice in this
  environment); the built-in CFB importer is the tested path. The bridge is
  opt-in and falls back cleanly.
- **Legacy import fidelity is text-level**: character formatting, tables,
  headers/footers, footnotes, images, animations and themes are not
  reconstructed; the import warns about this and saving goes to a modern
  format.
- Android builds pick `.doc/.dot/.ppt` through SAF, but no device/emulator was
  available here; Android compilation is covered by the release workflow.

## Implemented in 3.5.6 and 3.5.5 (unchanged)

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
