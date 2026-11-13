# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [7.47.0] - 2026-11-13

### Fixed

- **Desktop pre-send metadata check dialog:** renders `<PresendDialog>` inside `GlobalOverlays` rather than only the compact mobile navigation slot, ensuring `shareWithCheck` prompts display properly on desktop.

### Changed

- **Hierarchical Tagged PDF & pagination artifacts (`officecore`):** emits nested `/Table` → `/TR` → `/TH`/`/TD` → `/P` PDF structure elements with bidirectional `/P` links and valid `/ParentTree` entries across Writer, Calc, and Impress exports, wraps running headers/footers and Slide Master footers/dates/slide numbers in pagination `/Artifact` BDC blocks, and reopens `/Table` structure cleanly across split pages.

### Added

- **Extended Office fidelity corpus (`office_fidelity_extended_test.rs`):** verifies zero-delta round trips for images, cached-data charts, threaded/resolved/anchored comments, headers/footers, bullet/numbered lists, section breaks, freeze panes, links/tooltips, structured tables, print settings/breaks, sheet protection, slide masters/layouts, nested groups, image crops, animations, notes, transitions, and hidden slides across DOCX, ODT, XLSX, PPTX, and ODP.

## [7.46.0] - 2026-11-13

### Added

- **XLSX `t="shared"` formula export grouping:** groups consecutive filled-down and filled-right Calc formulas into OOXML `t="shared"` groups (`ref`/`si` anchors + `si` followers), keeping absolute refs, `_xlfn.`, `t="str"`, and `spill` arrays intact.

### Changed

- **Cross-platform CI engine lock check:** `.github/workflows/desktop.yml` now runs `node scripts/fetch-engines.mjs --verify-lock` in `lint` to validate all 20 pinned artifacts on Linux, macOS, and Windows.

## [7.45.0] - 2026-11-13

### Fixed

- **Unified background job store and bounded progress memory:** consolidated the duplicate `useJobs` stores (`src/lib/store.ts` and `src/lib/jobs.ts`) into a single store backed by extracted persistence helpers (`src/lib/jobs-persist.ts`), eliminating duplicate `job:progress` event subscriptions in `App.tsx`, settling completed tool jobs cleanly in `useTool.ts`, and evicting finished, cancelled, restarted, and pruned job progress payloads from memory.

### Changed

- **Modularized Writer Ribbon:** extracted the Writer editor's Ribbon (`src/office/writer/panels/WriterRibbon.tsx` and `src/office/writer/panels/WriterRibbonTabs.tsx`) from `src/office/WriterEditor.tsx`, reducing `WriterEditor.tsx` by roughly 500 lines without changing editor behavior.

## [7.44.0] - 2026-11-13

### Changed

- **Unauthenticated `pdf_info` runs in the isolated `--worker` subprocess:** opening a PDF
  without a password now parses document metadata, page count, and encryption status out of
  process under the worker's memory/CPU containment, returning a clean error if a malformed
  file crashes the parser while preserving `PasswordRequired` prompts.
- **Hardened LAN sync frame reader:** `synccore::lan` now reads frame bodies with a bounded
  reader instead of pre-allocating from an unverified length header.

### Added

- **Cross-platform engine and font fetcher (`scripts/fetch-engines.mjs`):** mirrors
  `scripts/fetch-engines.ps1` in pure Node.js with `--verify-lock`, `--update-lock`, and
  SHA-256 verification against `scripts/engines.lock.json` on Linux, macOS, and Windows.
- **Expanded fuzzing coverage:** added `eml_parse`, `odf_read`, and `lan_frame` fuzz targets
  in `fuzz/fuzz_targets/` and removed `std::panic::catch_unwind` from `pdf_parse` so
  libFuzzer catches parser panics directly.

## [7.43.0] - 2026-11-12

### Changed

- **Flatten burns markup that has no appearance stream:** highlights, ink, shapes, lines and
  free text saved by tools that leave drawing to the viewer were left in place by Flatten
  ("no appearance stream"). They are now drawn from their own geometry, the way N-up already
  draws them, and burned into the page. Annotations that ship an appearance are unchanged;
  sticky notes and stamps without one are still left with a warning.

### Fixed

- **Deleting an annotation could make the next edit hit the wrong one:** the PDF annotation
  editor sends its changes in the order they were made, and a delete shifted the position of
  every later annotation, so "delete one, then move another" missed or moved the wrong
  annotation. Positions now refer to the list as shown, and deletions are applied last.
- **Deleted comments left their popup and replies behind:** deleting a comment now also removes
  its popup note and every reply to it (and replies to those replies).

## [7.42.0] - 2026-11-12

### Added

- **Remember the AI data notice for a provider:** after confirming the notice that document text
  will be sent to the AI provider, "Remember for {provider}" keeps that consent for the same
  provider and host, so the next documents do not ask again. Switching provider or host asks
  again, and Settings > AI privacy shows what is remembered with a Forget button. The vault,
  embedding and page-image consents are still asked each time.

### Changed

- **Embeddings are sent in batches:** vault indexing and document Q&A sent one embeddings request
  per chunk or page; OpenAI-compatible providers now get one request per 64 texts (a 300-page PDF
  was 301 requests, now 5). Ollama keeps one request per text so older versions still work.

## [7.41.0] - 2026-11-12

### Added

- **Turkish religious holidays for 2030-2050:** the holiday table behind `WORKDAY`/`NETWORKDAYS`
  `"TR"` and the deadline calculator ended in 2029, so later Ramazan and Kurban Bayramı days were
  not counted. 2030-2050 are now listed, calculated with the rule Diyanet has used since 2016
  (International Hijri Calendar Union Congress); the calculation reproduces all twenty 2020-2029
  Diyanet dates (method in `docs/tr-holidays.md`). A year can hold two feasts of one kind
  (Ramazan Bayramı falls twice in 2033). The deadline calculator still asks you to check the last
  day after 2029, because those years are calculated, not published.
- **Criminal periods in the judicial recess:** the deadline calculator has a "Criminal case (CMK)"
  option: a period ending in the recess is extended by three days, to 3 September (CMK 331/4),
  instead of the civil week to 7 September (HMK 104).

## [7.40.0] - 2026-11-12

### Fixed

- **ODP presentations lost their slide masters:** OmniOffice wrote every ODP deck with a single
  "Default" master page and did not read master pages back, so a deck's masters, their
  backgrounds and their shapes disappeared after a save to ODP. Every master is now written as
  its own master page, each slide names the master it uses, and master pages with their
  background colour and shapes are read back. Layouts still travel only in PPTX; plain
  single-master ODP files are read and written as before.

## [7.39.0] - 2026-11-11

### Added

- **Spilled formulas are saved as Excel dynamic arrays:** a formula that spills in Calc
  (`SEQUENCE`, `FILTER`, `SORT`, `UNIQUE`, ...) is written to XLSX the way Excel writes a dynamic
  array (`cm="1"`, `t="array"` with its spill range and an `xl/metadata.xml` part), so Excel
  spills it too instead of showing one value. Workbooks without spills are written as before.

### Fixed

- **Dynamic arrays opened from Excel showed #SPILL!:** Excel saves a spilled array as its formula
  plus cached values in the rest of the range; those values were imported as ordinary cells and
  blocked Calc's own spill. The cached cells are now dropped on import, so the formula spills
  again. Legacy multi-cell array formulas are handled the same way.

## [7.38.0] - 2026-11-11

### Added

- **Edit a Calc chart after inserting it:** each chart has an Edit button next to Delete. Chart
  properties sets the title, the kind, the legend, the category range and each series' name and
  range (a range that is not a cell reference blocks Save), the doughnut hole and the scatter
  style. Until now an inserted chart could only be deleted.
- **Move and resize Calc charts:** drag a chart by its title bar and it snaps to the cell under
  its corner; drag the bottom-right handle to resize it (200x140 to 2400x1600 px).
- **Stacked Calc charts, value labels and axis titles:** column, bar and area charts can stack
  their series, every chart can show its values, and cartesian charts take a category and a
  value axis title. The XLSX and ODS files already carried these settings; Calc could not set or
  draw them.

### Changed

- **The command palette and global search load on first open:** the app's startup chunk drops
  from 58.7 to 57.6 KB gzip.

### Fixed

- **A Calc chart's legend could hang below its box:** the box now measures its title bar and
  legend instead of assuming their height (the buttons are larger on touch screens).

## [7.37.0] - 2026-11-11

### Changed

- **Long jobs use the typed command bindings (internal):** the 39 jobs the Jobs screen can
  retry (AI tasks, the vault scan and every PDF job) now call their generated tauri-specta
  wrapper, so a request or result that disagrees with the Rust command fails the TypeScript
  build. They are still recorded for retry under the same command name, which a test checks
  for each one. The older ts-rs bindings, `npm run types:gen` / `types:check` and their CI step
  are removed; the specta bindings (`npm run bindings:gen`) are the only generated types.

### Fixed

- **Two front-end types matched Rust again:** ink annotation strokes are lists of `[x, y]`
  points, and a compare result's `kind` is a plain string. Neither changed what the app did.

## [7.36.0] - 2026-11-11

### Changed

- **Calc editor split into components (internal):** the formula bar, the column and row
  headers, the editor drawn inside a cell while typing, and the rules a structured table
  applies to its cells (header fill, banding, totals row) now live in their own files with
  their own tests. `CalcEditor.tsx` went from 2541 to 2453 lines; no behaviour change.

## [7.35.0] - 2026-11-10

### Changed

- **Writer and Impress editors split into components (internal):** Writer's Find & replace
  dialog, its comments, review, spelling and official-letter panes, and its cross-reference
  and sections dialogs; Impress's presenter panel and slideshow bar, its Transitions,
  Animations and View ribbon tabs, the slide settings, notes and animation list, and the
  selected object's geometry and text tools now live in their own files with their own
  tests. `WriterEditor.tsx` went from 3252 to 2823 lines and `ImpressEditor.tsx` from 2777
  to 2422; no behaviour change.

## [7.34.0] - 2026-11-10

### Fixed

- **Calc undo and redo ran inside a React state updater:** the hook applied the previous
  workbook and pushed the redo entry from within `setUndoStack`'s updater, which React may
  call twice (it always does under StrictMode, in development builds). One undo could then
  be applied twice and leave two redo entries. Calc and Impress now share one snapshot
  history whose updaters are pure; Writer keeps its own operation-based history.

### Changed

- **App shell split (internal):** the lazily loaded screens live in one registry, the
  navigation icons are a module constant instead of an object rebuilt on every render,
  and the overlays both shells repeated render from one host. `App.tsx` went from 1009
  to 789 lines; no behaviour change.

## [7.33.1] - 2026-11-10

### Added

- **Accent colour:** Settings > Appearance offers indigo, blue, teal and pink, each with a
  light and a dark variant. The midnight and paper themes keep their own colours.

### Fixed

