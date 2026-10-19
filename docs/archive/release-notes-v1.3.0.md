# PDF Swiss Army Knife v1.3.0 — saved AI results and an operation log

Everything you produce is now kept on disk: AI answers become Markdown files in a library you can browse, and every finished operation is recorded in a log.

## New: AI library

- **Automatic saving.** Each finished summary, translation, Q&A answer, OCR-text repair or metadata suggestion is written as a **Markdown file** and added to the library — nothing is lost when you leave the screen.
- **Folder of your choice.** Default is `Documents\PDF Swiss Army Knife AI`; change it in Settings (or in the library screen). The default documents folder is resolved by the app, so OneDrive-redirected folders work too.
- **Library screen** (sidebar → AI Library): filter by result type, search inside saved previews, read the full text, **Save as…** a copy, open the Markdown file, reveal it in Explorer, delete single entries or clear the library.
- **Settings additions:** *Save AI results to the library automatically* (default on), *AI library folder*, and *Keep an operation log*.
- Files are timestamped and slugified (`2026-09-22_202448_summary_sample-1.md`) so they sort chronologically and stay safe on any filesystem.

## New: operation log

- Every successful tool run is recorded in `operations.json` (app config folder): operation name, input and output paths, page count, **input → output size**, timestamp and status.
- **History screen** now has two tabs: *Files* (the existing recent-files list) and *Operations* (the new log with a "Clear log" action).
- Paths and sizes only — never document content, never text, never passwords.

## Verified end-to-end

| Check | Result |
| --- | --- |
| AI summary with auto-save | Markdown file written (`..._summary_sample-1.md`, 386 bytes) and index entry created with model, pages, characters, options ✓ |
| Operation log | A real compression run was recorded with input/output paths and sizes ✓ |
| Library UI | Saved entry listed with type badge, timestamp, model, preview and metadata; folder shown and changeable ✓ |
| Rust store tests | 4 new unit tests: save/list/delete/clear round-trip, vanished files dropped from the index, newest-first operation log, timestamped/slugified file names ✓ |

`cargo test --workspace` → **84 tests passing** (4 new). Installer and portable packages built for **1.3.0**; the payload version was verified inside the installer.

## Upgrade

Install `PDF-Swiss-Army-Knife-Setup-1.3.0.exe` over your existing copy — settings, recent files, the operation log and the DPAPI-encrypted API key are preserved. Existing AI results saved manually as files are untouched; new ones land in the library folder.
