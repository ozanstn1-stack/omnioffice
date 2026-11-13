# Office fidelity report (D6)

D6 asked for the places where the office core still loses what a user wrote,
and for a measured baseline instead of an assurance. This report covers the
four items: DOCX tracked changes, XLSX formula depth, PPTX transition metadata
and the round-trip corpus for DOCX/ODT/XLSX.

The corpus is programmatic (`crates/officecore/tests/office_fidelity.rs`):
each test builds a document in the model, writes it in its format, reads it
back and compares the counters that represent content — blocks, paragraphs,
runs, bold runs, table cells, revisions and character count. A binary fixture
would hide *which* construct disappeared; the counters fail on the loss and
the printed line names it.

Run it with:

```bash
cargo test -p officecore --test office_fidelity -- --nocapture
```

## Measured round trip (this build)

| Format | blocks | paragraphs | runs | bold runs | tables | table cells | revisions | chars |
|---|---|---|---|---|---|---|---|---|
| DOCX | 2/2 | 5/5 | 8/8 | 1/1 | 1/1 | 4/4 | 2/2 | 48/48 |
| ODT | 2/2 | 5/5 | 6/6 | 1/1 | 1/1 | 4/4 | – | 31/31 |
| XLSX | 3/3 cells | 1/1 formula | 1/1 text cell | – | – | – | – | – |
| PPTX | `fade` transition + 800 ms | – | – | – | – | – | – | – |

`before/after`; `before` is the model written, `after` is the model read back.
**The measured delta is zero for every listed construct.**

The empty paragraph a DOCX writer must place after a table is packaging, not
content, so the counters ignore runs with no text; a lost run, table cell or
revision still changes them.

## What D6 changed

### DOCX tracked changes

- `w:ins`, `w:del` and `w:rPrChange` were already imported and exported; D6
  closes the visible gap in the editor: the inline marker now carries the
  **author and the date** in its tooltip (`Ada · 2026-10-31 09:00:00 UTC`)
  instead of the author alone. The kind is still a CSS class
  (`writer-rev-insert` / `-delete` / `-format`), so insertions and deletions
  are distinguishable at a glance.
- The round-trip test proves the model keeps the revisions across a save
  (`roundtrip_tracked_changes`), and the new corpus test proves the counters
  survive the same trip end to end.

### XLSX shared formulas

`calamine` exposes the base formula of a shared group and hands the filled-down
cells back empty. D6 adds `crates/officecore/src/xlsx/shared_formulas.rs`: it
reads the raw sheet XML, keeps the base formula per `si`, and expands every
follower by shifting its relative A1 references by the row/column offset from
the base cell.

- Absolute (`$A$1`), relative (`A1`) and mixed (`$A1`, `A$1`) references,
  ranges (`A1:A10`) and cross-sheet references (`Sheet2!A1`) are handled.
- Quoted strings are never treated as references (`"A1 text"&B2`), and a
  function name (`LOG10(...)`) or a structured reference (`Sales[Amount]`) is
  left alone.
- Corrupt or unknown groups are skipped silently; the import never fails
  because of a formula it cannot expand.
- Measured: the test workbook with one base and two followers imports with
  **2/2 followers filled down** (`B2 → =A2*2`, `B3 → =A3*2`).

### PPTX transitions

The model already stored `slide.transition` and `slide.transition_ms`, but the
writer hard-coded `spd="med"` and dropped the duration. D6 writes the duration
as `advTm` and reads it back on import: a `fade` transition with 800 ms
survives the round trip (previously the duration came back as the default
500 ms).

### Round-trip corpus

`crates/officecore/tests/office_fidelity.rs` is the start of the D6 corpus: it
names its constructs, prints a one-line fidelity table and fails on a silent
loss. It currently covers text styles, tables, revisions (DOCX), values and
formulas (XLSX), shared formulas and transition metadata (PPTX).

## Extended corpus (7.47.0)

`crates/officecore/tests/office_fidelity_extended_test.rs` extends the
baseline corpus across higher-risk document structures: embedded PNG images,
charts with cached category/value data, threaded comments and replies (with
resolved state and anchored runs), first/even/default headers and footers,
lists and numbering, multi-section page setup, freeze panes, hyperlinks with
tooltips, structured tables with column formulas/filters, print areas/titles/breaks,
sheet protection, slide masters and layouts, nested object groups, image crops,
SMIL/OOXML animations, speaker notes, slide transitions, and hidden slides.