- **Text on filled accent buttons was hard to read in three themes:** the dark theme put
  white on its light purple (3.2:1), midnight put light blue on light blue (2.1:1) and
  paper put dark brown on brown (2.1:1). Each now has a text colour that reads at 4.5:1 or
  better. Paper's secondary text was 4.2:1 on its ground and is darker now. Calc's
  formula hints marked the active argument and suggestion in white on a pale tint.
- A test now checks body, secondary, accent and document-kind colours in every theme and
  accent against WCAG AA (4.5:1), and the desktop smoke run opens a sidebar hub and the
  Tools hub.

## [7.33.0] - 2026-11-10

### Changed

- **Phone editors keep their tools in thumb reach:** while the on-screen keyboard is up,
  the ribbon of Writer, Calc and Impress leaves the top of the screen and floats just
  above the keyboard as one scrolling row of tools; the page scrolls clear of it. Tool
  buttons are 44 px on a touch screen.
- **Editor shell in the new look (Writer, Calc, Impress):** the ribbon is one row of
  tabs with an accent underline over a light tool row, pressed tools are tonal, tool
  buttons are 32 px, and the status bar and Writer's outline pane follow the same
  surfaces.

### Fixed

- **Pressed and selected items were white on a light tint in the light theme:** a pressed
  ribbon toggle (bold, lists, views), Calc's active sheet tab and selected row/column
  headers, active chips, the recovery banner and template icons used the white
  `--accent-text`. They now use the accent's ink. Writer's footnote markers were white on
  the white page and could not be seen; they use the accent colour. The paper and midnight
  themes get their own ink colour.

## [7.32.1] - 2026-11-10

### Changed

- **Documents, Spreadsheets and Presentations open with Home's recent-files table:** each
  screen lists only its own kind, with the kind's colour, the tool and a relative time,
  and offers a blank file, a template or Open. It used to be a plain list of names.
- **Calm section labels:** the letter-spaced capitals left from the old look (Home's tool
  groups, the Reader's side panel, Organize, Sync, tables, the command palette and the AI
  dialogs) are sentence case now. CSS upper-casing also turned the Turkish "i" into "I"
  outside a `lang="tr"` subtree; a test keeps them from coming back.
- **New screenshots** of Home on Windows and of Home and the Tools hub on Android.

## [7.32.0] - 2026-11-10

### Added

- **Tools hub:** every tool as a tile, grouped (Office, PDF & documents, Productivity,
  AI), with a search field and one chip per group. It is the phone's **Tools** tab (which
  lights up on every tool screen) and a sidebar entry on the desktop. The scanner leads
  the PDF group. Fourteen screens that had no card on Home - the scanner, the petition
  and official letter wizards, the deadline calculator, the evidence folder, invoices,
  e-mail files, mail merge, recipes, unlocking, document info and the AI task packs among
  them - now have one, and a test keeps every screen carded.

### Changed

- **Phone tab bar:** Home, Office, **Tools**, Assistant and More; Scan is the first tile
  of the Tools hub and stays in the drawer.
- **Recent files on Home are a table you can filter:** each row shows the document kind's
  colour, the tool the file was last used with and how long ago ("12 min ago",
  "yesterday"); chips narrow it to documents, spreadsheets, presentations or PDFs, and the
  whole row opens the file. The drop zone moves to a compact column beside it, with a card
  for the job running in the background.

## [7.31.0] - 2026-11-10

### Changed

- **A new interface for Windows and Android:** a calmer look built on one design
  language for both platforms - a grey ground, a single indigo accent instead of
  gradient washes, 12 px corners, softer shadows and fixed colours per document kind
  (documents blue, spreadsheets green, presentations orange). On the desktop the screen
  sits on a raised panel beside the sidebar.
- **The sidebar is grouped into hubs:** the ~55 flat entries now live under Documents
  & forms, Productivity, Scan & convert, PDF tools, Security, AI and System, which open
  and close. The hub that holds the current screen is always open, and closed entries
  stay reachable by keyboard shortcuts and the command palette. The sidebar also gains a
  **New** button (blank document, spreadsheet, presentation or a template), a search
  field (Ctrl+Shift+F) and your pinned tools; the phone drawer shares the same hubs.
- **Home starts with a greeting and a Create row:** one click opens a blank document,
  spreadsheet or presentation, or the template gallery - it used to take Office and the
  tab bar's small icons.
- **Phone tab bar:** Home, Office, Scan, **Assistant** and More, with a Material-style
  pill on the active tab. The reader opens from a file and stays in the drawer.

## [7.30.5] - 2026-11-10

### Fixed

- **A radio group can be filled by the name it shows:** `/Opt` gives a group the option
  names a form displays while the appearance states keep their own names (ISO 32000-1
  12.7.4.2.4, Table 227) - the states are `/0` and `/1`, the labels *Elma* and *Armut*.
  Filling by the label was refused as an unknown state, so a form that reads correctly on
  screen could not be filled by what it says. The label is resolved through the group's own
  option list before the state is written.


## [7.30.4] - 2026-11-09

### Fixed

- **Filling a checkbox with a state the document does not declare is refused:** the fill
  used to write the first declared state and continue, so the file ended up with a `/V`
  that claimed one thing while `/AS` named a state with no appearance stream - a control
  that renders blank, or shows a state the document does not hold (ISO 32000-1 12.5.5,
  Table 168). The call now returns an error and writes nothing, for checkboxes and radio
  groups alike.

  The three behaviours that were already right are pinned by the same probe: the on state
  the document declares goes to both `/V` and `/AS`, clearing writes `Off` on both, and a
  document that spells its state `/Yes` keeps it.


## [7.30.3] - 2026-11-09

### Fixed

- **A deleted page could still be reached through an action:** an annotation, a field
  or the catalog can hang additional actions on events (`/AA`, ISO 32000-1 12.6.3) and
  an action can carry a chain of steps (`/Next`, 12.6.2). A step that jumped to the
  deleted page was left in place, which kept the page object - and its content - in the
  file. The step is removed now, and only it: an action whose own destination is gone
  goes with it, and the rest of the chain stays.
- **An outline item no longer names a structure element of the deleted page:** its `/SE`
  (12.3.3) is cleared while its destination stays.
- **Page labels follow the pages that are left:** `/PageLabels` indexes positions
  (12.4.2), so deleting a page shifted every label after it - a document numbered
  i, ii, 1, 2, 3 read i, ii, 1, 2 afterwards. Each range now starts at the first page
  that survived it and its `/St` moves by what was removed before it.

  Files produced before this version keep the old content behind the action chain;
  delete the page again to get a file that no longer carries it.


## [7.30.2] - 2026-11-09

### Fixed

- **Deleting a page now really deletes it.** Dropping a page from the page tree left
  everything that named it behind: a link's destination, an outline entry, the
  catalog's `/OpenAction`, the `/Names /Dests` tree, a structure element's `/Pg` with
  its parent-tree key, a form field whose widget sat on the page, and an article
  thread's bead. A reader then offered links that opened onto nothing - and, because a
  reference keeps an object alive, **the content of the deleted page was still in the
  file**. Every one of those is pruned now, before the file is written, and an outline
  entry keeps its title so the reading order survives. The same cleanup covers
  extracting a page range, where the pages left out were previously still reachable.

  **Files produced before this version keep the old content:** delete the page again
  (or re-extract) to get a file that no longer carries it.


## [7.30.1] - 2026-11-09

### Fixed

- **A permission restriction now actually binds:** a PDF reader verifies the owner
  password before the user password, so an owner password that is empty or equal to
  the user password makes whoever opens the file its owner as well - and every
  permission the file declares becomes advisory. The security screen sent the user
  password as the owner whenever that field was left blank, which is how a file is
  normally protected, so "cannot be copied" was often not true. Protecting now
  refuses that combination when at least one permission is denied, and the screen
  says why instead of filling the field in.

  **Documents protected before this version are not repaired by it:** protect them
  again with a separate owner password, and remember that copies already handed out
  keep the old, weaker setting.


## [7.30.0] - 2026-11-09

### Added

- **The health check reports links that go nowhere:** a `/Link` annotation's
  destination is resolved through the page tree and through the document's own name
  tree (ISO 32000-1 12.3.2). One that names a page the document no longer has earns
  a `link.broken-internal` warning, and web links are counted as `link.external`.
  When a producer writes both an action and a destination, the action is what a
  reader runs (12.6.2), so only that one is counted.
- **The health check repeats the restrictions a document declares:** the `/P` bits
  of the file's own `/Encrypt` dictionary (7.6.4.2) are read from the bytes - a PDF
  library decrypts on load and drops that dictionary - and the report names what the
  document denies, such as printing or copying. It is the document's statement about
  itself; this app neither sets nor enforces it.


## [7.29.1] - 2026-11-09

### Fixed

- **A second stamp no longer draws the first one in the wrong font:** a stamp, a
  page number or an evidence bundle label registers its font in the page as
  `/OmniTr`. A page that already carried one - every file this app has stamped
  before - had that resource overwritten, so the earlier text was drawn with the
  new, much smaller subset: missing glyphs or a run of boxes, with nothing to warn
  the user. Registration now takes the next free name (ISO 32000-1 7.8.3) and the
  drawn text refers to the name it was given, so the earlier stamp keeps the font
  it was written with. Exhibits in a bundle keep their own resources the same way,
  including an inherited `/Font` and a content stream shared by two pages.


## [7.29.0] - 2026-11-08

### Fixed

- **An annotation lands where the viewer shows it:** a mark is now placed against
  the region a PDF viewer shows - the crop box clipped to the media box
  (ISO 32000-1 14.11.2) - so a cropped page no longer shifts every note and
  highlight by the crop offset. The bake path (`annotate_pdf`, which draws stamps
  and flattened marks into the content stream) also ignored the box origin
  entirely: on a page whose box does not start at `(0, 0)` the mark moved with
  the origin, and a reversed box (`[600 800 0 0]`, 7.9.5) put it outside the
  page. Both paths now size and place the mark from that same box.

## [7.28.0] - 2026-11-08

### Fixed

- **An annotation lands where it was placed on a page that does not start at the
  origin:** the display-space rectangle the annotation screen sends was converted
  into page coordinates as if every page began at `(0, 0)`. A page whose
  `/MediaBox` starts elsewhere - the shape a crop or an imposition produces, and
  one of this app's own resize outputs - put the mark that far off, so a note or a
  highlight appeared away from where it was drawn. The page's own origin is carried
  through the conversion now, for both the editable annotations and the drawn
  stamps.
- **A reversed `/MediaBox` no longer throws the mark off the page:** a box written
  as `[595 842 0 0]`, which ISO 32000-1 7.9.5 allows, was measured with a negative
  width, so the annotation landed outside the page entirely. The box is normalised
  before it is measured.

## [7.27.0] - 2026-11-08

### Fixed

