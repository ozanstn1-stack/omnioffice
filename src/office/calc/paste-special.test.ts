import { describe, expect, it } from "vitest";
import {
  cellText,
  defaultCellStyle,
  newSheet,
  newWorkbook,
  type CellStyle,
  type Workbook,
} from "../../lib/office-types";
import { applyCellEdit, computeSheetValues, computeWorkbookValues } from "./cells";
import { parseAddress } from "./formula";
import { clipboardFromText, clipboardText, pasteSpecial, snapshotClipboard, type PasteContent } from "./paste-special";

function seed(cells: Record<string, string>): Workbook {
  let workbook: Workbook = { ...newWorkbook("T"), sheets: [newSheet("S")] };
  for (const [address, text] of Object.entries(cells)) {
    const position = parseAddress(address)!;
    workbook = applyCellEdit(workbook, 0, position.row, position.col, text);
  }
  return workbook;
}

function style(patch: Partial<CellStyle>): CellStyle {
  return { ...defaultCellStyle(), ...patch };
}

function withStyle(workbook: Workbook, address: string, patch: Partial<CellStyle>): Workbook {
  const sheet = workbook.sheets[0];
  const cell = sheet.cells[address];
  return {
    ...workbook,
    sheets: [
      { ...sheet, cells: { ...sheet.cells, [address]: { ...cell, style: style({ ...cell.style, ...patch }) } } },
    ],
  };
}

const range = (top: number, left: number, bottom: number, right: number) => ({ top, left, bottom, right });

function paste(
  workbook: Workbook,
  source: ReturnType<typeof range>,
  target: string,
  content: PasteContent,
  transpose = false,
) {
  const sheet = workbook.sheets[0];
  const data = snapshotClipboard(sheet, computeSheetValues(workbook, sheet), source);
  const at = parseAddress(target)!;
  const next = pasteSpecial(workbook, 0, data, at, { content, transpose });
  return { next, cells: next.sheets[0].cells, text: (address: string) => cellText(next.sheets[0].cells[address]) };
}

describe("snapshotClipboard", () => {
  it("records the cells, their computed values and the origin", () => {
    const workbook = seed({ A1: "2", B1: "=A1*3", A2: "x" });
    const sheet = workbook.sheets[0];
    const data = snapshotClipboard(sheet, computeSheetValues(workbook, sheet), range(0, 0, 1, 1));
    expect(data.origin).toEqual({ row: 0, col: 0 });
    expect(data.values).toEqual([
      [2, 6],
      ["x", ""],
    ]);
    expect(data.cells[0][1]?.formula).toBe("=A1*3");
    expect(data.cells[1][1]).toBeUndefined();
  });
});

describe("clipboardFromText and clipboardText", () => {
  it("reads tab separated rows as values", () => {
    const data = clipboardFromText("a\t1\r\nb\t2\n");
    expect(data.values).toEqual([
      ["a", "1"],
      ["b", "2"],
    ]);
    expect(data.cells).toEqual([
      [undefined, undefined],
      [undefined, undefined],
    ]);
  });

  it("writes values back as the text other applications get", () => {
    const workbook = seed({ A1: "a", B1: "=1+1", A2: "TRUE" });
    const sheet = workbook.sheets[0];
    const data = snapshotClipboard(sheet, computeSheetValues(workbook, sheet), range(0, 0, 1, 1));
    expect(clipboardText(data)).toBe("a\t2\ntrue\t");
  });
});

describe("pasteSpecial values", () => {
  it("pastes what the cells show, not their formulas, and leaves the target formatting", () => {
    let workbook = seed({ A1: "2", B1: "=A1*3", D1: "old" });
    workbook = withStyle(workbook, "D1", { bold: true });
    const { cells, text } = paste(workbook, range(0, 1, 0, 1), "D1", "values");
    expect(cells.D1.formula).toBeNull();
    expect(cells.D1.value).toEqual({ kind: "number", value: 6 });
    expect(text("D1")).toBe("6");
    expect(cells.D1.style.bold).toBe(true);
  });

  it("clears the target where the source is empty", () => {
    const workbook = seed({ A1: "1", D1: "x", E1: "y" });
    const { cells } = paste(workbook, range(0, 0, 0, 1), "D1", "values");
    expect(cells.D1.value).toEqual({ kind: "number", value: 1 });
    expect(cells.E1).toBeUndefined();
  });
});

describe("pasteSpecial formulas", () => {
  it("shifts relative references by the distance and keeps absolute ones", () => {
    const workbook = seed({ A1: "1", A2: "2", B1: "=A1+$A$2" });
    const { text, cells } = paste(workbook, range(0, 1, 0, 1), "B2", "formulas");
    expect(text("B2")).toBe("=A2+$A$2");
    // The pasted formula is evaluated: A2 (2) + $A$2 (2).
    expect(cells.B2.value).toEqual({ kind: "number", value: 4 });
  });

  it("shifts columns as well as rows", () => {
    const workbook = seed({ A1: "5", B1: "=A1*2" });
    const { text } = paste(workbook, range(0, 1, 0, 1), "D3", "formulas");
    expect(text("D3")).toBe("=C3*2");
  });

  it("pastes constants as constants and keeps the target's own formatting", () => {
    let workbook = seed({ A1: "text", D1: "old" });
    workbook = withStyle(workbook, "A1", { italic: true });
    workbook = withStyle(workbook, "D1", { bold: true });
    const { cells } = paste(workbook, range(0, 0, 0, 0), "D1", "formulas");
    expect(cells.D1.value).toEqual({ kind: "text", value: "text" });
    expect(cells.D1.style.bold).toBe(true);
    expect(cells.D1.style.italic).toBe(false);
  });

  it("recalculates the cells that read a pasted formula", () => {
    const workbook = seed({ A1: "1", A2: "10", B1: "=A1", C1: "=B2*2" });
    const { next } = paste(workbook, range(0, 1, 0, 1), "B2", "formulas");
    // B2 now holds =A2 (10), so C1 = B2 * 2 = 20.
    expect(computeWorkbookValues(next).get("S!C1")).toBe(20);
  });
});

