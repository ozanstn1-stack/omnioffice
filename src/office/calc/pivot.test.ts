/**
 * Pivot table tests: aggregation, grouping, filters and the values the XLSX
 * exporter materialises.
 */
import { describe, expect, it } from "vitest";
import { newSheet, newWorkbook, type PivotTable, type Workbook } from "../../lib/office-types";
import { applyCellEdit, computeSheetValues } from "./cells";
import { computePivot, defaultPivot, pivotFields, pivotValues } from "./pivot";

function dataBook(): Workbook {
  const workbook = { ...newWorkbook("Pivot"), sheets: [newSheet("Data")] };
  let wb = workbook;
  const rows: string[][] = [
    ["Department", "Year", "Sales", "Region"],
    ["Hardware", "2025", "100", "North"],
    ["Hardware", "2025", "150", "South"],
    ["Software", "2025", "200", "North"],
    ["Hardware", "2026", "50", "North"],
    ["Software", "2026", "300", "South"],
  ];
  rows.forEach((row, rowIndex) => {
    row.forEach((value, colIndex) => {
      wb = applyCellEdit(wb, 0, rowIndex, colIndex, value);
    });
  });
  return wb;
}

function pivotOf(overrides: Partial<PivotTable> = {}): PivotTable {
  return {
    id: "p1",
    name: "Pivot",
    sourceSheet: "Data",
    source: "A1:D6",
    rows: ["Department"],
    columns: ["Year"],
    values: [{ field: "Sales", aggregation: "sum" }],
    filters: [],
    anchor: "F1",
    ...overrides,
  };
}

function sheetValues(workbook: Workbook) {
  return computeSheetValues(workbook, workbook.sheets[0]);
}

describe("pivot computation", () => {
  it("groups rows by the row field and columns by the column field", () => {
    const workbook = dataBook();
    const grid = computePivot(workbook, pivotOf())!;
    expect(grid).not.toBeNull();
    expect(grid.grid).toEqual([
      ["Department", "2025", "2026"],
      ["Hardware", 250, 50],
      ["Software", 200, 300],
    ]);
  });

  it("supports count, average, min and max aggregations", () => {
    const workbook = dataBook();
    const count = computePivot(workbook, pivotOf({ columns: [], values: [{ field: "Sales", aggregation: "count" }] }))!;
    expect(count.grid).toEqual([
      ["Department", "Sales (count)"],
      ["Hardware", 3],
      ["Software", 2],
    ]);
    const average = computePivot(
      workbook,
      pivotOf({ columns: [], rows: ["Region"], values: [{ field: "Sales", aggregation: "average" }] }),
    )!;
    expect(average.grid[1][1]).toBeCloseTo((100 + 200 + 50) / 3, 10);
    expect(average.grid[2][1]).toBeCloseTo(225, 10);
    const extremes = computePivot(
      workbook,
      pivotOf({ columns: [], values: [{ field: "Sales", aggregation: "min" }] }),
    )!;
    expect(extremes.grid[1][1]).toBe(50);
    const max = computePivot(workbook, pivotOf({ columns: [], values: [{ field: "Sales", aggregation: "max" }] }))!;
    expect(max.grid[1][1]).toBe(150);
  });

  it("applies filters to the source rows", () => {
    const workbook = dataBook();
    const filtered = computePivot(workbook, pivotOf({ filters: [{ field: "Region", values: ["North"] }] }))!;
    expect(filtered.grid).toEqual([
      ["Department", "2025", "2026"],
      ["Hardware", 100, 50],
      ["Software", 200, ""],
    ]);
  });

  it("lists the source fields from the header row", () => {
    expect(pivotFields(dataBook(), "Data", "A1:D6")).toEqual(["Department", "Year", "Sales", "Region"]);
  });

  it("renders two value fields side by side", () => {
    const workbook = dataBook();
    const grid = computePivot(
      workbook,
      pivotOf({
        columns: [],
        rows: ["Department"],
        values: [
          { field: "Sales", aggregation: "sum" },
          { field: "Sales", aggregation: "count" },
        ],
      }),
    )!;
    expect(grid.grid).toEqual([
      ["Department", "Sales (sum)", "Sales (count)"],
      ["Hardware", 300, 3],
      ["Software", 500, 2],
    ]);
  });

  it("returns null for a definition that cannot be read", () => {
    const workbook = dataBook();
    expect(computePivot(workbook, pivotOf({ values: [] }))).toBeNull();
    expect(computePivot(workbook, pivotOf({ rows: [], columns: [] }))).toBeNull();
    expect(computePivot(workbook, pivotOf({ values: [{ field: "Nope", aggregation: "sum" }] }))).toBeNull();
  });

  it("materialises the grid as plain values for export", () => {
    const workbook = dataBook();
    const values = pivotValues(workbook, pivotOf());
    expect(values[0]).toEqual(["Department", "2025", "2026"]);
    expect(values[1]).toEqual(["Hardware", 250, 50]);
    expect(sheetValues(workbook).get("A1")).toBe("Department");
  });

  it("builds a default definition from the source headers", () => {
    const workbook = dataBook();
    const pivot = defaultPivot(workbook, "Data", "A1:D6", "F1");
    expect(pivot).not.toBeNull();
    expect(pivot!.rows).toEqual(["Department"]);
    expect(pivot!.values).toEqual([{ field: "Sales", aggregation: "sum" }]);
    expect(computePivot(workbook, pivot!)).not.toBeNull();
  });
});
