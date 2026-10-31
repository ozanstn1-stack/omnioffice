# Scanner corpus and measured accuracy (D4)

The smart scanner (C12) finds the page in a photo without sending the image
anywhere: a downscaled grayscale copy goes through Gaussian blur, Canny edges
and Hough lines, the brightest convincing quadrilateral is scored, and for a
two-page capture the sheet is split at the spine. Until v6.14.0 that pipeline
had no published accuracy number.

This page records the corpus the number comes from, how it is generated, how a
pass is defined, and the measured result — including the two photos the
detector still gets wrong.

## Why a generated corpus

The C12 acceptance wants *real* camera photos, but photos of documents cannot
be committed to this repository: they carry personal data and would add tens of
megabytes. The committed corpus is therefore **photo-realistic and synthetic**:
`scripts/generate-scanner-corpus.py` renders 14 PNGs deterministically with the
Python standard library only (no Pillow, no numpy). It simulates the cases the
device matrix cares about — paper on dark and light desks, one-point
perspective, rotation, a strong shadow gradient, warm/cool white balance,
vignetting, paper texture with text lines, soft focus, a page cut by the frame
edge, and two-page spreads with a dark spine.

The images are deliberately small (single pages 360×480, spreads 560×400) so
the corpus stays around 2.5 MB and the golden test runs in seconds. Corner
accuracy is measured in exact pixels of the upright photo, so no resize
tolerance is folded into the number.

**Limit of this measurement.** This is *not* a real-camera measurement. Real
photos add motion blur, autofocus hunting, screen glare, curved pages and
EXIF-rotated sensors. The device pass in `docs/lan-sync-acceptance.md` style
(D1) stays the authority for real photos; this corpus makes the detector's
regression measurable on every commit.

## Layout

```
corpus/scanner/
  ground-truth.json        # version, per-image conditions and expected quads
  01_desk_dark_perspective.png
  ...
  14_spread_rotated.png
```

`ground-truth.json` holds the expected page corners in source pixels,
`[top-left, top-right, bottom-right, bottom-left]` per page, and a short list of
conditions (shown in the table below). Regenerate with:

```bash
python3 scripts/generate-scanner-corpus.py
```

The generator is deterministic (fixed seed, integer noise), so regenerating
does not silently move the baseline.

## Acceptance rule

A photo passes when **both** hold:

1. the detected page count equals the expected count (a spread must yield two
   pages, a single sheet exactly one), and
2. for every page, the mean corner error is ≤ 3.5 % of the image diagonal and
   the worst corner is ≤ 6 %.

The D4 target is at least **80 %** of the corpus passing; the test asserts it,
so a regression below the target fails CI.

## Measured result (v6.14.0)

`cargo test -p pdfcore --lib page_detect::corpus -- --nocapture` prints the
table below and asserts the target:

```
scanner corpus: 12/14 usable (85 %; target 80 %)
```

| Image | Conditions | Pages | Measured error (mean / max) | Verdict |
| --- | --- | --- | --- | --- |
| `01_desk_dark_perspective.png` | dark desk, perspective | 1 | 0.4 / 0.6 px | pass |
| `02_desk_light_shadow.png` | light desk, shadow | 1 | 18.9 / 39.3 px | **fail** |
| `03_rotated_12deg.png` | rotation 12°, dark desk | 1 | 0.4 / 0.6 px | pass |
| `04_low_contrast.png` | low contrast, soft edge | 1 | 0.5 / 0.7 px | pass |
| `05_busy_desk.png` | busy desk, perspective | 1 | 0.5 / 0.6 px | pass |
| `06_warm_light.png` | warm light, vignette | 1 | 0.5 / 0.7 px | pass |
| `07_cool_color.png` | cool light, colour cast | 1 | 0.4 / 0.7 px | pass |
| `08_strong_perspective.png` | strong perspective | 1 | 0.5 / 0.6 px | pass |
| `09_paper_texture.png` | paper texture, text lines | 1 | 0.3 / 0.6 px | pass |
| `10_soft_focus.png` | defocus | 1 | 0.6 / 1.0 px | pass |
| `11_edge_crop.png` | page near the edge | 1 | 0.5 / 0.7 px | pass |
| `12_spread_dark.png` | spread, dark desk, spine shadow | 2 | 12.8 / 25.5 px; 9.6 / 34.5 px | pass |
| `13_spread_light_shadow.png` | spread, light desk, shadow | 2 | 13.1 / 26.3 px; 38.5 / 80.6 px | **fail** |
| `14_spread_rotated.png` | spread, rotation 7° | 2 | 0.3 / 0.6 px; 0.3 / 0.5 px | pass |

Thresholds: single pages 21.0 px mean / 36.0 px max (diagonal 600 px); spreads
24.1 px mean / 41.3 px max (diagonal ≈ 688.8 px).

Page separation works: all three spreads return exactly two pages, and the
rotated spread is split as precisely as a straight single page. Dark-spine
spreads keep a few pixels of the spine shading in the outer corners (the
bright-region path lands inside the paper and the edge snap has no gradient to
follow at the spine shadow), well within the tolerance.

### The two known failures

- `02_desk_light_shadow.png` — a bright page on a **bright** desk under a
  shadow gradient. The page/desk boundary loses most of its contrast on the
  lit side, so the bright-region candidate grows into the desk; the corner
  snap then has too few edge pixels to pull back. The detector does return the
  correct page count, so the manual corner picker is offered instead of a
  wrong crop.
- `13_spread_light_shadow.png` — the same lighting on a two-page spread. The
  left half stays within tolerance; the right half runs into the shadowed desk,
  so its outer corners are pulled outwards (80.6 px worst). Page separation
  still returns two pages.

Both are accepted as documented limitations: 12/14 is above the 80 % target and
the detector never reports a false negative page count. The corpus is **not**
weakened to hide them; making a hard case easier would hide a real failure.
Improving them belongs to the next scanner pass (stronger shadow model in the
scoring, or a paper-sheen term), not to a corpus edit.

## Tests

- `crates/pdfcore/src/page_detect/corpus_tests.rs#the_corpus_meets_the_acceptance_target`
  — the measurement and the 80 % assertion.
- `crates/pdfcore/src/page_detect/corpus_tests.rs#a_two_page_spread_is_split_and_a_single_page_is_not`
  — page separation on every corpus image.
- `src-tauri/src/scanner.rs#scan_detect_batch_splits_a_two_page_spread` — the
  batch command the scanner screen calls: one spread becomes two page entries
  under the same photo path, a single page stays one, a blank wall reports
  nothing.
- `src/screens/Scanner.test.tsx#shows a two-page spread as two pages of the same photo`
  — the screen side: two rows with the spread badge, both halves sent to the
  PDF builder.
