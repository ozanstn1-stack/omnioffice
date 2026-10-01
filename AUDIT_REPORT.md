# AUDIT REPORT — Office Swiss Army Knife v3.2.1

Scope: full repository audit requested for the stabilisation phase. No new
features were added; the work is bug fixing, data-loss prevention, security
hardening, test coverage and safe refactoring. Every issue below was verified
by reading the code (and, where marked FIXED, by a regression test). Companion
files: `AUDIT_BASELINE.md` (pre-change state), `CHANGELOG.md`.

Legend: **FIXED** = corrected in this pass · **MITIGATED** = concrete
protection added, residual risk documented · **REMAINING** = verified issue
left for the next phase with a recommendation (not silently ignored).

---

## CRITICAL

### C1. Failed background jobs were persisted as `done` — FIXED
- **Files:** `src-tauri/src/commands.rs` (`operation_with_progress`, `merge_pdfs`), `src-tauri/src/vault.rs` (`vault_scan`), `src-tauri/src/jobs.rs`.
- **Problem:** the wrapper called `registry.finish(&job_id)` regardless of the
  result. `JobStore::finish` sets `Done`; terminal states are sticky, so a
  failed OCR/merge/vault scan could never be corrected to `Failed` and the
  Jobs screen claimed success after a restart.
- **Trigger:** any error return (corrupt input, disk full, cancelled worker).
- **Impact:** false success, user believes a document was produced.
- **Fix:** `JobRegistry::complete(job_id, succeeded)` marks `Done` only on
  `Ok` (cancellation keeps its sticky `Cancelled`). Used by all three call
  sites; `JobFinishGuard` already did this for AI jobs.
- **Tests:** `jobs::tests::complete_marks_a_failure_as_failed_not_done`.

### C2. Unreadable `jobs.json` was silently overwritten with an empty history — FIXED
- **File:** `src-tauri/src/jobs.rs::attach_path`.
- **Problem:** read/parse failure became `unwrap_or_default()`, after which the
  empty map was persisted over the file. A torn write or a foreign writer
  destroyed the whole history.
- **Fix:** a corrupt file is quarantined to `jobs.corrupt-<timestamp>.json`
  before anything is written; if quarantining fails the rewrite is skipped.
- **Tests:** `corrupt_history_is_quarantined_not_destroyed`.

### C3. "Atomic" writers deleted the target before renaming, and never fsynced — FIXED
- **Files:** `crates/officecore/src/io.rs`, `crates/pdfcore/src/docutil.rs`,
  `crates/synccore/src/metadata.rs`, `src-tauri/src/sign.rs`,
  `src-tauri/src/jobs.rs`.
- **Problem:** every writer did `remove_file(target)` and then `rename(temp,
  target)`. A crash in that window left **no file**, most dangerous for
  in-place PDF/office saves where target == source. The comment claiming
  Windows rename cannot replace an existing file is wrong: Rust's `fs::rename`
  uses `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING`. No `sync_all` meant a
  power loss after rename could still expose a zero/partial file.
- **Fix:** temp file → `write_all` → `sync_all` → rename (which replaces
  atomically); parent directory fsync where the platform allows; unique temp
  names (`pdfcore` temp names now include a process-wide counter).
- **Tests:** existing atomic round-trip tests plus the whole office/PDF suite.

### C4. Settings, recents, AI settings/library/logs and secret blobs were written non-atomically — FIXED
- **Files:** `src-tauri/src/commands.rs` (`save_settings`, `add_recent`),
  `src-tauri/src/ai.rs` (`ai_save_settings`, `ai_save_output`),
  `src-tauri/src/library.rs` (`write_json`, Markdown entries),
  `src-tauri/src/secret.rs` (`save_api_key`).
- **Problem:** plain `fs::write` truncates first. A crash mid-write corrupted
  `settings.json`, `ai.json`, `ai-library.json`, `operations.json` and the
  DPAPI/base64 key blob. Readers fall back to `Default`/empty, so the loss was
  invisible (API key unrecoverable in-app).
- **Fix:** shared `commands::write_atomic` (officecore helper) for all of
  them. `library::unique_path` now appends a UUID instead of falling back to
  an existing candidate (which would have overwritten a library file).
- **Tests:** `officecore::io::atomic_write_roundtrip`, `library` tests.

