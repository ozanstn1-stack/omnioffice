import { describe, expect, it } from "vitest";
import { newSheet, newWorkbook, type Sheet, type Workbook } from "../../lib/office-types";
import { applyCellEdit, computeSheetValues } from "./cells";
import {
  applyFilterDraft,
  clearFilter,
  conditionPredicate,
  NO_CONDITION,
  sheetFilterDraft,
  type FilterCondition,
  type FilterDraft,
} from "./filter";
import { FormulaError, parseAddress, type Scalar } from "./formula";

const condition = (op: FilterCondition["op"], value = "", value2 = ""): FilterCondition => ({ op, value, value2 });

function matches(c: FilterCondition, value: Scalar, column: Scalar[] = [value]): boolean {
  return conditionPredicate(c, column)(value);
}

describe("conditionPredicate", () => {
  it("lets everything through without a condition", () => {
    expect(matches(NO_CONDITION, "x")).toBe(true);
    expect(matches(NO_CONDITION, 5)).toBe(true);
  });

  it("matches text conditions without regard to case", () => {
    expect(matches(condition("contains", "APP"), "Pineapple")).toBe(true);
    expect(matches(condition("contains", "app"), "grape")).toBe(false);
    expect(matches(condition("begins", "pine"), "Pineapple")).toBe(true);
    expect(matches(condition("begins", "apple"), "Pineapple")).toBe(false);
    expect(matches(condition("ends", "APPLE"), "Pineapple")).toBe(true);
    expect(matches(condition("ends", "pine"), "Pineapple")).toBe(false);
    expect(matches(condition("equals", "pineapple"), "Pineapple")).toBe(true);
    expect(matches(condition("equals", "pine"), "Pineapple")).toBe(false);
  });

  it("reads numbers as their text for text conditions", () => {
    expect(matches(condition("contains", "2"), 1234)).toBe(true);
    expect(matches(condition("begins", "12"), 1234)).toBe(true);
  });

  it("does not filter on a text condition that has no text yet", () => {
    expect(matches(condition("contains", ""), "anything")).toBe(true);
  });

  it("compares numbers strictly for greater and less than", () => {
    expect(matches(condition("greater", "10"), 11)).toBe(true);
    expect(matches(condition("greater", "10"), 10)).toBe(false);
    expect(matches(condition("less", "10"), 9)).toBe(true);
    expect(matches(condition("less", "10"), 10)).toBe(false);
  });

  it("never matches text, blanks or errors on a number condition", () => {
    expect(matches(condition("greater", "1"), "50")).toBe(false);
    expect(matches(condition("greater", "1"), "")).toBe(false);
    expect(matches(condition("less", "99"), new FormulaError("#N/A"))).toBe(false);
  });

  it("includes both ends for between, in either order", () => {
    expect(matches(condition("between", "5", "10"), 5)).toBe(true);
    expect(matches(condition("between", "5", "10"), 10)).toBe(true);
    expect(matches(condition("between", "5", "10"), 11)).toBe(false);
    expect(matches(condition("between", "10", "5"), 7)).toBe(true);
  });

  it("accepts a comma decimal and ignores a number condition that is not a number", () => {
    expect(matches(condition("greater", "1,5"), 2)).toBe(true);
    expect(matches(condition("greater", "abc"), 2)).toBe(true);
    expect(matches(condition("between", "1", ""), 50)).toBe(true);
  });

  it("keeps the top N numbers of the column, ties included", () => {
    const column = [10, 40, 30, 40, 20, "text", ""];
    expect(matches(condition("top", "2"), 40, column)).toBe(true);
    expect(matches(condition("top", "2"), 30, column)).toBe(false);
    // Three items reach down to 30: the two 40s count as one item each.
    expect(matches(condition("top", "3"), 30, column)).toBe(true);
    expect(matches(condition("top", "3"), 20, column)).toBe(false);
    expect(matches(condition("top", "2"), "text", column)).toBe(false);
  });

  it("keeps the bottom N numbers", () => {
    const column = [10, 40, 30, 5, 20];
    expect(matches(condition("bottom", "2"), 5, column)).toBe(true);
    expect(matches(condition("bottom", "2"), 10, column)).toBe(true);
    expect(matches(condition("bottom", "2"), 20, column)).toBe(false);
  });

  it("keeps every number when N is larger than the column, and does not filter on a bad N", () => {
    expect(matches(condition("top", "99"), 1, [1, 2])).toBe(true);
    expect(matches(condition("top", "0"), 1, [1, 2])).toBe(true);
    expect(matches(condition("top", "x"), 1, [1, 2])).toBe(true);
  });

  it("matches blank and non-blank cells", () => {
    expect(matches(condition("blank"), "")).toBe(true);
    expect(matches(condition("blank"), "x")).toBe(false);
    expect(matches(condition("nonblank"), "x")).toBe(true);
    expect(matches(condition("nonblank"), 0)).toBe(true);
    expect(matches(condition("nonblank"), "")).toBe(false);
  });
});

/** A sheet with the header "Item | Qty" and the given rows. */
function table(rows: Array<[string, string]>): { workbook: Workbook; sheet: Sheet } {
  let workbook: Workbook = { ...newWorkbook("T"), sheets: [newSheet("S")] };
  const cells: Record<string, string> = { A1: "Item", B1: "Qty" };
  rows.forEach(([item, qty], index) => {
    cells[`A${index + 2}`] = item;
    cells[`B${index + 2}`] = qty;
  });
  for (const [address, text] of Object.entries(cells)) {
    const position = parseAddress(address)!;
    workbook = applyCellEdit(workbook, 0, position.row, position.col, text);
  }
  return { workbook, sheet: workbook.sheets[0] };
}

