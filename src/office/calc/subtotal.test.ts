/**
 * SUBTOTAL and AGGREGATE.
 *
 * Both take references, because what they sum depends on where the cells are:
 * rows hidden by a filter, and cells that are themselves subtotals, are left
 * out. The SUBTOTAL examples are the ones Microsoft documents; AGGREGATE's
 * option table (0-7) is covered case by case.
 */
import { describe, expect, it } from "vitest";
import { newSheet, newWorkbook, type Workbook } from "../../lib/office-types";
import { applyCellEdit, computeSheetValues } from "./cells";
import { evaluateFormula, isError, lookupFunction, type FormulaContext, type Scalar } from "./formula";

function context(values: Record<string, Scalar> = {}, extra: Partial<FormulaContext> = {}): FormulaContext {
  return {
    getValue: (_sheet, address) => (address in values ? values[address] : ""),
    sheetNames: ["Sheet1", "Data"],
    currentSheet: "Sheet1",
    ...extra,
  };
}

function value(formula: string, values: Record<string, Scalar> = {}, extra: Partial<FormulaContext> = {}): Scalar {
  return evaluateFormula(formula, context(values, extra));
}

function code(formula: string, values: Record<string, Scalar> = {}, extra: Partial<FormulaContext> = {}): string {
  const result = value(formula, values, extra);
  return isError(result) ? result.code : "";
}

const NA = evaluateFormula("=NA()", context());
const DIV = evaluateFormula("=1/0", context());

