# OmniOffice

A local-first productivity suite: a word processor (Writer), a spreadsheet
(Calc), a presentation editor (Impress), local productivity tools (Notes,
Planner, Data, Draw, Templates, PDF Forms, Document Vault) and the complete
PDF toolkit this project started from — on **Windows and Android**.

Everything runs on your machine. Documents are never uploaded, there is no
telemetry, AI is opt-in with your own provider, cloud sync is off until you
configure it, and the app stays useful without an internet connection. Macros
and embedded scripts in office files are never executed.

**Version 4.0.0** · Platforms: Windows (Tauri also targets Linux/macOS; the
desktop CI builds and tests all three, only Windows packaging is produced
here) and Android (arm64-v8a, armeabi-v7a) · UI languages: English, Turkish.

## What's new in 4.0.0

- **ODF formats keep more:** comments in ODT and RTF, groups and animations
  in ODP, and charts in ODS now survive saving and reopening.
- **Calc:** Text to columns, Remove duplicates, and a dropdown on cells with a
  list rule.
- **Writer:** find & replace with regular expressions and a single Replace,
  and a quick style gallery.
- **Turkish templates:** Dilekçe, Özgeçmiş, Toplantı Tutanağı, Fatura and
  Bütçe Tablosu.
- Fixes for ODT, ODP and ODS import (comment text in the body, lost shape
  positions, sheet names).

## What's new in 3.9.0

- **Update notice:** about once a week the app checks GitHub for a newer
  release and offers the Windows installer or the Android APK for your device
  (Settings can turn it off or check right away).
- **Android printing:** Print in Writer, Calc and Impress opens the document
  as a PDF in the system viewer, which prints it.
- **Export diagnostics** (Settings): a privacy-safe text report to attach to
  a GitHub issue; it is only saved where you choose.
- **Recent files** now include Word, Excel and PowerPoint documents, and new
  installs get a short welcome card on Home.
- **Phones:** Calc scrolls with one finger and has selection grips; the
  editor ribbons fold behind a "More" button.

## What's new in 3.8.3

Automatic releases and reliable CI (no change to the app itself).

- A version bump merged to master now tags the release and builds the Windows
  installer and Android APKs on its own; they appear on the GitHub release
  page without any manual step.
- Windows end-to-end tests and an Android start-up test run in CI, and the
  benchmark gate no longer fails just because the CI machine was slower.

## What's new in 3.8.2

Android pickers and save fixes.

- **Android:** Writer "Insert image", Impress pictures, Data import/export,
  Draw export, PDF Forms, the PDF Studio picker and Sync "Add file" now use the
  Android system picker / save dialog instead of failing silently. Exported
  office files carry their real type, so "open with" offers office apps.
- **Windows:** Draw -> PDF works again (no temporary file outside the allowed
  folder), and overwriting a file from PDF Forms or AI "Apply metadata" no
  longer fails after the save dialog confirmed it.
- **Data import** handles quoted CSV cells (`"Doe, Jane"`), `;` separators and
  more JSON shapes.

## What's new in 3.8.1

Templates open the editor again.

- Picking a template (Writer, Calc or Impress) now switches to the Office
  workspace with the new document in the active tab; before, the tab was
  created silently and the click looked like it did nothing.

## What's new in 3.8.0

New logo and a modernized shell.

- **New brand mark.** A gradient tile with a white "O" and a sparkle replaces
  the old "PDF" page-and-wrench icon; the same design is used for the app
  icon (Windows, macOS, Android adaptive icon), the Chrome extension icons,
  the favicon and the in-app brand (sidebar, drawer, Home hero).
- **Modern UI polish.** Deeper surfaces, softer shadows and larger radii;
  gradient primary buttons; branded active nav items, segmented controls,
  badges, cards, drop zones and progress bars; slimmer scrollbars; refined
  light and dark palettes. Nothing moved: same screens, same shortcuts.

## What's new in 3.7.0

The suite is now called **OmniOffice** (formerly Office Swiss Army Knife).

- **New name, same local-first suite.** Windows installs to
  `%LOCALAPPDATA%\Programs\OmniOffice` (the old per-user install and its
  shortcuts are removed automatically on update), the exe is
  `OmniOffice.exe`, and release files are `OmniOffice-*`. The Android package
  id and the `.oswk` file format are unchanged, so updates and existing
  documents keep working; the Chrome extension ships as
  `OmniOffice-Chrome-Extension-1.0.2.zip`.
- **Existing data keeps working.** The AI library falls back to the old
  `Documents/PDF Swiss Army Knife AI` folder until a new `Documents/OmniOffice
  AI` one exists; Android results go to `Downloads/OmniOffice` (older files
  stay where they are).
- **Per-machine installs** under `C:\Program Files\Office Swiss Army Knife`
  are not touched by the per-user updater; uninstall them from Windows
  Settings (administrator) after updating.

## What's new in 3.6.1

Writer wrapping fix and office input for the AI assistant.

- **AI assistant reads office documents.** Summaries, translation, Q&A, text
  cleanup and metadata suggestions accept DOCX/DOTX, ODT, RTF, DOC/DOT,
  TXT/MD/HTML, XLSX/XLS/ODS, CSV/TSV, PPTX/PPT, ODP and `.oswk` in addition to
  PDF: writer text becomes content chunks, spreadsheets one unit per sheet and
  presentations one unit per slide, so the page selector still works. The
  metadata "Apply" action stays PDF-only and is hidden for office inputs.
- **Writer wrapping fix.** Paragraphs now shrink to the A4 text column and
  long words break, instead of typing running off the right edge of the page.

## What's new in 3.6.0

PDF depth: content-stream text editing, reader bookmarks and RFC 3161
timestamps.

- **PDF Studio → Text.** Every text run of a page's content stream is listed
  (text, font, size, approximate position) and can be edited in place: only
  the string operand changes, so the font and layout stay put and the result
  is saved as an incremental revision - signatures over the original bytes
  remain valid. Replacement text is encoded with the run's font and verified
  by a round trip; text the font cannot represent is refused instead of
  garbled, and multi-string `TJ`/composite-font runs are marked read-only with
  the reason.
- **Reader bookmarks.** The document outline is parsed (bounded) and shown in
  the Reader's side panel with indentation and click-to-page navigation.
- **RFC 3161 timestamp signatures.** Sign with an optional TSA URL: the
  signature value is timestamped over HTTPS and the token is embedded; the
  verification report shows the token's `genTime`. A requested timestamp is
  mandatory, so a TSA failure fails the signing rather than being dropped.

## What's new in 3.5.7

Legacy Word/PowerPoint import and real OAuth cloud sync.

