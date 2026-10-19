# PDF Swiss Army Knife v1.4.0 — Android app

The toolkit now runs on Android as a self-contained APK: **every tool, every
screen, every engine**. Nothing was ported feature by feature — the same
`pdfcore` engine and the same UI ship inside the package.

## Android app

- **Feature parity with Windows.** Merge, split, organize, extract/delete/rotate, page
  size/crop, page numbers, metadata, text/image/annotation stamps, compress
  (lossless and re-render), OCR (searchable PDF, text, Markdown), password
  protect/unlock (AES-256), PDF → images, images → PDF, batch processing,
  reading mode with search, the optional AI assistant and the AI library.
- **Engines inside the APK** — no downloads, no network:
  - **pdfium 155.0.8057** (same Chromium build as the desktop app) for rendering,
    text extraction and full-text search,
  - **Tesseract 4.1** (statically linked CLI, Apache-2.0) for OCR, with the same
    `tessdata_fast` models: English, Turkish, German, Dutch, French, Spanish,
    Italian, Bulgarian + OSD for auto-rotation.
- **Phone-first layout**: drawer navigation with a top bar, safe-area aware,
  tables and tool panels reflow to the screen width. Tablets get the same
  layout with more room.
- **File handling the Android way**:
  - pick documents with the system picker (Storage Access Framework),
  - results are written to `Downloads/PDF Swiss Army Knife/` automatically, or
    to any location you choose with *Save as…*,
  - open results in the system viewer or send them with the share sheet.
- **Same privacy model**: no telemetry, no uploads, passwords never stored or
  logged, AI requests only with your own key and explicit confirmation.

## Build

```powershell
npm run android:engines     # pdfium, Tesseract CLI and the tessdata models
npm run android:build       # arm64-v8a APK  → release-artifacts/
npm run android:build:all   # arm64-v8a + armeabi-v7a
```

The Gradle project (`src-tauri/gen/android`) is part of the repository, and the
build script compiles the Rust library itself, so **Windows Developer Mode is
not required** (the Tauri CLI needs it for its symlink step). A GitHub Actions
workflow builds and attaches both APKs on every `v*` tag.

## Verified on device

| Check | Result |
| --- | --- |
| App starts on Android 16 (API 36 emulator) | Home screen renders, drawer navigation works ✓ |
| Engines | `{"pdfium":true,"tesseract":true,"tesseract_version":"tesseract 4.1.0","ocr_languages":["bul","deu","eng","fra","ita","nld","osd","spa","tur"]}` at runtime ✓ |
| Tesseract binary | `libtesseract.so --version` → `tesseract 4.1.0 / leptonica-1.79.0` from the app's native library directory ✓ |
| Language models | Copied from APK assets to the private files directory on first launch (9 models) ✓ |
| Document import | Picked a PDF with the system picker → imported → 5 pages, page size, text layer and encryption state detected by pdfium ✓ |
| File layout | `lib/<abi>/libpdf_sak_lib.so`, `libpdfium.so`, `libtesseract.so`, `assets/tessdata/*` inside the APK ✓ |

## Notes

- **qpdf is not bundled on Android.** It is only used for desktop diagnostics;
  none of the app's operations shell out to it.
- Minimum Android version is **7.0 (API 24)**; the app targets **Android 16 (API 36)**.
- The desktop build is unchanged and still ships as both installer and portable ZIP.
