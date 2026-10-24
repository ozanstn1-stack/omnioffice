# Release Readiness — OmniOffice 5.0.1

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

## Release trains and the freeze (v6.0.1)

The v6.0.0 review put a **feature freeze** in place: until its A-phase items
close, changes fix, verify or close what exists instead of adding features.
Two channels carry builds:

- **Stable** — the versioned releases; their installers check
  `releases/latest/download/latest.json`.
- **Beta** — `scripts/set-update-channel.mjs` (CI-only) and
  `.github/workflows/beta.yml` build a signed installer from master with a
  `X.Y.Z-beta.N` version (a prerelease of the *next* minor, so semver moves
  beta users forward and later pulls them back onto stable) and its own
  updater endpoint (`releases/download/beta/latest.json`), published to the
  rolling `beta` prerelease.

The channel is a property of the installed build; Settings shows which one and
links to the beta installers. Beta cadence is a manual dispatch until the
release-train schedule is decided.

## Implemented (this cycle, 6.1.13) - ODP okuyucu modülleri

- **ODP reader split** (#339): `odf/slides_read.rs` (shapes, frames) and
  `odf/slides_read_timing.rs` (timing, footers, package); `odf.rs` fell to
  1771 lines and the baseline was lowered.
  `test:crates/officecore/src/odf_tests.rs#odp_roundtrip`
- **Scope:** the ODP writer and the editor seams follow.
  (no test: tracked in #339.)

## Implemented (this cycle, 6.1.12) - ODS yazıcı modülü

- **ODS writer split** (#339): `odf/sheets_write.rs` owns the Calc export;
  `odf.rs` fell to 2282 lines and the baseline was lowered.
  `test:crates/officecore/src/odf_tests.rs#ods_roundtrip_with_formulas`
- **Scope:** the ODP halves and the editor seams follow.
  (no test: tracked in #339.)

## Implemented (this cycle, 6.1.11) - ODS okuyucu modülü

- **ODS reader split** (#339): `odf/sheets_read.rs` owns the Calc import;
  `odf.rs` fell to 2506 lines and the baseline was lowered.
  `test:crates/officecore/src/odf_tests.rs#ods_roundtrip_with_formulas`
- **Scope:** ODS writer, ODP halves and the editor seams follow.
  (no test: tracked in #339.)

## Implemented (this cycle, 6.1.10) - ODT okuyucu modülü

- **ODT reader split** (#339): `odf/text_read.rs` now owns the import path;
  `odf.rs` fell to 3418 lines and the file-size baseline was lowered.
  `test:crates/officecore/src/odf_tests.rs#odt_roundtrip`
- **Scope:** ODS/ODP halves and the Writer/Impress/Calc editor seams follow.
  (no test: tracked in #339.)

## Implemented (this cycle, 6.1.9) - Katı jeton denetimi

- **Raw paths rejected in desktop release builds** (#337): `input_file` and
  `output_file` require tokens; Android/debug keep the exception.
  `test:src-tauri/src/paths_tests.rs#debug_builds_accept_raw_paths`
- **MailMerge tokenized** with display labels; folder-setting exceptions are
  documented in `docs/file-tokens.md`. (no test: UI-only picker, covered by the
  token tests.)
- **Scope:** remaining B4 splits and the B2/2 plan (#316) follow.
  (no test: tracked in #337.)

## Implemented (this cycle, 6.1.8) - Kalan diyaloglar jetonlaştı

- **Remaining save dialogs** (#337): every save/export target listed in 4d-c/2b
  uses `pickSaveToken`; the shared DropZone picker and Recipes use
  `pickDocumentTokens`.
  `test:src/screens/Invoices.test.tsx#builds the VAT summary workbook and saves it as XLSX`
- **Display names for tokens** via `file-token-names.ts`; drop/launch events and
  dialogs populate it, so `fileBaseName(token)` keeps the UI readable. (no test:
  display-only cache exercised by the token tests.)
- **Scope:** the remaining input pickers (2c) and strict mode (4d-d) are next.
  (no test: tracked in #337.)

## Implemented (this cycle, 6.1.7) - Ofis yazma jetonları

- **Office in-place saves** (#337): `OpenDocument.token`, stored on the tab, sent
  by Ctrl+S; the real path still drives fingerprints, conflicts and recents.
  `test:src/office/useOfficeSession.test.tsx#never runs the compatibility report for a .oswk save`
- **Office save/export dialogs** use `pickSaveToken`; the workspace pickers use
  `pickDocumentToken`.
  `test:src/office/useOfficeSession.test.tsx#writes the lossless .oswk master instead when the user chooses it`
- **Scope:** the remaining screen dialogs (PFX, images, invoices/EML, mail
  merge, vault/sync folders, scanner, templates) and strict mode are next.
  (no test: tracked in #337.)

## Implemented (this cycle, 6.1.6) - Dizin jetonları

- **Directory tokens** (#337): `file_token_pick_directory` + `issue_dir`, and
  `paths::directory` resolves tokens; multi-output tools send the folder token.
  `test:src-tauri/src/paths_tests.rs#a_directory_token_resolves_through_the_directory_validator`
- **Fix:** the OutputBar file Browse now uses `pickSaveToken` (the v6.1.4 note
  claimed it, the code still opened the plugin dialog). (no test: UI wiring;
  the token wrapper is covered by `file-tokens.test.ts`.)
- **Scope:** the remaining dialog sources and strict mode are the next slices;
  Android uses SAF paths for folders. (no test: tracked in #337.)

## Implemented (this cycle, 6.1.5) - ODT yazıcısı ayrıldı

- **ODT writer split** (#339): `odf.rs` 6208 -> 4346 lines; the writer lives in
  `odf/text_write.rs`, the public API is re-exported and 241 officecore tests
  pass unchanged.
  `test:crates/officecore/src/odf_tests.rs#odt_manifest_matches_the_package`
- **Next B4 splits:** ODT reader, ODS/ODP halves, then the editor seams.
  (no test: tracked in #339.)

## Implemented (this cycle, 6.1.4) - Çıktı hedefleri jetonları

- **Output tokens** (#337): `suggest_output` issues the write token and returns
  the display path; the OutputBar's Browse uses `file_token_save`; commands get
  the token via `outputSpec`.
  `test:src/lib/file-tokens.test.ts#asks for a suggested save target and falls back to a display path`
- **Scope:** multi-output folders and Android SAF staging still use paths;
  strict mode and the B4 functional splits remain. (no test: tracked in #337 / #339.)

## Implemented (this cycle, 6.1.3) - Global seçici ve son dosyalar jetonları

- **Token-based global picker** and **recents in Rust** (#337): the open dialog
  returns tokens; `recent.json` keeps real paths and `load_recent` re-issues
  tokens with display names on every load, so a restart keeps the list without
  exposing paths.
  `test:src-tauri/src/recent_tests.rs#entries_become_token_refs_and_missing_files_are_dropped`
- **Scope:** output targets, the strict no-path mode and the B4 functional
  splits remain. (no test: tracked in #337 / #339.)

## Implemented (this cycle, 6.1.2) - Açılış ve sürükle-bırak jetonları

- **Launch and drop tokens** (#337): `startup_tokens`/`office_take_launch_files`
  return `FileRef`; the window drag-drop event issues tokens and emits
  `file-drop`; `file_names` gives display names.
  `test:src-tauri/src/files_tests.rs#names_resolve_tokens_and_plain_paths_without_a_path`
- **Routing by display name** for token documents.
  `test:src/lib/file-refs.test.ts#opens a PDF in the reader with the token, not the name`
- **Scope:** outputs, the global picker and the recents store still use paths.
  (no test: tracked in #337.)

## Implemented (this cycle, 6.1.1) - Jetonlarla araç seçicileri

- **Token-based tool pickers** (#337): `useTool` opens the Rust dialog, stores
  `{ path: token, token, name }`, releases the token on remove, and displays
  the FileRef name; `paths::lexical` resolves tokens like input/output.
  `test:src/lib/tool-files.test.ts#maps tokens to selected files without exposing a path`
- **Scope:** drop, launch/recents and the global App picker still use paths,
  and output targets remain paths. (no test: tracked in #337.)

## Implemented (this cycle, 6.1.0) - Yetki jetonları (slice 1)

- **Capability token registry and dialog commands** (#337): 128-bit tokens,
  read/read-write scope, 12 h TTL, revoke; a `FileRef` never carries a path.
  `test:src-tauri/src/files_tests.rs#a_read_token_cannot_be_used_for_writing`
- **Frontend wrapper** `src/lib/file-tokens.ts` for the pick/save/release
  commands.
  `test:src/lib/file-tokens.test.ts#asks the Rust dialog for a read token and passes the extensions`
- **Scope:** document commands still accept raw paths; the PDF/office/tool
  migration onto `files::resolve` is the next slice. (no test: tracked in #337.)

## Implemented (this cycle, 6.0.4) - Personalar ve sağlam E2E

- **Persona journeys** (#340): accountant, student, public and lawyer
  (redaction) run through the real UI and backend; `e2e/smoke.mjs --persona`
  selects them and the Desktop E2E jobs run all four. (no test: these journeys
  are the tests.)
- **E2E session stability** (#350): 150 s WebDriver bound, one `POST /session`
  retry, and the app is reaped between scenarios. (no test: exercised by the
  Desktop E2E job itself.)
- **Redaction helpers keep their unit coverage** after the move.
  `test:src/screens/pdf-screens.test.tsx#offers the sensitive data the detector found, and lets it be dropped`

## Implemented (this cycle, 6.0.3) - Bağımlılıklar ve araç parçaları

- **Paired dependency update** (#345): six Tauri plugin pairs upgraded on both
  sides (updater 2.14, fs 2.7.1, dialog 2.8.3, opener 2.7.2, single-instance
  2.5.4, android-fs 30.2.0), TypeScript 6.0.3, pbkdf2 0.13; the manifest gate
  checks all six pairs. (no test: the gate is
  `scripts/check-tauri-plugin-versions.mjs` in the Lint job, and every Rust
  suite ran against the new lockfile.)
- **Per-tool lazy chunks** (#328): the eight productivity screens live in
  `src/office/tools/*` and still render and act on their stores.
  `test:src/office/tools-screens.test.tsx#renders the converter, cleaner and PDF forms screens`

## Implemented (this cycle, 6.0.2) - Android indirmeleri (downloaded documents)

- **Downloaded docx/xlsx open from other apps** (#342): the typed intent filter
  accepts `application/octet-stream` (Chrome/DownloadManager hide the extension
  from the URI path), `doc`/`dot`/`docm`/`dotx`/`xlsm`/`pptm`/`ppt` are
  associated end to end, a rejected share now surfaces `errors.unsupported`,
  and display names lose control characters. (no test: pinned by the Android
  JVM tests `ManifestIntentFiltersTest` and `IncomingFilesTest` in the Android
  Kotlin CI job; the claims verifier reads the Rust/Vitest logs only.)

## Implemented (this cycle, 6.0.1) — Durdur, doğrula, dürüst ol (Stop, verify, be honest)

The v6.0.0 audit's A-phase, all verified in this tree:

- **Release channels** (#296): build-time beta endpoint, rolling `beta`
  prerelease, Settings display. `test:src/lib/update.test.ts#reads the channel from the installed version`
- **Verifiable redaction certificate** (#298): attached file + XMP metadata,
  strip-and-rehash verification. `test:crates/pdfcore/src/certificate.rs#a_modified_file_fails_verification`
  and `test:crates/pdfcore/src/certificate.rs#the_certificate_never_carries_a_redacted_value`
- **Crash-proof parsers** (#300): worker process with time, output and memory
  limits. `test:src-tauri/src/worker.rs#a_corrupt_workbook_is_an_error_not_a_crash`
- **Own tax id and VAT split** (#302): sales vs purchases, strict period.
  `test:src/lib/kdv-declaration.test.ts#splits sales and purchases by the configured own tax id`
- **Spell checker completeness** (#304): capitalized words, `-ken`, curly
  apostrophes, word cache. `test:crates/turkish/tests/spell_test.rs#capitalized_words_are_checked_against_the_proper_noun_list`
  and `test:crates/turkish/tests/spell_test.rs#the_ken_suffix_is_recognized`
- **UDF and vendor exports closed by default** (#275).
  `test:src/lib/flags.test.ts#keeps the unverified formats hidden by default`
- **Release gate** (#277): tags only after a green Desktop run, claims
  verified against the passing tests.
  `(no test: the gate is the workflow plus scripts/claims-audit.mjs, which this release runs in CI)`
- **Honesty cleanup** (#294): experimental screens and the slim README.
  `test:src/lib/flags.test.ts#keeps the unfinished screens hidden by default`
- **Routed v5.3-v6.0 screens** (#279).
  `test:src/screens/KvkkWizard.test.tsx#renders KVKK wizard and switches template types`
- **Invoice math** (#281): duplicate key and the same-amount-and-date rule.
  `test:src/lib/duplicate-invoices.test.ts#detects exact duplicate ETTNs and deduplicates them`
- **AI consent and certificate honesty** (#285).
  `test:src/lib/redact-cert.test.ts#builds a structured redaction certificate with valid metadata`
- **Android sharing** (#287): `.eml`/`.xml` match the typed intent filter.
  `test:src/lib/android-manifest.test.ts#opens and shares every supported type through one typed filter without a scheme`
- **Writer proofing** (#283): spell + official letter panels.
  `test:src/lib/official-letter.test.ts#detects non-compliant fonts and auto-fixes to Times New Roman`
- **CI leftovers** (#289): hardened fuzz targets, pinned runners.
  `(no test: the Fuzz job runs the hardened targets on every master push)`

## Implemented (this cycle, 6.0.0) — Gerçek mobil (True mobile)

- **Scanner page list** (#266): move up/down reordering and batch clearing on
  the scanner screen, on top of the existing Rust perspective-correction and
  PDF assembly. `test:src/screens/Scanner.test.tsx#allows reordering and clearing pages`
  The screen itself stays behind `experimentalScanner` (off by default) until
  the camera capture gets a real device test.
- **Android shortcuts and ACTION_SEND sharing** (#267): `.eml` and `.xml`
  shares route to the e-mail and e-invoice previewers; the intent filters are
  guarded by `test:src/lib/android-manifest.test.ts#opens and shares every supported type through one typed filter without a scheme`
  `(no test: the Kotlin unit tests IncomingFilesTest/ManifestIntentFiltersTest run in the tag-triggered Android workflow, not in the Desktop matrix)`
- **SAF compatibility and emulator smoke test** (#268): Storage Access
  Framework MIME mapping, document export/share and an Android launch smoke
  test. `test:src/lib/mobile.test.ts#exports the office whitelist used by the Android shell and the pickers`
  `(no test: the emulator smoke test runs in the Android workflow)`
- **XLSX parser panic guard** (#273): `catch_unwind` around the calamine
  reader so a malformed workbook reports an error instead of taking the app
  down. `(no test: release builds use panic=abort, so the guard cannot be
  exercised end to end yet; the worker-process fix is tracked in the v6.0.1
  audit plan)`

## Implemented (this cycle, 5.1.0 – 5.5.0) — contracts, Turkish suite, KVKK package, accounting bridge

The v5.1.0–v5.5.0 cycles shipped without their own `Implemented` sections.
This is the honest, test-linked record of what went out (written in v6.0.1);
it claims nothing that is not in the tree today.

### 5.1.0 — Sözleşme ve doğruluk (Contract and correctness)

- **Type generation and drift gate** (#222): ts-rs bindings under
  `src/lib/generated`, produced by `npm run types:gen` and guarded by
  `types:check`. `test:src/lib/ipc-contract.test.ts#RecipeStep carries the same variants and fields as the Rust enum`
- **Real serde wire contract** (#225): 20 DTOs pinned against real
  `serde_json` output. `test:src-tauri/src/wire_contract.rs#office_and_recipe_dtos_keep_their_wire_shape`
- **"Claim = test" CI rule** (#229): this script is the gate and runs in the
  Lint job. `(no test: scripts/claims-audit.mjs is itself the check)`
- **Feature flags** (#231): `src/lib/flags.ts`, the scanner hidden behind an
  off-by-default flag. `test:src/lib/flags.test.ts#keeps the experimental scanner hidden by default`

### 5.2.0 — Gerçek dosya laboratuvarı (Real-file lab)

- **License-clean sample corpus** (#235): `tests/corpus` across five formats
  with a manifest, documented in `docs/corpus.md`.
  `test:crates/officecore/tests/corpus_test.rs#every_corpus_sample_imports_through_its_reader`
- **Compatibility score** (#237): `crates/compat` structural + visual scores
  against committed goldens, nightly report, LibreOffice reference.
  `test:crates/compat/tests/score_test.rs#the_corpus_scores_above_the_threshold`
- **LibreOffice ODF fix** (#238): the manifest is generated from the entries
  actually written, so LibreOffice opens our ODT/ODS/ODP output.
  `test:crates/officecore/src/odf_tests.rs#odt_manifest_matches_the_package`
- **Per-importer fuzz targets** (#241): `docx`, `xlsx`, `pptx`, `udf`, `ubl`.
  `(no test: the Fuzz job runs the targets on every master push and nightly)`

### 5.3.0 — Türkçe yazım ve resmî yazı denetimi

- **Offline Turkish spell checker** (#245): Zemberek lexicon (Apache-2.0),
  suffix analysis, suggestions. `test:crates/turkish/tests/spell_test.rs#common_misspellings_are_caught`
  and `test:crates/turkish/tests/spell_test.rs#clean_text_has_few_false_positives`
- **Official letter compliance checker** (#246): regulation rules with
  1-click fixes. `test:src/lib/official-letter.test.ts#detects non-compliant fonts and auto-fixes to Times New Roman`
- **Petition wizard** (#247): TCKN mod-11 validation, DOCX and PDF output.
  `test:src/lib/petition.test.ts#validates TCKN mod-11 correctly`
  The UYAP `.udf` output is **disabled by default** since v6.0.1 (#275).

### 5.4.0 — Belge sağlığı ve KVKK paketi

- **Document health report** (#252): risk list with 1-click fixes.
  `test:src/lib/document-health.test.ts#detects hidden metadata leaks and applies 1-click fix`
- **Redaction certificate** (#253): structured certificate with hashes.
  `test:src/lib/redact-cert.test.ts#builds a structured redaction certificate with valid metadata`
  (the certificate still fabricates a default hash when none is provided; the
  real SHA-256/RFC 3161 version is tracked in the v6.0.1 audit plan)
- **KVKK wizard and templates** (#254).
  `test:src/screens/KvkkWizard.test.tsx#renders KVKK wizard and switches template types`
  The wizard is not routed in the app yet; routing is tracked in the v6.0.1
  audit plan.

### 5.5.0 — Muhasebe köprüsü (Accounting bridge)

- **Vendor accounting exports** (#259): Luca/Logo/Zirve CSV writers.
  `test:src/lib/accounting-export.test.ts#delegates properly through exportAccounting wrapper`
  **Disabled by default** since v6.0.1 (#275) until a real import template
  verifies them.
- **KDV declaration helper** (#260).
  `test:src/lib/kdv-declaration.test.ts#accurately summarizes VAT breakdown across rates 1%, 10%, and 20%`
- **Duplicate invoice detection** (#261).
  `test:src/lib/duplicate-invoices.test.ts#detects exact duplicate ETTNs and deduplicates them`

## Implemented (this cycle, 5.0.1) — Kanama durdurma (Stopping the bleeding)

Every P0 from the v5.0.0 audit (`docs/plan/DENETIM-v5.0.md`, PR #180), each
with a regression test that failed before the fix:

- **rustls 0.23.46** (issue #181): clears RUSTSEC-2026-0347/-0348 and two more;
  the `Dependency audit` CI gate is green again. `(no test: the gate itself is the check — the Dependency audit job fails on any new advisory)`
- **UTF-8 panic safety** (issue #183): `truncate_at_char_boundary` in the plugin
  PDF reader `test:src-tauri/src/plugin.rs#truncating_to_the_byte_limit_never_splits_a_character`;
  `chunk_snippet` folds 1:1 (`fold_char`) `test:src-tauri/src/vault.rs#chunk_snippet_survives_turkish_case_folding`;
  `VectorStore::load` rejects a short `embeddings.bin` and `set/retain/search`
  skip missing vectors `test:src-tauri/src/vault_index.rs#a_truncated_embeddings_file_is_rejected_not_panicked_on`.
- **`.eml` nesting** (issue #185): `parse_eml_with_depth` carries the depth
  across `message/rfc822`; a 20,000-deep bomb parses with a warning instead of
  overflowing the stack `test:crates/officecore/src/eml.rs#nested_messages_are_depth_limited_not_stack_overflowing`.
- **e-Devlet host** (issue #187): `officialTurkiyeUrl` requires an https
  `turkiye.gov.tr` (or subdomain) host; lookalike hosts and http are refused
  `test:src/lib/edevlet.test.ts#refuses a URL whose host merely contains the name (phishing)`.
- **Calc numbers** (issue #189): `cellEditText` writes numbers with the active
  locale's separator so edits round-trip; `%18`/`18%` are `0.18`
  `test:src/office/calc/locale.test.ts#round-trips a number through the editor text in both locales`.
- **Font integrity** (issue #191): a stamp picks a free `/OmniTrN` resource
  name instead of overwriting an earlier stamp's font
  `test:crates/pdfcore/tests/stamp_test.rs#a_second_stamp_keeps_the_first_stamps_glyphs`;
  form filling keeps the document's own `/Helv`
  `test:crates/pdfcore/tests/forms_test.rs#filling_a_field_keeps_the_documents_own_helvetica`.
- **Comparison** (issue #193): blocks align by type (removed blocks are shown)
  `test:src/office/writer/compare.test.ts#shows a removed table as a deletion instead of dropping it`,
  runs keep their own formatting `test:src/office/writer/compare.test.ts#keeps each run's own formatting`,
  and the LCS trims prefix/suffix with a cell cap for memory.
- **Blob store** (issue #195): blobs are marked sent only after the backend
  confirms the write `test:src/lib/blob-store.test.ts#resends the blobs after a failed save`;
  `recovery_gc_blobs` runs on refresh `test:src-tauri/src/office.rs#blob_refs_are_collected_for_gc`.
- **Watched folder** (issue #197): overlapping folders are refused and never
  scanned `test:src-tauri/src/recipes.rs#a_watch_pass_never_writes_into_its_own_input_folder`,
  a generation counter plus `Notify` prevents two loops, and the password is
  stored via `secret::save_api_key` (DPAPI/keystore).

### Not done in this pass (honest)

- The UDF reader/writer is **disabled by default** (v6.0.1): it was never
  verified against a real UYAP sample, so every user-facing entry point (open
  and save dialogs, converter, compatibility center, petition wizard, Windows
  file association, Android intent filters) is closed behind the off-by-default
  `experimentalUdf` flag. Reopening requires a real UYAP file that round-trips.
  The code, tests and the fuzz target stay in the build.
- Comparison renders a removed table as delete-marked text (no block-level
  revision in the model) and carries a matched table over from the revised side.
- The remaining P1 audit items (vault Turkish folding, invoice badge, tracked
  change author, e-Fatura validation, KVKK detector, hyphenation, Android
  shortcuts, plugin batch-step wiring, mail-merge encodings, i18n gaps) are not
  in this release.

## Implemented (this cycle, 5.0.0) — Erişilebilir ve hızlı (Accessible and fast)

- **Mail merge** (issue #114): `crates/officecore/src/mailmerge.rs` finds
  `{{placeholders}}`, parses CSV/TSV rows and fills the document; the
  `MailMerge` screen previews and reports missing fields.
- **Document comparison** (issue #115): `src/office/writer/compare.ts` diffs two
  documents word-level and emits tracked changes through the revision engine;
  the Writer "Compare with…" action loads a version and applies them.
- **Plugin API v2** (issue #116): `pdf` capability (`plugin_pdf_read`,
  sandbox-scoped) and `batch-step` manifest surface; `docs/plugins.md` and two
  example plugins (`plugins/examples/*`). Tests: capability/permission gating.
- **Tagged PDF export** (issue #117): `pdfcanvas.rs` emits `StructTreeRoot`,
  `/Lang`, `/MarkInfo`, `/ParentTree` and `/Alt`; the Inspector reports
  `a11y.figure-alt`. Tests: structure tree, `/H1`, `/Lang`, missing-alt finding.
- **Reader text layer + read-aloud** (issue #118): `src/lib/reader-text.ts`
  (pure geometry/speech helpers) plus a transparent overlay and Web Speech
  controls. Tests: geometry, text join, speech detection.
- **Performance** (issue #119): `src/lib/blob-store.ts` content-addressed image
  store wired into autosave (`recovery_save_blobs`/`recovery_load`/
  `recovery_gc_blobs`), and the Turkish dictionary split into a lazy chunk.
- **Audit closure** (issue #120): L2–L5 and the H5 follow-up closed with tests;
  new `pdf_parse` fuzz target in CI.

### Not done in this pass (honest)

- Tagged export does not tag table cells, headers/footers or XMP metadata, and
  is PDF/UA-oriented, not certified.
- Comparison is word-level and position-aligned; tables are carried over.
- Read-aloud depends on the platform's Web Speech voice and needs a text layer.
- The blob store covers autosave/recovery; normal Save writes the full model and
  blob GC is available but not scheduled.
- The plugin batch-step hook is exposed to the recipe UI but not auto-registered
  by the Rust recipe engine.

## Implemented (this cycle, 4.12.0) — Otomasyon (Automation)

- **Recipe engine** (issue #112): `src-tauri/src/recipes.rs` defines a `Recipe`
  of `RecipeStep`s (compress, stamp, pdfa, ocr, redact); `run_recipe` chains
  each step's output into the next and writes the result atomically.
  `recipe_save/list/delete` persist recipes. Tests: a **stamp → compress chain**
  produces a PDF, an empty recipe is refused, id sanitisation.
- **Watched folder** (issue #112): `WatchedFolder` config plus `scan_once`
  (dedup across passes), `watched_folder_scan` (one pass) and
  `watched_folder_start/stop` (polling loop that carries the seen set).
  Tests: a pass processes a new file once and skips it on the second pass.
  The **Recipes** screen builds/saves/runs recipes and configures the folder.
- **CLI** (issue #113): `src-tauri/src/cli.rs` parses `--cli merge/compress/ocr`
  with `-o`/`--lang` and returns 0/1/2. On Windows `AttachConsole` binds the
  parent console and output goes to `CONOUT$`/`CONERR$`. Tests: parsing (GUI
  pass-through, merge, OCR languages, usage errors) and a merge that produces a
  `%PDF`.
- **Explorer "Send to"** (issue #113): `src-tauri/windows/hooks.nsh` creates and
  removes a `$SENDTO` shortcut, wired through `installerHooks`.

### Not done in this pass (honest)

- The watched folder is **polling**, not notification-based; the long-running
  loop is not unit-tested.
- The recipe UI adds steps with **defaults** (OCR `tur+eng`, stamp
  `Sayfa {page}`); the detailed options are set in the dedicated tools.
- The CLI covers **merge/compress/ocr** only.
- The **NSIS hook and the "Send to" menu were not verified by a local install**;
  the hook is compiled by the Windows release build.
- Recipe steps bypass `JobRegistry` (direct pdfcore calls + a `recipe:progress`
  event).

## Implemented (this cycle, 4.11.0) — Cepte Ofis (Office in your pocket)

- **Perspective correction** (issue #108): `crates/pdfcore/src/scan.rs` orders
  four corners, derives the output size and resamples with a homography;
  `correct_file` reads an image and writes a corrected PNG. Tests: 6 unit tests
  (ordering, identity, offset, size, degenerate quad reported not panicked,
  shuffled corners).
- **Scanner command and screen** (issue #108): `office_scan_pages` corrects the
  pages that carry corners and assembles a multi-page PDF through the existing
  image writer; the **Scan** screen picks page photos and calls it. Tests: 2
  `office.rs` tests (PDF produced, empty list refused) and 2 screen tests.
- **Android CAMERA permission** (issue #108): declared, with the camera feature
  `required="false"`.
- **Share filters** (issue #109): the typed filter accepts `image/*`, the
  specific image types, `text/plain`, `text/markdown`, `text/html`, `text/csv`
  and `text/tab-separated-values`; both name fallbacks cover every image/text
  extension. The TS and Kotlin contract tests pin all of it.
- **Launcher shortcuts** (issue #109): `res/xml/shortcuts.xml` with Scan, New
  document and Recent files, plus the `android.app.shortcuts` metadata and the
  label strings.
- **Signature library** (issue #110): `src-tauri/src/signatures.rs` saves/lists/
  deletes PNGs with a sanitised name and a 2 MB cap. The Annotate screen saves,
  lists and reuses a signature, adds dated ready stamps as `/Annots` text boxes
  and applies a paraphe to every page. Tests: name/base64 unit tests and a
  screen test that a stamp reaches `pdf_annotate_editable` as a `textbox`.
- **Home recommendations and pinning** (issue #111): `src/lib/recommend.ts`
  maps an extension family to the tools that fit it; the Home screen uses it and
  lets tools be pinned (persisted in `settings.pinnedTools`); the duplicated
  sidebar group label is fixed with `nav.productivity`. Tests: 6 recommendation
  tests and a Home pinning test.

### Not done in this pass (honest)

- **The Android camera capture is not implemented.** The ML Kit document scanner
  / CameraX capture, the manual four-corner pick and the runtime CAMERA
  permission request are the remaining part of issue #108. The Scan screen
  states this; the Rust half and the screen scaffolding are complete.
- **Shortcut routing is not wired**: the shortcuts appear and open the app, but
  the action is not routed to the target screen.
- **No device/emulator verification** of the share, shortcut or scanner flows
  was possible in this environment; the CI Android job compiles the Kotlin tests
  (including the manifest contract test) but the on-device flows are untested.
- The perspective correction is implemented directly rather than with
  `imageproc` (same projection, no extra dependency).
- **Signatures are stored unencrypted** in the app data directory.
- The scanner does not run OCR; the OCR tool is used on the result.
- Tool pins are stored in the given order; there is no manual reordering.

## Implemented (this cycle, 4.10.0) — Belge Asistanı 2.1 (Document Assistant 2.1)

- **Persistent hybrid index** (issue #101): `src-tauri/src/vault_index.rs` —
  real BM25 (inverted index, IDF, length normalisation), a per-chunk embedding
  store (`embeddings.bin` + `index.json`, key `model + sha256(chunk)`), and RRF.
  `vault_search_hybrid` and the chat vault scope use it; `vault_index_build`
  embeds the pending chunks with a progress event and cancellation. Tests: 9
  unit tests including a **5.000-chunk query under 300 ms**, plus a vault test
  that the index persists and is not rebuilt when unchanged.
- **UI wiring and citation targets** (issue #102): the Vault screen calls
  `vault_search_hybrid`; `src/lib/citation-target.ts` resolves a citation to the
  right file (PDF page in the Reader, office file in the workspace). Tests: 6.
- **Verified citations** (issue #103): `crates/aicore/src/citations.rs` parses
  `[page N]` / `[sayfa N]` / `[pages 3-4]` / `(p. 4)` and fuzzy-matches the
  quote (Turkish-folded). The backend marks each citation `verified`; a page
  that was not retrieved is rejected; the UI shows an "unverified" badge.
  Tests: 5 unit + a screen test.
- **Prompt-injection hardening** (issue #104): `doc_chat_messages` delimits the
  context; the system prompt states it is data; `sanitize_history` refuses a
  `system` role and trims to `HISTORY_BUDGET_CHARS`. Tests: 3.
- **Bounded batch queue** (issue #105): `ai_batch_run` uses a `JoinSet` +
  `Semaphore(3)`; `chat_with_retry` retries a `429` after the provider's
  `Retry-After`; `chat_cancellable` aborts an in-flight call; per-item password;
  approximate tokens/cost. Tests: cost, retry delay, concurrency bound (Rust)
  and a screen test (password + cost).
- **Vision gate** (issue #106): the toggle is disabled without `vision`; the
  image has a width and 4 MB limit with a smaller-dpi retry; a failure sets
  `AiChatResult.warning`. Contract tests cover the OpenAI-compatible (incl.
  Gemini) `image_url` format and the Anthropic base64 `source` format.
- **Consent and privacy** (issue #107): separate consents (base/vault/embedding/
  image) enforced by `require_consent` in the backend; LM Studio (and a
  localhost base URL) is local; AI jobs persist no payload in `jobs.json`;
  `ai_clear_data` deletes chats (all or older than the retention) and the vault
  index; Settings has the retention field and the clear button. Tests: consent,
  retention expiry, jobs payload, LM Studio, and screen tests.

### Not done in this pass (honest)

- Embeddings are requested **one chunk at a time**; no provider batch endpoint
  is used.
- The **token/cost** figure is an estimate (characters / 4 and a per-model price
  table), shown with a `~`.
- **Cancelling a non-streaming call** takes effect on the next poll tick.
- An **office** citation opens the file but does not scroll to the paragraph,
  sheet or slide.
- The **consents are per session**; they are not remembered across restarts.
- `vault_search_hybrid` (the Vault screen) does not itself gate embeddings; it
  falls back to BM25 when they are not configured. Only the AI chat vault scope
  and `vault_index_build` require the embedding consent.
- The **retention** applies to saved chats only; documents and the vault
  metadata are not deleted, only the derived index.
- The total JS bundle budget stays at 500 KB.

## Implemented (this cycle, 4.9.0) — Resmî Belgeler (Official documents)

- **UYAP `.udf`** (issue #96): `crates/officecore/src/udf.rs` reads and writes
  the ZIP + `content.xml` container (paragraphs, headings, lists, character
  formatting, alignment, indents, spacing, tables with spans, page setup,
  headers/footers). Unknown elements are kept verbatim in
  `TextDocument.preservedUdf` and re-exported.
  **Status (v6.0.1): disabled by default** — the format was never verified
  against a real UYAP sample, so the Windows `fileAssociations`, the Android
  intent filters, `IncomingFiles.openableExtensions`, the converter targets,
  the pickers and the petition wizard's UDF button are all closed behind the
  off-by-default `experimentalUdf` flag.
  Tests: 8 unit tests plus `tests/udf_fixture_test.rs` over a hand-authored
  `tests/fixtures/uyap-ornek.udf`; a `src-tauri` open/save/convert test.
- **e-Fatura / e-Arşiv UBL-TR** (issue #97): `crates/officecore/src/ubl.rs`
  parses XML and ZIP invoices, validates the line totals, VAT, tax-exclusive/
  inclusive and payable amounts against the dated reference rate table,
  renders a deterministic Writer document and aggregates a VAT summary
  workbook. Commands `office_open_invoice`, `office_invoice_summary`,
  `office_vat_rates`; the **Invoices** screen (`src/screens/Invoices.tsx`).
  Tests: 8 unit tests (including a withholding example and an unknown rate) and
  a `src-tauri` open → PDF → XLSX test.
- **GİB karekod** (issue #98): `crates/pdfcore/src/gibqr.rs` decodes the QR from
  a rendered PDF page with `rxing` (QR only), parses the JSON payload and
  checks the VKN/TCKN. `office_read_invoice_qr`; the Invoices screen reads a
  PDF's karekod and compares it with the UBL document, showing disagreements in
  red (`src/lib/invoice-qr.ts`). Tests: 7 unit tests (encode→decode at three
  resolutions), 7 comparison tests, and a screen test.
- **e-Devlet verification** (issue #99): `src/lib/edevlet.ts` detects a
  "barkodlu belge" block; the Reader shows a "Verify document" button that
  opens `https://www.turkiye.gov.tr/belge-dogrulama` through
  `open_external_link`. Tests: 7 detector tests, a `links.rs` allow-list test,
  and a Reader screen test.
- **KEP `.eml` viewer** (issue #100): `crates/officecore/src/eml.rs` parses
  RFC 822 / MIME (multipart mixed/alternative, base64, quoted-printable,
  RFC 2047 encoded-words, ISO-8859-9) read-only and extracts attachments
  safely. `office_open_eml`; the **E-mail (.eml)** screen. Tests: 7 unit tests,
  a fixture test over `tests/fixtures/kep-ornek.eml`, and a screen test.

### Not done in this pass (honest)

- UDF is a **closed format** implemented from its documented container and the
  module's own files; **no open-licensed third-party `.udf` sample was
  available in this environment**, so compatibility with the official editor is
  not proven. Unknown elements keep their bytes, not their formatting. Since
  v6.0.1 the format is disabled by default (`experimentalUdf` flag) for exactly
  this reason.
- The **invoice XSLT is never executed** (the app always uses its own template).
- GİB karekod verification is against the **UBL document**, not the AI
  extraction cards; the AI screen is not wired to it.
- A **password-protected or encrypted KEP message is not decrypted**; the
  S/MIME part is kept as an attachment and reported.
- An HTML mail body is shown as **plain text**; no remote resource is loaded.
- The **LibreOffice two-way conversion check** for UDF/UBL was not run (UDF is
  closed; the UBL→XLSX path is covered by the XLSX import/export tests).
- The total JS bundle budget moved from 485 to **500 KB**; the justification is
  in `scripts/check-bundle-budget.mjs`.

## Implemented (this cycle, 4.8.0) — Türkçe Doğru (Turkish correctness)

- **Turkish text in generated PDFs** (issue #88): `crates/pdfcore/src/textfont.rs`
  loads a Type0/CID font and stamps, page numbering and form fields draw with
  it, so `ı İ ğ ş ç ö ü` survive. Covered by a pdfium text-extraction test in
  `crates/pdfcore/src/textfont.rs` (runs locally because the pdfium engine is
  fetched by `npm run engines:fetch`). The remaining over-page text paths
  (annotate, flatten, sign) still use the built-in fonts and are tracked in
  issue #137.
- **Calc Turkish locale** (issue #89): `src/office/calc/locale.ts`,
  `numberFormat.ts` and `cells.ts` parse `1.250,15` on input and format numbers
  and dates with the Turkish locale. Tests in `src/office/calc/*.test.ts`.
- **Turkish formulas and validation** (issue #90):
  `src/office/calc/functions/turkish.ts` (`YAZIYLA`, `TCKNDOGRULA`,
  `VKNDOGRULA`, `IBANDOGRULA`) and `tr-holidays.ts` (Diyanet table, 2020–2029)
  wired into `NETWORKDAYS`/`WORKDAY` through the `"TR"` argument
  (`datetime.ts`). Tests in the same folder and `tr-holidays.test.ts`.
- **Turkish first run and search folding** (issue #91): `src/lib/store.ts`
  follows the OS language on first run; `src/lib/search-fold.ts`,
  `crates/synccore` and `src-tauri/src/vault.rs` fold Turkish case so `İ/ı`
  match. Tests in `search-fold.test.ts` and the vault tests.
- **KVKK redaction** (issue #92): `crates/pdfcore/src/redact.rs` detects TCKN,
  VKN, TR IBAN, plates and TR phone numbers with a false-positive test set, and
  `src/screens/Batch.tsx` redacts them across a batch. Tests in `redact.rs` and
  `Batch.test.tsx`.
- **Turkish hyphenation** (issue #93): `crates/officecore/src/hyphen_tr.rs`
  breaks words at syllable boundaries, and justified Writer PDF export uses it
  (`layout.rs`). A 500-word golden list
  (`crates/officecore/tests/fixtures/hyphen-tr-golden.txt`) is shared with the
  TypeScript twin `src/office/writer/hyphen-tr.ts`, and the two are compared
  word by word in `hyphen-tr.test.ts`.
- **Author name** (issue #94): `Settings.authorName` marks new Writer comments
  and tracked changes (`src/office/writer/identity.ts`, `WriterEditor.tsx`).
  Tests in `identity.test.ts`, `Settings.test.tsx`; the DOCX round trip of a
  comment and reply author is asserted in `docx.rs`.
- **Seven Turkish templates** (issue #95): Resmî Yazı, İhtarname, Kira
  Sözleşmesi, İzin Dilekçesi, İstifa Dilekçesi, Bordro and İrsaliye added to
  `src/office/templates.ts` (12 Turkish templates in total). `templates.test.ts`
  checks the `{{alan}}` placeholders, scans every template for a valid TCKN,
  IBAN, `+90 5xx` phone and personal e-mail (none found), evaluates the Bordro
  formulas and hands each template to `office_export_pdf`.

### Not done in this pass (honest)

- Hyphenation drives the PDF export only; the **editor view** still uses the
  browser's hyphenation (soft-hyphen integration not implemented).
- Turkish public holidays cover **2020–2029** only; 2030+ is omitted because
  2030 has two Ramazans.
- Annotate, flatten and sign still draw their text with the built-in fonts
  (issue #137).
- Calc formulas keep `,`/`;` as argument separators, so a decimal **inside a
  formula** uses a dot (`YAZIYLA(1250.15)`); cell input accepts `1.250,15`.
- The Chrome-extension dependency update is tracked in issue #129.

## Implemented (this cycle, 4.7.1) - repairs and honesty

- Invoice extraction: snake_case/camelCase aliases, default page, tolerant
  amount parsing (`1.250,00`, `1,250.00`, `1250 TL`, `(1.234,56)`), per-row
  parsing, and `InvoiceParseError` carrying the raw reply (no silent zero
  rows). Tests in `crates/aicore/tests/client_test.rs`.
- i18n: 50 missing keys added (en+tr) and `scripts/i18n-audit.mjs` now scans
  `t()`/`label()`/`tr()` usage and fails `--check` on unknown keys.
- Update security: verification failures surface a translated error instead of
  the browser fallback; the URL allow-list parses and normalises URLs (rejects
  `..`/`%2e`); Android requires the release SHA-256 (frontend pre-check and
  backend refusal); pre-release versions compare per SemVer. Tests in
  `update.rs`, `android_update.rs` and `src/lib/update.test.ts`.
- Batch AI: Android SAF picking (`pickOfficeFiles`) and `AiBatchResult.status`
  (`completed`/`cancelled`/`failed`) with the job guard succeeding only on a
  real completion. Tests in `jobs.rs` and `Ai.test.tsx`.
- CSV export: formula-injection neutralisation and exact column mapping.
- Release pipeline: version consistency gates both builds, `auto-tag` checks
  `tauri.properties`, release notes carry only the version's CHANGELOG
  section, `desktop.yml` compiles the Kotlin sources and runs the Android unit
  tests when `src-tauri/gen/android/**` changes, and the Gradle shell wrapper
  is stored LF.
- Docs: the 4.7.0 claims corrected; 4.6.2 and 4.7.0 readiness sections added;
  the README key-storage note fixed; the audit status note updated to 4.7.0.

### Not done in this pass (honest)

- Invoice line items remain a summary string; itemised rows are not extracted.
- BM25, the hybrid-search UI wiring and the bounded batch queue are 4.10.0
  work.
- The Windows installer is not Authenticode-signed (SmartScreen warning) and
  Android in-app install needs the "install unknown apps" permission.
- macOS/Linux packaging stays out of scope; the Chrome-extension dependency
  update is tracked in its own issue.

## Implemented (this cycle, 4.7.0) — Belge Asistanı 2.0 (Document Assistant 2.0)

- **Document Chat 2.0 & Citations**: Grounded multi-turn document conversations
  persisting conversation history. Inline `[page N]` citation cards parsed from
  model responses and deep-linked into the PDF Reader (`onNavigate('reader', { files: [path], targetPage: page })`),
  which automatically scrolls to the specified page on load.
- **Local Thread Persistence**: Chat threads can be named and saved locally
  with full conversation histories, listed, reloaded, and deleted (`ai_chat_save`,
  `ai_chat_list`, `ai_chat_load`, `ai_chat_delete`).
- **Vision Model Support**: Optional toggle to render active document pages
  as images and send base64 image attachments to vision-capable models
  (Claude 3.5 Sonnet, GPT-4o, Ollama LLaVA/minicpm-v, LM Studio).
- **LM Studio Local Preset**: First-class support for LM Studio local inference
  server (`http://localhost:1234/v1`) with zero telemetry and offline privacy.
- **Vault-scoped chat retrieval**: the chat's vault scope ranks excerpts by
  keyword matching and, when an embedding model is configured, by embedding
  cosine similarity (`retrievalMode` reports `keyword` or `hybrid`). The
  `vault_search_hybrid` command exists but is not called from the UI yet, and
  BM25 is not implemented; both are planned for 4.10.0.
- **Invoice & Receipt Table Extraction**: `ai_extract_table` fills
  `InvoiceTableRow` rows with page, vendor, date, invoiceNumber, currency,
  subtotal, tax, total and a lineItems summary string, plus any requested
  custom fields, into an interactive preview table with RFC 4180 CSV export.
  (The 4.7.0 build lost snake_case replies and rows without a page; 4.7.1
  fixed the parser and reports unparseable replies instead of zero rows.)
- **Batch AI Queue**: multi-file batch processing (`ai_batch_run`, one file at
  a time) for summarize, translate, classify and extract, with the
  `ai:batch_progress` Tauri event and per-file result cards. (4.7.1 adds an
  honest `status`: cancelled or all-failed runs no longer report completed.)
- **Privacy & Telemetry**: Zero telemetry maintained; explicit opt-in consent
  dialogs for external providers; all data stays on device unless local or cloud
  API explicitly requested by the user.
- **Automated Tests**: Unit and integration tests in `crates/aicore` (multi-modal
  chat, image payloads, embeddings, prompts, parsers), `src-tauri` (AI commands,
  vault hybrid search), and Vitest screen suite (`src/screens/Ai.test.tsx`).
- **Translations**: 73 new translation keys added in English and Turkish;
  en/tr parity verified at release time (2,148 keys each).

### Not done in this pass (honest)

- BM25 keyword search is not implemented; vault retrieval is keyword matching
  plus embedding cosine similarity, and `vault_search_hybrid` is not called
  from the UI (planned for 4.10.0).
- The 4.7.0 invoice extractor dropped snake_case replies and rows without a
  page number, and reported zero rows without an error when parsing failed;
  4.7.1 fixed the parser.
- The AI screen used English fallbacks for 50 keys missing from the translation
  table (the parity check could not see them); 4.7.1 added the keys and made
  the i18n audit scan source usage.
- The `v4.7.0` tag points at the post-release `tauri.properties` sync merge
  (`b7c4e5c`), not at the release PR merge: the tag was moved after publishing.
- Batch items run in sequence, not simultaneously (the CHANGELOG wording was
  corrected).

## Implemented (this cycle, 4.6.2) - Android update installer fix

- The 4.6.1 Android build failed: the new update-installer plugin's comment
  contained `/*`, which Kotlin's nested block comments turn into an
  unterminated comment. The comment is fixed and the release ships the Windows
  installer/ZIP and both APK/AAB ABIs again.

### Not done in this pass (honest)

- 4.6.1 was a Windows-only release because of that Android build failure;
  4.6.2 is the first complete release of the in-app update feature.
- Kotlin nested block comments remain a footgun; the coding rule (never write
  `/*` inside a comment) is recorded in `docs/plan/AI-HANDOFF.md`.

## Implemented (this cycle, 4.6.1) - hygiene and in-app updates

- Docs hygiene: the README, this file, `docs/android.md` and `docs/roadmap.md`
  no longer claim the Android foreground service is missing, no longer pin
  old Android version numbers, and no longer describe tiled rendering as
  pending (it shipped in 4.1.0).
- `keepOperationLog` is enforced where the operation log is written, and the
  log can be filtered and exported as CSV from Settings.
- AI settings discover the configured provider's models live instead of
  showing a static list; the free-text custom entry stays.
- Sync gains a manual "delete from cloud" action for one document, leaving
  the local file untouched.
- In-app updates: the desktop updater verifies signed update manifests from
  the project's GitHub releases and installs in place; Android downloads the
  release APK, checks its SHA-256 and opens the system installer. Without a
  signed manifest/keys the app falls back to opening the release page.

### Not done in this pass (honest)

- The updater's public key and release secrets are configured by the project
  owner; until then the app uses the browser fallback, and the Windows
  installer still warns because the app is not Authenticode-signed.
- Android in-app install needs the user to allow "install unknown apps" for
  OmniOffice on the device.
- macOS/Linux packaging stays out of scope: CI builds and tests those
  platforms, only Windows packaging is produced.

## Implemented (this cycle, 4.6.0) - PDF tools (plan 2, phase 8)

- pdfcore: real `/Annots` annotations (note, highlight, underline, strike,
  ink, text box, image/signature) with listing, move/resize/update/delete;
  keyword and regex detection (`regex` 1.x, size- and count-limited); blank
  page insertion and outline writing; header/footer and Bates stamps as
  incremental revisions; N-up and saddle-stitch booklet imposition; pdf2doc
  simple-table recovery, password threading and list-numbering restart;
  officecore per-list numbering (own numId + startOverride).
- Tauri: seven new commands registered (`pdf_list_annotations`,
  `pdf_annotate_editable`, `pdf_edit_annotations`,
  `detect_redaction_matches`, `pdf_set_outline`, `stamp_pdf`, `nup_pdf`),
  job-tracked with unique retry kinds.
- Frontend: reworked Annotate screen with a dependency-free signature pad,
  Redact keyword/pattern search with preview, Organize blank pages and a
  bookmark editor, PageTools header/footer/Bates and N-up tabs, converter
  password field, platform-aware signing certificate section.
- Tests: workspace Rust suites and the frontend suite green locally
  (frontend 1863; pdfcore, officecore and the app crate include the new
  round-trip, incremental and wire-shape tests).

### Not done in this pass (honest)

- N-up rebuilds the page tree: source links, outlines and named destinations
  are dropped and source annotations are not repainted onto the sheets (also
  declared in the README). Old page objects stay unreferenced.
- Deleting an annotation leaves the orphan object in the file (incremental
  updates cannot remove objects), which is what keeps signed documents
  byte-stable.
- The signature pad is raster (PNG) based; vector or typed signatures are not
  offered. Android `.p12` signing is covered by unit tests and CI builds, not
  by a physical device here.
- Regex/keyword matches follow each page's reading order; matches that span a
  page break are not found.

## Implemented (4.5.0) - Writer and Impress, deeper editing (plan 2, phase 7)

- Writer: table cell merge/split and column-width drag, image wrap options
  (inline, square, top and bottom) with corner size handles, a format
  painter, a ruler with custom tab stops, and a watermark dialog with an
  on-page preview; ODT span/named-style import fixes and the RTF
  single-paragraph fix; only dirty blocks are re-measured during pagination.
- Impress: paragraph/run rich-text editing (bold, italic, underline, color,
  size, bullet levels), align/distribute, glued connectors, image crop,
  footer/date/slide-number settings, hidden slides, a slide sorter grid and
  laser/pen during the show.
- ODP writes real chart documents and keeps runs, footer/slide-number fields,
  hidden slides and connectors.
- PDF export: Writer merged table cells lay out on the grid and `\t` advances
  to custom tab stops; Impress skips hidden slides and draws per-run text
  formatting, master/layout objects and cropped images. Writer watermarks are
  stamped into the PDF as an incremental revision, so signatures survive.
- Tests: golden fixtures and round trips cover merged tables, tab stops,
  image wrap and watermarks, next to the ODP chart/run tests.

### Not done in this pass (honest)

- The slideshow laser trail is a simple dot, not a fading line.
- Writer image wrap is block-level in the PDF export; square and
  top-and-bottom wrap round-trip in DOCX and ODT.
- Center/right/decimal tab stops are approximated in the PDF export by
  advancing to the stop; RTF keeps no merged cells, custom tab stops or
  watermarks.
- ODP still writes a single default master page (declared in the
  compatibility matrix and gated by Data Loss Protection).

## Implemented (this cycle, 4.4.0) - Calc to Excel level (plan 2, phase 6)

- `CalcEditor.tsx` split into `src/office/calc/*` and `src/office/calc/ui/*`
  (4.9k to 3.1k lines); pure modules for find/replace, sort, filter, paste
  special, visibility, conditional formats, notes and links.
- 80 new functions (274 registered), each tested against documented examples.
- officecore: scatter/doughnut charts, color scales, data bars, icon sets,
  formula rules, notes and hyperlinks (http/https/mailto/internal only),
  hidden rows/columns, `#CALC!`, `_xlfn.` prefixes, ODS formula syntax;
  verified both ways with LibreOffice 24.2.
- `open_external_link` command (same allow-list as the importers).
- Tests: frontend 1719, Rust 599 across officecore, app, synccore and aicore.

### Not done in this pass (honest)

- No real Excel was available: XLSX output is checked against LibreOffice and
  the specification, not against Excel itself. Dynamic-array spill metadata is
  not written, so FILTER/SORT/HSTACK open in Excel as ordinary formulas.
- The original size of a hidden row or column is not kept (hidden is size 0).
- Tapping a link on a touch screen does not open it; use the long-press menu.
- Condition filters store only the resulting value list.
- Excel 2010 `x14` conditional-format extras and threaded comments are not
  imported; ODS cannot store stop-if-true or reversed icon sets.
- Carried over: live Drive/OneDrive OAuth, the Claude provider against the live
  API, and the Windows-only 4.3.0 changes (single instance, trimmed Tesseract
  DLL set) are verified by CI only. Please try OCR on the installed build.

## Implemented (4.3.0) - fixes and robustness (plan 2, phase 5)

- PPTX writer: real bullet, per-run/paragraph/deck language, runs used only
  when they match the paragraph text; Impress edits keep unchanged paragraphs.
- Calc: sparse prefix-sum row layout, sticky frozen panes, validation index,
  OFFSET/INDIRECT/CELL/INFO (INDIRECT R1C1 returns `#REF!`).
- Version history keyed by document path; index written before pruning.
- ErrorBoundary around screens and editor tabs; `crash.log` panic hook;
  single-instance launch forwarding on desktop.
- Android: typed intent filters for Office formats (manifest guard test in
  vitest, Kotlin JVM test), foreground service for long jobs.
- Installer: only `tesseract.exe` and its 26 imported DLLs.
- Tests: frontend 1033, officecore 128, app crate 159.

### Not done in this pass (honest)

- The Android foreground service, intent filters and Keystore were compiled
  and unit-tested but not run through Gradle or on a device here; the CI
  Android workflow does that.
- Single-instance, window focus and the trimmed engine fetch (PowerShell) first
  run on Windows CI. The freeze-pane look and header stacking were not checked
  in a real browser.
- Converter shows importer warnings untranslated; charts and pivots float over
  the frozen band; column widths still use the old O(cols) loops.
- Carried over from 4.2.0: live Drive/OneDrive OAuth needs the owner's client
  IDs; the Claude provider was only tested against mock servers.

## Implemented (4.2.0) - AI and keys (plan phase 4)

- `crates/aicore/src/anthropic.rs`: Messages API client (x-api-key,
  anthropic-version 2023-06-01, SSE, models pagination); Gemini model list via
  the OpenAI-compatible listing; per-provider key files; same-origin redirects
  only; truncation reported as an error.
- `ai_edit_text` plus Writer/Calc/Impress AI dialogs with consent per
  document, provider and data category; formula allow-list checks.
- Android Keystore (AES-256-GCM) secret storage through a Tauri mobile plugin;
  the full Android build, Kotlin unit tests and the emulator launch test passed
  on the branch before release.
- An independent review found seven issues (truncated answers accepted, key
  forwarded on redirects, one key for all providers, effort default, consent
  scope, formula safety, minor items); all fixed with tests.
- Tests: aicore 74, app crate 131, frontend 837.

### Not done in this pass (honest)

- Live Google Drive / OneDrive OAuth was not tested: it needs OAuth client IDs
  created by the project owner. Android has no custom-scheme OAuth redirect yet.
- The Claude provider was not exercised against the live API (tests use local
  mock servers); the Keystore was exercised only on the CI emulator.

## Implemented in 4.1.0 - PDF tools (plan phase 3)

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

- The ODT reader resolves character styles on spans and the RTF reader keeps
  formatting groups inside one paragraph; the old "ODT span formatting lost"
  and "RTF paragraph per group" notes no longer apply.
- Comment reply dates and initials are not kept in ODT/RTF; RTF comment ids
  are renumbered.
- ODP charts are written as real embedded chart objects (fixed in 4.5.0);
  PDF export still draws charts as labelled data-range boxes and renders
  image wrap as block-level.
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
- **E2E runs on Linux and Windows in CI.** Linux runs the engine-free smoke
  through tauri-driver (WebKitWebDriver); the Windows `e2e-windows` job runs
  smoke, Reader and deep flows through msedgedriver against the WebView2
  runtime with the native engines fetched.
- **The repair UI is desktop-only** (qpdf is not shipped on Android) and
  encrypted documents must be decrypted before qpdf can rewrite them.

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
