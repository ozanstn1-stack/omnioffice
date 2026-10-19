# Release notes - v2.5.0

v2.5.0 turns the stabilised 2.2.0 suite into a feature release. The two large
pieces are a dependency-driven Calc engine (with dynamic arrays and pivot
tables) and a measured Writer pagination engine (with real pages, a table of
contents and a navigation pane). Everything below is implemented end to end:
model, computation, UI, persistence, export and tests. The features that were
deliberately left out are listed under "Deferred" so the gap is explicit.

## Calc

### Dependency graph and incremental recalculation

`computeWorkbookValues` used to evaluate every formula from scratch on every
change. It now builds a precedent/dependent graph from each formula's
references (`collectReferences` in `formula.ts`) and, after an edit, walks the
dirty closure - the edited cell plus everything that reads it - in dependency
order. The result is cached per workbook chain, so the next change starts from
the previous values.

- Volatile formulas (`NOW`, `NOW`, `RAND`, `RANDBETWEEN`, `OFFSET`, `INDIRECT`,
  `CELL`, `INFO`) and references the graph cannot expand (ranges over 4096
  cells, undefined names, unparsable formulas) mark the formula as a global
  dependent that recomputes on every edit - correctness first.
- Circular references keep failing loudly (`#REF!` with a "circular
  reference" detail) in both the full and the incremental path.
- `lastComputeStats(workbook)` reports `{ mode: "full" | "incremental",
  recomputed }`, which the performance tests assert against.

### Dynamic arrays and spill

A formula whose result is a matrix now spills: the source cell shows the first
value, the rest are written into neighbouring cells. `#SPILL!` is reported when
a target cell already holds data or another array's spill. Spilled cells are
readable by other formulas, and shrinking an array releases the cells it no
longer owns. Binary operators broadcast element-wise, which is what lets
`=FILTER(A1:A9, A1:A9>5)` work instead of comparing only the first cell.

### Pivot tables

Insert -> Pivot table opens a dialog over the used range (header row required):
pick the row field, an optional column field, the value field and the
aggregation (sum/count/average/min/max). The pivot is stored as a definition in
the workbook and computed live, so it never goes stale; the on-sheet grid has
refresh and remove buttons. XLSX export materialises the computed grid as plain
values at the pivot anchor and adds a warning that it is not a live Excel pivot
table; `.oswk` keeps the definition. The Rust exporter runs the same
aggregation in `officecore::pivot`, mirrored by tests on both sides.

### Statistics

New functions: `VAR.S`, `VAR.P`, `VARP`, `STDEV.S`, `STDEV.P`, `PERCENTILE`,
`PERCENTILE.INC`, `QUARTILE`, `QUARTILE.INC`, `CORREL`, `COVARIANCE.P`,
`COVARIANCE.S` and the legacy `COVAR`.

## Writer

### Pagination engine and paginated view

The pagination engine is pure and unit-tested: given the measured geometry of
every block (line boxes for paragraphs, row boxes for tables) it decides which
fragment of each block lands on which page.

- Paragraphs split at line boundaries with widow/orphan control (minimum two
  lines on either side unless the paragraph is taller than a page).
- Tables split at row boundaries; continuation pages repeat the header row and
  pay for it in the page budget.
- `keepWithNext` moves a block with its follower when the pair does not fit;
  `keepTogether` moves a block that would be split; `pageBreakBefore` always
  starts a new page.
- Measurement runs in a hidden probe column at the exact content width, so the
  numbers match the rendered pages.

The paginated view renders real page containers at the document's page size
(A4/A5/A3/Letter/Legal, portrait/landscape, custom margins) with headers and
footers on every page and resolved `{{page}}` / `{{pages}}` numbers. The status
bar page count is the laid-out count. Clicking a page opens the continuous
editor on that block; the View ribbon toggles Paginated / Continuous, and the
continuous editor is unchanged from 2.2.0.

### Table of contents and navigation

Insert TOC builds entries from Heading 1-6 paragraphs with their page numbers
and click-to-jump; Update TOC refreshes the numbers. TOC blocks are part of the
model: they are saved in `.oswk`, rendered in the paginated view, exported as
static entries to DOCX, ODT, RTF, HTML and Markdown, and laid out by the PDF
export. The navigation pane lists the same outline and jumps to a block.

`keepWithNext` and `keepTogether` are also exported to DOCX as `w:keepNext`
and `w:keepLines`.

## Tests and performance

- Rust: 246 tests (was 241). Frontend: 375 tests (was 325).
- New suites: dependency graph/incremental recalculation, dynamic arrays and
  spill, pivot computation, pagination rules, plus component tests that insert
  a pivot and a TOC through the real UI.
- Performance smoke tests: 10k, 50k and 100k cell workbooks must compute within
  generous budgets, and a single-cell edit must recompute exactly one formula;
  a 1 000-cell chain must recalculate incrementally.

## Compatibility

- `.oswk` files from 2.2.0 and earlier open unchanged; all new model fields are
  serde-defaulted on the Rust side.
- XLSX pivot export writes values plus a warning; DOCX TOC export writes static
  lines plus a warning. Nothing is silently dropped.

## Known limitations

- The paginated view is a layout/reading view: clicking a page switches to the
  continuous editor. Editing directly on the page fragments is planned for
  V3.0.
- Sections (multiple page setups in one document), track changes and footnotes
  are not part of 2.5.0 - see the roadmap.
- TOC entries reflect the last update in the editor when exported; Word will
  not refresh them.
- Pivot filters exist in the model but the dialog does not expose them yet.
- Docx/ODT export of pivots and dynamic arrays writes computed values.

## Deferred to V3.0 (or later)

- Writer sections with per-section page setup, headers/footers and section
  breaks; real paginated editing with a caret that crosses page boundaries.
- Track changes (insertions/deletions/format changes with accept/reject) and
  footnotes/endnotes with DOCX/ODT round trips.
- Formula autocomplete in the formula bar, trace-precedents/dependents auditing
  arrows and named table objects with structured references.
- Impress master slides, PPTX chart import/export, grouped-shape hierarchy,
  an animation model and a presenter view.
- PDF/A validation, a PDF sanitizer, annotation/form flattening, deeper OCR
  preprocessing, redaction verification reporting and accessibility
  diagnostics beyond the existing inspector findings.
- AI document actions and a provider abstraction (OpenAI-compatible/Ollama).
- Command palette and global search across documents.
- Full collaboration, cloud sync, plugin marketplace and enterprise features.
