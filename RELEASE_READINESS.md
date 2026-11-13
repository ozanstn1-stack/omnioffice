# Release Readiness — OmniOffice 7.46.0

This file states what is actually implemented, tested and benchmarked, and
what is not. It is deliberately conservative: nothing is claimed as released,
built or verified unless it was reproduced in this environment or is produced
by CI.

Every `Implemented` bullet carries a `test:<file>#<name>` marker pointing at
the test that proves it (or an explicit `(no test: …)` reason);
`npm run claims:check` resolves every marker in CI, so a deleted or renamed
test cannot leave an unsupported claim behind. Since v6.0.1 the CI jobs that
run the suites also verify the markers against the test output
(`--rust-log` / `--junit`), so a claim cannot outlive its passing test.

## Implemented (this cycle, 7.46.0) - XLSX shared-formula export & CI engine lock parity

- **XLSX `t="shared"` shared-formula export grouping (`xlsx::shared_write`):**
  exports consecutive vertical and horizontal formula runs (2+ cells) with
  `t="shared"`, `ref`, and sequential `si` on anchors and `<f t="shared" si="..."/>`
  on followers while keeping mixed/absolute references, `_xlfn.` prefixes,
  `t="str"` cached text outputs, and dynamic-array `spill` cells lossless across
  export/import round trips.
  `test:crates/officecore/tests/xlsx_shared_write_test.rs#vertical_shared_formula_group_exports_and_round_trips`
  `test:crates/officecore/tests/xlsx_shared_write_test.rs#horizontal_and_mixed_absolute_references_group_correctly`
  `test:crates/officecore/tests/xlsx_shared_write_test.rs#future_functions_coalesce_and_dynamic_array_spills_stay_isolated`
- **Cross-platform CI engine lock coverage (`fetch-engines.mjs --verify-lock`):**
  wired `node scripts/fetch-engines.mjs --verify-lock` into `.github/workflows/desktop.yml`
  (`lint` job) and added automated Vitest coverage verifying all 20 pinned artifacts
  in `scripts/engines.lock.json`.
  `test:src/lib/reproducibility.test.ts#verifies scripts/engines.lock.json across all pinned artifacts via fetch-engines.mjs`
  `test:src/lib/reproducibility.test.ts#runs fetch-engines.mjs --verify-lock in desktop workflow lint job`

## Implemented (this cycle, 7.45.0) - Architecture, job store unification & Writer modularization

- **Unified `useJobs` store and bounded progress eviction:** merged `store.ts`
  and `jobs.ts` via `jobs-persist.ts`, settled tool jobs via `useTool.ts`,
  eliminated duplicate listener attachment in `App.tsx`, and evicted progress
  entries on `finish`, `cancel`, `settle`, `clearFinished`, restart, and prune.
  `test:src/lib/jobs.test.ts#evicts finished, cancelled, settled, and pruned progress payloads`
  `test:src/lib/jobs.test.ts#routes job:progress events through the unified attach() bridge once`
- **Modular Writer Ribbon (`WriterRibbon` & `WriterRibbonTabs`):** extracted the
  five Writer Ribbon tabs and action contract from `WriterEditor.tsx` into
  dedicated panel components under `src/office/writer/panels/`.
  `test:src/office/writer/panels/panels.test.tsx#switches tabs, enforces disabled state, and dispatches editor callbacks`

## Implemented (this cycle, 7.44.0) - Isolation, fuzzing & cross-platform tooling

- **Worker isolation for `pdf_info`:** unauthenticated `pdf_info` calls run via
  `Job::PdfInfo` in the contained `--worker` subprocess while preserving
  `PdfError::PasswordRequired` and keeping passworded calls off process `argv`.
  `test:src-tauri/src/worker_tests.rs#parses_worker_arguments`
  `test:crates/pdfcore/tests/serialization_test.rs#pdf_info_round_trips_through_json`
- **LAN sync frame decoder hardening & fuzzing:** `synccore::lan::parse_frame_bytes`
  and `transport::read_frame` decode frames with bounded body reads; added
  `eml_parse`, `odf_read`, and `lan_frame` fuzz targets and removed `catch_unwind`
  from `pdf_parse`.
  `test:crates/synccore/src/lan.rs#parse_frame_bytes_rejects_truncated_and_oversized_frames`
- **Cross-platform engine lock verifier:** `scripts/fetch-engines.mjs --verify-lock`
  verifies `scripts/engines.lock.json` coverage across all 20 pinned artifacts on
  Linux, macOS, and Windows without PowerShell.
  (no test: verified by `node scripts/fetch-engines.mjs --verify-lock`.)

## Release trains and the freeze (v6.0.1)

The v6.0.0 review put a **feature freeze** in place: until its A-phase items
close, changes fix, verify or close what exists instead of adding features.
Two channels carry builds:

- **Stable** — the versioned releases; their installers check
  `releases/latest/download/latest.json`.
- **Beta** — `scripts/set-update-channel.mjs` (CI-only) and
  `.github/workflows/beta.yml` build a signed installer from master with a
  `X.Y.Z-beta.N` version (a prerelease of the _next_ minor, so semver moves
  beta users forward and later pulls them back onto stable) and its own
  updater endpoint (`releases/download/beta/latest.json`), published to the
  rolling `beta` prerelease.

The channel is a property of the installed build; Settings shows which one and
links to the beta installers. Beta cadence is a manual dispatch until the
release-train schedule is decided.

## Implemented (this cycle, 7.1.0) - Tekrarlanabilirlik ve platform kararları (D11/D12)

