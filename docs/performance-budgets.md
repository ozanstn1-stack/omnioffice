# Performance and memory budgets (D9)

This page names the budgets the app promises, the measured numbers behind
them, and the test or gate that catches a regression. Two rules shape it:

- **A product budget is not a CI bound.** The product budget is what a user
  notices (a 500-page document must open in seconds, not minutes). The test
  bound is a generous multiple of the development measurement, so a loaded
  runner does not fail a correct build. Both are listed; a regression must
  cross the test bound to turn CI red.
- **Every number is measured, not estimated.** The command that prints it is
  next to the table.

## Measured baseline (this build, development machine)

| Flow | Measured | Test bound | Where |
|---|---|---|---|
| Writer DOCX save, 500 pages | **0.26 s** (23.5 KB) | 60 s | `crates/officecore/tests/perf_test.rs` |
| Writer DOCX open, 500 pages | **0.29 s** | 60 s | `crates/officecore/tests/perf_test.rs` |
| Calc XLSX save, 50 000 cells + 5 000 formulas | **1.09 s** (346 KB) | 60 s | `crates/officecore/tests/perf_test.rs` |
| Calc XLSX open, 50 000 cells | **4.24 s** | 60 s | `crates/officecore/tests/perf_test.rs` |
| Calc full compute, 50 000 cells | < 10 s | 10 s | `src/office/calc/perf.test.ts` |
| Calc incremental edit after one change | < 1 s (one dependent) | 1 s | `src/office/calc/perf.test.ts` |
| Calc render window, 50 000 rows | **≤ 60 rows** in the DOM | < 60 rows | `src/office/calc/perf.test.ts` |
| Writer PDF export, 100 pages | under 60 s | 60 s | `crates/officecore/tests/perf_test.rs` |
| Writer PDF export, 500 pages (heavy, `#[ignore]`) | under 300 s | 300 s | `crates/officecore/tests/perf_test.rs` |
| Peak memory, 500-page DOCX open/save | **29.4 MiB** | 384 MiB | `crates/officecore/tests/memory_budgets.rs` |
| Peak memory, 50 000-cell XLSX open/save | **135.3 MiB** | 256 MiB | `crates/officecore/tests/memory_budgets.rs` |
| Heavy-work concurrency bound | `max(2, min(8, cores))` slots | 2–8 | `src-tauri/src/concurrency.rs` |

Reproduce:

```bash
cargo test -p officecore --test perf_test -- --nocapture
cargo test -p officecore --test perf_test 500_pages -- --nocapture
cargo test -p officecore --test memory_budgets -- --nocapture
npx vitest run src/office/calc/perf.test.ts
cargo test -p pdf-swiss-army-knife --lib concurrency
```

The memory test installs a counting global allocator and reports the **peak
live bytes** of each flow; a process RSS reading would not be portable across
Windows, macOS and Linux CI. The measurement runs sequentially because two
parallel flows would share the counters.

## The named budgets

### Cold start

| Target | Budget | Status |
|---|---|---|
| Desktop window ready | ≤ 3.0 s | Manual timing per release (Desktop E2E launch log); no automated threshold yet |
| Android app first frame | ≤ 5.0 s | Manual device pass (the D8 record sheet); no automated threshold yet |

Cold start is not measured by a CI threshold on purpose: a shared runner's
first launch (disk cache cold, no GPU) says little about a user's machine, and
a flaky cold-start gate would be turned off. The release checklist records the
number; D10's signed release is where it becomes a recorded regression point.

### Document open and save

| Budget | Value |
|---|---|
| 500-page Writer document opens | ≤ 15 s product budget (test bound 60 s) |
| The same document saves | ≤ 10 s product budget (test bound 60 s) |
| 50 000-cell spreadsheet opens/saves | ≤ 30 s combined (test bounds 60 s each) |

### Rendering and editing

| Budget | Value |
|---|---|
| Writer pagination, ~100 pages of metrics | interactive (< 2 s, existing pagination guard) |
| Calc 50 000-cell compute | ≤ 10 s; a single dependent recalculates in ≤ 1 s |
| Calc render window | ≤ 60 rows / visible columns, independent of sheet size |
| PDF export | 100 pages ≤ 60 s; 500 pages ≤ 300 s |

### Memory ceilings

| Flow | Ceiling |
|---|---|
| 500-page DOCX open/save peak | 384 MiB |
| 50 000-cell XLSX open/save peak | 256 MiB |
| PDF runtime peak per heavy job | bounded by the worker slots: at most `max(2, min(8, cores))` heavy jobs at once |

## PDF runtime isolation

Heavy work (PDF render/thumbnails, OCR, raster compression, vault scan, office
import/export, signing) takes a slot from one process-wide semaphore before it
runs (`src-tauri/src/concurrency.rs`). The semaphore is capped at
`max(2, min(8, cores))` instead of Tokio's 512-thread default, so peak RAM and
child processes (tesseract) cannot multiply with the number of requests. The
bound is tested directly:

- `the_bound_is_a_small_constant_not_the_tokio_default` pins the 2–8 range and
  that the async semaphore starts with exactly that many permits.
- `a_panicking_worker_returns_its_blocking_slot` proves a parser panic does not
  leak a slot (a leak would starve the pool after a few malformed files).
- `blocking_permits_are_released_on_drop` covers the synchronous extraction
  path, which shares the same order of magnitude bound.

The rule for new heavy commands: take `crate::concurrency::acquire().await`
before `spawn_blocking`, and keep per-job memory proportional to the document,
not to the request count.

## Benchmark trends

`cargo bench -p officecore --bench parse -p pdfcore --bench pdf_ops` runs the
criterion suites. The Desktop workflow restores a `.bench-baseline.json` cache,
compares the run against it and reports a regression when a benchmark is more
than 15 % slower (warning at 7.5 %), then saves a new baseline only when the
check passed. The check is advisory (`continue-on-error`) because shared-runner
timing noise once produced nine red master runs; the artifact and the baseline
keep the trend visible without blocking a release. The always-on budgets above
are the blocking side of the same story.

## Türkçe özet

- **Ürün bütçesi ile CI eşiği ayrıdır:** ürün bütçesi kullanıcının fark ettiği
  sınır, test eşiği ise yüklü bir koşucuda yanlış kırmızı üretmemek için
  ölçümün cömert bir katı. İkisi de tabloda.
- **Ölçülenler:** 500 sayfa DOCX kaydetme 0.26 s / açma 0.29 s; 50 000 hücre
  XLSX kaydetme 1.09 s / açma 4.24 s; bellek tepeleri 29.4 MiB ve 135.3 MiB;
  50 000 satırda DOM'a giren pencere ≤ 60 satır.
- **Ağır iş sınırı:** PDF/OCR/sıkıştırma işleri `max(2, min(8, çekirdek))`
  yuvalı tek bir semafor kullanır; panikleyen iş yuvasını bırakır, böylece
  bellek ve alt süreç sayısı istek sayısıyla çarpılmaz.
- **Soğuk başlangıç** (masaüstü ≤ 3 s, Android ≤ 5 s) bilinçli olarak otomatik
  eşik değil; sürüm kontrol listesinde elle ölçülür. Kıyaslama trendleri
  criterion + önbellekli taban çizgisiyle izlenir; %15'ten yavaşlama raporlanır.