- **Word 97-2003 (.doc) and PowerPoint 97-2003 (.ppt) open now.** The new
  `officecore::legacy` importer reads the OLE2 container directly (Word's FIB
  piece table, PowerPoint's record-tree text atoms) so the documents open in
  Writer/Impress without a native Office dependency; when a local LibreOffice
  is installed it is used first for a full-fidelity conversion. A legacy tab
  has no save path: the first Ctrl+S asks for `.docx`/`.pptx`/`.oswk` and the
  original binary file is never edited in place.
- **Google Drive and OneDrive sync with OAuth 2.0 PKCE.** Sign-in opens the
  provider's own page in your browser and returns to a loopback listener on
  this machine; tokens live in the OS credential vault (Windows Credential
  Manager / macOS Keychain / Secret Service), and the Sync screen gains an
  OAuth panel with client ID, Connect/Disconnect and the connected account.
  The conflict rules are the same as WebDAV: conditional writes (Graph
  `If-Match`, Drive `sha256Checksum` comparison), hash-checked downloads and
  manual conflict resolution.
- **The converter completes its list**: PDF → JPG/PNG/TXT/DOCX and image → PDF
  are handled directly by the universal converter (a PDF input now offers JPG,
  PNG, TXT and DOCX).

## What's new in 3.5.6

Home opens what you pick and the Android converters work.

- **Picking a file on Home opens it.** A single picked or dropped document
  routes through the same logic as open-with: office documents in the
  workspace, PDFs in the reader, images in image-to-PDF. The Home picker also
  offers every format the suite opens, not just PDF and images.
- **PDF → images no longer fails on an existing file.** Multi-output exports
  default to unique names (matching the UI) and write into a per-document
  folder (`<document>_images/`), so a second export never collides.
- **Android tools get a staging folder.** The app-private `Outputs` folder is
  created before the first run; the native commands reject a missing output
  folder, which is why merge worked on Windows but not on Android.
- **The converter and cleaner use the Android picker.** Both used the desktop
  dialog plugin, which cannot open SAF: they now pick through the SAF bridge
  and publish finished files to the chosen folder or Downloads. A PDF input
  offers JPG/PNG; PDF → Word/Excel is not implemented.
- **Home reads as an office suite**: tagline, group order (Office first) and
  card descriptions lead with Writer, Calc and Impress.

## What's new in 3.5.5

Data-integrity fixes, qpdf repair and hard quality gates.

- **XLSX cross-sheet comments no longer contaminate each other (audit C11).**
  The exporter wrote one comments part for the whole workbook, so two sheets
  with a comment on the same address ended up sharing one comment after a
  save. Comments are now written as one part (and VML shape set) per sheet,
  wired through that sheet's own relationships; the regression test re-reads a
  two-sheet workbook and asserts each note stayed on its cell.
- **Writer tracked changes preserve the runs they are not editing (audit
  M12).** Suggest mode diffed flattened text and rebuilt the paragraph from
  the first run's format, restyling surrounding text, dropping footnote/field
  anchors and re-issuing revision ids on every keystroke. The diff now works
  on runs: formatting, links and anchors survive, an existing insertion keeps
  its id, deleting freshly suggested text cancels the insertion, and a pending
  deletion is no longer silently accepted while revisions are hidden.
- **PDF Studio Repair and Fast Web View.** The bundled qpdf engine is used at
  last: Repair rewrites a damaged PDF (broken xref/trailer, dangling objects)
  next to the original and Fast Web View writes the linearized layout. Both
  re-open the result, report its real page count and surface qpdf's
  diagnostics, with Jobs-screen retry like every other tool.
- **Benchmark regression gate.** The master bench job compares criterion means
  against the previous run's cached baseline and fails on a >15 % regression
  (warning at 7.5 %); the baseline only advances on a passing check.
- **Deeper desktop E2E.** `npm run e2e:flows` drives a real Writer type →
  Ctrl+S round trip (the DOCX on disk must change), a Merge auto-run and the
  Studio sanitizer through its real button; CI runs them engine-free on Linux.
- **The formatting backlog is cleared.** Prettier and rustfmt ran over the
  whole tree once, and CI now enforces formatting on every changed file plus
  the full repository instead of only newly added files.
- Rust dependencies: `cbc` 0.2.1 + `des` 0.9 replace the 0.1/0.8 pair (the
  versions lopdf already uses) and the unused `aes` dev-dependency is gone.

## What's new in 3.5.4

Quality infrastructure: real-app E2E, fuzzing and benchmark trends.

- **Desktop E2E through tauri-driver.** `npm run e2e:smoke` launches the real
  binary (embedded frontend, debug profile) through WebDriver and asserts the
  home tool grid plus Settings navigation, saving a screenshot; the Windows
  variant `npm run e2e:smoke:pdf` also opens a PDF in the Reader and waits for
  the rendered page. CI runs the engine-free smoke on Linux (Xvfb +
  WebKitWebDriver) on every pull request and uploads the screenshots.
- **Fuzzing.** A `fuzz/` crate with three libFuzzer targets: the hardened ZIP
  reader, the depth-limited XML parser, and the DOCX/XLSX/PPTX/ODT/RTF readers
  behind a container magic dispatcher. The seeded corpus ships with the
  samples; a nightly CI job on master runs each target for 30-60 s. (Fuzzing
  links libFuzzer, which is Linux/macOS only - Windows can build the targets
  with `cargo +nightly check` but not run them.)
- **Benchmark trends.** Criterion benchmarks for DOCX/XLSX/PPTX import and the
  lossless PDF compression path (`cargo bench -p officecore --bench parse -p
  pdfcore --bench pdf_ops`); the master CI job uploads the criterion report
  as an artifact, which is the comparison point for the next run.
- **Dependency policy.** `cargo deny` now checks licenses (permissive
  allow-list), duplicate versions and wildcard requirements, and registry
  sources in addition to advisories; the app crate is marked `publish =
  false` (the path dependencies are internal), and the glib advisory
  exception is retired because the Tauri update moved past it.

## What's new in 3.5.3

Cleanup release: the Jobs "Retry" button finally works, the vault can be
cleared from the UI, and two long-standing audit bugs are fixed.

- **Background jobs are retryable.** Every long operation (merge, split,
  compress, OCR, protect/unlock, watermark, annotate, redact, page tools,
  metadata, conversions, PDF Studio sanitize/flatten/PDF-A, AI actions and the
  vault scan) now persists a stable kind plus the exact invoke arguments with
  the job, and the app registers one retry handler per kind. A failed or
  interrupted job can be re-run after a restart even though its screen is not
  mounted. Credential-looking fields (passwords, tokens, keys) are blanked
  before the payload reaches `jobs.json`, so retrying an encrypted document
  asks for the password again instead of leaking it. Jobs from older builds
  without a handler show an honest "no automatic retry" note instead of a
  button that only produced a toast.
- **Vault clear in the UI**: the Document Vault settings gain a Clear action
  with a confirmation dialog; on Android it can also delete the imported
  document copies from app storage (previously only unreachable from Rust).
- **Writer fields and lists refresh** (audit M16): PAGE/NUMPAGES/DATE/TIME/
  TITLE/AUTHOR are rendered from the pagination result and metadata instead of
  the value cached at insertion time, and ordered lists are numbered across
  the document instead of repeating "1".
- **Unique-name outputs reserve their path atomically** (audit M7):
  `create_new` replaces the exists-then-write check, so two concurrent
  operations can no longer pick the same "unique" name and silently overwrite
  one result.
- **Tests and gates**: Organize, Annotate, Batch, Page Tools and Plugins now
  have render/interaction tests (38 files, 673 tests), the coverage floors rose
  from 58/68/46/58 to 62/69/46/62, and the Dependabot updates (base64 0.23,
  Rust/frontend/actions groups, extension TypeScript) are merged.

## What's new in 3.5.2

Pinch-to-zoom follows the fingers.

- **Live pinch scaling.** The reader previously only applied the zoom after the
  180 ms debounce, so nothing moved while the fingers were moving. While two
  fingers are down the pages now scale through a CSS transform that mirrors
  the gesture exactly (including the two-finger pan), and the final zoom is
  committed on release, when the debounced page width and the sharp render
  catch up. The transform keeps the pinch midpoint anchored; a pure two-finger
  pan is folded into the scroll offset.

## What's new in 3.5.1

Reader zoom on Android is sharp and light.

- **Physical-pixel previews.** Page previews were rendered at a fixed 96 dpi
  and only ever downscaled, so the webview upscaled the bitmap and zoomed text
  was blurry. The preview pipeline now renders at a high dpi and caps the
  result at the requested raster width, and the reader asks for CSS width ×
  device pixel ratio (capped at 3000 px; backend accepts up to 4000).
- **Progressive, reusable cache.** The reader keeps one bitmap per page (LRU,
  24 entries): zooming out reuses a sharper cached page instantly, and only a
  sharper zoom triggers a render. The page stays visible while the sharper
  bitmap loads instead of flashing a spinner, and the prefetch margin shrinks
  for high-resolution renders so Android does not rasterize neighbours it may
  never show.
- **Smoother pinch.** Pinch updates are coalesced to one state change per
  frame and pages are memoized, so a gesture no longer re-renders every page
  on every pointer event.
- Page previews on the other screens (`PageCanvas`: Info, Watermark, PDF
  Studio) get the same physical-pixel rendering.

## What's new in 3.5.0

Test coverage became a gate instead of a number.

- **Coverage ratchet.** `npm run test:coverage` now fails below the measured
  floors (statements 58 %, branches 68 %, functions 46 %, lines 58 %), and the
  frontend CI job runs the coverage command. The floors move up with the
  suite; they are never lowered to make a red build green.
- **The untested screens have tests.** 24 new tests render the real screens
  against mocked backends and pin the request envelopes: Merge, Split,
  Compress, Security (protect), Watermark, Metadata, Info, History, Home,
  Settings, Notes, Planner, Data, Draw, Templates, Converter, Cleaner, PDF
  forms, AI Library, Jobs, Compatibility, OCR and PDF→images. Coverage moved
  from 49.2 % to 60.7 % statements on the same source.
- **Templates contract.** The built-in templates are tested for unique ids,
  complete metadata and the model kind each card promises.

## What's new in 3.4.0

Quick wins from the roadmap: the command platform, Settings and Home become
useful instead of aspirational, and accessibility gets a first pass.

- **The command palette runs the app.** Every screen is a registered command
  and the palette keyboard bindings are wired through `matchKeybinding`
  instead of a hard-coded key handler. `Ctrl+Shift+P` (palette),
  `Ctrl+Shift+F` (search), `Ctrl+,` (settings) and `Ctrl+O` (open) are
  commands like any other, and the global open dialog accepts office formats
  too — documents open as workspace tabs.
- **Settings completion.** `midnight` and `paper` themes are selectable, the
  office defaults (Writer/Calc/Impress save format, version history, import
  warnings) are in the UI and reach the save/open code paths, the shortcut
  list is real, and editor shortcuts are labeled as editor-scoped.
- **Home is a tool directory.** The drop suggestion keeps every dropped file
  (multi-file Merge no longer loses the selection), the grid is searchable
  and grouped (all 42 screens), recent PDFs open in the Reader, and the
  reveal action is a labeled button.
- **Accessibility pass.** `aria-current` navigation, modal focus trap and
  focus restore, roving tabindex + arrow keys in segmented controls, a live
  region for background jobs, `lang` follows the UI language, translated
  control labels, and `prefers-reduced-motion` support.
- **Calc `CUMIPMT`/`CUMPRINC`** implemented with Excel reference values and
  argument validation (the dead "unsupported" list is gone).
- **Quality.** Frontend coverage now runs in CI (informational baseline), the
  README test counts are generated from a run rather than prose, and the
  `compat.rs` matrix has a contract test for the DSS/RFC 3161 note. The
  capability matrix now describes the signature state accurately (DSS is
  written for offline PAdES B-LT; RFC 3161 timestamps are not requested yet).

## What's new in 3.3.1

Security hardening and reliability patch on top of 3.3.0:

- **AES-256 keys come from the OS entropy source** (`getrandom`) instead of
  UUIDv4 values, which only carry 122 random bits each.
- **Input path validation uses an open handle.** `input_file()` validates the
  object that was actually opened and, on Unix, compares the handle identity
  with a fresh path lookup so a symlink swapped in between is rejected.
- **AI request structs redact document passwords** in their manual `Debug`
  implementations, so a log line or error context can never leak them.
- **Duplicated pages copy their annotations.** Cloning a page now copies each
  annotation dictionary and points its `/P` at the duplicate; previously the
  duplicate shared the annotation objects with the original page.
- **`LET` self-reference regression tests** prove the evaluator terminates
  (unbound name → `#NAME?`, matching Excel) and defined-name cycles report a
  circular reference; separator-only array literals (`{,}`, `{;}`) report
  `#VALUE!`.
- **UTF-16 round-trip tests** cover Turkish, Japanese and astral-plane text,
  and the date conversion helpers guard against integer underflow.

## What's new in 3.3.0

Production readiness on top of the 3.2.1 audit. `RELEASE_READINESS.md` states
what is tested, what is CI-only and what is not done yet.

- **Canonical `.oswk`.** The native format is now a versioned envelope with
  `documentType`, `applicationVersion`, a SHA-256 `checksum` of a canonical
  model serialization, a `featureManifest` and an `extensions` bag. Unknown
  fields survive a round trip, a checksum mismatch is reported as
  `corrupt_document`, migrations recompute the digest, and a newer schema is
  refused rather than rewritten.
- **Writer transactions and model undo/redo.** Structural edits are reversible
  operations with a bounded checkpoint + delta history, wired to Ctrl+Z /
  Ctrl+Y / Shift+Ctrl+Z and the toolbar. The browser's native undo could not
  see Enter/Backspace/format changes.
- **External file-conflict guard.** A document's SHA-256 is captured at
  open/save and re-checked before writing; if another program changed the
  file, the app offers Reload / Save as new / Cancel instead of silently
  overwriting it.
- **Calc Excel compatibility.** Approximate `MATCH`/`HLOOKUP` now return the
  position in the original range, `XLOOKUP` modes ±2 work, `COUNTIF`/`SUMIF`
  accept `*`/`?` wildcards, `NUMBERVALUE` handles locale separators, `FILTER`
  treats blank mask cells as FALSE, and `SUMPRODUCT(--(range>1))` works.
- **Release metadata**: `build-info.json` records the version, git SHA and
  toolchain of every build.

## What's new in 3.2.1

Security hardening, centralized path validation, and job reliability on top of 3.2.0:

- **Centralized Filesystem Validation.** All 78 Tauri filesystem commands are
  strictly guarded by typed `ValidatedInputFile`, `ValidatedOutputFile`, and
  `ValidatedDirectory` wrappers, blocking traversal, NULs, device names, and UNC paths.
- **Plugin Sandbox & SSRF Defense.** Plugin installs use native Rust folder dialogs
  rather than trusting webview paths. Plugin HTTP requests validate destinations
  with DNS resolution, blocking cloud metadata (`169.254.169.254`), private IPs, and loopbacks.
- **Trinary PDF Redaction Verification.** Redaction results enforce a trinary contract
  (`Removed`, `PossiblyPresent`, `NotVerified`) to prevent false successes and warn on remaining text.
- **Digital Signature Gate.** `pdf_sign` cryptographically verifies output signatures and
  ByteRange before reporting success.
- **Job Lifecycle Reliability.** RAII `JobFinishGuard` ensures determinism across all terminal states.

## What's new in 3.2.0

Quality gates and test confidence on top of 3.1.1:

- **Zero-warning lint, every rule an error.** The React Compiler diagnostics
  (set-state-in-effect, refs, immutability, purity, exhaustive-deps…) and the
  jsx-a11y interaction rules were burned down to zero and promoted to errors;
  the per-rule warning budget is gone.
- **WebDAV end-to-end tests.** A self-contained in-process DAV server
  (PROPFIND/MKCOL/PUT/GET/DELETE with conditional writes) drives the real
  provider through list/upload/download/delete, 412-conflict mapping and
  Depth-header assertions.
- **On-device Android intent tests.** The open-with / share pipeline
  (untrusted display names, extension whitelist, size cap) now runs against a
  real `content://` provider on an emulator in CI, next to the JVM unit tests.
- **Owned, time-boxed advisory exceptions.** `deny.toml` records an owner and
  a review date for every ignored RustSec advisory and CI enforces it with
  `cargo deny check advisories`.

## What's new in 3.1.1

Security and supply-chain hardening on top of 3.1.0:

- **Transport security is enforced, not assumed.** AI provider and WebDAV
  endpoints must be `https://`; plain `http://` is accepted only for loopback
  servers (`localhost` / `127.0.0.1` / `::1`) and, for WebDAV, only with an
  explicit opt-in in the settings. Redirects that downgrade to a public
  `http://` endpoint are refused before the credentials (Basic Auth / Bearer
  token) can leave the machine.
- **Engine downloads are fully pinned.** Both previously missing SHA-256
  entries are in `engines.lock.json` and a missing pin now fails the build
  instead of printing a warning; CI runs `-VerifyLock` over the whole download
  surface of both fetch scripts.
- **The Chrome extension context menu works.** "Open with PDF Swiss Army
  Knife" now hands the linked PDF to the app, which asks for consent, requests
  the optional host permission for that one origin and opens the downloaded
  file. Hostile `javascript:` / `data:` / `file:` links are ignored; covered
  by unit tests and a headless-Chrome E2E check.
- **Android storage access narrowed.** The WebView no longer holds a standing
  grant to `$DOCUMENT/**` / `$DOWNLOAD/**`; intermediate files live in
  app-private data/cache and every user-visible file goes through SAF. The
  open-with intent pipeline (untrusted names, extension whitelist, size cap)
  has JVM unit tests run by the release build.
- **Smaller startup, guarded maintenance.** Screens load lazily (entry bundle
  315 KB → 155 KB gzip) with a CI bundle budget, the ESLint gate is now a
  per-rule baseline with four accessibility rules promoted to error, WebDAV
  transfers stream instead of buffering up to 512 MB, OS open/reveal goes
  through validated document-only commands, and every GitHub Action is pinned
  to a commit SHA.

## What's new in 3.1.0

V3.1 completes the cross-platform story and raises Office/PDF fidelity:

- **Writer is a real WYSIWYG editor.** Click anywhere on a page and type
  there — the caret lands on the clicked character, Enter/Backspace/Delete/
  Shift+Enter work in place, selection and paste are native, and the caret
  **crosses pages** with the arrow keys. Caret position is preserved across
  reflow. Touch works the same way on Android.
- **Real digital signatures.** Detached CMS/PKCS#7 signatures (X.509,
  SHA-256, RSA or ECDSA P-256) with a visible appearance, a ByteRange that
  covers the saved revision, and re-verification of the written file.
  Certificates come from a PKCS#12 file on any platform or from the Windows
  certificate store; validation reports digest match, modification, signer
  identity and chain status. Trust is honestly reported as `unknown` offline.
