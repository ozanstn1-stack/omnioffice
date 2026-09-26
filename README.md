# Office Swiss Army Knife

A local-first desktop productivity suite: a word processor (Writer), a
spreadsheet (Calc), a presentation editor (Impress), local productivity tools
(Notes, Planner, Data, Draw, Templates, PDF Forms, Document Vault) and the
complete PDF toolkit this project started from.

Everything runs on your machine. Documents are never uploaded, there is no
telemetry, AI is opt-in with your own provider, and the app stays useful
without an internet connection. Macros and embedded scripts in office files
are never executed.

**Version 3.0.0** · Platform: Windows (Tauri also targets Linux/macOS; the
desktop CI builds and tests all three, only Windows packaging is produced
here) · UI languages: English, Turkish.

## What's new in 3.0.0

V3.0 turns the suite from a set of editors into a document platform:

- **Writer is sectioned and reviewed.** Real sections (per-section page setup,
  first/even headers, section breaks), footnotes and endnotes with automatic
  numbering and a reserved note area, tracked changes (insertions, deletions
  and formatting changes) with accept/reject per change or in bulk, comments
  with replies, bookmarks and cross-reference fields (REF/PAGEREF/DATE/...).
  All of it round-trips through DOCX and renders into the exported PDF.
- **Calc understands tables and audits formulas.** Excel-style structured
  tables with headers, totals, banded rows, calculated columns, filters and
  structured references (`=SUM(Sales[Amount])`, `Sales[@Amount]`), formula
  autocomplete with argument hints, and trace-precedents / trace-dependents /
  circular-reference auditing on top of the dependency graph.
- **XLSX import keeps the file, not just the numbers.** A custom OOXML pass now
  reads styles, number formats, column widths, row heights, merges, freeze
  panes, data validation, conditional formatting, hyperlinks, comments, defined
  names and structured tables; tables are also written back as real
  `xl/tables/tableN.xml` parts.
- **Impress has masters, groups, charts and a slideshow that animates.**
  Slide masters and layouts with placeholder inheritance, real nested shape
  groups with group-level transforms, PPTX chart import/export, an animation
  model (entrance/emphasis/exit with triggers and timing) that actually runs in
  the slideshow, and a presenter view with next-slide preview, notes and timer.
- **PDF tools grew up.** A real sanitizer (JavaScript, embedded files, launch
  actions, unsafe annotations, metadata), annotation/form flattening, PDF/A
  validation for 1b/2b/3b with an honest converter that re-validates and never
  claims compliance it does not have, redaction verification (the output is
  re-read and checked), and working OCR preprocessing (deskew, denoise,
  threshold, contrast, orientation detection on the rendered page).
- **Document Vault.** Opt-in local indexing of folders you choose (office
  documents and PDFs), full-text/phrase/fuzzy search with filters, snippets
  with match locations and a text preview. Local only, crash-resistant index,
  incremental rescans.
- **Document platform.** A unified command registry with a command palette
  (`Ctrl+Shift+P`) and global search (`Ctrl+Shift+F`), a background job center
  with progress/cancel/retry, a capability/compatibility system that reports
  what each format supports *before* a save, and schema versioning with tested
  migrations for `.oswk` (V2.x documents open and are upgraded in memory).
- **AI is a document assistant, not a black box.** Provider abstraction
  (DeepSeek, OpenAI-compatible, Ollama, Gemini, custom) with capability flags,
  a per-document consent gate, "show what will be sent" activity line, send
  scope (whole document / current page / selection), and a document chat that
  answers with `[page N]` citations. Ollama runs entirely on your machine.

## Verified workflows

These were exercised on the built application and with automated tests:

- **Writer**: click anywhere on a page fragment → the caret lands at the
  clicked character and the editing surface opens there; typing updates the
  model; DOCX round-trip tests cover sections, notes, revisions, fields and
  comments; the PDF export is section-aware and draws notes on the page that
  references them.
