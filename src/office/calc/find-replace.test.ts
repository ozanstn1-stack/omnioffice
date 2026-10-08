import { describe, expect, it } from "vitest";
import { newSheet, newWorkbook, type Sheet, type Workbook } from "../../lib/office-types";
import { applyCellEdit, computeWorkbookValues } from "./cells";
import {
  compileFind,
  matchCells,
  replaceCells,
  replaceInText,
  searchCells,
  stepMatch,
  type CalcFindOptions,
  type CompiledFind,
} from "./find-replace";
import { parseAddress } from "./formula";

const OPTIONS: CalcFindOptions = {
  scope: "sheet",
  lookIn: "formulas",
  matchCase: false,
  wholeCell: false,
  regex: false,
};

/** A workbook from `{ SheetName: { A1: "typed text or =formula" } }`. */
function seed(sheets: Record<string, Record<string, string>>): Workbook {
  let workbook: Workbook = { ...newWorkbook("Test"), sheets: Object.keys(sheets).map((name) => newSheet(name)) };
  Object.entries(sheets).forEach(([, cells], sheetIndex) => {
    for (const [address, text] of Object.entries(cells)) {
      const position = parseAddress(address)!;
      workbook = applyCellEdit(workbook, sheetIndex, position.row, position.col, text);
    }
  });
  return workbook;
}

function pattern(query: string, options: Partial<CalcFindOptions> = {}): RegExp {
  const compiled = compileFind(query, { ...OPTIONS, ...options }) as Extract<CompiledFind, { ok: true }>;
  expect(compiled?.ok).toBe(true);
  return compiled.pattern;
}

function addresses(cells: ReadonlyArray<{ address: string; sheet: number }>): string[] {
  return cells.map((cell) => `${cell.sheet}:${cell.address}`);
}

function values(workbook: Workbook) {
  return computeWorkbookValues(workbook);
}

function protect(workbook: Workbook, sheetIndex: number, how: "modern" | "legacy"): Workbook {
  return {
    ...workbook,
    sheets: workbook.sheets.map((sheet, index): Sheet => {
      if (index !== sheetIndex) return sheet;
      if (how === "legacy") return { ...sheet, sheetProtection: "ABCD" };
      return {
        ...sheet,
        protection: {
          enabled: true,
          passwordHash: null,
          algorithmName: "SHA-512",
          hashValue: "x",
          saltValue: "y",
          spinCount: 100000,
          options: [],
        },
      };
    }),
  };
}

describe("compileFind", () => {
  it("returns null for an empty query", () => {
    expect(compileFind("", OPTIONS)).toBeNull();
  });

  it("treats the query literally unless regex is on", () => {
    expect(pattern("a.b").test("axb")).toBe(false);
    expect(pattern("a.b").test("a.b")).toBe(true);
    expect(pattern("a.b", { regex: true }).test("axb")).toBe(true);
  });

  it("is case-insensitive by default and case-sensitive on request", () => {
    expect(pattern("çay").test("ÇAY")).toBe(true);
    expect(pattern("abc", { matchCase: true }).test("ABC")).toBe(false);
    expect(pattern("abc", { matchCase: true }).test("xabcx")).toBe(true);
  });

  it("reports an invalid regular expression instead of throwing", () => {
    const compiled = compileFind("(unclosed", { ...OPTIONS, regex: true });
    expect(compiled?.ok).toBe(false);
    if (compiled && !compiled.ok) expect(compiled.error.length).toBeGreaterThan(0);
  });

  it("anchors whole-cell matches, including every alternative of a regex", () => {
    const whole = pattern("foo", { wholeCell: true });
    expect(whole.test("FOO")).toBe(true);
    expect(pattern("foo", { wholeCell: true }).test("foobar")).toBe(false);
    expect(pattern("foo", { wholeCell: true }).test("a foo")).toBe(false);
    const alternatives = (text: string) => pattern("a|b", { regex: true, wholeCell: true }).test(text);
    expect(alternatives("a")).toBe(true);
    expect(alternatives("b")).toBe(true);
    expect(alternatives("ab")).toBe(false);
  });
});

