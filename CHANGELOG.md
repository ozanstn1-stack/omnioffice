# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [4.1.0] - PDF tools

### Added

- **Sharp pages at high zoom.** Past the size of a single page bitmap, the
  reader draws the visible part of the page as tiles rendered at full
  resolution, so text stays crisp up to 400 %. Normal zoom is unchanged.
- **PDF repair without qpdf.** A built-in engine rebuilds damaged files: it
  recovers objects from the raw bytes (broken or missing cross-reference
  tables, wrong stream lengths, truncated files, object streams) and rebuilds
  the page tree when the catalog is lost. Repair now works on Android; qpdf is
  still used when it is installed (also from PATH on Linux and macOS), and the
  result says which engine ran.
- **Smaller PDF/A files.** Fonts embedded during PDF/A conversion now contain
  only the characters the document uses (a Helvetica text came out at about
  10 % of the full font), and CID fonts without an embedded program are
  embedded when they carry a ToUnicode map.
- **Optional certificate revocation check** (Settings, off by default). When
  turned on, signature verification asks the certificate authority's OCSP
  responder, or downloads its CRL, and shows "Not revoked" or "Revoked on …".
  A certificate that is not revoked is not called trusted; the trust state
  stays "unknown" without a trust store.
- **PDF to Word keeps the layout.** Headings, paragraphs (joined across line
  breaks and hyphenation), bulleted and numbered lists, bold/italic, two-column
  pages and page breaks are recovered; running headers, footers and page
  numbers are dropped.

### Security

- Revocation requests only go to public internet addresses on ports 80/443
  (checked again after DNS resolution and on every redirect), and one
  verification makes at most 20 requests within 45 seconds.
- Hostile PDFs that could crash or stall PDF to Word, text extraction, repair
  or PDF/A font embedding are bounded (found by an independent review before
  release, each with a regression test).

## [4.0.0] - Office formats and editing tools

### Added

