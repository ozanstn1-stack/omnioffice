# Release Readiness — Office Swiss Army Knife 3.5.2

This file states what is actually implemented, tested and benchmarked, and
what is not. It is deliberately conservative: nothing is claimed as released,
built or verified unless it was reproduced in this environment or is produced
by CI.

## Implemented (this cycle)

- **Live pinch scaling** (`src/screens/Reader.tsx`): while two fingers are
  down, the page container is scaled through a CSS transform that mirrors the
  gesture 1:1 (including the two-finger pan), anchored at the pinch midpoint.
  On release the zoom is committed to the React layout; the transform is
  dropped by the layout effect once the debounced page width and the sharper
  bitmap arrive. A pure two-finger pan without a scale change is folded into
  the scroll offset.
- No backend changes in this release.

## Tested in this environment

| Gate | Command | Result |
|---|---|---|
| Frontend unit/integration | `npm test` | 35 files, 653 passed, 1 skipped |
| Frontend coverage | `npm run test:coverage` | 60.82 % statements, 71.05 % branches, 48.63 % functions (floors pass) |
| TypeScript | `npx tsc --noEmit` | clean |
| ESLint | `npm run lint` | 0 errors, 0 warnings |
| Rust workspace | `cargo test --workspace` | 529 passed, 3 ignored (unchanged) |

New test: `scales the pages with the fingers while pinching and commits the
zoom on release` — a 100→200 px two-finger gesture must produce
`translate(50px, 0px) scale(2)` on the content element immediately, and
release must commit the 200 % zoom.

## Benchmarked

- Existing cargo/frontend perf guards run in CI; the pinch path itself is pure
  DOM/CSS (no React renders per pointer event), so no new benchmark was added.

## Not done in this pass (honest)

- Tiled/partial rendering for extreme zoom on very high-density screens is
  still Phase 3; beyond the bitmap ceiling the image is upscaled.
- Desktop E2E and fuzzing remain Phase 2 leftovers.
- Pinch momentum (inertial zoom) is not implemented; the gesture follows the
  fingers exactly and stops on release.

## Platform support and artifacts

- **Windows**: source builds and tests locally; installer/portable ZIP via
  `npm run release:local` or CI, with SBOMs, checksums and provenance.
- **Android**: built by CI and by `npm run release:local`; artifacts are named
  `PDF-Swiss-Army-Knife-Android-3.5.2-<abi>.apk`.
- **Linux/macOS desktop**: built and tested by CI.
- **Chrome extension**: unchanged.

## Security status

Unchanged from 3.5.1: no silent overwrite, signatures report `trust: unknown`
offline with DSS archiving, plugin network HTTPS-only with private/metadata
refusal, Android `allowBackup=false` with the documented AI-key fallback.

## Known limitations

See the README "Known limitations" section (single source of truth), including
the reader-zoom note about the 3000 px preview ceiling.
