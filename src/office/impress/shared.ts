/**
 * Impress values shared by the editor controller and its extracted views.
 *
 * Moved verbatim out of ImpressEditor.tsx (B4 editor seam 3): the dialog and
 * slide-view modules need these runtime values, and importing them back from
 * the editor would make the modules import each other. ImpressEditor.tsx
 * re-exports the public names from here, so its module API is unchanged.
 */
import type { ChartData } from "../../lib/office-types";

/** The slide theme the editor picks from `THEMES` and the views render with. */
export type Theme = {
  id: string;
  name: string;
  background: string;
  title: string;
  body: string;
  accent: string;
  titleColor: string;
  bodyColor: string;
};

export const MAX_GROUP_DEPTH = 8;
export const INHERITED_KEYS: Set<string> = new Set();
export const CHART_PALETTE = ["#2563EB", "#F97316", "#10B981", "#8B5CF6", "#EF4444", "#14B8A6"];

export const ANIMATION_EFFECTS: Record<string, string[]> = {
  entrance: ["appear", "fade", "flyIn", "zoom"],
  emphasis: ["pulse", "spin", "grow", "shrink"],
  exit: ["disappear", "fadeOut", "flyOut"],
};

// ---------------------------------------------------------------------------
// Chart data helpers
//
// A presentation carries no workbook, so a chart that only has ranges cannot
// render outside the editor. The chart dialog therefore edits the V3.1 caches
// (`categoriesCache` / `seriesValuesCache`), which the PPTX exporter writes as
// `c:strCache` / `c:numCache` plus an embedded Excel workbook. The pure helpers
// below are exported for unit tests; the dialog binds them to the grid.
// ---------------------------------------------------------------------------

/** Sheet name the Rust exporter uses for the embedded chart workbook. */
const CHART_DATA_SHEET = "Sheet1";

/** Parses one cell as a locale-independent decimal; anything else is null. */
export function parseChartCellNumber(text: string): number | null {
  const trimmed = text.trim();
  if (!/^[+-]?(\d+(\.\d+)?|\.\d+)([eE][+-]?\d+)?$/.test(trimmed)) return null;
  const value = Number(trimmed);
  return Number.isFinite(value) ? value : null;
}

/** Text a cached number is edited as: plain decimal, no locale formatting. */
export function formatChartCellValue(value: number): string {
  return String(value);
}

/**
 * Splits pasted text into rows of cells. A row that contains a tab is split on
 * tabs only, so comma decimals inside spreadsheet exports are kept together;
 * otherwise commas are the separator. Line endings may be CRLF, CR or LF and a
 * single trailing newline is ignored. Blank rows are kept so pasted columns
 * stay aligned with the row they start at.
 */
export function parseChartClipboard(text: string): string[][] {
  const lines = text.replace(/\r\n?/g, "\n").split("\n");
  while (lines.length > 0 && lines[lines.length - 1] === "") lines.pop();
  return lines.map((line) => line.split(line.includes("\t") ? "\t" : ",").map((cell) => cell.trim()));
}

/**
 * A1 range for one column of the embedded workbook the exporter builds:
 * row 1 is the header, so the first cached value lives on row 2.
 */
export function chartWorkbookRange(columnIndex: number, rowCount: number): string {
  let value = columnIndex + 1;
  let letters = "";
  while (value > 0) {
    const remainder = (value - 1) % 26;
    letters = String.fromCharCode(65 + remainder) + letters;
    value = Math.floor((value - 1) / 26);
  }
  return `${CHART_DATA_SHEET}!$${letters}$2:$${letters}$${rowCount + 1}`;
}

/** True when any chart cache carries data; false for range-only charts. */
export function chartHasCachedData(chart: ChartData): boolean {
  return (
    (chart.categoriesCache ?? []).length > 0 || (chart.seriesValuesCache ?? []).some((values) => values.length > 0)
  );
}

/** Display rows: enough for every cache plus at least one empty row. */
export function chartDataRowCount(chart: ChartData): number {
  const valueRows = (chart.seriesValuesCache ?? []).reduce((max, values) => Math.max(max, values.length), 0);
  return Math.max(1, (chart.categoriesCache ?? []).length, valueRows);
}

/** One grid cell: row index and column index (0 is the category column). */
export interface ChartGridCell {
  row: number;
  column: number;
}

/**
 * Deep clone for the dialog draft plus cache arrays aligned with `series`, so
 * grid edits never mutate the document until Save is pressed.
 */
export function normalizeChartDraft(chart: ChartData): ChartData {
  const clone = JSON.parse(JSON.stringify(chart)) as ChartData;
  clone.categoriesCache = [...(chart.categoriesCache ?? [])];
  clone.seriesValuesCache = chart.series.map((_, index) => [...(chart.seriesValuesCache?.[index] ?? [])]);
  return clone;
}

/**
 * Writes a parsed clipboard block into the caches starting at `target`.
 * Column 0 holds category labels (any text), columns 1..n the series values;
 * cells that do not parse as numbers are skipped, mirroring the rule that
 * invalid cell input is never committed. Leading rows beyond the current
 * series length are padded with 0 because the cache is a dense list.
 */
export function mergeChartPaste(chart: ChartData, target: ChartGridCell, block: string[][]): ChartData {
  const categories = [...(chart.categoriesCache ?? [])];
  const seriesValues = (chart.seriesValuesCache ?? []).map((values) => [...values]);
  block.forEach((line, rowOffset) => {
    line.forEach((cell, columnOffset) => {
      const row = target.row + rowOffset;
      const column = target.column + columnOffset;
      if (column === 0) {
        while (categories.length <= row) categories.push("");
        categories[row] = cell;
        return;
      }
      const values = seriesValues[column - 1];
      if (!values) return;
      const parsed = parseChartCellNumber(cell);
      if (parsed === null) return;
      while (values.length < row) values.push(0);
      values[row] = parsed;
    });
  });
  while (categories.length > 0 && categories[categories.length - 1] === "") categories.pop();
  return { ...chart, categoriesCache: categories, seriesValuesCache: seriesValues };
}

/**
 * Rewrites the range fields to point at the embedded workbook the PPTX
 * exporter builds from the caches: category labels in column A, series 1..n in
 * B, C, ... This makes the exported chart self-consistent ("ranges that
 * reference nothing" become real). Called only when the user edited data in
 * this dialog session, so untouched imported charts keep their ranges.
 */
export function syncChartDataRanges(chart: ChartData): ChartData {
  const categories = chart.categoriesCache ?? [];
  const values = chart.seriesValuesCache ?? [];
  const dataRows = Math.max(categories.length, ...values.map((list) => list.length));
  if (dataRows === 0) return chart;
  return {
    ...chart,
    categories: chartWorkbookRange(0, categories.length > 0 ? categories.length : dataRows),
    series: chart.series.map((entry, index) => {
      const length = values[index]?.length ?? 0;
      return length > 0 ? { ...entry, range: chartWorkbookRange(index + 1, length) } : entry;
    }),
  };
}