describe("searchCells", () => {
  it("lists cells row by row, left to right, whatever order they were stored in", () => {
    const workbook = seed({ S: { C2: "x", A2: "x", B1: "x", A1: "x" } });
    expect(addresses(searchCells(workbook, 0, OPTIONS, values(workbook)))).toEqual(["0:A1", "0:B1", "0:A2", "0:C2"]);
  });

  it("skips empty and formatting-only cells", () => {
    let workbook = seed({ S: { A1: "x" } });
    workbook = {
      ...workbook,
      sheets: [
        {
          ...workbook.sheets[0],
          cells: {
            ...workbook.sheets[0].cells,
            B1: {
              ...workbook.sheets[0].cells.A1,
              value: { kind: "empty" },
              style: { ...workbook.sheets[0].cells.A1.style, bold: true },
            },
          },
        },
      ],
    };
    expect(addresses(searchCells(workbook, 0, OPTIONS, values(workbook)))).toEqual(["0:A1"]);
  });

  it("searches only the active sheet, or every sheet in workbook order", () => {
    const workbook = seed({ One: { A1: "a" }, Two: { A1: "b", B2: "c" } });
    expect(addresses(searchCells(workbook, 1, OPTIONS, values(workbook)))).toEqual(["1:A1", "1:B2"]);
    expect(addresses(searchCells(workbook, 1, { ...OPTIONS, scope: "workbook" }, values(workbook)))).toEqual([
      "0:A1",
      "1:A1",
      "1:B2",
    ]);
  });

  it("looks in formulas by formula text and constants by what was typed", () => {
    const workbook = seed({ S: { A1: "5", B1: "=A1*2", C1: "word" } });
    const texts = searchCells(workbook, 0, OPTIONS, values(workbook)).map((cell) => cell.text);
    expect(texts).toEqual(["5", "=A1*2", "word"]);
  });

  it("looks in values by what the grid displays", () => {
    let workbook = seed({ S: { A1: "0.5", B1: "=A1*2" } });
    workbook = {
      ...workbook,
      sheets: [
        {
          ...workbook.sheets[0],
          cells: {
            ...workbook.sheets[0].cells,
            A1: { ...workbook.sheets[0].cells.A1, style: { ...workbook.sheets[0].cells.A1.style, numberFormat: "0%" } },
          },
        },
      ],
    };
    const texts = searchCells(workbook, 0, { ...OPTIONS, lookIn: "values" }, values(workbook)).map((cell) => cell.text);
    expect(texts).toEqual(["50%", "1"]);
  });

  it("marks what can be replaced: everything in formulas mode, only text constants in values mode", () => {
    const workbook = seed({ S: { A1: "word", B1: "5", C1: "=A1", D1: "TRUE" } });
    const flags = (options: Partial<CalcFindOptions>) =>
      searchCells(workbook, 0, { ...OPTIONS, ...options }, values(workbook)).map((cell) => cell.replaceable);
    expect(flags({ lookIn: "formulas" })).toEqual([true, true, true, true]);
    expect(flags({ lookIn: "values" })).toEqual([true, false, false, false]);
  });

  it("never marks cells of a protected sheet replaceable, with either protection field", () => {
    const base = seed({ One: { A1: "x" }, Two: { A1: "x" } });
    for (const how of ["modern", "legacy"] as const) {
      const workbook = protect(base, 1, how);
      const cells = searchCells(workbook, 0, { ...OPTIONS, scope: "workbook" }, values(workbook));
      expect(cells.map((cell) => cell.replaceable)).toEqual([true, false]);
    }
  });

  it("does not treat a protection record that is switched off as protection", () => {
    const base = seed({ S: { A1: "x" } });
    const workbook: Workbook = {
      ...base,
      sheets: [
        {
          ...base.sheets[0],
          protection: {
            enabled: false,
            passwordHash: null,
            algorithmName: "",
            hashValue: "",
            saltValue: "",
            spinCount: 0,
            options: [],
          },
        },
      ],
    };
    expect(searchCells(workbook, 0, OPTIONS, values(workbook))[0].replaceable).toBe(true);
  });
});

describe("matchCells", () => {
  it("keeps the cells with at least one non-empty match", () => {
    const workbook = seed({ S: { A1: "apple", A2: "pear", A3: "Apple pie" } });
    const cells = searchCells(workbook, 0, OPTIONS, values(workbook));
    expect(addresses(matchCells(cells, pattern("apple")))).toEqual(["0:A1", "0:A3"]);
    expect(addresses(matchCells(cells, pattern("apple", { matchCase: true })))).toEqual(["0:A1"]);
  });

  it("ignores zero-length matches and honours the limit", () => {
    const workbook = seed({ S: { A1: "aaa", A2: "aaa", A3: "aaa" } });
    const cells = searchCells(workbook, 0, OPTIONS, values(workbook));
    expect(matchCells(cells, pattern("x*", { regex: true }))).toEqual([]);
    expect(matchCells(cells, pattern("a"), 2)).toHaveLength(2);
  });

  it("matches the whole cell only when asked to", () => {
    const workbook = seed({ S: { A1: "cat", A2: "cats", A3: "CAT" } });
    const cells = searchCells(workbook, 0, OPTIONS, values(workbook));
    expect(addresses(matchCells(cells, pattern("cat", { wholeCell: true })))).toEqual(["0:A1", "0:A3"]);
  });
});