- **PDF Studio grew forms and objects.** List, fill and validate AcroForm
  fields (text, checkbox, radio, dropdown, list) with regenerated appearance
  streams; flatten afterwards; select annotations, form widgets and drawn
  images to move, resize, rotate or delete — mouse on Windows, touch handles
  on Android. Text/vector content streams are documented as out of scope.
- **PDF/A conversion embeds missing fonts.** Simple fonts are substituted
  with bundled OFL fonts (Liberation Sans for the Helvetica/Arial family —
  metric-compatible — and PT Sans as a warned fallback), embedded as
  `/FontFile2` with real metrics, and the output intent carries a genuine
  sRGB ICC v4 `/DestOutputProfile`. What cannot be embedded (CID/Type0,
  symbolic fonts, custom encodings) is reported, never silently skipped.
- **XLSX import keeps the file, not just the numbers.** Charts,
  pictures with anchors and rotation, print settings (page setup, margins,
  header/footer, row/column breaks, Print_Area/Print_Titles), sheet
  protection and pivot parts now round-trip; pivot caches are preserved
  losslessly rather than recomputed.
- **PPTX charts carry data.** The chart dialog edits real categories and
  values; export writes `c:strCache`/`c:numCache` and an embedded Excel
  workbook, so other office suites show the chart instead of an empty plot.