describe("SUBTOTAL", () => {
  // Microsoft's sample: A2:A5 hold 120, 10, 150 and 23.
  const docs: Record<string, Scalar> = { A2: 120, A3: 10, A4: 150, A5: 23 };

  it("sums and averages a range (Microsoft examples)", () => {
    expect(value("=SUBTOTAL(9,A2:A5)", docs)).toBe(303);
    expect(value("=SUBTOTAL(1,A2:A5)", docs)).toBe(75.75);
  });

  it("maps every function_num 1-11 and 101-111 onto its function", () => {
    const data = { A1: 2, A2: 4, A3: 4, A4: 4, A5: 5, A6: 5, A7: 7, A8: 9 };
    const expected: Record<number, Scalar> = {
      1: 5,
      2: 8,
      3: 8,
      4: 9,
      5: 2,
      6: 2 * 4 * 4 * 4 * 5 * 5 * 7 * 9,
      7: Math.sqrt(32 / 7),
      8: 2,
      9: 40,
      10: 32 / 7,
      11: 4,
    };
    for (const [num, result] of Object.entries(expected)) {
      for (const offset of [0, 100]) {
        const got = value(`=SUBTOTAL(${Number(num) + offset},A1:A8)`, data);
        expect(got, `SUBTOTAL(${Number(num) + offset})`).toBeCloseTo(result as number, 10);
      }
    }
  });

  it("counts numbers for 2 and non-empty cells for 3", () => {
    const data = { A1: 1, A2: "text", A3: true, A4: NA, B1: "" };
    expect(value("=SUBTOTAL(2,A1:A4)", data)).toBe(1);
    expect(value("=SUBTOTAL(3,A1:B4)", data)).toBe(4);
    expect(value("=SUBTOTAL(103,A1:A4)", data)).toBe(4);
  });

  it("ignores text, booleans and empty cells in the numeric functions", () => {
    const data = { A1: 1, A2: "10", A3: true, A4: "", A5: 5 };
    expect(value("=SUBTOTAL(9,A1:A5)", data)).toBe(6);
    expect(value("=SUBTOTAL(1,A1:A5)", data)).toBe(3);
    expect(value("=SUBTOTAL(4,A1:A5)", data)).toBe(5);
  });

  it("returns what an empty selection returns in Excel", () => {
    expect(value("=SUBTOTAL(9,A1:A3)")).toBe(0);
    expect(value("=SUBTOTAL(4,A1:A3)")).toBe(0);
    expect(value("=SUBTOTAL(5,A1:A3)")).toBe(0);
    expect(value("=SUBTOTAL(6,A1:A3)")).toBe(0);
    expect(value("=SUBTOTAL(2,A1:A3)")).toBe(0);
    expect(code("=SUBTOTAL(1,A1:A3)")).toBe("#DIV/0!");
    expect(code("=SUBTOTAL(7,A1:A3)")).toBe("#DIV/0!");
    expect(code("=SUBTOTAL(8,A1:A3)")).toBe("#DIV/0!");
    expect(code("=SUBTOTAL(10,A1:A3)")).toBe("#DIV/0!");
    expect(code("=SUBTOTAL(11,A1:A3)")).toBe("#DIV/0!");
    expect(code("=SUBTOTAL(7,A1:A3)", { A1: 4 })).toBe("#DIV/0!");
    expect(value("=SUBTOTAL(8,A1:A3)", { A1: 4 })).toBe(0);
  });

  it("combines several references", () => {
    const data = { A1: 1, A2: 2, C1: 10, C2: 20 };
    expect(value("=SUBTOTAL(9,A1:A2,C1:C2)", data)).toBe(33);
    expect(value("=SUBTOTAL(1,A1:A2,C1:C2)", data)).toBe(8.25);
  });

  it("lets an error in the data through, except for the counting functions", () => {
    const data = { A1: 1, A2: NA, A3: 3 };
    expect(code("=SUBTOTAL(9,A1:A3)", data)).toBe("#N/A");
    expect(code("=SUBTOTAL(1,A1:A3)", data)).toBe("#N/A");
    expect(value("=SUBTOTAL(2,A1:A3)", data)).toBe(2);
    expect(value("=SUBTOTAL(3,A1:A3)", data)).toBe(3);
  });

  it("follows OFFSET, INDIRECT and defined names", () => {
    const data = { A1: 1, A2: 2, A3: 3 };
    expect(value("=SUBTOTAL(9,OFFSET(A1,1,0,2,1))", data)).toBe(5);
    expect(value('=SUBTOTAL(9,INDIRECT("A1:A3"))', data)).toBe(6);
    expect(value("=SUBTOTAL(9,TOTALS)", data, { names: { TOTALS: "A1:A3" } })).toBe(6);
  });

  it("truncates a fractional function_num and rejects numbers outside 1-11 and 101-111", () => {
    const data = { A1: 1, A2: 2 };
    expect(value("=SUBTOTAL(9.9,A1:A2)", data)).toBe(3);
    for (const num of [0, 12, 50, 100, 112, -9, 1000]) {
      expect(code(`=SUBTOTAL(${num},A1:A2)`, data), String(num)).toBe("#VALUE!");
    }
    expect(code('=SUBTOTAL("x",A1:A2)', data)).toBe("#VALUE!");
    expect(code("=SUBTOTAL(#N/A,A1:A2)", data)).toBe("#N/A");
  });

  it("wants references, not values", () => {
    expect(code("=SUBTOTAL(9,5)")).toBe("#VALUE!");
    expect(code("=SUBTOTAL(9,{1,2,3})")).toBe("#VALUE!");
    expect(code("=SUBTOTAL(9,Nowhere!A1:A2)")).toBe("#REF!");
  });

  it("needs a function_num and at least one reference", () => {
    expect(code("=SUBTOTAL(9)")).toBe("#VALUE!");
    expect(code("=SUBTOTAL()")).toBe("#VALUE!");
  });

  describe("nested subtotals and hidden rows", () => {
    // A2 and A3 are hidden by a filter, A4 is hidden by hand; A5 is a subtotal.
    const data = { A1: 1, A2: 2, A3: 4, A4: 8, A5: 15, A6: 32 };
    const formulas: Record<string, string> = { A5: "=SUBTOTAL(9,A1:A4)" };
    const hidden: Record<number, "filtered" | "hidden"> = { 1: "filtered", 2: "filtered", 3: "hidden" };
    const rows = {
      getFormula: (_sheet: string | null, address: string) => formulas[address] ?? null,
      hiddenRow: (_sheet: string | null, row: number) => hidden[row] ?? null,
    };

    it("leaves out cells that are SUBTOTALs themselves", () => {
      expect(value("=SUBTOTAL(9,A1:A6)", data, { getFormula: rows.getFormula })).toBe(1 + 2 + 4 + 8 + 32);
      expect(value("=SUBTOTAL(2,A1:A6)", data, { getFormula: rows.getFormula })).toBe(5);
    });

    it("leaves out nested subtotals even when they are part of a larger formula", () => {
      const nested = {
        getFormula: (_sheet: string | null, address: string) => (address === "A5" ? "=2*subtotal (9,A1:A4)+1" : null),
      };
      expect(value("=SUBTOTAL(9,A1:A6)", data, nested)).toBe(1 + 2 + 4 + 8 + 32);
    });

    it("skips filtered rows for 1-11 but keeps rows hidden by hand", () => {
      expect(value("=SUBTOTAL(9,A1:A4)", data, rows)).toBe(1 + 8);
      expect(value("=SUBTOTAL(2,A1:A4)", data, rows)).toBe(2);
    });

    it("skips every hidden row for 101-111", () => {
      expect(value("=SUBTOTAL(109,A1:A4)", data, rows)).toBe(1);
      expect(value("=SUBTOTAL(102,A1:A4)", data, rows)).toBe(1);
      expect(value("=SUBTOTAL(101,A1:A4)", data, rows)).toBe(1);
    });

    it("applies both rules together", () => {
      expect(value("=SUBTOTAL(109,A1:A6)", data, rows)).toBe(1 + 32);
      expect(value("=SUBTOTAL(9,A1:A6)", data, rows)).toBe(1 + 8 + 32);
    });

    it("counts every row when the caller knows nothing about the layout", () => {
      expect(value("=SUBTOTAL(109,A1:A4)", data)).toBe(15);
    });
  });
});

