# Release notes - v2.2.0

v2.2.0 is a reliability release. It does not add new modules; it makes the
existing editors, formula engine and file formats behave the way the previous
release notes said they did. Every fix below has a regression test.

## Added

- **XLSX chart export.** Column, bar, line, pie and area charts are written as
  real ChartML parts (`xl/charts/chartN.xml`), each anchored to its cell
  position through `xl/drawings/drawingN.xml` and its relationships. Titles,
  series names and colours, legends, axis titles and data labels are written.
  Multi-chart and multi-sheet workbooks get package-wide numbering. Chart kinds
  the exporter cannot represent stay in `.oswk` and each one produces a save
  warning naming the chart and the reason.
- **XLSX conditional-formatting export for all editor rule kinds**, including
  data bars:
  - greater / less / between / equal -> `cellIs` with the matching operator
  - text contains -> `containsText` with the required `text` attribute
  - duplicate values -> `duplicateValues`
  - top / bottom N -> `top10` with `rank` (and `bottom="1"`)
  - data bars -> `dataBar` with a min/max colour scale
  The editor writes `textContains` / `duplicate`; both spellings are accepted so
  existing `.oswk` files keep exporting.
- `WEEKNUM(serial, [type])` with Excel's types 1, 2, 11-17 and the ISO type 21.
- Component regression tests for the real editors: Calc (Enter/Tab/formula-bar
  entry, click-free consecutive typing) and Writer (Enter split, Backspace and
  Delete merges, Shift+Enter line breaks).
- XLSX round-trip integration suite over a golden 100-row, multi-sheet
  workbook: values, formulas, cross-sheet references, styles, number formats,
  merges, column widths, row heights, freeze panes, autofilter, data
  validation, conditional formatting and charts.

## Improved

- **Calc keyboard flow.** Focus after `Enter`/`Tab` is restored in the same
  event, the key handler cannot see a stale editor, and blur commits no longer
  steal focus from the formula bar. IME composition is ignored by the grid
  shortcut handler so dead keys and Turkish/accented input cannot open a cell
  editor by accident.
- **Writer structural editing.** The paragraph being edited is repainted from
  the model before focus moves, so `blur` can no longer write stale text back
  over the edit; the caret after split/merge/line-break is placed in the same
  React commit instead of a timer.
- **Formula engine.** References and sheet names are case-insensitive
  (`=a1`, `=sheet1!a1`); invalid A1 references produce `#REF!` instead of an
  empty value; the full reference matrix (relative, absolute, mixed, quoted
  sheet, ranges, cross-sheet) is covered by tests.
- **Calc data bars** scale against the maximum of their range and use the rule
  colour, matching the XLSX data bars they now export.
- **Error presentation.** A shared `errorMessage()` helper localizes errors by
  code; the office workspace, launcher and batch/converter screens no longer
  show raw `String(error)` text.
- **Openpyxl-verified package.** The generated XLSX opens in an independent
  reader without warnings or repairs; the structural checks are kept in the
  repository tests.

## Fixed

- Calc: the first keystroke after committing a cell with `Enter` could be lost
  ("continuing to type without clicking the next cell is not fully reliable").
- Calc: clicking the formula bar while a cell editor was open could lose focus
  back to the grid.
- Writer: Enter duplicated the paragraph tail; Backspace/Delete merges could
  resurrect the removed paragraph; Shift+Enter could lose the line break on the
  next keystroke.
- Formula engine: lowercase references and sheet names silently returned empty
  values; malformed references silently evaluated.
- XLSX: `[Content_Types].xml` was malformed whenever comments or charts were
  exported (overrides were appended after `</Types>`).
- XLSX: chart references were not XML-escaped, so a sheet name containing `&`
  or `<` corrupted the chart part.
- XLSX: an empty header produced a `headerFooter` that other readers reject.
- Office UI: raw `String(error)` messages in toasts and per-file results.

## Security

- No new macro, script or network behaviour. Charts and conditional formatting
  are escaped as XML; ZIP-bomb limits, XML hardening and the redaction pipeline
  from 2.1.0 are unchanged and still covered by their integration tests.
- Redaction is still verified by re-extracting text from the redacted output
  and asserting the target string is gone; the tests were not weakened.

## Performance

- The Calc grid no longer recomputes an address list per cell for data bars:
  one pass per rule builds the scale map, so large ranges with data bars render
  in linear time instead of per-cell scans.
- XLSX chart planning validates ranges once at export time.

## Compatibility

- `.oswk` files keep opening; nothing was removed from the model.
- XLSX export produces a richer but still valid package; XLSX import is
  unchanged (values and formulas via `calamine`).
- `WEEKNUM` is new; existing functions and their error codes are unchanged.

## Tests

- Rust workspace: 241 tests (was 223), all green, including the new XLSX
  chart/conditional-format structure tests and the 5-test round-trip suite.
- Frontend: 325 tests (was 303), including 12 new editor component tests.
- Type check: `npx tsc --noEmit` clean.

## Known limitations

- XLSX import reads values and formulas only; styling, merges, layout,
  validation rules, conditional rules and charts are not read back from
  imported files. Comments are exported but not imported.
- Exported charts use default styling beyond the properties the editor stores
  (title, series names/colours, legend, axis titles, data labels).
- Writer on-screen canvas is still a continuous page view, not a paginated
  WYSIWYG editor; page setup, print and PDF export are page-accurate.
- The formula engine has a call-depth/range guard but no full dependency-graph
  recalculation engine; that is planned for V2.5/V3.0.
- PPTX export still does not embed charts.

## Deferred to V2.5 / V3.0

- True paginated Writer canvas with live layout
- Track changes and footnotes
- Pivot tables and dynamic arrays
- Master slides and animation engine
- PPTX chart import/export and SmartArt
- Full dependency-graph recalculation engine
- Digital signatures, full PDF editor, local RAG, plugin system, cloud sync