- **ODT and RTF keep notes and tracked changes.** Footnote/endnote objects
  (`text:note`) and RTF `\footnote`/`{\revised}`/`{\deleted}` with author
  tables round-trip.
- **Android is a first-class platform.** Open-with intents (content:// URIs
  copied safely into cache), SAF import/export for office formats, Document
  Vault import + local indexing, persistent background jobs that survive
  process death, touch UX in every editor and the reader (pinch zoom, drag
  pan, object handles), in-app back navigation, hardened manifest with
  backups disabled.
- **Sandboxed plugin runtime.** Plugins are manifest-validated, run in a Web
  Worker with no DOM and no IPC, and every capability (`read_document`,
  `modify_document`, `read_files`, `write_files`, `clipboard`, `network`) is
  enforced by the host. A crashing plugin never takes the app down.
- **Local-first cloud sync foundation.** A real WebDAV provider (conditional
  PUT, ETag-aware) with per-file metadata and **three-way conflict
  detection** (local/cloud/base) and manual resolution: keep local, keep
  cloud, or keep both. Off by default, no silent overwrites; OneDrive/Google
  Drive are declared OAuth-only and disabled with an explicit message.
- **Data Loss Protection.** Every non-`.oswk` save/export — including the
  Universal Converter — runs the compatibility matrix first and shows
  Feature / Supported? / Imported? / Exported? / Transformed? / Lost? with
  Continue, Cancel and **Save as .oswk**.
- **Golden-file contract and performance guards.** V3.1 `.oswk` fixtures pin
  feature survival across `.oswk` and DOCX/XLSX/PPTX round trips, and
  performance tests cover large documents, spreadsheets and PDFs.

## Verified workflows

These were exercised on the built application and with automated tests:

- **Writer**: clicking a page fragment places a real caret inside the page
  sheet at the clicked character and typing updates the model; Enter splits,
  Backspace merges across fragments, ArrowDown crosses to the next page;
  DOCX round-trip tests cover sections, notes, revisions, fields, comments,
  and the V3.1 golden fixture covers all of it together.
- **Calc**: click a cell → type → `Enter` commits and moves on;
  `=SUM(Sales[Amount])` evaluates; editing a table cell recalculates its
  dependents; trace buttons highlight precedents and dependents; the XLSX
  round trip preserves layout, charts, pictures, print settings, protection,
  validation, conditional formatting, comments, links, names, tables and
  pivot parts.
- **Impress**: the sample PPTX loads with slides, shapes, images, tables,
  notes and transitions; masters/layouts round-trip; a nested group moves as
  one; charts export with caches and an embedded workbook; the slideshow runs
  entrance/emphasis/exit animations and the presenter view shows the next
  slide and notes; touch drags objects on Android.
- **PDF**: every tool from v1.x is unchanged and still covered by its tests.
- **Signatures**: `sign_pdf` output is verified by re-parsing the CMS with an
  independent parser, recomputing the byte-range digest and checking RSA/
  ECDSA signatures; tampering after signing is detected; a wrong PFX password
  is an error, never a fake success. The Windows store signing path was
  end-to-end tested with a temporary certificate.
- **Forms**: filled values set `/V`, appearances are regenerated and parsed
  back; validation flags required/max-length/option problems; flattening
  removes the interactive fields (tested).
- **PDF/A**: a document with an unembedded Helvetica is converted, the
  descriptor gains `/FontFile2`, the ICC profile stream appears, and the
  re-validation reports the font check as passing; Type0 fonts are reported
  as skipped and compliance is not claimed.
- **Redaction**: text under a redaction box is *deleted from the content
  stream*; the verification pass re-opens the output, re-extracts the text
  and reports any remaining matches (masked) instead of assuming success.
- **Sanitizer**: tests inject JavaScript, an OpenAction, an embedded file and
  an unsafe annotation, then walk every object in the output to prove they are
  gone, and Inspect confirms it independently.
