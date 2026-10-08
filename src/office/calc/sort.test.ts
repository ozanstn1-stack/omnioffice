import { describe, expect, it } from "vitest";
import { cellText, newSheet, newWorkbook, type Sheet, type Workbook } from "../../lib/office-types";
import { applyCellEdit, applySheetCells, computeSheetValues, computeWorkbookValues } from "./cells";
import { FormulaError, parseAddress, type Scalar } from "./formula";
import { compareValues, guessHeaders, planSort, rangeHasMerges, sortContext, sortOrder } from "./sort";

/** A one-sheet workbook from `{ A1: "typed text or =formula" }`. */
function seed(values: Record<string, string>): Workbook {
  let workbook: Workbook = { ...newWorkbook("Test"), sheets: [newSheet("S")] };
  for (const [address, text] of Object.entries(values)) {
    const position = parseAddress(address)!;
    workbook = applyCellEdit(workbook, 0, position.row, position.col, text);
  }
  return workbook;
}

function sorted(
  values: Record<string, string>,
  range: { top: number; left: number; bottom: number; right: number },
  levels: Array<{ column: number; ascending: boolean }>,
  options: { hasHeaders?: boolean; isHidden?: (row: number) => boolean; patch?: (sheet: Sheet) => Sheet } = {},
) {
  const workbook = seed(values);
  const sheet = options.patch ? options.patch(workbook.sheets[0]) : workbook.sheets[0];
  const plan = planSort(sheet, computeSheetValues(workbook, sheet), range, levels, {
    hasHeaders: options.hasHeaders ?? false,
    isHidden: options.isHidden,
  });
  return { plan, text: (address: string) => (plan ? cellText(plan.cells[address]) : cellText(sheet.cells[address])) };
}

describe("compareValues", () => {
  it("orders numbers numerically", () => {
    expect(compareValues(2, 10, true)).toBeLessThan(0);
    expect(compareValues(2, 10, false)).toBeGreaterThan(0);
    expect(compareValues(3, 3, true)).toBe(0);
  });

  it("orders text without regard to case", () => {
    expect(compareValues("apple", "Banana", true)).toBeLessThan(0);
    expect(compareValues("Apple", "apple", true)).toBe(0);
    expect(compareValues("b", "A", false)).toBeLessThan(0);
  });

  it("puts numbers before text before logical values before errors when ascending", () => {
    const ordered: Scalar[] = [5, "text", true, new FormulaError("#N/A")];
    for (let at = 0; at < ordered.length - 1; at += 1) {
      expect(compareValues(ordered[at], ordered[at + 1], true)).toBeLessThan(0);
      expect(compareValues(ordered[at], ordered[at + 1], false)).toBeGreaterThan(0);
    }
  });

  it("keeps FALSE before TRUE and sorts errors by code", () => {
    expect(compareValues(false, true, true)).toBeLessThan(0);
    expect(compareValues(new FormulaError("#DIV/0!"), new FormulaError("#N/A"), true)).toBeLessThan(0);
  });

  it("puts empty values last in both directions", () => {
    for (const ascending of [true, false]) {
      expect(compareValues("", 1, ascending)).toBeGreaterThan(0);
      expect(compareValues("z", "", ascending)).toBeLessThan(0);
      expect(compareValues("", "", ascending)).toBe(0);
    }
  });

  it("keeps numeric text apart from numbers", () => {
    expect(compareValues(100, "20", true)).toBeLessThan(0);
  });
});