- **Calc**: click a cell → type → `Enter` commits and moves on; `=SUM(Sales[Amount])`
  evaluates; editing a table cell recalculates its dependents; trace buttons
  highlight precedents and dependents; the XLSX round trip preserves layout,
  validation, conditional formatting, comments, links, names and tables.
- **Impress**: the sample PPTX loads with slides, shapes, images, tables,
  notes and transitions; masters/layouts round-trip; a nested group moves as
  one; the slideshow runs entrance/emphasis/exit animations and the presenter
  view shows the next slide and notes.
- **PDF**: every tool from v1.x is unchanged and still covered by its tests.
- **Redaction**: text under a redaction box is *deleted from the content
  stream*; the verification pass re-opens the output, re-extracts the text and
  reports any remaining matches (masked) instead of assuming success.
- **Sanitizer**: tests inject JavaScript, an OpenAction, an embedded file and
  an unsafe annotation, then walk every object in the output to prove they are
  gone, and Inspect confirms it independently.
- **PDF/A**: a document with unembedded fonts fails validation; conversion
  writes the XMP/output intent it can and still reports `valid: false` when the
  fonts remain unembedded.

## Modules

### Writer (word processor)
- DOCX, ODT, RTF, TXT, Markdown, HTML import/export · PDF export · lossless `.oswk`
- Styles with based-on/next inheritance, fonts, bold/italic/underline/strike,
  super/subscript, colour, highlight; alignment, spacing, indents
- Lists (bullet/numbered/multilevel), tables, images, hyperlinks, page breaks,
  horizontal rules, table of contents, navigation pane
- **Sections (V3)**: per-section page size/orientation/margins/columns, section
  breaks (new page/continuous/odd/even), default/first/even headers and footers
- **Notes (V3)**: footnotes and endnotes, automatic numbering by reference
  order, note area reserved at the bottom of the page in the paginated view and
  in PDF export
- **Track changes (V3)**: suggesting mode records typing and backspacing as
  revisions; review pane with per-change accept/reject, accept all/reject all,
  next/previous, show/hide; DOCX `w:ins`/`w:del`/`w:rPrChange` round trip
- **Comments (V3)**: anchored comments with replies and resolve
- **Fields and references (V3)**: bookmarks, cross references (REF/PAGEREF),
  page/page-count/date/time/title/author fields written as real Word fields
- Paginated view with real page containers, measured pagination (widow/orphan,
  keep-with-next, keep-together, page-break-before, repeated table headers) and
  click-to-caret positioning; continuous editing view
- Find & replace, word/character/page count, zoom, print, PDF export with
  selectable text

### Calc (spreadsheet)
- XLSX, ODS, CSV/TSV import/export · XLS import (read-only) · PDF export
- Virtualised grid, name box and formula bar, multi-sheet workbooks
- Formula engine with 160+ functions, `LET`, named ranges, inline arrays,
  dynamic arrays with spill, explicit errors and circular-reference detection
- Dependency graph with incremental recalculation
- **Formula autocomplete (V3)**: functions with signatures and descriptions,
  defined names, sheet names, table and column names, argument hints
- **Formula auditing (V3)**: trace precedents/dependents with coloured
  overlays, circular-reference and invalid-reference reporting
- **Structured tables (V3)**: create/rename/delete tables, header and totals
  rows, banded rows, calculated columns, filters and structured references
- **XLSX fidelity (V3)**: styles, number formats, widths, heights, merges,
  freeze panes, validations, conditional formatting, hyperlinks, comments,
  defined names and tables are read back; tables are exported as real table
  parts
- Cell formatting, number formats, sorting, filtering, conditional formatting,
  data validation, freeze panes, charts (column/bar/line/pie/area), pivot
  tables (computed live, exported as values)

### Impress (presentations)
- PPTX and ODP import/export · PDF export · lossless `.oswk`
- **Master slides (V3)**: masters with themes/backgrounds and layouts with
  placeholders inherited by slides; layout picker per slide
- **Grouped shapes (V3)**: real nested groups with group-level move/resize and
  Alt+click child selection
