# Release Readiness — Office Swiss Army Knife 3.5.3

This file states what is actually implemented, tested and benchmarked, and
what is not. It is deliberately conservative: nothing is claimed as released,
built or verified unless it was reproduced in this environment or is produced
by CI.

## Implemented (this cycle)

- **Retryable jobs** (`src/lib/job-retries.ts`, `src/lib/api.ts`): tracked
  wrappers persist `kind` + the exact invoke args (credentials blanked) and one
  handler per kind re-invokes the command. Covers merge, split, organize,
  extract/delete/rotate, compress, OCR, protect/unlock, PDF↔images, resize,
  crop, metadata, page numbers, watermark, annotate, redact, compare, PDF
  Studio sanitize/flatten/PDF-A, the five AI actions and the vault scan.
- **Vault Clear UI** (`src/screens/Vault.tsx`, `src-tauri/src/vault.rs`):
  confirmation dialog, optional deletion of the app-private imported copies
  (`vault_clear(delete_imports)`).
- **Writer fields + list numbering** (`src/office/writer/writerDom.ts`,
  `WriterEditor.tsx`): PAGE/NUMPAGES/DATE/TIME/TITLE/AUTHOR from the live
  pagination/metadata, ordered lists numbered across the document.
- **TOCTOU-safe unique names** (`crates/pdfcore/src/docutil.rs`): candidates
  are reserved with `create_new`.
- **Dependency updates**: base64 0.23.1, Rust/frontend/actions Dependabot
  groups, Chrome extension TypeScript.

## Tested in this environment

| Gate | Command | Result |
|---|---|---|
| Frontend unit/integration | `npm test` | 38 files, 673 passed, 1 skipped |
| Frontend coverage | `npm run test:coverage` | 64.49 % statements, 71.29 % branches, 48.50 % functions (floors 62/69/46/62 pass) |
| TypeScript | `npx tsc --noEmit` | clean |
| ESLint | `npm run lint` | 0 errors, 0 warnings |
| Rust workspace | `cargo test --workspace` | 533 passed, 3 ignored |
| Rust (new) | `cargo test -p pdf-swiss-army-knife vault::tests::clear` | passed |
| Rust (new) | `cargo test -p pdfcore --lib docutil` | passed (reservation + policy contract) |

New tests: 5 job-retry tests (tracking, sanitizing, re-running, unavailable
after uninstall), 5 screen tests (Organize, Annotate, Batch, Page Tools,
Plugins), Writer list/field tests, writerDom unit tests, vault clear (desktop
+ Android) and the `UniqueName` reservation tests.

## Benchmarked

- Existing cargo/frontend perf guards run in CI; no new benchmark was added in
  this pass.

## Not done in this pass (honest)

- **XLSX cross-sheet comments (audit C11)** stay on the follow-up list: the
  writer still merges every sheet's comments into one part.
- Desktop E2E (tauri-driver) and fuzzing/benchmark trends (Phase 2 leftovers)
  are still not started.
- A retried job for an encrypted document needs the password re-entered (the
  payload intentionally does not store it) and a retry whose output now exists
  fails with `output_exists`; there is no overwrite prompt in the Jobs screen.
- `UniqueName` reservations are zero-byte placeholders; an operation that fails
  after resolving leaves one behind.

## Platform support and artifacts

- **Windows**: source builds and tests locally; installer/portable ZIP via
  `npm run release:local` or CI, with SBOMs, checksums and provenance.
- **Android**: built by CI and by `npm run release:local`; artifacts are named
  `PDF-Swiss-Army-Knife-Android-3.5.3-<abi>.apk`.
- **Linux/macOS desktop**: built and tested by CI.
- **Chrome extension**: dependency bump only; CI runs its self test.

## Security status

Unchanged from 3.5.2, plus: job retry payloads are sanitized (password/token/
key fields blanked) before they are written to `jobs.json`; the vault clear
dialog keeps the folder selection and only deletes imported copies when the
user ticks the Android checkbox.

## Known limitations

See the README "Known limitations" section (single source of truth).
