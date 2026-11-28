/**
 * Shared chart data helpers, palette, and default Writer chart factory
 * shared between Impress and Writer chart editors and previews.
 */
import type { ChartData } from "../../lib/office-types";

export const CHART_PALETTE = ["#2563EB", "#F97316", "#10B981", "#8B5CF6", "#EF4444", "#14B8A6"];

export interface ChartPreviewTheme {
  background?: string;
  accent: string;
  bodyColor: string;
}

export const WRITER_CHART_THEME: ChartPreviewTheme = {
  background: "#ffffff",
  accent: "#2563EB",
  bodyColor: "#1f2328",
};

export const CHART_KINDS = ["column", "bar", "line", "area", "pie", "doughnut", "scatter"] as const;

export const SCATTER_STYLES = [
  { value: "", key: "impress.chartScatterMarkers" },
  { value: "lineMarker", key: "impress.chartScatterLineMarker" },
  { value: "line", key: "impress.chartScatterLine" },
  { value: "smoothMarker", key: "impress.chartScatterSmoothMarker" },
  { value: "smooth", key: "impress.chartScatterSmooth" },
] as const;

/** Sheet name the Rust OOXML/ODF exporter uses for the embedded chart workbook. */
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
 * Splits pasted text into rows of cells (tabs take precedence over commas).
 */
export function parseChartClipboard(text: string): string[][] {
  const lines = text.replace(/\r\n?/g, "\n").split("\n");
  while (lines.length > 0 && lines[lines.length - 1] === "") lines.pop();
  return lines.map((line) => line.split(line.includes("\t") ? "\t" : ",").map((cell) => cell.trim()));
}

/**
 * A1 range for one column of the embedded workbook: row 1 is the header.
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

/** Deep clone for the dialog draft plus cache arrays aligned with `series`. */
export function normalizeChartDraft(chart: ChartData): ChartData {
  const clone = JSON.parse(JSON.stringify(chart)) as ChartData;
  clone.categoriesCache = [...(chart.categoriesCache ?? [])];
  clone.seriesValuesCache = chart.series.map((_, index) => [...(chart.seriesValuesCache?.[index] ?? [])]);
  return clone;
}

/** Writes a parsed clipboard block into the caches starting at `target`. */
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

/** Rewrites A1 range strings to match the embedded Sheet1 caches. */
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

/** Default 4-category chart pre-populated when inserting a chart in Writer. */
export function defaultWriterChart(title: string, categoryPrefix: string, seriesName: string): ChartData {
  return syncChartDataRanges({
    kind: "column",
    title,
    categories: "",
    series: [{ name: seriesName, range: "", color: CHART_PALETTE[0] }],
    legend: true,
    xTitle: "",
    yTitle: "",
    stacked: false,
    showLabels: false,
    categoriesCache: [1, 2, 3, 4].map((n) => `${categoryPrefix}${n}`),
    seriesValuesCache: [[24, 38, 31, 46]],
  });
}