- **Charts (V3)**: ChartML import/export for column/bar/line/pie/area with
  titles, series, legend, axes, stacking and data labels
- **Animations (V3)**: entrance/emphasis/exit effects with triggers, duration
  and delay; the slideshow executes them; `<p:timing>` round trip
- **Presenter view (V3)**: current/next slide, notes, timer, navigation
- Eight layouts, six themes, transitions, full-screen slideshow, speaker notes

### PDF module
Reader with search, Merge, Split, Organize, Compress, OCR (Tesseract), Protect
(AES-256), Unlock, Watermark, Annotate, Metadata, Page tools, PDF → JPG/PNG,
JPG/PNG → PDF, Batch, Info, Redact, Compare, Inspect and the optional offline
AI assistant — all unchanged.

**PDF Studio (V3)**: Sanitize (JavaScript/attachments/actions/unsafe
annotations/metadata with a removal report), Flatten (annotation and form
appearances burned into the page), PDF/A-1b/2b/3b validation and honest
conversion with a re-validation pass.

**OCR (V3)**: preprocessing now actually runs on the rendered page —
orientation detection via Tesseract OSD on the raster (not the PDF), deskew,
denoise, threshold and contrast, reported in the OCR result.

### Document Vault
- Index folders you explicitly choose; nothing is scanned by default
- DOCX, ODT, RTF, TXT, Markdown, HTML, XLSX, ODS, CSV, PPTX, ODP and PDF
- Index keeps file name, path, type, dates, size, extracted text, headings and
  locations (page/paragraph/cell/slide)
- Full-text search with exact/phrase/fuzzy modes, extension/date/folder filters,
  snippets with `<</term/>>` highlighting and a preview panel
- Crash-resistant atomic index, incremental rescans, corrupted-index recovery

### Document platform
- Unified command registry feeding the palette, keyboard shortcuts and menus
- Command palette (`Ctrl+Shift+P`) and global search (`Ctrl+Shift+F`) over
  commands, recent files and the vault index
- Background job center with progress, cancellation and retry
- Compatibility Center: per-format capability matrix and a pre-save loss report
- `.oswk` schema versioning with migrations; documents from a newer schema are
  refused rather than misread
- Autosave with crash recovery, local version history (25 snapshots)

### AI assistant (opt-in)
- Providers: DeepSeek, OpenAI-compatible endpoints, Ollama (local), Gemini,
  custom — with capability flags (chat/embeddings/vision/structured/streaming)
- Per-document consent before any request; network activity line shows the
  provider and the character count being sent
- Send scope: whole document, current page or selected text
- Document chat with `[page N]` citations, summaries, translation, text
  cleanup, metadata suggestions
- API keys are stored with Windows DPAPI when available, never logged, never
  written into documents or version history

## Supported formats

Only combinations that actually work are marked. “–” means not supported.

| Format | Open | Edit | Save | PDF export |
|---|---|---|---|---|
| DOCX | ✓ | ✓ | ✓ | ✓ |
| DOC | – | – | – | – |
| ODT | ✓ | ✓ | ✓ | ✓ |
| RTF | ✓ | ✓ | ✓ (basic formatting, tables, images) | ✓ |
| TXT / Markdown / HTML | ✓ | ✓ | ✓ | ✓ (via Writer) |
| XLSX | ✓ | ✓ | ✓ | ✓ |
| XLS | ✓ | – | – | – |
| ODS | ✓ | ✓ | ✓ | ✓ |
| CSV / TSV | ✓ | ✓ | ✓ | – |
| PPTX | ✓ | ✓ | ✓ | ✓ |
| PPT | – | – | – | – |
| ODP | ✓ | ✓ | ✓ | ✓ |
| PDF | ✓ | ✓ (existing tools + PDF Studio) | ✓ | – |
| JPG / PNG / BMP / GIF / WebP | ✓ | ✓ (as images) | ✓ | ✓ (images → PDF) |
| SVG | ✓ (inserted as image) | ✓ | ✓ (media in DOCX/ODT) | ✗ (not rasterised) |
| `.oswk` unit | ✓ | ✓ | ✓ | ✓ |

