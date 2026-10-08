import { describe, expect, it } from "vitest";
import { newSheet, newWorkbook, type Workbook } from "../../lib/office-types";
import { applyCellEdit, computeSheetValues } from "./cells";
import { planAutoSum } from "./autosum";
import { parseAddress } from "./formula";

function plan(
  cells: Record<string, string>,
  selection: { top: number; left: number; bottom: number; right: number },
  active = { row: selection.bottom, col: selection.right },
) {
  let workbook: Workbook = { ...newWorkbook("T"), sheets: [newSheet("S")] };
  for (const [address, text] of Object.entries(cells)) {
    const position = parseAddress(address)!;
    workbook = applyCellEdit(workbook, 0, position.row, position.col, text);
  }
  return planAutoSum(computeSheetValues(workbook, workbook.sheets[0]), selection, active);
}

const at = (address: string) => {
  const position = parseAddress(address)!;
  return { top: position.row, left: position.col, bottom: position.row, right: position.col };
};

describe("planAutoSum on a single cell", () => {
  it("sums the numbers directly above", () => {
    expect(plan({ B2: "1", B3: "2", B4: "3" }, at("B5"))).toEqual({
      kind: "write",
      edits: [{ row: 4, col: 1, text: "=SUM(B2:B4)" }],
    });
  });

  it("stops at a header or a blank above the run", () => {
    expect(plan({ B1: "Total", B2: "1", B3: "2" }, at("B4"))).toMatchObject({
      edits: [{ text: "=SUM(B2:B3)" }],
    });
    expect(plan({ B1: "9", B3: "2", B4: "3" }, at("B5"))).toMatchObject({ edits: [{ text: "=SUM(B3:B4)" }] });
  });

  it("names a single number without a range", () => {
    expect(plan({ B4: "5" }, at("B5"))).toMatchObject({ edits: [{ text: "=SUM(B4)" }] });
  });

  it("counts the results of formulas as numbers", () => {
    expect(plan({ B3: "=1+1", B4: "=B3*2" }, at("B5"))).toMatchObject({ edits: [{ text: "=SUM(B3:B4)" }] });
  });

  it("looks to the left when nothing numeric is above", () => {
    expect(plan({ A2: "1", B2: "2", C2: "3" }, at("D2"))).toEqual({
      kind: "write",
      edits: [{ row: 1, col: 3, text: "=SUM(A2:C2)" }],
    });
  });

  it("prefers the numbers above over those to the left", () => {
    expect(plan({ A5: "1", B3: "2", B4: "3" }, at("B5"))).toMatchObject({ edits: [{ text: "=SUM(B3:B4)" }] });
  });

  it("ends the run at a text label", () => {
    // A text label in the run ends it.
    expect(plan({ B3: "Q1", B4: "4" }, at("B5"))).toMatchObject({ edits: [{ text: "=SUM(B4)" }] });
  });

  it("opens an empty SUM for the user to complete when there is nothing to sum", () => {
    expect(plan({ A1: "label" }, at("C5"))).toEqual({ kind: "edit", row: 4, col: 2, text: "=SUM()", caret: 5 });
  });

  it("does not run past the top of the sheet", () => {
    expect(plan({ B1: "1", B2: "2" }, at("B3"))).toMatchObject({ edits: [{ text: "=SUM(B1:B2)" }] });
  });
});

describe("planAutoSum on a selected range", () => {
  it("puts the total of a column below it", () => {
    expect(plan({ A2: "1", A3: "2", A4: "3" }, { top: 1, left: 0, bottom: 3, right: 0 })).toEqual({
      kind: "write",
      edits: [{ row: 4, col: 0, text: "=SUM(A2:A4)" }],
    });
  });

  it("totals every column of a block in the row below", () => {
    const result = plan({ A2: "1", B2: "2", A3: "3", B3: "4" }, { top: 1, left: 0, bottom: 2, right: 1 });
    expect(result).toEqual({
      kind: "write",
      edits: [
        { row: 3, col: 0, text: "=SUM(A2:A3)" },
        { row: 3, col: 1, text: "=SUM(B2:B3)" },
      ],
    });
  });

  it("puts the total of a single row to its right", () => {
    expect(plan({ A2: "1", B2: "2" }, { top: 1, left: 0, bottom: 1, right: 1 })).toEqual({
      kind: "write",
      edits: [{ row: 1, col: 2, text: "=SUM(A2:B2)" }],
    });
  });
});
