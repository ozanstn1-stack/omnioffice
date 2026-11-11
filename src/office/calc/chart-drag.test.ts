import { describe, expect, it } from "vitest";
import { MAX_CHART, MIN_CHART, anchorCell, applyChartPatch, columnAtX, columnLeft, resizedChart } from "./chart-drag";
import { DEFAULT_COL_WIDTH, HEADER_HEIGHT, HEADER_WIDTH } from "./grid-metrics";

describe("chart move and resize (4.1b)", () => {
  const widths = { "1": 200 };

  it("maps columns to canvas x and back, honouring custom widths", () => {
    expect(columnLeft(widths, 0)).toBe(HEADER_WIDTH);
    expect(columnLeft(widths, 2)).toBe(HEADER_WIDTH + DEFAULT_COL_WIDTH + 200);
    expect(columnAtX(widths, 0)).toBe(0);
    expect(columnAtX(widths, HEADER_WIDTH + DEFAULT_COL_WIDTH + 199)).toBe(1);
    expect(columnAtX(widths, HEADER_WIDTH + DEFAULT_COL_WIDTH + 200)).toBe(2);
  });

  it("snaps a moved corner to the cell under it", () => {
    const rowAtY = (y: number) => Math.floor(y / 24);
    expect(anchorCell(widths, rowAtY, HEADER_WIDTH + 10, HEADER_HEIGHT + 24 * 5 + 3)).toEqual({ row: 5, col: 0 });
    // Above the first row stays on row 0.
    expect(anchorCell(widths, rowAtY, HEADER_WIDTH, 0)).toEqual({ row: 0, col: 0 });
  });

  it("keeps a resized chart within its limits", () => {
    expect(resizedChart(420, 260, 30.4, -20)).toEqual({ widthPx: 450, heightPx: 240 });
    expect(resizedChart(420, 260, -1000, -1000)).toEqual({ widthPx: MIN_CHART.width, heightPx: MIN_CHART.height });
    expect(resizedChart(420, 260, 1e5, 1e5)).toEqual({ widthPx: MAX_CHART.width, heightPx: MAX_CHART.height });
  });

  it("patches one chart's placement or removes it", () => {
    const base = { chart: {} as never, anchor: "A1", widthPx: 420, heightPx: 260 };
    const charts = [
      { id: "a", ...base },
      { id: "b", ...base },
    ];
    expect(applyChartPatch(charts, "b", { anchor: "C3" }).map((entry) => entry.anchor)).toEqual(["A1", "C3"]);
    expect(applyChartPatch(charts, "a", null).map((entry) => entry.id)).toEqual(["b"]);
  });
});