describe("sortOrder", () => {
  it("returns the source row for each target position", () => {
    expect(sortOrder([[3], [1], [2]], [{ offset: 0, ascending: true }])).toEqual([1, 2, 0]);
    expect(sortOrder([[3], [1], [2]], [{ offset: 0, ascending: false }])).toEqual([0, 2, 1]);
  });

  it("is stable: equal keys keep their order", () => {
    const rows = [
      ["b", 1],
      ["a", 2],
      ["b", 3],
      ["a", 4],
    ] as Scalar[][];
    expect(sortOrder(rows, [{ offset: 0, ascending: true }])).toEqual([1, 3, 0, 2]);
    // Stability holds for descending too: ties are not reversed.
    expect(sortOrder(rows, [{ offset: 0, ascending: false }])).toEqual([0, 2, 1, 3]);
  });

  it("breaks ties with the next level, each in its own direction", () => {
    const rows = [
      ["x", 1],
      ["y", 5],
      ["x", 3],
      ["y", 2],
    ] as Scalar[][];
    expect(
      sortOrder(rows, [
        { offset: 0, ascending: true },
        { offset: 1, ascending: false },
      ]),
    ).toEqual([2, 0, 1, 3]);
  });

  it("supports three levels", () => {
    const rows = [
      ["a", "x", 2],
      ["a", "x", 1],
      ["a", "w", 9],
      ["b", "a", 0],
    ] as Scalar[][];
    expect(
      sortOrder(rows, [
        { offset: 0, ascending: true },
        { offset: 1, ascending: true },
        { offset: 2, ascending: true },
      ]),
    ).toEqual([2, 1, 0, 3]);
  });

  it("sorts mixed columns numbers, then text, then empty", () => {
    const rows = [["pear"], [""], [10], ["Apple"], [2]] as Scalar[][];
    const order = sortOrder(rows, [{ offset: 0, ascending: true }]);
    expect(order.map((index) => rows[index][0])).toEqual([2, 10, "Apple", "pear", ""]);
  });

  it("leaves a sorted input in place", () => {
    expect(sortOrder([[1], [2], [3]], [{ offset: 0, ascending: true }])).toEqual([0, 1, 2]);
  });
});

describe("guessHeaders", () => {
  it("is true for a text row above a row with numbers", () => {
    expect(guessHeaders(["Name", "Age"], ["Ada", 36])).toBe(true);
  });

  it("is false when the first row is as numeric as the data", () => {
    expect(guessHeaders([1, 2], [3, 4])).toBe(false);
  });

  it("is false for a single row or an empty header cell", () => {
    expect(guessHeaders(["Name"], undefined)).toBe(false);
    expect(guessHeaders(["Name", ""], ["Ada", 36])).toBe(false);
  });

  it("is false when everything is text, since nothing tells header from data", () => {
    expect(guessHeaders(["b", "a"], ["d", "c"])).toBe(false);
  });
});

describe("rangeHasMerges", () => {
  it("detects a merged range that touches the sort range", () => {
    const sheet: Sheet = { ...newSheet("S"), merges: [{ start: "B2", end: "C2" }] };
    expect(rangeHasMerges(sheet, { top: 0, left: 0, bottom: 4, right: 3 })).toBe(true);
    expect(rangeHasMerges(sheet, { top: 3, left: 0, bottom: 4, right: 3 })).toBe(false);
    expect(rangeHasMerges(sheet, { top: 0, left: 3, bottom: 4, right: 4 })).toBe(false);
  });
});

