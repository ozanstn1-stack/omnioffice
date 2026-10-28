# OmniOffice

A local-first productivity suite: a word processor (Writer), a spreadsheet
(Calc), a presentation editor (Impress), local productivity tools (Notes,
Planner, Data, Draw, Templates, PDF Forms, Document Vault) and the complete
PDF toolkit this project started from — on **Windows and Android**.

Everything runs on your machine. Documents are never uploaded, there is no
telemetry, AI is opt-in with your own provider, cloud sync is off until you
configure it, and the app stays useful without an internet connection. Macros
and embedded scripts in office files are never executed.

**Version 6.4.6** · Platforms: Windows (Tauri also targets Linux/macOS; the
desktop CI builds and tests all three, only Windows packaging is produced
here) and Android (arm64-v8a, armeabi-v7a) · UI languages: English, Turkish.

## What's new in 6.2 (file tokens, strict mode, module splits)

6.1 and 6.2 are one continuous arc: user files are no longer handed to the
webview as raw paths, release builds enforce that boundary, and the office core
was split into per-format modules.

**Capability tokens (B3)**

- Every user file the UI touches is addressed by a random, scoped token
  (`FileRef`): read, read-write and write scopes, a 12-hour TTL and explicit
  revoke. The webview never receives a filesystem path.
- Covered surfaces: open/save dialogs, drag-and-drop and launch/association
  events, recents, the global picker and command palette, output targets and
  `suggest_output`, PDF Studio (PFX, watermark/annotate images), Invoices, Ai
  attachments, Eml, Scanner, Sync, MailMerge, Converter/Cleaner and the shared
  pickers.
- **Strict mode in release builds**: `input_file`/`output_file` reject raw
  paths in desktop release builds; Android SAF and debug/E2E keep documented
  exceptions (`docs/file-tokens.md`). The `addPaths` path mode was removed.

**Office core splits (B4)**

- `odf.rs` went from 4,346 to ~945 lines: ODT/ODS/ODP readers and writers now
  live in dedicated modules (`odf/text_read.rs`, `text_write.rs`,
  `sheets_read.rs`, `sheets_write.rs`, `slides_read*.rs`,
  `slides_write*.rs`).
- `xlsx.rs` (4,750 lines) was split the same way in 6.2.2: the reader/import and
  writer/export halves live under `xlsx/`, the largest file is 748 lines, and
  the ratchet dropped the single 4,750-line entry.
- Writer/Impress/Calc editor seams: watermark/image options and
  selection/animation helpers moved out of the multi-thousand-line editors into
  `office/writer/objects.tsx`, `office/impress/selection.ts` and
  `office/calc/gestures.ts`; the file-size ratchet drops with every split.
- The Writer view layer moved out the same way in 6.2.3: the static page
  renderers live in `office/writer/views.tsx` and the table surface in
  `office/writer/table-view.tsx`; `WriterEditor.tsx` is 3,246 lines (was 4,745).
- Impress followed in 6.2.4: the footer/master/chart/animation dialogs and the
  slide renderers live in `office/impress/dialogs.tsx` and
  `office/impress/slide-view.tsx`; `ImpressEditor.tsx` is 2,795 lines (was
  4,493).
- `docx.rs` (3,621 lines) followed in 6.2.5: the writer (`docx/writer.rs`, 1,523)
  and reader (`docx/reader.rs`, 1,326) halves are separate modules and the unit
  tests live in `docx_tests.rs` (757).
- `pptx.rs` (3,271 lines) completed the per-format pass in 6.2.6: the writer
  (`pptx/writer.rs`, 1,643), the reader (`pptx/reader.rs`, 1,115) and the 17
  unit tests (`pptx_tests.rs`, 493) are separate files.

**Also in this window**

- Fuzz fix: calamine cell-reference overflow patched in `vendor/calamine`
  (checked arithmetic) with the crash input added to the corpus.
- Security: `tokio 1.54.1` (RUSTSEC-2026-0384), `ctutils 0.4.5`, and a
  time-boxed `crypto-bigint` exception in `deny.toml`; `cargo audit` green.
- CI: npm audit gates production dependencies only; the Desktop, Rust and
  Android matrices stay green.
- `tauri-specta` scaffold (B2/2) landed in 6.2.1; the command-by-command
  migration continues in 6.2.x.