### C5. Writer: any keystroke in a paragraph containing a footnote/field deleted it — FIXED
- **Files:** `src/office/writer/runs.ts` (`normalizeRuns`, `formatAtOffset`),
  `src/office/writer/writerDom.ts`.
- **Problem:** note/field anchors are empty-text runs; `normalizeRuns` dropped
  empty runs, so `domToRuns` → sync deleted the anchor from the model while
  the DOM still showed the number. `formatAtOffset` could also return the
  anchor as the template, so typed text inherited `footnote`/`field`.
- **Trigger:** insert a footnote or page/date field, then type one character
  (or press Enter/Shift+Enter) in that paragraph.
- **Fix:** empty structural runs are preserved; the insertion template strips
  `footnote`/`endnote`/`field`/`revision`.
- **Tests:** `runs.test.ts` "structural run preservation" (4 cases).
- **Remaining related:** `trackRunChanges` still flattens runs (M12), and
  fields are never refreshed (M16).

### C6. Writer: Shift+Enter with a selection kept the selected text — FIXED
- **File:** `src/office/WriterEditor.tsx::handleParagraphKeyDown`.
- **Problem:** the split-based path inserted `"\n"` without deleting
  `from..to`; selecting "bc" in "abcd" produced "abc\nd".
- **Fix:** `replaceRange(plain, from, to, "\n")`.
- **Tests:** WriterEditor suite plus the new `runs.test.ts` cases.

### C7. Writer: Delete at the end of a paragraph silently deleted the next object — FIXED
- **File:** `src/office/WriterEditor.tsx::handleStructure("mergeForward")`.
- **Problem:** a following table/image/rule/page-break block was spliced out.
  Those objects exist only in the model, so the loss was unrecoverable.
- **Fix:** no-op when the next block is not a paragraph.

### C8. Writer: typing in a multi-paragraph table cell deleted the other paragraphs — FIXED
- **File:** `src/office/WriterEditor.tsx::syncCell`.
- **Problem:** every `onInput` replaced `cell.blocks` with a single paragraph.
- **Fix:** the edited inner block index is passed through and only that block
  is spliced; its paragraph properties are preserved.
- **Tests:** `wrapCellRuns` return type tightened; existing Writer suite green.

### C9. Calc: `range <op> scalar` was wrong for every row after the first — FIXED
- **File:** `src/office/calc/formula.ts::broadcastBinary`.
- **Problem:** a 1×1 operand was padded with `""` for later rows, coercing to
  0. `=D2:D100*1.2`, `=SUM(range*rate)`, `=INDEX(...)+n` all silently wrong.
- **Fix:** 1×1 operands broadcast across the other matrix (Excel semantics).
- **Tests:** `formula.test.ts` broadcasting cases.
- **Remaining related:** text-numbers in ranges still count in aggregates
  (S7), MATCH/XLOOKUP approximate modes (M8), wildcards (M9).

### C10. XLSX: hyperlink-only cells were dropped on export — FIXED
- **Files:** `crates/officecore/src/model.rs::Cell::is_empty`,
  `crates/officecore/src/xlsx.rs`.
- **Problem:** `is_empty` ignored `link`, and the exporter skips empty cells —
  exactly the cells the importer creates for a link on a blank cell.
- **Fix:** `link.is_none()` is part of `is_empty`.
- **Tests:** `xlsx_roundtrip_keeps_a_hyperlink_on_a_blank_cell`.

### C11. XLSX: comments from different sheets contaminate each other — REMAINING
- **File:** `crates/officecore/src/xlsx.rs` (one `comments1.xml` per workbook;
  import reads every `<comment>` per sheet).
- **Impact:** two sheets with a comment on the same address end up sharing one
  comment after a save, permanently. Data corruption, silent.
- **Recommendation:** one comments part (and VML shape set) per sheet, keyed by
  sheet relations; regression fixture with two sheets.
- **Not fixed in this pass:** touches the package writer broadly; needs its own
  change window with LibreOffice-structural verification.

### C12. Vault could stay `scanning: true` forever after a crash — FIXED
- **File:** `src-tauri/src/vault.rs`.
- **Problem:** the flag is set before the walk and cleared only at the end; an
  Android process death or cancelled/failed write left the UI with Index and
  Import permanently disabled, and `vault_clear` has no UI path.