## Sample documents

`samples/` contains documents generated by the suite itself (no personal data):
`test-document.docx`, `test-document.odt`, `test-document.rtf`,
`test-spreadsheet.xlsx`, `test-spreadsheet.ods`, `test-spreadsheet.csv`,
`test-presentation.pptx`, `test-presentation.odp`.

Regenerate them with:

```bash
cargo run -p officecore --example make-office-samples
```

## Architecture

```
crates/officecore   Document model + DOCX/ODT/ODS/ODP/RTF/XLSX/CSV/PPTX engines,
                    sections/notes/revisions/fields, structured tables,
                    schema migrations, capability matrix, PDF layout with an
                    embedded OFL font, hardened ZIP/XML layers
crates/pdfcore      The PDF engine (render, merge, split, compress, OCR with
                    preprocessing, security, watermark, annotations, metadata,
                    page layout, sanitizer, PDF/A validation, flattening,
                    redaction with verification)
crates/aicore       Optional assistant client with a provider abstraction;
                    the only component that talks to the network, and only
                    after the user opts in
src-tauri           Tauri shell: PDF commands, office commands, PDF Studio
                    commands, document vault, JSON stores, version history,
                    recovery, file associations
src/                React 19 + TypeScript + Tailwind 4 frontend
  src/office        Writer, Calc (formula engine, tables, auditing), Impress,
                    tool screens
  src/screens       PDF screens, Vault, Compatibility Center, Jobs, PDF Studio
  src/lib           Command registry, background jobs, office stores, i18n
```

The document model (`crates/officecore/src/model.rs`) is the single source of
truth shared by the Rust engines and the TypeScript editors. File formats are
import/export targets; the native `.oswk` format preserves everything the suite
understands, including features a given file format cannot represent. The
schema version is written into every unit and migrated on open
(`crates/officecore/src/schema.rs`).

## Build

Requirements: Node.js 20+, Rust 1.82+, Visual Studio Build Tools (Windows).

```bash
npm install
npm run engines:fetch      # pdfium, qpdf, tesseract and fonts (skipped if present)
npm run build              # type-check + frontend production build
npm run test:rust          # cargo test --workspace
npm run app:build          # Tauri release build (first run downloads NSIS)
npm run package            # NSIS installer + portable ZIP into release-artifacts/
```

Development: `npm run app:dev`.

## Tests

```bash
cargo test --workspace
npm test
npx tsc --noEmit
```

**322 Rust tests** (34 aicore, 120 officecore, 146 pdfcore, 22 src-tauri) and
**451 frontend tests** pass, with a strict TypeScript type check on top.

Highlights:

- `officecore`: DOCX round trips for sections, footnotes/endnotes, tracked
  changes, fields and comments; a 6-test PPTX V3 suite (masters, nested
  groups, charts, animations); XLSX import fidelity and table round trips;
  schema migration tests (V2 opens, migrations are idempotent, future schemas
  are refused, corrupt models are reported); compatibility reports; revision
  accept/reject rules.
- `pdfcore`: sanitizer (poisoned document, every object walked), PDF/A
  validation/conversion, flattening, redaction verification, OCR preprocessing
  (tesseract-guarded).
- `src-tauri`: vault indexing/search tests (incremental rescan, corrupted
  index recovery, folder permission rules).
- Frontend: formula engine (dependency graph, spill, structured references,
  auditing), pagination rules including sections and note reservation, writer
  runs/caret/revisions, editor component tests, i18n parity and encoding.

The redaction and sanitizer tests are the ones worth knowing about: they
re-open the produced file and prove the removed content is gone. A black
rectangle or a deleted key would pass a visual check and fail these.

## Privacy and security

- No cloud upload, no telemetry, no document content collection, no mandatory
  account. The vault only scans folders you pick.
- Only `aicore` performs network requests, and only after the user explicitly
  enables the assistant and confirms the send for the document.
