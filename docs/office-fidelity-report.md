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

## Not measured yet (honest list)

- Images, charts, comments, headers/footers, numbering and section breaks are
  covered by their own format tests but are **not** part of this corpus yet;
  the next D6 round should add them with the same counter style.
- ODT tracked changes are read by a separate importer and are not part of the
  ODT corpus row; the row measures text, styles and tables.
- Dynamic arrays (spill ranges) are read as their formula text; the app does
  not calculate spill ranges yet.
- XLSX structured tables and shared formulas are read and expanded, but the
  writer still writes plain formulas (no `t="shared"` output); Excel accepts
  the result, the file is just larger.
- The corpus is programmatic, not a fuzz target. A random-mutation fuzz
  harness is still open; the deterministic corpus is what CI runs today.

## Türkçe özet

- D6'da DOCX değişiklik izleri düzenleyicide **yazar + tarih** ile görünüyor ve
  kaydetmeyi atlatıyor; XLSX'te paylaşılan formüller içe aktarımda satır
  ofsetiyle **açılıyor** (`B2 → =A2*2`); PPTX geçiş süresi (`advTm`) artık
  yazılıp okunuyor (fade + 800 ms korunuyor).
- Programatik round-trip korpusu (`crates/officecore/tests/office_fidelity.rs`)
  her biçim için sayaçları yazdırıyor: **ölçülen delta sıfır** (DOCX 2/2 blok,
  8/8 run, 2/2 revizyon; ODT 6/6 run, 4/4 hücre; XLSX 3/3 hücre, 1/1 formül;
  PPTX geçiş + süre).
- Henüz ölçülmeyenler dürüstçe listelendi: resim/grafik/yorum/üst bilgi,
  ODT değişiklik izleri, dinamik diziler, XLSX yazıcısının paylaşılan formül
  üretmemesi ve fuzz hedefinin hâlâ açık olması.
