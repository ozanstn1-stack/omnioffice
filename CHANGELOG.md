# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [6.4.7] - Tipli IPC: yapay zekâ ve ofis

### Changed (internal)

- **Typed AI commands (#316, group 4):** the 31 AI commands (summaries,
  translation, chat, tables, batch runs, the in-editor actions and the AI
  library) are called through generated tauri-specta wrappers whose types
  come from the Rust structs. The long jobs stay on the job tracker so the
  Jobs screen can still re-run them.
- **Typed office commands (#316, group 5):** the 43 office commands
  (documents, conversion, e-invoices, KEP mail, the scanner, mail merge, the
  spell checker, history and recovery, the compatibility reports) go the same
  way. The document models stay opaque JSON.
- The generated wrappers live in their own chunk, preloaded with the
  language before the first render (`src/lib/typed-call.ts`), so the entry
  chunk stays under its 60 KB budget and a call is as direct as `invoke`.

### Fixed

- **PDF Studio: moving, resizing, deleting and rotating objects now works
  (#477).** The Objects panel's edits never reached the document: Rust
  expected the action nested one level deeper than the screen sends it and
  refused every request before any change was made.
- Typing the AI results against Rust surfaced three mismatches the
  hand-written types hid: a temperature that arrives as `null` (NaN) falls
  back to its default, a saved chat message's role is narrowed when a thread
  is loaded, and a chat thread saved without a document sends `null` for its
  path.
## [6.4.6] - Teknik borç: kalanlar

### Changed (internal)

- **Calc editor hooks done (#415, #339):** the list-validation dropdown
  (`useListDropdown`) and the formula draft and assist (`useFormulaDraft`)
  left the controller, behaviour unchanged. With undo/redo and the grid
  gestures from 6.4.5, `CalcEditor.tsx` went from 3,052 to 2,541 lines.
- **Typed sync, OAuth and plugin commands (#316, groups 2-3):** the sync
  screen and the plugin transport call generated tauri-specta wrappers
  whose types come from the Rust structs; their hand-written wrappers left
  `api.ts`. Vault, sync and plugin are typed; ai, app, office and pdf
  remain.
- **Command palette off the entry budget:** the palette's vault search
  calls `invoke` with the generated types instead of loading the typed
  wrappers, so the entry chunk is back under its 60 KB budget (59.5 KB).

### Dependencies

- lopdf 0.46 (its parser now rebuilds a missing cross-reference table; the
  repair test checks that qpdf did the repair instead of relying on lopdf
  failing), uuid 1.29, zustand 5.0.16, lucide-react 1.61 and @types/chrome
  0.3.6 in the browser extension.
- const-oid 0.10 is held with the der 0.8 family (Dependabot ignore rule):
  it drops the `db` module the signing code takes its OIDs from.
## [6.4.5] - Teknik borÃ§: tipti IPC, hook'lar ve jetonlar

### Added

- **Typed vault commands (#316, group 1):** the nine vault commands go
  through generated tauri-specta wrappers; the Rust side emits the bindings
  behind a `bindings` feature, so shipped builds are unchanged, and
  `src/lib/generated/specta.ts` is checked for drift in CI.

### Changed

- **IPC registrations in domains (#338):** the single 220-entry
  `generate_handler!` becomes seven domain lists in `src-tauri/src/ipc.rs`,
  with a contract test that every declared command is registered exactly
  once.
- **Calc editor hooks (#415, slices 1â€“2):** the undo/redo stacks move to
  `useCalcUndoRedo` and the grid's pointer, touch and pinch code to
  `useGridGestures`, with the same behaviour.
- **E2E waits on a screen-ready signal (#407):** flows wait for
  `<html data-screen-ready>` instead of counting buttons, so a screen that
  loads slowly no longer fails them.
- **Token pickers (#337):** PDF Studio and Document health pick files through
  capability tokens; the raw-path fallbacks are gone.
- **Android-only clippy (#461):** `android.yml` runs clippy for
  `aarch64-linux-android`, so Android-only lints fail the build; the first
  run removed dead code in android-only modules.
- **Entry bundle cap 60 â†’ 61 KB** for the typed wrappers the command palette
  needs at startup; the total cap stays 490 KB.

### Fixed

- **Calc column resize (#457):** dragging a column header grip resized
  nothing (the width went to `colWidths["NaN"]`); it now reads
  `data-col-resize` and resizes the column.
## [6.4.4] - Mobil ergonomi

### Added

- **Bottom bar on phones (C11, #341):** Home, Office, Read, Scan and More sit
  at the bottom of the screen within thumb reach; More opens the full menu.
  The tab a screen belongs to is highlighted (Documents, Spreadsheets and
  Presentations light up Office). The bar steps aside while the on-screen
  keyboard is up and keeps clear of the system gesture area.

### Changed

- **Larger touch targets:** on a touch screen, buttons and icon buttons are
  at least 44 px tall (small buttons 40 px) and icon buttons 44 px wide.
- **No zoom on focus:** text fields and drop-downs use 16 px text on a touch
  screen, so tapping one no longer zooms the page.

## [6.4.3] - Mali müşavir dönem denetimi

### Added

- **Period close audit (C4, #341):** Invoices → "Period audit" checks a
  month's e-invoices before the VAT declaration: invoices dated outside the
  period, duplicates (same ETTN, same supplier and number, or same supplier,
  date and amount), gaps in the numbering of your own sales series (a
  missing or cancelled invoice), VKN/TCKN numbers that fail their check
  digit, invoices that are neither a sale nor a purchase of your VKN, and
  the sales base, calculated VAT and deductible VAT against the figures
  typed from the declaration (more than 1 TL apart is an error).
- **Audit workbook:** "Export to Excel" writes the summary with the
  comparison, the findings and every invoice of the period with its side to
  an `.xlsx` file.

## [6.4.2] - Tebligat süre hesaplayıcı

### Added

- **Notification deadline calculator (C2, #341):** a new screen that gives
  the last day of a court period. An e-notice is served at the end of the
  fifth day after it reaches the electronic address (Tebligat Kanunu 7/a);
  the period runs from the next day, weeks and months end on the matching
  day (HMK 92), a period ending in the judicial recess moves to 7 September
  (HMK 104), and a last day on a weekend or an official holiday moves to the
  next working day (HMK 93). Every step is listed, a half-day eve and a year
  outside the holiday table are flagged.
- **Dates from an e-Tebligat PDF:** drop or pick the PDF and the arrival and
  service dates in its text are offered; the lawyer confirms one.
- **Planner and calendar:** the last day goes to the Planner or into a
  `.ics` file with a reminder two days before.
- Presets for the common periods (2 weeks for HMK appeal/cassation/answer,
  7 days for an İİK objection, 30 and 60 days for tax and administrative
  actions) and any custom length.
- The lawyer's desk links the calculator instead of the plain Planner.

## [6.4.1] - Gönderim öncesi kontrol

### Added

- **Pre-send check (C10, #341):** sharing a PDF first reads the real file and
  lists what it carries that the sender may not mean to send: valid TCKNs,
  IBANs, phone numbers and e-mail addresses in the text, notes and hidden
  annotations, attached files, JavaScript, author and software names in the
  properties, fillable fields, signatures that no longer verify or were
  changed after signing, and whether it is PDF/A. The worst findings come
  first; the personal data itself is never shown, only how much of each kind.
- **Clean and send:** metadata, attachments and scripts are removed into a
  copy that is shared instead; the original stays as it is. It is not offered
  for a signed PDF, where rewriting the file would break the signature.
- **Redact:** with personal data found, one button opens the redaction tool
  on that file.
- The check runs from the Reader, the office Share (as PDF), the tool result
  cards and Recent files on Android, and from the Reader on desktop.
  **Settings → "Check PDFs before sharing"** turns it off.

## [6.4.0] - Meslek masaları

### Added

- **Profession desks on Home (C1, #341):** the first time Home opens it asks
  "What do you do?" (lawyer, accountant, public sector, student, or all
  tools). The answer puts a desk of six to eight tasks at the top of Home:
  a petition, an evidence bundle, KVKK redaction and KEP messages for a
  lawyer; the invoice inbox with its VAT summary, the period workbook and
  client letters for an accountant; official letters, templates and PDF/A
  for the public sector; scanning, homework, slides and upload-size PDFs for
  a student. Every task opens a screen that already exists.
- **"All tools"** keeps the full catalogue one tap away while a desk leads
  Home; search still covers every tool.
- **Settings → Home desk** changes the choice; "Change desk" on Home does
  the same.

## [6.3.3] - İmzalar şifreli saklanıyor

### Security

- **Saved signatures are encrypted (#443):** signatures and paraphs saved in
  Annotate used to be plain PNGs in the app folder, readable by any backup or
  program running as the user. They are now sealed like the API keys: DPAPI
  on Windows, the Android Keystore on Android. Existing signatures are
  converted the first time the library opens; the plain PNG is removed only
  after the encrypted copy reads back.

### Changed

- A signature that can no longer be decrypted (Android: the Keystore key is
  lost with a re-install) is skipped instead of failing the whole list; its
  file is kept (#443).

## [6.3.2] - Kamerayla belge tarama

### Added

- **Camera document scanning (#441):** the Scan screen (and the Android Scan
  shortcut) takes page photos with the phone's camera app and opens each one
  in a new four-corner picker: drag the corners onto the page, or keep the
  whole photo. The result is a multi-page PDF that can go straight to OCR.
  ML Kit's scanner was not used: it needs Google Play services and brings
  Google's usage logging along.

### Fixed

- Page photos taken in portrait no longer land sideways in the PDF: every page
  is turned upright from its EXIF orientation first (#441).
- The camera button could not find a camera app on Android 11+ and opened the
  file picker instead; the manifest now queries `IMAGE_CAPTURE` (#441).
- Scanned pages are stored as JPEG instead of PNG, so a 12 MP page no longer
  makes a PDF of tens of MB (#441).
- Old scanner page photos and the camera's leftover captures are removed at
  start (#441).

### Changed

- The scanner is no longer behind a feature flag; its labels are translated
  (#441).

## [6.3.1] - Son kullanılanlar ve doğrudan paylaşım

### Added

- **Shared PDFs appear under Recent (#439):** a PDF or image opened from
  WhatsApp or another app is listed under Home's Recent as soon as it
  arrives; office documents already were, once they opened.

### Changed

- **Share sends the app's own file (#439):** Share on a tool result, a Recent
  entry or in History hands the file to the Android share sheet straight
  from the app's storage. A file not published in the current session (every
  Recent entry) used to be copied to `Downloads/OmniOffice` first, leaving
  another copy there on each tap.
- Android shows a share icon instead of a folder icon on those buttons (#439).

### Fixed

- History opens office documents in the editor instead of handing them to
  another app (#439).

## [6.3.0] - Paylaşılan belgeyi kaydet ve geri gönder

### Added

- **Saving a document opened from another app (#430):** the first Save of a
  document shared from WhatsApp, mail or a download manager asks where the
  changes go - Save to Downloads (`Downloads/OmniOffice`), Save and share
  (then the system share sheet with the saved file) or Choose a location
  (keeping the document's format). Later saves go to the same place without
  asking.

### Changed

- A save that lands in Downloads names the folder in its confirmation
  instead of only "Saved" (#430).

## [6.2.11] - Telefon düzeltmeleri: dil, Writer tabloları ve düzeni, diyaloglar

### Fixed

- **Mixed-language UI (#428):** with Turkish selected, the drawer, screen
  titles and toasts pushed by App stayed in English. `useT` kept a `ready`
  flag that English had already set, so components mounted before the
  settings picked Turkish never re-rendered when its dictionary landed.
- **Writer tables collapsed into one row (#428):** unmerged cells arrive with
  `rowspan` 0 and `<td rowspan="0">` spans every remaining row, so the later
  rows were laid out to the right of the first ("P e n" in a sliver column).
- **Writer page cut off on phones (#428):** the page now opens fitted to the
  width below 700 px (with a "Fit width" button), and a page wider than the
  window keeps its left edge reachable.
- **Office dialogs rendered inside the page (#428):** the office `Dialog`
  classes had no styles; unsaved-changes, version history and table dialogs
  now open over the editor.
- The editors' status line shows the file name on Android instead of the
  app's private cache path (#428).
- Old open-with and share copies (older than 14 days) are removed from the
  cache at startup; the open-with diagnostics log starts fresh with each app
  version and the report lists the frontend trace first (#428).

## [6.2.10] - WhatsApp'tan "Birlikte aç" ve Paylaş

### Fixed

- **"Open with" from WhatsApp left the app on Home (#424):** on a cold start
  the Kotlin shell copied the document and queued it, and the webview's first
  queue drain took it - but the settings finished loading while that drain was
  in flight, App passed `useAndroidPendingOpen` a new handler and the effect
  cleanup dropped the drained path. Rust had already truncated the queue, so
  the document was lost. The poll now starts once and every drained path
  reaches the current handler (reproduced and verified with adb on a Pixel 8
  Pro, Android 17).
- **An open-with intent with `FLAG_ACTIVITY_CLEAR_TASK` closed the app (#424):**
  Android recreated `MainActivity`, the generated `WryActivity.onDestroy` ended
  the process, and the next start opened the leftover document as a duplicate
  tab. Document intents now arrive at `OpenWithActivity`, an invisible
  trampoline in its own task that copies the streams while it holds the
  sender's read grant and wakes `MainActivity` with `NEW_TASK` only.
- The open-with diagnostics log no longer fills with an entry per empty 2.5 s
  poll; it keeps the first drain of the process and every drain that returned
  documents.

### Added

- **Share on Android (#424):** the office workspace (Writer, Calc, Impress)
  shares the open document as its own format (DOCX/XLSX/PPTX/ODT/...; legacy
  `.doc`/`.xls` and `.oswk` go out as DOCX/XLSX/PPTX) or as a PDF, including
  unsaved edits; the PDF reader shares the open PDF. Files go to the system
  share sheet (WhatsApp, mail, Drive, Quick Share, ...) straight from private
  storage through the app's FileProvider, without a copy in Downloads.

## [6.2.9] - Android'de belgeler süreç içinde açılıyor

### Fixed

- **Android office/invoice/EML/repair jobs failed in the worker:** `run()`
  spawned `std::env::current_exe()` with `--worker`; on Android that path is
  the `app_process` launcher, so the child "started", exited immediately with
  an empty stderr and every document failed with "the office-open parser
  failed" (named by the v6.2.8 toast). Android now parses these jobs
  in-process, still wrapped in `catch_unwind` so a malformed document is a
  parse error and never takes the app down; desktop keeps the worker process
  and its timeout/memory containment.

## [6.2.8] - Android açma tanılaması ve görünür açma hatası

### Added

- **In-app open-with diagnostics (#420):** Settings -> Diagnostics shows the
  open-with pipeline as it ran on the device - the Rust cache directory and
  pending queue, the Kotlin log of received intents, copied documents and
  queued paths, and the webview's drain/route/error trace - with a copy
  button, because the "shared document does not open" report came from a
  phone without adb.
- The queue drain appends a `drain count=N cache=<dir>` line per poll, so a
  report can tell "the webview never polls" from "the queue was empty".

### Fixed

- **The Android queue-drain commands are async:** Tauri runs synchronous
  commands on the main thread while `app.path()` is a mobile-plugin round trip
  back to the main thread, so the drain could stall and the webview never
  receive a shared document (the reported "share opens the app, stays on
  Home"). The diagnostics trace now shows whether that happened.
- **A failed office open is visible:** `openOfficePath` used to swallow the
  error after switching to the Office screen, leaving an empty workspace; the
  failure now raises a toast with the reason and is recorded in the report.

## [6.2.7] - Android open-with ve XLSX fuzz düzeltmesi

### Fixed

- **Android open-with no longer depends on the sender's file name (#417):**
  a document shared from WhatsApp or a download manager is accepted by its
  display name, then its MIME type, then its bytes: the copy is sniffed for
  PDF, OOXML, ODF, legacy Office, RTF, EML, XML and text, renamed to the
  detected type and opened on the right screen. Files whose type cannot be
  determined are deleted and reported with a specific message (too large,
  unreadable/no access, unknown type) instead of one generic "unsupported".
  `adb logcat -s PdfSakIntent` shows the received intent and the copy outcome.
- **Legacy Excel (.xls) opens on Android (#416):** the typed intent filter
  accepts `application/vnd.ms-excel` (plus the `msexcel`/`x-msexcel` aliases)
  and both name-based fallbacks list `.xls`; the copy pipeline accepts it and
  the editor treats it as a legacy document - the cached values are imported,
  the first save asks for `.xlsx`/`.oswk`, and the original file is never
  overwritten. The reader says so explicitly instead of implying formulas
  were imported.
- **XLSX dimension underflow (nightly fuzz, #418):** a worksheet with a
  reversed `<dimension ref="O1:C4"/>` made the vendored `calamine` subtract
  past zero; reversed references are normalized to their bounding box (and the
  table header/totals offsets use saturating arithmetic). The crashing input
  is part of the fuzz corpus, with a regression test.

## [6.2.6] - PPTX okuyucu/yazıcı ayrıldı (PPTX reader and writer modules)

### Changed

- **PPTX split (#339 / PR #413):** `crates/officecore/src/pptx.rs` (3,271
  lines) became `crates/officecore/src/pptx/` - `writer.rs` (1,643 lines) and
  `reader.rs` (1,115 lines), with `mod.rs` re-exporting the same public API
  (`DeckRead`, `DeckWrite`, `write_pptx_package`, `write_pptx`,
  `write_pptx_file`, `read_pptx`, `read_pptx_file`), and the 17 inline unit
  tests moved verbatim to `pptx_tests.rs` (493 lines). Pure move: no behaviour
  change, and the ratchet dropped the 3,271-line entry for the new modules
  without raising a budget.

## [6.2.5] - DOCX okuyucu/yazıcı ayrıldı (DOCX reader and writer modules)

### Changed

- **DOCX split (#339 / PR #411):** `crates/officecore/src/docx.rs` (3,621
  lines) became `crates/officecore/src/docx/` - `writer.rs` (1,523 lines) and
  `reader.rs` (1,326 lines), with `mod.rs` re-exporting the same public API
  (`DocxRead`, `write_docx`, `write_docx_file`, `read_docx`, `read_docx_file`),
  and the inline unit tests moved verbatim to `docx_tests.rs` (757 lines). Pure
  move: no behaviour change, and the ratchet dropped the 3,621-line entry for
  the new modules without raising a budget.

## [6.2.4] - Impress editör dikişi ve E2E dayanıklılığı

### Changed

- **Impress editor seam (#339 / PR #409):** the footer/master/chart/animation
  dialogs, the slide object renderers, the selection handles, the object tree
  and the slide preview moved out of `src/office/ImpressEditor.tsx` into
  `src/office/impress/dialogs.tsx` (803 lines) and
  `src/office/impress/slide-view.tsx` (816 lines); a small
  `src/office/impress/shared.ts` (168 lines) holds the theme and chart values
  both halves need, so the module graph stays acyclic. Pure extraction: all 37
  public exports are unchanged, no behaviour change, the editor dropped from
  4,493 to 2,795 lines and the existing tests stayed green.
- **E2E deep-flow resilience (#407 / PR #408):** the WebView2 deep flows now
  relaunch once with a fresh session when the dev launch context misses, the
  dev-route deadline grew from 15 s to 45 s, and a navigation fallback fails
  fast with the accurate reason. This removes the flake that blocked the v6.2.3
  release tag even though every other job was green.

## [6.2.3] - Writer görünüm katmanı ayrıldı (writer view modules)

### Changed

- **Writer editor seam (#339 / PR #404):** the static page renderers, the
  paginated paragraph, the table surface and the compare helpers moved out of
  `src/office/WriterEditor.tsx` into `src/office/writer/views.tsx` (1,166
  lines) and `src/office/writer/table-view.tsx` (363 lines). Pure extraction:
  the public exports are unchanged, no behaviour change, the full Vitest suite
  stays green, and the editor dropped from 4,745 to 3,246 lines. The file-size
  ratchet fell accordingly.

### Fixed

- **Hostile RTF parameters (nightly fuzz):** a control word with an unbounded
  digit run overflowed the `i64` accumulator and aborted the reader in
  overflow-checked builds (`crates/officecore/src/rtf.rs`). Parameters now
  saturate and clamp to the `i32` range, the crashing input is part of the fuzz
  corpus and a regression test covers it.

## [6.2.2] - XLSX bölmesi, bağımlılık turu ve ölçülmüş istatistikler

### Changed

- **XLSX split (#339 / PR #402):** `crates/officecore/src/xlsx.rs` (4,750
  lines) became `crates/officecore/src/xlsx/` with thirteen modules
  (`reader`/`import`, `sheet_read`, `conditional_read`, `drawings_read`,
  `pivot_read`, `styles_read`, `writer`, `sheet_write`, `chart_write`,
  `conditional_write`, `styles_write`, `mod`). Pure move: public API unchanged,
  the 19 XLSX unit tests still run, and the file-size ratchet dropped the
  4,750-line entry for ten tracked modules without raising a budget. Largest
  resulting file: 748 lines.
- **Dependency pass (#399, #387):** `crc32fast` 1.5.3 plus the grouped frontend
  updates (`@tauri-apps/*`, React 19.3.1, lucide-react 1.61, jsdom 30.1.3,
  prettier 3.9.10, Vite 6.4.5). Desktop, Rust and Android jobs green.
- **Honest statistics (PR #401):** the README stopped repeating stale prose
  test counts. It now states the measured Vitest numbers (158 files / 2,117
  cases) and defines the generated Rust count (1,269 `#[test]` attributes) as a
  source metric rather than a run result.

### Docs

- Homepage refreshed for the 6.1/6.2 releases — capability tokens, strict mode
  and the module splits (PR #400).

## [6.2.1] - tauri-specta iskeleti (bindings scaffold)

### Added

- **B2/2 scaffold (#316 / PR #397):** the tauri-specta RC trio is pinned behind
  the `bindings` Cargo feature; `specta_scaffold` exports one typed command to
  `src/lib/generated/specta.ts` via `npm run bindings:specta`. Production
  registration and builds are unchanged; command groups migrate one PR at a
  time (policy in `docs/dependencies.md`).

## [6.2.0] - B4 tamamlandı: formatlar ve editör dikişleri

### Changed

- **B4 complete (#339):** every office format now has its own reader/writer
  modules (ODT, ODS, ODP; `odf.rs` 4346 -> 945 lines before the ODP writer
  split, then the ODP writer moved out) and the editors have their first seams:
  `office/writer/objects.tsx` (image/watermark panels), `office/impress/selection.ts`
  (paths/groups/animation helpers) and `office/calc/gestures.ts` (pointer hit
  testing). The file-size ratchet fell with each slice.

## [6.1.15] - Writer nesne panelleri ayrıldı (writer object panels split)

### Changed

- **Writer editor seam (#339 / PR #392):** the image and watermark panels and
  the watermark overlay moved to `src/office/writer/objects.tsx`;
  `WriterEditor.tsx` dropped by ~176 lines.

## [6.1.14] - ODP yazıcı ayrıldı (ODP writer split)

### Changed

- **ODP export modules (#339 / PR #390):** effects/timing helpers moved to
  `odf/slides_write.rs`, frames/shapes to `odf/slides_write_objects.rs` and the
  package writer to `odf/slides_write_file.rs`. `odf.rs` fell from 1771 to 945
  lines; every office format now has its own reader/writer modules.

## [6.1.13] - ODP okuyucu ayrıldı (ODP reader split)

### Changed

- **ODP import modules (#339 / PR #388):** shapes and text frames moved to
  `odf/slides_read.rs`, timing/footers/package reading to
  `odf/slides_read_timing.rs` (both under the 400-line new-file limit).
  `odf.rs` fell from 2281 to 1771 lines and the baseline dropped.

## [6.1.12] - ODS yazıcı ayrıldı (ODS writer split)

### Changed

- **ODS export module (#339 / PR #385):** `write_ods`/`write_ods_file` and the
  chart local-table type moved to `odf/sheets_write.rs`. `odf.rs` fell from
  2505 to 2282 lines and the file-size baseline dropped again.

## [6.1.11] - ODS okuyucu ayrıldı (ODS reader split)

### Changed

- **ODS import module (#339 / PR #383):** the Calc reader (sheets, cells,
  charts, conditional formats, links) moved to `odf/sheets_read.rs`. `odf.rs`
  fell from 3418 to 2506 lines and the file-size baseline dropped again.

## [6.1.10] - ODT okuyucu ayrıldı (ODT reader split)

### Changed

- **ODT import module (#339 / PR #380):** the ODT reader (content, styles,
  notes, frames, watermarks) moved from `odf.rs` into `odf/text_read.rs`,
  mirroring the writer split. `odf.rs` dropped from 4346 to 3418 lines and the
  file-size baseline fell.
- **CI:** the npm audit gate now checks production dependencies only; the
  jsdom/undici dev advisory has no fixed release and is tracked by Dependabot.

## [6.1.9] - Katı dosya jetonları (strict file tokens)

### Changed

- **Desktop release builds only accept capability tokens (#337 / PR #378):**
  `paths::input_file` / `paths::output_file` reject raw paths with a clear
  error; Android's SAF bridge and debug/E2E builds keep the documented raw-path
  exception. MailMerge picks tokens and shows the picked name. Remaining
  persisted-folder exceptions (Vault, AI library/settings) are documented in
  `docs/file-tokens.md`.

## [6.1.8] - Kalan diyaloglar jetonlaştı (remaining dialogs tokenized)

### Changed

- **Save/export dialogs on the remaining screens (#337 / PR #375):** Invoices
  (PDF/XLSX/CSV), Ai (CSV/PDF/Markdown), AiLibrary, Redact, Petition/KVKK,
  Scanner, Sync download, Recipes, DocumentHealth and pdf-forms now use the
  Rust save dialog and send a write token; the shared DropZone picker and the
  Recipes input use document tokens. `fileBaseName` resolves a remembered token
  name so existing display call sites show the real name. The office save dialog
  is seeded with the file name. Remaining input pickers are part 2c; strict
  mode is 4d-d.

## [6.1.7] - Ofis yazma jetonları (office write tokens)

### Changed

- **In-place office saves use a document write token (#337 / PR #371):**
  `office_open_document` returns a write token for the opened file (issued in
  the app process); the tab stores it beside the real path and Ctrl+S sends the
  token. Legacy imports get none.
- **Office save/export dialogs and workspace pickers issue tokens (#337 / PR
  #372):** save-as and PDF export use the Rust save dialog (`pickSaveToken`),
  and the workspace open pickers use `pickDocumentToken`. The remaining screen
  dialogs and strict mode are the next slices.

## [6.1.6] - Dizin jetonları (directory tokens)

### Changed

- **Folder outputs use directory tokens (#337 / PR #368):** the native folder
  dialog registers the folder as a read-write token (`issue_dir`), and
  `paths::directory` resolves tokens like the file validators. The OutputBar
  folder Browse stores the token beside the display path; Split, Convert and
  Batch send it to the multi-output commands. The OutputBar **file** Browse now
  also uses the Rust save dialog token (the v6.1.4 wiring fix). On Android the
  native folder dialog stays SAF-based (desktop-only command).

## [6.1.5] - ODT yazıcısı ayrıldı (ODT writer split)

### Changed

- **B4 functional split (#339 / PR #366):** the ODT package writer (styles,
  notes, tables, media, package parts) moved from `odf.rs` to
  `odf/text_write.rs`; the reader and the ODS/ODP halves stay. `odf.rs` drops
  from 6208 to 4346 lines, and the file-size ratchet records the lower budget
  (shared helpers are `pub(crate)`, public API unchanged).

## [6.1.4] - Çıktı hedefleri jetonları (output target tokens)

### Changed

- **Desktop output targets issue write tokens (#337 / PR #363):** `suggest_output`
  returns the write token plus the display path (a configured output folder is
  validated in Rust). The OutputBar shows the path read-only and its Browse
  button opens the Rust save dialog; single-output commands receive a token,
  never a typed path. Multi-output folders and Android SAF staging keep paths.
  Strict mode (rejecting every raw path) and the B4 functional splits are next.

## [6.1.3] - Global seçici ve son dosyalar jetonları (picker and recents tokens)

### Changed

- **Global open picker and recent files issue tokens (#337 / PR #361):** the
  command palette / Ctrl+O dialog opens through the Rust token picker and routes
  by display name. `recent.json` still stores real paths, but `load_recent`
  returns a fresh opaque token and the display name per entry, so recents
  survive a restart without exposing paths; `add_recent` resolves a token before
  storing. Home, History, the search palette and Sync display names and open by
  token. Output targets, strict mode and the B4 functional splits are next.

## [6.1.2] - Açılış ve sürükle-bırak jetonları (launch and drop tokens)

### Changed

- **Launch/association and window drops issue tokens (#337 / PR #359):** the
  launch commands and the window drop handler register the paths in Rust and
  return/emit `FileRef` tokens only; `file_names` resolves display names without
  a path, and the App routes by name. The webview event is still used for the
  drag highlight (it carries no paths). Outputs, the global picker and the
  recents store still use paths, in the next slices.

## [6.1.1] - Jetonlarla araç seçicileri (token-based tool pickers)

### Changed

- **Tool file pickers issue capability tokens (#337 / PR #356):** every tool's
  "Add" dialog opens on the Rust side and returns an opaque token plus the
  display name; `useTool` stores the token, releases it when a file is removed,
  and the reader, batch list and password prompt show the name instead of a
  path. Drop, launch, recents and the global picker still deliver paths (dual
  mode), and output targets remain paths - the next slice migrates them.

## [6.1.0] - Yetki jetonları, ilk dilim (capability tokens, slice 1)

### Added

- **File capability tokens (#337 / PR #353):** `src-tauri/src/files.rs` issues
  random, scoped (read / read-write), expiring tokens for paths the user picked
  through a Rust-side dialog. `file_token_pick`, `file_token_save` and
  `file_token_release` are the only commands that learn a user path, and
  `FileRef` serializes the token and display name only. The frontend wrapper is
  `src/lib/file-tokens.ts`.

### Scope (honest)

Document commands still accept raw paths; moving the PDF, office and tool
commands onto `files::resolve` is the next slice (tracked in #337).

## [6.0.4] - Personalar ve sağlam E2E (personas and a stable E2E)

### Added

- **Persona E2E journeys (#340 / PR #351):** lawyer (KVKK redaction of a
  synthetic TCKN/IBAN/e-mail/phone document through the real detector and
  redaction commands), public (Official Letter Check) and student (Turkish
  spell check, suggestion click, save). The accountant flow from 6.0.2 stays.
  Engine-free personas run on Linux; the lawyer journey runs in the
  engine-enabled Windows job.
- **E2E stability (#350):** WebDriver calls wait up to 150 s, `/status` up to
  60 s, `POST /session` is retried once, and the app is reaped between
  scenarios (an unsaved-changes dialog used to block the next launch).

### Changed

- The Redact screen's development auto-run scans before redacting; the pure
  keyword/match helpers moved to `src/lib/redact-rows.ts`, and
  `samples/redaction-sample.pdf` comes from its own generator example.

## [6.0.3] - Bağımlılıklar ve araç parçaları (dependency pass and tool chunks)

### Changed

- **Paired Tauri plugin upgrades (#345):** updater 2.14, fs 2.7.1, dialog
  2.8.3, opener 2.7.2, single-instance 2.5.4 and android-fs 30.2.0 moved
  together with their npm packages (the version-sync gate rejects one-sided
  bumps); TypeScript 6.0.3, typescript-eslint 8.71.2, pbkdf2 0.13.
- **Tool screens are separate lazy chunks (#328 / PR #346):** Notes, Planner,
  Data, Draw, Templates, Converter, Cleaner and PDF Forms moved out of one
  1712-line module into `src/office/tools/*`; the 19.9 KB `ToolsScreens` chunk
  is gone and each tool route now loads 1.1-3.3 KB (templates 10.3 KB).

## [6.0.2] - Android indirmeleri (Open with for downloaded documents)

### Fixed

- **Android "Open with" for downloaded docx/xlsx (#342 / PR #343):** Chrome's
  download provider hands a file over as a `content://` URI without an
  extension and an `application/octet-stream` type, so none of the intent
  filters matched and OmniOffice was missing from the chooser. The typed filter
  now accepts the generic type (the display name is still checked against the
  extension whitelist), legacy/macro/template formats (`doc`, `dot`, `docm`,
  `dotx`, `xlsm`, `pptm`, `ppt`) are associated end to end, a refused file
  shows a message instead of doing nothing, and provider display names lose
  control characters so the line-based open queue cannot be split.

## [6.0.1] - Durdur, doğrula, dürüst ol (Stop, verify, be honest)

The v6.0.0 audit's A-phase. The release process became a real gate, the two
areas that could not be verified were closed instead of shipped, and the
money, document and parser paths were fixed.

### Added

- **Release channels (#296 / PR #297):** stable installers follow
  `releases/latest`, beta installers (built from master with a
  `X.Y.Z-beta.N` version) follow the rolling `beta` prerelease; Settings
  shows the installed channel.
- **Verifiable redaction certificate (#298 / PR #299):** SHA-256 digests of
  the input and the certified output, the item list (page/category/rule,
  never a value) and an optional RFC 3161 timestamp, embedded in the redacted
  PDF as an attached file and XMP metadata; the Reader verifies it and shows
  a file changed after certification in red.
- **Crash-proof parsers (#300 / PR #301):** docx/xlsx/pptx/odt/ods/odp/UDF/
  RTF/UBL/EML imports and PDF repair run in a worker process with time,
  output and (on Windows) memory limits; a malformed file can no longer take
  the application down.
- **Own tax id and VAT split (#302 / PR #303):** invoices are split into
  sales (calculated VAT) and purchases (deductible VAT) by your VKN/TCKN,
  with a strict period selector in the KDV helper.
- **Spell checker completeness (#304 / PR #305):** capitalized words are
  checked against proper-noun lists, the `-ken` suffix and curly apostrophes
  are recognized, and the checker runs off the UI thread with a word cache.

### Changed

- **UDF and vendor accounting exports are closed by default.** The UYAP `.udf`
  format and the Luca/Logo/Zirve CSV exports are hidden behind the
  off-by-default `experimentalUdf` and `accountingVendorExport` flags until a
  real UYAP file and a real import template verify them. Every user-facing
  entry point (open/save dialogs, converter, compatibility center, petition
  wizard, Windows file association, Android intent filters) is closed; the
  engine, tests and the fuzz target stay in the build, and README /
  RELEASE_READINESS say exactly that. Developers can re-enable either area
  with `flag:experimentalUdf=1` / `flag:accountingVendorExport=1`. (#275 / PR #276)
- **Release gate (#277 / PR #278):** tags are created only after the Desktop
  workflow for the same commit succeeds; the release workflow no longer
  overwrites published assets; the claim audit scans every cycle section and
  verifies each claim against the passing tests in CI.
- **Honesty cleanup (#294 / PR #295):** the document-health and KVKK screens
  are marked experimental and hidden behind flags, and the README dropped its
  44 duplicated "What's new" sections.
- **Routed the v5.3-v6.0 screens (#279 / PR #280):** the petition wizard,
  KVKK wizard and document-health screens are reachable; the launcher-shortcut
  token bug is fixed.

### Fixed

- **Invoice math (#281 / PR #282):** excise tax and withholding no longer
  pollute the KDV summary, %18/%8 land in their brackets, `declaredVatAmount`
  is computed, and the duplicate check no longer collapses unrelated invoices
  into an empty key.
- **AI consent, vault and certificate honesty (#285 / PR #286):** AI requests
  require consent, the certificate no longer fabricates hashes and never
  leaks the redacted value.
- **Android sharing (#287 / PR #288):** `.eml` and `.xml` shares match the
  typed intent filter, and the scanner hands off to OCR.
- **Writer proofing (#283 / PR #284):** the Turkish spell check and official
  letter panels are wired into the Writer; wrapped paragraphs emit a single
  tagged-PDF structure element; non-ASCII titles/alt text use UTF-16BE.
- **CI leftovers (#289 / PR #290):** fuzz targets hardened, runner images
  pinned, `turkish`/`synccore` covered in the test and release crate lists.

## [6.0.0] - Gerçek mobil (True mobile)

A major release bringing first-class mobile capabilities to Android, featuring advanced document scanner page reordering and batch clearing, seamless launcher shortcuts and ACTION_SEND sharing intents for emails and e-invoices, and robust Storage Access Framework (SAF) integration validated with Android emulator launch smoke tests.

### Added

- **Document scanner enhancements (#266 / PR #269):** Page reordering (move up / down) and 1-click batch clearing in the Android document scanner interface, with full unit test coverage.
- **Android shortcuts and ACTION_SEND intent sharing (#267 / PR #270):** Launcher shortcuts and deep ACTION_SEND / ACTION_VIEW integration routing `.eml` and `.xml` files directly to e-posta and e-fatura previewers.
- **SAF compatibility and Android emulator smoke test (#268 / PR #271):** Storage Access Framework (SAF) MIME mapping for XML and EML, verified document export/sharing, and expanded Android launch smoke test asserting MainActivity runtime status.

## [5.5.0] - Muhasebe köprüsü (Accounting bridge)

A minor release introducing accounting export bridges for Luca, Logo, and Zirve, interactive Turkish KDV1 VAT declaration helper summary tables with rate & withholding breakdowns, and automated duplicate invoice detection with 1-click workspace deduplication.

### Added

- **Accounting export bridges (#259 / PR #262):** One-click export of UBL-TR invoices into standard exchange CSV formats for Turkey's leading accounting platforms: Luca, Logo, and Zirve.
- **KDV declaration helper table & modal (#260 / PR #263):** Interactive KDV1 tax declaration helper breaking down taxable bases and calculated taxes across standard rates (%20, %10, %1, %0) and tevkifat withholdings with periodic date filtering.
- **Duplicate invoice detection and deduplication (#261 / PR #264):** Automatic detection of duplicate invoices by ETTN UUID, supplier taxId + invoice number clash, or exact amount & date match, with a 1-click deduplication tool to clean the workspace.

## [5.4.0] - Belge sağlığı ve KVKK paketi (Document health and KVKK compliance)

A minor release introducing one-click document health analysis and repair, verifiable PDF redaction certificates with SHA-256 integrity stamps, and a KVKK (Turkish Personal Data Protection Law) compliance wizard with customizable templates.

### Added

- **One-click document health analyzer and repair (#252 / PR #255):** Document health assessment checking uncompressed assets, missing metadata, un-redacted sensitive data, and structural compliance with 1-click automatic fixes.
- **Verifiable redaction certificate (#253 / PR #256):** Generate audit-compliant PDF redaction certificates with cryptographic SHA-256 digests, operator sign-off, and appendix attachments.
- **KVKK compliance wizard & templates (#254 / PR #257):** Step-by-step Turkish PDPL (KVKK) disclosure statement generator supporting multiple scenarios (customer, employee, visitor, CCTV) with export to PDF, DOCX, and UDF.

## [5.3.0] - Türkçe yazım ve resmî yazı denetimi (Turkish spell & official letter)

A minor release introducing fully offline Turkish spell checking backed by Zemberek NLP root dictionary, Official Letter Regulation compliance checker with 1-click fixes, and a Petition Wizard with TCKN mod-11 validation producing UDF and PDF outputs.

### Added

- **Offline Turkish spell checking (`crates/turkish`, #245 / PR #248):** Zemberek root lexicon + frequency list (Apache-2.0), suffix morphology analysis (vowel harmony, consonant assimilation/softening), Damerau-Levenshtein suggestions, Tauri IPC commands (`turkish_spellcheck`, `turkish_suggestions`), and Writer wavy red underline styling.
- **Official Letter Regulation compliance checker (#246 / PR #249):** verifies page margins (min 2.5 cm), font & size (Times New Roman 12 pt / Arial 11 pt), line spacing, number/subject/date headers, and signature blocks with 1-click automatic fixes.
- **Petition Wizard (#247 / PR #250):** step-by-step petition creation with local TCKN mod-11 validation, producing compliant UDF (UYAP) and PDF documents.

## [5.2.0] - Gerçek dosya laboratuvarı (Real-file lab)

A minor release that builds the real-file test corpus, establishes nightly corpus compatibility scoring against LibreOffice, fixes LibreOffice ODF package manifest alignment, and extends fuzzing to all office file importers.

### Added

- **License-clean sample corpus (#235 / PR #236):** added `tests/corpus` with public/clean samples across 5 formats (DOCX, ODT, XLSX, PPTX, UDF) and documented in `docs/corpus.md` with an automated import test.
- **Corpus compatibility score & nightly CI (#237 / PR #239):** added `crates/compat` measuring structural, visual, and reference rendering scores against LibreOffice, reported nightly and tested in CI.
- **Fuzz targets extension (#241 / PR #242):** added per-importer fuzz targets for `docx`, `xlsx`, `pptx`, `udf`, and `ubl` with seed corpus under `fuzz/fuzz_targets/` and integrated into nightly/master CI with AddressSanitizer.

### Fixed

- **LibreOffice ODF output compatibility (PR #240):** fixed ODF output manifest and package alignment so LibreOffice can open ODT, ODS, and ODP documents without load errors.

## [5.1.0] - Sözleşme ve doğruluk (Contract and correctness)

A minor release that hardens the IPC wire contracts, establishes type generation from Rust to TypeScript, enforces test-backed claims, and introduces feature flags.

### Added

- **Rust -> TS DTO type generation (#222 / PR #223):** automatic TypeScript types generation for IPC DTOs via `ts-rs` under an optional feature flag, with a CI drift gate (`types:check`).
- **Real-serde wire contract tests (#225 / PR #226):** pins 20 DTO top-level JSON wire keys with `serde_json`, verifying them with mutation checks to prevent runtime discrepancies.
- **"Claim = Test" enforcement in CI (#229 / PR #230):** every implemented claim in `RELEASE_READINESS.md` names its verifying test marker, enforced by `scripts/claims-audit.mjs` during CI lint.
- **Feature flags infrastructure (#231 / PR #232):** added `src/lib/flags.ts` to hide experimental or incomplete features (e.g. scanner preview) behind off-by-default flags while preserving route contracts.

### Security & Dependencies

- Addressed `yoke-derive 0.8.3` yank by bumping to `0.8.4` (#227 / PR #228).
- Added time-boxed, verified exceptions for upstream `zip` advisory extraction issues (`RUSTSEC-2026-0367/0368/0369`).

## [5.0.1] - Kanama durdurma (Stopping the bleeding)

A patch release that closes every **P0** from the independent v5.0.0 audit
(`docs/plan/DENETIM-v5.0.md`, PR #180). No new features.

### Fixed

- **Security:** the e-Devlet "verify" button now requires an https URL whose
  host is `turkiye.gov.tr` (or a subdomain), so a document can no longer point
  it at a lookalike (#187).
- **Crashes:** a nested `message/rfc822` `.eml` overflowed the stack and closed
  the app; the nesting depth now carries across the whole chain (#185). Three
  UTF-8 panics (plugin PDF text truncation, vault snippets with Turkish `İ`,
  a truncated `embeddings.bin`) no longer panic (#183).
- **Data loss:** editing a cell no longer turns `1.234` into `1234` in Turkish,
  and `18%` is `0.18` (#189); a second stamp no longer overwrites the first
  stamp's font, and filling a form keeps the document's own `/Helv` (#191);
  document comparison keeps removed blocks and per-run formatting (#193); a
  failed autosave no longer loses image blobs permanently (#195).
- **Watched folder:** a folder that overlaps its output is refused and never
  overwrites its input, stop/start can no longer run two loops, and the
  document password lives in the platform secret store instead of the config
  file (#197).
- **Dependencies:** `rustls` 0.23.45 → 0.23.46, clearing four RUSTSEC
  advisories and the red `Dependency audit` CI gate (#181).

### Known limitations

- The UDF reader/writer is still unverified against a real UYAP sample (tracked
  for the v5.2 file corpus).
- Comparison shows a removed table as delete-marked text (the model has no
  block-level revision mark); a matched table is taken from the revised side.

## [5.0.0] - Erişilebilir ve hızlı (Accessible and fast)

### Added

- **Mail merge** (issue #114): a Writer document's `{{placeholders}}` are filled
  from a CSV/TSV table, producing one document per row (or a single preview),
  with a missing-field report before anything is written.
- **Document comparison** (issue #115): the Writer "Compare with…" action diffs
  the open document against a picked version and turns the differences into
  **tracked changes** (word-level insert/delete), reusing the revision engine.
- **Plugin API v2** (issue #116): a read-only **`pdf` capability**
  (`host.pdf.read` → page count, text layer, bounded text) and a **`batch-step`
  capability** that exposes recipe steps from a plugin manifest. API v1 plugins
  keep loading. See `docs/plugins.md` and the two example plugins.
- **Tagged PDF export** (issue #117): Writer and Impress PDFs now carry a
  `StructTreeRoot` (`H1`..`H6`/`P`/`Figure` with `/Alt`), `/Lang (tr-TR)`,
  `/MarkInfo` and a document title, so screen readers can read the structure.
  The Inspector reports PDF/UA gaps (`a11y.figure-alt` included).
- **Reader selectable text + read-aloud** (issue #118): a transparent text
  layer over the page bitmap makes text selectable/copyable, and a Web Speech
  (tr-TR) control reads the current page with rate and pause/resume. Scanned
  pages show the OCR hint.

### Changed

- **Performance** (issue #119): office images move out of the model JSON into a
  content-addressed blob store, so autosave only sends images that changed
  (measured reduction). The Turkish dictionary is a **lazy chunk**, dropping the
  entry bundle from ~123 KB to ~87 KB gzip.

### Security

- **Audit closure** (issue #120): L2 (plugin sandbox reparse-point re-checks),
  L3 (strict manifest validation), L4 (streamed "keep both" download) and L5
  (AI save-path scope documented) are closed; H5 (AI page extraction) is now
  bounded by the heavy-work semaphore and a 4 MB text cap. A new `pdf_parse`
  fuzz target runs in CI. `AUDIT_REPORT.md` records the evidence.

### Known limitations

- Comparison diffs at the **word level** and aligns paragraphs by position;
  tables are carried over from the revised document, not diffed.
- Tagged export covers Writer paragraphs/images and Impress text/images;
  **table cells, headers/footers** and XMP metadata are not tagged, so it is a
  PDF/UA-oriented output, not a certified one.
- Read-aloud depends on the platform's Web Speech voice; it may not be fully
  offline, and a scanned PDF (no text layer) cannot be read.
- The blob store applies to the **autosave/recovery** path; a normal Save still
  writes the full model. Blob GC is available (`recovery_gc_blobs`) but not yet
  scheduled.
- The plugin `batch-step` hook is exposed to the recipe UI; the Rust recipe
  engine does not yet auto-register plugin steps.

## [4.12.0] - Otomasyon (Automation)

### Added

- **Chainable batch recipes** (issue #112): a recipe is an ordered list of
  steps — compress, stamp, PDF/A, OCR, redact — run over one PDF with each
  step's output chained into the next and written atomically. Recipes are saved
  as JSON and reloaded, so a chain is built once and reused.
- **Watched folder** (issue #112): a folder paired with a recipe processes new
  PDFs into an output folder, with a per-pass dedup set and a `watched:file`
  event. A background polling loop can run it continuously. The **Recipes**
  screen builds, saves and runs recipes and configures the watched folder.
- **Command-line interface** (issue #113): `OmniOffice.exe --cli merge a.pdf
  b.pdf -o out.pdf` (also `compress` and `ocr`, with `--lang`), returning 0/1/2
  exit codes. On Windows the process attaches to the parent console so the
  output is visible.
- **Explorer "Send to"** (issue #113): the NSIS installer creates a
  `Send to → OmniOffice` shortcut; sent files arrive through the launch-file
  queue.

### Known limitations

- The watched folder uses a **polling** loop (no filesystem-notification
  dependency); a single pass is unit-tested, the long-running loop is not.
- The recipe UI adds steps with **sensible defaults**; detailed options live in
  the dedicated tools.
- The CLI covers **merge/compress/ocr** only; other tools can be added to the
  same skeleton.
- The **NSIS "Send to" hook was not verified by a local install**; the hook is
  compiled by the Windows release build.
- Recipe steps call the pdfcore functions directly rather than through
  `JobRegistry`; progress is reported with a `recipe:progress` event.

## [4.11.0] - Cepte Ofis (Office in your pocket)

### Added

- **Document scanner** (issue #108): a new `pdfcore::scan` module corrects the
  perspective of a photographed page (order the four corners, derive the output
  size, resample with a homography) and `office_scan_pages` assembles the
  corrected pages into a multi-page PDF, reusing the existing image writer and
  OCR preprocessing. The Android `CAMERA` permission is declared and a
  **Scan** screen lets page photos be assembled today.
- **Share filters** (issue #109): images (`image/*`, jpeg/png/webp/bmp/gif/tiff)
  and plain text (`text/plain`, `text/markdown`, `text/html`, `text/csv`,
  `text/tab-separated-values`) are accepted through the Android share sheet,
  with matching name fallbacks; the contract tests cover every extension.
- **Launcher shortcuts** (issue #109): Scan, New document and Recent files are
  declared as static app shortcuts.
- **Signature and paraphe library** (issue #110): a drawn signature/paraphe is
  saved locally (`signature_save/list/delete`), listed with previews, and
  reused. Ready dated stamps (**ASLI GİBİDİR**, **ONAYLANDI**, **GÖRÜLDÜ**) are
  written as real `/Annots` text boxes, and **"Paraphe every page"** applies the
  chosen signature to every page in one action.
- **File-type-aware Home** (issue #111): the Home screen recommends tools from
  the dropped file's type (PDF, Word/ODT/UDF, spreadsheet, presentation, image)
  instead of always showing the same PDF tools; tools can be **pinned** and the
  choice persists; the duplicated sidebar group label is fixed.

### Known limitations

- The **Android camera capture is not implemented**: the ML Kit / CameraX flow,
  the manual four-corner pick and the runtime camera permission request are the
  remaining part of the scanner. The Scan screen says so; page photos can be
  picked and corrected today.
- **Shortcut routing is not wired**: the shortcuts appear and open the app, but
  the action is not yet routed to the Scan/New document/Recent files screen.
- The share, shortcut and scanner flows were **not tested on a device or
  emulator** in this environment; only the Rust geometry, the manifest contract
  tests and the screen wiring are covered.
- The perspective correction is implemented directly instead of `imageproc`
  (same projection, no extra dependency).
- Signatures are stored **unencrypted** in the app data directory (OS-account
  access only).
- The scanner does not run OCR itself; the OCR tool is used on the result.

## [4.10.0] - Belge Asistanı 2.1: Gerçek arama, doğrulanmış atıf (Document Assistant 2.1)

### Added

- **Real hybrid search** (issue #101): a persistent index with **BM25** (inverted
  index, IDF, length normalisation), **per-chunk embeddings** stored in
  `embeddings.bin` + `index.json` (keyed by `model + hash(chunk text)`, so an
  unchanged chunk is never re-embedded) and **Reciprocal Rank Fusion**. A
  5.000-chunk query runs in well under 300 ms.
- **Wired into the app** (issue #102): the Vault screen and the chat's vault
  scope use the hybrid search, and a citation now targets **`(document,
  location)`** — clicking it opens the right file at the right page instead of
  pages 1–5 of the current document.
- **Verified citations** (issue #103): the model must back each claim with a
  short quote; the quote is fuzzy-matched against the source page (Turkish
  letters folded) and an unverifiable one is shown with an **"unverified"**
  badge. `[page N]`, `[sayfa N]`, `[pages 3-4]`, `[sayfalar 3-4]` and `(p. 4)`
  are all parsed; a page that was not retrieved is rejected.
- **Prompt-injection hardening** (issue #104): the document text is wrapped in
  `<document_context>` and the system prompt says it is data, not instructions;
  a `system` role in the client history is refused; the history is trimmed to a
  context budget.
- **Bounded batch queue** (issue #105): at most 3 items run at once, a `429` is
  retried after the provider's `Retry-After`, a non-streaming call can be
  cancelled, each item may carry its own password, and the result shows the
  approximate token use and cost.
- **Vision capability gate** (issue #106): the "send page image" option is
  disabled when the provider cannot receive images, the image has a width and a
  4 MB size limit, and a render/encode failure is reported instead of being
  swallowed.
- **Consent and privacy** (issue #107): separate consents for the vault scope,
  embedding and page images (enforced in the backend too), **LM Studio** is
  treated as local, AI jobs no longer write their input to `jobs.json`, and
  Settings has **"Clear AI data"** plus a **retention period** for saved chats.

### Known limitations

- Embeddings are sent one chunk at a time (providers expose no batch endpoint);
  "batching" is the bounded concurrency, not a single grouped request.
- The token/cost figure is an **estimate** (characters / 4 and a per-model price
  table), shown with a `~`.
- Cancelling a non-streaming call takes effect on the next poll tick, not
  instantly.
- An **office** citation opens the file in the workspace; a paragraph/sheet/slide
  anchor is not scrolled to yet.
- Consents are **per session** (asked again after a restart).
- `vault_search_hybrid` falls back to BM25 when embeddings are not configured or
  not consented.
- The total JS bundle budget stays at 500 KB.

## [4.9.0] - Resmî Belgeler (Official documents)

### Added

- **UYAP `.udf` documents** (issue #96): the Turkish judicial document format
  opens in Writer (paragraphs, headings, lists, bold/italic/underline, font,
  size, colour, alignment, indents, spacing, tables with spans and a page
  setup) and saves back as `.udf`. An element the importer does not model is
  kept verbatim and re-exported, so a round trip never drops it, and every such
  element is reported. `.udf` is associated on Windows (`fileAssociations`) and
  on Android (intent filters + the incoming-file whitelist).
- **e-Fatura / e-Arşiv (UBL-TR) invoices** (issue #97): the new **Invoices**
  screen reads a `.xml` or `.zip` invoice — supplier and customer with their
  VKN/TCKN, ETTN, lines, VAT rates and bases, withholding (tevkifat) and the
  payable amount — checks the arithmetic and the dated VAT-rate table, renders
  the invoice with the app's own deterministic template (never the invoice's
  XSLT), exports it to PDF and aggregates a **VAT summary** (by month, supplier
  and rate) into an XLSX workbook.
- **GİB karekod reading** (issue #98): the QR code on a printed invoice is
  decoded locally with a pure-Rust QR reader, its JSON payload is parsed and its
  sender VKN/TCKN is checksummed. When the invoice's UBL document is open too,
  the karekod is compared with it field by field and any disagreement is shown
  in red.
- **e-Devlet document verification** (issue #99): when a PDF carries an
  e-Devlet "barkodlu belge" block, the Reader shows a "Verify document" button
  that opens the official page (`turkiye.gov.tr/belge-dogrulama`). The code is
  shown so the user types it there; nothing is sent automatically.
- **KEP `.eml` viewer** (issue #100): a read-only screen opens a KEP e-mail
  message — headers (RFC 2047 decoded), text/HTML body and attachments. A PDF
  attachment opens in the Reader; an executable attachment is never launched.
  An encrypted S/MIME body is kept as an attachment and reported, never
  decrypted.

### Known limitations

- UDF is a **closed format**: the reader/writer implement the documented
  container (ZIP + `content.xml`) and the module's own files; no open-licensed
  third-party `.udf` sample was available to test against. Unknown elements keep
  their bytes but not their formatting.
- An invoice's own **XSLT is never executed**; the app always uses its
  deterministic template.
- GİB karekod verification compares against the **UBL document**, not the AI
  extraction cards (deterministic and stronger). A low-resolution scan may not
  decode; the UI says so.
- A **password-protected or encrypted KEP message is not decrypted**; the
  S/MIME part is kept as an attachment.
- An HTML mail body is shown as **plain text** so no remote resource is loaded.
- The total JS bundle budget moved from 485 to 500 KB for this cycle (all new
  screens are lazy routes); the reason is recorded in
  `scripts/check-bundle-budget.mjs`.

## [4.8.0] - Türkçe Doğru (Turkish correctness)

### Added

- **Turkish text in generated PDFs**: stamps, page numbering and form fields
  are drawn with an embedded Type0/CID font, so `ı İ ğ ş ç ö ü` render instead
  of blank boxes (`crates/pdfcore/src/textfont.rs`, `stamp.rs`, `numbering.rs`,
  `forms.rs`; verified with a pdfium text-extraction test).
- **Calc with Turkish numbers**: typing `1.250,15` in a cell stores `1250.15`,
  and the sheet's default number and date format follows the Turkish locale
  (`src/office/calc/locale.ts`, `numberFormat.ts`, `cells.ts`).
- **Turkish formulas and validation**: `YAZIYLA()` writes a number in Turkish
  words; `TCKNDOGRULA()`, `VKNDOGRULA()` and `IBANDOGRULA()` validate a Turkish
  ID, tax number and IBAN; `NETWORKDAYS()`/`WORKDAY()` accept a `"TR"` argument
  that includes the built-in Turkish public holidays for 2020–2029
  (`src/office/calc/functions/turkish.ts`, `tr-holidays.ts`, `datetime.ts`).
- **Turkish search that finds İ/i and I/ı**: the file search and the document
  vault fold Turkish case correctly, and a Turkish Windows starts with the
  Turkish interface and OCR languages on first run (`src/lib/search-fold.ts`,
  `crates/synccore`, `src-tauri/src/vault.rs`).
- **KVKK redaction**: the redaction screen detects Turkish national IDs, tax
  numbers, IBANs, plates and phone numbers, with a false-positive test set, and
  the Batch screen can redact them across many PDFs at once
  (`crates/pdfcore/src/redact.rs`, `src/screens/Batch.tsx`).
- **Turkish hyphenation**: justified Writer PDF export breaks long Turkish
  words at real syllable boundaries from a rule table, with a 500-word golden
  list shared by the Rust and TypeScript implementations
  (`crates/officecore/src/hyphen_tr.rs`, `src/office/writer/hyphen-tr.ts`).
- **Your name on comments and changes**: Settings has a "Your name" field, and
  new Writer comments and tracked changes are marked with it
  (`src/office/writer/identity.ts`, `WriterEditor.tsx`).
- **Seven Turkish templates**: Resmî Yazı (per the Resmî Yazışma Yönetmeliği),
  İhtarname, Kira Sözleşmesi, İzin Dilekçesi, İstifa Dilekçesi, Bordro (with
  SGK/vergi formulas) and İrsaliye; every field is a `{{alan}}` placeholder and
  no template contains a real person's data (`src/office/templates.ts`).

### Known limitations

- Hyphenation drives the **PDF export**; the editor view still relies on the
  browser's own hyphenation (soft-hyphen integration is not done).
- Turkish public holidays are tabulated for **2020–2029 only** (Diyanet data);
  2030 and later are deliberately omitted because 2030 has two Ramazans.
- Some PDF text paths that draw over existing pages (annotate, flatten, sign)
  still use the built-in fonts; the remaining ones are tracked in issue #137.
- Calc formulas keep `,`/`;` as argument separators, so a decimal **inside a
  formula** is written with a dot (`YAZIYLA(1250.15)`); only cell input accepts
  `1.250,15`.
- The Chrome-extension dependency update is still tracked in issue #129.

## [4.7.1] - Onarım ve dürüstlük (Repairs and honesty)

### Fixed

- **Invoice extraction keeps every field**: the parser accepts snake_case and
  camelCase replies (`invoice_number`, `line_items`, `custom_fields` aliases),
  defaults a missing `page`, reads amounts written as `1.250,00`, `1,250.00`,
  `1250 TL` or `(1.234,56)`, parses rows one by one and reports an
  unparseable reply with the raw model answer instead of silently returning
  zero rows.
- **The AI screen is fully translated**: 50 keys used through `label(...)`
  were missing from both tables (the parity audit could not see them); they
  are added, `ai.jumpToPage` interpolates the page number, and the i18n audit
  now scans source usage and fails CI on unknown keys.
- **Update security**: a rejected signature no longer falls back to the
  browser silently (the banner reports that the download could not be
  verified); the release URL allow-list parses and normalises URLs and rejects
  `..`/`%2e` escapes; Android refuses to install an APK without the published
  SHA-256; pre-release tags compare per SemVer.
- **Batch AI honesty**: file picking goes through the Android SAF bridge, and
  cancelled or all-failed batches report `cancelled`/`failed` instead of
  looking completed.
- **CSV export**: cells starting with `=`, `+`, `-`, `@`, tab or CR get a
  leading apostrophe (formula-injection neutralisation) and columns map by
  exact field name.
- **Release pipeline**: version-consistency jobs gate the Windows and Android
  builds, `auto-tag` also checks `tauri.properties`, release notes contain
  only the version's CHANGELOG section, and `desktop.yml` compiles the Kotlin
  sources and runs the Android unit tests when `src-tauri/gen/android/**`
  changes. The Gradle shell wrapper is stored LF so it runs under `sh`.
- **Documentation honesty**: the 4.7.0 claims (BM25, invoice fields, batch
  parallelism, `ai-batch-progress`) are corrected to what the code does, and
  the missing 4.6.2/4.7.0 readiness sections were added.
- **Dependabot**: the actions, frontend and rust groups were merged; the
  `pkcs8`/`hmac` majors are deferred with their real blockers, and the
  Chrome-extension updates are tracked in a separate issue.

### Known limitations

- Invoice extraction still depends on the model: line items are a summary
  string, not itemised rows.
- Vault retrieval is keyword matching plus optional embedding similarity;
  BM25 and the UI wiring of `vault_search_hybrid` arrive in 4.10.0.
- Batch items run sequentially; the bounded queue arrives in 4.10.0.
- The Windows installer is not Authenticode-signed (SmartScreen warning) and
  Android in-app install needs the "install unknown apps" permission.

## [4.7.0] - Belge Asistanı 2.0 (Document Assistant 2.0)

### Added

- **Multi-turn document chat with verifiable citations**: contextual conversation with document grounding. Every claim emits clickable `[page N]` citation cards that deep-link directly into the PDF Reader, auto-scrolling to the exact cited page.
- **Vault-wide Q&A**: query the document vault from the chat scope; excerpts
  are ranked by keyword matching and, when an embedding model is configured,
  by embedding cosine similarity. BM25 is not implemented and the dedicated
  hybrid command is not wired into the UI yet (planned for 4.10.0).
- **Structured invoice & receipt table extraction**: extract page, vendor,
  date, invoiceNumber, currency, subtotal, tax, total, a lineItems summary and
  requested custom fields into an interactive tabular preview with RFC 4180 CSV
  export. (The 4.7.0 parser lost snake_case replies and rows without a page;
  4.7.1 fixed it.)
- **Batch AI queue**: process multiple documents one at a time for
  summarization, translation, classification, or table extraction with
  real-time progress (`ai:batch_progress`) and per-file result inspection.
- **Vision model support**: render document pages as images and send them directly to vision-capable models for multi-modal analysis.
- **Local provider presets**: quick one-click configuration for local models in Ollama (`http://localhost:11434`) and LM Studio (`http://localhost:1234/v1`).
- **Chat thread persistence**: locally persist, list, reload, and delete document conversation threads.
- **Offline-first privacy & consent**: transparent data flow with provider-specific consent notices and zero telemetry.

## [4.6.2] - Android update installer fix

### Fixed

- **Android release build**: the new update-installer plugin's comment
  contained `/*`, which Kotlin's nested block comments turn into an
  unterminated comment; the release build failed and v4.6.1 shipped Windows
  assets only. The comment is fixed and v4.6.2 is the complete release
  (Windows installer/ZIP and both APK/AAB ABIs).

## [4.6.1] - Hygiene and in-app updates

### Added

- **In-app updates**: the desktop app reads signed update manifests from the
  project's GitHub releases and installs a new version with one click
  (Windows; the installer still warns because the app is not
  Authenticode-signed). Android downloads the release APK, verifies its
  SHA-256 (from the GitHub asset digest) and opens the system installer. When
  no signed manifest is configured, the app keeps the browser fallback.
- **Operation log**: an "Export CSV" button (filtered rows, RFC 4180
  escaping) and a text filter in History; the `keepOperationLog` setting now
  really controls whether operations are recorded.
- **AI settings**: a "Discover models" button asks the configured provider for
  its live model list and fills the dropdown.
- **Sync**: a manual "Delete from cloud" action removes the cloud copy of a
  tracked document after confirmation; the local file is never touched.

### Fixed

- **Documentation told the wrong story in places**: the README still said
  4.4.0, the Android section claimed there is no foreground service (it
  shipped in 4.3.0) and that tiled rendering was pending (4.1.0), and the
  release-readiness file contradicted itself about Windows E2E. All corrected,
  and the missing 4.5.0 readiness section was added.

### Changed

- The desktop update check uses the signed updater manifest first; the GitHub
  releases API check remains as the fallback.

## [4.6.0] - PDF tools

### Added

- **Editable annotations**: sticky notes, highlights, underline, strikethrough,
  freehand ink, text boxes and image/signature stamps are written into the
  PDF's annotation layer (`/Annots`). Existing annotations are loaded, listed,
  moved, resized, edited and deleted, and the old "flatten into the page"
  behavior stays as a toggle.
- **Fill and sign**: draw a signature with a finger or mouse, or pick a
  PNG/JPG, and place it on the page. The signature is a plain annotation,
  separate from certified PAdES signing. Digital signing on Android accepts a
  `.p12`/`.pfx` file (the Windows certificate store stays desktop-only).
- **Keyword and regex redaction** next to the automatic detector: search the
  whole document, preview the matches, confirm and redact. Matching uses the
  Rust `regex` engine (linear time, size-limited), so a slow pattern cannot
  freeze the app.
- **Organize**: insert blank pages, and edit the document outline (bookmarks)
  with titles, target pages and levels.
- **Header, footer and Bates stamps**: six template slots with `{page}` and
  `{pages}` tokens, plus Bates numbering with prefix/suffix, start and
  zero-padded digits; both append a new revision so signatures survive.
- **N-up and booklet**: two or four pages per sheet with margins, gutter and
  optional borders, and saddle-stitch booklet ordering.
- **PDF to Word**: password-protected PDFs convert with a password prompt,
  simple tables are recovered as real Word tables, and numbered lists restart
  at their own start instead of continuing the previous list.

### Fixed

- Numbered lists in DOCX no longer share one numbering definition: every list
  run gets its own `startOverride`, the PDF export increments numbers per item,
  and the Writer editor restarts a list whose start differs.
- PDF to Word surfaces the real `password_required`/`wrong_password` error
  instead of writing an empty document.

## [4.5.0] - Writer and Impress, deeper editing

### Added

- **Writer tables**: merge and split cells (rectangular selection, context
  menu) and drag column borders to resize. Colspan and rowspan now survive
  DOCX and ODT (complete `vMerge` continuations and covered cells on write,
  rowspan rebuilt on read) and lay out correctly in the PDF export.
- **Writer images**: text wrapping (inline, square, top and bottom) written as
  DOCX `wp:anchor` and ODT graphic wrap styles, corner size handles in the
  editor, and ODT images are no longer dropped when they sit inside a
  paragraph.
- **Format painter** and a **ruler with tab stops** (click to add, drag to
  move, double-click to remove). Tab stops are written to DOCX `w:tabs` and
  ODF `style:tab-stops` and position the PDF export.
- **Watermark** ("TASLAK", "GİZLİ" or custom, with opacity, rotation and
  size): shown behind the page in the editor, written to DOCX (header VML)
  and ODT (run-through frame plus lossless meta), and stamped into the PDF
  export.
- **Impress rich text**: a paragraph and run based editor with bold, italic,
  underline, color, size and bullet toggles and Tab/Shift+Tab levels. Runs
  survive editing and the PPTX/ODP exports, and the PDF export draws
  run-level formatting.
- **Impress editing**: align to selection and distribute, glued connectors
  that follow their shapes, image crop, hidden slides, footer/date/slide
  number settings, a slide sorter grid, laser pointer and pen during the
  show, and keyboard shortcuts (Delete, arrows scroll, Ctrl+Z/Y, Ctrl+C/V/D).
- **ODP presentations** now embed real chart documents instead of drawn
  placeholder shapes, round-trip footer/slide-number fields, hidden slides
  and connectors, and keep paragraph runs.

### Fixed

- **ODT import** no longer loses bold and italic on spans with named
  character styles, and named paragraph styles (alignment, indents, spacing)
  are applied again.
- **RTF import** keeps one paragraph per paragraph instead of starting a new
  paragraph at every formatting group.
- **PPTX run fidelity**: underline, strike, highlight, superscript and
  subscript round-trip, and footer/slide-number/date placeholders carry real
  fields.
- Writer paginated view measures only the blocks that changed while typing
  instead of re-measuring the whole document.

## [4.4.0] - Calc, closer to Excel

### Added

- **Find and replace** (Ctrl+F / Ctrl+H) in the sheet or the whole workbook,
  in values or formulas, with case, whole-cell and regular-expression options.
  A slow pattern is stopped instead of freezing the app; one undo step.
- **Sort dialog** with up to three keys and a header row option, and **filter
  conditions** (contains, begins/ends with, greater/less/between, top 10,
  blank) next to the value list. Formulas move with their relative references.
- **Hide and unhide rows and columns** (header menu, View tab, Ctrl+9 / Ctrl+0);
  hidden rows and columns survive XLSX and ODS and are skipped in PDF export.
- **Paste special** (Ctrl+Alt+V: values, formats, formulas, transpose) and
  **AutoSum** (Alt+=).
- **Cell notes**: add, edit, delete and pin; author and text are kept in XLSX
  and ODS. **Hyperlinks** (Ctrl+K): web, e-mail and place in this document;
  Ctrl+click follows a link, also for `HYPERLINK()` cells.
- **Charts**: scatter and doughnut, with XLSX and ODS round trips.
- **Conditional formatting**: color scales, data bars, icon sets and formula
  rules, with bold, italic, fill and font color; read and written in XLSX and
  ODS. The 5,000-cell limit per rule is gone.
- **About 80 more functions** (now 274): SUBTOTAL, AGGREGATE, ROW, COLUMN,
  ADDRESS, ISFORMULA, FORMULATEXT, MODE, GEOMEAN, SLOPE, INTERCEPT, FORECAST,
  TREND, NORM.DIST, NORM.INV, T.DIST, BINOM.DIST, POISSON.DIST, HSTACK, VSTACK,
  TAKE, DROP, CHOOSECOLS, TOCOL, WRAPROWS, FACT, COMBIN, MROUND, EVEN, ODD,
  DOLLAR, FIXED and more. The `#CALC!` error (empty array result) is shown and
  explained.
- **Screen readers** hear the active cell, its value and its note; toggle
  buttons report their state.

### Fixed

- **ODS formulas were all broken** (commas not turned into `;`, some function
  names written as cell references, a missing namespace made LibreOffice show
  `Err:510`). They now round trip and calculate in LibreOffice.
- XLSX import no longer rejects a whole sheet that contains `#CALC!`, `#SPILL!`
  or another error code it does not know.
- XLSX export prefixes newer functions the way Excel expects (`_xlfn.`), so
  they calculate in Excel instead of showing `#NAME?`.
- Conditional-format colors are no longer replaced by a single yellow on export.
- A plain sheet filter no longer hides the header row; inserting or deleting
  rows and columns moves row heights and column widths with the data.
- `COUNTIF` counts error values; several functions follow Excel more closely
  (`N("7")` is 0, `CLEAN` removes all control characters, `GCD`/`LCM` reject
  negatives).

### Changed

- The Calc editor is split into smaller modules and loads on demand.

## [4.3.0] - Fixes and robustness

### Fixed

- **PowerPoint export**: bullets are written as real bullets instead of
  garbled characters, text is no longer marked as Turkish, and text edited in
  Impress is exported as edited. Decks saved by 4.2.0 export correctly now.
- **Impress**: changing the font size or alignment of a text frame no longer
  merges its paragraphs.
- **Android**: Word, Excel, PowerPoint and OpenDocument files now appear in
  "Open with" and in the share sheet.
- **Calc**: Freeze panes is visible and stays in place while scrolling;
  custom and hidden row heights are honoured (no overlapping or blank rows);
  data validation applies to every cell of its range, not just the first 100;
  arrow keys skip rows hidden by a filter; scrolling a cell into view is exact.
- **Calc functions**: OFFSET, INDIRECT, CELL and INFO work instead of
  returning `#NAME?`.
- **Version history** is kept per document path, so it survives closing and
  reopening a file, and a crash while pruning can no longer leave an index
  pointing at a missing version.
- **Import notice** when an XLSX or ODS file has data beyond 100,000 rows or
  1,000 columns, which is not imported.
- "System" theme follows the operating system live.
- Table menu in Writer and transition names in Impress are translated.
- The compatibility notes for PDF/A font subsetting and RFC 3161 timestamps
  were out of date and are corrected.

### Added

- **Crash safety**: a screen or editor tab that fails shows a recovery panel
  instead of blanking the app, with an autosave copy of unsaved work; Rust
  panics are written to `crash.log` and included in the diagnostics export.
- **Windows**: opening a second file uses the running window instead of
  starting another copy.
- **Android**: long jobs (OCR, compression, conversion) keep running when the
  app is in the background, with a notification.

### Changed

- The Windows installer ships only `tesseract.exe` and the libraries it loads,
  about 26 MB smaller compressed.

## [4.2.0] - AI in the editors, Claude, safer keys

### Added

- **Claude (Anthropic) as an AI provider**: Claude Opus 5.5, Sonnet 5.5 and
  Haiku 4.5 through the Messages API with your own key, including streaming
  and the model list. Gemini's model list works again.
- **AI inside the editors** (optional, off until a provider is set up).
  Writer: rewrite, shorten, expand, fix, translate and change tone for the
  selection; Calc: summarize a column and suggest a formula; Impress: slides
  from an outline. Every result is previewed first and applied as one undo
  step. A notice names the provider and what will be sent before the first
  request of each kind.
- **Encrypted keys on Android**: the AI key, cloud-sync tokens and the WebDAV
  password are encrypted with the Android Keystore; existing values are
  migrated on first use without being lost.
- **One key per AI provider**: switching provider never sends another
  provider's key.

### Fixed

- An AI answer cut off by the token limit is reported instead of replacing
  your text with a partial one.
- API keys are never sent to a different host after a redirect.
- Suggested formulas that could reach the network or other programs
  (WEBSERVICE, HYPERLINK, DDE, external references) are refused.

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
