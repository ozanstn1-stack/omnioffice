/** Builds the chart model for the selection the Insert chart dialog starts from. */
import type { ChartData } from "../../lib/office-types";
import { DEFAULT_HOLE_SIZE, holeSizeOf } from "./chart-geometry";
import { formatAddress, type RangeParts } from "./formula";

/** What the dialog adds to the kind. */
export interface ChartOptions {
  /** Doughnut hole, as a percentage of the radius. */
  holeSize?: number;
  /** Scatter flavour (`c:scatterStyle`); `marker` or absent is markers only. */
  scatterStyle?: string | null;
}

/**
 * The first column is the categories (the X values of a scatter chart), the
 * first row names the series, every other column is a series. Null when the
 * selection has no data row under the header.
 */
export function chartFromSelection(
  kind: string,
  range: RangeParts,
  headerText: (row: number, col: number) => string,
  title: string,
  options: ChartOptions = {},
): ChartData | null {
  const { start, end } = range;
  if (start.row === end.row) return null;
  const series: ChartData["series"] = [];
  for (let col = start.col + 1; col <= end.col; col += 1) {
    series.push({
      name: headerText(start.row, col) || `Series ${col}`,
      range: `${formatAddress(start.row + 1, col)}:${formatAddress(end.row, col)}`,
      color: null,
    });
  }
  const chart: ChartData = {
    kind,
    title,
    categories: `${formatAddress(start.row + 1, start.col)}:${formatAddress(end.row, start.col)}`,
    series,
    legend: true,
    xTitle: "",
    yTitle: "",
    stacked: false,
    showLabels: false,
  };
  if (kind === "doughnut") chart.holeSize = holeSizeOf({ holeSize: options.holeSize ?? DEFAULT_HOLE_SIZE });
  if (kind === "scatter") {
    chart.scatterStyle = options.scatterStyle && options.scatterStyle !== "marker" ? options.scatterStyle : null;
  }
  return chart;
}