describe("AGGREGATE", () => {
  // A mix of numbers, text and the two error kinds that options 2/3/6/7 skip.
  const data: Record<string, Scalar> = { A1: 10, A2: 20, A3: NA, A4: 30, A5: 5, A6: 40, A7: "txt", A8: 25, A9: DIV };
  // The numbers, sorted: 5 10 20 25 30 40.

  it("computes the reference-form functions, ignoring errors (option 6)", () => {
    const expected: Record<number, number> = {
      1: 130 / 6,
      2: 6,
      3: 7,
      4: 40,
      5: 5,
      6: 10 * 20 * 30 * 5 * 40 * 25,
      9: 130,
      12: 22.5,
    };
    for (const [num, result] of Object.entries(expected)) {
      expect(value(`=AGGREGATE(${num},6,A1:A9)`, data), `AGGREGATE(${num},6)`).toBeCloseTo(result, 10);
    }
  });

  it("computes the variance and deviation functions", () => {
    const sorted = [5, 10, 20, 25, 30, 40];
    const mean = 130 / 6;
    const squares = sorted.reduce((sum, item) => sum + (item - mean) ** 2, 0);
    expect(value("=AGGREGATE(7,6,A1:A9)", data)).toBeCloseTo(Math.sqrt(squares / 5), 10);
    expect(value("=AGGREGATE(8,6,A1:A9)", data)).toBeCloseTo(Math.sqrt(squares / 6), 10);
    expect(value("=AGGREGATE(10,6,A1:A9)", data)).toBeCloseTo(squares / 5, 10);
    expect(value("=AGGREGATE(11,6,A1:A9)", data)).toBeCloseTo(squares / 6, 10);
  });

  it("computes the array-form functions 14-19", () => {
    expect(value("=AGGREGATE(14,6,A1:A9,2)", data)).toBe(30);
    expect(value("=AGGREGATE(15,6,A1:A9,2)", data)).toBe(10);
    expect(value("=AGGREGATE(16,6,A1:A9,0.5)", data)).toBe(22.5);
    expect(value("=AGGREGATE(17,6,A1:A9,1)", data)).toBe(12.5);
    expect(value("=AGGREGATE(18,6,A1:A9,0.5)", data)).toBe(22.5);
    expect(value("=AGGREGATE(19,6,A1:A9,1)", data)).toBe(8.75);
  });

  it("picks the most frequent number for 13", () => {
    expect(value("=AGGREGATE(13,6,A1:A4)", { A1: 3, A2: 7, A3: 3, A4: NA })).toBe(3);
    expect(code("=AGGREGATE(13,6,A1:A9)", data)).toBe("#N/A");
  });

  it("takes an array expression for 14-19, the classic way to skip errors", () => {
    expect(value("=AGGREGATE(15,6,1/{1,0,2,4},1)", {})).toBe(0.25);
    expect(value("=AGGREGATE(14,6,A1:A5/(A1:A5>0),2)", { A1: 4, A2: 0, A3: 9, A4: -3, A5: 6 })).toBe(6);
  });

  describe("options", () => {
    const formulas: Record<string, string> = { A4: "=SUBTOTAL(9,A1:A3)", A5: "=AGGREGATE(9,0,A1:A3)" };
    const hidden: Record<number, "filtered" | "hidden"> = { 1: "filtered" };
    const rows = {
      getFormula: (_sheet: string | null, address: string) => formulas[address] ?? null,
      hiddenRow: (_sheet: string | null, row: number) => hidden[row] ?? null,
    };
    // A1:A3 = 1, 2 (hidden), NA; A4 and A5 are subtotals worth 100 and 1000.
    const cells: Record<string, Scalar> = { A1: 1, A2: 2, A3: NA, A4: 100, A5: 1000, A6: 7 };
    const sum = (option: number, range = "A1:A6") => value(`=AGGREGATE(9,${option},${range})`, cells, rows);

    it("0 and omitted: ignores nested SUBTOTAL and AGGREGATE (errors still count)", () => {
      expect(sum(0, "A1:A2")).toBe(3);
      expect(sum(0, "A1:A2")).toBe(3);
      expect(sum(0, "A4:A6")).toBe(7);
      expect(code("=AGGREGATE(9,0,A1:A6)", cells, rows)).toBe("#N/A");
      expect(value("=AGGREGATE(9,,A4:A6)", cells, rows)).toBe(7);
    });

    it("1: ignores hidden rows and nested subtotals", () => {
      expect(sum(1, "A1:A2")).toBe(1);
      expect(sum(1, "A4:A6")).toBe(7);
    });

    it("2: ignores errors and nested subtotals", () => {
      expect(sum(2)).toBe(1 + 2 + 7);
    });

    it("3: ignores hidden rows, errors and nested subtotals", () => {
      expect(sum(3)).toBe(1 + 7);
    });

    it("4: ignores nothing", () => {
      expect(sum(4, "A1:A2")).toBe(3);
      expect(sum(4, "A4:A6")).toBe(1107);
      expect(code("=AGGREGATE(9,4,A1:A6)", cells, rows)).toBe("#N/A");
    });

    it("5: ignores hidden rows only", () => {
      expect(sum(5, "A1:A2")).toBe(1);
      expect(sum(5, "A4:A6")).toBe(1107);
    });

    it("6: ignores errors only", () => {
      expect(sum(6)).toBe(1 + 2 + 100 + 1000 + 7);
    });

    it("7: ignores hidden rows and errors", () => {
      expect(sum(7)).toBe(1 + 100 + 1000 + 7);
    });

    it("rejects an option outside 0-7", () => {
      for (const option of [-1, 8, 9, 100]) expect(code(`=AGGREGATE(9,${option},A1:A2)`, cells, rows)).toBe("#VALUE!");
      expect(code('=AGGREGATE(9,"x",A1:A2)', cells, rows)).toBe("#VALUE!");
    });
  });

  it("counts errors in COUNTA only when they are not ignored", () => {
    expect(value("=AGGREGATE(3,4,A1:A9)", data)).toBe(9);
    expect(value("=AGGREGATE(3,6,A1:A9)", data)).toBe(7);
    expect(value("=AGGREGATE(2,4,A1:A9)", data)).toBe(6);
  });

  it("reports the first error when errors are not ignored", () => {
    expect(code("=AGGREGATE(9,4,A1:A9)", data)).toBe("#N/A");
    expect(code("=AGGREGATE(14,4,A1:A9,1)", data)).toBe("#N/A");
  });

  it("returns the errors the underlying functions return", () => {
    expect(code("=AGGREGATE(1,6,B1:B3)")).toBe("#DIV/0!");
    expect(code("=AGGREGATE(7,6,B1:B3)")).toBe("#DIV/0!");
    expect(code("=AGGREGATE(12,6,B1:B3)")).toBe("#NUM!");
    expect(code("=AGGREGATE(13,6,B1:B3)")).toBe("#N/A");
    expect(value("=AGGREGATE(4,6,B1:B3)")).toBe(0);
    expect(code("=AGGREGATE(14,6,A1:A9,7)", data)).toBe("#NUM!");
    expect(code("=AGGREGATE(14,6,A1:A9,0)", data)).toBe("#NUM!");
    expect(code("=AGGREGATE(15,6,A1:A9,7)", data)).toBe("#NUM!");
    expect(code("=AGGREGATE(16,6,A1:A9,1.5)", data)).toBe("#NUM!");
    expect(code("=AGGREGATE(17,6,A1:A9,5)", data)).toBe("#NUM!");
    expect(code("=AGGREGATE(18,6,A1:A9,0.05)", data)).toBe("#NUM!");
    expect(code("=AGGREGATE(19,6,A1:A9,4)", data)).toBe("#NUM!");
    expect(code("=AGGREGATE(19,6,A1:A9,0)", data)).toBe("#NUM!");
  });

  it("rejects an unknown function_num and a missing k", () => {
    for (const num of [0, 20, -1]) expect(code(`=AGGREGATE(${num},6,A1:A9)`, data), String(num)).toBe("#VALUE!");
    expect(code("=AGGREGATE(14,6,A1:A9)", data)).toBe("#VALUE!");
    // A blank k reads as 0, which LARGE rejects.
    expect(code("=AGGREGATE(14,6,A1:A9,)", data)).toBe("#NUM!");
    expect(code('=AGGREGATE(14,6,A1:A9,"x")', data)).toBe("#VALUE!");
    expect(code("=AGGREGATE(#N/A,6,A1:A9)", data)).toBe("#N/A");
  });

  it("wants a reference for 1-13 but not for 14-19", () => {
    expect(code("=AGGREGATE(9,6,{1,2,3})")).toBe("#VALUE!");
    expect(value("=AGGREGATE(14,6,{1,5,3},1)")).toBe(5);
  });

  it("combines several references for 1-13", () => {
    expect(value("=AGGREGATE(9,6,A1:A2,A4:A5)", data)).toBe(10 + 20 + 30 + 5);
  });
});