describe("planSort", () => {
  const people = { A1: "Name", B1: "Age", A2: "Cy", B2: "30", A3: "Al", B3: "25", A4: "Bo", B4: "30" };

  it("moves whole rows and keeps the header where it is", () => {
    const { text } = sorted(people, { top: 0, left: 0, bottom: 3, right: 1 }, [{ column: 0, ascending: true }], {
      hasHeaders: true,
    });
    expect([text("A1"), text("A2"), text("A3"), text("A4")]).toEqual(["Name", "Al", "Bo", "Cy"]);
    expect([text("B2"), text("B3"), text("B4")]).toEqual(["25", "30", "30"]);
  });

  it("sorts by several keys with their own directions", () => {
    const { text } = sorted(
      people,
      { top: 0, left: 0, bottom: 3, right: 1 },
      [
        { column: 1, ascending: false },
        { column: 0, ascending: true },
      ],
      { hasHeaders: true },
    );
    expect([text("A2"), text("A3"), text("A4")]).toEqual(["Bo", "Cy", "Al"]);
  });

  it("sorts the header row too when there is no header", () => {
    const { text } = sorted({ A1: "b", A2: "c", A3: "a" }, { top: 0, left: 0, bottom: 2, right: 0 }, [
      { column: 0, ascending: true },
    ]);
    expect([text("A1"), text("A2"), text("A3")]).toEqual(["a", "b", "c"]);
  });

  it("returns null when the rows are already in order", () => {
    const { plan } = sorted({ A1: "a", A2: "b" }, { top: 0, left: 0, bottom: 1, right: 0 }, [
      { column: 0, ascending: true },
    ]);
    expect(plan).toBeNull();
  });

  it("moves formulas with their rows and shifts relative references like a copy", () => {
    const { plan, text } = sorted(
      { A1: "3", B1: "=A1*2+$A$1", A2: "1", B2: "=A2*2+$A$1", A3: "2", B3: "=A3*2+$A$1" },
      { top: 0, left: 0, bottom: 2, right: 1 },
      [{ column: 0, ascending: true }],
    );
    expect(plan).not.toBeNull();
    // 1 (was row 2) -> row 1, 2 (row 3) -> row 2, 3 (row 1) -> row 3.
    expect([text("A1"), text("A2"), text("A3")]).toEqual(["1", "2", "3"]);
    expect(text("B1")).toBe("=A1*2+$A$1");
    expect(text("B2")).toBe("=A2*2+$A$1");
    expect(text("B3")).toBe("=A3*2+$A$1");
  });

  it("carries styles, comments and empty neighbours along with the row", () => {
    const workbook = seed({ A1: "b", B1: "note", A2: "a" });
    const sheet: Sheet = {
      ...workbook.sheets[0],
      cells: {
        ...workbook.sheets[0].cells,
        A1: {
          ...workbook.sheets[0].cells.A1,
          comment: "first",
          style: { ...workbook.sheets[0].cells.A1.style, bold: true },
        },
      },
    };
    const plan = planSort(
      sheet,
      computeSheetValues(workbook, sheet),
      { top: 0, left: 0, bottom: 1, right: 1 },
      [{ column: 0, ascending: true }],
      {
        hasHeaders: false,
      },
    )!;
    expect(plan.cells.A2.comment).toBe("first");
    expect(plan.cells.A2.style.bold).toBe(true);
    expect(cellText(plan.cells.B2)).toBe("note");
    // The row that had no B cell leaves none behind at B1.
    expect(plan.cells.B1).toBeUndefined();
    expect(plan.cells.A1.style.bold).toBe(false);
  });

  it("sorts blanks last whatever the direction", () => {
    for (const ascending of [true, false]) {
      const { text } = sorted({ A1: "b", A3: "a" }, { top: 0, left: 0, bottom: 2, right: 0 }, [
        { column: 0, ascending },
      ]);
      expect([text("A1"), text("A2"), text("A3")]).toEqual(ascending ? ["a", "b", ""] : ["b", "a", ""]);
    }
  });

  it("sorts by the displayed value of a formula key", () => {
    const { text } = sorted({ A1: "x", B1: "=2+2", A2: "y", B2: "=1+1" }, { top: 0, left: 0, bottom: 1, right: 1 }, [
      { column: 1, ascending: true },
    ]);
    expect([text("A1"), text("A2")]).toEqual(["y", "x"]);
  });

  it("leaves hidden rows where they are and sorts the visible ones among themselves", () => {
    const { text } = sorted(
      { A1: "c", A2: "z", A3: "a", A4: "b" },
      { top: 0, left: 0, bottom: 3, right: 0 },
      [{ column: 0, ascending: true }],
      { isHidden: (row) => row === 1 },
    );
    expect([text("A1"), text("A2"), text("A3"), text("A4")]).toEqual(["a", "z", "b", "c"]);
  });

  it("changes only the rows inside the range and the columns inside it", () => {
    const { text } = sorted(
      { A1: "2", B1: "x", C1: "keep", A2: "1", B2: "y", C2: "keep2" },
      { top: 0, left: 0, bottom: 1, right: 1 },
      [{ column: 0, ascending: true }],
    );
    expect([text("A1"), text("B1"), text("C1")]).toEqual(["1", "y", "keep"]);
    expect([text("A2"), text("B2"), text("C2")]).toEqual(["2", "x", "keep2"]);
  });

  it("reports the addresses it changed, for incremental recalculation", () => {
    const { plan } = sorted({ A1: "b", B1: "1", A2: "a", B2: "2", A3: "c" }, { top: 0, left: 0, bottom: 2, right: 1 }, [
      { column: 0, ascending: true },
    ]);
    expect(plan!.changed.sort()).toEqual(["A1", "A2", "B1", "B2"]);
  });

  it("refuses a key column outside the range", () => {
    const { plan } = sorted({ A1: "b", A2: "a" }, { top: 0, left: 0, bottom: 1, right: 0 }, [
      { column: 5, ascending: true },
    ]);
    expect(plan).toBeNull();
  });
});