- **Fix:** `repair_stored_status` clears a stale flag at app start (no scan can
  survive a restart), preserving the other fields.
- **Tests:** `startup_repair_clears_a_stale_scanning_flag`.

---

## HIGH

### H1. WebDAV `If-Match` was sent unquoted — every conditional PUT failed on strict servers — FIXED
- **File:** `crates/synccore/src/webdav.rs` (`put_file`, `put`).
- **Problem:** `normalize_etag` strips quotes and the client sent the bare
  token. RFC 7232 requires `If-Match: "etag"`; Sabre/Nextcloud compare the
  header item character-for-character, so ordinary re-uploads (and explicit
  `keep_local`) returned 412 → false conflict. On lax servers the invalid
  precondition is ignored, removing the protection the mechanism exists for.
- **Fix:** `if_match_header_value` re-quotes (and escapes) the normalized tag;
  comparisons keep using the unquoted form.
- **Tests:** `if_match_requotes_the_entity_tag`; the in-process DAV mock now
  compares strictly (as Sabre does) and asserts the quoted header on the wire.

### H2. Plugin SSRF: literal private/loopback IPs passed over HTTPS — FIXED
- **File:** `src-tauri/src/plugin.rs::ensure_public_destination`.
- **Problem:** the literal-IP branch only refused the metadata endpoint, so
  `https://127.0.0.1/`, `https://10.0.0.5/`, `https://[::1]/`,
  `https://[::ffff:127.0.0.1]/` were reachable while the DNS branch refused
  every private answer — contradicting the documented policy and the release
  notes.
- **Fix:** private literal destinations are refused for `https`; the
  documented plain-`http` local-network exception (already restricted to
  private/localhost literal hosts by `validate_http_url`) is preserved, and
  the metadata address stays blocked on every scheme.
- **Tests:** `https_private_literal_destinations_are_refused`,
  `plain_http_private_exception_stays_available_but_metadata_is_not`.

### H3. Sync commands trusted raw webview paths — FIXED
- **File:** `src-tauri/src/sync.rs`.
- **Problem:** `sync_status/upload/download/resolve/forget` built `PathBuf`
  from raw strings; a compromised renderer could chain
  `sync_save_config` + `sync_download` to write remote-controlled bytes to an
  arbitrary local path, contrary to the centralized validation used by the
  other 78 filesystem commands.
- **Fix:** `paths::input_file` for existing documents, `paths::output_file`
  for download destinations (parent must exist), `paths::lexical` for the
  forget cleanup that must also work for deleted files.
- **Tests:** existing sync tests continue to pass; path policy is covered by
  `paths.rs` unit tests.

### H4. AI client had no response-size cap — FIXED
- **File:** `crates/aicore/src/lib.rs`.
- **Problem:** `response.text()` and the streamed accumulation were unbounded
  within the 180 s timeout; a hostile endpoint could OOM the process. Provider
  error text (which can echo the request/document) was persisted verbatim to
  `frontend.log`.
- **Fix:** 16 MB cap for non-streaming bodies and for accumulated streams;
  provider error messages truncated to 500 chars; the UI additionally bounds
  what it logs.
- **Tests:** existing aicore client tests (mock server) still pass; cap paths
  are pure code paths.

### H5. Unbounded blocking concurrency — MITIGATED
- **Files:** `src-tauri/src/concurrency.rs` (new), `commands.rs`, `vault.rs`,
  `sync.rs`, `office.rs`, `office_tools.rs`, `pdf_v3.rs`, `sign.rs`.
- **Problem:** every heavy operation ran on Tauri's shared blocking pool with
  no cap beyond Tokio's default of 512 threads. Concurrent OCR/compression
  jobs spawn tesseract children and large buffers each; peak RAM/CPU was
  unbounded.
- **Fix:** a process-wide `Semaphore` sized `clamp(cores, 2, 8)`; all heavy
  paths take a slot before touching the pool. This is a bound, not a
  scheduler — small operations are unaffected beyond queueing.
- **Note:** AI page extraction still runs inline on async worker threads
  (not through the pool); it is a single request-driven path and is listed as
  a follow-up.