- **Vault**: indexes user-picked folders on Windows and SAF-imported
  documents on Android; search, snippets and preview work on both; the index
  is crash-resistant and incremental.
- **Jobs**: jobs persist across restarts, running ones come back as
  `interrupted` with retry where routing exists, and cancellation works.
- **Plugins**: the sample Word Frequency plugin reads the document through
  the capability API, and permission tests prove a plugin without
  `write_files` cannot touch the sandbox.
- **Sync**: conflict states are unit-tested in a three-way matrix; uploads
  are conditional (`If-Match`) and a 412 maps to a conflict, never a silent
  overwrite.

## Modules

### Writer (word processor)
- DOCX, ODT, RTF, TXT, Markdown, HTML import/export · PDF export · lossless `.oswk`
- Styles with based-on/next inheritance, fonts, bold/italic/underline/strike,
  super/subscript, colour, highlight; alignment, spacing, indents
- Lists (bullet/numbered/multilevel), tables, images, hyperlinks, page breaks,
  horizontal rules, table of contents, navigation pane
- **Sections**: per-section page size/orientation/margins/columns, section
  breaks (new page/continuous/odd/even), default/first/even headers and footers
- **Notes**: footnotes and endnotes with automatic numbering, reserved note
  area, DOCX and ODT objects plus RTF destinations
- **Track changes**: run-level insertions, deletions and formatting changes
  with suggest mode, review pane, accept/reject (single and bulk),
  next/previous, DOCX round trip and RTF `\revised`/`\deleted` marks
- **Comments** with replies and resolve; **bookmarks, fields and cross
  references** (REF/PAGEREF/DATE/TIME/TITLE/AUTHOR)
- **Direct paginated editing (V3.1)**: caret on the page, cross-page caret
  movement, selection, pointer/touch input, caret persistence across reflow
- Find & replace, word/character/page count, zoom, print, PDF export with
  selectable text

### Calc (spreadsheet)
- XLSX, ODS, CSV/TSV import/export · XLS import (read-only) · PDF export
- Virtualised grid, name box and formula bar, multi-sheet workbooks
- Formula engine with 160+ functions, `LET`, named ranges, inline arrays,
  dynamic arrays with spill, explicit errors and circular-reference detection
- Dependency graph with incremental recalculation
- Formula autocomplete (functions, names, sheets, tables, columns, argument
  hints) and auditing (trace precedents/dependents, circular/invalid refs)
- Structured tables with headers, totals, banded rows, calculated columns,
  filters and structured references (`=SUM(Sales[Amount])`)
- **XLSX import fidelity (V3.1)**: styles, number formats, widths, heights,
  merges, freeze panes, validations, conditional formatting, hyperlinks,
  comments, names, tables, charts, pictures, print settings, sheet
  protection and preserved pivot parts
- Cell formatting, sorting, filtering, conditional formatting, data
  validation, freeze panes, charts (column/bar/line/pie/area), pivot tables
- **Mobile (V3.1)**: touch selection, fill handle, pinch zoom, bottom-docked
  formula bar, scrollable toolbars and tabs

### Impress (presentations)
- PPTX and ODP import/export · PDF export · lossless `.oswk`
- **Master slides and layouts** with placeholder inheritance
- **Grouped shapes**: nested groups with group-level transforms
- **Charts (V3.1)**: ChartML import/export with titles, series, legend, axes,
  stacking, data labels, cached categories/values and an embedded workbook
  (editable data grid in the chart dialog, clipboard paste)
- **Animations**: entrance/emphasis/exit with triggers, duration and delay;
  the slideshow executes them; `<p:timing>` round trip
- **Presenter view**: current/next slide, notes, timer, navigation
- **Mobile (V3.1)**: touch object move/resize/rotate, marquee select,
  double-tap to edit, coarse-pointer handles
- Eight layouts, six themes, transitions, full-screen slideshow, speaker notes

### PDF module
Reader with search, Merge, Split, Organize, Compress, OCR (Tesseract), Protect
(AES-256), Unlock, Watermark, Annotate, Metadata, Page tools, PDF → JPG/PNG,
JPG/PNG → PDF, Batch, Info, Redact, Compare, Inspect and the optional offline
AI assistant — all unchanged.

**PDF Studio (V3.1, repair in V3.5.5)**: **Repair and Fast Web View** (the
bundled qpdf rewrites a damaged file or writes the linearized layout, then the
result is re-opened and its page count reported), Sanitize
(JavaScript/attachments/actions/unsafe annotations/metadata with a removal
report), Flatten, PDF/A-1b/2b/3b
validation and conversion with **real font embedding and an ICC output
intent**, **Signatures** (list/validate/sign with Windows store or PKCS#12,
visible appearance, counter-signing: a second signature is appended as an
incremental update and leaves the first one valid, and a signature that a later
revision superseded is reported as such instead of "modified"), **sign-safe
editing** (metadata changes, stamps, watermarks and page numbers on a signed
document are appended as a new revision - the original bytes stay
byte-identical, so every signature remains valid and the change is visible as a
later revision; the Studio screens carry a "keep existing signatures" switch
for it), **Validation
data archiving** (the certificate chains a signature needs are written into the
document DSS as an incremental update - the offline half of PAdES B-LT, nothing
is downloaded), **RFC 3161 timestamps** (an optional TSA URL; the token is
embedded as an unsigned attribute and the verification report shows its
`genTime`), **Text editing** (list a page's text runs and replace their text
in place as a new revision - fonts, layout and existing signatures stay
intact), and **Forms & objects** (list/fill/validate AcroForm
fields, move/resize/rotate/delete annotations, widgets and drawn images).

**OCR**: preprocessing runs on the rendered page — orientation detection via
Tesseract OSD on the raster, deskew, denoise, threshold and contrast,
reported in the OCR result.

**Reader (V3.1)**: pinch zoom, drag pan, double-tap zoom, touch-friendly
toolbar.

### Document Vault
- Windows: index folders you explicitly choose; nothing is scanned by default
- Android: import documents through SAF into app-private vault storage and
  index those copies (SAF has no browsable paths for the Rust scanner)
- DOCX, ODT, RTF, TXT, Markdown, HTML, XLSX, ODS, CSV, PPTX, ODP and PDF
- Full-text search with exact/phrase/fuzzy modes, filters, snippets and a
  preview panel; crash-resistant atomic index, incremental rescans

### Document platform
- Unified command registry feeding the palette, shortcuts and menus
- Command palette (`Ctrl+Shift+P`) and global search (`Ctrl+Shift+F`)
- **Persistent background job center (V3.1)**: progress, cancel, retry,
  interrupted-job recovery, `jobs.json` state
- **Data Loss Protection (V3.1)**: compatibility report before every lossy
  save/export with Continue/Cancel/Save as `.oswk`
- Compatibility Center: per-format capability matrix and loss reports
- `.oswk` schema versioning with migrations; newer schemas are refused
- Autosave with crash recovery, local version history (25 snapshots)
- **Plugins (V3.1)**: sandboxed Web Worker runtime, manifest permissions,
  install/list/remove, sample plugin; crash-isolated
- **Cloud sync (V3.1, OAuth in V3.5.7)**: WebDAV plus Google Drive and
  OneDrive through OAuth 2.0 PKCE (loopback sign-in, tokens in the OS
  credential vault), all with conflict detection and manual resolution; off by
  default

### AI assistant (opt-in)
- Providers: DeepSeek, OpenAI-compatible endpoints, Ollama (local), Gemini,
  custom — with capability flags
- Per-document consent before any request; the network activity line shows
  the provider and the character count being sent
- Send scope: whole document, current page or selected text (Windows and
  Android)
- Document chat with `[page N]` citations, summaries, translation, text
  cleanup, metadata suggestions
