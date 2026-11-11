import { describe, expect, it } from "vitest";
import type { SpreadsheetTable } from "../../../lib/office-types";
import { BAND_FILL, tableLook, type PlacedTable } from "./cell-look";

function placed(overrides: Partial<SpreadsheetTable> = {}): PlacedTable {
  return {
    table: {
      id: "t1",
      name: "Table1",
      range: "B2:C7",
      hasHeaders: true,
      hasTotals: true,
      bandedRows: true,
      bandedColumns: false,
      headerFill: "#1d4ed8",
      headerBold: true,
      styleName: "TableStyleMedium2",
      columns: [],
      filter: null,
      ...overrides,
    },
    // B2:C7 -> rows 1..6, columns 1..2 (zero-based)
    parts: { start: { row: 1, col: 1 }, end: { row: 6, col: 2 } },
  };
}

describe("Calc table look (3.3h)", () => {
  it("gives the header row the header fill and marks it", () => {
    expect(tableLook([placed()], 1, 1)).toMatchObject({ fill: "#1d4ed8", header: true });
  });

  it("bands every other body row and never the totals row", () => {
    const tables = [placed()];
    expect(tableLook(tables, 2, 2).fill).toBeUndefined();
    expect(tableLook(tables, 3, 2).fill).toBe(BAND_FILL);
    expect(tableLook(tables, 5, 1).fill).toBe(BAND_FILL);
    expect(tableLook(tables, 6, 1)).toMatchObject({ fill: undefined, header: false });
  });

  it("starts the bands on the first row of a table without headers", () => {
    const tables = [placed({ hasHeaders: false, hasTotals: false })];
    expect(tableLook(tables, 1, 1)).toMatchObject({ fill: undefined, header: false });
    expect(tableLook(tables, 2, 1).fill).toBe(BAND_FILL);
  });

  it("leaves cells outside every table alone", () => {
    expect(tableLook([placed()], 0, 0)).toEqual({ entry: undefined, fill: undefined, header: false });
    expect(tableLook([placed()], 3, 3).entry).toBeUndefined();
  });
});