describe("registration", () => {
  it("lists both with a signature for the picker", () => {
    for (const name of ["SUBTOTAL", "AGGREGATE"]) {
      expect(lookupFunction(name)?.signature, name).toMatch(new RegExp(`^${name}\\(`));
    }
  });
});

describe("workbook integration", () => {
  function book(): Workbook {
    return { ...newWorkbook("Subtotal"), sheets: [newSheet("Sheet1")] };
  }

  function enter(workbook: Workbook, address: string, input: string): Workbook {
    const match = address.match(/^([A-Z]+)(\d+)$/)!;
    const col = match[1].split("").reduce((sum, letter) => sum * 26 + letter.charCodeAt(0) - 64, 0) - 1;
    return applyCellEdit(workbook, 0, Number(match[2]) - 1, col, input);
  }

  function valueAt(workbook: Workbook, address: string): Scalar | undefined {
    return computeSheetValues(workbook, workbook.sheets[0]).get(address);
  }

  /** Rows 2 and 3 (1-based) are hidden: by the filter, or by hand when `filtered` is false. */
  function withHiddenRows(workbook: Workbook, filtered: boolean): Workbook {
    const sheet = workbook.sheets[0];
    return {
      ...workbook,
      sheets: [
        {
          ...sheet,
          rowHeights: { 1: 0, 2: 0 },
          filter: filtered ? { range: "A1:A6", column: 0, values: ["1", "8"] } : null,
        },
      ],
    };
  }

  function filled(): Workbook {
    let workbook = book();
    for (const [index, input] of ["1", "2", "4", "8"].entries()) workbook = enter(workbook, `A${index + 1}`, input);
    workbook = enter(workbook, "B1", "=SUBTOTAL(9,A1:A4)");
    workbook = enter(workbook, "B2", "=SUBTOTAL(109,A1:A4)");
    workbook = enter(workbook, "B3", "=AGGREGATE(9,5,A1:A4)");
    workbook = enter(workbook, "B4", "=AGGREGATE(9,4,A1:A4)");
    return workbook;
  }

  it("totals every row when nothing is hidden", () => {
    const workbook = filled();
    expect([1, 2, 3, 4].map((row) => valueAt(workbook, `B${row}`))).toEqual([15, 15, 15, 15]);
  });

  it("leaves out the rows a filter hides", () => {
    const workbook = withHiddenRows(filled(), true);
    expect([1, 2, 3, 4].map((row) => valueAt(workbook, `B${row}`))).toEqual([9, 9, 9, 15]);
  });

  it("keeps rows hidden by hand in 1-11 but not in 101-111", () => {
    const workbook = withHiddenRows(filled(), false);
    expect([1, 2, 3, 4].map((row) => valueAt(workbook, `B${row}`))).toEqual([15, 9, 9, 15]);
  });

  it("recalculates after an edit on a sheet with hidden rows", () => {
    let workbook = withHiddenRows(filled(), true);
    expect(valueAt(workbook, "B1")).toBe(9);
    workbook = enter(workbook, "A1", "100");
    expect(valueAt(workbook, "B1")).toBe(108);
    workbook = enter(workbook, "A2", "1000");
    expect(valueAt(workbook, "B1")).toBe(108);
    expect(valueAt(workbook, "B4")).toBe(100 + 1000 + 4 + 8);
  });

  it("ignores a subtotal that sits inside the range", () => {
    let workbook = filled();
    workbook = enter(workbook, "A5", "=SUBTOTAL(9,A1:A4)");
    workbook = enter(workbook, "C1", "=SUBTOTAL(9,A1:A5)");
    workbook = enter(workbook, "C2", "=SUM(A1:A5)");
    expect(valueAt(workbook, "A5")).toBe(15);
    expect(valueAt(workbook, "C1")).toBe(15);
    expect(valueAt(workbook, "C2")).toBe(30);
  });

  it("starts counting a cell again once it stops being a subtotal", () => {
    let workbook = filled();
    workbook = enter(workbook, "A5", "=SUBTOTAL(9,A1:A4)");
    workbook = enter(workbook, "C1", "=SUBTOTAL(9,A1:A5)");
    expect(valueAt(workbook, "C1")).toBe(15);
    workbook = enter(workbook, "A5", "=SUM(A1:A4)");
    expect(valueAt(workbook, "C1")).toBe(30);
  });

  it("evaluates an array argument inside a workbook formula", () => {
    let workbook = book();
    workbook = enter(workbook, "A1", "=AGGREGATE(15,6,1/{1,0,2},1)");
    expect(valueAt(workbook, "A1")).toBe(0.5);
  });
});