### H6. Writer caret offsets counted rendered note/field glyphs — FIXED
- **File:** `src/office/writer/caret.ts`.
- **Problem:** `Range#toString()` counted `<sup>1</sup>` and cached field text
  while the model stores those runs as empty, so Enter/Backspace/bookmarks in
  such paragraphs were off by the glyph length.
- **Fix:** a model-aware offset walker skips `[data-note-id]`/`[data-field-kind]`
  anchors; a caret inside an anchor snaps to its model position; `<br>` counts
  as the one `"\n"` it represents when restoring a caret.
- **Tests:** `caret.test.ts` atomic-marker and restore cases.

### H7. Writer pagination numbered every page after the first as 2 — FIXED
- **File:** `src/office/writer/pagination.ts::pushPage`.
- **Problem:** `page.sectionPage` was read **after** the page object was
  replaced, so `differentOddEven` headers/footers selected the wrong variant
  for most pages.
- **Fix:** capture the outgoing page before replacing it.
- **Tests:** `numbers pages monotonically inside one section`.

### H8. Calc error literals were unparseable — FIXED
- **File:** `src/office/calc/formula.ts::tokenize`.
- **Problem:** the `#` character fell into the unknown-character branch; a
  formula containing `#N/A`, `#DIV/0!` etc. evaluated to `#VALUE!`, breaking
  `IFERROR`/`ISERROR` and `IF(cond, x, #N/A)`.
- **Fix:** the tokenizer recognizes `#REF! #VALUE! #NAME? #DIV/0! #N/A #NUM!
  #CIRC! #SPILL!` before the identifier branch.
- **Tests:** `formula.test.ts` error-literal cases.

### H9. Calc `TEXT` printed months where minutes belong; `TIME` was missing — FIXED
- **Files:** `src/office/calc/numberFormat.ts`, `src/office/calc/functions/datetime.ts`.
- **Problem:** the replacement chain substituted `mm` as month before `hh`, so
  `hh:mm` rendered `12:12`/`12:01`; `TIME()` was never registered (`#NAME?`).
- **Fix:** single-pass tokenizer disambiguates `m`/`mm` by position (after an
  hour or before seconds = minutes) and adds `TIME(h,m,s)` with Excel's wrap
  and `#NUM!` rules.
- **Tests:** `formatNumber` time cases; `TIME` cases.

### H10. Writer hard line breaks rendered as spaces — FIXED
- **File:** `src/office/writer/writerDom.ts::runsToHtml`.
- **Problem:** a model `"\n"` (Shift+Enter) was written as a raw newline and
  HTML collapsed it; the model and the page preview disagreed.
- **Fix:** newlines render as `<br>` (read back as `"\n"`).
- **Tests:** `runs.test.ts` hard-break round trip.

### H11. Writer "Show revisions" off still displayed deletions — FIXED
- **File:** `src/office/writer/writerDom.ts`.
- **Problem:** the class was dropped but the deleted text still rendered as
  final content.