describe("stepMatch", () => {
  const workbook = seed({ One: { B1: "x", A3: "x" }, Two: { A1: "x" } });
  const matches = searchCells(workbook, 0, { ...OPTIONS, scope: "workbook" }, values(workbook));

  it("steps to the next match after a position, in sheet, row, column order", () => {
    expect(stepMatch(matches, { sheet: 0, row: 0, col: 0 }, 1)?.address).toBe("B1");
    expect(stepMatch(matches, { sheet: 0, row: 0, col: 1 }, 1)).toMatchObject({ sheet: 0, address: "A3" });
    expect(stepMatch(matches, { sheet: 0, row: 2, col: 0 }, 1)).toMatchObject({ sheet: 1, address: "A1" });
  });

  it("wraps around at both ends", () => {
    expect(stepMatch(matches, { sheet: 1, row: 0, col: 0 }, 1)).toMatchObject({ sheet: 0, address: "B1" });
    expect(stepMatch(matches, { sheet: 0, row: 0, col: 1 }, -1)).toMatchObject({ sheet: 1, address: "A1" });
  });

  it("steps backwards from between two matches", () => {
    expect(stepMatch(matches, { sheet: 0, row: 1, col: 5 }, -1)).toMatchObject({ sheet: 0, address: "B1" });
  });

  it("returns null when there is nothing to step to", () => {
    expect(stepMatch([], { sheet: 0, row: 0, col: 0 }, 1)).toBeNull();
  });
});

describe("replaceInText", () => {
  it("replaces every occurrence and counts them", () => {
    expect(replaceInText("a-b-c", pattern("-"), "", false)).toEqual({ text: "abc", count: 2 });
  });

  it("keeps the replacement literal when regex is off", () => {
    expect(replaceInText("price", pattern("price"), "$&$1", false).text).toBe("$&$1");
  });

  it("expands groups and $& when regex is on", () => {
    const groups = pattern("(\\w+)@(\\w+)", { regex: true });
    expect(replaceInText("ada@lab", groups, "$2:$1", true).text).toBe("lab:ada");
    expect(replaceInText("ada@lab", groups, "[$&]", true).text).toBe("[ada@lab]");
  });

  it("replaces the whole text for a whole-cell match", () => {
    expect(replaceInText("Foo", pattern("foo", { wholeCell: true }), "bar", false)).toEqual({ text: "bar", count: 1 });
  });

  it("leaves a text without matches alone and ignores empty matches", () => {
    expect(replaceInText("abc", pattern("z"), "y", false)).toEqual({ text: "abc", count: 0 });
    expect(replaceInText("abc", pattern("x*", { regex: true }), "-", true)).toEqual({ text: "abc", count: 0 });
  });
});