- **Android open-with**: documents shared from WhatsApp or a download manager
  open directly when the sender grants read access - accepted by MIME type and
  content, not by the sender's file name - and legacy `.xls` opens too. A sender
  that grants nothing is reported instead of failing silently. See
  `docs/android.md`.
- **Open-with diagnostics**: Settings -> Diagnostics copies the on-device
  pipeline trace (queue, intent log, drain/route events) when a shared document
  still does not open; failed opens now show a reason instead of a blank screen.
- **Android office documents open in-process** (an app cannot exec a worker
  binary there), which fixes the `office-open parser failed` error on devices.

- **WhatsApp "Open with" (6.2.10)**: the cold-start queue drain is no longer
  dropped and a sender's `CLEAR_TASK` flag no longer closes the app (an
  invisible `OpenWithActivity` receives the intent and copies it while it holds
  the read grant). If WhatsApp hands over the URI without a read grant, the app
  now says so instead of staying on Home.
- **Share on Android** (6.2.10): Writer/Calc/Impress share the open document
  (own format or PDF, unsaved edits included) and the reader shares the PDF
  through the system share sheet.

- **Phone fixes** (6.2.11): the Turkish UI no longer mixes in English,
  Writer tables keep their rows, the page opens fitted to a phone's width, and
  office dialogs open over the editor.

- **Save shared documents** (6.3.0): a document opened from WhatsApp asks on
  its first save whether to save it to Downloads, save and share it back, or
  pick a location; later saves go to the same place.

- **Recent and direct share** (6.3.1): PDFs opened from WhatsApp appear under
  Recent, and Share sends the app's own file without leaving a copy in
  Downloads.

- **Camera scanning** (6.3.2): Scan takes page photos with the phone's camera,
  lets you drag the four corners onto the page and builds an upright,
  multi-page PDF; no Google services involved.

- **Encrypted signatures** (6.3.3): saved signatures and paraphs are sealed
  with Windows DPAPI or the Android Keystore instead of plain PNG files.

- **Profession desks** (6.4.0): Home asks what you do and leads with six to
  eight tasks for lawyers, accountants, the public sector or students; all
  tools stay one tap away.
- **Pre-send check** (6.4.1): before a PDF is shared the app lists personal
  data, notes, attachments, scripts, metadata and signature problems, and can
  send a cleaned copy instead.
- **Notification deadline calculator** (6.4.2): the last day of a court
  period from an e-Tebligat (7/a, holidays, judicial recess), to the Planner
  or a `.ics` file.
- **Period close audit** (6.4.3): out-of-period and duplicate invoices,
  sales numbering gaps, bad VKN/TCKN and the VAT figures against the
  declaration, exported to Excel.
- **Phone ergonomics** (6.4.4): a bottom bar within thumb reach, 44 px
  touch targets and no zoom when a field is tapped.
- **Typed IPC and Android clippy** (6.4.5): the vault commands call generated
  typed wrappers, Android-only lints now fail the build, the Calc editor's
  undo/redo and gestures live in their own hooks, and PDF Studio and Document
  health pick files as capability tokens.
- **Technical debt, the rest** (6.4.6): the Calc editor's hook split is
  finished, sync and plugin commands are typed end to end, and lopdf moves
  to 0.46.