function draftFor(sheet: Sheet, workbook: Workbook, column: number, patch: Partial<FilterDraft> = {}): FilterDraft {
  return { ...sheetFilterDraft(sheet, computeSheetValues(workbook, sheet), column)!, ...patch };
}

describe("sheetFilterDraft", () => {
  it("lists the distinct values below the header, all checked, without a condition", () => {
    const { workbook, sheet } = table([
      ["a", "1"],
      ["b", "1"],
      ["c", "2"],
    ]);
    const draft = draftFor(sheet, workbook, 1);
    expect(draft.values).toEqual([
      { value: "1", checked: true },
      { value: "2", checked: true },
    ]);
    expect(draft.condition).toEqual(NO_CONDITION);
  });
});

describe("applyFilterDraft", () => {
  const rows: Array<[string, string]> = [
    ["apple", "5"],
    ["banana", "20"],
    ["cherry", "50"],
    ["apricot", "20"],
  ];

  function hidden(sheet: Sheet): number[] {
    return Object.entries(sheet.rowHeights)
      .filter(([, height]) => height === 0)
      .map(([row]) => Number(row))
      .sort((a, b) => a - b);
  }

  it("hides the rows whose value is unchecked, as before", () => {
    const { workbook, sheet } = table(rows);
    const draft = draftFor(sheet, workbook, 1);
    draft.values = draft.values.map((entry) => ({ ...entry, checked: entry.value !== "20" }));
    const next = applyFilterDraft(sheet, draft, computeSheetValues(workbook, sheet));
    expect(hidden(next)).toEqual([2, 4]);
    expect(next.filter).toEqual({ range: "A1:B5", column: 1, values: ["5", "50"] });
  });

  it("never hides the header row, which the value list does not contain", () => {
    const { workbook, sheet } = table(rows);
    const draft = draftFor(sheet, workbook, 1);
    draft.values = draft.values.map((entry) => ({ ...entry, checked: false }));
    const next = applyFilterDraft(sheet, draft, computeSheetValues(workbook, sheet));
    expect(hidden(next)).toEqual([1, 2, 3, 4]);
  });

  it("hides the rows that fail the condition", () => {
    const { workbook, sheet } = table(rows);
    const draft = draftFor(sheet, workbook, 0, { condition: condition("begins", "ap") });
    const next = applyFilterDraft(sheet, draft, computeSheetValues(workbook, sheet));
    expect(hidden(next)).toEqual([2, 3]);
    expect(next.filter?.values.sort()).toEqual(["apple", "apricot"]);
  });

  it("combines the value list and the condition with AND", () => {
    const { workbook, sheet } = table(rows);
    const draft = draftFor(sheet, workbook, 0, { condition: condition("begins", "ap") });
    draft.values = draft.values.map((entry) => ({ ...entry, checked: entry.value !== "apricot" }));
    const next = applyFilterDraft(sheet, draft, computeSheetValues(workbook, sheet));
    // banana and cherry fail the condition, apricot is unchecked: only apple stays.
    expect(hidden(next)).toEqual([2, 3, 4]);
    expect(next.filter?.values).toEqual(["apple"]);
  });

  it("filters numbers with a number condition and the top N of the column", () => {
    const { workbook, sheet } = table(rows);
    const between = applyFilterDraft(
      sheet,
      draftFor(sheet, workbook, 1, { condition: condition("between", "10", "30") }),
      computeSheetValues(workbook, sheet),
    );
    expect(hidden(between)).toEqual([1, 3]);
    const top = applyFilterDraft(
      sheet,
      draftFor(sheet, workbook, 1, { condition: condition("top", "1") }),
      computeSheetValues(workbook, sheet),
    );
    expect(hidden(top)).toEqual([1, 2, 4]);
  });

  it("shows the rows again that a looser condition lets through", () => {
    const { workbook, sheet } = table(rows);
    const values = computeSheetValues(workbook, sheet);
    const narrow = applyFilterDraft(
      sheet,
      draftFor(sheet, workbook, 0, { condition: condition("equals", "apple") }),
      values,
    );
    expect(hidden(narrow)).toEqual([2, 3, 4]);
    const wide = applyFilterDraft(
      narrow,
      draftFor(sheet, workbook, 0, { condition: condition("contains", "a") }),
      values,
    );
    // Only "cherry" has no "a".
    expect(hidden(wide)).toEqual([3]);
  });

  it("filters blank and non-blank cells", () => {
    const { workbook, sheet } = table([
      ["a", "1"],
      ["", "2"],
      ["c", "3"],
    ]);
    const values = computeSheetValues(workbook, sheet);
    expect(
      hidden(applyFilterDraft(sheet, draftFor(sheet, workbook, 0, { condition: condition("blank") }), values)),
    ).toEqual([1, 3]);
    expect(
      hidden(applyFilterDraft(sheet, draftFor(sheet, workbook, 0, { condition: condition("nonblank") }), values)),
    ).toEqual([2]);
  });
});

describe("clearFilter", () => {
  it("drops the hidden rows and the sheet filter", () => {
    const sheet: Sheet = {
      ...newSheet("S"),
      rowHeights: { "1": 0 },
      filter: { range: "A1:A3", column: 0, values: [] },
    };
    expect(clearFilter(sheet)).toMatchObject({ rowHeights: {}, filter: null });
  });
});
