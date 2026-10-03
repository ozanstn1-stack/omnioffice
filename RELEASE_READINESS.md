# Release Readiness — Office Swiss Army Knife 3.5.1

This file states what is actually implemented, tested and benchmarked, and
what is not. It is deliberately conservative: nothing is claimed as released,
built or verified unless it was reproduced in this environment or is produced
by CI.

## Implemented (this cycle)

- **Physical-pixel previews** (`src-tauri/src/commands.rs`,
  `crates/pdfcore/src/render.rs`): `page_preview` renders at 600 dpi and lets
  `max_width` cap the result, so the requested raster width is actually
  produced instead of the fixed 96 dpi bitmap being upscaled by the webview.
- **Reader sizing/cache** (`src/screens/Reader.tsx`, `src/lib/format.ts`):
  requests CSS width × device pixel ratio (capped at 3000 px; backend clamp
  200–4000), keeps one bitmap per page with LRU eviction (24 entries), reuses
  a cached bitmap when it is at least as sharp, keeps the old bitmap visible
  while a sharper one loads, and shrinks the prefetch margin for
  high-resolution renders.
- **Pinch smoothness**: zoom updates coalesce to one React update per frame;
  the page component is memoized.
- **`PageCanvas`** (Info, Watermark, PDF Studio) uses the same physical-pixel
  request width.

## Tested in this environment

| Gate | Command | Result |
|---|---|---|
| Frontend unit/integration | `npm test` | 35 files, 652 passed, 1 skipped |
| Frontend coverage | `npm run test:coverage` | 60.70 % statements, 71.18 % branches, 48.60 % functions (floors pass) |
| TypeScript | `npx tsc --noEmit` | clean |
| ESLint | `npm run lint` | 0 errors, 0 warnings |
| Rust (new regression) | `cargo test -p pdfcore --test render_images_test high_dpi` | passed |
| Rust workspace | `cargo test --workspace` | 529 passed, 3 ignored |
| Rust clippy (app) | `cargo clippy -p pdf-swiss-army-knife --all-targets -- -D warnings` | clean |

New tests: `high_dpi_render_honours_the_requested_raster_width` (proves the
old 96 dpi path stayed at ~793 px while the high-dpi path reaches 2000 px) and
`reader preview sizing and cache` (`previewRasterWidth`, `rememberPreview`,
`reusablePreview`).

## Benchmarked

- Existing cargo/frontend perf guards run in CI (Writer pagination,
  50k/100k-cell workbooks, dependency chains, PDF compression). Reader preview
  rendering is bounded by the raster cap and the LRU cache; no dedicated
  benchmark was added for it.

## Not done in this pass (honest)

- **Tiled/partial rendering** for extreme zoom on very high-density screens:
  beyond what a full-page bitmap covers (3000 px request, 4000 px backend
  cap) the image is still upscaled. This is Phase 3 work.
- Desktop E2E and fuzzing (Phase 2 leftovers) are still not started.
- The reader cache holds 24 bitmaps per document; a long high-zoom scroll
  re-renders evicted pages instead of keeping everything.

## Platform support and artifacts

- **Windows**: source builds and tests locally; installer/portable ZIP via
  `npm run release:local` or CI, with SBOMs, checksums and provenance.
- **Android**: built by CI and by `npm run release:local`; artifacts are named
  `PDF-Swiss-Army-Knife-Android-3.5.1-<abi>.apk`.
- **Linux/macOS desktop**: built and tested by CI.
- **Chrome extension**: unchanged.

## Security status

Unchanged from 3.5.0: no silent overwrite, signatures report `trust: unknown`
offline with DSS archiving, plugin network HTTPS-only with private/metadata
refusal, Android `allowBackup=false` with the documented AI-key fallback.

## Known limitations

See the README "Known limitations" section (single source of truth), including
the new reader-zoom note about the 3000 px preview ceiling.