- **The document health check no longer passes a document that has no title:** the
  `a11y.missing-title` finding tested the title the screen *shows*, which falls back
  to the file name, so a document without a `/Title` in its metadata was reported as
  fine. Conformance asks for the metadata itself (PDF/UA 7.1), and the file name is
  only what a screen reader announces when that metadata is empty. The display keeps
  its fallback; the finding now looks at the document's own title.

## [7.26.0] - 2026-11-08

### Fixed

- **Flatten no longer bakes in an annotation the viewer never shows:** an
  annotation whose `/F` carries the Hidden bit (2) was painted onto the page like
  any other, so a hidden stamp or note appeared in the flattened file that was not
  in the document the user was looking at. It is now left in place and reported in
  the flatten report instead.

## [7.25.0] - 2026-11-08

### Fixed

- **Resize keeps a page's annotations on the content they mark:** the page content
  moved through the resize matrix, but the marks on it did not, so a highlight, a
  note, an ink stroke or a line ended up somewhere else on the page. Their `/Rect`,
  `/QuadPoints`, `/InkList`, `/L` and `/Vertices` now go through the same matrix.
- **Crop lands on the right region when the page does not start at the origin:** a
  `/MediaBox` such as `[10 20 605 862]` - the shape a scan or a print-ready export
  often has - shifted the cropped region by its own origin, so the rectangle the
  user drew selected a different part of the page.

## [7.24.0] - 2026-11-08

### Changed

- **The font cache's memory budget is now measured, not estimated:** a counting
  allocator in the test suite proves that a 4 MiB budget really holds at most that
  much while imposing a document with sixty-four composite fonts, and that a cache
  which fits its fonts *lowers* the peak - on that fixture the cached run peaked at
  about 6 MB against 21 MB uncached, because reading a map per note builds and
  drops one map per note. The benchmark also gained a real-size scenario: a
  twenty-thousand-entry `/ToUnicode` map costs 1.35x a thousand-entry one with the
  cache on (223 ms against 166 ms for 2000 notes), and 13.3 s without the cache.

## [7.23.0] - 2026-11-08

### Changed

- **A simple `/DR` font is read once per document too:** `/Widths`, `/Encoding`,
  `/Differences` and the descriptor were re-read for every note, which is the same
  waste the composite path had before 7.22.0. Both kinds now share one cache, keyed
  by the font's object id - never its resource name, since two pages may call
  different fonts `/F1`. On a 500-page document with four notes per page (2000
  notes, release build, median of five runs, `cargo run --release -p pdfcore
  --example nup_bench`): a composite font went from **896 ms to 183 ms** and a
  simple font from **205 ms to 178 ms**. The benchmark compares the two outputs
  byte for byte before printing, so the cache is measured and proven invisible.

## [7.22.0] - 2026-11-08

### Changed

- **Each composite font is read once per document, not once per note:** N-up
  imposition now remembers each `/DA` font's `/ToUnicode`, `/Encoding` CMap,
  `/CIDToGIDMap` and `/W` for the length of one imposition call. A refusal is
  remembered as well, so a font that cannot be used is not re-read for every
  note either. The memory the cache may hold is capped; a font that would take it
  past the cap is used but not stored. The output is byte-identical to before.

## [7.21.0] - 2026-11-08

### Added

- **A note can be drawn with a font whose codes are one byte wide:** an embedded
  `/Encoding` CMap that declares a one-byte codespace, or one-byte and two-byte
  ranges together (the Shift-JIS shape), is now read. Each code is written at its
  own length, so a note may mix `<41>` and `<8140>`, and `/W` and `/CIDToGIDMap`
  are still looked up by CID. A codespace whose one-byte ranges overlap the lead
  byte of a two-byte code is refused - a viewer would split the string at the
  wrong place and draw the wrong glyphs with no error - as are vertical writing,
  unknown base CMaps and codes longer than two bytes; those notes fall back
  exactly as before.

## [7.20.0] - 2026-11-08

### Added

- **A note can be drawn with a font that embeds its own `/Encoding` CMap:** a
  composite font whose `/Encoding` is a stream - `cidrange`/`cidchar` on top of
  an identity or an `Identity-H` base - is now read, so its codes select the CIDs
  its `/W` is keyed by and the note keeps its face. The font is still referenced,
  never copied. Refused, and unchanged in output: a codespace that is not exactly
  two bytes per code (a one-byte or mixed font would silently shift every glyph
  after the mistake), a base CMap this build does not ship, vertical writing, and
  a character whose CID is unmapped, `.notdef`, or has no advance.

## [7.19.0] - 2026-11-08

### Added

- **A document's own `/ToUnicode` map can now be read:** the first piece of the
  composite-font work is a reader for the CMap a Type0 font uses to map its
  glyph codes back to characters (`pdfcore::cmap`). It inverts the map so text
  can be drawn the other way round, keeps the lowest code when a character is
  listed twice (so the result does not depend on entry order), and treats the
  stream as untrusted input: a broken entry costs only itself, a destination
  that is not exactly one character (a ligature, a lone surrogate) is skipped,
  placeholders and private-use codes never become drawable, and the stream is
  bounded before anything is allocated.
- **A note can be drawn in the document's own composite font:** when a free-text
  note's `/DA` font is an embedded Type0 font with `/Identity-H` codes, and
  neither WinAnsi nor the bundled face can show its text, the note is now
  printed in that font - by reference, so the font, its descriptor and its
  program are not copied into the output. Codes come from `/ToUnicode`, advances
  from `/W` and `/DW`, and a `/CIDToGIDMap` that sends a used code to glyph 0
  costs that character. Anything else is still left out rather than guessed:
  vertical writing (`/Identity-V`), predefined or embedded CMaps, a missing
  `/ToUnicode`, a non-embedded program, a malformed `/W` (which rejects the
  whole font, because dropping one entry would silently give that code the
  default width), and notes with more than 255 distinct characters.

## [7.18.0] - 2026-11-07

### Added

- **A cloudy border reaches the paper:** a square or circle annotation without
  an appearance stream that asks for `/BE << /S /C >>` now draws the revision
  cloud its dictionary describes - a chain of outward scallops around the box,
  stroked with `/C` and filled with `/IC` on the same path - with `/I` (0 to 2)
  deciding how coarse the bumps are. A shape smaller than its own bumps, a
  perimeter too short for three of them, or a non-cloudy `/S` keeps the plain
  frame, and an annotation with an appearance stream is never touched.

## [7.17.0] - 2026-11-07

### Security

- **Release assets now carry SLSA build provenance:** every desktop file the
  release publishes is named by `omnioffice-<tag>-desktop.intoto.jsonl`, which
  records the SHA-256 of each asset and the workflow run that built it, so a
  download can be verified offline with `slsa-verifier verify-artifact` (the
  command is in `SECURITY.md`). The repository already attested the same builds
  into GitHub's attestation store; this publishes the statement as a release
  asset as well, and a new `scripts/provenance-subjects.mjs` hash-then-check step
  refuses to publish an envelope when the bytes on the release differ from the
  ones the build produced. The Android packages get their own envelope from the
  workflow that builds them, so a release is covered end to end. `latest.json`
  is excluded on purpose: several jobs rewrite it, and its own updater
  signature protects it.

## [7.16.0] - 2026-11-07

### Security

- **A crafted spreadsheet can no longer take the app down:** `calamine` 0.37
  parsed the OLE2/CFB container of `.xls` and `.xlsb` files (and VBA
  extraction) with no bound on a hostile input, so a crafted file could panic
  or allocate until memory ran out - RUSTSEC-2026-0436, reachable from the
  public reader on a file the user opens. The workspace now vendors **0.38.0**,
  which fixes it, with the two arithmetic guards our own fuzzing had added to
  0.37 carried over (hostile cell references and reversed `/dimension`
  references are parse errors, not overflow panics). A new hostile-input suite
  pins that every malformed container is an error, promptly.

### Added

- **Free-text notes can use the bundled face too:** a note whose text neither
  the document's `/DR` font nor the standard font can carry is now set in a
  subset of the bundled Liberation Sans, laid out with that face's own advance
  widths, so Cyrillic, Greek and the wider Latin ranges reach the paper instead
  of the note being dropped. The layout keeps working in one-byte codes: the
  note's characters are mapped to dense slots (the space keeps `0x20`, which
  the wrapper writes between words) and only the emission turns slots back into
  the face's two-byte glyph ids. Text no available face can show still drops
  the note, and notes the document font or WinAnsi can already carry are
  byte-identical.

## [7.15.0] - 2026-11-07

### Added

- **A line caption the standard font cannot encode is set in the bundled
  face:** caption text outside WinAnsi used to be dropped from the imposition.
  It is now set in a subset of the bundled Liberation Sans written for exactly
  the characters it draws (Identity-H with a `ToUnicode` map, described in
  `textfont`), so Cyrillic, Greek and the wider Latin ranges reach the paper;
  WinAnsi text keeps the standard font and its bytes are unchanged. Text the
  bundled face cannot show is still left out rather than printed as replacement
  glyphs, and one subset font is shared by every annotation on a sheet that
  needs the same characters. Free-text notes still use the standard font and
  the document's own `/DR` font.

## [7.14.0] - 2026-11-07

### Added

- **A line's caption survives an imposition:** a `Line` annotation with
  `/Cap true` and no appearance stream used to lose the text it carries. The
  synthesized appearance now sets it - `/CP /Inline` (the default) puts the
  caption on the line and breaks the stroke around it, `/CP /Top` puts it above,
  and `/CO` moves it along and across the line - in Helvetica, filled with the
  line's own colour, with the appearance box grown to contain it. The caption is
  one line of `/Contents`; a character the standard font cannot show means no
  caption at all rather than replacement glyphs, and an annotation that already
  has an appearance stream is never touched.

## [7.13.0] - 2026-11-07

### Security

- **LAN requests can bind the range they ask for:** the request signature has a
  second version. Version 1 signs `device_id`, timestamp, nonce, operation,
  path and the body hash and is unchanged, so an older build keeps working.
  Version 2 keeps all of it and adds `offset`, the chunk length, `total`, the
  document `sha256`, a `transfer_id` derived from them and the receiver's device
  id, with every field length-prefixed, so the MAC of a resumable transfer now
  says which range of which document version was requested. The two ends agree
  on the version while pairing (`features: ["sig-v2"]`, stored on the peer) and
  the version travels per request, so `protocol_version` does not move and a
  new build still talks to an older peer. The first accepted version 2
  request from a peer is pinned (`min_sig = 2`) and a later version 1 request
  from it is refused as `sig-downgrade`; a refused request now names its kind
  (`bad-signature`, `sig-downgrade`, `range-mismatch`) without ever saying which
  field failed. `get_chunk` sends the document hash it is assembling, and the
  host checks the range against the file it holds.

### Changed

- **LAN sync: a pair formed before the request signature v2 upgrades itself.**
  Until both records know the capability the client sends the version 1 tag in
  `mac` and the version 2 tag in a `sig_v2` sidecar; a host that verifies the
  sidecar pins the pair (`min_sig = 2`) and answers with version 2 alone, so
  the other end pins it in turn and the pair converges in one round trip. No
  re-pairing is needed, a device on an older build keeps being answered on
  version 1, and a version 1 request from an upgraded peer is refused. The
  first offer can still be dropped by an attacker on the network, which keeps
  that pair on version 1 until it is re-paired.

