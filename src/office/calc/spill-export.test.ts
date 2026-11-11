import { describe, expect, it } from "vitest";
import { newWorkbook, type Workbook } from "../../lib/office-types";
import { applyCellEdit } from "./cells";
import { spillRange, withSpillRanges } from "./spill-export";

function sheetWith(edits: Array<[number, number, string]>): Workbook {
  let workbook = newWorkbook();
  for (const [row, col, input] of edits) workbook = applyCellEdit(workbook, 0, row, col, input);
  return workbook;
}

describe("spill ranges on the saved copy (4.2b)", () => {
  it("gives a spilling formula the range it covers", () => {
    const workbook = sheetWith([
      [1, 1, "=SEQUENCE(3,2)"],
      [0, 5, "=1+1"],
    ]);
    const name = workbook.sheets[0].name;
    expect(spillRange(workbook, name, "B2")).toBe("B2:C4");
    expect(spillRange(workbook, name, "F1")).toBeNull();
  });

  it("marks only the saved copy and leaves the open workbook alone", () => {
    const workbook = sheetWith([
      [0, 0, "=SEQUENCE(3)"],
      [0, 3, "=1+1"],
    ]);
    const saved = withSpillRanges(workbook);
    expect(saved).not.toBe(workbook);
    expect(saved.sheets[0].cells.A1.spill).toBe("A1:A3");
    expect(saved.sheets[0].cells.D1.spill).toBeUndefined();
    expect(workbook.sheets[0].cells.A1.spill).toBeUndefined();
  });

  it("does not mark a blocked spill", () => {
    const workbook = sheetWith([
      [0, 0, "=SEQUENCE(3)"],
      [1, 0, "in the way"],
    ]);
    expect(withSpillRanges(workbook).sheets[0].cells.A1.spill ?? null).toBeNull();
  });

  it("returns the same workbook when nothing spills", () => {
    const workbook = sheetWith([[0, 0, "=1+1"]]);
    expect(withSpillRanges(workbook)).toBe(workbook);
  });
});
