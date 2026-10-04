# Roadmap

Living plan for the releases after 3.3.x. Each phase is independently
shippable; the detailed audit leftovers live in `AUDIT_REPORT.md` and the
honest status of the current build in `RELEASE_READINESS.md`.

Effort estimates assume a single developer.

| Phase | Scope | Status |
|---|---|---|
| 0 | Docs consistency, `compat.rs` freshness contract, frontend coverage baseline, roadmap | Done in 3.4.0 |
| 1 (v3.4) | Quick wins: command palette + keybindings, Settings completion, Home IA, a11y pass, dead ends | Done in 3.4.0 |
| 2 (v3.5) | Quality infrastructure: coverage thresholds, screen tests, desktop E2E, fuzzing, benchmark trends, CI hardening | Done in 3.5.0-3.5.5 (coverage gate + screen tests in 3.5.0; E2E, fuzzing, benchmark trends in 3.5.4; regression gate + deep flows + repo-wide format in 3.5.5) |
| 3 (v3.6) | PDF depth: content-stream object editing, render cache, Reader, Studio UX, PDF/A subsetting, signature chain (RFC 3161/OCSP) | Partial in 3.6.0: content-stream **text** editing, Reader bookmarks, RFC 3161 timestamps. Font subsetting, tiled rendering and OCSP/CRL remain |
| 4 (v3.7) | Office depth: Writer fields/revisions, Calc data tools + chart UI, Impress timeline + media, format fidelity | Planned |
| 5 (v4.x) | Platform: auto-update, macOS/Linux packaging, stores, Android foreground service + Keystore, sync OAuth, Vault 2.0, plugins, AI | Planned |

## 3.4.0 — Quick wins (delivered)

- **Command platform is real.** Every screen is a palette command; the
  registry drives the keybindings (`matchKeybinding`), the former hard-coded
  shortcut handler is gone, and `Ctrl+O` routes office documents to the
  workspace and everything else through the active screen.
- **Settings**: `midnight`/`paper` themes, office default formats, version
  history and import-warning toggles that actually reach the save/open paths,
  corrected shortcut list with an editor-shortcut hint.
- **Home**: the drop suggestion keeps every dropped file, the tool grid is
  searchable and grouped (all 42 screens reachable), recent PDFs open in the
  Reader, and the reveal button is a real labeled control.
- **Accessibility**: `aria-current` navigation, modal focus trap + focus
  restore, roving tabindex and arrow keys in segmented controls, live region
  for background jobs, `lang` follows the UI language, translated control
  labels, `prefers-reduced-motion` support.
- **Calc**: `CUMIPMT`/`CUMPRINC` implemented with Excel reference values.
- **Quality**: frontend coverage runs in CI; `officecore` has a contract test
  that keeps the Compatibility Center honest about DSS vs. RFC 3161.

## 3.4.0 — Not done (honest)

- Coverage is informational, not a blocking threshold yet. (Closed in 3.5.0.)
- Desktop E2E, fuzzing and benchmark trends are still Phase 2.
- Jobs "Retry" has no registered handler yet; `vault_clear` has no UI path.
- `AUDIT_REPORT.md` items not touched by 3.4.0 remain as listed there.

## 3.5.0 — Coverage gate and screen tests (delivered)

- `npm run test:coverage` enforces statement/branch/function/line floors
  (58/68/46/58) measured from the suite; the frontend CI job runs it.
- 24 new tests cover Merge, Split, Compress, Security, Watermark, Metadata,
  Info, History, Home, Settings, Notes, Planner, Data, Draw, Templates,
  Converter, Cleaner, PDF forms, AI Library, Jobs, Compatibility, OCR and
  PDF→images; statements moved 49.2 % → 60.7 %.
- Templates contract test for ids, metadata and model kind.

## 3.5.0 — Not done (honest)

- Desktop E2E (tauri-driver or a mocked-invoke browser suite) and fuzzing are
  still not started; benchmark trend tracking is not started either.
- The coverage floors keep a few points of headroom and should be raised as
  the suite grows.

## 3.5.1 — Reader zoom quality (delivered)

- Previews render in physical pixels (high-dpi + `max_width` cap) instead of a
  fixed 96 dpi, and the reader caches one bitmap per page (LRU 24) with reuse
  when the cached bitmap is already sharp enough.
- Pinch updates coalesce per frame and pages are memoized.
- Extreme zoom on very high-density screens still upscales until tiled
  rendering (Phase 3) lands.

## 3.5.2 — Live pinch (delivered)

- Two-finger gestures scale the page container with a CSS transform that
  follows the fingers 1:1 (including the two-finger pan) and commit the zoom
  to the layout on release.
- No inertial/momentum zoom; tiled rendering for extreme zoom is still open.