- **A version cannot be tagged without its release notes:** a new
  `scripts/release-guard.mjs` runs in pull-request CI, in the auto-tag job
  before the tag is created, and as the first step of the release and Android
  builds. It fails when the `## [X.Y.Z]` CHANGELOG section is missing or empty,
  when `fastlane/metadata/android/en-US/changelogs/<versionCode>.txt` is
  missing or empty, or when `package.json`, `tauri.conf.json`, `Cargo.toml` and
  the Android metadata disagree (the tag has to match the version too). The
  v7.12.0 release was tagged without both files, the Android publish job failed
  with the tag already pushed and the release page was never created, so the
  tag had to be moved with a force push; this makes that mistake cost a red
  pull request instead.

## [7.12.0] - 2026-11-07

### Added

- **Border styles arrive in N-up:** a synthesized frame now honours `/BS /S`:
  `B` bevels it with a light top/left and a dark bottom/right (tones derived
  from the stroke colour, a quarter toward white or black), `I` swaps those
  tones so the frame looks sunk, and `U` keeps only the bottom edge the way a
  field underline is drawn. Squares, free-text notes, circles (two half-arcs)
  and polygons (one tone per edge) follow; a shape too small for a band falls
  back to the plain frame, and a line, polyline or ink stroke has no inside to
  bevel, so it stays plain. The bands are painted inside the frame, so the
  form's box does not grow.
- **A line with a leader is drawn instead of dropped:** a `Line` annotation
  whose `/LL` moves it beside its anchors used to disappear from an imposition,
  because a viewer draws the shifted line plus a short leader from each anchor
  and the synthesized appearance did not. It does now: `/LL` sets the sideways
  offset (negative flips the side), `/LLE` runs the leader past the line and
  `/LLO` leaves a gap at the anchor, `/LE` endings attach to the shifted line,
  and the box grows to cover the leaders. A dashed line dashes its leaders too.

## [7.11.0] - 2026-11-07

### Added

- **Lines and callouts get their ending marks in N-up:** a `Line` or `PolyLine`
  without an appearance stream now carries its `/LE` ends - open and closed
  arrows in both directions, squares, circles, diamonds, a butt cap and a slash
  - drawn at the line's own width and filled with `/IC` when the annotation has
  an interior colour. A free-text callout honours its single `/LE` at the point
  it points to. Each mark is painted solid even on a dashed line, and the form's
  box grows so a thick arrow head is never clipped.
- **Polygons, polylines and dashed borders arrive in N-up:** a `Polygon` or
  `PolyLine` without an appearance stream is drawn from its `/Vertices` now -
  the closed path filled with `/IC` and stroked with `/C`, the open one only
  stroked, exactly as the annotation describes. A border can also be dashed:
  `/BS << /S /D /D [...] >>` writes the `d` operator before the shape, the
  specification's `[3]` applies when the pattern is missing, and a malformed
  pattern falls back to a solid line rather than losing the annotation. A solid
  style still ignores a stray `/D`, and a legacy `/Border`'s fourth entry works
  as the pattern too.
- **Free-text notes reach the paper in N-up:** a note without an appearance
  stream used to disappear, taking the user's own words with it. The text is
  laid out now with a standard font referenced by name - Helvetica, Times or
  Courier with the bold and italic members, chosen from `/DA` and the form's
  `/DR` - and the glyphs WinAnsi lacks are declared through a fixed
  `/Differences` table, so Turkish and the other Latin-A letters print
  correctly without embedding a font program. `/Q` alignment, a `/C`
  background, the `/BS` or legacy `/Border` frame, `/RD` insets and a `/CL`
  callout are honoured. A character the standard fonts cannot show drops that
  note instead of printing a guess, and an annotation that ships an `/AP` is
  still never re-synthesized.

## [7.10.0] - 2026-11-06

### Added

- **Marks without an appearance stream are drawn anyway in N-up:** many
  producers ship a highlight, a square or an ink stroke without an `/AP` stream
  and let the viewer paint it from the geometry; an imposed sheet has no viewer,
  so those marks used to disappear. The geometry is turned into operators now -
  the quad-point kinds (`Highlight` with a Multiply blend, `Underline`,
  `StrikeOut`, `Squiggly`) and the shapes (`Square`, `Circle`, `Line`, `Ink`),
  with `/C`, `/IC` and the `/BS` or legacy `/Border` width. The content is built
  in page space inside a form of its own, so the placement matrix, `/NoRotate`
  and the cell clip apply exactly as they do to an existing appearance. An
  annotation that ships an `/AP` is never re-synthesized, and malformed
  geometry drops only its own mark.

## [7.9.0] - 2026-11-06

### Added

- **Notes and highlights reach the paper in N-up:** a link survived an
  imposition since 7.8.0, but every dotted underline, sticky note and stamp
  vanished because an annotation cannot live inside the Form XObject a source
  page becomes. The kinds that are meant to be printed (fifteen of them, from
  `/Text` to `/Stamp`) are painted into the sheet's own content stream now: the
  appearance stream is referenced rather than copied, its box is fitted to the
  annotation rectangle, `/NoRotate` keeps a note upright on a rotated page, and
  an annotation that is hidden or not marked for printing stays behind. Widgets
  and redactions are left alone on purpose - half-flattening a form can print
  the wrong value, and a flattened redaction would look redacted while the text
  is still there.
- **Filled form fields reach the paper too:** a widget is painted from its
  appearance stream when the field type says the appearance can be trusted - a
  button's `/AS` state, a text field the producer kept current, a signature
  field that actually carries a value. `NeedAppearances` holds back text and
  choice fields, because their stored appearance may predate the value, and an
  unsigned signature field stays a placeholder instead of printing as one. The
  field type is inherited through the `/Parent` chain, a cycle in it is cut, and
  a widget without the print flag is skipped.

## [7.8.0] - 2026-11-06

### Added

- **N-up and booklet sheets keep their links:** imposing a document used to drop
  every link, because the imposition rebuilds the page tree and an annotation
  cannot live inside the Form XObject a source page becomes. Link annotations are
  copied onto the sheet now: the rectangle is mapped through the placement
  matrix (source rotation included) and clipped to the cell, an external target
  is kept as it was, an internal one is rewritten to the sheet page it points at,
  and a named destination is resolved into an explicit array so the output no
  longer depends on the source name tree. Other annotations are still dropped on
  purpose.
- **N-up keeps its bookmarks too:** the outline is rebuilt on the sheet pages
  with its hierarchy, colours, flags and the signed `/Count` rule, so an entry
  that pointed at page 7 points at the sheet carrying page 7 now. A named
  destination resolves the same way a link's does, and an entry whose page fell
  outside the selection loses its target - or disappears when it has no child to
  keep it. A source outline that cannot reach any sheet is removed instead of
  left dangling.
- **A LAN download resumes where it stopped:** uploads have resumed since D3, but
  a download was one request and started over when the Wi-Fi dropped. The
  protocol gains `begin_get` and `get_chunk`; the partial file carries a note
  with the hash it belongs to, the assembled document is verified before it
  reaches the caller, and a peer from before this change falls back to the
  whole-file transfer.

## [7.7.0] - 2026-11-04

### Fixed

- **A toast no longer covers the bottom bar (6.5.1):** the stack sat 16 px from
  the viewport edge, so on a phone it covered the navigation bar and reached
  into the system gesture area - the last open layout defect of the 6.5.1 device
  check. It clears both now, and the stack is click-through, so a full-width
  toast cannot swallow a tap meant for the screen behind it.
- **The last four strings the expression scan found are translated (E4.7):** the
  Impress empty-image hint, the autosave durations (`30 s` / `5 min`), the
  signature panel's "Yes / Off" default and the "Page 1" page-number example.

### Changed

- **One password field instead of two:** the Protect/Unlock panels and the
  converter carried their own copies of the same component and had already
  drifted on `autoComplete`. They share one implementation now; the behaviour
  both copies relied on - the toggle never submits the form, and it is labelled
  in the interface language - is pinned by a test file of its own. The two
  screens lost 22 and 34 lines.

### Developer notes

- **Deliberate literals can be declared on the JSX side too (E4.7):** a
  `// i18n-literal: reason` comment silences one finding that must not be
  translated (a unit, a shortcut, a formula example), with the same discipline
  as the object scan's `i18n-content:`. The reason must be real, a stale or
  ambiguous marker fails the run, and the total is ratcheted plus hard-capped at
  five. The baseline schema is v5 and stands at 58 text / 12 attributes / 6
  props / 0 object strings / 3 declared literals.

## [7.6.0] - Ölçülen güvenlik (Phase E / E5 ve E4 kalanları)

### Changed

- **Sixteen hardcoded strings moved into the dictionary (E4.7):** the password
  visibility toggle spoke English inside the Turkish interface, `History` wrote
  its date header as a conditional with the same word in both arms,
  `PdfStudio`'s "no private key" and `Ai`'s citation fallback were literals, and
  `Merge`, `Ocr` and `Compress` chose their text with a
  `language === "tr" ? … : …` ternary instead of a key. All of them go through
  `t()` now, with the Turkish wording unchanged and pinned key by key by a
  contract test. The petition subtitle had been showing the Turkish sentence to
  English users; it has one key per product state now (`experimentalUdf` on or
  off). Baseline 73/20/8 -> 64/15/6.
- **The release and beta trains stop re-downloading the engines (E5):** both
  restore `src-tauri/resources/engines` from the entry the Desktop run stored on
  the default branch and never write one themselves, so a publishing run cannot
  seed a cache another run trusts. The key covers the lock file and the fetch
  script, because the script trusts files that are already present. The nightly
  stays uncached on purpose: it is the canary for the mutable `main` URLs in the
  lock, and a cached copy would hide exactly that drift.
- **Workflow write permissions moved to the job that uses them (E5):** Scorecard
  scored Token-Permissions at 0 because five workflows granted the repository
  token's write scopes to every job. They default to `contents: read` now and the
  publishing job asks for what it writes; the effective scopes are unchanged
  everywhere except `release/version-consistency`, which never wrote anything.
  The score went to 10.
- **The tag job reads the release metadata instead of checking it out (E5):**
  `auto-tag.yml` checked out the triggering run's commit to read four version
  files - the shape of a fork-code-with-secrets attack, and a write-scoped token
  sitting in a working tree. It fetches those four files from the commit it is
  about to tag and runs with no checkout at all; the version gate and the tag are
  guaranteed to describe one commit. Dangerous-Workflow went from 0 to 10 and the
  total score from 5.3 to 6.5.

### Added