Run it with:

```bash
cargo test -p officecore --test office_fidelity_extended_test -- --nocapture
```

| Format | Measured constructs (after / before) |
|---|---|
| DOCX | images `1/1`, bytes `67/67`, comments `1/1`, replies `1/1`, anchors `1/1`, headers `4/4`, footers `2/2`, bullets `1/1`, numbered `1/1`, sections `2/2` |
| ODT | images `1/1`, bytes `67/67`, comments `1/1`, replies `1/1`, resolved `1/1`, anchors `1/1`, headers `1/1`, footers `1/1`, lists `2/2` |
| XLSX | images `1/1`, charts `1/1`, cache points `2/2`, tables `1/1`, links `1/1`, tooltips `1/1`, comments `1/1`, freeze `1x2/1x2`, breaks `2/2`, names `1/1`, print settings `3/3`, protection `1/1` |
| PPTX | masters `1/1`, layouts `1/1`, groups `2/2`, crops `1/1`, charts `1/1`, cache points `6/6`, animations `3/3`, notes `1/1`, transitions `1/1`, hidden slides `1/1` |
| ODP | masters `1/1`, groups `2/2`, crops `1/1`, charts `1/1`, cache points `6/6`, animations `3/3`, hidden slides `1/1` |

## Not measured yet (honest list)

- ODT tracked changes are read by a separate importer and are not part of the
  ODT corpus rows; ODT list import maps `<text:list>` items into list
  paragraphs (`lists 2/2`) rather than separating numbered vs. bulleted kinds
  the way DOCX does, and multi-section DOCX breaks are measured on DOCX.
- ODP layout definitions, slide notes, and slide transitions are measured in
  their dedicated ODP unit/integration tests; the extended corpus compares the
  seven ODP counters listed above (`masters`, `groups`, `crops`, `charts`,
  `cache_pts`, `anims`, `hidden`).
- Dynamic arrays: Calc calculates spill ranges, and an Excel spill imports as
  its anchor formula (the cached spill cells are dropped so it spills again).
  On save the spill range is written as Excel's dynamic array (`cm="1"`,
  `t="array" ref`, `xl/metadata.xml`), so Excel spills it as well.
- XLSX shared formulas are imported by expanding relative offsets and exported
  with `t="shared"` (`crates/officecore/src/xlsx/shared_write.rs`) across
  consecutive vertical/horizontal runs (`si` + `ref` on anchors); arbitrary
  disconnected/overlapping 2D rectangles stay separate or ordinary `<f>`
  elements.
- Both `office_fidelity.rs` and `office_fidelity_extended_test.rs` are
  deterministic corpora, complementing the byte-level fuzz targets under
  `fuzz/fuzz_targets/`.

## Türkçe özet

- D6'da DOCX değişiklik izleri düzenleyicide **yazar + tarih** ile görünüyor ve
  kaydetmeyi atlatıyor; XLSX'te paylaşılan formüller içe aktarımda satır
  ofsetiyle **açılıyor** (`B2 → =A2*2`) ve dışa aktarımda ardışık yatay/dikey
  formüller `t="shared"` olarak gruplanıyor; PPTX geçiş süresi (`advTm`) yazılıp
  okunuyor (fade + 800 ms korunuyor).
- Programatik round-trip korpusları (`office_fidelity.rs` ve 7.47.0'da eklenen
  `office_fidelity_extended_test.rs`) sıfır kayıp ölçüyor: DOCX, ODT, XLSX, PPTX
  ve ODP'de gömülü görseller, önbellekli grafikler, yanıtlı/çapalı yorumlar,
  üst/alt bilgiler, numaralı/madde listeler, bölüm sonları, dondurulmuş bölmeler,
  yapılandırılmış tablolar, yazdırma ayarları, sayfa koruması, asıl slaytlar,
  iç içe gruplar, kırpılmış görseller, animasyonlar ve gizli slaytlar doğrulanır.
- Henüz kapsanmayanlar dürüstçe listelendi: ODT değişiklik izleri, dinamik
  diziler, ODT liste türü ayrımı ile 2B çoklu paylaşım dikdörtgenleri.