- Macros and embedded scripts are never executed; documents always open with
  macros disabled.
- ZIP extraction is bounded (entry count, size, compression ratio) to resist
  ZIP bombs; XML parsing is depth-limited and does not expand external
  entities; OOXML/ODF importers parse parts through those hardened layers.
- Writes are atomic (temp sibling + rename); passwords are never logged or
  persisted; API keys use DPAPI when available.
- PDF Studio sanitization removes JavaScript, launch actions and embedded
  files from a document and the result is verified with the inspector.
- Recovery snapshots and version history stay in the app data directory.

## Known limitations

These are real and honest:

- **Writer paginated view**: typing happens on the continuous editing surface.
  Clicking a page fragment places the caret at the clicked character and opens
  that surface; it is not yet a WYSIWYG typing canvas, and a caret cannot be
  dragged across a page boundary while typing.
- **Track changes** tracks text-level insertions/deletions and formatting
  changes. Structural edits (paragraph splits/merges) are applied directly and
  are not recorded as revisions; paragraph move revisions are not modelled.
- **Notes** round-trip DOCX and render in the PDF export; the ODT, RTF and
  plain-text exports do not write note objects (TXT/Markdown/HTML append the
  note text at the end). The Compatibility Center reports this before saving.
- **Sections** export to DOCX with per-section page setup and headers; ODT and
  the text formats turn section breaks into page breaks.
- **XLSX import** reads the structures listed above, but charts, drawings,
  pivot caches, print settings and sheet protection are not imported back
  (they are kept in `.oswk`). Charts are export-only in both directions for
  import.
- **PPTX charts** are written as ChartML referencing cell ranges; the range
  values themselves are not embedded in the chart part, so other suites show
  an empty plot until the workbook is attached. All chart properties round-trip.
- **Impress masters**: layout/master decorative objects are composited behind
  slides; ODP keeps a single default master page.
- **Animations**: the built-in slideshow executes the effects the model stores;
  PowerPoint-specific effects are simplified on import with a warning.
- **PDF/A conversion** applies sanitization, XMP and an output intent, then
  re-validates. It does **not** embed missing fonts, so documents with
  unembedded fonts remain non-compliant and are reported as such.
- **Digital signatures** are not implemented: no signing, no validation. This
  is declared, not faked.
- **Plugin architecture** is not implemented yet; the command registry is the
  extension point that a plugin runtime will build on.
- **Cloud sync** is not implemented; `.oswk` conflict-safe sync is planned for
  V3.1.
- **Vault PDF indexing** needs the bundled pdfium engine; without it PDFs are
  indexed as metadata only and a warning is reported.
- Interoperability with Microsoft Office/LibreOffice was validated
  structurally (package parts, content types, relationships, an independent
  reader in development) rather than by launching those applications. The PPTX
  V3 output was converted with headless LibreOffice during development.
- macOS/Linux desktop builds are produced and tested by CI; packaging is
  Windows-only here.

## Roadmap

Delivered in 3.0.0: sections, footnotes/endnotes, track changes, comments,
fields and cross references; direct caret placement in the paginated view;
structured tables and formula auditing; XLSX import fidelity; master slides,
groups, PPTX charts, animations and presenter view; PDF sanitizer, PDF/A
validation, flattening, redaction verification and OCR preprocessing; the
Document Vault; the command platform, jobs, compatibility center and schema
migrations; the AI provider abstraction with document chat.

Planned for 3.1 (architecture prepared, not implemented):

- Direct typing inside page fragments with cross-page caret movement
- Paragraph-level tracked structural changes and move revisions
- Digital signatures (CMS/PKCS#7) with the Windows certificate store
- A sandboxed plugin runtime with manifest permissions
- WebDAV/OneDrive sync with conflict detection and manual resolution
- Native PDF pivot caches, ODP notes and animations, SmartArt import

## License

MIT. See [LICENSE](LICENSE). Third-party components keep their own licenses
(Tauri, React, Tailwind, lopdf, pdfium, qpdf, Tesseract, PT Sans/OFL, …).