- API keys use Windows DPAPI when available; the Android build reports its
  plaintext fallback and excludes app data from device backups. Keys are
  never logged, never written into documents, versions or crash reports.

## Supported formats

Only combinations that actually work are marked. “–” means not supported.

| Format | Open | Edit | Save | PDF export |
|---|---|---|---|---|
| DOCX | ✓ | ✓ | ✓ | ✓ |
| DOCM / DOTX / XLSM / PPTM | ✓ | ✓ | ✓ | ✓ (macros are detected, never executed and dropped on save) |
| DOC / DOT | ✓ (text import; save as .docx/.oswk) | ✓ (in memory) | – | ✓ (via Writer) |
| ODT | ✓ | ✓ | ✓ | ✓ |
| RTF | ✓ | ✓ | ✓ (basic formatting, tables, images, notes) | ✓ |
| TXT / Markdown / HTML | ✓ | ✓ | ✓ | ✓ (via Writer) |
| XLSX | ✓ | ✓ | ✓ | ✓ |
| XLS | ✓ | – | – | – |
| ODS | ✓ | ✓ | ✓ | ✓ |
| CSV / TSV | ✓ | ✓ | ✓ | – |
| PPTX | ✓ | ✓ | ✓ | ✓ |
| PPT | ✓ (slide text import; save as .pptx/.oswk) | ✓ (in memory) | – | ✓ (via Impress) |
| ODP | ✓ | ✓ | ✓ | ✓ |
| PDF | ✓ | ✓ (tools + Studio + forms + signatures) | ✓ | – |
| JPG / PNG / BMP / GIF / WebP | ✓ | ✓ (as images) | ✓ | ✓ (images → PDF) |
| SVG | ✓ (inserted as image) | ✓ | ✓ (media in DOCX/ODT) | ✗ (not rasterised) |
| `.oswk` unit | ✓ | ✓ | ✓ | ✓ |

Android runs the same engines and formats; file access goes through the
Storage Access Framework and the UI adapts to touch and phone screens.

## Android