- **Reproducibility inputs gate (D11 preparation, #531):** `npm run repro:check`
  pins the inputs the repository controls — committed lockfiles, one workspace
  version inherited by every crate, SHA-pinned actions, `npm ci`, sorted
  archive inputs — and the release builds now pass `--locked` so a lockfile
  change cannot slip into a release build.
  `test:src/lib/reproducibility.test.ts#passes the repository as it stands`
  `test:src/lib/reproducibility.test.ts#fails when an action loses its commit pin`
- **Reproducibility report (D11 preparation, #531):**
  `docs/reproducibility.md` separates the pinned inputs from what still differs
  between builds (unpinned rustc channel, ZIP/NSIS/deb/AppImage/APK bytes) and
  gives the verification steps and the path to bit-reproducible Linux builds.
  `test:src/lib/reproducibility.test.ts#fails when the reproducibility report loses its gaps section`
- **Platform decisions (D12, #531):** `docs/platform-decisions.md` records the
  three decisions with reasons, evidence and revisit triggers — macOS packaging
  declined for v7.1 (CI build/test stays), Flatpak declined for v7.1 (sandbox
  and engine constraints), Windows ARM64 deferred (device verification and CI
  cost); the supported release matrix is unchanged.
  (no test: a decision record, documented with its reasons and triggers.)

### Not done / Known limitations (7.1.0)

- **D11 is not closed:** the IzzyOnDroid request still needs the owner's
  Codeberg submission (the text is ready in
  `docs/izzyondroid-submission-template.md`); F-Droid main is declined for now
  because of the prebuilt engines. Reproducibility is pinned at the input
  level, not proven byte-for-byte.
  (no test: external listing submission and future build work.)
- **D12 declines are decisions, not shipped platforms:** no macOS, Flatpak or
  Windows ARM64 package exists; each version ships only the Windows, Linux and
  Android assets verified in the release checklist.
  (no test: decision record.)

## Implemented (this cycle, 7.43.0) - PDF: görünümsüz işaretleme, not silme

- **Flatten draws markup without an appearance stream and burns it in:**
  `test:crates/pdfcore/tests/flatten_synth_test.rs#markup_without_an_appearance_is_drawn_and_burned_in`
- **A delete in an annotation batch does not shift the index of a later edit:**
  `test:crates/pdfcore/tests/annotation_delete_test.rs#a_delete_does_not_shift_the_index_of_a_later_edit`
- **Deleting a comment removes its popup and every reply:**
  `test:crates/pdfcore/tests/annotation_delete_test.rs#deleting_a_comment_removes_its_popup_and_every_reply`

## Implemented (this cycle, 7.42.0) - Yapay zekâ: toplu gömme, hatırlanan onay

- **Embeddings for many texts go in one request and come back in input order:**
  `test:crates/aicore/tests/client_test.rs#openai_embed_batch_sends_one_request_and_keeps_input_order`
- **A remembered AI data notice applies only to the same provider and host:**
  `test:src/lib/ai-consent.test.ts#remembers a consent only for the same provider and host`
- **A remembered notice shows a Forget button instead of the consent checkbox:**
  `test:src/components/ai-consent-notice.test.tsx#shows a remembered consent with a forget button and no checkbox`

## Implemented (this cycle, 7.41.0) - Türkiye: 2030+ bayramlar, CMK adli tatil

- **Religious holidays after 2029 are counted, including two feasts in one year:**
  `test:src/office/calc/turkish-functions.test.ts#counts the calculated feasts after 2029, twice in a year when the calendar does`
- **A deadline after 2029 moves past a calculated feast and is still flagged for checking:**
  `test:src/lib/legal-deadline.test.ts#moves past a calculated feast after 2029 and still asks to check the day`
- **A criminal period ending in the recess ends on 3 September:**
  `test:src/lib/legal-deadline.test.ts#extends a criminal period ending in the recess by three days, to 3 September`

## Implemented (this cycle, 7.40.0) - ODP asıl sayfaları

- **Every slide master is written to ODP and each slide names its own:**
  `test:crates/officecore/src/odf/slides_masters.rs#every_master_is_written_and_each_slide_names_its_own`
- **ODP masters round-trip with their names, backgrounds and shapes:**
  `test:crates/officecore/src/odf/slides_masters.rs#masters_round_trip_with_their_names_backgrounds_and_shapes`
- **A deck without masters keeps the single default page and reads back without masters:**
  `test:crates/officecore/src/odf/slides_masters.rs#a_deck_without_masters_keeps_the_single_default_page_and_reads_back_without_masters`

## Implemented (this cycle, 7.39.0) - Excel dinamik dizileri

- **A spilled array saved by Excel imports as one formula that Calc spills again:**
  `test:crates/officecore/src/xlsx/array_formulas.rs#an_excel_spill_imports_as_one_formula_the_editor_can_spill_again`
- **A spilled formula is saved with Excel's dynamic-array metadata and reads back as one formula:**
  `test:crates/officecore/src/xlsx/spill_write.rs#a_saved_spill_carries_excels_metadata_and_reads_back_as_one_formula`
- **The saved copy carries each formula's spill range:**
  `test:src/office/calc/spill-export.test.ts#gives a spilling formula the range it covers`

## Implemented (this cycle, 7.38.0) - Calc grafikleri düzenlenir, taşınır, yığılır

- **A Calc chart's title, kind, legend and ranges are edited after it is inserted:**
  `test:src/office/calc/ui/ChartDialogs.test.tsx#saves the edited title, kind, legend and series`
- **A Calc chart moves by its title bar to the cell under the drop:**
  `test:src/office/calc/ui/ChartDialogs.test.tsx#moves a chart by its title bar to the cell under the drop`
- **A Calc chart resizes by its corner, and a click on the handle changes nothing:**
  `test:src/office/calc/ui/ChartDialogs.test.tsx#resizes a chart by its corner and ignores a click`
- **Stacking, value labels and axis titles are saved with the chart:**
  `test:src/office/calc/ui/ChartDialogs.test.tsx#saves stacking, value labels and axis titles`
- **Stacked columns scale to the largest stack:**
  `test:src/office/calc/ui/cartesian-plot.test.tsx#stacks columns so the tallest stack reaches the top of the plot`
- **The category axis title follows the category axis:**
  `test:src/office/calc/ui/cartesian-plot.test.tsx#puts the category title below a column chart and left of a bar chart`
- **The command palette and global search load only when opened:**
  `test:src/components/global-overlays.test.tsx#loads the command palette and global search only when opened`

## Implemented (this cycle, 7.37.0) - Uzun işler tipli komut bağlamalarıyla

- **Every retryable job names the same command its typed wrapper invokes:**
  `test:src/lib/api-tracked.test.ts#finds every tracked job`
- **A tracked job records the request it sends, built before recording:**
  `test:src/lib/api-tracked.test.ts#builds the metadata request before recording it`
- **Recipe steps carry the same variants and fields in both directions of the specta bindings:**
  `test:src/lib/ipc-contract.test.ts#RecipeStep carries the same variants and fields as the Rust enum`
- **The compress and OCR options keep their snake_case wire names:**
  `test:src/lib/ipc-contract.test.ts#documents the snake_case wire names of the compress/OCR options`

## Implemented (this cycle, 7.36.0) - Calc düzenleyicisi bileşenlere ayrıldı

- **Calc's formula bar turns Enter, Tab and Escape into intents for the editor to commit:**
  `test:src/office/calc/ui/FormulaBar.test.tsx#passes the trimmed name box and turns Enter, Tab and Escape into intents`
- **The formula assist gets a key before the formula bar:**
  `test:src/office/calc/ui/FormulaBar.test.tsx#lets the formula assist consume a key first`
- **Calc's column headers mark the active and hidden-neighbour columns and pin frozen ones:**
  `test:src/office/calc/ui/GridHeaders.test.tsx#marks the active column, a hidden neighbour and pinned columns, and opens the menu`
- **Structured tables band every other body row and never the totals row:**
  `test:src/office/calc/ui/cell-look.test.ts#bands every other body row and never the totals row`
- **Escape in a Calc cell editor does not reach the grid:**
  `test:src/office/calc/ui/CellEditor.test.tsx#keeps Escape from reaching the grid`

## Implemented (this cycle, 7.35.0) - Writer ve Impress panelleri bileşenlere ayrıldı

- **Writer's Find & replace, comments, review, spelling and official-letter panes are components; review cards select by key and resolve without selecting:**
  `test:src/office/writer/panels/panels.test.tsx#selects a revision by click or key and resolves it without selecting it`
- **The official-letter pane jumps only to violations with a paragraph and fixes one or all:**
  `test:src/office/writer/panels/panels.test.tsx#jumps only to violations that have a paragraph and fixes one or all`
- **Writer's cross-reference dialog inserts only with a target:**
  `test:src/office/writer/panels/dialogs.test.tsx#inserts a cross-reference only once a target is chosen, then closes`
- **Writer's sections dialog swaps the page for landscape and ignores the last section's start:**
  `test:src/office/writer/panels/dialogs.test.tsx#swaps the page for landscape and leaves the last section's start alone`
- **Impress's slideshow bar counts slides, picks ink tools and closes:**
  `test:src/office/impress/show-chrome.test.tsx#counts slides, picks a tool and closes from the show bar`
- **Impress's View tab starts the show with or without presenter view:**
  `test:src/office/impress/show-chrome.test.tsx#starts the show with or without presenter view`
- **Impress's animation list keeps its order and guards its ends:**
  `test:src/office/impress/properties.test.tsx#orders animations, guards the ends, deletes by id and needs an object to add`
- **Impress's text tools keep the focus in the edited frame:**
  `test:src/office/impress/object-properties.test.tsx#formats through the editing handle and keeps the focus in the frame`

## Implemented (this cycle, 7.34.0) - Ortak geri alma geçmişi ve uygulama kabuğu

- **One undo or redo is applied exactly once, also under StrictMode:**
  `test:src/office/useSnapshotHistory.test.tsx#applies one undo and one redo exactly once under StrictMode`
- **The shared history walks back and forth and clears redo on a new edit:**
  `test:src/office/useSnapshotHistory.test.tsx#walks back and forth and clears redo on a new edit`
- **The history is bounded:**
  `test:src/office/useSnapshotHistory.test.tsx#keeps at most the limit plus one snapshots and ignores an empty stack`

## Implemented (this cycle, 7.33.1) - Vurgu rengi ve tema kontrastı

- **Text colours read at 4.5:1 in every theme:**
  `test:src/lib/theme-contrast.test.ts#reads body, secondary, accent and document-kind text at 4.5:1 in every theme and accent`
- **Every accent has a light and a dark variant:**
  `test:src/lib/theme-contrast.test.ts#defines all three non-default accents for both themes`
- **The root carries the accent, except where a theme has its own palette:**
  `test:src/components/accent-picker.test.tsx#marks the root with a non-default accent, except in the themes with their own palette`
- **Settings offers the accents as a radio group:**
  `test:src/components/accent-picker.test.tsx#offers four swatches as a radio group and applies the chosen one`
  `test:src/components/accent-picker.test.tsx#is disabled, with a note, in the midnight and paper themes`

## Implemented (this cycle, 7.33.0) - Düzenleyici kabuğu (masaüstü ve Android)

- **A phone ribbon floats just above the on-screen keyboard while it is up:**
  `test:src/office/editor-shell-css.test.ts#floats a phone ribbon just above the keyboard while it is up`
- **The root says whether the keyboard is up:**
  `test:src/lib/viewport.test.ts#marks the root while the keyboard is up, for the floating phone ribbon (7.33)`

- **The active ribbon tab is underlined in the accent on a light tool row:**
  `test:src/office/editor-shell-css.test.ts#marks the active ribbon tab with the accent underline, not a raised tab`
- **A pressed tool uses the accent's ink, never white on the tint:**
  `test:src/office/editor-shell-css.test.ts#paints a pressed tool in the accent's ink, never white on the light tint`
- **Sheet tabs, selected headers, chips, the recovery banner, template icons and Writer
  footnote markers are readable:**
  `test:src/office/editor-shell-css.test.ts#never paints the white --accent-text on a light tint or the white page`
- **Every theme has an accent ink:**
  `test:src/office/editor-shell-css.test.ts#gives every theme an accent ink`

## Implemented (this cycle, 7.32.1) - Belge türü ekranları ve sakin başlıklar

- **The office launchers list only their kind's recent files, without kind chips, and start
  a blank one or open the template gallery:**
  `test:src/screens/OfficeLauncher.test.tsx#lists only this kind's recent files, without kind chips, and starts a blank one`
- **Section labels are sentence case; letter-spaced capitals cannot come back:**
  `test:src/components/layout-css.test.tsx#labels sections in sentence case, not letter-spaced capitals (7.32.1)`

## Implemented (this cycle, 7.32.0) - Ana sayfa: son dosyalar ve Araçlar merkezi

- **The Tools hub shows every enabled tool once and opens the one tapped; the scanner
  leads the PDF group:**
  `test:src/screens/Tools.test.tsx#shows every enabled tool once, grouped, and opens the one tapped`
- **Every screen it promises has a tile:**
  `test:src/screens/Tools.test.tsx#has a tile for every screen it promises`
- **Group chips and search narrow the hub:**
  `test:src/screens/Tools.test.tsx#narrows by group chip and by search, and says when nothing matches`
- **The phone tab bar offers Tools, lit for every tool screen:**
  `test:src/components/bottom-nav.test.tsx#offers Home, Office, Tools, Assistant and More within thumb reach`
  `test:src/components/bottom-nav.test.tsx#lights the tools tab for every tool screen, the scanner included`

- **Recent files are filtered by kind and open from the whole row:**
  `test:src/components/recent-table.test.tsx#filters by kind and opens or shares the chosen row`
- **An empty kind and an empty list say so:**
  `test:src/components/recent-table.test.tsx#says when a kind has no recent files and when there are none at all`
- **Rows are dated relative to now, in the interface language:**
  `test:src/components/recent-table.test.tsx#dates rows relative to now in the interface language`
- **The running background job shows on Home and opens Jobs:**
  `test:src/components/recent-table.test.tsx#shows the first running job with its progress and opens Jobs`

## Implemented (this cycle, 7.31.0) - Yeni arayüz (Windows ve Android)

- **The sidebar folds the routes into hubs, scan, images and OCR together:**
  `test:src/components/sidebar.test.tsx#folds the routes into a few hubs, with scan, images and OCR together`
- **The hub of the current screen is open and closed entries stay findable:** closed hubs
  hide their items instead of removing them, so shortcuts and the e2e flows still reach them.
  `test:src/components/sidebar.test.tsx#opens the hub of the current screen and keeps closed items findable`
- **The compact sidebar keeps one flat icon column:**
  `test:src/components/sidebar.test.tsx#shows one flat icon column when compact`
- **New creates a blank file from the sidebar:**
  `test:src/components/sidebar.test.tsx#creates a blank spreadsheet from the New menu`
- **Home creates a blank document in one click:**
  `test:src/components/sidebar.test.tsx#opens a blank document in one click and the gallery for a template`
- **Home greets by the hour, in the interface language:**
  `test:src/components/sidebar.test.tsx#greets by the hour and dates in the interface language`
- **The phone tab bar offers the assistant:**
  `test:src/components/bottom-nav.test.tsx#offers Home, Office, Tools, Assistant and More within thumb reach`
  `test:src/components/bottom-nav.test.tsx#lights the assistant tab for the task packs and the library`

## Implemented (this cycle, 7.30.5) - Radyo grubu yazdırılan adla da doldurulabiliyor

- **A radio group filled by its printed name lands on the state behind it:** `/Opt`
  gives the options the names a form shows while the appearance states keep their own
  names (ISO 32000-1 12.7.4.2.4, Table 227), so asking for the printed name used to be
  refused as "not a declared state". The name is now resolved through the group's own
  options before the state is written.
  `test:crates/pdfcore/tests/radio_opt_probe.rs#a_radio_group_can_be_filled_by_the_printed_name`
- **The value goes on the field and the state on each child:** filling a three-way group
  writes `/V` once and `/AS` per widget, so only the chosen child shows on.
  `test:crates/pdfcore/tests/radio_opt_probe.rs#a_radio_group_writes_the_value_once_and_the_state_on_each_child`
- **Clearing turns every child off:**
  `test:crates/pdfcore/tests/radio_opt_probe.rs#clearing_a_radio_group_turns_every_child_off`

## Implemented (this cycle, 7.30.4) - Buton durumu sessizce değiştirilmiyor

- **An unknown checkbox or radio state is refused instead of quietly replaced:** the
  fill path used to write the first declared state (and warn), so asking for a state the
  document does not declare produced a file whose `/V` claimed one thing while `/AS`
  showed another - or a state with no appearance stream at all (ISO 32000-1 12.5.5,
  Table 168). It now returns an error and writes nothing.
  `test:crates/pdfcore/tests/button_state_probe.rs#an_unknown_state_is_refused_and_the_document_is_left_alone`
- **The on state is the document's, not `/Yes`:** a checkbox whose appearance declares
  `/Off` and `/Ja` is filled with `/Ja` for both `/V` and `/AS`, and the `/Yes` spelling
  only appears when the document itself uses it.
  `test:crates/pdfcore/tests/button_state_probe.rs#a_checkbox_takes_the_on_state_the_document_declares`
- **Clearing writes `Off` on both keys:**
  `test:crates/pdfcore/tests/button_state_probe.rs#clearing_a_checkbox_writes_off_on_both`
- **Not covered yet:** a radio group's `/Opt` labels and per-child `/AS` are the next
  slice; the probe stops at a single merged checkbox for now. (no test: out of scope for
  this change and recorded in the plan.)

## Implemented (this cycle, 7.30.3) - Eylem zincirleri ve sayfa etiketleri planla birlikte gider

- **An action chain no longer keeps the deleted page alive:** `/Next` (`ISO 32000-1
12.6.2`) and the additional-action dictionaries `/AA` (12.6.3) can carry a step that
  jumps to a page the plan dropped; only that step is removed now, and an action whose
  own destination is gone goes with it.
  `test:crates/pdfcore/tests/probe_page_plan_residue.rs#a_next_chain_drops_only_the_dead_step`
- **An additional action on a kept widget loses just the dead page:**
  `test:crates/pdfcore/tests/probe_page_plan_residue.rs#an_additional_action_on_a_kept_widget_loses_the_dead_page`
  keeps the field value and the control itself.
- **An outline item keeps its destination and loses a dead structure reference:** an
  `/SE` that names a structure element of the deleted page is cleared (12.3.3), while
  `/Dest` stays.
  `test:crates/pdfcore/tests/probe_page_plan_residue.rs#an_outline_item_keeps_its_destination_and_loses_a_dead_structure_reference`
- **Page labels follow the pages that are left:** `/PageLabels` indexes positions
  (12.4.2), so removing a page moved every label after it; the ranges are rebuilt with
  the start of each one moved to the first surviving page and `/St` offset by what was
  removed before it.
  `test:crates/pdfcore/tests/probe_page_plan_residue.rs#page_labels_still_name_the_pages_they_belong_to`
- **The OBJR case was already handled** by the structure pruner; the case now pins it.
  `test:crates/pdfcore/tests/probe_page_plan_residue.rs#an_object_reference_to_a_deleted_page_annotation_leaves_the_structure`

## Implemented (this cycle, 7.30.2) - Yapı ağacı, form alanları ve makale iplikleri de temizlenir

- **A deleted page no longer survives through the structure tree:** tagged documents
  describe each page with `/Pg` (ISO 32000-1 14.7.2) and index the page's content in
  the `/ParentTree` (14.7.4.4). `apply_page_plan` now walks the tree, drops the
  elements that describe a page the plan left out, and removes their parent-tree keys,
  so the page object - and the text on it - is no longer reachable.
  `test:crates/pdfcore/tests/organize_pageref_probe.rs#the_structure_tree_drops_the_deleted_page`
- **Form fields on the deleted page go with it:** a widget names its page with `/P`
  (12.5.6.19) and the form lists it through `/Fields`/`/Kids` (12.7.3.1); a field whose
  widgets all sat there is dropped, or its `/V` would keep the page alive.
  `test:crates/pdfcore/tests/organize_pageref_probe.rs#a_form_field_on_the_deleted_page_goes_with_it`
- **Article threads skip the page:** beads are relinked (`/N`, `/V`) and the thread
  points at the first bead that survives (12.4.3); a thread with no bead left is
  removed from `/Threads`.
  `test:crates/pdfcore/tests/organize_pageref_probe.rs#an_article_thread_skips_the_deleted_page`
- **Extraction gets the same cleanup:** leaving a page out of `extract_pages` is a
  deletion for the output, so the pruning now lives in `apply_page_plan` and covers
  every plan (delete, extract, reorder).
  `test:crates/pdfcore/tests/organize_pageref_probe.rs#extraction_leaves_no_destination_to_a_missing_page`
- **Reordering stays correct without touching destinations:** a destination names a
  page object (12.3.2), so it follows the page.
  `test:crates/pdfcore/tests/organize_pageref_probe.rs#reordering_keeps_a_destination_on_its_page`

## Implemented (this cycle, 7.30.2) - Silinen sayfaya giden her başvuru temizlenir

- **Deleting a page no longer leaves everything that pointed at it behind:** a link's
  destination, an outline entry, the catalog's `/OpenAction` and the `/Names /Dests`
  tree were left naming a page object that was no longer a page. A reader then offered
  links that opened onto nothing, and the reference kept the page object alive - so the
  content of a page the user deleted was still in the file. `delete_pages` now prunes
  all four before it writes: links are removed from `/Annots`, an outline entry keeps
  its title and loses its target, `/OpenAction` and the dead name-tree entries go.
  `test:crates/pdfcore/tests/organize_dest_probe.rs#a_link_to_the_deleted_page_goes_and_a_link_to_a_kept_page_stays`
- **The deleted page's content does not survive:**
  `test:crates/pdfcore/tests/organize_dest_probe.rs#nothing_in_the_output_still_reaches_the_deleted_pages_content`
  reads the reachable streams of the output and fails if the deleted page's mark is
  still drawn, which is the privacy half of the bug.
- **A named destination is resolved before it is judged:**
  `test:crates/pdfcore/tests/organize_dest_probe.rs#a_named_destination_on_the_deleted_page_goes_and_the_other_one_stays`
  covers the `/Names /Dests` tree, and
  `test:crates/pdfcore/tests/organize_dest_probe.rs#an_outline_entry_on_the_deleted_page_keeps_its_title_but_loses_its_target`
  the outline.
- **Known limit:** `/StructTreeRoot`, `/AcroForm` widgets and `/Threads` are not
  pruned; a document that reaches the deleted page through one of them can keep the
  object alive even after this change. (no test: the paths are out of scope for this
  slice and the probe covers the four that were reported.)

## Implemented (this cycle, 7.30.1) - Kısıtları gerçekten bağlayan sahip parolası

- **A restriction is refused when it could not bind:** a reader verifies the owner
  password first, so an owner password that is empty or equal to the user password
  makes whoever opens the file its owner too, and every permission the file declares
  becomes advisory. `protect_pdf` now refuses that combination when at least one
  permission is denied, and accepts it when everything is allowed - there is nothing
  to enforce then.
  `test:crates/pdfcore/tests/permissions_probe.rs#an_owner_password_that_matches_the_user_password_is_refused_when_restrictions_apply`
- **The screen says why instead of sending the user password twice:** the security
  screen no longer falls back to the user password for the owner field; it stops with
  a message that names both ways out.
  `test:src/screens/security/protection-guard.test.ts#refuses an owner password that matches the user password`
- **The permission bits stay pinned:** `test:crates/pdfcore/tests/permissions_probe.rs#one_denied_permission_clears_exactly_its_bits`
  and `test:crates/pdfcore/tests/permissions_probe.rs#every_permission_denied_clears_all_eight_but_keeps_accessibility`
  compare `/P` with the integers ISO 32000-1 Table 22 produces.

## Implemented (this cycle, 7.30.0) - Sağlık raporu bağlantıları ve belgenin kendi kısıtları

- **A health report notices a link that points at a page the document no longer
  has:** a `/Link` annotation's destination is resolved through the page tree and
  through the document's own name tree (ISO 32000-1 12.3.2), and an unresolved one
  earns a `link.broken-internal` warning; a web link is reported as
  `link.external`. When a producer writes both an action and a destination, the
  action is what a reader runs (12.6.2), so only that one is counted.
  `test:crates/pdfcore/tests/inspect_links_probe.rs#a_link_to_a_page_that_is_gone_is_reported`
- **A declared restriction is repeated, not enforced:** the report reads `/P` from
  the file's own `/Encrypt` dictionary (7.6.4.2) - the parsed document cannot answer
  it, because a PDF library decrypts on load and drops the dictionary - and says
  which permissions the document denies. An owner-password file with an empty user
  password opens without a prompt and still declares restrictions, which is the case
  `test:crates/pdfcore/tests/inspect_links_probe.rs#a_declared_restriction_is_reported_and_a_fully_open_document_is_not`
  covers; a fully open document reports nothing.
- **Neither finding changes a document:** they are report rows, so no new struct
  field, no binding change and no screen work - the findings list already renders
  whatever the backend sends. (no test: the claim is about what the change does not
  do; the behaviour it adds is covered by the two markers above.)

## Implemented (this cycle, 7.29.1) - Kendi font adını koruyan belgeler

- **A second stamp no longer draws the first one in the wrong font:** a stamp,
  page number or bundle label registers its font as `/OmniTr` on the page. A page
  that already carried one - every file this app has stamped before - had that
  resource overwritten, so the earlier text was drawn with the new subset and came
  out garbled. Registration now takes the next free name (ISO 32000-1 7.8.3) and
  the drawn text refers to the name it was given.
  `test:crates/pdfcore/tests/evidence_content_probe.rs#a_font_name_the_exhibit_already_uses_is_not_reused`
- **An exhibit's own resources survive a bundle:** inherited `/Resources` are
  resolved onto the page rather than replaced,
  `test:crates/pdfcore/tests/evidence_content_probe.rs#an_inherited_resource_stays_on_the_page`;
  two pages that share one content stream each get their own label,
  `test:crates/pdfcore/tests/evidence_content_probe.rs#two_pages_sharing_a_content_stream_each_get_their_own_label`;
  and compressed content still decodes to the exhibit's own text,
  `test:crates/pdfcore/tests/evidence_content_probe.rs#a_compressed_exhibit_keeps_its_text`.

## Implemented (this cycle, 7.29.0) - Filigran, damga ve kanıt etiketi görünen kutuda

- **A watermark sits on the region the viewer shows:** the artwork was placed from
  the media box alone, so a page whose box starts away from `(0, 0)` moved the
  mark with the origin, a reversed box turned the page size negative, and a
  cropped page centred the watermark on the media box instead of the visible
  region. The placement now uses the crop box clipped to the media box
  (ISO 32000-1 14.11.2), whose corner starts the page coordinates (7.7.3.3).
  `test:crates/pdfcore/tests/visible_box_marks_probe.rs#a_centre_watermark_sits_at_the_centre_of_the_crop_box`
- **Page numbers and headers follow the same box:**
  `test:crates/pdfcore/tests/visible_box_marks_probe.rs#a_footer_follows_the_page_box_origin`
  and `test:crates/pdfcore/tests/visible_box_marks_probe.rs#a_footer_sits_on_the_crop_box_corner`
  cover the stamp path.
- **The exhibit label follows it too:**
  `test:crates/pdfcore/tests/visible_box_marks_probe.rs#a_watermark_on_a_rotated_cropped_page_stays_inside_the_box`
  pins the rotation case, and the shared rule lives in `pagegeom` so the three
  call sites cannot drift apart again.

## Implemented (this cycle, 7.29.0) - Açıklama görünen kutuya oturur

- **An annotation lands where it was drawn on a page that does not start at the
  origin:** the display rectangle was converted into page coordinates as if every
  page began at `(0, 0)`, so a page whose `/MediaBox` starts elsewhere - what a crop
  or an imposition produces, including this app's own resize output - put the mark
  that far off. The origin is carried through the conversion now, on the editable
  annotation path and on the drawn stamps.
  `test:crates/pdfcore/tests/annotate_probe.rs#a_mark_follows_the_page_box_origin`
- **A reversed `/MediaBox` keeps the mark on the page:** a box written as
  `[595 842 0 0]` (ISO 32000-1 7.9.5) gave a negative page width, so the annotation
  landed outside the page. The box is normalised before it is measured.
  `test:crates/pdfcore/tests/annotate_probe.rs#a_reversed_box_lands_like_a_normal_one`
- **An annotation lands on the box the viewer shows:** the canvas reports display
  space against the region a viewer draws - the crop box clipped to the media box
  (ISO 32000-1 14.11.2) - and both paths sized the page from the media box alone,
  so every mark on a cropped page moved by the crop offset. The bake path
  (`annotate_pdf`) also dropped the box origin, moving the mark on any page whose
  box does not start at `(0, 0)` and pushing it off a reversed one. Both paths take
  their size and their origin from that box now.
  `test:crates/pdfcore/tests/annotate_box_probe.rs#a_mark_lands_where_the_crop_box_shows_the_page`
- **The baked mark follows the same box:** `test:crates/pdfcore/tests/annotate_box_probe.rs#a_baked_mark_lands_where_the_crop_box_shows_the_page`
  and `test:crates/pdfcore/tests/annotate_box_probe.rs#a_baked_mark_follows_the_box_origin`
  cover the drawn path, and `test:crates/pdfcore/tests/annotate_box_probe.rs#a_crop_box_is_clipped_to_the_media_box`
  pins the intersection rule.
- **The rest of the path is pinned by the same probe:** the baseline page still puts
  the mark where the display rectangle says, an inherited `/MediaBox` behaves like a
  direct one, and an existing link survives while an indirect `/Annots` array stays a
  list.
  `test:crates/pdfcore/tests/annotate_probe.rs#a_mark_lands_where_the_page_starts_at_the_origin`
  `test:crates/pdfcore/tests/annotate_probe.rs#an_inherited_box_lands_like_a_direct_one`
  `test:crates/pdfcore/tests/annotate_probe.rs#an_existing_link_survives_and_an_indirect_list_stays_a_list`
- **Still open:** rotation, the crop box, ink and line coordinates, and signing are
  not probed yet.
  (no test: those paths are outside this slice.)

## Implemented (this cycle, 7.27.0) - Denetim (inspect) probu

- **A document with no metadata title is reported as such:** the
  `a11y.missing-title` finding used to test the title the screen shows, which falls
  back to the file name, so a document whose `/Title` is empty came back clean. The
  display keeps that fallback; the finding now reads the document's own title, which
  is what conformance asks for (PDF/UA 7.1).
  `test:crates/pdfcore/tests/inspect_probe.rs#a_document_with_no_metadata_is_told_what_is_missing`
  `test:crates/pdfcore/src/inspect.rs#a_file_name_stands_in_for_display_but_not_for_conformance`
- **The rest of the inspector is pinned by the same probe:** a complete document
  reports no accessibility error, a figure without `/Alt` is counted and reported,
  the page and object counts follow the file, an outline survives read → write →
  read with its titles, destinations and nesting, the writer refuses a destination
  outside the document, a document without an outline reports none, and a cyclic
  `/Next` chain terminates.
  `test:crates/pdfcore/tests/inspect_probe.rs#a_document_that_is_fine_gets_no_errors`
  `test:crates/pdfcore/tests/inspect_probe.rs#a_figure_without_alt_text_is_an_error`
  `test:crates/pdfcore/tests/inspect_probe.rs#the_page_and_object_counts_match_the_file`
  `test:crates/pdfcore/tests/inspect_probe.rs#an_outline_survives_a_round_trip_through_the_writer`
  `test:crates/pdfcore/tests/inspect_probe.rs#the_writer_refuses_a_destination_outside_the_document`
  `test:crates/pdfcore/tests/inspect_probe.rs#a_document_without_an_outline_reports_none`
  `test:crates/pdfcore/tests/inspect_probe.rs#a_cyclic_outline_terminates`
- **Still open:** an unembedded font is reported as a `pdfa.*` finding, which this
  probe leaves alone; link-target resolution, encryption and form-appearance rules
  are not probed yet.
  (no test: those paths are outside this slice.)

## Implemented (this cycle, 7.26.0) - Flatten probu

- **Flatten skips a hidden annotation instead of baking it in:** an annotation
  whose `/F` carries the Hidden bit (ISO 32000-1 12.5.3) is never displayed, so
  painting its appearance added a stamp or note to the flattened file that was not
  in the document the user saw. It is now left in place and reported.
  `test:crates/pdfcore/tests/flatten_probe.rs#a_hidden_annotation_is_not_drawn`
- **The rest of the contract is pinned by the same probe:** an appearance lands on
  its `/Rect` (also when the box is smaller or the rectangle reversed), an
  annotation without the print flag is still drawn because it is visible on screen,
  and an annotation with no appearance is either drawn or kept and reported rather
  than silently dropped.
  `test:crates/pdfcore/tests/flatten_probe.rs#an_appearance_lands_on_its_rect`
  `test:crates/pdfcore/tests/flatten_probe.rs#a_smaller_bbox_is_scaled_onto_the_rect`
  `test:crates/pdfcore/tests/flatten_probe.rs#a_reversed_rect_is_normalised`
  `test:crates/pdfcore/tests/flatten_probe.rs#an_annotation_without_the_print_flag_is_still_drawn`
  `test:crates/pdfcore/tests/flatten_probe.rs#an_appearance_without_a_box_is_reported`
- **Still open:** `/AP /Matrix` is read but not pinned by a case yet, widgets and
  field values are not in this probe, and `/NoRotate`/`/OC` annotations are out of
  scope.
  (no test: those paths are untouched by this slice.)

## Implemented (this cycle, 7.25.0) - Resize ve Crop probu

- **Resize moves a page's annotations with its content:** the content went through
  the resize matrix while the marks on it stayed put, so a highlight, a note, an
  ink stroke and a line ended up beside the words they were drawn on. Their
  `/Rect`, `/QuadPoints`, `/InkList`, `/L` and `/Vertices` now go through the same
  matrix.
  `test:crates/pdfcore/tests/pagelayout_probe.rs#a_highlight_stays_on_the_text_after_resize`
- **Crop honours the page's own origin:** a `/MediaBox` that does not start at
  `(0, 0)` - common in scans and print-ready exports - shifted the cropped region
  by that origin, so the rectangle the user drew selected a different part of the
  page.
  `test:crates/pdfcore/tests/pagelayout_probe.rs#crop_lands_on_the_visible_region_of_an_offset_page`
  `test:crates/pdfcore/tests/pagelayout_probe.rs#resize_sets_the_requested_media_box`
- **Still open:** link and outline destinations are not moved by a resize (a
  follow-up), and `/UserUnit` pages are reported but not converted.
  (no test: those paths are untouched by this slice and their expectations are not
  settled.)

## Implemented (this cycle, 7.24.0) - Önbellek belleği ölçülerek kanıtlandı

- **The font cache holds no more than its budget, measured:** a counting global
  allocator compares the peak of an imposition with the cache against the peak
  without it. With sixty-four composite fonts and a 4 MiB budget the cached run
  stays within the budget of the uncached baseline, and a cache that fits its fonts
  lowers the peak instead of raising it (about 6 MB against 21 MB on that fixture),
  because an uncached run builds and drops one map per note. The output is
  byte-identical either way.
  `test:crates/pdfcore/tests/font_cache_memory.rs#the_cache_never_holds_more_than_its_budget`
  `test:crates/pdfcore/tests/font_cache_memory.rs#a_cache_that_fits_avoids_the_per_note_maps`
- **A real-size map is measured too:** the benchmark's `composite-large` scenario
  carries a twenty-thousand-entry `/ToUnicode`, four thousand `/W` entries and a
  full `/CIDToGIDMap`. With the cache on it costs 1.35x the small map (223 ms
  against 166 ms for 2000 notes) - under the 1.5x gate that would have justified a
  binary-search index, so no index was added. Without the cache the same document
  spends 13.3 s.
  `test:crates/pdfcore/src/nup/markup/appearance/cidfont/cache/tests.rs#the_same_font_is_read_once_however_many_notes_use_it`
- **Still open:** Type3 fonts, a cache shared across documents, and an Android
  on-device memory test (needs an emulator).
  (no test: the emulator and the app crate are not available on this host.)

## Implemented (this cycle, 7.23.0) - Basit font önbelleği ve ölçüm

- **Simple fonts are cached per document too, and the cache is measured:** the
  `/Widths`, `/Encoding`, `/Differences` and descriptor of a `/DR` font were
  re-read for every note. Both font kinds share one cache keyed by the font's
  object id - never its resource name, since two pages may call different fonts
  `/F1`. A budget of zero stores nothing, which is exactly how the code behaved
  before, so the benchmark runs both modes on one document and compares the bytes
  before printing: on 500 pages with four notes each (2000 notes, release build,
  median of five), a composite font went from 896 ms to 183 ms and a simple font
  from 205 ms to 178 ms.
  `test:crates/pdfcore/src/nup/markup/appearance/cidfont/cache/tests.rs#the_same_font_is_read_once_however_many_notes_use_it`
  `test:crates/pdfcore/src/nup/markup/appearance/cidfont/cache/tests.rs#a_font_past_the_budget_is_used_but_not_stored`
  `test:crates/pdfcore/src/nup/markup/appearance/cidfont/cache/tests.rs#two_fonts_get_two_entries`
- **Still open:** Type3 fonts, a cache shared across calls or documents, and any
  timing gate in CI (noisy across three operating systems).
  (no test: those are the paths this slice deliberately leaves alone.)

## Implemented (this cycle, 7.22.0) - Font başına tek okuma

- **Each composite font is read once per document, not once per note:** N-up
  imposition creates a cache inside the call that carries the annotations and
  drops it when that call returns, because an object id means something in one
  document and nothing in another. A refusal is cached as well as a success - the
  reads that prove a font unusable are the same reads - so a font that cannot be
  used is not re-read for every note. The budget is enforced at insert time: a
  font that would take the cache past it is used but not stored, and nothing is
  ever evicted. The output is byte-identical.
  `test:crates/pdfcore/src/nup/markup/appearance/cidfont/cache/tests.rs#the_same_font_is_read_once_however_many_notes_use_it`
  `test:crates/pdfcore/src/nup/markup/appearance/cidfont/cache/tests.rs#two_fonts_get_two_entries`
  `test:crates/pdfcore/src/nup/markup/appearance/cidfont/cache/tests.rs#a_font_that_cannot_be_used_is_remembered_too`
  `test:crates/pdfcore/src/nup/markup/appearance/cidfont/cache/tests.rs#a_font_past_the_budget_is_used_but_not_stored`
  `test:crates/pdfcore/src/nup/markup/appearance/cidfont/cache/tests.rs#the_heap_estimate_grows_with_the_font`
- **Still open, unchanged:** the cache covers the composite-font path only;
  simple fonts' `/Widths`, rendering and text extraction still read per call,
  and no wall-clock benchmark is published.
  (no test: those are the paths this slice deliberately leaves alone.)

## Implemented (this cycle, 7.21.0) - Tek baytlık ve karışık kod uzayları

- **A note can be drawn with a font whose codes are one byte wide:** an embedded
  `/Encoding` CMap may declare a one-byte codespace, or one-byte and two-byte
  ranges together (the Shift-JIS shape). The width is part of the code, so the
  map keeps a table per width, admits a code only inside a range of its own
  width, and writes each code at its own length - a note may carry `<41>` and
  `<8140>` in one string. `/W` and `/CIDToGIDMap` are still looked up by CID.
  A one-byte range inside a two-byte lead byte refuses the whole font: a viewer
  would split the string at the wrong place and draw wrong glyphs with no error.
  A map with only two-byte ranges keeps the older, lenient reading, so a
  document that works today keeps its exact bytes.
  `test:crates/pdfcore/tests/nup_freetext_cid_narrow.rs#a_mixed_codespace_writes_each_code_at_its_length`
  `test:crates/pdfcore/tests/nup_freetext_cid_narrow.rs#a_one_byte_space_is_not_word_spacing`
  `test:crates/pdfcore/tests/nup_freetext_cid_narrow.rs#an_overlapping_codespace_falls_back_byte_identical`
  `test:crates/pdfcore/src/cmap/cid/tests.rs#a_one_byte_codespace_selects_one_byte_codes`
  `test:crates/pdfcore/src/cmap/cid/tests.rs#a_shift_jis_codespace_mixes_widths`
  `test:crates/pdfcore/src/cmap/cid/tests.rs#a_one_byte_range_inside_a_two_byte_lead_is_refused`
  `test:crates/pdfcore/src/cmap/code/tests.rs#the_hex_body_follows_the_width`
- **Still open, unchanged:** predefined CMap names, `/UseCMap` pointing at a
  stream, vertical writing, and codes longer than two bytes.
  (no test: these are the refusals asserted above; what is missing is the
  support, not the check.)

## Implemented (this cycle, 7.20.0) - Gömülü `/Encoding` CMap

- **A note can be drawn with a font that embeds its own `/Encoding` CMap:** the
  stream is a small code-to-CID program, and it is read so a font whose codes are
  not their own CIDs keeps its face instead of falling back to Helvetica. Only a
  two-byte codespace is accepted: `/ToUnicode` does not report the length of the
  code it inverted, so a one-byte font would shift every glyph after the mistake
  without any error. An unknown base CMap (`90ms-RKSJ-H` and friends) is refused
  for the same reason - it brings its own codespace with it - while `Identity-H`
  as a base is accepted and overridden by the stream's own entries. Advances are
  looked up by CID, which is what `/W` is keyed by, and `/W` itself still refuses
  the whole font when it is malformed rather than guessing a default width.
  `test:crates/pdfcore/tests/nup_freetext_cid_cmap.rs#an_embedded_cmap_draws_the_code_keyed_widths_by_cid`
  `test:crates/pdfcore/tests/nup_freetext_cid_cmap.rs#a_one_byte_codespace_falls_back_like_a_refused_encoding`
  `test:crates/pdfcore/tests/nup_freetext_cid_cmap.rs#latin_text_is_byte_identical_with_or_without_the_font`
  `test:crates/pdfcore/tests/nup_freetext_cid_cmap.rs#a_character_the_map_does_not_cover_leaves_the_note_out`
  `test:crates/pdfcore/tests/nup_freetext_cid_cmap.rs#the_font_is_referenced_and_its_cmap_kept_once`
  `test:crates/pdfcore/src/cmap/cid/tests.rs#an_identity_equivalent_stream_selects_each_code_as_its_cid`
  `test:crates/pdfcore/src/cmap/cid/tests.rs#an_identity_base_fills_the_codes_the_stream_does_not_map`
  `test:crates/pdfcore/src/cmap/cid/tests.rs#a_vertical_or_unknown_base_is_refused`
  `test:crates/pdfcore/src/cmap/cid/tests.rs#a_codespace_wider_than_two_bytes_is_refused`
  `test:crates/pdfcore/src/cmap/cid/tests.rs#a_malformed_entry_rejects_the_map`
  `test:crates/pdfcore/src/cmap/cid/tests.rs#entries_that_disagree_are_refused_and_ones_that_agree_merge`
  `test:crates/pdfcore/src/nup/markup/appearance/cidfont/mapping/tests.rs#an_encoding_stream_may_be_a_reference`
  `test:crates/pdfcore/src/nup/markup/appearance/cidfont/mapping/tests.rs#widths_are_keyed_by_cid_and_notdef_never_draws`
  `test:crates/pdfcore/src/nup/markup/appearance/cidfont/mapping/tests.rs#a_vertical_stream_is_refused`
- **Still open, unchanged:** one-byte and mixed code spaces, predefined CMap
  names, `/UseCMap` pointing at a stream, vertical writing, and caption text.
  (no test: these are the refusals asserted above; what is missing is the
  support, not the check.)

## Implemented (this cycle, 7.19.0) - Belgenin kendi kompozit fontu

- **A document's `/ToUnicode` map is read (and inverted):** a Type0 font names
  its glyph codes and maps them back to characters; drawing with such a font
  needs the other direction. The reader is lenient on purpose - a broken entry
  costs only itself - keeps the lowest code when a character is listed twice,
  skips any destination that is not exactly one character (a ligature, a lone
  surrogate) and never draws a placeholder, and bounds the stream before it
  allocates. The fuzz target found a real overflow on its first run: a
  destination spanning all eight bytes shifted a `u64` by its own width.
  `test:crates/pdfcore/src/cmap/tests.rs#a_bfrange_increments_the_destination`
  `test:crates/pdfcore/src/cmap/tests.rs#a_destination_of_any_width_up_to_eight_bytes_is_fine`
  `test:crates/pdfcore/src/cmap/tests.rs#the_lowest_code_wins_for_a_character_listed_twice`
- **A free-text note is drawn in the document's own composite font:** an
  embedded `Type0` font with `/Identity-H` codes is read from
  `/DescendantFonts`, its advances from `/W` and `/DW`, its characters from
  `/ToUnicode`, and the note is printed in it by reference - the font, its
  descriptor and its program are not copied. A `/CIDToGIDMap` that sends a used
  code to glyph 0 costs that character. Everything else falls through to
  WinAnsi, then the bundled face, then no text at all; a malformed `/W` rejects
  the whole font rather than guess a default width for one code.
  `test:crates/pdfcore/tests/nup_freetext_cid.rs#a_cjk_note_is_drawn_in_the_document_font`
  `test:crates/pdfcore/tests/nup_freetext_cid.rs#a_missing_glyph_keeps_the_note_out`
  `test:crates/pdfcore/tests/nup_freetext_cid.rs#latin_text_still_uses_winansi`
  `test:crates/pdfcore/tests/nup_freetext_cid.rs#two_notes_share_one_font_object`
  `test:crates/pdfcore/src/nup/markup/appearance/cidfont/tests.rs#an_identity_h_cid_font_is_read`
  `test:crates/pdfcore/src/nup/markup/appearance/cidfont/tests.rs#an_identity_v_or_predefined_encoding_is_refused`
  `test:crates/pdfcore/src/nup/markup/appearance/cidfont/tests.rs#a_font_without_a_program_is_refused`
  `test:crates/pdfcore/src/nup/markup/appearance/cidfont/tests.rs#broken_widths_reject_the_font`
  `test:crates/pdfcore/src/nup/markup/appearance/cidfont/widths.rs#both_forms_are_read_and_mixed`
  `test:crates/pdfcore/src/nup/markup/appearance/cidfont/widths.rs#random_arrays_never_panic`
- **Still open, unchanged:** vertical writing (`/Identity-V`), predefined and
  embedded CMaps, non-embedded programs, caption text (a `/Cap` annotation
  carries no `/DA`), and notes with more than 255 distinct characters.
  (no test: these are the refusals above, which the reader tests assert one by
  one; what is missing is the support, not the check.)

## Implemented (this cycle, 7.18.0) - `/BE` bulut kenarlıklar + iki zarf (desktop + Android)

- **A LAN download resumes where it stopped (E6.3):** uploads have resumed since
  D3; a download was one request that started over when the Wi-Fi dropped. Two
  operations join the protocol - `begin_get` announces size, hash and mtime, and
  `get_chunk` serves a window from an offset and refuses when the document
  changed under the client. The partial file lives in `.parts` with a note that
  says which document it belongs to, so a resume never splices two versions; the
  assembled file is hashed before it reaches the caller and drops back to zero
  on a mismatch, up to three attempts. A host from before this change answers
  "unknown operation" and the client falls back to the whole-file `get`, so
  mixed versions keep working.
  `test:crates/synccore/src/lan/download_tests.rs#a_chunked_download_resumes_from_the_partial_file`
  `test:crates/synccore/src/lan/download_tests.rs#a_download_restarts_when_the_host_file_changed`
  `test:crates/synccore/src/lan/download_tests.rs#a_corrupted_partial_is_discarded_after_the_hash_check`
  `test:crates/synccore/src/lan/download_tests.rs#a_download_chunk_checks_offsets_sizes_and_paths`
  `test:crates/synccore/src/lan/download_tests.rs#an_empty_document_downloads_without_chunks`
  `test:crates/synccore/src/lan/download_tests.rs#an_unknown_operation_is_recognised_for_the_whole_file_fallback`

- **N-up keeps links and bookmarks (E6.3):** imposing a document rebuilds the
  page tree, and a source page becomes a Form XObject that cannot carry
  annotations - so links and the outline used to disappear. `/Link` annotations
  are copied onto the sheet that carries their page: the rectangle goes through
  the placement matrix (source rotation included), is clipped to the cell, and
  the target is rewritten - a named destination resolves through `/Names /Dests`
  into an explicit array. The outline is rebuilt with its hierarchy, colours,
  flags and the signed `/Count` rule; entries whose page falls outside the
  selection lose their target or disappear, and a stale outline is removed
  instead of left dangling. Markup and widget annotations are still dropped on
  purpose.
  `test:crates/pdfcore/tests/nup_links_test.rs#uri_link_rect_lands_in_its_cell`
  `test:crates/pdfcore/tests/nup_links_test.rs#goto_link_targets_second_sheet`
  `test:crates/pdfcore/tests/nup_links_test.rs#named_destinations_are_resolved_to_explicit_arrays`
  `test:crates/pdfcore/tests/nup_outline_test.rs#a_flat_outline_lands_on_the_sheets`
  `test:crates/pdfcore/tests/nup_outline_test.rs#nested_entries_keep_their_counts`
  `test:crates/pdfcore/tests/nup_outline_test.rs#an_outline_with_no_surviving_target_does_not_appear`

- **Printable markup is flattened into the N-up sheets:** fifteen annotation
  kinds (`/Text` through `/Stamp`) are painted when their `/F` flag carries
  Print and not Hidden, using the `/AP /N` stream (or the `/AS` state) without
  copying it: the box goes through the appearance `/Matrix` onto the annotation
  rectangle and then onto the sheet, `/NoRotate` keeps a note upright, `/CA`,
  `/ca` and `/BM` become a graphics state, and the block is clipped to the
  painted page. Widgets, popups, links, redactions and annotations without an
  appearance stream are skipped on purpose.
  `test:crates/pdfcore/tests/nup_markup_test.rs#a_printable_square_is_painted_on_the_sheet`
  `test:crates/pdfcore/tests/nup_markup_test.rs#norotate_keeps_the_markup_upright`
  `test:crates/pdfcore/tests/nup_markup_test.rs#annotation_opacity_becomes_a_graphics_state`
  `test:crates/pdfcore/tests/nup_markup_edge_test.rs#broken_appearances_are_skipped_one_by_one`
  `test:crates/pdfcore/tests/nup_markup_edge_test.rs#a_link_only_document_is_left_alone`

- **Form widgets are flattened into the N-up sheets when they can be
  trusted:** the field type (`/FT`, inherited through `/Parent` with a depth and
  cycle guard) decides. `/Btn` buttons are painted from their `/AS` state, a
  `/Tx` or `/Ch` field is painted only when the document does not set
  `/AcroForm /NeedAppearances` (a stale appearance could print an old value),
  and a `/Sig` field is painted only when it carries a value - an unsigned
  signature stays a placeholder. A widget without the print flag is skipped,
  and a broken field dictionary drops only that widget.
  `test:crates/pdfcore/tests/nup_widget_test.rs#a_checkbox_draws_its_on_state`
  `test:crates/pdfcore/tests/nup_widget_test.rs#need_appearances_holds_back_text_but_not_buttons`
  `test:crates/pdfcore/tests/nup_widget_test.rs#an_unsigned_signature_field_stays_a_placeholder`
  `test:crates/pdfcore/tests/nup_widget_test.rs#the_field_type_is_inherited_from_the_parent`

- **Marks without an appearance stream are synthesized:** the quad-point kinds
  (`Highlight` filled with a Multiply blend, `Underline`, `StrikeOut`,
  `Squiggly`) and the shapes (`Square`, `Circle`, `Line`, `Ink`) are drawn from
  the annotation's own geometry when it ships no `/AP`, honoring `/C`, `/IC` and
  the `/BS` or legacy `/Border` width. The operators live in page space inside a
  form, so the placement matrix, `/NoRotate` and the cell clip apply unchanged;
  an annotation with an `/AP` is never re-synthesized, and a malformed one drops
  only its own mark.
  `test:crates/pdfcore/tests/nup_appearance_test.rs#a_highlight_is_filled_and_blends`
  `test:crates/pdfcore/tests/nup_appearance_test.rs#a_square_strokes_and_fills`
  `test:crates/pdfcore/tests/nup_appearance_test.rs#ink_draws_every_stroke`
  `test:crates/pdfcore/tests/nup_appearance_test.rs#an_annotation_with_a_broken_appearance_is_not_synthesized`

- **Border styles are synthesized:** `/BS /S` `B` paints a light top/left and a
  dark bottom/right band inside the frame (tones a quarter toward white or
  black from the stroke colour), `I` swaps them and `U` keeps only the bottom
  edge. Squares, free-text notes, circles (two half-arcs) and polygons (one tone
  per edge) follow; a shape smaller than four widths falls back to the plain
  frame, and lines, polylines and ink keep a plain stroke.
  `test:crates/pdfcore/tests/nup_border_styles_test.rs#a_beveled_square_paints_two_tones`
  `test:crates/pdfcore/tests/nup_border_styles_test.rs#an_inset_square_swaps_the_tones`
  `test:crates/pdfcore/tests/nup_border_styles_test.rs#an_underline_keeps_only_the_bottom_edge`
  `test:crates/pdfcore/tests/nup_border_styles_test.rs#a_beveled_polygon_colours_each_edge`

- **Line leaders are synthesized:** a `Line` with `/LL` is drawn beside its
  anchors with a leader from each one; `/LLE` extends the leader past the line,
  `/LLO` leaves a gap at the anchor, a negative `/LL` flips the side, `/LE`
  endings attach to the shifted line, the box covers the leaders and a dashed
  line dashes them too. A collapsed line (both anchors equal) has no direction
  and is skipped.
  `test:crates/pdfcore/tests/nup_polish_test.rs#a_positive_leader_draws_the_line_beside_its_anchors`
  `test:crates/pdfcore/tests/nup_polish_test.rs#a_leader_extension_runs_past_the_line`
  `test:crates/pdfcore/tests/nup_polish_test.rs#a_leader_offset_leaves_a_gap_at_the_anchor`
  `test:crates/pdfcore/tests/nup_polish_test.rs#line_ends_sit_on_the_shifted_line`

- **Free-text notes reuse the document's simple font:** a note whose `/DA` names
  a `/DR` Type1, MMType1 or TrueType font with `/Widths` is set in that font,
  with the encoding read from WinAnsi and any `/Differences`; advances come from
  the font's own widths, so line breaking matches the document. A composite or
  Type3 font, a symbolic TrueType without an encoding, a font without widths and
  a glyph the font cannot show all fall back to the standard-14 path as a whole.
  `test:crates/pdfcore/tests/nup_freetext_dr.rs#a_document_font_is_referenced_instead_of_a_standard_one`
  `test:crates/pdfcore/tests/nup_freetext_dr.rs#narrow_widths_change_the_line_breaking`
  `test:crates/pdfcore/tests/nup_freetext_dr.rs#differences_carry_turkish_characters_through_the_document_font`
  `test:crates/pdfcore/tests/nup_freetext_dr.rs#a_composite_font_is_refused`

- **Line endings are synthesized:** a `Line`, `PolyLine` or free-text callout
  honours `/LE` - open/closed arrows in both directions, square, circle,
  diamond, butt and slash - drawn at `max(6, 3w)` with the line's own width and
  filled with `/IC` for the closed kinds. Each mark is solid even on a dashed
  line, and the form's box grows so the head is not clipped.
  `test:crates/pdfcore/tests/nup_line_ends_test.rs#closed_arrows_sit_at_both_ends_of_a_diagonal_line`
  `test:crates/pdfcore/tests/nup_line_ends_test.rs#the_closed_kinds_fill_when_the_line_has_an_interior_colour`
  `test:crates/pdfcore/tests/nup_line_ends_test.rs#a_free_text_callout_gets_its_end_at_the_first_point`
  `test:crates/pdfcore/tests/nup_line_ends_test.rs#a_reversed_arrow_points_past_the_tip`

- **Polygons, polylines and dashed borders are synthesized:** a `Polygon` or
  `PolyLine` without an `/AP` is drawn from `/Vertices` (the closed one filled
  with `/IC` and stroked with `/C`, the open one only stroked), and `/BS /S /D`
  writes a `d` operator before the shape. A broken pattern or a solid style
  falls back to a solid line, and a legacy `/Border`'s fourth entry is read too.
  `test:crates/pdfcore/tests/nup_shape_polish_test.rs#a_polygon_is_filled_and_stroked`
  `test:crates/pdfcore/tests/nup_shape_polish_test.rs#a_polyline_stays_open_and_is_never_filled`
  `test:crates/pdfcore/tests/nup_shape_polish_test.rs#a_dashed_border_is_written_before_the_shape`
  `test:crates/pdfcore/tests/nup_shape_polish_test.rs#a_broken_pattern_draws_a_solid_border`

- **Free-text notes are synthesized with a standard font:** a `FreeText`
  annotation without an `/AP` is laid out from `/Contents`, `/DA` (falling back
  to `/AcroForm /DA`), `/Q`, `/C`, `/RD` and `/CL`, using a Helvetica, Times or
  Courier member referenced by name. The width tables come from the Adobe
  Core14 AFM metrics and the glyphs WinAnsi lacks are declared through a fixed
  `/Differences` table, so Turkish prints without an embedded font. Text none
  of the available faces can show drops the whole note; text outside WinAnsi
  that the bundled face carries is set in a subset of it instead.
  `test:crates/pdfcore/tests/nup_freetext_test.rs#a_free_text_note_is_laid_out_with_a_standard_font`
  `test:crates/pdfcore/tests/nup_freetext_test.rs#the_form_default_appearance_is_the_fallback`
  `test:crates/pdfcore/tests/nup_freetext_subset_test.rs#a_note_no_face_can_show_is_dropped`

- **The toast stack clears the bottom bar (6.5.1):** `.toast-host` offsets by
  `env(safe-area-inset-bottom)` everywhere and, below 720 px, by the bottom bar
  plus its inset; the stack is `pointer-events: none` with only the cards taking
  the pointer.
  `test:src/components/touch-css.test.ts#clears the home indicator at the foot of the screen`
  `test:src/components/touch-css.test.ts#sits above the bottom bar on a phone`
  `test:src/components/touch-css.test.ts#stops a full-width toast from swallowing taps behind it`

- **One password field instead of two:** `src/components/password-field.tsx`,
  with `focusOnMount` naming the autofocus behaviour (the a11y rule flags an
  `autoFocus` prop even on a custom component). Security 263 -> 241 lines,
  converter 296 -> 262.
  `test:src/components/password-field.test.tsx#toggles the input type between hidden and visible`
  `test:src/components/password-field.test.tsx#never submits the form around it`
  `test:src/components/password-field.test.tsx#labels the toggle from the dictionary, following the language`

- **The i18n baseline stands at 58/12/6/0 with three declared literals:** the
  expression scan's last four leaks are translated and the marker has its own
  rules (a real reason, one finding per marker, no stale markers, hard cap 5).
  `test:src/lib/ci-workflow.test.ts#records a hardcoded-JSX baseline the audit compares against`
  `test:src/lib/ci-workflow.test.ts#caps the deliberate literals a marker may silence`

## Implemented (this cycle, 7.6.0) - Sürüm hatları ve tedarik zinciri (E5)

- **The toast stack clears the bottom bar (6.5.1 device check):** a toast sat
  `bottom: 16px` from the viewport, which on a phone covered the navigation bar
  and reached into the system gesture area. `.toast-host` now offsets by
  `env(safe-area-inset-bottom)` everywhere and, below 720 px, by the bottom bar
  plus its inset as well; the stack is `pointer-events: none` with only the
  cards taking the pointer, so a full-width toast cannot swallow a tap meant for
  the screen behind it.
  `test:src/components/touch-css.test.ts#clears the home indicator at the foot of the screen`
  `test:src/components/touch-css.test.ts#sits above the bottom bar on a phone`
  `test:src/components/touch-css.test.ts#stops a full-width toast from swallowing taps behind it`
- **The last four English strings the expression scan could see are gone
  (E4.7):** `SlideObjectView` told the Turkish Impress UI "Double-click to add an
  image", the autosave buttons wrote `30 s` / `5 min` with English units, the
  signature panel's default checklist read "Yes / Off", and the page-numbering
  example said "Page 1". All four come from the dictionary now (the page example
  reuses `common.pageAlt`), which took the baseline to **58/12/6**, with three
  declared literals: the Calc structured-reference placeholder, the link
  dialog's sample address and a filename example's extensions.
  `test:src/lib/ci-workflow.test.ts#records a hardcoded-JSX baseline the audit compares against`
  `test:src/lib/ci-workflow.test.ts#caps the deliberate literals a marker may silence`

- **A deliberate literal can be declared, not just counted (E4.7):** the object
  scan has had an `i18n-content:` pragma for data that must not be translated;
  the JSX side had nothing, so a unit or a formula example either inflated the
  baseline forever or was "fixed" by translating something that must not be.
  A `// i18n-literal: reason` comment (or `{/* … */}` above a child) now
  silences exactly one finding, and the rules make it hard to abuse: the reason
  must be real (12+ characters, not `todo`), a marker that binds to nothing or
  to two attributes fails the run, a `//` line inside JSX text is reported
  rather than obeyed, and the total is both ratcheted in the baseline and hard
  capped at five. `--literals-report` prints each one with its reason. The
  schema moves to v5 for the count; one marker exists today, on the Calc
  table dialog's structured-reference placeholder.
  `test:src/lib/i18n-jsx-audit.test.ts#excludes an attribute marked by a comment on the line above`
  `test:src/lib/i18n-jsx-audit.test.ts#rejects a marker without a reason and keeps counting the text`
  `test:src/lib/i18n-jsx-audit.test.ts#reports a marker that binds to no finding`
  `test:src/lib/i18n-jsx-audit.test.ts#reports a marker whose line holds two different attributes`
  `test:src/lib/i18n-jsx-audit.test.ts#treats a // line inside JSX children as text, not as a marker`
  `test:src/lib/ci-workflow.test.ts#caps the deliberate literals a marker may silence`

- **One password field instead of two (E4.7 follow-up):** the Protect/Unlock
  panels and the converter each carried their own copy of the same component,
  and they had already drifted: one set `autoComplete="new-password"`, the other
  `"off"`. `src/components/password-field.tsx` now holds the implementation and
  the difference is a prop; the two screens shrank by 22 and 34 lines and their
  file budgets came down with them. The behaviour the copies shared is pinned by
  its own test file, including the two that would bite a user: the toggle is
  `type="button"` (a stray submit would start a PDF job with a half-typed
  password) and its `aria-label` follows the interface language.
  `test:src/components/password-field.test.tsx#toggles the input type between hidden and visible`
  `test:src/components/password-field.test.tsx#never submits the form around it`
  `test:src/components/password-field.test.tsx#labels the toggle from the dictionary, following the language`
  `test:src/components/password-field.test.tsx#passes autoComplete through, so the browser knows the field's purpose`

- **The sixteen strings the wider measurement found are in the dictionary
  (E4.7):** the audit's new expression rule surfaced text that never passed
  through the tables, and this slice moves all of it out of hand-written
  conditionals: the password toggle's `Hide`/`Show` (two screens, read out
  loud in English inside the Turkish interface), `History`'s `Date` header,
  `PdfStudio`'s "no private key", `Ai`'s citation fallback, the
  `language === "tr" ? … : …` pairs in `Merge`, `Ocr` and `Compress`, and the
  petition subtitle - whose English interface had been showing the Turkish
  sentence. The Turkish wording is unchanged, which a contract test pins key by
  key. Working on it also turned up a real defect: `writer.pageBreak` was
  defined twice in both tables and the second definition won, so the ribbon
  button rendered "— page break —"; the marker now has its own key and the
  button reads "Page break". Baseline: 73/20/8 -> **64/15/6** objects 0, and the
  CI ceilings follow it down.
  `test:src/lib/i18n-conditional-strings.test.ts#keep their exact wording in both languages`
  `test:src/lib/i18n-conditional-strings.test.ts#gives the page-break marker its own key`
  `test:src/screens/tools-screens.test.tsx#labels the password toggle in the interface language`
  `test:src/lib/ci-workflow.test.ts#records a hardcoded-JSX baseline the audit compares against`

- **The audit reads expressions, and the wider rule found real text (E4.7):**
  until now a string only counted when it was written directly in a JSX text
  node or as a plain attribute value, so `label={flag ? "Kaydet" : "İptal"}`,
  `<p>{ok ? "Tamam" : "Hata"}</p>`, `name ?? "Adsız"` and the words inside
  `` `${n} ${n === 1 ? "Sayfa" : "Sayfalar"}` `` were invisible - and so was the
  same gap in the object scan. One recursive rule now reads them all, with two
  deliberate exclusions: the condition of a conditional (`mode === "edit"` is a
  comparison, not prose) and `+` concatenation. `--expressions-report` lists
  every string the rule added with its origin (`branch` or `span`), so the
  measurement is reviewable rather than a number. Measured: files 50 -> 73,
  attributes 11 -> 20, props 6 -> 8, objects 0 -> 0 (that side found nothing).
  The findings include hand-rolled `language === "tr" ? … : …` pairs in `Merge`,
  `Ocr` and `Compress` and an untranslated `Hide`/`Show`/`Date` cluster; the
  baseline records them so the translation slice has a fixed list.
  `test:src/lib/i18n-jsx-audit.test.ts#counts both branches of a conditional attribute and prop`
  `test:src/lib/i18n-jsx-audit.test.ts#counts the literal fallback of ??, || and &&`
  `test:src/lib/i18n-jsx-audit.test.ts#counts a string, template or conditional written as a JSX child`
  `test:src/lib/i18n-jsx-audit.test.ts#never reads a condition, a call, a class name or a separator as prose`
  `test:src/lib/i18n-jsx-audit.test.ts#reads the literals inside a template's interpolations`
  `test:src/lib/ci-workflow.test.ts#records a hardcoded-JSX baseline the audit compares against`

- **The tag job no longer checks code out (E5):** OpenSSF Scorecard scored
  Dangerous-Workflow at 0 because `auto-tag.yml` checked out
  `github.event.workflow_run.head_sha` - the shape of a fork-code-with-secrets
  attack, even though the job's gate only ever admits a push-to-master run that
  passed. The job reads four files (`package.json`, `tauri.conf.json`,
  `Cargo.toml`, `tauri.properties`), so it now fetches exactly those from the
  commit it is about to tag through the Contents API and has no working tree at
  all; the version gate and the tag therefore describe one commit, and the
  write-scoped token no longer sits in a `.git/config`. The gate also states the
  repository comparison in the file, so a fork's run cannot reach the tag step
  even if the event filter changed.
  `test:src/lib/ci-workflow.test.ts#never checks out the triggering run's commit`
  `test:src/lib/ci-workflow.test.ts#reads each version file from the commit it is about to tag`
  `test:src/lib/ci-workflow.test.ts#keeps the gate on a push from this repository`

- **The token is read-only unless a job writes (E5):** the OpenSSF Scorecard
  run scored Token-Permissions at 0 because five workflows granted the write
  scopes at the workflow level. `auto-tag`, `beta`, `linux-package`, `pr-size`
  and `release` now declare `contents: read` at the top and hand the write
  scopes to the one job that uses them; a job `permissions:` block replaces the
  workflow one rather than adding to it, so the effective scopes are unchanged
  everywhere except `release/version-consistency`, which only checks out and
  compares versions and therefore loses its inherited `contents: write`.
  `actions: write` stays with the auto-tag job because the dispatch endpoint is
  an Actions permission; `release/linux` keeps `contents: write` since it is
  the ceiling the reusable `linux-package.yml` runs under.
  `test:src/lib/ci-workflow.test.ts#never grants a write permission at the workflow level`
  `test:src/lib/ci-workflow.test.ts#grants each job the permission its steps actually call`
  `test:src/lib/ci-workflow.test.ts#keeps the called workflow inside its caller's ceiling`
  `test:src/lib/ci-workflow.test.ts#keeps the write permissions the publishing jobs need`

- **The repository's security posture is graded, not claimed (E5):** an
  OpenSSF Scorecard workflow runs weekly, on every push to master, when a
  branch-protection rule changes and on dispatch, uploads the SARIF report to
  code scanning and publishes the score. A `pull_request` trigger filtered to
  this one file exists because the action rejects every other event off the
  default branch: it is the only way to exercise the action, the permissions
  and the SARIF upload before a merge, and publishing is off by itself on a
  pull-request event. The path filter keeps an ordinary PR from growing a
  check. Top-level permissions stay read-only and the job asks for
  `security-events: write`, `id-token: write`, `contents: read` and
  `actions: read`; there is no `run:` step, no top-level `env`/`defaults`, the
  checkout keeps `persist-credentials: false`, and all four actions are pinned
  to commits.
  `test:src/lib/ci-workflow.test.ts#grades the repository on a schedule, on master and on a protection change`
  `test:src/lib/ci-workflow.test.ts#only runs on a pull request when this workflow itself changes`
  `test:src/lib/ci-workflow.test.ts#keeps the analysis least-privilege and pinned`
  `test:src/lib/ci-workflow.test.ts#publishes the score and uploads the report`
  `test:src/lib/ci-workflow.test.ts#uses one codeql-action commit across code scanning and scorecard`

- **The release train writes no engine cache and reads the tested one (E5):**
  `release.yml` and `beta.yml` downloaded the same ~100 MB of pinned
  third-party artifacts on every run. Both now restore
  `src-tauri/resources/engines` with `actions/cache/restore` only - the entry is
  written by the Desktop run on the default branch and never by a publishing
  job, so a release cannot seed a cache another run then trusts. The key carries
  the hash of `scripts/engines.lock.json` **and** `scripts/fetch-engines.ps1`,
  because the fetch script skips files that are already present and so the
  cache is part of the trust chain: either file changing invalidates it. A tag
  run restores the entry the same commit stored on the default branch; a miss
  falls back to the full verified download. `nightly.yml` stays uncached on
  purpose - several pins point at mutable `main` URLs and the daily run is the
  canary for that drift.
  `test:src/lib/ci-workflow.test.ts#restores the engine cache before fetching them, everywhere but the nightly`
  `test:src/lib/ci-workflow.test.ts#publishing workflows only read the engine cache`
  `test:src/lib/ci-workflow.test.ts#pins one actions/cache commit for the engine cache`
  `test:src/lib/ci-workflow.test.ts#nightly stays the uncached upstream-drift canary`

## Implemented (this cycle, 7.5.0) - Kanıt akışları ve E4 kalanları (E4/E5)

- **The store listing cannot fall behind a release (E5):** `scripts/fastlane-changelogs.mjs`
  generates `fastlane/metadata/android/en-US/changelogs/<versionCode>.txt` from
  the CHANGELOG section and requires every translated locale to carry the newest
  version by hand. It used to take the _last_ CHANGELOG entry as the current
  version, which is the oldest one, so the translated locales were checked
  against 6.6.0 forever and a missing file could not fail; the check now selects
  the highest version and fails by name. `FASTLANE_ROOT` lets the test drive a
  fixture tree, and write mode reports a missing translated note without failing
  so the English bodies can be produced first.
  `test:src/lib/fastlane-changelogs.test.ts#demands the newest version, not the oldest, from a translated locale`
  `test:src/lib/fastlane-changelogs.test.ts#never overwrites a hand-written translated changelog`

- **The vault has an end-to-end flow (E4.3):** `npm run e2e:vault` hands a text
  file to the app through the dev launch context; the screen turns that file's
  folder into a scan, the real backend indexes both files, a keyword search finds
  the seeded one, the match is highlighted and the preview shows the document.
  No engine and no provider is involved — `vault_search_hybrid` falls back to
  keyword search without an embedding client — so the flow runs in the Linux
  tauri-driver job with the other engine-free flows. Seeding is behind
  `PDFSAK_DEV_RUN` and off on Android: a normal launch never indexes a folder on
  its own, which a unit test pins.
  `test:src/screens/Vault.test.tsx#indexes the handed-over file's folder in a development auto-run`
  `test:src/screens/Vault.test.tsx#never indexes anything on a normal launch`
  `test:src/screens/vault-dev.test.ts#deduplicates one folder, case-insensitively`

- **The Android ABI matrix is checked on the artifacts (E5):** the APK and AAB
  are read as files before anything is published - each one carries only its own
  ABI's `lib/` directory with the three native libraries, the libraries are the
  right ELF shape (64-bit AArch64 / 32-bit ARM), both APKs share one signing
  certificate, and `SHA256SUMS-android.txt` matches the files next to it. That
  list is now part of the uploaded `android-apks` artifact too: the release
  attached it while the run artifact omitted it, so nobody downloading a run
  could verify a build. The check runs in the same job, before the upload, so a
  wrong or mixed-ABI package fails the build instead of reaching a device.
  `test:src/lib/ci-workflow.test.ts#checks the ABI matrix on the artifacts and publishes the checksum list`

- **The task-pack pipeline runs end to end against a local mock (E4.3):** the
  AI packs screen now takes documents through `useTool` like every other tool
  (open-with, drag and drop, dev launch), and `--ai-pack` starts
  `scripts/mock-deepseek.mjs` on loopback, points the app's provider at it
  through Settings, launches the screen with a sample document, ticks the
  consent box and presses Run like a user, and requires the verification card to
  report `parsed` **and** `ok`. Task packs now accept office documents too, not
  only PDFs - the office parsers need no native engine, which is what lets the
  flow run in the engine-free job. The mock answers a pack prompt by echoing the JSON
  shape the prompt asks for, so the strict contract is what gets tested. No key
  and no network call are involved, and the flow starts and stops the server
  itself.
  `test:src/lib/e2e-structure.test.ts#exports every flow the CI jobs and the personas run`
  `test:src/screens/AiPacks.test.tsx#runs a pack through ai_chat and verifies the structured answer`

- **An Impress flow closes the office trio (E4.3):** `--impress` opens
  `samples/test-presentation.pptx` in the office editor, reads the slide rail
  (`.slide-thumb` + `.is-active`, so no translated label is matched), clicks the
  second slide and requires it to become active, adds a slide through the
  `.slide-add` button and requires the rail to grow by one, then saves with a
  real Ctrl+S through the shared `useEditorShortcuts` handler and requires the
  PPTX on disk to change. With Writer and Calc this covers all three office
  editors in the Linux tauri-driver job on every PR.
  `test:src/lib/e2e-structure.test.ts#exports every flow the CI jobs and the personas run`

- **The secret store is exercised end to end (E4.3):** `--secrets` types a
  marker key into the Settings field, clicks Save, and reads the tier the UI
  reports from `data-key-storage`. It then inspects what reached the disk: above
  the fallback the file must **not** contain the key, and on the documented
  `plain` fallback (a Linux session with no Secret Service, which is what CI
  has) the file must be **0600** and carry the marker. A second app launch must
  read the key back as configured, and the flow clears it again. This proves the
  three-tier store across a real process restart, which a unit test with a fake
  vault cannot.
  `test:src/lib/e2e-structure.test.ts#exports every flow the CI jobs and the personas run`

- **A Calc flow proves the formula engine in the real UI (E4.3):** `--calc`
  opens `samples/test-spreadsheet.xlsx` in the office editor and asserts, through
  `data-cell="row:col"` hooks rather than labels or coordinates, that the header
  row came from the workbook's shared strings ("Name", "Quantity", "Total"), that
  `G2 = SUM(D2:D101)` holds a positive total (the file caches 0, so a positive
  number is the engine's own answer), and that selecting that cell puts
  `=SUM(...)` in `.formula-input`. Engine-free (officecore), so it runs in the
  Linux tauri-driver job on every PR.
  `test:src/lib/e2e-structure.test.ts#exports every flow the CI jobs and the personas run`

- **A new end-to-end flow proves signature verification (E4.3):** `--verify`
  launches the app on the Verify screen with the unsigned sample, waits for the
  screen to report it holds its file, and reads the verdict from a
  `data-testid`/`data-verdict` hook rather than a translated sentence - the real
  `pdf_verify_signatures` command must answer "unsigned" and the file must stay
  listed. It is engine-free (lopdf), so it runs in the Linux tauri-driver job on
  every PR next to the smoke and ACL gates.
  `test:src/lib/e2e-structure.test.ts#exports every flow the CI jobs and the personas run`

- **The end-to-end suite is modules, not one file (E4.3):** `e2e/smoke.mjs` went
  from 735 lines to a 109-line entry point that parses the flags and dispatches.
  The WebDriver plumbing (bounded calls, one session per scenario, screenshots,
  relaunch-on-failure) lives in `e2e/lib/driver.mjs`, the sample working copies in
  `e2e/lib/files.mjs`, and each flow in `e2e/flows/` (`home`, `reader`, `office`,
  `pdf`, `acl`, `personas`). The binary check moved behind
  `requireAppBinary()` so a module can be imported without a built app, and a
  test pins the split: the entry point stays thin, every flow keeps its export,
  every module in `flows/` is imported, and the guard stays out of module load.
  `test:src/lib/e2e-structure.test.ts#keeps the entry point to argument parsing and dispatch`
  `test:src/lib/e2e-structure.test.ts#exports every flow the CI jobs and the personas run`

- **The Rust coverage floors are a ratchet (E4.1):** the numbers from the
  `rust-coverage` job are now enforced. `scripts/check-rust-coverage.mjs` reads
  the `TOTAL` row of `cargo llvm-cov report --summary-only` and compares it with
  `scripts/coverage-baseline.json`; a measurement under a floor fails the job,
  and `--update` refuses to lower one (the floors may only rise). The first
  baseline is 54/55/55 against the 2026-11-02 measurement of 55.23/56.37/56.30,
  so run-to-run noise cannot redden a green PR while a lost test still fails.
  `test:src/lib/coverage-ratchet.test.ts#fails a measurement below a floor and names the metric`
  `test:src/lib/coverage-ratchet.test.ts#refuses to lower a floor through --update`
  `test:src/lib/ci-workflow.test.ts#measures Rust coverage and publishes the report instead of guessing a floor`

- **Rust coverage is measured and published (E4.1):** a `rust-coverage` job runs
  `cargo llvm-cov --lib` over the library crates (the set that runs on every
  platform), prints the summary and uploads `coverage-rust` (`lcov.info` plus
  `coverage-summary.txt`). It is measurement, not a gate: no floor is invented
  before the number exists, and the job deliberately skips the Tauri app crate
  (GTK/WebKit) and the Windows-only engine integration tests, which would report
  coverage for the wrong target.
  `test:src/lib/ci-workflow.test.ts#measures Rust coverage and publishes the report instead of guessing a floor`

- **Android builds both artifacts in one Gradle run per ABI (E5):** `--apk` and
  `--aab` are independent flags, so the four sequential `tauri android build`
  calls became two, each producing the APK and the AAB for its target and
  keeping the same artifact names and the single `SHA256SUMS-android.txt`.
  `test:src/lib/ci-workflow.test.ts#builds both Android artifacts in one Gradle run per ABI`

- **The i18n audit measures component props too (E4.5):** the scanner moved to
  `scripts/lib/i18n-jsx.mjs` and counts a third section - component `label`
  values - next to text nodes and attributes, in baseline schema **v3**
  (77 text nodes, 9 attribute strings, 25 prop strings at first record). A
  value counts when it is a literal (`label="Save"`, `label={"Save"}`,
  ``label={`Save`}``); a `t(…)` call, a variable or a dynamic template does not.
  That also closes the braced/template blind spot the attribute scan had. The
  first slice moved `PetitionWizard`'s ten field labels into the tables and
  lowered the prop baseline to **15**, and `--props-report` lists the remaining
  queue.
  `test:src/lib/i18n-jsx-audit.test.ts#counts the three literal forms of a prop`
  `test:src/lib/i18n-jsx-audit.test.ts#ignores translated calls, variables and dynamic templates`
  `test:src/lib/i18n-jsx-audit.test.ts#refuses to raise a props count without --allow-raise, and raises it with the flag`
  `test:src/lib/ci-workflow.test.ts#records a hardcoded-JSX baseline the audit compares against`

- **The prop scan covers `hint` and `subtitle` too (E4.5):** `PROPS` in
  `scripts/lib/i18n-jsx.mjs` now lists the names a component really declares -
  `label` and `hint` (`Field`, `SectionTitle`) and `subtitle` (`Screen`) - so
  a hint under a field and the line under a screen title cannot slip past the
  audit any more. `description`, `caption` and `emptyText` were checked and left
  out on purpose: they are data fields here, not JSX props. `title` stays an
  attribute only, because counting it as a prop as well would report the same
  text twice. The five literals the widened scan found (four petition hints and
  the KVKK screen subtitle) moved into the tables in the same slice, with Turkish
  keeping the original wording, so **the baseline did not move: version 3, 77
  text nodes, 9 attribute strings, 6 prop strings** - widening the list changes
  what is counted, not the shape of the baseline, and no migration was needed.
  `test:src/lib/i18n-jsx-audit.test.ts#counts the sibling props the components really declare`
  `test:src/lib/i18n-jsx-audit.test.ts#does not count a title twice when a component takes it as a prop`
  `test:src/lib/i18n-jsx-audit.test.ts#fails when a sibling prop grows above its baseline`
  `test:src/lib/i18n-jsx-audit.test.ts#keeps the baseline schema at v5 when the widened props are recorded`

- **Object literals and `.ts` modules are measured too (E4.6):** interface text
  that never passes through a JSX literal - the KVKK template options
  (`{ value: "employee", label: "Çalışan" }`), the wizard toast titles, and the
  findings `src/lib/document-health.ts` builds - is now counted in a fourth
  baseline section, `objects`, under schema **v4**. The scan is narrow on
  purpose: seven text-carrying keys, and only a value with a Turkish-specific
  letter, so product names and identifiers stay out; `// i18n-content: <reason>`
  excludes a line or a whole declaration, and a file that writes the document
  itself (`official-letter`, its template, the AI scoring fixtures) is listed in
  `CONTENT_PATHS` with its reason in code. First record: **34 strings across 7
  files**, and `--objects-report` prints what is counted, what is excluded and
  what the narrow rule let through. This slice measures only - making those
  strings translatable is the next one.
  `test:src/lib/i18n-jsx-audit.test.ts#counts the interface text an object literal holds, in a .ts module too`
  `test:src/lib/i18n-jsx-audit.test.ts#leaves product names and identifiers to the info list`
  `test:src/lib/i18n-jsx-audit.test.ts#honours an i18n-content pragma, and refuses one without a reason`
  `test:src/lib/i18n-jsx-audit.test.ts#fails when an object string grows above its baseline`
  `test:src/lib/i18n-jsx-audit.test.ts#asks for a re-record instead of passing when the baseline has no objects section`

- **The document-health findings became translatable (E4.7):** a `.ts` module
  cannot translate anything - it has no language - so `analyzeDocumentHealth`
  stopped building Turkish sentences. A `HealthIssue` now carries
  `titleKey`/`descriptionKey` and `params` (`{count}`, `{fields}`), and the screen
  draws it with `t(key, params)`, which also means the text follows a language
  change instead of freezing at first render. Nine findings, eighteen dictionary
  keys, and the Turkish wording is byte-for-byte what it was - a golden test
  holds all nine pairs against the old sentences. The KVKK template options moved
  the same way. `hardcoded-objects` **34 -> 13**; what is left is the toast
  titles still written inside `push({ title: … })` calls.
  On the way this fixed a real rendering bug: `KvkkWizard` printed the literal
  text `t("kvkk.lawBody")` and `t("kvkk.legalWarningBody")` where the law text
  should be, so the frame showed the call instead of the paragraph
  (`hardcoded-jsx` 77 -> 75).
  `test:src/lib/document-health-text.test.ts#keeps the Turkish wording exactly as the screen said it before`
  `test:src/lib/document-health-text.test.ts#gives every finding an English title and description, with no placeholder left`
  `test:src/lib/document-health-text.test.ts#keeps both tables in step for every finding key`

- **The toast titles became translatable too (E4.8):** the last of the object
  strings - the copy a screen pushes when something finishes - moved out of the
  `push({ title: … })` calls: the petition validation and save notices, the KVKK
  copy and save notices (sharing one `{format}` key), the invoice export and
  duplicate-cleanup notices, the redaction certificate pair and the vault index
  report. `hardcoded-objects` **13 -> 0**, and `ci-workflow.test.ts` now pins the
  section at zero instead of a ceiling, so the next literal fails the build
  rather than fitting under a number.
  `test:src/lib/ci-workflow.test.ts#records a hardcoded-JSX baseline the audit compares against`

- **The scanner has an end-to-end flow (E4.3):** `npm run e2e:scanner` hands two
  generated photos to the Scanner through the dev launch context and proves the
  on-device path: the detector finds the page on one photo and nothing on a blank
  wall (`data-auto` = `true,false`), the grey filter sticks (`aria-pressed`), and
  the build writes a real two-page PDF next to the first photo - read from disk,
  not from the toast. It is engine-free (pdfcore's page detector and lopdf, no
  pdfium), so it runs in the Linux job with the other engine-free flows and adds
  about a minute to every PR. The screen gained the hooks that made it drivable:
  an `initialFiles` hand-over (open-with and drag-and-drop use it too), a ready
  signal carrying the page count, `data-testid` buttons and rows, and a
  development auto-run that writes next to the source instead of opening a save
  dialog the driver could never answer. The entry point stayed a flag table: the
  four single-context flows now own their launch context (`impressScenario`,
  `calcScenario`, `verifyScenario`, `scannerScenario`), so `e2e/smoke.mjs` is
  shorter than before despite the new flow.
  `test:src/screens/Scanner.test.tsx#takes the handed-over images once and ignores a document`
  `test:src/screens/Scanner.test.tsx#reports the page count on the ready signal the driver waits for`
  `test:src/screens/Scanner.test.tsx#builds next to the first photo in a development auto-run, with no save dialog`
  `test:src/screens/scanner-dev.test.ts#keeps the Windows separator`

- **The audit also measures JSX attributes, and Redact speaks both languages
  (E4.5):** the scanner now counts `alt`, `aria-label`, `title` and `placeholder`
  strings that hold prose, with its own section in `scripts/i18n-baseline.json`
  (version 2), so an attribute cannot smuggle English into the Turkish UI any
  more than a text node could. The `Redact` certificate card and the KVKK and
  petition wizard titles and placeholders moved into the tables, taking the
  baseline to **77 text nodes and 9 attribute strings**. The nine that remain are
  literal by design - model ids (`nomic-embed-text`, `deepseek-v4-flash`), prompt
  examples (`tr, en, de`, `tax_id, iban, payment_due`), format lists
  (`pdf, docx, md`), units (`ms`) and the `Bates` stamp name.
  `test:src/lib/ci-workflow.test.ts#records a hardcoded-JSX baseline the audit compares against`

## Implemented (v7.4.0 cycle) - E1 kalanları ve kalite altyapısı (E4/E5)

- **Coverage floors raised to the measured suite (E4.1):** the vitest thresholds
  move from 62/69/46/62 to 77/79/64/77 after re-measuring the full suite
  (statements 78.4, branches 80.2, functions 65.9, lines 78.4 across 219 files /
  2472 tests); each floor sits one point under its measurement so run-to-run
  jitter cannot redden a green suite. The frontend job now uploads `coverage/` as
  the `coverage-frontend` artifact, so a failing gate can be read instead of
  guessed at. Rust coverage (`cargo llvm-cov`) is still unmeasured and is the
  remaining half of E4.1.
  `test:src/lib/ci-workflow.test.ts#publishes the coverage report from the frontend job`

- **A coverage floor may only rise:** a workflow test reads `vite.config.ts` and
  fails when any threshold drops below the 2026-11-01 measurement, so a red build
  cannot be made green by relaxing the gate.
  `test:src/lib/ci-workflow.test.ts#never lowers a vitest coverage floor`

- **Linux runners pinned (E5):** every workflow asks for `ubuntu-24.04` instead
  of `ubuntu-latest` (five jobs in `desktop.yml`, plus `auto-tag.yml` and
  `fuzz-pr.yml`), so an image rollover cannot change a build between runs; a test
  fails if a workflow brings the floating label back.
  `test:src/lib/ci-workflow.test.ts#no workflow falls back to ubuntu-latest`

- **Toolchain and runtime pinned (E5):** `rust-toolchain.toml` declares the
  `stable` channel with clippy and rustfmt, so a contributor and CI install the
  same tools. A toolchain file takes precedence over the installed default, so
  the MSRV job sets `RUSTUP_TOOLCHAIN: "1.89.0"` explicitly - otherwise it would
  silently test stable and the minimum would never be proven - and the fuzz jobs
  set `RUSTUP_TOOLCHAIN: nightly`. Tests pin all three facts.
  `test:src/lib/ci-workflow.test.ts#pins the toolchain without letting the file mask the MSRV job`
  `test:src/lib/ci-workflow.test.ts#keeps the fuzz jobs on nightly despite the toolchain file`

- **Node version declared (E5):** `.nvmrc` says 22 (what CI runs) and
  `package.json` declares `engines.node >= 20` (what the toolchain actually
  needs), so the README's "20+" and the CI version stop disagreeing.
  `test:src/lib/ci-workflow.test.ts#declares the node version the suite is built and tested with`

- **Engine downloads cached (E5):** the workspace-test and Windows E2E jobs cache
  `src-tauri/resources/engines` under the `scripts/engines.lock.json` hash, so a
  lock change invalidates the cache and a lock-identical build stops re-fetching
  the pinned engines. The release, beta and nightly workflows still download
  fresh and are the next step.
  `test:src/lib/ci-workflow.test.ts#caches the engine downloads by lock hash`

- **Plural forms come from `Intl.PluralRules` (E4.5):** `t.plural("key", n)`
  picks `key.one` / `key.other` through the runtime's plural rules instead of the
  `{count} file(s)` suffix that English readers saw in Turkish text. The 22
  strings that spelled a plural as "(s)" now carry both variants (Turkish uses
  the same sentence for both, because the noun is not pluralised after a
  numeral), 15 literal call sites moved over, and the audit fails on a new
  "(s)" value or on a `t.plural` key missing a variant. The proposal's "68 fake
  plurals" counted both languages and every call site; the dictionary had 22
  keys.
  `test:src/lib/i18n.test.ts#picks the singular and plural English forms`
  `test:src/lib/i18n.test.ts#keeps the Turkish sentence the same for every count`

- **The three untested components now have tests (E4.2):** `LanSyncPlan`
  (6 cases: one row per document, the three explicit conflict choices, and the
  delete/restore pair for each side a file was removed from - nothing is ever
  chosen by default), `ScannerPageList` (8 cases: page ids rather than photo
  paths as row identity, the two halves of a book spread, auto vs hand-cropped
  badges, filter/move/adjust/remove/clear callbacks, and the locked state while
  the detector runs) and `CornerPicker` (6 cases: the preview comes from the
  backend, the detector's "nothing found" path, arrow keys moving a handle by
  1 % / 5 % with Shift, saving pixel corners, and `undefined` when the whole
  photo is kept). `PdfStudio`, `Reader`, `Ai` and `Vault` already had coverage
  through `pdf-screens.test.tsx`, `Ai.test.tsx` and `Vault.test.tsx`, so the
  slice added the genuinely missing files instead of duplicating them.
  `test:src/screens/lan-sync-plan.test.tsx#offers the three explicit choices on a conflict and nothing is chosen by default`
  `test:src/screens/scanner-page-list.test.tsx#renders one row per page, with both halves of a spread`
  `test:src/screens/scanner-corners.test.tsx#keeps the whole photo when the frame is the full page`

- **The i18n audit grew teeth (E4.5):** the audit now fails on a `{placeholder}`
  set that differs between English and Turkish (a Turkish value missing
  `{count}` rendered the literal `{count}`) and on more user-visible JSX text
  than `scripts/i18n-baseline.json` records. The baseline is a ratchet -
  `--update` may only lower it unless `--allow-raise` is passed on purpose - and
  it is measured with the TypeScript parser, so generics and comparisons are not
  mistaken for prose. Identical-in-both values and keys no literal call reaches
  are printed as advisory, because both have legitimate cases.
  `test:src/lib/ci-workflow.test.ts#runs the audit with --check in the lint job`
  `test:src/lib/ci-workflow.test.ts#records a hardcoded-JSX baseline the audit compares against`

- **Five screens speak both languages (E4.5):** `DocumentHealthScreen` (14 JSX
  text nodes), `PetitionWizard` (4), `Invoices` (23 of 42), `KvkkWizard` (9) and
  the thumbnail-size options in `Organize` (3) moved into the tables, with new
  `health.*`, `invoices.kdv.*`, `kvkk.*` and `organize.thumb*` namespaces in both
  languages; the five attribute strings the proposal named - the dialog's
  `aria-label`, the thumbnail-size label and three `alt="Page N"` texts - come
  from the table too. The baseline fell 131 -> 80. What is left is deliberately
  literal: `TRY`, the accounting-software names (Luca/Logo/Zirve), paper sizes,
  the `1, 2, 3` numbering samples and units, plus a handful of real prose in
  `Redact` that is the next step. `docs/i18n.md` documents the workflow.
  `test:src/screens/DocumentHealthScreen.test.tsx#renders health screen with high score on clean text`
  `test:src/screens/DocumentHealthScreen.tokens.test.tsx#picks through the Rust dialog and inspects a PDF token as a PDF`

- **Static analysis and a reporting channel (E5):** `SECURITY.md` documents the
  private advisory channel, the supported versions and the deliberate limits
  (unsigned Windows installers, ACL scope, third-party engines). The `CodeQL`
  workflow analyses `javascript-typescript` and the workflow files themselves on
  push, pull request and a weekly schedule, with both CodeQL uses pinned to a
  commit.
  `test:src/lib/ci-workflow.test.ts#analyzes the frontend and the workflows themselves`
  (no test: `SECURITY.md` is a policy document; the workflow half is pinned by
  the CodeQL test above.)

## Implemented (this cycle, 7.3.0) - İzolasyon ve dayanıklılık (E2, ilk dilim)

- **Worker containment (E2.2, #550):** the untrusted-document parser child is
  contained before it executes. On Windows it is created with
  `CREATE_SUSPENDED`, assigned to a job object with a process-memory limit and
  only then resumed (the previous code assigned the job after `spawn`), and the
  suspended thread is resumed by thread id. On Linux/macOS `RLIMIT_AS` (1.5 GiB)
  and `RLIMIT_CPU` (120 s) are installed between `fork` and `exec`. The child
  inherits an explicit environment allow-list (engine paths, temp dir, locale and
  the platform basics) instead of the parent's environment, so a parser never
  sees API keys, provider tokens or the dev launch context. `--worker` counts
  only as the first argument, so a document path cannot select the worker.
  `test:src-tauri/src/worker_tests.rs#the_child_environment_is_an_allow_list`
  `test:src-tauri/src/worker_tests.rs#a_document_path_cannot_turn_the_gui_into_a_worker`
  The Unix ceilings are also proven by a probe test
  (`the_child_inherits_the_resource_ceilings`) that reads `ulimit -v`/`-t` back
  from a real child; it only exists where `setrlimit` does, so it is prose rather
  than a marker. On Windows the job object path is exercised by the desktop E2E,
  which opens documents through the real worker.

- **Async engine probe and typed command errors (E2.3, E2.4, #550):** the
  engine probe (`engine_status`) spawns `tesseract --version`, so it now runs on
  the blocking pool with `EngineStatus::missing()` as the answer when the probe
  cannot run; `ocr_languages` (the command the proposal named) only lists a
  directory and stays sync, which `docs/command-async-audit.md` records along
  with the classification of the 50 sync commands that touch the filesystem and
  the rule for new ones. The commands that still answered with a bare string —
  external links, the file/folder token picks, job status/retry, the updater and
  the two Android bridges — now return `PdfError`, so the UI receives
  `{code, message}` and localizes the failure instead of printing English.
  `test:crates/pdfcore/tests/engine_status_test.rs#a_failed_probe_reports_every_engine_missing`
  `test:src-tauri/src/links.rs#a_rejected_link_is_a_typed_error_on_the_wire`
  `test:src-tauri/src/jobs.rs#an_unknown_job_status_is_a_typed_error`

### Not done / Known limitations (7.3.0)

- **The containment is resource and environment, not an OS sandbox.** The child
  is the same binary with the same user's rights: it can still read any file that
  user can read, and it is not in an AppContainer/seccomp profile. The limits
  stop a runaway parser from exhausting the machine and keep the parent's secrets
  out of its environment; a full sandbox is a separate, platform-specific step.
  (no test: a scope statement; the resource limits themselves are the tests
  above.)

## Implemented (this cycle, 7.2.1) - Android derleme düzeltmesi

- **The Android target runs the release's clippy gate in review (v7.2.1):** the
  `Android target compile` PR job now runs
  `cargo clippy -p pdf-swiss-army-knife --lib --target aarch64-linux-android --
-D warnings`, the same command the tagged Android build runs. v7.2.0 shipped a
  keyring constant that is only unused on Android, so the PR (`cargo check`)
  passed and the release build failed; the gate now catches that class in review.
  `test:src/lib/ci-workflow.test.ts#runs the Android clippy gate in review, not only at tag time (v7.2.1)`
- **The unused constant is Android-gated (v7.2.1):** `secret/vault.rs` keeps
  `SERVICE` only where the keyring backend exists, so the Android target has no
  dead code to deny.
  (no test: a platform cfg attribute; the Android clippy job above is the gate
  that proves it.)

## Implemented (this cycle, 7.2.0) - Güvenlik sertleştirmesi (E1, ilk dilim)

- **CSP and prototype freeze (E1, #550):** the webview policy now blocks
  plugins/objects (`object-src 'none'`), base-tag rewrites (`base-uri 'none'`),
  form posts (`form-action 'none'`) and framing (`frame-ancestors 'none'`) on
  top of the existing local-asset/IPC allow-lists, and `freezePrototype` is on.
  `test:src/lib/security-config.test.ts#blocks objects, base changes, form posts and framing`
  `test:src/lib/security-config.test.ts#freezes the prototypes the webview exposes`
- **No unsafe code in the library crates (E1, #550):** `aicore`, `compat`,
  `officecore`, `pdfcore`, `synccore` and `turkish` declare
  `#![forbid(unsafe_code)]`; the workspace compiles under it.
  `test:src/lib/security-config.test.ts#declares #![forbid(unsafe_code)] in every library crate`

- **Raw-path policy (E1/2, #550):** `paths::lexical` and `paths::directory`
  now run raw paths through the policy instead of accepting any absolute path:
  a release webview may only pass app-owned directories (cache/data/config/log/
  temp, registered at startup) or paths the backend produced (a resolved save
  target and its folder become trusted). Fingerprint/output-exists reads,
  `ensure_dir`, batch output validation and the LAN share folder therefore need
  a token or an owned/trusted path.
  `test:src-tauri/src/paths_policy.rs#debug_builds_accept_and_release_builds_need_trust`
  `test:src-tauri/src/paths_policy.rs#registration_normalizes_and_deduplicates`
  `test:src-tauri/src/paths_policy.rs#trusted_paths_cover_their_tree`

- **Code-authenticated LAN pairing (E1/3, #550):** the six-digit code never
  crosses the wire. Pairing is a SPAKE2 handshake with the code as the password
  (`crates/synccore/src/lan/pairing.rs`), so only a device that knows the code
  derives the key, and a wrong guess costs one online attempt instead of
  letting a recorded handshake be tested offline. Both certificate fingerprints
  are hashed into the derived key: a device that terminates TLS with its own
  certificate and relays the handshake derives two different keys, so the
  client pins nothing and the host records nothing. The confirmation round runs
  on a connection pinned to the first answer's certificate, a pending session
  is taken out of the host's map before it is checked, the pairing fields are
  validated before the handshake (a 16-64 character hex session, a fixed-size
  SPAKE2 message, short identity fields), accepted nonces are remembered for
  the freshness window (`crates/synccore/src/lan/replay.rs`) and four worker
  threads replace the one-connection-at-a-time accept loop, so a stalled peer
  neither queues the others nor holds "stop sharing".
  `test:crates/synccore/src/lan/pairing_tests.rs#a_relaying_mitm_cannot_turn_a_pairing_into_a_pin`
  `test:crates/synccore/src/lan/pairing_tests.rs#the_pairing_key_is_bound_to_both_certificates`
  `test:crates/synccore/src/lan/pairing_tests.rs#an_attacker_that_guesses_codes_spends_the_host_attempts`
  `test:crates/synccore/src/lan/pairing_tests.rs#a_signed_request_is_accepted_once`
  `test:crates/synccore/src/lan/pairing_tests.rs#a_stalled_peer_does_not_block_a_pairing`
  `test:crates/synccore/src/lan/pairing_tests.rs#stopping_the_host_does_not_wait_for_a_stalled_peer`
  `test:crates/synccore/src/lan/pairing_tests.rs#a_malformed_pairing_request_is_refused_without_side_effects`
  `test:crates/synccore/src/lan/pairing_tests.rs#a_client_on_the_old_pairing_protocol_is_refused`

- **OS keyring for desktop secrets, owner-only fallback (E1/4, #550):** the
  secret store has three tiers. DPAPI (Windows) and the Android Keystore stay
  first; on a platform with no cipher (Linux, macOS) the value moves into the
  OS credential vault (`src-tauri/src/secret/vault.rs`: Secret Service,
  Keychain, Credential Manager) and the file keeps only a `keyring:<slot>`
  marker with no secret in it. Only when the vault does not answer does the
  value land in a `plain:` file, and every such file - and every sealed store:
  the LAN device key, person cards, signature images, trash copies - is written
  with `0600` (POSIX modes; on Windows those files stay inside the user's
  profile and inherit its ACLs). Legacy `plain:` values migrate into the vault
  (or the cipher) on read, with a read-back check before the plain file is
  replaced, and an old file's permissions are tightened when it is read. The UI
  names the tier.
  `test:src-tauri/src/secret_tests.rs#save_uses_the_vault_when_there_is_no_cipher`
  `test:src-tauri/src/secret_tests.rs#save_falls_back_to_plain_when_the_vault_refuses`
  `test:src-tauri/src/secret_tests.rs#load_migrates_a_plain_value_into_the_vault`
  `test:src-tauri/src/secret_tests.rs#load_keeps_the_plain_value_when_the_vault_does_not_answer`
  `test:src-tauri/src/secret_tests.rs#a_marker_without_a_vault_is_reported_instead_of_read_as_empty`
  `test:src-tauri/src/secret_tests.rs#a_plain_file_is_restricted_when_it_is_read`
  `test:src-tauri/src/secret_tests.rs#a_plain_fallback_file_is_created_owner_only`
  `test:src-tauri/src/secret/vault.rs#a_slot_is_stable_per_path_and_keeps_the_path_out_of_the_name`
  `test:src-tauri/src/secret/vault.rs#a_value_sealed_into_the_vault_round_trips_through_its_marker`
  `test:src-tauri/src/secret/vault.rs#a_secret_file_is_restricted_to_its_owner`
  `test:src/lib/ai-storage-label.test.ts#names every storage tier the Rust layer reports`

- **Application command ACL (E1/4b, #550):** the webview may only invoke the
  commands a capability allows. `src-tauri/build.rs` now declares an app ACL
  manifest, `src-tauri/permissions/` carries one `allow-<domain>-commands`
  permission per IPC domain generated from the domain lists
  (`scripts/command-acl.mjs`, `npm run acl:sync`/`acl:check`, run in the Lint
  job), and `capabilities/default.json` grants the eight permissions to the
  main window. Because the manifest comes from the registry, an unregistered
  command cannot be called and an added one cannot be forgotten; the Rust suite
  compares the two artifacts the build script writes (`acl-manifests.json` and
  `capabilities.json`) with the registered surface, and the WebDriver scenario
  `e2e/smoke.mjs --acl` proves the runtime refusal on the running app (an
  unknown command is rejected, `app_info` still answers).
  `test:src-tauri/src/command_acl.rs#the_application_acl_is_active`
  `test:src-tauri/src/command_acl.rs#every_registered_command_is_granted_to_the_main_window`
  `test:src-tauri/src/command_acl.rs#the_capability_grants_exactly_the_generated_permissions`

- **Plugin installs need a human and are recorded (E1/5, #550):** `plugin_install`
  validates the manifest/source pair, then the Rust side opens a native dialog
  with the plan - name, id, version, requested permissions, capabilities, size
  and bundle SHA-256 - and writes nothing unless the user accepts; a declined
  install returns `forbidden` and leaves the disk untouched. The bundled sample
  and the user's folder pick are the only paths without the dialog, because the
  bytes are either compiled in or chosen by the human. `install.json` records the
  id, hash, time and confirmation, and `plugin_list` compares the files on disk
  with it, so a plugin changed after install reads as `modified` and the plugin
  card shows it.
  `test:src-tauri/src/plugin/install_tests.rs#a_plan_summarises_what_the_user_confirms`
  `test:src-tauri/src/plugin/install_tests.rs#a_declined_install_writes_nothing`
  `test:src-tauri/src/plugin/install_tests.rs#a_confirmed_install_records_the_hash_and_verifies_it`
  `test:src-tauri/src/plugin/install_tests.rs#a_plugin_changed_on_disk_is_reported_as_modified`
  `test:src-tauri/src/plugin/install_tests.rs#a_plugin_without_a_record_is_unrecorded`
  `test:src-tauri/src/plugin/install_tests.rs#the_hash_covers_the_manifest_and_the_source`

### Not done / Known limitations (7.2.0)

- **Configured output destinations are grants now (E1 follow-up, closed).**
  Settings' default output folder, batch targets and the AI library folder used
  to bypass the policy through `paths::directory_configured`. A folder the human
  picks in the Rust dialog is persisted as a grant
  (`<config>/path-grants.json`, at most 32 entries), the grants are trusted at
  startup, and the four call sites (split, PDF→images, the office variant, the
  output suggestion) now run the strict `paths::directory`; the legacy validator
  is gone. A path the user never picked is refused again.
  `test:src-tauri/src/paths_policy.rs#a_grant_survives_a_restart_and_only_covers_its_own_folder`
  `test:src-tauri/src/paths_policy.rs#a_grant_must_be_an_absolute_path`
- The release-mode denial branch is exercised through the policy decision table
  and code path; CI runs debug builds where raw paths are allowed, so a release
  E2E remains the manual check.
  (no test: requires a release build on a desktop device.)
- **Pairing needs both devices on 7.2 or newer.** The pre-7.2 handshake sent
  the code inside the TLS session; this host refuses that operation with an
  update message instead of honouring it, so a mixed-version pair cannot
  downgrade. A real two-device version mixture is the manual row S2 in
  `docs/lan-sync-acceptance.md`.
  `test:crates/synccore/src/lan/pairing_tests.rs#a_client_on_the_old_pairing_protocol_is_refused`
- **Pairing availability, not the host, can be blocked.** Eight in-flight
  pairing sessions are held for up to the 120 s window, so a device that opens
  first rounds without finishing them can make a legitimate pairing wait for
  the window to be reopened. It cannot pair, read or write anything, and the
  host keeps serving paired peers meanwhile.
  (no test: a timing property of the bounded session map; the cap and the
  expiry are covered by the attacker test above.)
- **The vault tier is best effort and desktop-only.** A Linux session with no
  Secret Service (a bare SSH box, a container) has no vault, so the value falls
  back to the `0600` file and the UI says "unprotected"; the vault itself is
  only as strong as the user's login session.
  `test:src-tauri/src/secret_tests.rs#save_falls_back_to_plain_when_the_vault_refuses`
- **Sealed stores stay `plain:` without a cipher.** The LAN store, person
  cards, signatures and trash copies can grow past what a keyring entry should
  hold, so they are written owner-only (`0600`) rather than vaulted. A file
  written before this change is tightened on its next write, not on read.
  `test:src-tauri/src/secret/vault.rs#a_secret_file_is_restricted_to_its_owner`
- **The ACL is domain granular.** One permission per IPC domain means a
  compromised webview can still call every command of a domain the app uses;
  the ACL bounds the surface, it does not split it per feature. It also
  separates the webview from the backend, not users from each other: a local
  process under the same account reaches the files directly, which is what the
  path policy and the secret store cover.
  (no test: a design property of the manifest; the enforcement itself is the
  Rust and WebDriver checks in the claim above.)
- **The plugin hash is a record, not a signature.** It proves the installed
  bytes are the ones the user approved; it does not prove who wrote them. A
  signed manifest (and a publisher key the app pins) is the later step, and until
  then "modified" can only report a change, not the author.
  (no test: a documented limit of the record; the change detection itself is the
  `modified` test above.)
- **The install dialog is a native window.** It is drawn by the platform and
  cannot be restyled or tested in the WebDriver suite; the plan text it shows is
  pinned by a unit test instead, and the dialog itself stays the manual check.
  (no test: platform UI; the plan summary is tested.)
- **Clippy hygiene ratchet, complete (E1.7).** Every crate now declares
  `#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]`, so
  production code in `turkish`, `pdfcore`, `aicore`, `officecore`, `synccore`
  and the `compat` CLI cannot unwrap or expect. The first slice fixed the three
  offenders in `turkish`/`pdfcore`/`aicore`; this one cleared the remaining 55
  (`officecore` 34: the XML parser stack, ODF range and reference splitting, the
  DOCX tracked-change writer, the layout page accessor, a UBL column letter;
  `synccore` 11: the DAV listing parser and the WebDAV verb helpers; `compat`
  10: CLI flag parsing and the JSON report). Each site became a typed error, an
  explicit fallback, or an `if let` over the value - none was silenced. Two
  calls are infallible by construction and stay as documented
  `#[expect(..., reason = "...")]`: the embedded PDF face parses a build
  artifact, and HMAC keys its block with a key of any length. The earlier note's
  `compat` 36 was a miscount: `cargo clippy -p compat --lib` re-lints
  `officecore` as a dependency, and `compat`'s own ten live in its binary.
  `test:src/lib/security-config.test.ts#denies unwrap/expect in every crate outside tests`

## Implemented (this cycle, 7.1.1) - E0 hızlı kazanımlar

- **Command-surface hygiene (E0, #550):** the unregistered `issue_tokens`
  lost its `#[tauri::command]` attribute (it was never invocable, but the
  attribute inflated the counted command surface); the README counts are
  refreshed by `docs:stats`.
  (no test: attribute-only cleanup; the IPC totals test pins the registered
  surface.)
- **CI guards pinned (E0, #550):** the desktop workflow cancels superseded PR
  runs but never a master run, every job has a wall-clock timeout, and the fuzz
  crash-artifact upload now runs after the last target so a `pdf_parse` crash
  keeps its input.
  `test:src/lib/ci-workflow.test.ts#cancels superseded PR runs but never a master run`
  `test:src/lib/ci-workflow.test.ts#gives every job a wall-clock timeout`
  `test:src/lib/ci-workflow.test.ts#uploads fuzz crash artifacts after the last fuzz target`
- **Release builds are lock-pinned (E0, #550):** release, beta, Linux-package,
  Android and the desktop Android target run a `cargo metadata --locked`
  preflight before the Tauri build; the reproducibility gate requires the
  preflight and a negative test proves it fails without one.
  `test:src/lib/ci-workflow.test.ts#runs a cargo metadata --locked preflight before every release tauri build`
  `test:src/lib/reproducibility.test.ts#fails when a tauri release build loses its lockfile preflight`
- **Single advisory policy (E0, #550):** the duplicated `cargo audit --ignore`
  list is gone; `cargo deny check` (advisories, licenses, bans, sources) is the
  only advisory gate, with the vendored-calamine/zip-9 and updater-zip-4 facts
  recorded in `deny.toml`.
  `test:src/lib/ci-workflow.test.ts#keeps a single advisory policy (no cargo audit --ignore duplicate)`
- **Impress undo/redo purity (E0, #550):** the handlers read the stack before
  updating and keep the updaters pure, so StrictMode no longer runs the state
  transition (and `edit`) twice; behaviour is covered by the existing editor
  suite.
  (no test: behaviour preserved; the Impress editor suite runs unchanged.)
- **Generated bindings out of the line ratchet (E0, #550):** `src/lib/generated/`
  is excluded from `files:check`; its size follows the command surface, not a
  hand-written budget.
  (no test: the ratchet gate itself runs in CI.)
- **Hygiene (E0, #550):** `check:fast` removed (it was byte-identical to
  `check`); stale panic=abort, bindings and aicore network comments corrected;
  `files.rs`'s file-wide dead-code allowance replaced with a targeted one;
  README platform sentence fixed.
  (no test: documentation and script hygiene; the behaviour claims above carry
  their own markers.)

## Implemented (this cycle, 7.1.0) - İmzalama hazırlığı ve D10 kararı

- **D10 closed as unsigned for 7.1.0 (#531):** no Authenticode provider
  account is available in this cycle, so the release ships unsigned by
  decision; the SignPath Foundation route is free and needs no business
  identity, but it requires the owner's application and its reputation review.
  The workflow, preflight, readiness gate and runbook stay ready, and the
  decision and revisit triggers are recorded in `docs/signing.md`.
  (no test: decision record; the workflow readiness below is tested.)
- **SignPath preflight (D10 preparation, #531):** the release workflow now
  fails fast when a provider is selected by one secret but misses another,
  naming the missing secrets instead of failing deep inside the signing action;
  the three-provider detection and the unsigned fallback for forks are pinned.
  `test:src/lib/signing-readiness.test.ts#fails when a required SignPath secret disappears from the workflow`
  `test:src/lib/signing-readiness.test.ts#pins the unsigned fallback, the preflight and the updater refresh`
- **First-signed-release runbook (D10 preparation, #531):** `docs/signing.md`
  gained the operator checklist — free OSS application, the five SignPath
  secrets, the artifact configuration for the app exe and the NSIS installer, a
  dispatch dry-run, the `Get-AuthenticodeSignature` / `signtool verify` checks,
  the updater-signature refresh and a troubleshooting table.
  `test:src/lib/signing-readiness.test.ts#passes the workflow and runbook invariants`
  `test:src/lib/signing-readiness.test.ts#fails when the runbook drops the verification step`
- **Readiness gate (D10 preparation, #531):** `npm run signing:check` validates
  the workflow and the runbook together (provider guards, required secrets,
  verification steps, documented commands) and runs in the test suite.

### Not done / Known limitations (7.1.0)

- **D10 acceptance was not met:** the release is unsigned by decision because
  no provider account exists. Reopening needs either a SignPath Foundation
  approval (free, reputation-reviewed) or a Certum OSS certificate; the first
  signed release must then pass `Get-AuthenticodeSignature` (Valid, with a
  timestamp) and `signtool verify /pa /v`. SmartScreen keeps showing
  "unknown publisher" until that happens.
  (no test: requires an external signing account and an approved policy.)

## Implemented (this cycle, 7.1.0) - Komut paleti görev komutları

- **Desk tasks in the palette (D13, #531):** ten profession tasks (redact the
  selected PDF, VAT summary, evidence bundle, petition, official letter,
  deadline, KVKK, document health, cleaner, data tools) are registered as
  palette commands with `Ctrl+Alt+…` shortcuts and take the current file
  selection with them; the VAT task also selects its AI pack on arrival.
  `test:src/lib/desk-tasks.test.ts#covers the profession desk screens`
  `test:src/screens/AiPacks.test.tsx#preselects the pack a desk task navigates with (D13)`
- **Disabled reason (D13, #531):** a query lists a task that cannot run right
  now with the reason ("Select a document first"), an `aria-disabled` row and a
  no-op click instead of hiding it silently.
  `test:src/lib/commands.test.ts#keeps a disabled command out of the default search and lists it on request`
  `test:src/components/command-palette.test.tsx#shows a disabled task with the reason and does not run it`
- **Plugin registry unchanged (D13, #531):** desk tasks are registered and
  removed by id through the same registry; plugin commands keep their own ids
  and the existing plugin suite runs unchanged.
  (no test: the plugin command registry suite runs unchanged and covers it.)

### Not done / Known limitations (7.1.0)

- The palette rows are driven by the current screen's file selection, not by a
  range/selection inside an editor; a Writer text selection does not yet scope
  a desk task.
  (no test: documented limitation.)

## Implemented (this cycle, 7.0.0) - Performans ve bellek

- **Large-document flows (D9, #531):** a 500-page Writer document opens and
  saves through the real DOCX reader/writer, and a 50 000-cell workbook with
  5 000 formulas opens and saves through the real XLSX paths; both are always-on
  tests with generous bounds and printed measurements (dev: 0.26 s / 0.29 s and
  1.09 s / 4.24 s).
  `test:crates/officecore/tests/perf_test.rs#perf_writer_docx_open_save_of_500_pages_under_bounds`
  `test:crates/officecore/tests/perf_test.rs#perf_calc_xlsx_open_save_of_50k_cells_under_bounds`
- **Memory ceilings (D9, #531):** a counting global allocator measures the peak
  live bytes of the same flows; the ceilings are 384 MiB (500-page DOCX, measured
  29.4 MiB) and 256 MiB (50k-cell XLSX, measured 135.3 MiB), so a regression that
  copies the document a second time fails the test.
  `test:crates/officecore/tests/memory_budgets.rs#large_document_flows_stay_under_their_memory_budgets`
- **Calc virtualization (D9, #531):** a 50 000-row sheet keeps at most 60 rows
  in the render window at both ends of the scroll, independent of sheet size.
  `test:src/office/calc/perf.test.ts#renders only the visible row window on a 50 000-row sheet`
- **PDF runtime isolation (D9, #531):** the heavy-work semaphore is pinned to
  2–8 slots, a panicking worker returns its blocking slot, and the synchronous
  extraction path is covered by the same bound tests.
  `test:src-tauri/src/concurrency.rs#the_bound_is_a_small_constant_not_the_tokio_default`
  `test:src-tauri/src/concurrency.rs#a_panicking_worker_returns_its_blocking_slot`
- **Named budgets (D9, #531):** `docs/performance-budgets.md` records the
  product budgets (cold start, open/save, render/export, memory ceilings), the
  measured baseline, the commands that reproduce it, the isolation rule for new
  heavy commands and how criterion trends are tracked in CI.
  (no test: a documentation and policy record; the numbers are enforced by the
  tests above.)

### Not done / Known limitations (7.0.0)

- Cold start (desktop ≤ 3 s, Android ≤ 5 s) is recorded manually in the release
  checklist; there is no automated threshold, because shared-runner launch
  timings are not representative and a flaky gate would be disabled.
  (no test: manual release measurement.)
- The criterion regression check is advisory (`continue-on-error`), so a >15 %
  slowdown reports and uploads an artifact but does not fail the workflow.
  (no test: documented policy in `docs/performance-budgets.md`.)
- Peak memory is measured with a counting allocator in the test binary, not by
  sampling a real process; native allocations outside Rust (WebView, PDFium)
  are outside the number.
  (no test: measurement boundary documented.)

## Implemented (this cycle, 7.0.0) - Ofis sadakati

- **DOCX tracked changes in the editor (D6, #531):** `w:ins`/`w:del` runs keep
  their author and date in the inline tooltip, the kind is a CSS class, and the
  model (with the revisions) survives a save and reload.
  `test:src/office/writer/runs.test.ts#marks insertions and deletions with their kind, author and date`
  `test:crates/officecore/src/docx_tests.rs#roundtrip_tracked_changes`
- **XLSX shared formulas (D6, #531):** the raw sheet XML is read to keep the
  base formula per `si` and expand every filled-down follower by shifting its
  relative A1 references; absolute/mixed references, ranges and quoted strings
  are handled.
  `test:crates/officecore/src/xlsx/shared_formulas.rs#shifts_relative_references_only`
  `test:crates/officecore/tests/office_fidelity.rs#shared_formulas_are_expanded_on_import`
- **PPTX transition duration (D6, #531):** the duration is written as `advTm`
  and read back, so a `fade` with 800 ms no longer returns as the 500 ms
  default.
  `test:crates/officecore/tests/office_fidelity.rs#pptx_transition_and_duration_round_trip`
- **Round-trip fidelity corpus (D6, #531):** a programmatic corpus writes,
  reloads and compares the content counters for DOCX, ODT, XLSX and PPTX; the
  measured delta is zero for every listed construct and the report lives in
  `docs/office-fidelity-report.md`.
  `test:crates/officecore/tests/office_fidelity.rs#docx_round_trip_preserves_every_supported_construct`
  `test:crates/officecore/tests/office_fidelity.rs#odt_round_trip_preserves_text_styles_and_tables`
  `test:crates/officecore/tests/office_fidelity.rs#xlsx_round_trip_preserves_values_and_formulas`

### Not done / Known limitations (7.0.0)

- The corpus does not yet count images, charts, comments, headers/footers,
  numbering or section breaks; those have their own tests but are outside the
  measured table.
  (no test: honest gap, listed in `docs/office-fidelity-report.md`.)
- Dynamic arrays (spill ranges) are imported as their formula text; the app
  does not calculate spills yet.
  (no test: not implemented.)
- The XLSX writer still emits plain formulas (no `t="shared"` output), and ODT
  tracked changes are a separate importer not covered by the ODT corpus row.
  (no test: documented limitation.)

## Implemented (this cycle, 7.0.0) - Yerel yapay zekâ v2

- **Turkish eval harness (D5, #531):** every pack carries golden answers plus
  answers it must reject (broken arithmetic, wrong dates, missing fields); the
  same verifier the screen uses scores them, a suite floor of 90 % per pack and
  overall turns "the model seems fine" into a number, and a degraded model is
  caught by the score instead of by a user.
  `test:src/lib/ai-task-pack-eval.test.ts#scores the golden answers at 100 % and reports every pack`
  `test:src/lib/ai-task-pack-eval.test.ts#catches a regression when the local model starts answering badly`
- **Three new Turkish packs (D5, #531):** e-invoice analysis (line arithmetic,
  per-line VAT, the three totals), KEP/e-Tebligat summary (sender, subject,
  sections, service date, period and last day), and court-petition
  classification (court type from a closed list, subject, requests, evidence) —
  seven packs in total, each with its own verifier.
  `test:src/lib/ai-task-packs.test.ts#ships the seven Turkish packs (C7 + D5) with unique ids`
- **Strict JSON contract with a repair turn (D5):** when an answer does not
  verify, the runner sends one repair question carrying the concrete problems
  and the rejected answer; the screen shows the repaired, verified answer and
  says a repair happened.
  `test:src/lib/ai-task-pack-eval.test.ts#sends one repair turn with the concrete problems and then accepts`
- **Zero-network isolation (D5):** the runner takes the provider call as a
  parameter, so a test proves the harness and the runner never call `fetch` or
  telemetry; the only network user is the provider the user configured, and the
  local presets point at localhost.
  `test:src/lib/ai-task-pack-eval.test.ts#never calls the network or telemetry itself`
- **Model bundling decision (D5):** `docs/local-ai-decision.md` records the
  candidate models, licences, sizes, RAM budgets, packaging and update costs,
  the decision to keep the model out of the installer for v7.0.0, and the
  criteria that would reopen it.
  (no test: a decision record, documented with its numbers and limits.)

### Not done / Known limitations (7.0.0)

- No model is bundled or downloaded by the app; local AI still requires Ollama
  or LM Studio installed by the user, as decided and documented.
  (no test: decision record in `docs/local-ai-decision.md`.)
- The eval harness scores recorded answers; it runs on CI, not on the user's
  machine against the user's model.
  (no test: by design; the harness is provider-agnostic and network-free.)

## Implemented (this cycle, 7.0.0) - Kaynak şeffaflığı v2

- **DOCX provenance manifests (D7, #531):** a DOCX produced by the app carries
  the C8 manifest shape as one package part (`omnioffice/provenance.json`); the
  recorded digest is computed over the package entries in a canonical order, so
  re-compressing the file does not report a false change while editing a part
  does. Re-embedding replaces the part instead of stacking it.
  `test:crates/officecore/src/docx_provenance.rs#embedding_round_trips_the_manifest_and_the_digest`
  `test:crates/officecore/src/docx_provenance.rs#changing_a_part_after_the_manifest_is_detected`
  `test:crates/officecore/src/docx_provenance.rs#the_digest_does_not_depend_on_zip_order_or_timestamps`
- **Unified source chain (D7):** `provenance_read_chain` sniffs a PDF or a DOCX
  and returns the derivation chain — sources with their SHA-256, the
  transformation step, the output digest — with an `intact` / `changed` /
  `absent` / `unsupported` verdict; the verification screen shows the links in
  order and puts a red tamper warning before the sources when the digest no
  longer matches.
  `test:src-tauri/src/provenance.rs#the_chain_lists_sources_then_the_transformation_then_the_output`
  `test:src/components/provenance-card.test.tsx#warns clearly when the document no longer matches its manifest`
- **Automatic producer embedding (D7):** format conversion to DOCX, OCR PDFs
  and merged PDFs write their manifest after the output is saved; the hook is
  best effort and never blocks the produced file. The generic
  `provenance_write` command embeds into either container, and the card keeps a
  DOCX copy path (`-provenance.docx`).
  `test:src-tauri/src/provenance.rs#a_produced_docx_gets_the_manifest_attached_automatically`
  `test:src/components/provenance-card.test.tsx#writes a DOCX copy with the picked sources and the app version`
- **Guide (D7):** `docs/provenance-v2.md` documents the manifest fields, the
  canonical DOCX digest, the chain verdicts, the producer hooks and the honest
  limits (no XMP in DOCX, a self-consistent forged manifest is out of scope,
  sources are not re-read, Android read-only).

### Not done / Known limitations (7.0.0)

- The chain detects changes to the document; it cannot prove a source file was
  the one recorded without the user supplying it again. The recorded digests
  are shown for out-of-band comparison, and no source is re-hashed while
  verifying.
  (no test: by design, documented in `docs/provenance-v2.md`.)
- The invoice export path reaches the manifest only through the generic
  conversion/save surface, not as a dedicated invoice hook.
  (no test: the office save dialog owns the output name.)

## Older cycles

The sections for 6.15.0 and earlier releases are in
[`docs/archive/release-readiness-6.x-and-older.md`](docs/archive/release-readiness-6.x-and-older.md).
`npm run claims:check` reads that file too, so their claims are still checked
against their tests.

## Platform support and artifacts

- **Windows**: source builds and tests locally; installer/portable ZIP via
  `npm run release:local` or CI, with SBOMs, checksums and provenance.
- **Android**: built by CI and by `npm run release:local`; artifacts are named
  `OmniOffice-Android-<version>-<abi>.apk` (and `.aab`).
- **Linux/macOS desktop**: built and tested by CI; the E2E jobs run on Linux
  and Windows.
- **Chrome extension**: unchanged.

## Security status

Unchanged from 3.5.4. The qpdf invocation runs the bundled, SHA-256-pinned
binary with a null stdin and a captured, length-capped stderr; the repair
module never shells out to a user-controlled program. A cancelled repair kills
the child and removes the partial output.
