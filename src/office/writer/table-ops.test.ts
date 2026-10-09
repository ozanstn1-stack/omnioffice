import { describe, expect, it } from "vitest";
import type { Block, TableCell, TableData } from "../../lib/office-types";
import { cellAt, coveredBy, gridWidth, isRectangular, mergeCells, splitCell } from "./table-ops";

function paragraph(text: string): Block {
  return {
    type: "paragraph",
    props: {
      style: "Normal",
      align: "left",
      lineSpacing: 1.15,
      spaceBeforePt: 0,
      spaceAfterPt: 8,
      indentLeftPt: 0,
      indentRightPt: 0,
      firstLinePt: 0,
      list: null,
      pageBreakBefore: false,
    },
    runs: [
      {
        text,
        bold: false,
        italic: false,
        underline: false,
        strike: false,
        color: null,
        highlight: null,
        font: null,
        sizePt: null,
        link: null,
        comment: null,
        superscript: false,
        subscript: false,
      },
    ],
  };
}

function cell(text: string, colspan = 1, rowspan = 1): TableCell {
  return { blocks: [paragraph(text)], colspan, rowspan, background: null, align: "left", valign: "top", widthPt: null };
}

function table(rows: TableCell[][]): TableData {
  return {
    rows: rows.map((cells) => ({ cells, heightPt: null, header: false })),
    columnWidthsPt: rows[0]?.map(() => 100) ?? [],
    borders: true,
    borderColor: "#000000",
    align: "left",
  };
}

function cellText(current: TableCell): string {
  return current.blocks
    .map((block) => (block.type === "paragraph" ? block.runs.map((run) => run.text).join("") : ""))
    .join("|");
}

describe("table grid helpers", () => {
  it("maps grid positions through colspans and rowspans", () => {
    const grid = table([
      [cell("A", 2, 1), cell("B", 1, 1)],
      [cell("C", 1, 1), cell("D", 1, 1), cell("E", 1, 1)],
    ]);
    expect(gridWidth(grid)).toBe(3);
    expect(coveredBy(grid, 0, 1)).toEqual({ row: 0, cell: 0 });
    expect(coveredBy(grid, 1, 2)).toEqual({ row: 1, cell: 2 });
    expect(cellAt(grid, 0, 1)?.data).toBe(grid.rows[0].cells[0]);
    expect(coveredBy(grid, 0, 9)).toBeNull();
  });

  it("keeps columns aligned when a rowspan from above occupies a slot", () => {
    const grid = table([
      [cell("A", 1, 2), cell("B")],
      [cell("C"), cell("D")],
    ]);
    expect(coveredBy(grid, 1, 0)).toEqual({ row: 0, cell: 0 });
    expect(coveredBy(grid, 1, 1)).toEqual({ row: 1, cell: 0 });
    expect(coveredBy(grid, 1, 2)).toEqual({ row: 1, cell: 1 });
  });
});

describe("mergeCells", () => {
  it("merges a 2x2 rectangle and concatenates the covered contents", () => {
    const grid = table([
      [cell("A1"), cell("B1")],
      [cell("A2"), cell("B2")],
    ]);
    const merged = mergeCells(grid, { row0: 0, col0: 0, row1: 1, col1: 1 });
    expect(merged.rows).toHaveLength(2);
    expect(merged.rows[0].cells).toHaveLength(1);
    expect(merged.rows[1].cells).toHaveLength(0);
    const origin = merged.rows[0].cells[0];
    expect(origin.colspan).toBe(2);
    expect(origin.rowspan).toBe(2);
    expect(origin.blocks.map((block) => (block.type === "paragraph" ? block.runs[0].text : "?"))).toEqual([
      "A1",
      "",
      "B1",
      "",
      "A2",
      "",
      "B2",
    ]);
  });

  it("merges a single row segment and keeps the other cells in place", () => {
    const grid = table([[cell("A"), cell("B"), cell("C")]]);
    const merged = mergeCells(grid, { row0: 0, col0: 0, row1: 0, col1: 1 });
    expect(merged.rows[0].cells.map((entry) => entry.colspan)).toEqual([2, 1]);
    expect(cellText(merged.rows[0].cells[1])).toBe("C");
  });

  it("rejects a single cell, a non-rectangle and out-of-range selections", () => {
    const grid = table([
      [cell("A1"), cell("B1")],
      [cell("A2"), cell("B2")],
    ]);
    expect(mergeCells(grid, { row0: 0, col0: 0, row1: 0, col1: 0 })).toBe(grid);
    expect(isRectangular(grid, { row0: 0, col0: 0, row1: 0, col1: 0 })).toBe(false);
    expect(mergeCells(grid, { row0: 0, col0: 0, row1: 2, col1: 1 })).toBe(grid);

    // A selection cutting through a colspan is not a rectangle.
    const wide = table([[cell("W", 2), cell("X")]]);
    expect(isRectangular(wide, { row0: 0, col0: 1, row1: 0, col1: 2 })).toBe(false);
    expect(mergeCells(wide, { row0: 0, col0: 1, row1: 0, col1: 2 })).toBe(wide);
  });

  it("expands columnWidthsPt when the grid is wider than the array", () => {
    const grid = table([[cell("A"), cell("B")]]);
    grid.columnWidthsPt = [100];
    const merged = mergeCells(grid, { row0: 0, col0: 0, row1: 0, col1: 1 });
    expect(merged.columnWidthsPt).toEqual([100, 100]);
  });
});

describe("splitCell", () => {
  it("splits a 2x2 merge back into four 1x1 cells with empty leftovers", () => {
    const grid = table([
      [cell("A1"), cell("B1")],
      [cell("A2"), cell("B2")],
    ]);
    const merged = mergeCells(grid, { row0: 0, col0: 0, row1: 1, col1: 1 });
    const split = splitCell(merged, 0, 0);
    expect(split.rows[0].cells.map((entry) => entry.colspan)).toEqual([1, 1]);
    expect(split.rows[1].cells.map((entry) => entry.colspan)).toEqual([1, 1]);
    expect(cellText(split.rows[0].cells[0])).toBe("A1||B1||A2||B2");
    expect(split.rows[0].cells[1].blocks).toEqual([]);
    expect(split.rows[1].cells[0].blocks).toEqual([]);
    expect(gridWidth(split)).toBe(2);
  });

  it("splits a rowspan and keeps each row's grid width", () => {
    const grid = table([[cell("T", 1, 2), cell("B")], [cell("C")]]);
    const split = splitCell(grid, 0, 0);
    expect(split.rows[0].cells).toHaveLength(2);
    expect(split.rows[1].cells).toHaveLength(2);
    expect(coveredBy(split, 1, 0)).toEqual({ row: 1, cell: 0 });
    expect(gridWidth(split)).toBe(2);
  });

  it("is a no-op for a plain cell and out-of-range indices", () => {
    const grid = table([[cell("A"), cell("B")]]);
    expect(splitCell(grid, 0, 0)).toBe(grid);
    expect(splitCell(grid, 5, 0)).toBe(grid);
    expect(splitCell(grid, 0, 9)).toBe(grid);
  });
});