- **Comments in ODT and RTF.** Writer comments are written to and read from
  ODT (`office:annotation` ranges, LibreOffice's resolved flag) and RTF
  (Word-style annotations). Replies are written as "Re: Author: text"
  paragraphs, as for DOCX, and read back as replies.
- **ODP keeps groups and animations.** Groups are written as nested
  `draw:g` and read back; slide animations (entrance, exit and emphasis
  effects, on click / with previous / after previous, duration and delay)
  use LibreOffice's own timing presets in both directions.
- **ODS keeps charts.** Column, bar, line, pie and area charts are written
  as embedded chart objects with titles, legend, colours, labels, stacking
  and cached values, and are read back. Pivot tables are written as their
  computed values.
- **Calc data tools** (Data tab): Text to columns (comma, semicolon, tab,
  space or a custom delimiter, quoted text, preview) and Remove duplicates
  (column choice, header row), each one undo step. A list validation shows
  a dropdown arrow on the cell (mouse, touch and Alt+Down); a list can also
  come from cells, such as `=A1:A5`.
- **Writer find & replace**: regular expressions with `$1` groups, a single
  Replace, a live match count, and whole-word matching that understands
  Turkish letters. A pattern that would take too long is stopped with a
  message instead of freezing the editor.
- **Quick style gallery** in the Writer Home ribbon (Normal, Title,
  Heading 1-3, Quote) with a preview of each style.
- **Turkish templates**: Dilekçe, Özgeçmiş, Toplantı Tutanağı, Fatura (KDV
  %20 with formulas, ₺ format) and Bütçe Tablosu. They carry a "TR" badge
  and are listed first when the app is in Turkish.

### Fixed

- **ODT import put comment text into the document.** An inline comment's
  author, date and text became body text. Text after an inline element was
  also read out of order ("Before **bold** after" became "Before after
  bold"), which misplaced footnote markers too.
- **ODP import lost positions and pictures.** Shape positions and sizes and
  picture links were looked up under the wrong attribute names, so every
  shape came back at the default position and pictures were dropped; shapes
  inside groups were skipped.
- **ODS import named every sheet "Sheet"**, and cells after an empty gap of
  more than 2,048 rows landed on the wrong row.
- **XLSX list validation from cells** is written as a reference that Excel
  resolves, instead of a one-item list containing the text "=A1:A5".
- **Hostile files**: a small ODS whose chart declared thousands of series
  could exhaust memory, and a document with tens of thousands of comments
  took minutes to open. Both are bounded now.

## [3.9.0] - Update notice, Android printing, mobile layout

### Added

- **"New version available" notice.** Once a week (and from Settings -
  "Check for updates") the app asks GitHub's public release list for the
  latest OmniOffice release. When it is newer, Home shows a banner that opens
  the Windows installer or the Android APK for this device (arm64 or armv7);
  only links under this repository's releases are opened. The check sends
  nothing about the user or their documents and can be turned off.
- **Printing on Android.** Writer, Calc and Impress "Print" did nothing in the
  Android WebView. The document is now rendered to a PDF in the app cache and
  opened in the system viewer, whose menu prints it. Desktop printing is
  unchanged.
- **Export diagnostics** (Settings). Writes a plain-text report - app and core
  version, platform, engine status, the last 20 jobs and the tail of the app
  log - to a file the user picks, to attach to a GitHub issue. Job titles and
  inputs are left out and every path is reduced to `<path>` plus its
  extension; nothing is sent anywhere.
- **Welcome card** on Home for new installs: the office suite, the PDF tools
  and the optional AI setup, one tap each. Upgrades skip it; Settings can show
  it again.
- **Office documents in Recent files.** Documents opened or saved in Writer,
  Calc and Impress are listed on Home and in the Documents / Spreadsheets /
  Presentations launchers; "Open" takes them back to their editor.
- **Touch selection in Calc.** One finger on the sheet now scrolls it, a tap
  selects a cell (a double tap edits it) and two round grips on the
  selection's corners drag its extent. The fill handle sits just outside the
  bottom-right grip. Mouse and pen input are unchanged.
- **Folding ribbon on phones.** At phone width the Writer, Calc and Impress
  ribbons show their first row of groups and a "More" button reveals the rest,
  instead of a long sideways scroll.

### Fixed

- Opening a recent document from a launcher skipped the legacy-format import
  and the "changed on disk" fingerprint; it now uses the same opener as every
  other entry point.

## [3.8.3] - Automatic releases and reliable CI

No change to the app itself; this release makes every following update reach
GitHub on its own and keeps CI trustworthy.

### Added

- **Automatic release tag** (`.github/workflows/auto-tag.yml`). When a merge
  to master changes the version, the merge commit is tagged `vX.Y.Z` and the
  Release and Android workflows are started on that tag, so the Windows
  installer/portable ZIP and the Android APK/AAB are attached to the GitHub
  release without a manual `git push --tags`. An existing tag is left alone.
- **Windows desktop E2E** (`e2e-windows` in `desktop.yml`): the WebView2 build
  driven by msedgedriver runs the smoke test, a real pdfium Reader render and
  the Writer/Merge/Sanitize deep flows on every pull request. wry always passes
  its own WebView2 browser arguments, and the runtime then ignores the
  `--remote-debugging-port` msedgedriver requests through
  `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`; the test build therefore adds the
  port through a build-time `--config` (the shipped app is unchanged).
- **Android launch smoke test** (`scripts/android-launch-smoke.sh`): on the CI
  emulator the debug APK is installed and started like a user would, and the
  test passes once the frontend has run and called into Rust. It runs outside
  instrumentation on purpose: Tauri ends the process when its activity is
  destroyed, which took an instrumentation-based test runner down with it.

### Fixed

- **Benchmark gate failed on a slower runner.** Every benchmark moved by
  +25..+88 % at once in October 2026 - a runner change, not a code change -
  and the gate stayed red. The median current/baseline ratio is now factored
  out, so only a benchmark that regressed against the others fails; a uniform
  shift is still reported as a warning.
- **E2E could hang for half an hour after passing.** Helper processes started
  under tauri-driver (WebKit's network process on Linux) inherit its output
  pipes and outlive it, so the harness waited for EOF forever after every
  scenario had passed. The pipes are now closed when a session ends and the
  harness exits explicitly; each WebDriver call is also bounded (90 s) and the
  E2E steps have time limits.

## [3.8.2] - Android file pickers and save fixes

### Fixed

- **Android: several tools opened no picker or could not save.** The Writer
  "Insert image", the Impress image placeholder, Data import/export (CSV,
  JSON), Draw export (SVG, PNG, PDF), PDF Forms (open and save), the PDF
  Studio file picker and Sync "Add file" still called the desktop dialog
  plugin, which cannot open the Android picker. They now use the Storage
  Access Framework: files are read from and written to the picked
  `content://` document, and PDF Forms output is staged in the app cache and
  published to the chosen destination. The AI library "Choose folder" button
  (a desktop path picker) is hidden on Android.
- **Android: exported office files opened as "unknown file".** Published
  DOCX/XLSX/PPTX/ODF/CSV/JSON/SVG files carried a generic MIME type, so the
  "open with" chooser offered no office app. They now carry their real type.
- **Windows: Draw -> PDF always failed.** The export wrote a temporary PNG next
  to the chosen PDF, outside the file-system scope the save dialog grants, so
  the write was refused. The PDF is now built in memory (the drawing as one
  JPEG page at its own size) and written in one step.
- **Windows: overwriting a file from PDF Forms and AI "Apply metadata"
  failed.** The save dialog had already confirmed the replacement, but the
  command was sent with the "error if exists" policy and refused it.
- **Data import split quoted cells.** CSV import cut `"Doe, Jane"` into two
  cells and ignored escaped quotes and quoted line breaks; it now follows
  RFC 4180, detects `;`-separated files and strips a BOM. JSON import accepts
  objects with differing keys, a single object or an array of arrays.
- **Impress images.** GIF, WebP and BMP pictures were labelled `image/png`;
  they keep their real type now.
- **Tests:** the Windows-only file-lock test is compiled on Windows only, so
  `cargo test --workspace` builds on Linux again.

## [3.8.1] - Templates open the editor again

### Fixed

- **Clicking a template did nothing visible.** The template cards created a
  document tab in the office store but never switched to the Office
  workspace, so the click looked dead. Picking a template now opens the
  workspace with the new document (Writer, Calc or Impress) in the active
  tab; covered by a test that asserts the navigation callback fires.

## [3.8.0] - New brand mark and modern UI

### Changed

- **New logo.** The OS icon is now a rounded indigo -> violet gradient tile
  with a white "O" ring and a sparkle - no text, readable down to 16 px. The
  same mark is used in the sidebar, the drawer, the Home hero and as the
  favicon, and regenerates the Windows/macOS, Android adaptive-icon and
  Chrome-extension icons from one source (`scripts/make-icon.ps1` +
  `tauri icon`).
- **Modernized shell.** Refined dark and light palettes with deeper surfaces,
  softer shadows and 16 px radii; gradient primary buttons with a soft glow;
  nav items with a gradient active pill and indicator; branded segmented
  controls, badges, tool cards, drop zones, progress bars and scrollbars;
  gradient app title on Home. No layout or workflow changes.

## [3.7.0] - Renamed to OmniOffice

### Changed

- **The product is now OmniOffice.** The Windows product name, window title,
  executable (`OmniOffice.exe`), installer (`OmniOffice-Setup-<version>.exe`
  plus the Tauri-named `OmniOffice_<version>_x64-setup.exe`), portable ZIP
  (`OmniOffice-Portable-<version>.zip`), Android artifacts
  (`OmniOffice-Android-<version>-<abi>.apk/.aab`), Chrome extension
  (`OmniOffice-Chrome-Extension-1.0.2.zip`), per-user install path
  (`%LOCALAPPDATA%\Programs\OmniOffice`), shortcuts, About dialog, Android app
  label and the generator/creator strings written into produced PDF, DOCX,
  XLSX, PPTX and ODF files all use the new name.
- **Upgrade safety.** The package identifier
  (`io.github.ozanstn1.pdfswissarmyknife`), the `.oswk` format tag
  (`office-swiss-army-knife`) and the OAuth keyring service name are unchanged,
  so Android in-place updates, existing documents and stored cloud tokens keep
  working. After installing, the updater removes the old per-user install
  folder and shortcuts; per-machine installs under
  `C:\Program Files\Office Swiss Army Knife` need the elevated uninstaller.
- **Data paths.** The AI library uses `Documents/OmniOffice AI` and falls back
  to the legacy `Documents/PDF Swiss Army Knife AI` folder while the new one
  does not exist; Android results go to `Downloads/OmniOffice`; new cloud-sync
  setups default to the "OmniOffice" remote folder (saved configurations keep
  their folder).

## [3.6.1] - Writer wrapping and office input for the AI assistant

### Added

- **Office text extraction in the AI assistant.** Summaries, translation,
  document Q&A, text cleanup and metadata suggestions now accept the formats
  the suite already opens: DOCX/DOCM/DOTX, ODT, RTF, legacy DOC/DOT,
  TXT/MD/Markdown/HTML, XLSX/XLSM/XLS/ODS, CSV/TSV, PPTX/PPTM, ODP, legacy PPT
  and the native `.oswk` unit. Writer files become ~4000-character chunks,
  spreadsheets one unit per sheet (cell addresses and formulas included) and
  presentations one unit per slide (notes included), so the page selector
  still works; unsupported extensions fail with a clear message. The format
  picker, drag-and-drop and Android SAF import all accept documents now, and
  the metadata "Apply" action - which writes PDF files - is hidden for office
  inputs while the suggestion stays visible.

### Fixed

- **Writer paragraphs wrap at the page edge again.** A flexbox `min-width:
  auto` let a paragraph (or an unbroken word/URL) grow past the A4 text
  column, so typing ran off the right side of the page instead of wrapping.
  Paragraphs now shrink within the column and long words break when needed.

## [3.6.0] - PDF depth: text-run editing, bookmarks and RFC 3161 timestamps

### Added

- **PDF Studio "Text" tab (content-stream text editing).** The new
  `pdfcore::content` module walks a page's decoded content stream with
  graphics/text state (q/Q, Tf, Tm, Td/TD, T*, TL, Tc/Tw/Tz/Ts/Tr) and lists
  every text-showing run with its text, font, size and approximate position.
  A run's text can be replaced in place: only the string operand changes, the
  font and every other byte stay untouched, and the result is written as an
  **incremental revision** - the original bytes (and any signature over them)
  remain valid. The replacement is encoded with the run's font and verified by
  a round trip; text the font cannot represent is refused with a clear message
  instead of being garbled. Multi-string `TJ` runs and composite-font runs
  whose encoding cannot round-trip are reported read-only with the reason.
- **Reader bookmarks.** `pdf_info` now carries the document outline (bounded
  walk of `/Outlines`, `/Dest` destinations resolved to page numbers), and the
  Reader's side panel lists it with indentation and click-to-page navigation.
- **RFC 3161 timestamp signing.** The signing dialog accepts an optional TSA
  URL: the signature value is timestamped over HTTPS (plain HTTP only for a
  loopback TSA) and the token is embedded as the `id-aa-timeStampToken`
  unsigned attribute; verification reads it back and reports the token's
  `genTime`. A requested timestamp is mandatory - a TSA failure fails the
  signing instead of silently producing an untimestamped file. `pdfcore`
  builds and parses the `TimeStampReq`/`TimeStampResp` itself and never
  performs network requests.

## [3.5.7] - Legacy .doc/.ppt import and OAuth cloud sync

### Added

- **Word 97-2003 (.doc) and PowerPoint 97-2003 (.ppt) import.** Both are OLE2
  Compound File Binary containers; the new `officecore::legacy` module reads
  them without a native Office dependency: the Word FIB piece table (compressed
  and UTF-16 pieces, paragraph/cell marks) and the PowerPoint record tree's
  text atoms per `Slide` container. Documents open in Writer/Impress; a legacy
  tab has no save path, so the first Ctrl+S asks for `.docx`/`.pptx`/`.oswk`
  and the original binary file is never edited in place. When a local
  LibreOffice is installed (`PDFSAK_SOFFICE` or `soffice` on PATH) it is used
  first for a full-fidelity conversion into a private temporary directory
  (fixed arguments, no shell, 120 s timeout, cleaned up afterwards). The
  capability matrix and the README table now report the honest flags
  (open ✓ text import, edit ✓ in memory, save –, PDF export ✓).
- **OAuth 2.0 PKCE cloud sync (Google Drive + OneDrive).** `synccore::oauth`
  implements the Authorization Code flow with S256 (RFC 7636 test vector), a
  loopback listener on `127.0.0.1:<random port>` with state verification,
  token exchange and refresh. `synccore::cloud` adds Google Drive v3 and
  Microsoft Graph providers for the full sync surface (listing, streaming
  upload/download, staged downloads, folder creation) under the existing
  conflict contract: Graph writes are conditional on `If-Match`; Drive
  compares the stored `sha256Checksum` before replacing content. The Sync
  screen's OneDrive/Google Drive options are enabled with an OAuth panel
  (client ID/secret/tenant, Connect/Disconnect, account and storage note), and
  tokens live in the OS credential vault (Windows Credential Manager, macOS
  Keychain, Secret Service) with the app's secret-store fallback reported in
  the UI.
- **The universal converter completes its format list.** `office_convert`
  handles PDF → JPG/PNG/TXT/DOCX and image → PDF directly, and the conversion
  targets list reflects it (a PDF input offers JPG, PNG, TXT and DOCX).

### Changed

- Android picks `.doc`/`.dot`/`.ppt` through the SAF office picker alongside
  the modern formats, and the legacy extensions route to the workspace like
  the rest of the office family.

## [3.5.6] - Home opens what you pick, Android converters actually work

### Fixed

- **Picking a file on Home left it unopened.** A single picked or dropped
  document now routes through the same logic as open-with: office documents
  open in the workspace, PDFs in the reader, images in image-to-PDF (unknown
  types go to the system viewer). Several files still stay on the board as a
  quick-action suggestion. The Home picker also offered only PDF and images;
  it now lists every format the suite opens.
- **PDF → images failed with "a file with this name already exists".** The
  multi-output overwrite state defaulted to `error` while the UI showed
  "create new", so any folder that already contained `page_001.jpg` (a second
  export, another PDF in the same folder) failed. Multi-output tools now
  default to unique names and write into a per-document folder
  (`<document>_images/`).
- **PDF merge (and every tool) failed on Android** because the app-private
  staging folder (`.../Outputs`) was never created and the native commands
  reject a missing output folder. The tool session creates it before the first
  run.
- **The universal converter and the cleaner were dead on Android**: both used
  the desktop dialog plugin, which cannot open the Android picker. They now
  pick through the SAF bridge, stage results in the app cache and publish
  finished files to the chosen folder or to Downloads. Conversion targets stay
  honest: a PDF input offers JPG/PNG; PDF → Word/Excel layout reconstruction
  is not implemented.

### Changed

- **Home reads as an office suite.** The tagline, group order (Office first)
  and card descriptions now lead with Writer, Calc and Impress instead of
  presenting the app as PDF-only.

## [3.5.5] - Data-integrity fixes, PDF repair and hard quality gates

### Fixed

- **XLSX cross-sheet comments no longer contaminate each other (audit C11).**
  The exporter wrote one `comments1.xml` part for the whole workbook, so every
  sheet resolved the same part on import and read every other sheet's notes:
  two sheets with a comment on the same address shared one comment after a
  save. Comments and their VML shape sets are now one part per sheet, wired
  through that sheet's own relationships and content-type overrides. The
  regression test writes two sheets with a comment on A1 and re-reads the
  produced file.
- **Writer tracked changes keep the runs they are not editing (audit M12).**
  Suggest-mode synchronisation flattened the paragraph to text and rebuilt it
  from the first run's format, so every keystroke restyled the surrounding
  text, dropped footnote/field anchors and re-issued revision ids. The diff now
  works on runs: unchanged runs keep their formatting, links and anchors, an
  existing insertion keeps its revision id, deleting text the same suggestion
  inserted cancels the insertion, and a pending deletion survives a sync even
  while revisions are hidden (it used to be silently accepted).

### Added

- **PDF Studio repair and Fast Web View.** The bundled qpdf engine is wired
  into the app: "Repair" rewrites a damaged PDF (broken xref or trailer,
  dangling objects) into a working file next to the original, and "Fast Web
  View" writes the linearized layout. Both re-open the produced file, report
  its real page count and surface qpdf's diagnostics, and both are retryable
  from the Jobs screen like every other long-running tool.
- **Benchmark regression gate.** `scripts/check-bench-regression.mjs` compares
  the criterion means against the previous master run (cached baseline) and
  fails the bench job on a >15 % mean regression (warning at 7.5 %); the
  baseline advances only when the check passed.
- **Deeper desktop E2E.** `npm run e2e:flows` drives real flows through
  tauri-driver: Writer type → Ctrl+S with the DOCX on disk proved changed, two
  PDFs merged through the Merge screen's auto-run, and the Studio sanitizer
  run through its real button. CI runs them engine-free on Linux next to the
  smoke test, and a failure saves a screenshot.

### Changed

- **The formatting backlog is gone.** Prettier and rustfmt ran over the whole
  repository in one no-behaviour commit and the CI gate now checks every
  changed file plus the full tree, instead of only the files a change added.
- The entry bundle is ~92 KB gzip instead of ~155 KB: `manualChunks` uses the
  function form so the JSX runtime and the React scheduler land in the
  React vendor chunk instead of the entry.
- Rust crypto dependencies deduplicated: `cbc` 0.2.1 and `des` 0.9 replace the
  0.1/0.8 pair (the same versions lopdf already uses) and the unused `aes`
  dev-dependency is removed.
- Deferred dependency majors are documented in `.github/dependabot.yml` with
  the concrete blocker for each: eslint 10 (eslint-plugin-jsx-a11y peer
  range), TypeScript 7 (typescript-eslint supports `<6.1.0`), vitest/vite
  majors (the coverage instrumentation changes the measured percentages
  completely, so the enforcement floors need a deliberate re-baseline; the
  current vitest 3.2.7 also keeps the npm audit high gate green), der 0.8
  (cms/x509-cert pin 0.7) and rand 0.10 (rsa 0.9 uses rand_core 0.6).

## [3.5.4] - Quality infrastructure: E2E, fuzzing, benchmark trends

### Added

- **Desktop E2E** (`e2e/smoke.mjs`): tauri-driver + WebDriver against the real
  binary. The script waits for the home tool grid, navigates to Settings
  (sidebar or drawer), optionally opens a PDF in the Reader via the dev launch
  context, and saves screenshots. CI runs it on every pull request (Xvfb +
  WebKitWebDriver, engine-free) and uploads `e2e-screenshots`.
- **Fuzzing** (`fuzz/`): three libFuzzer targets - the hardened ZIP reader,
  the depth-limited XML parser, and the DOCX/XLSX/PPTX/ODT/RTF readers with a
  container-magic dispatcher - plus a seeded corpus and a nightly CI job that
  runs each target for 30-60 s on master.
- **Benchmark trends**: criterion benchmarks for DOCX/XLSX/PPTX import and the
  lossless PDF compression path, run by a master CI job that uploads the
  criterion report as an artifact.

### Changed

- `cargo deny check` now covers licenses (permissive allow-list), bans
  (duplicate majors warned, wildcards denied, internal path deps allowed) and
  sources, not just advisories; `src-tauri` is marked `publish = false` and
  the retired glib advisory exception is removed.
- The fuzz crate is excluded from the main workspace so `cargo test
  --workspace` stays nightly-free.

## [3.5.3] - Cleanup: retryable jobs, vault clear, writer fields, TOCTOU

### Added

- **Retryable background jobs** (`src/lib/job-retries.ts`): every long
  operation persists a stable kind plus the exact invoke arguments, and one
  handler per kind re-invokes the same command after a restart. Credential
  fields are blanked before the payload is written to `jobs.json`. The Jobs
  screen only offers Retry for kinds with a handler and explains the rest.
- **Vault Clear** in the Document Vault settings, with a confirmation dialog;
  on Android the dialog can also delete the app-private imported copies
  (`vault_clear` gained an optional `delete_imports` flag).
- Writer tests for ordered-list numbering and live field values; screen tests
  for Organize, Annotate, Batch, Page Tools and Plugins; `job-retries` tests
  for tracking, sanitizing and re-running.

### Fixed

- **Jobs "Retry" was unreachable** (audit L6): no screen registered a handler
  and most producers never persisted a payload, so every press ended in an
  "unavailable" toast.
- **Writer ordered lists always showed "1"** and PAGE/NUMPAGES/DATE/TIME/
  TITLE/AUTHOR fields kept their insertion-time cache (audit M16). Numbering
  and field values are now computed at render time.
- **`resolve_output_path` had a TOCTOU window** for `UniqueName` (audit M7):
  the candidate is now reserved atomically with `create_new`, so concurrent
  runs cannot pick the same name and overwrite one result.

### Changed

- Coverage floors raised from 58/68/46/58 to 62/69/46/62 (measured 64.5 %
  statements); frontend suite 673 tests, Rust suite 533 tests.
- Dependabot updates merged: base64 0.23.1, the Rust/frontend/actions groups
  and the Chrome extension TypeScript bump.

## [3.5.2] - Pinch-to-zoom follows the fingers

### Changed

- **Reader pinch is live.** Page width (and therefore the visible zoom) only
  changed after the 180 ms debounce finished, so a pinch gave no feedback
  while the fingers were moving. Two-finger gestures now scale the page
  container with a CSS transform that follows the fingers 1:1, including a
  two-finger pan; the transform keeps the pinch midpoint anchored and is
  committed to the real layout on release, where the sharp bitmap is
  requested. A pure two-finger pan (no scale change) is folded into the
  scroll offset instead of a zoom commit.

### Added

- Reader test proving the CSS transform tracks the gesture (`scale(2)` and the
  50 px pan for a 100→200 px pinch) and that release commits the 200 % zoom.

## [3.5.1] - Sharp, light reader zoom

### Fixed

- **Blurry zoom (Android).** Page previews were rendered at a fixed 96 dpi
  and could only be downscaled, so a requested raster width was never
  produced; the webview upscaled the small bitmap and text blurred as soon as
  the reader was zoomed. `page_preview` now renders at a high dpi and lets
  `max_width` cap the result, and the web layer requests CSS width × device
  pixel ratio (capped at 3000 px; the backend accepts up to 4000).
- **Heavy zoom.** Every settled zoom step re-requested every nearby page and
  remounted the page components, flashing a spinner and re-rendering pages
  that had just been rasterized. Previews are now cached per page (one entry
  per page, LRU 24), reused whenever the cached bitmap is at least as sharp,
  and the previous bitmap stays on screen while a sharper one loads. The
  prefetch margin shrinks for high-resolution renders so Android does not
  rasterize pages it may never show.
- **Pinch jank.** Pinch updates are coalesced to one React update per frame
  and the page component is memoized, so a gesture no longer re-renders every
  page on every pointer event.
- `PageCanvas` (Info, Watermark, PDF Studio) previews use the same
  physical-pixel rendering.

### Added

- Rust regression `high_dpi_render_honours_the_requested_raster_width`
  (pdfcore) proving the high-dpi + `max_width` pipeline reaches the requested
  raster width, plus reader helper tests for `previewRasterWidth` and the
  preview cache (`rememberPreview` / `reusablePreview`).

## [3.5.0] - Test coverage gate and screen coverage

### Added

- **Coverage floors in CI.** `npm run test:coverage` now enforces statement,
  branch, function and line floors (58/68/46/58) measured from this suite, and
  the frontend CI job runs it. The comment in `vite.config.ts` states the
  ratchet rule: raise the floors as coverage grows, never lower them.
- **Screen tests for the previously untested paths**: Merge, Split, Compress,
  Security protect, Watermark, Metadata, Info, History, Home, Settings
  (`src/screens/tools-screens.test.tsx`); AI Library, Jobs, Compatibility,
  OCR and PDF→images (`src/screens/more-screens.test.tsx`); Notes, Planner,
  Data, Draw, Templates, Converter, Cleaner and PDF forms
  (`src/office/tools-screens.test.tsx`). The tests render the real screens and
  assert the `request` envelopes the Rust commands deserialize.
- **Templates contract test** (`src/office/templates.test.ts`): unique ids,
  complete metadata and the model kind each template promises.

### Changed

- Frontend coverage moved from 49.2 % to 60.7 % statements (branches 71.2 %,
  functions 48.5 %) with 24 new tests and no production code changes.

## [3.4.0] - Quick wins: commands, settings, Home and accessibility

### Added

- **Command platform.** Every screen is a palette command and every registered
  keybinding now executes through `matchKeybinding`; the palette grew from 7
  view commands to all 42 screens plus `file.open`, `app.commandPalette` and
  `app.globalSearch`. `Ctrl+O` opens office formats into the workspace and
  routes other documents through the active screen's drop handler.
- **Settings.** `midnight`/`paper` themes, office default formats
  (`defaultWriterFormat`, `defaultCalcFormat`, `defaultImpressFormat`),
  version-history and import-warning toggles. The format defaults now drive
  the save dialog, version history gates `historyPush`, and import warnings
  surface once per open. The shortcut list reflects real bindings.
- **Home tool directory.** Searchable, grouped grid covering every screen;
  recent PDFs open in the Reader; the reveal action is a labeled `IconButton`.
- **Calc.** `CUMIPMT` and `CUMPRINC` implemented with Excel reference values,
  range/type validation and regression tests; the unused
  `UNSUPPORTED_FINANCIAL` export is gone.
- **Accessibility.** Modal focus trap and focus restore, `aria-current` on the
  sidebar, roving tabindex + arrow-key navigation in `Segmented`,
  `aria-live` job announcements, `documentElement.lang` follows the language,
  translated labels for previously hard-coded controls, and a
  `prefers-reduced-motion` stylesheet rule.
- **Quality.** `npm run test:coverage` (Vitest v8 provider) runs in the
  frontend CI job; `src/components/ui.test.tsx` covers the modal focus
  behavior; `docs/roadmap.md` tracks the phases after 3.4.0.

### Fixed

- **Multi-file drop suggestion** on Home kept only the first file, so running
  Merge from the suggestion silently dropped the rest of the selection.
- **`officecore::compat`** still claimed long-term validation data
  (`DSS`) was not written; the note now matches `pdfcore::ltv` (DSS is written
  for offline PAdES B-LT, RFC 3161 timestamps are not requested yet) and the
  README contract test enforces the wording.
- **Docs drift**: `docs/android.md` now matches the README (JDK 21, Rust
  1.89+), and the README test counts are refreshed (528 Rust, 626 frontend).
- **Slide move buttons** and the color picker use localized accessible names.

## [3.3.1] - Security hardening and reliability patch

### Security

- **AES-256 key generation**: `random_key()` now fills the key from the OS
  entropy source (`getrandom`) instead of concatenating two UUIDv4 values
  (122 random bits each and not intended as key material). `uuid` is no longer
  a pdfcore dependency.
- **TOCTOU mitigation**: `input_file()` validates an **open handle** and, on
  Unix, compares the handle's device/inode with a fresh path lookup so a
  symlink swapped in between open and check is rejected. `NotFound` and the
  "not a file" contract are preserved.
- **Credential redaction**: the AI request structs (`AiSummarizeRequest`,
  `AiTranslateRequest`, `AiAskRequest`, `AiCleanupRequest`,
  `AiMetadataRequest`) no longer derive `Debug`; their manual implementations
  print `**REDACTED**` for the document password, with a regression test.
- **WebDAV plain-http restriction verified**: `allow_insecure_http` was already
  limited to loopback hosts by `normalize_base_url_with_options` (a public
  `http://` endpoint is refused even when the flag is on, covered by
  `crates/synccore` tests). No code change was needed; a naive
  `starts_with("http://127.0.0.1")` check would have accepted
  `http://127.0.0.1.evil.com` and was deliberately not added.

### Bug Fixes

- **PDF page duplication**: cloning a page now **copies** each annotation
  dictionary and points its `/P` at the duplicate. Previously the duplicate
  shared the annotation objects with the original page, so the `/P` back
  references were wrong; re-pointing the shared objects would have corrupted
  the original. Regression test covers both pages.
- **Date conversion**: the `civil_from_days` helpers (office, vault, library,
  sync metadata, layout, zip) use `saturating_sub` so the floor-division
  branch cannot overflow on extreme negative input.
- **Calc `LET`**: added regression tests proving a self-referential binding
  terminates as `#NAME?` (Excel semantics) instead of looping, and that a
  self-referential **defined name** reports a circular reference. No infinite
  loop existed; the guard proposed for `evaluateLet` would have changed the
  correct `#NAME?` result and was not applied.
- **Calc array literals**: separator-only literals (`{,}`, `{;}`, `{,;}`)
  are pinned to `#VALUE!` by tests (the parser already reported it).
- **PDF merge**: the object remap preserves generation numbers and offsets
  object numbers; a new regression test merges a document containing the same
  object number with two generations and proves no object is lost. Resetting
  every generation to 0 (a suggested "fix") would have collapsed those two
  objects onto one key, so it was not applied.
- **`.oswk` migration checksums**: verified that `schema::migrate_unit`
  already recomputes the checksum when it adds fields (implemented in 3.3.0);
  no duplicate logic was added to `office.rs`.

### Code Quality

- UTF-16 round-trip tests for PDF text objects cover Turkish, Japanese and
  astral-plane (emoji) text.

### Notes

- A 500 ms sleep on `CloseRequested` was proposed as a "graceful shutdown"
  wait. It was **not** applied: blocking the window event handler does not wait
  for worker threads and delays every close, while the atomic write path
  (temp + fsync + atomic rename) already guarantees that an interrupted save
  cannot leave a partial document behind. Cancellation stays cooperative.

## [3.3.0]

Production-readiness: the security, data-loss and correctness hardening from
the post-3.2.1 audit plus a canonical `.oswk` format, a transactional Writer
history, Excel-compatibility fixes in Calc, an external file-conflict guard and
release metadata. See `AUDIT_REPORT.md` and `RELEASE_READINESS.md` for detail.

### Security

- WebDAV conditional uploads now quote the `If-Match` entity-tag as RFC 7232
  requires. Strict servers (Sabre/Nextcloud) were answering 412 to every
  ordinary re-upload, and lax servers silently ignored the precondition.
- Plugin HTTP requests refuse literal private/loopback IP destinations over
  `https` (previously only the metadata address was blocked in that branch).
- Sync commands (`sync_status`, `sync_upload`, `sync_download`,
  `sync_resolve`, `sync_forget`) now validate local paths with the centralized
  `paths::*` wrappers instead of accepting raw webview strings.
- AI responses and streams are capped at 16 MB and provider error text is
  bounded before it reaches the UI log.
- Failed background jobs are persisted as `failed`, not `done`; a corrupt
  `jobs.json` is quarantined instead of overwritten; reusing a job id cancels
  the previous worker; the job store uses a unique temp file with fsync.
- All "atomic" writers (`officecore`, `pdfcore`, `synccore`, signing, jobs)
  no longer delete the target before renaming and now fsync before the rename;
  settings, recents, AI settings, library index, operation log and stored
  secrets use the same atomic path. PDF→images, OCR text/PDF and visual-diff
  outputs are atomic too.
- Writer: footnote/endnote/field anchors survive any keystroke; Shift+Enter
  with a selection removes it; Delete no longer removes a following object;
  multi-paragraph table cells keep their other paragraphs; caret offsets skip
  note/field glyphs and hard breaks; `Show revisions` off hides deletions;
  pagination numbers pages monotonically; hard line breaks render as `<br>`.
- Calc: `range <op> scalar` broadcasts the scalar to every cell; error
  literals (`#N/A`, `#DIV/0!`, …) parse and work with `IFERROR`/`ISERROR`;
  `TEXT` time formats show minutes (not the month); `TIME(h,m,s)` added.
- XLSX: a hyperlink on a blank cell round-trips instead of being dropped.
- IPC: `ai_translate` sends `targetLanguage` (the feature previously failed
  deserialization); `ai_example_prompts.translateTargets` is read;
  `SanitizeReport.metadataRemoved` is typed as a count.
- Vault: a stale `scanning: true` left by a crash is repaired at startup.
- Jobs: live jobs appear in the job center without an app restart.
- Process-wide heavy-work semaphore (`src-tauri/src/concurrency.rs`) bounding
  concurrent PDF render/OCR/compression, vault scans, office import/export and
  signing to roughly the CPU count instead of Tokio's 512-thread default.

### Added

- **Canonical `.oswk` envelope** (`crates/officecore/src/unit.rs`):
  `documentType`, `applicationVersion`, a SHA-256 `checksum` of a canonical
  (key-sorted) model serialization, a `featureManifest`, and a free-form
  `extensions` bag. Unknown envelope fields survive a round trip; a checksum
  mismatch is reported as `corrupt_document`; migration recomputes the digest
  when it adds fields, and a newer schema is refused, not rewritten. Golden
  fixture tests in `crates/officecore/tests/unit_envelope_test.rs`.
- **Writer transaction history** (`src/office/writer/history.ts`): reversible
  `EditOperation`s and a bounded checkpoint + delta undo/redo stack wired to
  Ctrl+Z / Ctrl+Y / Shift+Ctrl+Z and the toolbar. The browser's native
  `execCommand("undo")` could not see structural or formatting edits.
- **External file-conflict guard**: the office session fingerprints a file at
  open/save (`file_fingerprint`) and, before writing, offers Reload external
  changes / Save as new file / Cancel when the file changed on disk.
- **Release metadata**: `scripts/build-info.mjs` writes `build-info.json`
  (version, git SHA, Node/Rust/Java/toolchain versions) into
  `release-artifacts/`, attached by the release workflow.

### Fixed

- **Calc Excel compatibility**: `MATCH`/`HLOOKUP` approximate modes return the
  position in the original range (they used to sort a copy); `XLOOKUP` modes
  ±2 work and follow the original order; `COUNTIF`/`SUMIF` support `*`/`?`
  wildcards and `~` escapes; `NUMBERVALUE` no longer strips the decimal
  separator; `FILTER` treats a blank mask cell as FALSE; the unary operator
  maps over arrays so `SUMPRODUCT(--(range>1))` works; a stray NUL byte in
  `arrays.ts` was removed.
- `.oswk` open verifies the content checksum before migration and reports
  `corrupt_document` instead of silently reinterpreting a damaged file.

### Changed

- Regression tests: job failure/quarantine/id-reuse, vault status repair,
  WebDAV ETag quoting (strict in-process server), plugin private-IPv4/host
  literals, Writer structural runs/caret/hard breaks/revision hiding,
  pagination numbering, Calc broadcasting and error literals, `TEXT`/`TIME`,
  XLSX link-only cells.

### Changed

- Version bumped to 3.3.0 across `package.json`, `Cargo.toml`,
  `tauri.conf.json` and the generated Android `tauri.properties`
  (`versionCode 3003000`).

## [3.2.1]

Comprehensive security hardening, centralized path validation, and job reliability.

### Security

- Native Folder Dialog for Plugins: Replaced webview-provided path installation (`plugin_install_from_path`) with Rust-side native folder selection (`plugin_install_from_dialog`), preventing compromised or malicious webviews from installing arbitrary filesystem directories.
- Plugin SSRF & Cloud Metadata Protection: Implemented DNS resolution and strict IP filtering in `plugin_http_request` blocking cloud metadata endpoints (`169.254.169.254`, IPv4-mapped IPv6), CGNAT, multicast, and private networks.
- Centralized Filesystem Validation: Enforced `ValidatedInputFile`, `ValidatedOutputFile`, and `ValidatedDirectory` across all 78 Tauri filesystem commands in `commands.rs`, `pdf_v3.rs`, `office.rs`, `office_tools.rs`, `sign.rs`, and `ai.rs`. Rejects traversal (`..`), NUL, control characters, Windows reserved device names, UNC (`\\server\share`), and extended paths (`\\?\`).
- Trinary PDF Redaction Verification: Upgraded redaction verification to a trinary state (`Removed`, `PossiblyPresent`, `NotVerified`), preventing false successes and surfacing potential remaining content warnings to the user.
- Digital Signature Post-Sign Verification: Gated `pdf_sign` on immediate cryptographic and ByteRange verification prior to returning success.

### Fixed

- Job Lifecycle Reliability: Implemented RAII `JobFinishGuard` to ensure async commands transition deterministically to terminal states (`success`, `failure`, `cancelled`, `interrupted`) even on early errors or cancellations.
- Dynamic AI Model Discovery: Added `discover_models` for OpenAI-compatible and Ollama endpoints with credential-safe error messages.

### Added

- CI Quality Scripts: Added `check:fast` and `check:all` in `package.json` for rapid and exhaustive local and CI validation.

## [3.2.0]

Quality gates and test confidence.

### Changed

- ESLint now fails on every warning: the React Compiler diagnostics
  (`set-state-in-effect`, `refs`, `immutability`, `purity`, `use-memo`,
  `preserve-manual-memoization`, `globals`, `exhaustive-deps`) and the
  jsx-a11y interaction rules were burned down from 143 warnings to zero and
  promoted to errors. The per-rule warning budget
  (`scripts/lint-baseline.*`) is removed; `npm run lint` is a plain
  `eslint .`. Several real fixes fell out of this: state that must reflect the
  active document is derived during render instead of cleared in an effect,
  drag-highlight state replaces ref reads during render, and `Date.now()`
  call sites moved out of render.
- Accessibility: keyboard handlers/roles/`aria-label`s were added to the
  editors and screens (spreadsheet grid and tabs, slide thumbnails, writer
  paragraphs and image captions, modal backdrops), and a `.sr-only` helper
  labels icon-only controls.

### Added

- WebDAV end-to-end tests (`crates/synccore/tests/webdav_e2e.rs`): an
  in-process DAV server exercises `list`/`put`/`get`/`delete`, conditional
  uploads (HTTP 412 → `Conflict`), streaming hashing and the Depth header.
- Android on-device instrumentation tests for the open-with / share pipeline
  (`app/src/androidTest`): real Android I/O through
  `ContentResolver` → `IncomingFiles.copyToCache` verifies display-name
  sanitization, the extension whitelist, the size cap and the cache hand-off
  on an emulator in CI (a `file://` fixture feeds the same `openInputStream`
  path the SAF result uses).
- `deny.toml`: the RustSec advisory exceptions now carry an owner and a review
  date, and CI enforces them with `cargo deny check advisories` (in addition
  to `cargo audit`).

## [3.1.1]

Security and supply-chain patch on top of 3.1.0.

### Security

- AI provider and WebDAV base URLs are validated on save and at client
  construction: `https://` is mandatory for every non-loopback host, plain
  `http://` is limited to `localhost` / loopback IPs (WebDAV additionally
  requires an explicit opt-in in the sync settings) and a redirect that
  downgrades to a public `http://` endpoint is refused before Basic Auth
  credentials, Bearer tokens or document bytes can leave the machine.
- Native engine downloads are fully pinned: the two previously missing
  SHA-256 entries (Tesseract 4.1.0 source archive, Liberation fonts archive)
  are in `engines.lock.json`, and a download without a pin now fails the
  build instead of warning. CI verifies the lock covers the complete download
  surface of both fetch scripts (`-VerifyLock`).
- The webview no longer holds a standing Android grant to `$DOCUMENT/**` or
  `$DOWNLOAD/**`: intermediates live in app-private data/cache and
  user-visible files go through SAF. AI library files on Android also moved
  to app-private storage.
- OS open/reveal no longer uses the opener plugin permission from the
  webview; two validated Rust commands accept only existing documents with
  extensions the app produces, so a compromised renderer cannot launch
  executables through the shell.
- Every GitHub Action is pinned to a commit SHA (Dependabot keeps the pins
  current).

### Fixed

- Chrome extension: "Open with PDF Swiss Army Knife" now passes the selected
  PDF to the app, which shows a consent prompt, requests the optional host
  permission for that origin and opens the downloaded file. Hostile
  `javascript:` / `data:` / `file:` links are ignored, and a changed hash
  replaces or closes a stale prompt.
- AI settings now surface save errors instead of failing silently when a
  provider URL is rejected.

### Changed

- WebDAV uploads and downloads stream with hashing instead of buffering up to
  512 MB per transfer; conflict checks happen against a staged temporary file
  before anything is promoted into place.
- Screens load lazily with a CI bundle budget (entry chunk 315 KB → 155 KB
  gzip); the ESLint gate is a per-rule baseline with
  `jsx-a11y/no-autofocus`, `no-noninteractive-element-interactions`,
  `no-noninteractive-tabindex` and `interactive-supports-focus` promoted to
  errors.
- The Android intent pipeline (name sanitization, extension whitelist, 256 MB
  copy cap) is covered by JVM unit tests run by the Android release workflow.
- The Chrome extension job runs the headless-Chrome self test (pdf.js worker,
  canvas rendering, deep-link consent) in CI.

## [3.1.0]

Cross-platform completion, Office/PDF fidelity, secure signing and production
release. The paginated Writer is now a real editing canvas, PDFs can be signed
with genuine CMS/PKCS#7 signatures and validated, PDF forms can be filled and
flattened, XLSX import keeps charts, pictures, print settings, protection and
pivot parts, PPTX charts ship cached values and an embedded workbook, ODT/RTF
keep notes and tracked changes, the Android app becomes a first-class platform
(intents, SAF, touch editing, vault import, persistent jobs), and a sandboxed
plugin runtime and a local-first sync foundation land with data-loss
protection in front of every lossy save.

### Writer

- **Direct WYSIWYG editing in the paginated view.** Typing now happens on the
  page itself: clicking anywhere on a page places a real caret at the clicked
  character, typing/paste/delete go through the document model, Enter splits,
  Backspace merges, Shift+Enter inserts a line break, and the caret moves
  **across page boundaries** with the arrow keys. Caret persistence across
  reflow is preserved; the continuous surface remains available.
- Cross-page/fragment selection works through the native selection when the
  drag starts in the active editable; all editors accept pointer events so the
  same behavior works with touch on Android.

### Calc

- **XLSX import fidelity**: charts (column/bar/line/pie/area, titles, series,
  colours, caches, drawing anchors), pictures (media, anchor, size, rotation),
  print settings (page setup, margins, options, header/footer, row/column
  breaks, Print_Area and Print_Titles), and sheet protection now round-trip.
- Pivot tables are preserved losslessly (definition, records, table parts are
  re-exported); the grid is not recomputed from the cache.
- Formula-autocomplete and auditing overlays now apply on touch (pointerdown),
  with a bottom-docked formula bar on Android.

### Impress

- **PPTX chart data path completed**: the chart dialog edits real
  categories/values (grid entry, paste, add/remove rows and series), the model
  carries `categoriesCache`/`seriesValuesCache`, and export writes
  `c:strCache`/`c:numCache` plus an **embedded Excel workbook**, so charts open
  with data in other office suites.
- Touch object editing (move/resize/rotate), marquee selection and
  double-tap-to-edit; larger handles on coarse pointers.

### PDF

- **Real digital signatures** (`pdfcore::sign`): detached CMS/PKCS#7 with
  X.509, SHA-256, RSA and ECDSA P-256; AcroForm signature field + widget with
  a generated visible appearance; ByteRange covers the saved revision; the
  written file is re-verified. Validation reports digest match, whole-file
  coverage, modification detection and signature validity, and prints subject/
  issuer/serial/validity/algorithm/signing time. Certificates come from a
  PKCS#12 (.pfx) file on every platform, or from the **Windows certificate
  store** (listing + signing, with a clear error for non-exportable keys).
  Trust is reported as `unknown` offline; nothing is faked.
- **Forms & objects in PDF Studio**: field listing (text/checkbox/radio choice/
  list/dropdown with flags, options, required, tab order), filling with
  regenerated appearances, validation (required, max length, options, hinted
  formats), and annotation/widget/image object selection with move, resize,
  rotate and delete - mouse on Windows, touch handles on Android. Text and
  vector content-stream objects are documented as out of scope.
- **PDF/A font embedding** (`pdfcore::fontembed`): non-embedded simple fonts
  are substituted with bundled OFL fonts (Liberation Sans for the Helvetica/
  Arial family, metric-compatible; PT Sans fallback with a warning) and
  embedded as `/FontFile2` with real descriptor metrics; skipped fonts are
  reported honestly (CID/Type0, symbolic, custom encodings). Conversion also
  writes a real `/DestOutputProfile` sRGB ICC v4 profile.
- Reader pinch-zoom, drag pan and double-tap zoom for Android.

### Digital Signatures

- New `pdfcore::sign` module, Tauri commands (`pdf_sign`,
  `pdf_verify_signatures`, `pdf_list_signing_certificates`) and a Signatures
  tab in PDF Studio. PFX passwords/key material never touch disk or logs.

### Android

- **Open-with intents work**: MainActivity resolves content:// URIs (VIEW,
  SEND, SEND_MULTIPLE), copies them into app cache under size/extension
  guards, and the app opens them at startup or on new intent.
- **Office documents can be imported and exported**: SAF pickers for office
  MIME types, saving through SAF targets with publish-to-Downloads fallbacks.
- **Document Vault on Android**: import documents through SAF into app-private
  vault storage, index, search and preview locally; folder scanning remains a
  desktop capability and is labeled as such.
- **Background jobs persist**: job records survive process death, previously
  running jobs return as `interrupted`, and cancellation keeps working.
- Touch UX across Writer/Calc/Impress/Reader; Android back button follows an
  in-app history; secrets excluded from backups (`allowBackup=false`),
  network-security config pins cleartext to localhost/emulator hosts.

### Windows

- File associations now register unique ProgIds for all supported extensions
  (previously generic names collided and only `.oswk` registered), the
  installer still accepts "Office Swiss Army Knife_3.1.0_x64-setup.exe" as its
  asset name, and the portable ZIP ships the same binaries.

### Compatibility

- **Data Loss Protection**: before every non-`.oswk` save/export (including
  the Universal Converter) the compatibility report is computed and a modal
  shows Feature / Supported? / Imported? / Exported? / Transformed? / Lost?
  with Continue, Cancel and **Save as .oswk**. The check fails open so a
  broken report can never trap a save.
- Compatibility matrix updated for the new ODT/RTF/PPTX/XLSX capabilities.

### Security

- Android manifest hardening: `allowBackup=false`, data-extraction rules,
  narrowed FileProvider paths, network security config, intent input
  validation (whitelist, size cap, sanitized names, no traversal).
- Plugin runtime runs untrusted plugin code in a Web Worker with no DOM and no
  IPC; capabilities (`read_document`, `modify_document`, `read_files`,
  `write_files`, `clipboard`, `network`) are host-enforced per manifest,
  plugin file IO is scoped to a per-plugin sandbox, and a plugin crash only
  kills its worker.
- Sync credentials reuse the existing secret store (DPAPI on Windows); cloud
  is off by default and nothing is ever overwritten silently.

### Performance

- Performance guards added: Writer pagination and 100-page PDF export, Calc
  20k/100k-cell recalculation, PDF merge/text extraction/render, plus
  `#[ignore]` heavy cases (500-page Writer PDF, 500k-cell chain, 500-page
  scanned render). Measured on the development machine with generous bounds.

### AI

- AI remains opt-in with visible provider and send scope on both platforms;
  the Android build documents its plaintext key fallback and the backup
  exclusion, and LAN/localhost cleartext access for local models.

### Cloud

- **Local-first sync foundation** (`crates/synccore` + Sync screen): provider
  abstraction with a real WebDAV implementation (PROPFIND/GET/conditional
  PUT/MKCOL/DELETE), per-file metadata with device id and revision, three-way
  conflict detection (local/cloud/base) and manual resolution - keep local,
  keep cloud or keep both. OneDrive/Google Drive are declared as requiring
  OAuth and are disabled with an explicit message. Off by default; no
  background polling.

### Release

- Version 3.1.0 everywhere (package.json, workspace Cargo.toml, Cargo.lock,
  tauri.conf.json, Android `versionName`/`versionCode`, UI constants).
- Windows: NSIS installer (`Office Swiss Army Knife_3.1.0_x64-setup.exe`),
  portable ZIP and SHA256 sums. Android: signed release APKs for arm64-v8a
  and armeabi-v7a plus AABs for both, with SHA256 sums.
- CI updated: Android workflow builds APKs **and** AABs, attaches all assets
  to the tag release (creating it if the desktop job has not yet), and both
  workflows validate the Android version metadata against the tag.

### Added

- Golden-file contract tests: Writer/Calc/Impress V3.1 fixtures commit the
  `.oswk` bytes and assert feature survival across `.oswk` and DOCX/XLSX/PPTX
  round trips; the same engine ships on Android, so the contract is testable
  in CI and manually on device.
- Sandboxed plugin runtime with manifest permissions, sample plugin, install/
  list/remove commands and a Plugins screen.
- `pdfcore::forms` (AcroForm list/fill/validate, page objects), `pdfcore::sign`,
  `pdfcore::fontembed`, `crates/synccore`, `vault_import_files`, persisted
  `JobStore`, Android intent handling.

### Changed

- Writer paginated fragments host the editing surface directly; the old
  "click opens the continuous surface" behavior is gone for paragraphs.
- `PrintSettings`, `Sheet`, `Workbook`, `ChartData` and `TextDocument` gained
  V3.1 fields, all serde-defaulted: V2/V3 `.oswk` documents open unchanged.
- `SheetProtection` is now a structured model instead of a boolean.

### Fixed

- XLSX: the exporter dropped footer text, hardcoded margins and page breaks,
  and ignored imported print settings; all of them round-trip now.
- PPTX: charts exported without caches rendered empty in other suites; they
  now carry values and an embedded workbook.
- RTF: footnote groups were previously parsed as ordinary text.
- ODT: note bodies were folded into the paragraph text on import.
- Android file intents were silently dropped; they now open the document.
- Windows file associations for office extensions never registered.

## [3.0.0]

The document-platform release. Writer gains sections, notes, tracked changes,
comments and fields; Calc gains structured tables, auditing and real XLSX
import fidelity; Impress gains masters, groups, charts, animations and a
presenter view; the PDF module gains a sanitizer, PDF/A validation, flattening,
redaction verification and working OCR preprocessing; and the app gains a
Document Vault, a command platform and an opt-in AI document assistant.

### Added

- **Writer sections.** `Block::SectionBreak` with per-section page setup,
  default/first/even headers and footers, and section start types (new page,
  continuous, odd, even). DOCX exports each section as a real `w:sectPr` with
  its own header/footer parts; the PDF export switches page geometry, headers
  and footers per section.
- **Footnotes and endnotes.** `Footnote` model with automatic numbering by
  reference order, `w:footnoteReference`/`w:footnote` parts in DOCX, a reserved
  note area at the bottom of the referencing page in the paginated view and in
  PDF export, and note text appended in the text exports.
- **Track changes.** Run-level insertions, deletions and formatting changes
  with author/timestamp, a suggest mode, a review pane with accept/reject,
  accept all/reject all and next/previous, show/hide, and DOCX
  `w:ins`/`w:del`/`w:rPrChange` round trips. Deleted text stays in the model
  until a decision is made, so accept/reject is lossless
  (`officecore::revisions` and `src/office/writer/revisions.ts`).
- **Comments with replies** and **bookmarks/cross-reference fields**
  (REF, PAGEREF, NOTEREF, DATE, TIME, TITLE, AUTHOR) written as real Word
  fields and resolved in the PDF export.
- **Calc structured tables** (`SpreadsheetTable`): header/totals rows, banded
  rows, calculated columns, filters and structured references
  (`=SUM(Sales[Amount])`, `Sales[@Amount]`, `[#All]`, `[#Headers]`,
  `[#Data]`, `[#Totals]`), evaluated by the formula engine and wired into the
  dependency graph; exported as real `xl/tables/tableN.xml` parts.
- **Calc formula autocomplete and auditing.** Suggestions for functions (with
  signatures), defined names, sheets, tables and columns; argument hints;
  trace precedents/dependents with coloured overlays; circular and invalid
  reference reporting.
- **XLSX import fidelity.** A custom OOXML pass reads styles (fonts, fills,
  borders, alignment), number formats, column widths, row heights, merges,
  freeze panes, data validation, conditional formatting, hyperlinks, comments,
  defined names and structured tables. Malformed parts degrade to
  values+formulas with a warning instead of failing the import.
- **Impress master slides and layouts** with placeholder inheritance, **real
  nested groups** with group transforms and Alt+click child selection, **PPTX
  chart import/export** (column/bar/line/pie/area), an **animation model**
  (entrance/emphasis/exit, triggers, duration, delay) that the slideshow
  executes and that round-trips as `<p:timing>`, and a **presenter view**.
- **PDF sanitizer** (`pdfcore::sanitize`): removes JavaScript, embedded files,
  launch/URI actions, unsafe annotations and metadata by walking every object,
  with a removal report.
- **PDF/A validation and conversion** (`pdfcore::pdfa`) for 1b/2b/3b: real
  structural checks (XMP identifier, output intent, embedded fonts, encryption,
  JavaScript, attachments, title, trailer ID). Conversion applies the fixes it
  can and re-validates the written file; it reports failure when fonts remain
  unembedded instead of claiming compliance.
- **Annotation/form flattening** (`pdfcore::flatten`): appearance streams are
  burned into the page content and the interactive objects removed.
- **Redaction verification**: the redacted output is re-opened and its text
  layer re-extracted; the report now carries `verified`, `remaining_matches`
  (masked) and an honest message when verification was skipped.
- **OCR preprocessing** now works: orientation detection runs on the rendered
  raster (previously it was handed the PDF path and silently did nothing),
  deskew/denoise/threshold/contrast are applied, and `OcrResult` reports what
  was applied.
- **PDF Studio screen** for sanitize, flatten and PDF/A validate/convert.
- **Document Vault**: opt-in indexing of user-picked folders for office
  documents and PDFs, incremental rescans, crash-resistant JSON index,
  full-text/phrase/fuzzy search with filters, snippets with match locations and
  a preview panel. Nothing is scanned unless the user adds the folder.
- **Command platform**: unified command registry, command palette
  (`Ctrl+Shift+P`), global search (`Ctrl+Shift+F`) over commands, recent files
  and the vault index, and a background job center with progress/cancel/retry.
- **Compatibility Center** and capability system: `officecore::compat` reports
  per-format support and a pre-save loss report; new Tauri commands
  `office_capabilities`, `office_model_capabilities`, `office_compatibility`.
- **`.oswk` schema versioning** (`officecore::schema`): every unit is stamped
  with `schemaVersion`, V2.x files are migrated in memory on open (with
  notes), future schemas are refused instead of misread, and corrupt models
  produce a clear error.
- **AI provider abstraction**: DeepSeek, OpenAI-compatible, Ollama (local,
  keyless), Gemini and custom providers with capability flags
  (`crates/aicore`), a document-chat prompt with `[page N]` citations,
  Writer/Calc action prompts, and a provider/capabilities UI with the send
  scope and a "what will be sent" activity line.
- **Desktop CI** (`.github/workflows/desktop.yml`): frontend tests/type
  check/build on Linux, Rust workspace tests on Windows/Linux/macOS, Chrome
  extension tests/build/verify, and a dependency audit job.
- **Release pipeline** (`.github/workflows/release.yml`): on `v*` tags, a
  Windows Tauri build, installer/portable packaging, SHA256 checksums, artifact
  upload and release attachment, plus a version-consistency job.
- 76 new Rust tests and 76 new frontend tests.

### Changed

- The `.oswk` `version` field is now the schema version (3) instead of a
  mis-parsed semver (it used to always write 2).
- `NativeUnit` carries `schemaVersion`; opening an old unit reports the
  migration in the document warnings.
- The Writer paginated view maps a click to a character offset (caret lands
  where you clicked) before opening the editing surface.
- XLSX export skips the sheet-level autofilter when a structured table owns
  the range, so the package no longer contains duplicate filters.

### Fixed

- OCR `auto_rotate` was a no-op (Tesseract OSD received a PDF path); it now
  detects orientation on the rendered page and applies the rotation.
- `xlsx_roundtrip_loss_is_limited_to_presentation_metadata` (which asserted
  layout was dropped) is replaced by a test that asserts the new fidelity.
- The DOCX reader no longer drops `w:ins` content or ignores `w:del`,
  `footnotes.xml`, `comments.xml`, `sectPr` (beyond the last one) or non
  page-number fields.

### Compatibility

- `.oswk` files written by 2.2.0/2.5.0 open unchanged and are migrated in
  memory; saving rewrites them at schema 3.
- DOCX packages gain footnotes/endnotes/comments parts and per-section
  header/footer parts; ODT/RTF/text exports keep their previous behaviour and
  the Compatibility Center lists exactly what they drop.
- XLSX import now returns much more of the original structure; documents that
  previously round-tripped as values+formulas still import the same values and
  formulas.

### Known limitations

- Writer typing still happens on the continuous surface; the paginated view is
  caret-accurate but not a WYSIWYG typing canvas.
- Track changes covers run-level insertions/deletions/formatting; paragraph
  moves and structural edits are not recorded as revisions.
- ODT/RTF do not write note objects yet; PDF/A conversion does not embed
  missing fonts; digital signatures, the plugin runtime and cloud sync are not
  implemented in 3.0.0 and are declared as such in the README.

### Tests

- Rust: 322 tests (was 246), including the schema migration suite, Writer V3
  round trips, PPTX V3 suite, XLSX import fidelity, sanitizer/PDF-A/flatten
  and redaction verification, and vault indexing.
- Frontend: 451 tests (was 375), including structured references, auditing,
  sections/notes pagination, tracked changes and the new screens.

## [2.5.0]

A feature release on top of 2.2.0's stabilisation: Calc gets a dependency-driven
engine, dynamic arrays and pivot tables; Writer gets a measured pagination
engine with real page containers, a table of contents and a navigation pane.
Detailed notes: [docs/release-notes-v2.5.0.md](docs/release-notes-v2.5.0.md).

### Added

- **Calc dependency graph and incremental recalculation.** An edit recalculates
  the edited cell plus the formulas that (transitively) read it; unrelated
  formulas keep their cached values. Volatile formulas (`NOW`, `RAND`, ...) and
  ranges too large to expand into edges are treated as global dependents.
  `lastComputeStats(workbook)` reports whether the last pass was full or
  incremental and how many formulas it evaluated.
- **Dynamic arrays with spill semantics.** A formula that returns a matrix
  writes its extra cells into the grid (`=SEQUENCE(5)` spills down), blocked
  spill ranges report `#SPILL!`, other formulas can read spilled cells, and a
  resized array frees the cells it no longer owns.
- **Array broadcasting** for `+ - * / ^ & = <> < > <= >=` between matrices and
  scalars, which is what makes `=FILTER(A1:A9,A1:A9>5)` work - comparisons over
  a range used to collapse to their first cell.
- **Pivot tables (Calc).** Insert -> Pivot table summarises the used range with
  row/column/value fields and sum/count/average/min/max aggregation. The grid
  is computed live, rendered on the sheet with refresh and remove, and the
  definition is kept in `.oswk`. XLSX export materialises the computed values
  at the anchor and warns that the result is not a live Excel pivot; the Rust
  side runs the same aggregation (`officecore::pivot`).
- **Extended statistics**: `VAR.S`, `VAR.P`, `VARP`, `STDEV.S`, `STDEV.P`,
  `PERCENTILE`, `PERCENTILE.INC`, `QUARTILE`, `QUARTILE.INC`, `CORREL`,
  `COVARIANCE.P`, `COVARIANCE.S`, `COVAR`.
- **Writer pagination engine** (`src/office/writer/pagination.ts` and
  `measure.ts`): line boxes and table rows are measured in a hidden probe at
  the exact content width and split into page fragments with widow/orphan
  control, keep-with-next, keep-together, page-break-before and repeated table
  header rows on continuation pages.
- **Writer paginated view**: real page containers at the document's page size
  with per-page headers/footers and `{{page}}` / `{{pages}}` numbers; the page
  count in the status bar is the laid-out count, not a scroll-height estimate.
  Clicking a page opens the continuous editor on that block; View -> Paginated
  / Continuous switches modes.
- **Table of contents**: Insert TOC builds entries from Heading 1-6 paragraphs
  with page numbers and click-to-jump, Update TOC refreshes them. The TOC is
  exported as static entries to DOCX, ODT, RTF, HTML and Markdown, and is
  rendered by the PDF export.
- **Navigation pane** listing the heading outline, with jump-to-block.
- `keepWithNext` / `keepTogether` paragraph properties, written to DOCX as
  `w:keepNext` / `w:keepLines`.
- Performance smoke tests for 10k, 50k and 100k cell workbooks plus a
  1 000-cell dependency chain.

### Fixed

- Calc: array comparisons (for example `A1:A3>1`) inside functions evaluated
  only their first cell, silently giving a scalar instead of a column.
- Calc: a formula that returns a matrix stored the whole matrix in the cell
  value; the source cell now stores its first value and the rest spills.
- Writer: a table continuation page did not repeat the header row and table
  fragments could be laid out with the wrong row offset.

### Security

- No new macro or script behaviour; TOC text and keep properties are plain
  model data. The ZIP/XML/redaction hardening from earlier releases is
  unchanged and still covered by its tests.

### Compatibility

- `.oswk` files written by 2.2.0 and earlier open unchanged: every new field
  (`keepWithNext`, `keepTogether`, `pivotTables`, `toc` blocks) is
  serde-defaulted in the Rust model.
- DOCX export writes `w:keepNext`/`w:keepLines` and static TOC lines; the TOC
  is not a live Word field, which the save warnings state.
- XLSX export of pivot tables writes values, not a pivot cache, and warns.

### Tests

- Rust: 246 tests (was 241), including pivot engine tests and the XLSX pivot
  materialisation round trip.
- Frontend: 375 tests (was 325), including dependency-graph, spill,
  pagination-rule, pivot, and editor component tests.

## [2.2.0]

A reliability release: no new screens, but the editors, the formula engine and
the file formats were gone through with tests that reproduce the reported
problems. Detailed release notes: [docs/release-notes-v2.2.0.md](docs/release-notes-v2.2.0.md).

### Added

- **XLSX chart export** - column, bar, line, pie and area charts are written as
  real ChartML parts (`xl/charts/chartN.xml`) anchored to their cells through a
  drawing part, with titles, series names/colours, legends, axis titles and
  data labels. Chart kinds the exporter cannot represent are kept in `.oswk`
  and reported in the save warnings.
- **XLSX conditional-formatting export for every editor rule kind**: data bars
  now export (`type="dataBar"` with min/max colour scale), text-contains rules
  carry the required `text` attribute, top/bottom rules use `rank`, and the
  `textContains` / `duplicate` kind strings the editor writes are mapped -
  previously those two rules silently exported nothing.
- `WEEKNUM` (Excel types 1, 2, 11-17 and ISO type 21), sharing its ISO
  implementation with `ISOWEEKNUM`.
- Component regression tests that drive the real Calc and Writer editors with
  `@testing-library/user-event` and assert model state after keyboard flows.
- XLSX round-trip suite (`crates/officecore/tests/xlsx_roundtrip_test.rs`)
  against a 100-row, multi-sheet golden workbook with formulas, number formats,
  merges, widths, heights, freeze panes, a filter, list + numeric validation,
  conditional formatting and two charts.

### Fixed

- **Calc: consecutive typing after `Enter` is now reliable.** The grid keydown
  handler closed over the `editing` *state*, so the first keystroke after a
  commit was rejected by the previous render's closure; and focus was restored
  in a passive effect, which runs after paint, so the keystroke could be
  dispatched to `<body>` first. The handler reads the editor through a ref and
  focus is restored in the same event. Blur commits no longer pull focus back
  from the formula bar, and IME composition is ignored by the shortcut handler.
- **Writer: structural edits can no longer be undone by the blur handler.**
  A focused `contentEditable` is not re-rendered by React, so after Enter /
  Backspace / Delete the element still held the pre-edit text; moving focus
  fired `blur`, which read that stale DOM and wrote it back into the model
  (paragraphs duplicated on Enter, merges resurrected the removed paragraph).
  Affected paragraphs are repainted before focus moves, Shift+Enter restores
  the caret after the line break, and the caret for structural edits is placed
  in the same commit instead of a `setTimeout`.
- **Formula engine: silent wrong answers removed.** Lowercase references and
  sheet names (`=a1`, `=sheet1!a1`) were read as empty cells and are now
  case-insensitive like Excel; malformed references (`=A0+1`,
  `=SUM(A0:A3)`) now fail with `#REF!` instead of silently evaluating.
- **XLSX: `[Content_Types].xml` is well-formed again.** Comment and chart
  overrides were appended after the closing `</Types>` tag, which made the
  package unreadable for other office suites; an independent reader (openpyxl)
  catches this and the package structure now has a regression test.
- **XLSX: chart references are XML-escaped**, so sheet names containing `&` or
  `<` no longer produce a malformed chart part.
- **XLSX: empty headers no longer emit an unparseable `headerFooter`** - the
  element is omitted when the header is empty, which openpyxl previously
  warned about.
- Error messages shown to the user now go through the shared `errorMessage()`
  helper (localized by code) instead of raw `String(error)` output in the
  office workspace, the launcher and the batch/converter screens.
- The office `Dialog` exposes `role="dialog"` / `aria-modal` for assistive
  technology and tests.

### Improved

- **Formula engine reliability**: the A1/mixed/absolute/cross-sheet/quoted-sheet
  reference matrix and the malformed-reference cases are pinned down by tests;
  `a1`-style case-insensitivity now matches Excel.
- **Data bars in Calc** scale against the maximum of their range (like every
  spreadsheet) instead of an absolute percentage of the raw value, and the
  chosen colour is used.
- **XLSX writer** escapes chart titles/series names, validates chart ranges and
  reports each chart that could not be exported, instead of one blanket
  "charts are not embedded" warning.
- **README/tests documentation** updated to the real test counts and the real
  state of XLSX chart/conditional-format export.

### Security

- No new macro or script execution path was added. Chart and conditional
  formatting XML is escaped against package corruption; the ZIP, XML and
  redaction hardening from 2.1.0 is unchanged and still covered by its tests.

### Compatibility

- `.oswk` remains the lossless format; old files keep opening. XLSX export
  gained parts; XLSX import support is unchanged (values and formulas).
- `WEEKNUM` follows Excel's numbering schemes; `TODAY`/`NOW` unaffected.

### Tests

- Rust: 241 tests across the workspace (was 223); new XLSX structure tests for
  charts and conditional formatting, plus the 5-test round-trip suite.
- Frontend: 325 tests (was 303), including 12 new editor component tests that
  type Enter/Backspace/Shift+Enter and assert caret and model behaviour.

## [2.1.0]

### Added

- **Redact** (`nav.redact`) — permanent redaction. Text under a box is removed
  from the PDF content stream rather than covered. Detection for e-mail
  addresses, card numbers (Luhn), IBANs (mod-97), phone numbers and
  passport/ID numbers, with per-item selection. Image areas can be painted out
  or have their pixels removed. Optional metadata stripping. Cancellable, with
  progress. Verified by tests that re-extract the text and assert it is gone.
- **Compare** (`nav.compare`) — two documents compared page by page. A text
  pass classifies pages as added, removed or changed; an optional pixel pass
  with adjustable resolution and tolerance marks visual changes on a
  side-by-side diff. Per-document passwords, cancellable, with progress.
- **Inspect** (`nav.inspect`) — read-only document report: page geometry,
  fonts with embedded/base-14 distinction, images with dimensions, colour
  space and bit depth, form fields with labels and options, outline, links,
  annotations, encryption, linearization and metadata. Findings carry a
  severity and a plain-language explanation; accessibility conformance is
  derived from them.
- Tauri commands `redact_pdf`, `detect_sensitive_text`, `compare_pdfs` and
  `inspect_document`, all wired to the existing job registry for progress and
  cancellation.
- `docutil::page_xobjects` — resolves a page's image XObjects across inline,
  referenced and inherited `/Resources`. The inspector, the compressor and
  redaction all previously saw zero images for documents whose page resources
  are inline rather than inherited.
- Formula engine: ~70 additional functions (array literals, `LET`, named
  ranges, financial, date, lookup, statistical and matrix functions),
  extracted from a single file into `src/office/calc/functions/`.
- XLSX export: defined names, autofilter, hyperlinks, comments, conditional
  formatting, sheet tab colour and print settings.
- Version history screen: browse and restore local snapshots.
- 825 previously missing Turkish translations; the i18n audit now reports
  zero missing and zero extra keys in both directions.

### Fixed

- Calc: a double commit when pressing `Enter` after a formula edit, and focus
  moving off the grid instead of to the next cell.
- Calc: circular references no longer loop forever; they raise a readable
  error.
- Calc: formulas are now evaluated against the new workbook rather than the
  pre-edit one, so a formula that references a cell edited in the same action
  gets the right value.
- Writer: `font-size` values were being treated as pixels where they are
  points, and `font-family` was lost on DOM round-trip.
- 47 mojibake sequences in the English translation table.
- `settings.enginePdfium` had been appended to the end of an unrelated line
  and orphaned in the Turkish block, so the pdfium engine label never rendered.

### Changed

- Writer, Calc and Impress keyboard editing (Enter, Shift+Enter, Backspace,
  Delete, Tab/Shift+Tab, edge arrows, Home/End, PageUp/PageDown) is handled
  against a real model, and the pure run/caret logic has been extracted into
  `src/office/writer/` with tests.
- Document close now warns about unsaved changes, `Ctrl+W` works, and
  autosave flushes pending changes before a recovery prompt.

### Known limitations

These are real and unchanged by this release:

- Redaction removes text from the content stream and pixels from images, but it
  does not rewrite incremental-update history. If a document already carries
  an earlier revision containing the same text, that earlier revision can still
  contain it. Remove incremental updates before redacting if that matters.
- Accessibility conformance is a check of the properties this application can
  read (tagging, language, title, form labels, embedded fonts). It is not a
  certified WCAG or PDF/UA validation.
- The document inspector reports facts; it does not repair a document.
- Comparison renders both documents for the visual pass, which is
  meaningfully slower than the text pass on large files. `max_pages` bounds
  the work.
