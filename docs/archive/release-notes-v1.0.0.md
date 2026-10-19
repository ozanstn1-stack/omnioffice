# PDF Swiss Army Knife v1.0.0

**All your PDF tools in one place — local-first, offline, private.**
A modern Windows desktop toolkit for everyday PDF work. Nothing is uploaded; every operation runs on your machine.

## Features

- **Merge PDF** — drag to order, optional metadata preservation
- **Split PDF** — page ranges, every N pages, individual pages, at selected pages
- **Organize pages** — thumbnails, pointer drag-to-reorder, rotate, duplicate, delete, extract, undo/redo
- **Extract / delete pages** — selections like `1,3,5-8,12`
- **Compress** — lossless optimizer or "strong" re-render (DPI/quality/grayscale) with real, sample-based size estimates
- **OCR** — offline Tesseract 5: searchable PDF, TXT and Markdown; English, Turkish, German, Dutch, French, Spanish, Italian, Bulgarian; auto-orientation, deskew, contrast, denoise, binarize
- **Watermark** — text or image, six positions, opacity, rotation, tiling
- **Password protect** — AES-256 with user/owner passwords and permissions; **Unlock** with a known password
- **Convert** — PDF → JPG/PNG (72–450 DPI) and images → PDF (page size, fit/cover/actual, margins, rotation)
- **Page tools** — page size/resize (A3…A5/Letter/Legal/custom), crop, page numbering (4 formats, 6 positions)
- **Metadata editor** — title, author, subject, keywords, creator, producer, dates; remove all metadata
- **Add text / images / shapes** — text and image stamps, rectangles, highlights, lines
- **Document info** — pages, size, version, encryption, text-layer detection, image count, page sizes
- **Batch processing** — run one operation over many files with per-file status
- Dark/light/system theme, English + Turkish UI, drag & drop, keyboard shortcuts, cancellation everywhere

## Downloads

| File | Description |
| --- | --- |
| `PDF-Swiss-Army-Knife-Setup-1.0.0.exe` | Windows installer (per-user or all-users, uninstall supported) |
| `PDF-Swiss-Army-Knife-Portable-1.0.0.zip` | Portable build — extract and run, no installation |
| `SHA256SUMS.txt` | SHA-256 checksums for the two files above |

**Requirements:** Windows 10/11 x64. The WebView2 runtime is preinstalled on current Windows versions. All PDF engines (pdfium, qpdf, Tesseract + language data) are bundled; no extra downloads or runtime setup.

## Validation status

- 62 automated tests (`cargo test --workspace`: 13 unit + 49 integration) cover every operation, the IPC wire format, and hostile inputs (empty/corrupt/locked/oversized files, wrong passwords, cancellation, disk errors).
- End-to-end UI validation performed on a real Windows machine: import → organize → merge → split → rotate → compress → OCR → watermark → protect/unlock → PDF→JPG → JPG→PDF → metadata → export, plus crash scenarios.
- Example measured result: a 300-DPI scanned invoice (1.44 MB) compressed to **150.9 KB (−89.8 %)**; OCR produced a searchable PDF with a correct, extractable text layer.

## Known limitations (v1.0.0)

- Merge does not carry over bookmarks/outlines.
- "Strong" compression is lossy (pages become JPEG images); use lossless for text documents.
- OCR searchable PDFs are larger than the input because tesseract embeds page images.
- Crop stores a PDF CropBox (non-destructive); annotations and stamps are flattened into page content.
- Text stamps/watermarks are rasterized (Unicode-safe) rather than embedded as subset fonts.
- No printing from the app; no macOS/Linux builds yet (the core is platform-agnostic).

## Privacy

No cloud upload, no telemetry, no analytics. Passwords are never logged or persisted. Recent files store paths and timestamps only. Temporary files are deleted after each job.

Full details, architecture and build instructions: see the [README](https://github.com/ozanstn1-stack/pdf-swiss-army-knife#readme).
