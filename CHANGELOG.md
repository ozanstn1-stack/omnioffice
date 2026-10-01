# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