describe("pasteSpecial formats", () => {
  it("copies the formatting and keeps the target's content", () => {
    let workbook = seed({ A1: "src", D1: "keep" });
    workbook = withStyle(workbook, "A1", { bold: true, fill: "#ffff00", numberFormat: "0.00" });
    const { cells } = paste(workbook, range(0, 0, 0, 0), "D1", "formats");
    expect(cells.D1.value).toEqual({ kind: "text", value: "keep" });
    expect(cells.D1.style).toMatchObject({ bold: true, fill: "#ffff00", numberFormat: "0.00" });
  });

  it("applies formatting to empty target cells", () => {
    let workbook = seed({ A1: "src" });
    workbook = withStyle(workbook, "A1", { bold: true });
    const { cells } = paste(workbook, range(0, 0, 0, 0), "C3", "formats");
    expect(cells.C3.style.bold).toBe(true);
    expect(cells.C3.value).toEqual({ kind: "empty" });
  });

  it("resets the target to the default formatting where the source has none", () => {
    let workbook = seed({ A1: "plain", D1: "styled" });
    workbook = withStyle(workbook, "D1", { bold: true });
    const { cells } = paste(workbook, range(0, 0, 0, 1), "D1", "formats");
    expect(cells.D1.style.bold).toBe(false);
    // E1 had nothing and gets nothing.
    expect(cells.E1).toBeUndefined();
  });

  it("leaves a formula in the target alone", () => {
    let workbook = seed({ A1: "src", D1: "=1+1" });
    workbook = withStyle(workbook, "A1", { italic: true });
    const { cells } = paste(workbook, range(0, 0, 0, 0), "D1", "formats");
    expect(cells.D1.formula).toBe("=1+1");
    expect(cells.D1.value).toEqual({ kind: "number", value: 2 });
    expect(cells.D1.style.italic).toBe(true);
  });
});

describe("pasteSpecial all", () => {
  it("pastes formulas and formatting together", () => {
    let workbook = seed({ A1: "3", B1: "=A1*2" });
    workbook = withStyle(workbook, "B1", { bold: true });
    const { cells, text } = paste(workbook, range(0, 1, 0, 1), "B2", "all");
    expect(text("B2")).toBe("=A2*2");
    expect(cells.B2.style.bold).toBe(true);
  });
});

describe("pasteSpecial transpose", () => {
  it("swaps rows and columns", () => {
    const workbook = seed({ A1: "a", B1: "b", C1: "c", A2: "d", B2: "e", C2: "f" });
    const { text, cells } = paste(workbook, range(0, 0, 1, 2), "E1", "values", true);
    expect([text("E1"), text("F1")]).toEqual(["a", "d"]);
    expect([text("E2"), text("F2")]).toEqual(["b", "e"]);
    expect([text("E3"), text("F3")]).toEqual(["c", "f"]);
    expect(cells.G1).toBeUndefined();
  });

  it("transposes the formatting with the cells", () => {
    let workbook = seed({ A1: "a", B1: "b" });
    workbook = withStyle(workbook, "B1", { bold: true });
    const { cells } = paste(workbook, range(0, 0, 0, 1), "D1", "formats", true);
    expect(cells.D2.style.bold).toBe(true);
    expect(cells.D1?.style.bold ?? false).toBe(false);
  });

  it("moves formulas by their displacement", () => {
    const workbook = seed({ A1: "1", B1: "=A1+1" });
    const { text } = paste(workbook, range(0, 1, 0, 1), "D2", "formulas", true);
    // B1 (row 0, col 1) lands on D2 (row 1, col 3): +1 row, +2 columns.
    expect(text("D2")).toBe("=C2+1");
  });
});

describe("pasteSpecial limits", () => {
  it("returns the same workbook for an empty clipboard", () => {
    const workbook = seed({ A1: "1" });
    expect(
      pasteSpecial(workbook, 0, clipboardFromText(""), { row: 0, col: 0 }, { content: "all", transpose: false }),
    ).toBe(workbook);
  });

  it("works from text copied elsewhere: values only, transpose still applies", () => {
    const workbook = seed({});
    const data = clipboardFromText("1\t2\n3\t4");
    const next = pasteSpecial(workbook, 0, data, { row: 0, col: 0 }, { content: "formulas", transpose: true });
    const text = (address: string) => cellText(next.sheets[0].cells[address]);
    expect([text("A1"), text("B1"), text("A2"), text("B2")]).toEqual(["1", "3", "2", "4"]);
  });

  it("grows the sheet when the paste reaches past its edge", () => {
    const workbook = seed({ A1: "x" });
    const rows = workbook.sheets[0].rowCount;
    const { next } = paste(workbook, range(0, 0, 0, 0), `A${rows + 5}`, "values");
    expect(next.sheets[0].rowCount).toBeGreaterThan(rows);
  });
});