**Supported features.** Writer/Calc/Impress editing (touch, virtual keyboard,
pointer gestures), PDF tools and PDF Studio (including signatures with a
PKCS#12 file, forms and redaction), Document Vault import/search/preview,
background jobs with persistence, AI (opt-in; local network or remote
endpoints), plugins, data-loss protection, command platform (drawer
navigation and touch entry points).

**Supported formats.** The same table as above. Open-with intents and SAF
import accept docx, odt, rtf, txt, md, html, xlsx, ods, csv, tsv, pptx, odp,
pdf and `.oswk`.

**APK architectures.** `arm64-v8a` and `armeabi-v7a` release APKs are built
and signed with the project release keystore; AABs are produced for both.
minSdk 24, targetSdk 36, `versionName 3.3.1`, `versionCode 3003001`.

**Storage behavior.** Documents opened from other apps are copied into app
cache (extension and size validated) before parsing. Exports go to a SAF
location you pick, or to the public Downloads folder. Intermediates stay in
app-private data/cache directories: the WebView has no standing access to
shared storage. The vault imports copies into app-private storage; it does
not watch live folders. Temporary files are cleaned up by the app; backups are
disabled (`allowBackup=false`).

**AI/privacy behavior.** AI is off until you enable it and configure a
provider; every request shows what is sent. API keys on Android use a
plaintext fallback (reported in the UI) that is excluded from backups.
Cleartext HTTP from the WebView is restricted to localhost/emulator hosts;
the Rust client can still reach a local-network model server (e.g. Ollama on
your LAN) because it uses its own TLS stack.

**Limitations.** No foreground service: a job keeps running only while the
process lives (state survives death and is marked interrupted). Android
cannot index arbitrary SAF folders; imports are the supported path. The
launcher label is "OmniOffice"; the package id is unchanged for upgrade
continuity.

## Sample documents

`samples/` contains documents generated by the suite itself (no personal
data): `test-document.docx`, `test-document.odt`, `test-document.rtf`,
`test-spreadsheet.xlsx`, `test-spreadsheet.ods`, `test-spreadsheet.csv`,
`test-presentation.pptx`, `test-presentation.odp`.
`crates/officecore/tests/fixtures/` contains the V3.1 golden `.oswk`
fixtures used by the cross-format contract tests.

Regenerate them with:

```bash
cargo run -p officecore --example make-office-samples
```

## Architecture

```
crates/officecore   Document model + DOCX/ODT/ODS/ODP/RTF/XLSX/CSV/PPTX engines,
                    sections/notes/revisions/fields, structured tables, charts,
                    XLSX import fidelity (charts/pictures/print/protection/pivots),
                    ODT/RTF notes, schema migrations, capability matrix,
                    PDF layout with an embedded OFL font, hardened ZIP/XML layers
crates/pdfcore      The PDF engine (render, merge, split, compress, OCR with
                    preprocessing, security, watermark, annotations, metadata,
                    page layout, sanitizer, PDF/A validation + font embedding +
                    ICC output intent, flattening, redaction with verification,
                    qpdf-backed repair/linearization (desktop), content-stream
                    text runs, RFC 3161 timestamps,
                    forms fill/validate, CMS/PKCS#7 signatures)
crates/aicore       Optional assistant client with a provider abstraction;
                    the only component that talks to the network for AI, and
                    only after the user opts in
crates/synccore     Local-first sync foundation: WebDAV client, Google Drive
                    v3 and Microsoft Graph providers, OAuth 2.0 PKCE
                    (loopback listener, refresh), per-file metadata,
                    three-way conflict detection, manual resolution
src-tauri           Tauri shell: PDF commands, office commands, PDF Studio
                    (sanitize/repair/flatten/PDF-A/signatures/forms), vault,
                    jobs store, plugins, sync, Windows certificate store,
                    Android intent handling, JSON stores, recovery
src/                React 19 + TypeScript + Tailwind 4 frontend
  src/office        Writer (paginated direct editing), Calc (formula engine,
                    tables, auditing), Impress, tool screens
  src/screens       PDF screens, PDF Studio, Vault, Compatibility, Jobs,
                    Plugins, Sync, Reader
  src/lib           Command registry, background jobs, office stores, i18n,
                    plugin runtime, mobile/SAF bridge, sync client
```

The document model (`crates/officecore/src/model.rs`) is the single source of
truth shared by the Rust engines and the TypeScript editors; the same engines
run on Windows and Android. File formats are import/export targets; the native
`.oswk` format preserves everything the suite understands. Every unit carries
`schemaVersion` and is migrated on open (`crates/officecore/src/schema.rs`).

## Build

Requirements: Node.js 20+, Rust 1.89+ (the dependency tree sets that floor:
`rust-version` in `Cargo.toml` is the single source of truth and a CI job pins
it), Visual Studio Build Tools (Windows).
Android additionally needs JDK 21, Android SDK (platform 36, build-tools 36)
and NDK 27.3.13750724.

### Windows

```bash
npm install
npm run engines:fetch      # pdfium, qpdf, tesseract, fonts (skipped if present)
npm run build              # type-check + frontend production build
npm run test:rust          # cargo test --workspace
npm run app:build          # Tauri release build (first run downloads NSIS)
npm run package            # installer + portable ZIP into release-artifacts/
```

The installer is written as both `OmniOffice-Setup-<version>.exe`
and `OmniOffice_<version>_x64-setup.exe`; `SHA256SUMS.txt` covers
both plus the portable ZIP.

`npm run release:local` runs the whole update loop in one command: Windows +
Android builds, `build-info.json`, CycloneDX SBOMs, checksums, and a per-user
install of the new build on this machine (no administrator rights needed) with
refreshed Start Menu / Desktop shortcuts. Add `-Publish` to create or update
the GitHub release for the current version. The per-machine copy in
`C:\Program Files\Office Swiss Army Knife` is only replaced by running the
installer elevated.

Every engine download is pinned by SHA-256 in `scripts/engines.lock.json` and
verified by the fetch scripts: a mismatch stops the build instead of shipping an
unverified binary. When an upstream release is updated on purpose, verify it and
re-run the fetch script with `-UpdateLock` to re-pin. Releases also publish
CycloneDX SBOMs (Rust and npm) next to the installer and carry a signed build
provenance attestation, verifiable with
`gh attestation verify <file> --repo ozanstn1-stack/omnioffice`.

### Android

```bash
npm run android:engines                                    # pdfium/tesseract per ABI
npm run android:build                                      # arm64-v8a debug/release flow
npm run android:build:all                                  # arm64-v8a + armeabi-v7a
# APKs and AABs (with -Bundle) land in release-artifacts/:
powershell -NoProfile -ExecutionPolicy Bypass -Command "& ./scripts/build-android.ps1 -Abi arm64-v8a,armeabi-v7a -Bundle"
```

Release signing reads `src-tauri/gen/android/keystore.properties` when
present (CI restores it from secrets) and falls back to the local debug
keystore for sideload builds. The Android version metadata is generated from
`tauri.conf.json` into `app/tauri.properties` on every build.

Development: `npm run app:dev` (desktop) · `npm run android:dev`.

## Tests

```bash
cargo test --workspace
npm test
npx tsc --noEmit
```

Beyond the unit/integration suites: `npm run e2e:smoke` drives the real
binary through tauri-driver (add `-- --pdf samples/sample-1.pdf` for the
Reader step; `npm run e2e:flows` adds the Writer save round trip, Merge and
Studio sanitizer flows; on Windows point `TAURI_NATIVE_DRIVER` at the
msedgedriver that matches your WebView2 runtime), `cargo +nightly fuzz run
<target>` from `fuzz/` runs the libFuzzer targets (Linux/macOS),
`cargo bench -p officecore --bench parse -p pdfcore --bench pdf_ops`
produces the criterion report, and `npm run bench:check` compares it against
the stored baseline (the CI bench job does this with a cached baseline and
fails on a >15 % mean regression).

**564 Rust tests** (3 heavy performance cases are `#[ignore]`d) and **690
frontend tests** pass, plus the 1 heavy case gated by `OSAK_PERF_HEAVY=1`,
with a strict TypeScript type check on top. Frontend coverage floors are
enforced by `npm run test:coverage` (see `vite.config.ts`). The per-crate split
is deliberately not repeated here - `cargo test --workspace` prints it, and
the numbers written out in prose went stale every release.

### What runs where

The Rust matrix builds and tests all three desktop platforms. Only the Windows
runner has the native engines (pdfium, qpdf, tesseract are Windows binaries), so
it runs the whole workspace suite; Linux and macOS run `cargo test -p officecore`
(all of its integration tests: golden fixtures, format round trips, the README
contract) plus the engine-free `--lib` tests of the other crates. Clippy runs
on Windows for the same reason, and a separate `msrv` job builds the library
crates with the declared `rust-version` so the declaration cannot drift again.

### Quality gates

The same gates run in CI (`.github/workflows/desktop.yml`), so a clean local
run means a clean pull request:

```bash
npm run lint               # ESLint (0 errors; the warning count is a frozen budget)
npm run i18n:audit -- --check   # en/tr tables must stay in sync
npm run format:gate        # Prettier on the files this change adds
npm run format:gate:rust   # rustfmt on the files this change adds
npm run lint:rust          # cargo clippy --workspace --all-targets -- -D warnings
npm run test:coverage      # Vitest + coverage floors (enforced)
```

Neither rustfmt nor Prettier had ever run over this code base, so the
formatting gate is staged: files **added** by a change must be formatted, and
the repository-wide backlog is printed but not enforced. Run `npm run format`
and `npm run fmt:rust` (one commit, no behaviour change) to clear the backlog
and then switch the gate to `--all-changed`.

Deliberate lint exceptions live in one place — `[workspace.lints]` in the root
`Cargo.toml` — each with the reason it exists.

Highlights:

- `officecore`: the ZIP container refuses bombs whose header lies about the
  uncompressed size, verifies every entry CRC, refuses duplicate entry names
  and reads the central directory in one pass; the XML parser caps elements per
  part; the legacy text decoder is Windows-1254 rather than a Latin-1 byte
  cast, so old Turkish files keep İ, ş, ğ and ı. DOCX round trips for sections,
  footnotes/endnotes, tracked
  changes, fields and comments; ODT/RTF note and revision round trips; PPTX
  V3 suite (masters, nested groups, charts with caches and embedded
  workbook); XLSX import fidelity (charts, pictures, print, protection,
  pivots); schema migrations (V2/V3 open, migrations idempotent, future
  schemas refused); **V3.1 golden fixtures** assert feature survival across
  `.oswk` and DOCX/XLSX/PPTX.
- `pdfcore`: sanitizer (every object walked), PDF/A validation/conversion
  with font embedding and ICC structure, flattening, redaction verification,
  OCR preprocessing (tesseract-guarded), **signature round trips (RSA/ECDSA,
  tamper detection, PFX passwords, ByteRange coverage)**, **forms
  fill/validate and object edits**, performance guards.
- `synccore`: metadata/state-machine matrix, WebDAV multistatus parsing,
  conditional PUT conflict mapping, keep-both naming; live network test is
  opt-in.
- `src-tauri`: vault indexing/search/import tests, job persistence
  (interrupted recovery), plugin install/sandbox guards, Android intent
  file handling, Windows store certificate listing.
- Frontend: paginated Writer editing (click-to-caret, Enter/Backspace/arrows
  across fragments), formula engine and auditing, pagination rules, P2P
  pointer interactions, data-loss gate flows, plugin permission enforcement,
  sync helpers, i18n parity and encoding.

The redaction, sanitizer and signature tests are the ones worth knowing
about: they re-open the produced file and prove the claims instead of
trusting a visual check.

## Privacy and security

- No cloud upload, no telemetry, no document content collection, no mandatory
  account. The vault only indexes folders you pick (Windows) or documents you
  import (Android). Cloud sync is off until configured and never overwrites
  silently.
- Only `aicore` (AI) and `synccore` (sync) perform network requests, and only
  after the user explicitly enables them; the AI activity line shows the
  provider and the character count being sent.
- **Transport security**: provider and WebDAV URLs must be `https://`. Plain
  `http://` is accepted only for loopback servers, and WebDAV requires an
  explicit opt-in for it; redirects to a public `http://` endpoint are
  refused, so Basic Auth credentials, Bearer tokens and document bytes never
  travel in cleartext to a remote host.
- **Supply chain**: engine binaries, fonts and language models are pinned by
  SHA-256 in `engines.lock.json`; a missing or mismatched hash fails the build
  (no warning-and-continue path), and CI verifies that the lock covers every
  download both fetch scripts can perform.
- **OS integration**: the WebView has no opener permission; opening or
  revealing a file goes through Rust commands that only accept existing
  documents with extensions the app itself produces, so a compromised
  renderer cannot ask the shell to launch an executable.
- Macros and embedded scripts are never executed; PDF JavaScript is never
  executed by forms; documents always open with macros disabled.
- ZIP extraction is bounded (entry count, size, compression ratio) to resist
  ZIP bombs; XML parsing is depth-limited and does not expand external
  entities; OOXML/ODF importers parse parts through those hardened layers.
- Writes are atomic (temp sibling + rename); passwords are never logged or
  persisted; API keys use DPAPI on Windows. PFX passwords and private keys
  are held in memory only during signing.
- The plugin runtime sandboxes plugin code in a Web Worker with no DOM, no
  Tauri IPC and host-enforced capabilities; plugin file access is scoped to a
  per-plugin directory and path traversal is rejected on both sides.
- Android: `allowBackup=false`, data-extraction rules exclude app data,
  FileProvider paths are narrowed, network security config restricts
  cleartext to localhost/emulator, intent input is validated (extension
  whitelist, size cap, sanitized names) and activities stay unexported except
  the launcher.
- PDF Studio sanitization removes JavaScript, launch actions and embedded
  files from a document and the result is verified with the inspector.
- Recovery snapshots and version history stay in the app data directory.

## Known limitations

These are real and honest:

- **Legacy `.doc`/`.ppt` import is text-level.** The built-in importer keeps
  text and paragraph/slide boundaries only (character formatting, tables,
  headers, footnotes, images, animations and themes are not reconstructed);
  a locally installed LibreOffice gives full fidelity when available. Legacy
  documents are never saved back as `.doc`/`.ppt`: the first save asks for
  `.docx`/`.pptx`/`.oswk`.
- **Track changes** tracks run-level insertions, deletions and formatting
  changes plus paragraph-level split/merge as text edits; paragraph *move*
  revisions and table/list structural revision objects are not modelled and
  DOCX `w:moveFrom/w:moveTo` is not written. Structural edits are applied
  directly in the model.
- **Writer paginated editing**: dragging a selection that starts on a static
  (non-active) fragment across *different blocks* is limited; the document
  layout itself is measured per reflow with caret persistence, but the full
  editing surface is activated per block rather than per page.
- **Bookmarks**: the model and PDF export resolve bookmarks; the DOCX writer
  does not yet emit `w:bookmarkStart/End` anchors, so REF targets rely on the
  cached field values in Word.
- **PDF/A**: Type0/CID fonts, symbolic fonts, custom encodings and fonts used
  only inside form appearances are skipped and reported; PT Sans fallbacks
  are not metric-compatible; there is no subsetting; the generated sRGB ICC
  profile is structurally valid but has not been run through an external
  validator, and no external veraPDF run is claimed.
- **Signatures**: trust is reported `unknown` (no network revocation/OCSP,
  no system trust store); encrypted PDFs must be decrypted first; RC2/RC4
  PFX files are rejected with a clear error; ECDSA is P-256 only; the
  `/Contents` placeholder holds up to 8 KB of DER. An RFC 3161 timestamp's
  `genTime` is read and reported, but the TSA certificate chain and its
  revocation status are not validated offline.
- **PDF object editing** covers annotations, form widgets and drawn images.
  **Text runs** inside content streams can be listed and their text replaced
  in place (the font and position stay fixed, so the replacement must fit the
  same encoding - composite fonts are refused); vector graphics are still
  read-only, and listed positions/widths are approximations without glyph
  metrics.
- **Forms**: JavaScript field formatting/validation is never executed; such
  fields are flagged but not evaluated. Non-WinAnsi characters fall back to
  `?` in generated Base14 appearances (the real `/V` keeps the string).
- **XLSX**: pivot caches are preserved and re-exported, not recomputed;
  unsupported chart kinds, secondary/combo axes and some conditional formats
  degrade with warnings; SVG export of sheets is not offered.
- **PDF repair / Fast Web View** run the bundled qpdf, which is a desktop
  engine: Android and engine-less builds report the tool as unavailable.
  Repair rewrites the file as qpdf reads it (encrypted documents need the
  password first); a file too damaged for qpdf is reported, not guessed at.
- **PPTX**: programmatic animations are simplified to what the model
  represents; ODP loses animations, groups and charts on export (declared in
  the compatibility matrix and gated by Data Loss Protection).
- **ODT/RTF**: RTF cannot mark endnotes distinctly (endnote-only documents
  request endnote placement via `\aendnotes`), revision timestamps lose
  seconds, and format-change revisions are not written to RTF.
- **Vault on Android** imports documents; it cannot watch SAF folders
  because the scanner uses real filesystem paths. Imported copies consume
  storage and are not deleted by `vault_clear`.
- **Cloud sync**: Google Drive and OneDrive need an OAuth client ID you create
  in the provider's developer console (installed/desktop app type); a live
  sign-in needs a browser and internet. There is no background polling, no
  auto-merge and no delete propagation. Conflict resolution is manual by
  design.
- **Plugins**: a Web Worker is not an OS/WASM sandbox - it protects documents
  and user data through the capability boundary, not against a WebView
  engine escape. Installation is folder-based (no zip), and `doc.applyEdits`
  works on whole runs/cells.
- **Android**: no foreground service (long jobs run only while the process
  lives; state survives death as `interrupted`), the intent pipeline has JVM
  unit tests (`./gradlew :app:testUniversalDebugUnitTest`) and on-device
  instrumentation tests (`./gradlew :app:connectedUniversalDebugAndroidTest`,
  real Android I/O on an emulator), both run by the Android release workflow;
  the picker UI itself (`ACTION_OPEN_DOCUMENT`) is still not automated,
  `osed/ospr/osdt` are accepted by the intent filter but the engine does not
  understand them yet, and the launcher label is "OmniOffice".
- **Reader zoom**: page previews are rasterized up to 3000 px wide (backend
  cap 4000) and cached per page (24 entries); zooming past what the bitmap
  covers on very high-density screens still upscales until tiled rendering is
  implemented.
- Interoperability with Microsoft Office/LibreOffice was validated
  structurally (package parts, content types, relationships, independent
  readers) plus headless LibreOffice conversion during development, not by
  launching those applications in CI.
- macOS/Linux desktop builds are produced and tested by CI; packaging is
  Windows-only here.

## Roadmap

Delivered in 3.1.0: direct page editing with cross-page caret; real CMS/
PKCS#7 signatures (Windows store + PKCS#12); PDF forms fill/validate and
annotation/image object editing; PDF/A font embedding and ICC output intent;
XLSX charts/pictures/print/protection/pivot fidelity; PPTX chart caches and
embedded workbook; ODT/RTF notes and revisions; sandboxed plugin runtime;
WebDAV sync with conflict detection; Android intents, SAF, vault import,
persistent jobs and touch UX; data-loss protection; golden-file and
performance contracts; release assets for Windows and Android.

Hardened in 3.1.1: HTTPS-only AI/WebDAV endpoints with a loopback exception
and downgrade-proof redirects, fully pinned engine downloads, a working
extension context menu, app-private Android storage, streaming WebDAV
transfers, lazy-loaded screens with a CI bundle budget, a per-rule lint
baseline and SHA-pinned CI actions.

Quality gates in 3.2.0: zero-warning lint with every React Compiler and
accessibility rule as an error, WebDAV end-to-end tests against an in-process
DAV server, on-device Android intent tests on an emulator, and owned,
time-boxed RustSec advisory exceptions in `deny.toml`.

Data integrity and gates in 3.5.5: per-sheet XLSX comments (C11),
run-preserving Writer tracked changes (M12), qpdf-backed repair and Fast Web
View, a benchmark regression gate with a cached baseline, deep desktop E2E
flows in CI, and a repository-wide formatting gate after the one-off burn-down.

Next (architecture prepared, not implemented):

- Paragraph move revisions and table/list structural tracked changes with
  DOCX `w:moveFrom`/`w:moveTo`
- PDF content-stream object editing (text/vector) with graphics-state-aware
  rewriting
- Font subsetting and CID remapping for PDF/A-2/3 with Type0 fonts
- OAuth cloud providers (OneDrive/Google Drive) and background sync
- Android foreground service for long-running jobs and share-sheet polish
- SmartArt and advanced PPTX effect import

## License

MIT. See [LICENSE](LICENSE). Third-party components keep their own licenses
(Tauri, React, Tailwind, lopdf, pdfium, qpdf, Tesseract, PT Sans/Liberation/
OFL, RustCrypto, …).