- **OpenSSF Scorecard grades the repository (E5):** a weekly run, a run on every
  push to master, a run when a branch-protection rule changes and a dispatch
  analyse the project's own security posture, upload the SARIF report to code
  scanning and publish the score. The action refuses every event that is neither
  a pull request nor the default branch, so a `pull_request` trigger filtered to
  the workflow file is what lets the change be proven before it merges. The first
  measured score was 4.6.
- **The i18n audit reads expressions (E4.7):** a string used to count only when
  it was written directly in a JSX text node or as a plain attribute value, so
  `label={flag ? "Kaydet" : "İptal"}`, `<p>{ok ? "Tamam" : "Hata"}</p>`,
  `name ?? "Adsız"` and the words inside `` `${n} ${n === 1 ? "Sayfa" :
  "Sayfalar"}` `` were invisible. One recursive rule now reads them, with two
  tested exclusions: the condition itself (`mode === "edit"` is a comparison, not
  prose) and `+` concatenation. `--expressions-report` lists what the rule added
  and where it came from, so the measurement is reviewable. Files 50 -> 73,
  attributes 11 -> 20, props 6 -> 8; the object section stayed at zero.

### Fixed

- **`writer.pageBreak` was defined twice in both tables:** the second definition
  won, so the ribbon button asking for it rendered "— page break —" instead of
  "Page break". The marker has its own key (`writer.pageBreakMarker`).
- **`--expressions-report` printed only half its rows:** the flag was handled
  twice and the first handler exited before the object scan ran, so the object
  side's expression strings never reached the report.

### Notes

- The Windows installers remain **unsigned by decision** (`docs/signing.md`,
  D10): SmartScreen shows "unknown publisher". Verify a download against
  `SHA256SUMS-windows.txt`. Scorecard's Signed-Releases check reads 8/10 for
  that reason.
- Security-Policy reads 4/10: `SECURITY.md` is detected but carries no link, and
  GitHub's private vulnerability reporting is off, so the advisories link would
  404. Turning it on is a repository-settings step, and the link follows it.
- Moved to 7.7.0: the duplicated `PasswordField` component (two screens carry
  their own copy), a JSX-side ignore marker for deliberate literals such as the
  formula placeholder in `TablesUi`, and the scanner gaps that remain (a string
  arriving through a variable or a return value).

## [7.5.0] - Uçtan uca kanıt (Phase E / E4 kalanları, E5)

### Added