describe("replaceCells", () => {
  function run(
    workbook: Workbook,
    query: string,
    replacement: string,
    options: Partial<CalcFindOptions> = {},
    activeSheet = 0,
  ) {
    const merged = { ...OPTIONS, ...options };
    const regex = pattern(query, merged);
    const cells = matchCells(searchCells(workbook, activeSheet, merged, values(workbook)), regex);
    return { cells, ...replaceCells(workbook, cells, regex, replacement, merged) };
  }

  it("replaces constants in one pass and keeps their formatting and comment", () => {
    let workbook = seed({ S: { A1: "red apple", A2: "green apple", A3: "pear" } });
    const styled = { ...workbook.sheets[0].cells.A1, comment: "note" };
    styled.style = { ...styled.style, bold: true };
    workbook = { ...workbook, sheets: [{ ...workbook.sheets[0], cells: { ...workbook.sheets[0].cells, A1: styled } }] };

    const result = run(workbook, "apple", "plum");
    expect(result.replaced).toBe(2);
    const cells = result.workbook.sheets[0].cells;
    expect(cells.A1.value).toEqual({ kind: "text", value: "red plum" });
    expect(cells.A1.style.bold).toBe(true);
    expect(cells.A1.comment).toBe("note");
    expect(cells.A2.value).toEqual({ kind: "text", value: "green plum" });
    expect(cells.A3.value).toEqual({ kind: "text", value: "pear" });
  });

  it("re-parses a replaced number as a number", () => {
    const result = run(seed({ S: { A1: "100" } }), "100", "250");
    expect(result.workbook.sheets[0].cells.A1.value).toEqual({ kind: "number", value: 250 });
  });

  it("re-parses and recalculates a replaced formula and the cells that read it", () => {
    const workbook = seed({ S: { C1: "5", D1: "7", A1: "=C1", B1: "=A1*2" } });
    expect(values(workbook).get("S!B1")).toBe(10);

    const result = run(workbook, "C1", "D1", { lookIn: "formulas" });
    expect(result.replaced).toBe(1);
    const cells = result.workbook.sheets[0].cells;
    expect(cells.A1.formula).toBe("=D1");
    // The cached value saved with the file is current, not one edit behind.
    expect(cells.A1.value).toEqual({ kind: "number", value: 7 });
    const computed = computeWorkbookValues(result.workbook);
    expect(computed.get("S!A1")).toBe(7);
    expect(computed.get("S!B1")).toBe(14);
  });

  it("turns a replaced formula text into a constant when it no longer starts with =", () => {
    const result = run(seed({ S: { A1: "=1+1" } }), "=", "", { lookIn: "formulas" });
    expect(result.workbook.sheets[0].cells.A1.formula).toBeNull();
    expect(result.workbook.sheets[0].cells.A1.value).toEqual({ kind: "text", value: "1+1" });
  });

  it("does not touch protected sheets and reports what it skipped", () => {
    const workbook = protect(seed({ Open: { A1: "x" }, Locked: { A1: "x", B1: "x" } }), 1, "modern");
    const result = run(workbook, "x", "y", { scope: "workbook" });
    expect(result.replaced).toBe(1);
    expect(result.skipped).toBe(2);
    expect(result.workbook.sheets[1]).toBe(workbook.sheets[1]);
    expect(result.workbook.sheets[0].cells.A1.value).toEqual({ kind: "text", value: "y" });
  });

  it("replaces only text constants when looking in values", () => {
    const workbook = seed({ S: { A1: "item 7", B1: "7", C1: "=A1" } });
    const result = run(workbook, "7", "8", { lookIn: "values" });
    expect(result.replaced).toBe(1);
    expect(result.skipped).toBe(2);
    const cells = result.workbook.sheets[0].cells;
    expect(cells.A1.value).toEqual({ kind: "text", value: "item 8" });
    expect(cells.B1.value).toEqual({ kind: "number", value: 7 });
    expect(cells.C1.formula).toBe("=A1");
  });

  it("replaces across sheets with a workbook scope", () => {
    const workbook = seed({ One: { A1: "cat" }, Two: { A1: "cat food", B1: '=A1&"!"' } });
    const result = run(workbook, "cat", "dog", { scope: "workbook" });
    expect(result.replaced).toBe(2);
    expect(result.workbook.sheets[0].cells.A1.value).toEqual({ kind: "text", value: "dog" });
    expect(result.workbook.sheets[1].cells.A1.value).toEqual({ kind: "text", value: "dog food" });
    expect(computeWorkbookValues(result.workbook).get("Two!B1")).toBe("dog food!");
  });

  it("returns the very same workbook when nothing changes", () => {
    const workbook = seed({ S: { A1: "x" } });
    const result = run(workbook, "x", "x");
    expect(result.workbook).toBe(workbook);
    expect(result.replaced).toBe(0);
    expect(result.unchanged).toBe(1);
  });

  it("removes a cell that a replacement empties", () => {
    const result = run(seed({ S: { A1: "gone", B1: "kept" } }), "gone", "");
    expect(result.workbook.sheets[0].cells.A1).toBeUndefined();
    expect(result.workbook.sheets[0].cells.B1).toBeDefined();
  });

  it("applies regex group references to formulas", () => {
    const workbook = seed({ S: { A1: "1", A2: "2", B1: "=A1-A2" } });
    expect(values(workbook).get("S!B1")).toBe(-1);
    const result = run(workbook, "(A1)-(A2)", "$2-$1", { regex: true });
    expect(result.workbook.sheets[0].cells.B1.formula).toBe("=A2-A1");
    expect(computeWorkbookValues(result.workbook).get("S!B1")).toBe(1);
  });
});
