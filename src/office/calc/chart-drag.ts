/**
 * Where a chart lands when it is dragged or resized on the sheet (4.1b).
 * Positions are canvas units: the sheet's own pixels before the grid zoom.
 */
import type { ChartPlacement } from "../../lib/office-types";
import { DEFAULT_COL_WIDTH, HEADER_HEIGHT, HEADER_WIDTH } from "./grid-metrics";

/** The smallest chart that still fits a title, a plot and a legend. */
export const MIN_CHART = { width: 200, height: 140 };
/** Large enough for any screen; stops a runaway drag from making a huge box. */
export const MAX_CHART = { width: 2400, height: 1600 };

const widthOf = (colWidths: Record<string, number>, col: number) => colWidths[String(col)] ?? DEFAULT_COL_WIDTH;

/** Left edge of a column on the canvas, past the row headers. */
export function columnLeft(colWidths: Record<string, number>, col: number): number {
  let x = HEADER_WIDTH;
  for (let index = 0; index < col; index += 1) x += widthOf(colWidths, index);
  return x;
}

/** The column under an x coordinate; left of the first column is column 0. */
export function columnAtX(colWidths: Record<string, number>, x: number, maxCol = 16383): number {
  let left = HEADER_WIDTH;
  for (let col = 0; col < maxCol; col += 1) {
    left += widthOf(colWidths, col);
    if (x < left) return col;
  }
  return maxCol;
}

/** The cell a chart's top-left corner snaps to after a move. */
export function anchorCell(
  colWidths: Record<string, number>,
  rowAtY: (y: number) => number,
  x: number,
  y: number,
): { row: number; col: number } {
  return { row: rowAtY(Math.max(0, y - HEADER_HEIGHT)), col: columnAtX(colWidths, x) };
}

/** A chart's size after its corner handle moved by (dx, dy), within the limits. */
export function resizedChart(widthPx: number, heightPx: number, dx: number, dy: number) {
  const clamp = (value: number, min: number, max: number) => Math.round(Math.min(max, Math.max(min, value)));
  return {
    widthPx: clamp(widthPx + dx, MIN_CHART.width, MAX_CHART.width),
    heightPx: clamp(heightPx + dy, MIN_CHART.height, MAX_CHART.height),
  };
}

/** A chart placement change from the chart layer: new fields, or null to delete the chart. */
export type ChartPatch = Partial<Omit<ChartPlacement, "id">> | null;

/** The sheet's charts with one chart patched or removed. */
export function applyChartPatch(charts: ChartPlacement[], id: string, patch: ChartPatch): ChartPlacement[] {
  return patch
    ? charts.map((entry) => (entry.id === id ? { ...entry, ...patch } : entry))
    : charts.filter((entry) => entry.id !== id);
}