describe("planSort applied to a workbook", () => {
  it("refreshes the cached value of moved formulas and the formulas that read the range", () => {
    const workbook = seed({ A1: "3", B1: "=A1*10", A2: "1", B2: "=A2*10", A3: "2", B3: "=A3*10", D1: "=B1" });
    const sheet = workbook.sheets[0];
    const plan = planSort(
      sheet,
      computeSheetValues(workbook, sheet),
      { top: 0, left: 0, bottom: 2, right: 1 },
      [{ column: 0, ascending: true }],
      { hasHeaders: false },
    )!;
    const next = applySheetCells(workbook, 0, plan.cells, plan.changed);
    const cells = next.sheets[0].cells;
    // B1 now holds the row of the value 1: its cached value is 10, not the old 30.
    expect(cells.B1.value).toEqual({ kind: "number", value: 10 });
    expect(cells.B3.value).toEqual({ kind: "number", value: 30 });
    const computed = computeWorkbookValues(next);
    expect(computed.get("S!D1")).toBe(10);
    expect(computed.get("S!B2")).toBe(20);
  });

  it("returns the same workbook when nothing changed", () => {
    const workbook = seed({ A1: "1" });
    expect(applySheetCells(workbook, 0, workbook.sheets[0].cells, [])).toBe(workbook);
  });
});

describe("sortContext", () => {
  const people = { A1: "Name", B1: "Age", A2: "Cy", B2: "30", A3: "Al", B3: "25", B9: "far" };

  function context(
    selection: { top: number; left: number; bottom: number; right: number },
    patch?: (sheet: Sheet) => Sheet,
  ) {
    const workbook = seed(people);
    const sheet = patch ? patch(workbook.sheets[0]) : workbook.sheets[0];
    return sortContext(sheet, computeSheetValues(workbook, sheet), selection);
  }

  it("takes the whole used range for a single selected cell, and guesses the header", () => {
    const result = context({ top: 1, left: 1, bottom: 1, right: 1 });
    expect(result).toEqual({ ok: true, range: { top: 0, left: 0, bottom: 8, right: 1 }, headerGuess: true });
  });

  it("prefers the filter range to the used range", () => {
    const result = context({ top: 0, left: 0, bottom: 0, right: 0 }, (sheet) => ({
      ...sheet,
      filter: { range: "A1:B3", column: 0, values: [] },
    }));
    expect(result).toMatchObject({ ok: true, range: { top: 0, left: 0, bottom: 2, right: 1 } });
  });

  it("sorts a multi-cell selection as selected, clipped to the data", () => {
    const result = context({ top: 1, left: 0, bottom: 500, right: 1 });
    expect(result).toEqual({ ok: true, range: { top: 1, left: 0, bottom: 8, right: 1 }, headerGuess: false });
  });

  it("explains why a sort is not possible", () => {
    expect(context({ top: 0, left: 0, bottom: 0, right: 0 }, (sheet) => ({ ...sheet, sheetProtection: "X" }))).toEqual({
      ok: false,
      reason: "protected",
    });
    expect(
      context({ top: 0, left: 0, bottom: 2, right: 1 }, (sheet) => ({
        ...sheet,
        merges: [{ start: "A2", end: "B2" }],
      })),
    ).toEqual({ ok: false, reason: "merged" });
    expect(context({ top: 3, left: 0, bottom: 3, right: 1 })).toMatchObject({ ok: false, reason: "tooSmall" });
    expect(sortContext(newSheet("E"), new Map(), { top: 0, left: 0, bottom: 0, right: 0 })).toEqual({
      ok: false,
      reason: "tooSmall",
    });
  });
});
