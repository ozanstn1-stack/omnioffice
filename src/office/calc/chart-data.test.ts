import { describe, expect, it } from "vitest";
import { chartFromSelection } from "./chart-data";

const range = { start: { row: 0, col: 0 }, end: { row: 3, col: 2 } };
const header = (row: number, col: number) => ["Month", "Sales", "Costs"][col] ?? `${row}`;

describe("chartFromSelection", () => {
  it("reads categories from the first column and one series per other column", () => {
    const chart = chartFromSelection("column", range, header, "Chart")!;
    expect(chart.categories).toBe("A2:A4");
    expect(chart.series).toEqual([
      { name: "Sales", range: "B2:B4", color: null },
      { name: "Costs", range: "C2:C4", color: null },
    ]);
    expect(chart).toMatchObject({ kind: "column", title: "Chart", legend: true, stacked: false, showLabels: false });
    expect(chart.holeSize).toBeUndefined();
    expect(chart.scatterStyle).toBeUndefined();
  });

  it("needs a data row under the header", () => {
    expect(
      chartFromSelection("column", { start: { row: 2, col: 0 }, end: { row: 2, col: 3 } }, header, "Chart"),
    ).toBeNull();
  });

  it("names a series without a header", () => {
    const chart = chartFromSelection("line", range, () => "", "Chart")!;
    expect(chart.series.map((series) => series.name)).toEqual(["Series 1", "Series 2"]);
  });

  it("gives a doughnut the default hole of 50 percent and clamps a chosen one", () => {
    expect(chartFromSelection("doughnut", range, header, "Chart")!.holeSize).toBe(50);
    expect(chartFromSelection("doughnut", range, header, "Chart", { holeSize: 70 })!.holeSize).toBe(70);
    expect(chartFromSelection("doughnut", range, header, "Chart", { holeSize: 5 })!.holeSize).toBe(10);
    expect(chartFromSelection("pie", range, header, "Chart", { holeSize: 70 })!.holeSize).toBeUndefined();
  });

  it("keeps the first column as the X values of a scatter chart and stores its style", () => {
    const markers = chartFromSelection("scatter", range, header, "Chart")!;
    expect(markers.categories).toBe("A2:A4");
    expect(markers.series).toHaveLength(2);
    expect(markers.scatterStyle).toBeNull();
    expect(chartFromSelection("scatter", range, header, "Chart", { scatterStyle: "marker" })!.scatterStyle).toBeNull();
    expect(chartFromSelection("scatter", range, header, "Chart", { scatterStyle: "smoothMarker" })!.scatterStyle).toBe(
      "smoothMarker",
    );
  });
});