- **Seven more end-to-end flows drive the real UI (E4.3):** the smoke script
  became modules - a 109-line entry point that parses flags and dispatches, the
  WebDriver plumbing in `e2e/lib/driver.mjs`, the sample copies in
  `e2e/lib/files.mjs` and one module per flow in `e2e/flows/` (#577) - and every
  one of them runs in the Linux tauri-driver job on every pull request:
  signature verification on an unsigned document (#578), the Calc formula engine
  through `data-cell` hooks (#580), the three-tier secret store across a real
  process restart including the `0600` fallback file (#581), Impress (select a
  slide, add one, save the PPTX) (#582), the AI task-pack pipeline against
  `scripts/mock-deepseek.mjs` on loopback (#583), the scanner's photo round trip
  with the detector finding a page in one frame and nothing on a bare wall
  (#596), and the vault, from a seeded folder through the real index to a
  highlighted hit and its preview (#603). No flow asserts a translated string:
  they read `data-testid`, `data-*` hooks and the files on disk.
- **Rust coverage is measured, published and enforced (E4.1):** a
  `rust-coverage` job runs `cargo llvm-cov --lib` over the five library crates
  and uploads `coverage-rust` (#576); `scripts/check-rust-coverage.mjs` then
  compares the `TOTAL` row with `scripts/coverage-baseline.json`, and `--update`
  refuses to lower a floor (#584). The floors are 54/55/55 against the measured
  55.23 regions / 56.37 functions / 56.30 lines, so run-to-run noise cannot
  redden a green suite while a lost test still fails the job.
- **The Android artifacts are verified before anything is published (E5):** each
  APK and AAB is read as a file - it carries only its own ABI's `lib/`
  directory with the three native libraries, the libraries are the right ELF
  shape, both APKs share one signing certificate, and the checksum list matches
  the files next to it (#597). The same change put
  `SHA256SUMS-android.txt` into the uploaded run artifact, which had been
  attached to the release while missing from the artifact, so nobody
  downloading a run could verify a build.
- **Android CI runs on demand and in parallel (E5):** adding the `android-ci`
  label to a pull request runs the dispatched build (#598), and the version
  moved to a two-ABI matrix with one `assemble` job that verifies and publishes
  (#599). The tagged dispatch went from 40m21s to 20m44s while both arms started
  on the same second, and the emulator arm now builds only the libraries it
  runs.
- **The i18n audit reaches the text JSX attributes and modules hid (E4.5-E4.8):**
  the scanner moved out of `scripts/i18n-audit.mjs` into
  `scripts/lib/i18n-jsx.mjs` and counts component `label` props (#585, #586),
  `hint` and `subtitle` (#587), text held in object literals and `.ts` modules
  (#588), toast titles (#590) and `detail` lines (#602), and `${...}` template
  attributes, whose plain words decide whether they count (#605). The baseline
  schema is v4 - `files`, `attributes`, `props`, `objects` - and the measured
  totals moved from 80 to 50 hardcoded text nodes, from 25 to 6 props, from 34
  to 0 object strings, and from 9 to 11 attributes (that last one rose because
  more is measured, not because text was added).

### Changed

- **Document-health findings are keys with parameters (E4.7):** a `.ts` module
  stopped composing sentences and returns `titleKey`/`descriptionKey` plus the
  numbers a sentence needs, so the screen renders `t(key, params)` and the text
  follows a language change instead of freezing at the first render (#589). A
  golden test pins that all nine findings read exactly as before in Turkish.
- **`KvkkWizard`, Vault and Invoices speak both languages (E4.5):** the KVKK
  screen's law paragraph, the vault paging and semantic-index controls and the
  KDV amounts moved into the tables (#589, #601), and six English sentences
  followed them out of the Turkish interface (#604) - including the certificate
  toast, which the scanner could not see because its plain fragments carried no
  Turkish-specific letter.
- `file-size-baseline.json` grew to a documented budget for the two files the
  model work enlarged.

### Fixed

- **The KDV amounts print two decimals (#601):** sixteen hand-written `} TRY`
  suffixes now go through one `money()` helper, and it passes `{minimumFractionDigits: 2,
  maximumFractionDigits: 2}` - `toLocaleString`'s default of three decimals
  could print `1.500,005`.
- **`KvkkWizard` printed the key instead of the paragraph (#589):** the screen
  rendered the literal `kvkk.lawBody` text rather than the sentence behind it.
- **Two template attributes were English in the Turkish interface (#605):**
  `Source page ${n}` became `Kaynak sayfa {page}` and `${rows} by ${cols}` became
  `{rows} x {cols}`, matching how the size reads in Turkish.
- **A page-number format string claimed a file count (#604):** the batch header
  said "N sayfa" while counting files.
- **The fastlane check named the wrong "current" version (#606):** it treated the
  last CHANGELOG entry as the newest release, so `fastlane:check` compared the
  translated locales against 6.6.0 and a missing `tr-TR/changelogs/<code>.txt`
  never failed. It now picks the highest version, and 7.5.0's Turkish store
  notes exist.
- **`RUSTSEC-2026-0429` is documented and 7-Zip comes from its pinned
  release (#579):** the advisory in the updater's `zip 4.6.1` is recorded in
  `deny.toml` with its reason (calamine 9.0.5 is patched), and the 7-Zip
  download moved off `www.7-zip.org`, which stopped resolving, onto the GitHub
  release asset with the same sha256.

### Notes

- The Windows installers remain **unsigned by decision** (`docs/signing.md`,
  D10): SmartScreen shows "unknown publisher". Verify a download against
  `SHA256SUMS-windows.txt`.
- The Android release path - two matrix arms signing separately, `assemble`
  comparing the identities - was exercised by a signed dispatch on master
  (`ABI matrix OK: 4 artifacts, two ABIs, one signing identity`) before this
  release; the three tag-only steps still meet a `v*` tag for the first time
  here.
- Moved to 7.6.0: the engine cache in `release.yml`/`beta.yml`/nightly, the
  OpenSSF Scorecard and Renovate decision, the scanner gaps that remain (JSX
  conditional branches, `${...}` inside object literals) and the fuzz extension.

## [7.4.0] - Kalite hattı (Phase E / E1 kalanları, E4, E5)

### Added

- **The panic-free ratchet covers every crate (E1.7, #550):** production code in
  `turkish`, `pdfcore`, `aicore`, `officecore`, `synccore` and the `compat` CLI
  now denies `clippy::unwrap_used` and `clippy::expect_used`. The 55 sites the
  attribute flagged became typed errors (the XML/DAV parser stacks, the WebDAV
  verb helpers, DOCX tracked changes, the layout page accessor, CLI flag
  parsing), explicit fallbacks, or `if let` guards; two calls are infallible by
  construction and stay as documented `#[expect]` (the embedded PDF face, the
  HMAC key). Test modules keep their freedom.
- **Coverage gate at the measured suite (E4.1):** the vitest floors move from
  62/69/46/62 to 77/79/64/77 against a measured 78.4/80.2/65.9/78.4, the report
  is published as the `coverage-frontend` artifact, and a workflow test refuses
  to let a floor drop below the measurement.
- **The i18n audit has teeth (E4.5):** `npm run i18n:audit -- --check` now fails
  on a `{placeholder}` set that differs between English and Turkish, on more
  user-visible JSX text than `scripts/i18n-baseline.json` records (measured with
  the TypeScript parser, a ratchet that may only shrink), on a value that spells
  a plural as `(s)`, and on a `t.plural` key missing a variant. Values identical
  in both languages and keys no literal call reaches are advisory.
- **Counts go through `Intl.PluralRules`:** `t.plural(key, count, params)` picks
  `key.one`/`key.other` with the language's own rules; the 22 `(s)` strings and
  their 15 call sites moved over. Turkish keeps one sentence for every count
  because the noun is not pluralised after a numeral.
- **Five screens speak both languages:** `DocumentHealthScreen`,
  `PetitionWizard`, `Invoices` (the KDV declaration modal and the duplicate
  banner), `KvkkWizard` and `Organize` moved their strings into the tables, plus
  the dialog `aria-label`, the thumbnail-size label and three `alt="Page N"`
  texts. The hardcoded-text baseline fell 131 -> 80.
- **Pinned toolchain and runtime (E5):** `rust-toolchain.toml` declares the
  stable channel with clippy/rustfmt (the MSRV job overrides it to 1.89.0 and the
  fuzz jobs to nightly, so the file cannot mask those gates), `.nvmrc` says 22
  and `package.json` declares `engines.node >= 20`. Every Linux runner is
  `ubuntu-24.04` instead of `ubuntu-latest`, pinned by a test.
- **Supply chain:** `SECURITY.md` documents the private advisory channel,
  supported versions and the deliberate limits (unsigned Windows installers, ACL
  scope, third-party engines); a `CodeQL` workflow analyses
  `javascript-typescript` and the workflow files on push, pull request and a
  weekly schedule, with both uses pinned to a commit.
- **Engine downloads are cached (E5):** the workspace-test and Windows E2E jobs
  cache `src-tauri/resources/engines` under the `engines.lock.json` hash.
- **Tests for the three untested components (E4.2):** `LanSyncPlan` (6 cases),
  `ScannerPageList` (8) and `CornerPicker` (6) now pin the conflict choices, the
  spread row identity, the crop badges, the keyboard handle movement and the
  "whole photo" contract.

### Fixed

- **The accountant E2E no longer breaks when a label is translated:** it finds
  the KDV button and its panel through `data-testid` instead of searching the
  Turkish button text, which is exactly what the i18n work changed.
- **The CI job parser saw no digit-bearing job names:** `[a-z-]+` skipped `e2e`
  and `e2e-windows`, so the "every job has a timeout" rule never reached them.
- **`cargo clippy -p <crate> --lib` counts were misleading:** it re-lints the
  workspace dependencies, so `compat`'s "36" was `officecore`'s 34 printed twice.
  The real remaining total was 55.

### Notes

- The Windows installers remain **unsigned by decision** (`docs/signing.md`,
  D10): SmartScreen shows "unknown publisher". Verify a download against
  `SHA256SUMS-windows.txt`.
- `cargo llvm-cov`, the Android ABI matrix, `OpenSSF Scorecard`, the rest of
  E4.2/E4.3/E4.5 and the fuzz extension stay scheduled for v7.5.0.

## [7.3.0] - İzolasyon ve dayanıklılık (Phase E / E2)

### Added

- **Contained worker child (E2.2, #550):** the untrusted-document parser child is
  now contained before it executes. On Windows it is created suspended, assigned
  to a job object with a 1.5 GiB process-memory limit and only then resumed (the
  old code assigned the job after `spawn`), with the suspended thread resumed by
  thread id; Linux/macOS install `RLIMIT_AS` and `RLIMIT_CPU` between `fork` and
  `exec`. The child inherits an explicit environment allow-list (engine paths,
  temp directory, locale, Windows system basics) instead of the parent's
  environment, so a parser never sees API keys, provider tokens or the dev launch
  context.
- **Async engine probe and audit (E2.3, #550):** `engine_status` spawns
  `tesseract --version`, so it runs on the blocking pool now and answers
  `EngineStatus::missing()` when the probe cannot run. `docs/command-async-audit.md`
  records the classification of the command surface and the rule for new
  commands; it also corrects the Phase E proposal, which had named
  `ocr_languages` (a directory listing) as the tesseract caller.
- **Typed command errors (E2.4, #550):** the commands that returned a bare
  string now return `PdfError`, so the UI receives `{code, message}` and
  localizes by code: external links, the file/folder token picks, job status and
  retry, the updater, and the two Android bridges.

### Changed

- **`--worker` counts only as the first argument (E2.2, #550):** it used to be
  searched anywhere on the command line, so a document path containing the word
  could turn the GUI into a worker process.

## [7.2.1] - Android derleme düzeltmesi

### Fixed

- **Android release build (E1/4 follow-up, #550):** `secret/vault.rs` kept the
  keyring service constant outside the keyring backend, so the Android target's
  `-D warnings` clippy run failed (`constant SERVICE is never used`) and the
  v7.2.0 Android build produced no APK/AAB assets. The constant now carries the
  same Android gate as its only user.
- **The Android gate runs in review:** the `Android target compile` PR job now
  runs the same `cargo clippy -- -D warnings` command as the release job instead
  of `cargo check`, so a warning that only exists on the Android target fails on
  the PR rather than at tag time. `ci-workflow.test.ts` pins this.

## [7.2.0] - Güvenli sınırlar (Phase E / E1)

### Added

- **Code-authenticated LAN pairing (E1/3, #550):** pairing is a SPAKE2 handshake
  over the six-digit code, so the code never crosses the wire and a wrong guess
  costs one online attempt instead of leaving a recorded handshake testable
  offline. Both certificate fingerprints are hashed into the derived key, so a
  device that terminates TLS with its own certificate and relays the exchange
  derives two different keys and can confirm neither side - the client pins
  nothing and the host records nothing. The confirmation round runs on a
  connection pinned to the first answer's certificate, pairing fields are
  validated before the handshake, and the pre-7.2 handshake is refused with an
  update message.
- **OS keyring for desktop secrets (E1/4, #550):** where the platform has no
  cipher (Linux, macOS) the API keys, WebDAV password and OAuth token move into
  the OS credential vault (Secret Service, Keychain, Credential Manager) and the
  file keeps only a `keyring:<slot>` marker. The fallback `plain:` file, and
  every sealed store (LAN device key, person cards, signatures, trash copies),
  is written owner-only (`0600`); legacy plain values migrate into the vault on
  read and old files are tightened when read. The AI settings panel names the
  tier in use.
- **Application command ACL (E1/4b, #550):** the build declares a Tauri app ACL
  manifest, so the webview may only invoke commands a capability allows. One
  `allow-<domain>-commands` permission per IPC domain is generated from the IPC
  registry (249 commands) and granted by the main-window capability; an
  unregistered command cannot be called, and every registered one is proven
  reachable by the Rust suite and by a WebDriver scenario.
- **Plugin installs need a human and are recorded (E1/5, #550):** `plugin_install`
  validates the manifest and source, shows the plan (permissions, capabilities,
  size, bundle SHA-256) in a native Rust-side dialog and writes nothing without
  consent. `install.json` records the hash, and the plugin list reports a plugin
  changed after install as `modified` (before the record: `unrecorded`).

### Changed

- **CSP, prototype freeze and unsafe-free crates (E1/1, #550):** the webview
  policy now blocks plugins/objects, base-tag rewrites, form posts and framing,
  `freezePrototype` is on, and the six library crates declare
  `#![forbid(unsafe_code)]`.
- **Raw-path policy (E1/2, #550):** raw paths from the webview run through a
  policy instead of being accepted: only app-owned directories (registered at
  startup) and backend-produced paths pass, the validators take an explicit
  access mode, and `ensure_dir`, fingerprint/output-exists, the LAN share folder
  and the office output folders are all behind it.
- **LAN pairing protocol and host loop (E1/3, #550):** pairing takes three
  messages over two connections, accepted request nonces are remembered for the
  five-minute freshness window so a captured request cannot be replayed, and four
  worker threads with a bounded queue replace the one-connection-at-a-time loop,
  so a stalled peer no longer queues other devices or holds "stop sharing".
  Paired devices must both run 7.2 or newer.
- **Dependency policy and CI gates:** `cargo deny` is the single advisory gate
  and the command ACL, its generated permissions and the plugin install record
  all have their own checks in the Lint and Rust jobs.

### Fixed

- The plugin list distinguishes files the user approved from files that changed
  afterwards, instead of trusting whatever is on disk.

## [7.1.1] - Hızlı kazanımlar (Phase E / E0)

### Changed

- **Command surface and docs (E0, #551):** the unregistered `issue_tokens`
  command attribute is gone and the counted command surface/README numbers were
  refreshed; stale panic, bindings and network comments, the platform sentence
  and the file-wide dead-code allowance in `files.rs` were corrected.
- **Desktop CI guards (E0, #551):** superseded PR runs are cancelled (master
  runs never are), every job has a wall-clock timeout, and the fuzz
  crash-artifact upload now runs after the last target so a `pdf_parse` crash
  keeps its input file.
- **Lock-pinned release builds (E0, #551):** a `cargo metadata --locked`
  preflight runs before every release Tauri build (release, beta, Linux
  packages, Android, desktop Android target); the reproducibility gate requires
  it and a negative test proves the gate fails without it.
- **Single advisory policy (E0, #551):** the duplicated `cargo audit --ignore`
  list is removed; `cargo deny check` is the only advisory/license/ban/source
  gate, with the vendored-calamine (zip 9) and updater (zip 4) facts recorded.
- **Impress undo/redo (E0, #551):** pure updaters; the deck edit and sibling
  stack update are plain event-handler side effects, so StrictMode no longer
  runs them twice.
- **Hygiene (E0, #551):** generated `specta.ts` is out of the line ratchet,
  `check:fast` (byte-identical to `check`) is removed, and `check:all` builds on
  `check`.

## [7.1.0] - Dağıtım ve platform

### Added

- **Desk tasks in the command palette (D13, #545):** ten profession tasks —
  redact the selected PDF, extract the VAT summary, create an evidence bundle,
  review a petition, write an official letter, read a deadline from a
  notification, run the KVKK check, check document health, clean the document,
  open the data tools — run from the palette with `Ctrl+Alt+…` shortcuts, take
  the current file selection with them and preselect the AI pack (VAT). A query
  now shows a task that cannot run with an `aria-disabled` row and the reason
  ("Select a document first").
- **Signing readiness and runbook (D10 preparation, #546):** the release
  workflow fails fast and names the missing secret when an Authenticode
  provider is only partially configured; `npm run signing:check` pins the
  provider detection, the secret list, both verification steps and the runbook;
  `docs/signing.md` gained the operator checklist for the first signed release.
- **Reproducibility inputs gate and report (D11 preparation, #547):**
  `npm run repro:check` pins committed lockfiles, the single workspace version,
  SHA-pinned actions, `npm ci`, sorted archive inputs and the report structure;
  the release/desktop builds now pass `--locked`. `docs/reproducibility.md`
  separates the pinned inputs from what still differs between builds.
- **Platform decisions (D12, #547):** `docs/platform-decisions.md` records that
  macOS packaging, Flatpak and Windows ARM64 are declined/deferred for this
  train, each with its reason and the trigger that reopens it. The supported
  matrix stays Windows, Linux and Android.

### Changed

- **Windows Authenticode: unsigned by decision for 7.1.0.** The workflow stays
  ready for SignPath/Azure/PFX, but no provider account is available and the
  SignPath Foundation route needs the owner's application (and its reputation
  review); the decision and the revisit triggers are recorded in
  `docs/signing.md`. SmartScreen therefore still shows "unknown publisher" on
  the first download.
- The release notes no longer imply a signed installer before it exists; the
  signing notes point at the decision record.

## [7.0.0] - Yerel, eksiksiz

The major bump marks the scope of the train (local capabilities), not a file
format break: DOCX/XLSX/PPTX/ODT files open and save as before.

### Added

- **DOCX provenance manifests and the source chain (D7, #540):** a DOCX
  produced by the app carries the C8 manifest as one package part
  (`omnioffice/provenance.json`); the digest is canonical over the package
  entries, so re-compressing the file does not report a false change while
  editing a part does. `provenance_read_chain` sniffs a PDF or DOCX and returns
  sources → transformation → output with an intact/changed/absent/unsupported
  verdict; the verification screen shows the chain and puts a red tamper
  warning before the recorded sources. Conversion to DOCX, OCR PDFs and merged
  PDFs embed their manifest automatically.
- **Turkish task-pack evaluation and three new packs (D5, #541):** e-invoice
  analysis, KEP/e-Tebligat summary and court-petition classification join the
  four C7 packs; every pack has golden and negative eval answers with a 90 %
  floor, and an answer that does not verify gets one repair turn carrying the
  concrete problems. A test proves the harness and runner make no network or
  telemetry call; `docs/local-ai-decision.md` records the decision to keep a
  model out of the installer for this release.
- **DOCX tracked changes in the editor (D6, #542):** inline revision markers
  keep their kind, author and date, and the model survives a save.
- **XLSX shared formulas (D6):** filled-down followers of a shared formula are
  expanded on import by shifting relative A1 references (`B2 → =A2*2`), instead
  of arriving empty.
- **PPTX transition durations (D6):** the duration is written as `advTm` and
  read back, so a fade with 800 ms no longer returns as the 500 ms default.
- **Round-trip fidelity corpus (D6):** a programmatic DOCX/ODT/XLSX/PPTX corpus
  prints its measured table and fails on a silent loss; the measured delta is
  zero for every listed construct (`docs/office-fidelity-report.md`).
- **Named performance and memory budgets (D9, #543):** always-on 500-page DOCX
  and 50 000-cell XLSX open/save tests, counting-allocator memory ceilings
  (384/256 MiB), a calc render-window test for 50 000 rows and a pinned 2–8
  slot heavy-work bound with panic-release tests
  (`docs/performance-budgets.md`).

### Changed

- The heavy-work semaphore that isolates PDF/OCR/compression jobs is now pinned
  by tests: the async bound, the synchronous extraction bound and the
  panic-release behaviour.
- `docs/provenance-v2.md`, `docs/office-fidelity-report.md`,
  `docs/local-ai-decision.md` and `docs/performance-budgets.md` document the
  new capabilities and their limits.

## [6.15.0] - Telefon denetimi

### Added

- **Thumb-reach primary actions (D8, #538):** the shared screen shell keeps
  the actions in the header on a desktop window and moves them, below 900 px,
  into a bottom action bar with 44 px buttons — 25 of the 55 routes, while the
  rest keep their primary action in the page flow (forms, cards, editor
  canvas).
- **Keyboard overlap handling (D8, #538):** the covered visual-viewport pixels
  are published as `--keyboard-inset`, so the action bar lifts itself above the
  on-screen keyboard instead of hiding Save behind it; a focused field is
  scrolled to the centre and grows a 128 px scroll margin, and the bottom tabs
  still step aside.
- **Safe areas and landscape (D8, #538):** `viewport-fit=cover` plus
  `env(safe-area-inset-*)` padding keep the notch, the punch hole and the system
  navigation bar clear; at a phone height the action bar becomes one scrolling
  row, the tabs drop their labels, the drawer narrows to 60 vw and dialogs are
  capped to the viewport.
- **Audit report (D8, #538):** `docs/mobile-ergonomics-audit.md` carries the
  bilingual (en-US / tr-TR) 360 dp acceptance matrix for all 55 routes, a
  14-scenario device record sheet, the rules the matrix applies and the known
  limits.

### Changed

- The C11 keyboard helper moved to `lib/viewport.ts` and is shared by the
  bottom tabs and the new screen action bar; three test suites pin the compact
  placement, the safe-area/keyboard/landscape CSS and the scroll-into-view
  helper.

## [6.14.0] - Tarayıcı korpusu

### Added

- **Measured scanner corpus (D4, #536):** `corpus/scanner/` commits 14
  photo-realistic, deterministic images (perspective, rotation, dark and light
  desks, shadow gradients, warm/cool light, vignetting, paper texture, defocus,
  edge crop and three two-page spreads) with their ground-truth quads, generated
  by `scripts/generate-scanner-corpus.py` (Python standard library only). The
  golden test measures **12/14 usable (85 %)** against the 80 % target: a pass
  means the page count matches, the mean corner error is ≤ 3.5 % of the
  diagonal and every corner ≤ 6 %. Real camera photos cannot be committed
  (personal data), so the real-device pass stays the D1 manual matrix.
- **Automatic two-page separation (D4, #536):** a capture with two facing pages
  is split at the spine (or between two bright regions) into two quads, and
  every quad is refined against the photo's Sobel gradients. All three corpus
  spreads return exactly two pages; single pages land at ~0.5 px mean error.
- **Batch auto-crop (D4, #536):** `office_scan_detect_batch` checks the whole
  capture in one call, and the scanner screen shows a spread as two page rows
  (left/right half, spread badge) that are built as separate PDF pages. Rows
  are keyed by page id, so correcting one half never touches the other.

### Changed

- `docs/scanner-corpus.md` documents the corpus design, the acceptance rule,
  the measured table and the two known failures (bright page on a bright desk
  under a shadow gradient; the right half of the same spread) — documented, not
  hidden, and the corpus was not weakened.
- `docs/android.md`, `RELEASE_READINESS.md` and `docs/plan/PHASE-D-PROPOSAL.md`
  record the measured scanner baseline.

## [6.13.0] - Yerel ağ kapanışı

### Added

- **Two-device LAN acceptance (D1, #532):** `crates/synccore/tests/lan_two_host.rs`
  runs two hosts with two folders over loopback through pair, list, download,
  upload, a symmetric call, a three-way conflict, a confirmed delete and an
  unpair refusal as a release contract test, and
  `docs/lan-sync-acceptance.md` is the manual matrix (Windows/Android pairs,
  fourteen scenarios, record sheet) for the real Wi-Fi pass.
- **Deletion trash, restore and tombstones (D2, #533):** every confirmed LAN
  deletion first copies the document into a local trash (sealed index, 30-day
  retention, 200 entries, transfer cap); "Recently deleted" in the LAN card
  restores it into the selected folder. A sealed tombstone history per peer
  stops the plan from resurrecting or re-reporting the file, and base entries
  for paths gone on both sides are dropped so a later file with the same name
  is not a fake conflict.
- **Resumable chunked uploads (D3, #534):** uploads use begin/chunk/finish with
  512 KiB chunks, byte offsets, a streamed SHA-256 at the end and hidden
  `.parts/` partials that are cleaned after 24 hours; the client retries and
  resumes from the host's offset, so an interrupted Android transfer (foreground
  service restart, dropped Wi-Fi) continues instead of restarting. The LAN card
  shows live "Uploading … % / bytes" progress from a `lan-progress` event.

### Changed

- `docs/lan-sync.md` documents the resumable protocol, the trash/tombstone
  policy and the honest limits (downloads are still whole-file, the real
  two-device pass is manual).

## [6.12.0] - Yerel ağ eşitleme, ikinci aşama

### Added

- **mDNS/DNS-SD discovery (C14 phase 2, #524):** the host advertises
  `_omnioffice._tcp.local.` with its device id, certificate fingerprint and
  protocol version in the TXT record; a client browses the same type and gets
  the address from the resolved A/AAAA record. The first cut's UDP broadcast
  stays as a fallback for networks without multicast and both answers are
  merged by device id.
- **Symmetric pairing:** the pairing request carries the client's certificate
  fingerprint and, when it shares too, its listening port; the host stores the
  pin and address, so one pairing works in both directions once both sides
  share a folder.
- **Confirmed deletion propagation:** a path in the last common state missing
  on exactly one side is reported as a deletion. Nothing is removed by a plan;
  the user confirms `delete_remote`/`restore_local` (removed here) or
  `delete_local`/`restore_remote` (removed on the peer). The new `delete`
  request is HMAC-signed and path-checked like every other operation.
- **Android foreground service for sharing (C14 phase 2, #524):** a `dataSync`
  service with its own notification channel keeps the shared folder reachable
  while the app is in the background, and a `WifiManager.MulticastLock` lets
  mDNS responses through Wi-Fi power saving. Desktop keeps the process-bound
  behaviour.

### Changed

- The LAN plan card gains a "Removed documents" section with one explicit
  button per side; the threat model (`docs/lan-sync.md`) documents mDNS,
  symmetric pairing, deletion confirmation, the Android service limits and the
  updated residual risks.

## [6.11.0] - Yerel ağ eşitleme

### Added

- **Same-network document sync (C14, #522):** two devices on one Wi-Fi pair
  once and move office documents between their folders without an account, a
  cloud or a relay. The host shares one folder and shows a random six-digit
  code with a 120 s lifetime, five attempts and one-time use; the client pairs
  with that code over TLS, records the certificate's SHA-256 and refuses any
  other certificate afterwards. Every request is signed with HMAC-SHA256 over
  device id, timestamp, nonce, operation, path and body hash, and paths are
  validated (relative document paths only, extension allow-list, nothing
  outside the shared folder). Transfers are capped at 512 MiB; no delete
  operation exists.
- **Sync plan with explicit conflict resolution:** the local folder is hashed
  and compared with the peer's listing through the existing three-way
  `metadata::detect_state`; conflicts are never auto-resolved and go through
  the existing `merge::plan` policy - keep both saves the peer copy next to the
  local file, keep local uploads, keep remote downloads. "Sync safe changes"
  applies only the unambiguous items.
- **Device identity and pairing store:** one sealed store (DPAPI / Android
  Keystore, `plain:` warning elsewhere) holds the device certificate and key,
  the paired peers and the last common hashes; unpairing removes the key on
  either side and the next request is refused.
- **LAN card on the Sync screen:** share a picked folder, show and copy the
  code, discover hosts (UDP broadcast on port 47821), pair with a code, compare
  folders, sync safe changes and resolve each conflict; en and tr strings.
- **Threat model (`docs/lan-sync.md`):** assets, actors, pairing, post-pairing
  authentication, revocation, platform limits and the honest residual-risk
  list (six-digit-code MITM, traffic analysis, discovery metadata, one role per
  device, no delete propagation, single-threaded host).

### Changed

- The bundle budget moves 515 -> 520 KB gzip for the LAN card, and the new
  Rust modules join the file-size ratchet.

## [6.10.0] - Otomatik belge tarama

### Added

- **On-device page detection (C12, #520):** the scanner now finds the page
  instead of asking for four corners on every photo. The photo is downscaled,
  turned into one luminance channel, blurred and run through Canny; a Hough
  transform turns the strongest edges into lines, two near-parallel pairs and
  two crossing pairs become quadrilateral candidates, and each candidate is
  scored by area and by how much of its perimeter lies on an edge. An Otsu
  bright-region contour is added as a fallback for low-contrast borders. The
  best quad is scaled back to the full-size photo before the perspective
  correction, so the flattening still runs on full-resolution pixels.
- **Scanner filters (C12, #520):** colour, greyscale and adaptive black and
  white (a local box-mean threshold, so uneven light does not swallow the
  page); the colour path is byte-for-byte the old behaviour.
- **Batch capture:** a captured photo is auto-cropped and added to the page
  list without opening the picker; the picker opens with the detected corners
  and "find corners automatically" runs the detector again. The manual corner
  picker stays the correction surface when nothing is found.
- **`imageproc` 0.27** (MIT, default features off) joins `pdfcore` for the
  edge, blur, contrast and contour primitives; its transitive tree stays in the
  `deny.toml` license allow list and does not raise the Rust 1.89 MSRV.

### Changed

- The scanner commands moved out of `office.rs` into `src-tauri/src/scanner.rs`
  (`office.rs` 2,758 -> 2,562 lines); the file-size ratchet follows.

## [6.9.0] - Yerel Türkçe yapay zekâ görev paketleri

### Added

- **Turkish AI task packs (C7, #518):** four ready-made tasks - fix a
  petition, summarise a contract clause, extract a VAT summary and read the
  deadline out of a notification - each with a Turkish instruction, the
  expected JSON shape and a verification pass. The prompt asks for JSON only
  and forbids invented values (dot decimals, `YYYY-MM-DD` dates); the verifier
  tolerates fences and surrounding prose, then checks every field, type and
  enum. The VAT pack re-computes each row and the totals, and the notification
  pack checks the date format, that the last day is not before the service
  date and that the period is plausible. Problems come back as i18n codes.
- **AI packs screen:** a new screen runs a pack against a PDF through the
  existing `ai_chat` command and shows the verification badges, the problems
  and the raw answer; the provider card carries a **local** badge, offers a
  one-click switch to the Ollama preset, and states that no model ships inside
  the installer.
- **Local mode keeps documents on the machine:** the screen itself makes no
  network request (asserted in its test), and a new `aicore` test proves that a
  local provider stays on its own loopback origin - a remote `http://` base is
  rejected and a redirect cannot leave the origin.

### Changed

- The bundle budget moves 510 -> 515 KB gzip, and `ai-task-packs.ts` joins the
  file-size ratchet.

## [6.8.0] - Ofis belgelerinde model seviyesinde fark

### Added

- **Model-level office diff (C13, #514):** Writer paragraphs from the body,
  header and footer plus table-cell paragraphs, Calc sheets and cells and
  Impress slides are compared structurally instead of being rendered page by
  page. Writer and Impress changes carry word-level segments; Calc reports
  before/after per cell (formula text when the cell has one). Every result is
  bounded and says when the list was cut.
- **Compare screen, Office mode:** the Compare screen gains a PDF / Office
  switch; in Office mode two Writer, Calc or Impress documents are opened as
  models and compared without rendering.
- **Compare a saved version with the current document:** the version-history
  dialog now has "Compare with current"; it loads the snapshot and diffs it
  against the open tab's model (unsaved edits included) and shows the result
  in the same viewer.

### Changed

- The version-history dialog moved out of `OfficeWorkspace.tsx`
  (465 -> 361 lines) so that file keeps shrinking instead of growing.
- The bundle budget moves 505 -> 510 KB gzip for the diff engine and viewer
  (both lazy; the entry chunk is unchanged), and the `Compare.tsx` ratchet
  grows by the mode switch.

## [6.7.0] - Şablon alanları, kişi kartları ve Linux paketleri

### Added

- **Template fields and the fill form (C6, #508):** a Writer template that
  carries `{{alan}}` placeholders now asks for their values before it is
  created - one labelled input per field, today's date proposed for date
  fields, "fill and create" or "create without filling". The rules mirror the
  file-based mail merge (letters, digits and underscores between double
  braces, Turkish letters included); the body, the header, the footer and
  table cells are scanned, and a placeholder split across runs is filled by
  collapsing that paragraph into one run. A field-carrying Turkish petition
  template ships with the catalogue.
- **Encrypted person cards (C6, #509):** a local address book behind the
  template form and the mail merge. Cards hold the fields a document needs
  (name, VKN/TCKN, address, phone, e-mail, note), are sealed like the saved
  signatures (DPAPI on Windows, Android Keystore on Android) and live in one
  file per card; a card that cannot be read is reported and skipped. The
  dialog adds/edits/deletes cards, copies CSV or vCard 3.0, imports from a
  paste with a KVKK warning, and the template form fills the fields a card
  knows.
- **Linux packages (C6-era distribution, #500):** every release now also
  ships `OmniOffice-Linux-X.Y.Z-amd64.deb` and `.AppImage` with
  `SHA256SUMS-linux.txt`, built by `linux-package.yml` on Ubuntu and attached
  next to the Windows and Android assets. This is the first release that
  includes them.
- **Typed bindings on a runner (#511):** a `workflow_dispatch` job regenerates
  `src/lib/generated/specta.ts` and uploads it as an artifact, so a Rust
  command change no longer needs a Tauri build on a workstation; the Desktop
  workflow's `bindings:check` still guards the committed file.
- **Distribution docs (#502 groundwork):** `docs/distribution.md` records what
  every release ships, the Authenticode provider options with the secrets
  each needs, and the F-Droid/IzzyOnDroid path; fastlane metadata (en-US/tr)
  supplies the Android listing text.

### Changed

- README documents the Linux packages and the new release assets.
- The bundle budget moves 500 -> 505 KB gzip for the C6 UI (a lazy route and a
  lazy dialog; the entry chunk is unchanged).

## [6.6.0] - Doğrulama, geri bildirim ve kaynak manifesti

### Added

- **Signature verification screen (C9, #493):** the Security section gains
  "Verify signature": pick a PDF and see, without signing anything, whether
  the document was signed, whether the signed revision changed afterwards,
  one verdict for the document (verified, changed after signing, does not
  verify, unsigned, unreadable) and, per signature, the signer, issuer,
  serial, validity, signing time, RFC 3161 timestamp, algorithm, SHA-256,
  certificate chain and notes. The optional online revocation check is
  disabled until it is turned on in Settings and a failed online check keeps
  the offline result. Trust is still reported unknown: the build has no
  system trust store.
- **Source manifest for generated PDFs (C8, #497):** a manifest embedded in
  the file (JSON record plus XMP) records the source files with their
  SHA-256, the creation time, the producing module and - when AI was
  involved - the provider, model and citation count. Reopening the document
  shows the manifest and reports when the document changed after it was
  written. The verification screen shows the manifest and writes a manifested
  copy from picked source files and a note. A signed document is refused: a
  rewrite would break its signatures. DOCX manifests and automatic manifests
  from the producer screens remain follow-ups.
- **Report a problem without uploading anything (C15, #495):** Settings'
  diagnostics section now opens a feedback flow that builds the existing
  redacted diagnostics report, shows it for review, copies it on request and
  opens the GitHub issue form pre-filled with a template. Nothing is sent
  automatically; file and folder names stay masked as `<path>`.

## [6.5.1] - Telefon denetimindeki arayüz kusurları

### Fixed

- **PDF Studio scrolls on a phone again, and five screens have their
  padding back.** PDF Studio, Jobs, Plugins, Compatibility and the office
  launcher used layout classes that had no styles, so their content touched
  the screen edge and PDF Studio's Run button could not be reached on a
  phone. The office empty screen and the Calc dialogs, which used the same
  unstyled helpers, have their gaps back too.
- PDF Studio shows each file's real size instead of "0 B".
- New office documents are named in the interface language ("Adsız hesap
  tablosu" with a "Sayfa1" tab in Turkish).
- Reader: on a phone the page opens at the screen width instead of partly
  off screen, and the search icon no longer covers the placeholder (also in
  the Home and AI library searches).
- Dialog buttons wrap on a narrow screen; the pre-send dialog's first button
  was cut off.
- The drawer's "100% local" badge sits above the system navigation bar.
- A long dropdown value ends with an ellipsis before the arrow.
## [6.5.0] - Delil klasörü, resmî yazı ve okunur ofis PDF'leri

### Added

- **Evidence bundle (#341 C3, Delil klasörü):** one PDF with a cover sheet,
  an index of every exhibit's page range, size and SHA-256, the exhibits in
  the chosen order stamped `EK-n · i/n` with a bundle-wide page number, and
  an embedded manifest. Reopening the bundle tells whether it was changed
  after it was built, and original files are matched to their exhibits by
  hash. The lawyer desk's evidence task opens it (it used to open Merge).
- **Official letter (#341 C5, Resmî yazı):** a letter laid out as the
  Official Correspondence Regulation describes it: "T.C." and the
  institution, number and date on one line, subject, addressee in capitals,
  references, a justified body, the signature block, attachments,
  distribution and the contact block in the page footer. The number
  (`E-DETSİS-file plan code-sequence`) is counted per year and code, the
  institution and contact details are remembered, and the letter opens in
  Writer or is saved as DOCX or PDF/A-2b. The public servant desk's letter
  task opens it.
- Office documents export straight to PDF/A (`office_export_pdfa`); the
  report says whether the result validates.

### Fixed

- **Office PDF exports were unreadable.** Every Writer, Calc, Impress and UDF
  PDF export drew the wrong glyphs and carried no extractable text, in the
  app's own reader and in other viewers alike: the font's CIDs were numbered
  in order of use instead of being glyph ids, the widths were one entry off
  and the ToUnicode map used the wrong operator. Exports now show the right
  text, which can be searched and copied.
- Centred and right-aligned lines in office PDF exports were offset twice:
  centred text ended at the right margin.
- A footer of several lines ran off the bottom of the page; it now ends at the
  footer distance above the page edge, as in Word.
## [6.4.9] - Tipli IPC: PDF komutları, #316 tamam

### Changed (internal)

- **Typed PDF commands (#316, group 7):** the 70 PDF commands (inspection,
  pages, annotations, forms, redaction, signing and certificates, recipes,
  PDF Studio, conversion and the rest) are called through wrappers generated
  from the Rust types. The long PDF jobs keep `invokeTracked`, because the
  Jobs screen re-runs them from the stored arguments.
- **#316 is complete.** Every registered command has a generated wrapper,
  and a test now fails if any screen calls a typed command through a
  hand-written `invoke`. The only hand-written calls left are the job tracker
  and the two Android-only update commands.
- The recipe step and redaction certificate types on the front end are now
  aliases of the generated ones, so they can no longer drift from Rust.

### Fixed

- Redaction certificate sizes are typed as numbers, which is what Rust sends.
  The old generated file said `bigint`.
## [6.4.8] - Tipli IPC: uygulama komutları

### Changed (internal)

- **Typed app commands (#316, group 6):** 40 of the 42 app commands (app
  info and updates, diagnostics, engines, file tokens, jobs, settings,
  recents, the operation log, launch files, Android open-with and share) are
  called through wrappers generated from the Rust types. The two Android-only
  update commands keep their hand-written call: the bindings are exported on
  desktop, where they do not exist.
- The IPC coverage check now also reads the generated wrappers, so a typed
  command is still checked against the commands Rust registers.

### Fixed

- The operation log's optional fields (pages, sizes, detail) are typed as
  Rust sends them (`null`, not absent), and the History export keeps writing
  an empty cell for them.
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
