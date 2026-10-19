# PDF Swiss Army Knife — Chrome extension v1.0.0

The browser build of PDF Swiss Army Knife: the same offline, local-first philosophy, running entirely inside Chrome. **No host permissions, no network requests, no uploads** — the extension literally cannot talk to a server.

## What's inside

| Tool | Notes |
| --- | --- |
| **Reading mode** | Continuous scroll, fit-width and 25–400 % zoom, full-text search with context snippets, click-to-jump, copy page text |
| **Merge** | Ordered multi-file merge, metadata from the first document |
| **Organize pages** | Thumbnails, click/Ctrl/Shift selection, move, rotate 90/180/270°, duplicate, delete, undo |
| **Split** | Every N pages · page ranges · individual pages · at selected pages |
| **Page tools** | Extract, delete, rotate with selections like `1,3,5-8` |
| **Compress** | Lossless optimizer or strong re-render (DPI + JPEG quality) with a measured size estimate |
| **Watermark** | Unicode text (bundled PT Sans, SIL OFL), six positions, rotation, opacity, tiling |
| **Page numbers** | `1`, `Page 1`, `1 / 20`, `Page 1 of 20` · six positions · start value · colour |
| **Metadata** | Edit or clear title, author, subject, keywords, creator, producer |
| **Convert** | PDF → PNG/JPG (72–300 DPI, page selection) and PNG/JPG/WEBP → PDF (page size, fit/cover/actual, margins, rotation) |

Originals are never modified — every operation produces a new download.

## Not included (desktop app only)

OCR, password protect/unlock, page size/crop, annotations, batch processing. The browser build cannot decrypt PDFs, so encrypted files are refused with a clear message instead of producing a broken copy.

## Install (unpacked)

1. Download **PDF-Swiss-Army-Knife-Chrome-Extension-1.0.0.zip** below and extract it to a folder.
2. Open `chrome://extensions` in Chrome and enable **Developer mode** (top right).
3. Click **Load unpacked** and select the extracted folder.
4. Pin the extension and click its icon — the toolkit opens in a tab. You can also drag & drop PDFs straight onto the page.

Works in any Chromium-based browser (Chrome, Edge, Brave, …) with Manifest V3 support.

> Note: Chrome 137+ removed the `--load-extension` command-line flag, so unpacked extensions can only be loaded through `chrome://extensions` — that is why the installation step above is manual. Everything else in this release is verified automatically (see below).

## Verification

- **10 Node integration tests** (`npm test`) over the shared operation modules: merge order, extract/delete/reorder/rotate, split modes, metadata, numbering text, Unicode watermark text, images → PDF, lossless compression, encryption refusal.
- **11 in-browser checks** executed in real Chrome (`npm run selftest:browser`): parsing, canvas rendering with ink detection, text extraction, text search, merge, rotation/extraction, page numbering labels, watermark font embedding, compression, canvas → PDF, encrypted input refusal.
- **Package verification** (`npm run verify`): MV3 manifest shape, no host permissions, all referenced assets present (worker, fonts, cmaps, standard fonts), and **zero remote references** in the bundle.

```
11/11 checks passed · SELFTEST_RESULT=OK
All checks passed: MV3 manifest, no host permissions, no remote code, all assets bundled.
```

## Privacy

The manifest declares **no host permissions and no network permissions**; the service worker only opens the app tab and registers a context-menu entry. All parsing, rendering, OCR-free text work and file writing happen inside the extension page, and results are saved through the browser's normal download mechanism.

## Build from source

```powershell
cd chrome-extension
npm install
npm run build     # -> chrome-extension/dist (load unpacked from here)
npm test          # Node integration tests
npm run package   # release-artifacts/PDF-Swiss-Army-Knife-Chrome-Extension-<version>.zip
```

Third-party components: [pdf-lib](https://github.com/Hopding/pdf-lib) (MIT), [pdf.js](https://github.com/mozilla/pdf.js) (Apache-2.0), [React](https://github.com/facebook/react) (MIT), [lucide-react](https://github.com/lucide-icons/lucide) (ISC), [PT Sans](https://github.com/google/fonts/tree/main/ofl/ptsans) (SIL OFL 1.1), [@pdf-lib/fontkit](https://github.com/Hopding/fontkit) (MIT).