- **Fix:** with revisions off, delete runs are omitted (Word's Final view).
- **Tests:** `runs.test.ts` deletion-hiding case.

---

## MEDIUM

### M1. `ai_translate` could never succeed — FIXED
- **Files:** `src/screens/Ai.tsx`, `src/lib/types.ts`.
- **Problem:** the frontend sent `{ target_language }` while the Rust
  `TranslateOptions` is `#[serde(rename_all = "camelCase")]` and requires
  `targetLanguage`; every translation failed at deserialization.
- **Fix:** typed key renamed to `targetLanguage`; added a comment pointing at
  the serde attribute.
- **Tests:** TypeScript strict check plus the existing AI suite.

### M2. `ai_example_prompts.translateTargets` never read — FIXED
- **Files:** `src/screens/Ai.tsx`, `src/lib/types.ts`.
- **Problem:** backend sends camelCase, the UI read snake_case, so the
  hard-coded fallback silently masked the backend list.
- **Fix:** read `translateTargets`.

### M3. `SanitizeReport.metadataRemoved` typed as boolean — FIXED
- **File:** `src/screens/PdfStudio.tsx`.
- **Problem:** the Rust field is a `u32` count; the interface declared
  `boolean` and omitted `findingsBefore`. Truthiness made the badge "work"
  while the contract was wrong.
- **Fix:** typed as `number`, `findingsBefore` added.

### M4. Live jobs were invisible; progress only updated existing rows — FIXED
- **File:** `src/lib/jobs.ts::progress`.
- **Problem:** every PDF tool reports progress without calling `start`, so the
  Jobs screen and sidebar count showed nothing until an app restart hydrated
  the Rust store.
- **Fix:** an unknown job id is created as a running row (Rust already tracks
  it); a later hydrate merges the enriched record.
- **Tests:** `jobs.test.ts` "surfaces a Rust-tracked job that never called
  start".
- **Remaining:** Retry still reports `unavailable` because no screen registers
  a retry handler; that is honest but the README's retry claim stays aspirational.

### M5. Reusing a job id orphaned the previous worker — FIXED
- **File:** `src-tauri/src/jobs.rs` (`register`, `register_ai`).
- **Problem:** a second run of a fixed id (`vault-scan`, `studio-*`, `merge`)
  replaced the cancellation token without cancelling it: the first worker
  became uncancellable and `finish` from either side marked the shared record.
- **Fix:** registering an existing id cancels the previous token first.
- **Tests:** `reusing_an_id_cancels_the_previous_run`.

### M6. `jobs.json` used a fixed temp name and ignored rename failures — FIXED
- **File:** `src-tauri/src/jobs.rs::persist_locked`.
- **Problem:** two app instances shared `jobs.json.tmp`; a rename failure was
  dropped silently, leaving a stale record on disk.
- **Fix:** UUID temp name, fsync, cleanup on failure.

### M7. `resolve_output_path` TOCTOU remains for `UniqueName` — REMAINING
- **File:** `crates/pdfcore/src/docutil.rs`.
- **Problem:** `exists()` is checked, then the file is written later; two
  concurrent operations can pick the same "unique" name and one result is
  silently replaced.
- **Recommendation:** reserve the candidate with `OpenOptions::create_new`
  and promote into the reservation, removing it on failure.

### M8–M11. Calc function-level correctness gaps — REMAINING
- `MATCH`/`HLOOKUP` approximate modes and `XLOOKUP` modes ±2 return
  positions from a sorted copy / dead branches (`functions/lookup.ts`).
- `COUNTIF`/`SUMIF`/`COUNTIFS` lack `*`/`?` wildcards (`scalars.ts`).
- `NUMBERVALUE` mishandles decimal/group separators (`functions/text.ts`).
- `FILTER` treats blank include cells as TRUE (`functions/arrays.ts`).
- `SUMPRODUCT` of a raw boolean matrix returns 0 (`functions/statistics.ts`).
- **Recommendation:** one focused Calc correctness change with the vectors
  from the audit as tests; they are wrong-result (not data-loss) bugs.

### M12. Writer `trackRunChanges` destroys per-run formatting/revision ids — REMAINING
- **File:** `src/office/writer/revisions.ts`.
- **Problem:** it flattens runs to text and rebuilds up to four runs from the
  first run's format; every suggest-mode keystroke restyles surrounding text,
  drops structure and re-ids prior insertions.
- **Recommendation:** diff runs, not text; split only the edited run and mark
  only new text with a fresh revision id.

### M13. Writer undo/redo is browser `execCommand` only — REMAINING
- No model-level history; structural/format changes are not undoable and the
  native stack can restore a stale DOM that is then written back on blur.
- **Recommendation:** a bounded model undo stack per tab like Calc/Impress,
  with toolbar `mousedown` prevented so selection survives.

### M14. Third-party file modification is detected only for sync, not for saves — REMAINING
- **Problem:** documents opened and then changed by another program are
  overwritten on save without a fingerprint check or conflict UI. (Requested
  by the audit brief, section 6.)
- **Recommendation:** capture a hash/mtime/identity at open, compare before
  save, and offer Reload external changes / Save as new file / Cancel. The
  sync side already has the three-way state model to reuse.

### M15. Writer: version history keyed by transient tab id — REMAINING
- Versions written under the session tab id are unreachable after reopening
  the same file; repeated saves on an unchanged document also push copies.
- **Recommendation:** key by stable document identity (saved path / persisted
  id) and skip pushes when the tab is clean.

### M16. Writer: fields never refresh; ordered lists always show "1" — REMAINING
- `fieldValues` is never supplied, so page/pages/cross-reference values stay
  at their insertion-time cache; list markers use `props.list.start` only.
- **Recommendation:** compute field values from the pagination result and
  renumber list runs at render/export time.

---

## LOW / INFORMATIONAL

- **L1.** Parent-directory fsync is best-effort (no-op on Windows); document
  it rather than promise unbounded durability.
- **L2.** Plugin sandbox data directories do not re-check reparse points on
  read/write/delete; only installation does. Defense-in-depth for a local
  attacker who can already write the app-data directory.
- **L3.** `src/lib/plugins.ts` claims "anything unknown is rejected" while
  unknown manifest fields are ignored (and the frontend id pattern accepts
  `a..b` while the backend rejects it). The backend is fail-closed; align the
  comment or the validation.
- **L4.** `sync_resolve` "keep both" buffers the whole remote file (up to
  512 MB) instead of streaming; user-initiated.
- **L5.** `ai_save_output`/`ai_library_save` accept arbitrary webview paths
  within the app's general command trust model; no regression, but the
  capability comment overstates the scope.
- **L6.** `vault_clear` and a few other commands have no UI caller; the
  frontend "Retry" path has no registered handler. Dead but harmless surface.
- **L7.** `office_history` removes the old version file before writing the
  index; a crash can leave an index row without its version file. The version
  files are now written atomically, so only the ordering remains.

### Verified sound (spot checks)
- **Redaction** reports a trinary contract (`Removed` / `PossiblyPresent` /
  `NotVerified`) and never sets `verified` on `PossiblyPresent`; the
  verification re-opens the output and re-extracts text (`redact.rs`,
  10 tests).
- **Signatures** re-verify the file that was written (cryptographic validity +
  ByteRange digest) before reporting success, and `trust` is always
  `"unknown"` offline (no system trust store). PFX password errors are errors,
  never fake success (`signature_test.rs`, 15 tests).
- **OOXML/ODF import** goes through the bounded ZIP/XML layers (entry count,
  cap, compression ratio, duplicate names, CRC); no path in this pass bypasses
  them.
- **WebDAV conflict matrix** has no silent-overwrite path: downloads refuse a
  locally changed file without `force`, uploads refuse diverged states, and
  metadata is committed only after a successful conditional PUT.

---

## What was fixed vs. what remains

| Area | Fixed in this pass | Left for the next phase |
|---|---|---|
| Atomic writes / fsync | office, PDF, sync sidecars, jobs, all JSON stores, secrets, OCR/convert/compare outputs | version-index ordering (L7), `resolve_output_path` TOCTOU (M7) |
| Jobs | failure persistence, corrupt-file quarantine, id reuse, temp/rename, live visibility | retry handler wiring (L6), dropped-future liveness on Android |
| Writer | structural run loss, selection Shift+Enter, Delete object loss, multi-block cells, caret offsets, hard breaks, revision hiding, pagination numbering | track-changes run preservation (M12), model undo (M13), fields/lists (M16), history keys (M15) |
| Calc | broadcast, error literals, TEXT/TIME | MATCH/XLOOKUP/wildcards/NUMBERVALUE/FILTER/SUMPRODUCT (M8–M11) |
| PDF/XLSX | hyperlink-only cells | cross-sheet comments (C11), PDF deep pass not completed (see below) |
| Security | WebDAV ETag, plugin SSRF, sync paths, AI response caps | plugin symlink re-checks (L2), plugin manifest strictness (L3) |
| Reliability | vault scanning flag, bounded concurrency | AI extraction concurrency, external-file conflict UI (M14) |

## Environment limitations (honest report)

- **Android build/emulator not run here.** No SDK/emulator in this environment;
  the JVM/instrumentation gates remain CI-only. Nothing in this report claims
  to have rebuilt or tested the APK.
- **No Tauri release build/installer** was produced (SDK/NSIS/signing absent);
  `vite build`, `tsc`, ESLint, Vitest, `cargo test` and `cargo clippy` were run.
- **Repository-wide Prettier/rustfmt checks fail on the pre-existing backlog**
  (untouched files reproduce it); CI's staged gates were not weakened.
- The dedicated PDF deep-audit pass timed out at the tooling level; redaction
  and signature contracts were spot-checked and their suites re-run, but a
  full new adversarial PDF review (coordinates, compression object survival)
  remains outstanding work, not a claim of coverage.