**Downloads (v6.4.6)** — Windows Setup/Portable, Android APK/AAB and SHA-256
manifests: [Release v6.4.6](https://github.com/ozanstn1-stack/omnioffice/releases/tag/v6.4.6)
· [Setup .exe](https://github.com/ozanstn1-stack/omnioffice/releases/download/v6.4.6/OmniOffice-Setup-6.4.6.exe)
· [Portable .zip](https://github.com/ozanstn1-stack/omnioffice/releases/download/v6.4.6/OmniOffice-Portable-6.4.6.zip)
· [APK arm64](https://github.com/ozanstn1-stack/omnioffice/releases/download/v6.4.6/OmniOffice-Android-6.4.6-arm64-v8a.apk)
· [APK armv7](https://github.com/ozanstn1-stack/omnioffice/releases/download/v6.4.6/OmniOffice-Android-6.4.6-armeabi-v7a.apk)

## Release history

Every release's notes live in [CHANGELOG.md](CHANGELOG.md). The older
"What's new" sections used to be duplicated here; the changelog is the single
source, and this README keeps only the latest release above.

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
- Formula engine with 278 functions, `LET`, named ranges, inline arrays,
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
- **Calc file model (V4.4)**: scatter and doughnut charts, color scales, data
  bars, icon sets and formula rules, and cell notes and hyperlinks as editable
  data (note author and visibility, link display text, screen tip, internal
  references) round-trip through `.oswk`, XLSX and ODS; links are limited to
  http, https, mailto and internal targets
- **Calc editing (V4.4)**: the conditional-format dialog adds color scales,
  data bars, icon sets and custom-formula rules; cell notes (right-click or
  Shift+F2) and hyperlinks (Ctrl+K; Ctrl+click follows one, `HYPERLINK()`
  cells included); `#CALC!` and the other error values are marked and
  explained in the grid
- Cell formatting, sorting, filtering, conditional formatting, data
  validation, freeze panes, charts (column/bar/line/pie/doughnut/area/scatter),
  pivot tables
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
- API keys use Windows DPAPI on desktop and the Android Keystore (AES-256-GCM)
  on Android; if no platform cipher is available the app reports that keys are
  stored unprotected. Android excludes app data from device backups. Keys are
  never logged, never written into documents, versions or crash reports.

## Supported formats

Only combinations that actually work are marked. “–” means not supported.

| Format | Open | Edit | Save | PDF export |
|---|---|---|---|---|
| DOCX | ✓ | ✓ | ✓ | ✓ |
| DOCM / DOTX / XLSM / PPTM | ✓ | ✓ | ✓ | ✓ (macros are detected, never executed and dropped on save) |
| DOC / DOT | ✓ (text import; save as .docx/.oswk) | ✓ (in memory) | – | ✓ (via Writer) |
| ODT | ✓ | ✓ | ✓ | ✓ |
| UDF (experimental; hidden until real UYAP files verify it) | ✓ (via flag) | ✓ (via flag) | ✓ (via flag) | ✓ (via flag) |
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
minSdk 24, targetSdk 36. The version name and code are generated from
`tauri.conf.json` into `src-tauri/gen/android/app/tauri.properties` on every
build (`versionCode = major*1e6 + minor*1e3 + patch`).

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

**Limitations.** Long jobs keep running through a `dataSync` foreground
service (a notification is shown; Android 13+ asks for the notification
permission once), and a process death still brings a job back as
`interrupted` to retry. Android cannot index arbitrary SAF folders; imports
are the supported path. The launcher label is "OmniOffice"; the package id is
unchanged for upgrade continuity.

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

Measured on `master` (2026-10-25): **158 frontend test files and 2,117 Vitest
cases pass** (1 skipped), on top of a strict TypeScript type check; coverage
floors are enforced by `npm run test:coverage` (see `vite.config.ts`).
`cargo test --workspace` is the authority for the Rust suite and prints the
per-crate split. The generated block at the end of this file counts
`#[test]`/`#[tokio::test]` attributes in the sources; that is a source metric,
not the number of cases a given run executes, so it must not be read as "tests
passed". Three heavy performance cases are `#[ignore]`d and one more is gated by
`OSAK_PERF_HEAVY=1`. Prose numbers went stale every release, so the generated
block stays the single source for source counts.

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
- **Legacy `.xls` import is values-only.** The same `calamine` pass used for
  XLSX reads the cached cell values of a BIFF workbook; formula records,
  formatting, comments and layout are not reconstructed, and a locally
  installed LibreOffice is used for full fidelity when available. `.xls` files
  are never saved back: the first save asks for `.xlsx`/`.oswk`.
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
- **PDF/A**: embedded fonts are subset to the characters the document uses.
  Type0/CID fonts without a ToUnicode map, symbolic fonts, custom encodings
  and fonts used only inside form appearances are skipped and reported; PT
  Sans fallbacks are not metric-compatible; the generated sRGB ICC
  profile is structurally valid but has not been run through an external
  validator, and no external veraPDF run is claimed.
- **Signatures**: trust is reported `unknown` (there is no system trust
  store; revocation is only asked of the OCSP responder or CRL when the
  optional setting is on); encrypted PDFs must be decrypted first; RC2/RC4
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
  unsupported chart kinds (radar, bubble, stock, surface), secondary/combo
  axes, scatter series with their own X values and Excel 2010 data-bar and
  icon-set extensions (negative colours, custom icons) degrade with warnings;
  hyperlinks other than http, https, mailto and internal references (file:,
  javascript:, network paths) are dropped on import and export; SVG export of
  sheets is not offered. Hidden rows and columns round-trip in XLSX and ODS
  (also the rows an AutoFilter hides), but the editor keeps no size for a
  hidden row or column, so showing it again gives the default size, and a sheet
  that hides all unused rows with `zeroHeight` opens with them visible.
  Dynamic-array formulas (`FILTER`, `SORT`, `HSTACK`, ...) are written with
  Excel's `_xlfn.` names but without spill metadata, so Excel shows them as
  ordinary formulas and does not spill them.
- **PDF repair / Fast Web View.** Fast Web View needs qpdf, a desktop engine
  (bundled on Windows, a system install on Linux/macOS): Android and builds
  without qpdf report it as unavailable. Repair uses qpdf when present and
  otherwise the built-in rebuild (so it works on Android too); encrypted
  documents need the password first, and the result says which engine ran.
- **PDF annotations** are real annotation objects (editable, listed, movable,
  deletable) and can also be flattened into the page as before. Deleting an
  annotation removes it from the page but leaves the unreferenced object in the
  file, so signed documents stay byte-stable. Keyword matching folds case
  character-by-character (Turkish `İ` maps to `i`); regex matches are capped at
  10,000 per document and patterns over 2,000 characters are refused.
- **N-up and booklets** rebuild the page tree from the selected pages, so
  links, named destinations and outlines that pointed at the old pages are
  dropped, and source annotations are not repainted onto the sheets. The old
  page objects are left unreferenced.
- **PPTX**: programmatic animations are simplified to what the model
  represents; ODP keeps groups, animations, real charts, footer/date/
  slide-number frames, hidden slides, connectors and image crops (since
  4.5.0) but writes a single default master page (declared in the
  compatibility matrix and gated by Data Loss Protection).
- **Writer PDF export** lays out custom tab stops (center/right/decimal
  stops are approximated by advancing to the stop) and colspan/rowspan table
  cells on the grid; a text watermark is added as an incremental PDF
  revision. Image wrap is block-level in PDF (square/top-and-bottom wrap
  round-trip in DOCX and ODT). RTF keeps no merged cells, custom tab stops
  or watermarks, and Markdown/TXT drop image data.
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
- **In-app updates**: the desktop updater verifies signed update manifests
  from the project's GitHub releases (Windows; the installer still warns
  because the app is not Authenticode-signed). Android downloads the APK from
  the release, checks its SHA-256 and opens the system installer. When no
  signed manifest/keys are configured the app falls back to opening the
  release page.
- **Plugins**: a Web Worker is not an OS/WASM sandbox - it protects documents
  and user data through the capability boundary, not against a WebView
  engine escape. Installation is folder-based (no zip), and `doc.applyEdits`
  works on whole runs/cells.
- **Android**: long jobs keep running via a `dataSync` foreground service
  (since 4.3.0), and a process death still resumes them as `interrupted`; the
  intent pipeline has JVM unit tests
  (`./gradlew :app:testUniversalDebugUnitTest`) and on-device
  instrumentation tests (`./gradlew :app:connectedUniversalDebugAndroidTest`,
  real Android I/O on an emulator), both run by the Android release workflow;
  the picker UI itself (`ACTION_OPEN_DOCUMENT`) is still not automated,
  `osed/ospr/osdt` are accepted by the intent filter but the engine does not
  understand them yet, and the launcher label is "OmniOffice".
- **Reader zoom**: page previews are rasterized up to 3000 px wide (backend
  cap 4000) and cached per page (24 entries); past what one bitmap covers the
  visible region is drawn as tiles at full resolution (since 4.1.0), so text
  stays sharp up to 400 %.
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
- CID remapping for PDF/A-2/3 Type0 fonts without a ToUnicode map (embedded
  fonts are already subset)
- Background sync for the OAuth cloud providers (OneDrive/Google Drive)
- SmartArt and advanced PPTX effect import

## Project statistics

<!-- stats:start -->
| Project statistics | Count |
| --- | --- |
| Screens | 51 |
| Tauri commands | 225 |
| Rust tests | 1298 |
| TypeScript test files | 187 |
| TypeScript test cases (approx.) | 2267 |
<!-- stats:end -->

## License

MIT. See [LICENSE](LICENSE). Third-party components keep their own licenses
(Tauri, React, Tailwind, lopdf, pdfium, qpdf, Tesseract, PT Sans/Liberation/
OFL, RustCrypto, …).