## 3.5.3 — Cleanup (delivered)

- Background jobs are retryable from their persisted payload (one handler per
  kind; credentials blanked; Jobs UI hides Retry when no handler exists).
- Vault Clear UI (optionally deletes the imported copies on Android).
- Writer field values and ordered-list numbering computed at render time.
- TOCTOU-safe `UniqueName` output reservation.
- Coverage floors 62/69/46/62; 38 frontend test files, 673 tests.
- Still open: C11 (XLSX cross-sheet comments), desktop E2E, fuzzing,
  benchmark trends, the red Dependabot majors.

## 3.5.4 — Quality infrastructure (delivered)

- Desktop E2E through tauri-driver on every PR (engine-free smoke on Linux;
  the Windows run also opens a PDF in the Reader).
- Fuzzing: ZIP reader, XML parser and the office reader dispatcher, seeded
  corpus, nightly job on master.
- Criterion benchmarks for office import and lossless PDF compression with a
  report artifact per master run.
- `cargo deny check` now enforces licenses, bans and sources as well.
- Still open: C11, benchmark thresholds/regression alerts, deeper E2E flows,
  the red Dependabot majors, and the Phase 3/4/5 roadmap.

## 3.5.5 — Data integrity and hard gates (delivered)

- **C11 fixed**: XLSX comments are written per sheet (own part, VML set,
  relationships and content-type overrides); two sheets on the same address no
  longer share a comment.
- **M12 fixed**: Writer tracked changes diff runs, keeping surrounding
  formatting/links/anchors, existing insertion ids, insertion cancellation and
  hidden pending deletions.
- **qpdf wired in**: PDF Studio Repair and Fast Web View (desktop only), with
  Jobs retry, real page counts and engine diagnostics.
- **Benchmark regression gate**: cached master baseline, >15 % mean
  regression fails the bench job, baseline advances only on success.
- **Deep E2E flows** in CI: Writer type → save, Merge, Studio sanitizer, with
  failure screenshots.
- **Formatting backlog burned down** in one no-behaviour commit; the gate now
  checks every changed file and the whole tree.
- Rust deps deduped (`cbc` 0.2/`des` 0.9, unused `aes` removed); the blocked
  majors are documented in `.github/dependabot.yml`.
## 3.5.6 — Home opens what you pick, Android converters (delivered)

- Home opens a single picked/dropped file on the right screen (office
  workspace, reader, image-to-PDF); the picker accepts every supported format
  and the Home page leads with the office suite.
- PDF → images: unique-name default plus a per-document output folder; verified
  on Windows including a rerun over existing files.
- Android: the app-private staging folder is created before native tool runs
  (fixes PDF merge and other exports), and the converter/cleaner use the SAF
  picker and publish their results.
## 3.5.7 — Legacy formats and OAuth sync (delivered)

- Word 97-2003 (`.doc`) and PowerPoint 97-2003 (`.ppt`) import: built-in CFB
  text extraction plus an optional LibreOffice conversion bridge; legacy tabs
  always save to a modern format.
- OAuth 2.0 PKCE Google Drive and OneDrive providers with loopback redirect,
  OS credential-vault token storage and the same conflict contract as WebDAV;
  Sync screen gains the client/connect panel.
- Universal converter handles PDF → JPG/PNG/TXT/DOCX and image → PDF.
## 3.7.0 — Renamed to OmniOffice (delivered)

- Product, installer, portable ZIP, Android artifacts, Chrome extension,
  per-user install path, shortcuts, About dialog and generated file metadata
  use the new name; the package identifier, `.oswk` format tag and OAuth
  keyring service stay stable so updates, documents and cloud tokens keep
  working.
- The updater removes the old per-user install folder and shortcuts; the AI
  library falls back to the legacy folder until a new one exists.
## 3.6.0 — PDF depth, first half (delivered)

- **Content-stream text editing**: `pdfcore::content` lists a page's text runs
  with graphics/text-state tracking and replaces a run's text in place as an
  incremental revision; the font must be able to represent the replacement
  (round-trip checked) and multi-string `TJ`/composite runs are read-only.
  Studio gains the "Text" tab.
- **Reader bookmarks**: the outline is parsed into `pdf_info` and shown in the
  Reader side panel.
- **RFC 3161 timestamps**: optional TSA URL in the signing dialog; the token
  is embedded as an unsigned attribute and its `genTime` is reported. HTTPS
  only (loopback HTTP allowed for a local TSA); a TSA failure fails signing.
- Still open from Phase 3: PDF/A font subsetting (Type0/CID), tiled rendering
  for extreme zoom, OCSP/CRL revocation, vector-object editing, and the
  coverage re-baseline that the vitest 4/5 instrumentation change requires. PDF → Word/Excel layout reconstruction is
  not planned for v3.6.
