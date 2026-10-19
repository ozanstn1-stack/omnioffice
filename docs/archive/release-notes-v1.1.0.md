# PDF Swiss Army Knife v1.1.0

**New in this release: a real reading mode** — plus everything from v1.0.0, still fully local and offline.

## What's new

### Reading mode (new)
- **Continuous scroll** through the whole document. Pages render lazily (only what is near the viewport), so even huge documents stay responsive.
- **Zoom**: fit-to-width and 25–400 % with `Ctrl +` / `Ctrl -` / `Ctrl+0`, crisp re-rendering at the chosen size.
- **Full-text search** over the PDF text layer using pdfium's own matcher: match count and page count, context snippets, click a result to jump to the page, `Ctrl+F` to focus the search box.
- **Copy page text** to the clipboard (works for digital PDFs and for OCR'd searchable PDFs).
- **Page navigation**: jump-to-page field, previous/next buttons, `Home`/`End`, `PgUp`/`PgDn`, current page indicator.
- **Open in default app** button, and the reader is available from the sidebar, the Home screen and History (clicking a recent PDF opens it in the reader).
- Keyboard shortcuts are shown at the bottom of the reading view: `Ctrl+F` search, `Ctrl +` / `Ctrl -` zoom, `Ctrl+0` fit width, `PgUp`/`PgDn`, `Home`/`End`.

### Also in this release
- English and Turkish dictionary entries for the reader, including a hint when a document has no text layer (use the OCR tool to make it searchable).
- Version is now shown from a single source (`package.json`).

## Everything from v1.0.0

- **Merge**, **Split** (ranges / every N / individual / at pages), **Organize pages** (drag-reorder, rotate, duplicate, delete, extract, undo/redo), **Extract/Delete** with `1,3,5-8` selections
- **Compress**: lossless optimizer or "strong" re-render with real sample-based size estimates
- **OCR** (offline Tesseract 5): searchable PDF, TXT, Markdown; 8 bundled languages; auto-orientation, deskew, contrast, denoise, binarize
- **Watermark** (text/image, opacity, rotation, tiling), **Add text/images/shapes**
- **Password protect** (AES-256 with permissions) and **Unlock** with a known password
- **Convert**: PDF → JPG/PNG (72–450 DPI) and images → PDF (page size, fit/cover/actual, margins, rotation)
- **Page tools**: page size/resize, crop, page numbering
- **Metadata editor**, **Document info**, **Batch processing**, **History**, **Settings** (dark/light/system theme, English/Turkish)

## Downloads

| File | Description |
| --- | --- |
| `PDF-Swiss-Army-Knife-Setup-1.1.0.exe` | Windows installer (per-user or all-users, uninstall supported) |
| `PDF-Swiss-Army-Knife-Portable-1.1.0.zip` | Portable build — extract and run, no installation |
| `SHA256SUMS.txt` | SHA-256 checksums for the two files above |

**Requirements:** Windows 10/11 x64. All PDF engines (pdfium, qpdf, Tesseract + language data) are bundled — no extra downloads.

## Validation status

- **63 automated tests** (`cargo test --workspace`: 13 unit + 50 integration) cover every operation, reading-mode search (case sensitivity, result caps, cancellation), the IPC wire format and hostile inputs (empty/corrupt/locked/oversized files, wrong passwords, cancellation).
- Reading mode verified end-to-end on a real Windows machine: continuous rendering at fit-width, search for a phrase returning "1 match on 1 page" and jumping to page 3, page navigation and zoom controls.
- Installer and portable build re-verified for this version (install → run → uninstall, portable extract → run).

## Privacy

No cloud upload, no telemetry, no analytics. Passwords are never logged or persisted. Recent files store paths and timestamps only. Temporary files are deleted after each job.

Full details: see the [README](https://github.com/ozanstn1-stack/pdf-swiss-army-knife#readme).

## Upgrade note

Install `PDF-Swiss-Army-Knife-Setup-1.1.0.exe` over an existing 1.0.0 installation — settings, recent files and preferences are kept.
