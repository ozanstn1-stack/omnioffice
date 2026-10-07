/**
 * ROW, COLUMN, ADDRESS, HYPERLINK, ISFORMULA and FORMULATEXT.
 *
 * ROW, COLUMN, ISFORMULA and FORMULATEXT look at the location an argument
 * denotes, not at its value, so they are context functions like OFFSET. The
 * ADDRESS and HYPERLINK examples are the ones Microsoft documents.
 */
import { describe, expect, it } from "vitest";
import { newSheet, newWorkbook, type Workbook } from "../../lib/office-types";
import { applyCellEdit, computeSheetValues } from "./cells";
import {
  evaluateFormula,
  evaluateToMatrix,
  isError,
  lookupFunction,
  type FormulaContext,
  type Scalar,
} from "./formula";

function context(
  values: Record<string, Scalar> = {},
  extra: Partial<FormulaContext> & { sheets?: Record<string, Record<string, Scalar>> } = {},
): FormulaContext {
  const { sheets = {}, ...rest } = extra;
  return {
    getValue: (sheet, address) => {
      const source = sheet && sheets[sheet] ? sheets[sheet] : values;
      return address in source ? source[address] : "";
    },
    sheetNames: ["Sheet1", "Data", "My Sheet", ...Object.keys(sheets)],
    currentSheet: "Sheet1",
    ...rest,
  };
}

function code(formula: string, extra: Partial<FormulaContext> = {}): string {
  const result = evaluateFormula(formula, context({}, extra));
  return isError(result) ? result.code : "";
}

describe("ROW", () => {
  it("returns the row of a reference (Microsoft example: ROW(C4) is 4)", () => {
    expect(evaluateFormula("=ROW(C4)", context())).toBe(4);
    expect(evaluateFormula("=ROW(Data!B20)", context())).toBe(20);
    expect(evaluateFormula("=ROW(A1)+ROW(A5)", context())).toBe(6);
  });

  it("returns the row of the formula's own cell without an argument", () => {
    expect(evaluateFormula("=ROW()", context({}, { currentAddress: "D7" }))).toBe(7);
    expect(code("=ROW()")).toBe("#VALUE!");
  });

  it("spills the rows of a multi-row reference as a column", () => {
    expect(evaluateToMatrix("=ROW(B2:D4)", context())).toEqual([[2], [3], [4]]);
    expect(evaluateToMatrix("=ROW(A5:C5)", context())).toEqual([[5]]);
  });

  it("follows OFFSET, INDIRECT and defined names", () => {
    expect(evaluateFormula("=ROW(OFFSET(A1,4,0))", context())).toBe(5);
    expect(evaluateFormula('=ROW(INDIRECT("C9"))', context())).toBe(9);
    expect(evaluateFormula("=ROW(BLOCK)", context({}, { names: { BLOCK: "B3:B6" } }))).toBe(3);
  });

  it("is #VALUE! for something that is not a location and passes errors through", () => {
    expect(code("=ROW(5)")).toBe("#VALUE!");
    expect(code('=ROW("A1")')).toBe("#VALUE!");
    expect(code("=ROW(#N/A)")).toBe("#N/A");
    expect(code("=ROW(Nowhere!A1)")).toBe("#REF!");
    expect(code("=ROW(A1,A2)")).toBe("#VALUE!");
  });
});

describe("COLUMN", () => {
  it("returns the column of a reference (Microsoft example: COLUMN(B6) is 2)", () => {
    expect(evaluateFormula("=COLUMN(B6)", context())).toBe(2);
    expect(evaluateFormula("=COLUMN(AA1)", context())).toBe(27);
    expect(evaluateFormula("=COLUMN(Data!XFD1)", context())).toBe(16384);
  });

  it("returns the column of the formula's own cell without an argument", () => {
    expect(evaluateFormula("=COLUMN()", context({}, { currentAddress: "D7" }))).toBe(4);
    expect(code("=COLUMN()")).toBe("#VALUE!");
  });

  it("spills the columns of a multi-column reference as a row", () => {
    expect(evaluateToMatrix("=COLUMN(B2:D4)", context())).toEqual([[2, 3, 4]]);
    expect(evaluateToMatrix("=COLUMN(C1:C9)", context())).toEqual([[3]]);
  });

  it("is #VALUE! for something that is not a location", () => {
    expect(code("=COLUMN(5)")).toBe("#VALUE!");
    expect(code("=COLUMN(#DIV/0!)")).toBe("#DIV/0!");
  });
});

describe("ADDRESS", () => {
  it("builds an absolute reference by default (Microsoft example)", () => {
    expect(evaluateFormula("=ADDRESS(2,3)", context())).toBe("$C$2");
    expect(evaluateFormula("=ADDRESS(1,1)", context())).toBe("$A$1");
    expect(evaluateFormula("=ADDRESS(1048576,16384)", context())).toBe("$XFD$1048576");
  });

  it("picks absolute or relative parts with abs_num (Microsoft example)", () => {
    expect(evaluateFormula("=ADDRESS(2,3,2)", context())).toBe("C$2");
    expect(evaluateFormula("=ADDRESS(2,3,3)", context())).toBe("$C2");
    expect(evaluateFormula("=ADDRESS(2,3,4)", context())).toBe("C2");
    expect(evaluateFormula("=ADDRESS(2,3,1)", context())).toBe("$C$2");
  });

  it("writes R1C1 notation when a1 is FALSE (Microsoft example)", () => {
    expect(evaluateFormula("=ADDRESS(2,3,2,FALSE)", context())).toBe("R2C[3]");
    expect(evaluateFormula("=ADDRESS(2,3,1,FALSE)", context())).toBe("R2C3");
    expect(evaluateFormula("=ADDRESS(2,3,3,FALSE)", context())).toBe("R[2]C3");
    expect(evaluateFormula("=ADDRESS(2,3,4,FALSE)", context())).toBe("R[2]C[3]");
    expect(evaluateFormula("=ADDRESS(2,3,1,TRUE)", context())).toBe("$C$2");
  });

  it("prefixes the sheet name and quotes it when needed (Microsoft examples)", () => {
    expect(evaluateFormula('=ADDRESS(2,3,1,FALSE,"[Book1]Sheet1")', context())).toBe("[Book1]Sheet1!R2C3");
    expect(evaluateFormula('=ADDRESS(2,3,1,FALSE,"EXCEL SHEET")', context())).toBe("'EXCEL SHEET'!R2C3");
    expect(evaluateFormula('=ADDRESS(2,3,,,"Data")', context())).toBe("Data!$C$2");
    expect(evaluateFormula('=ADDRESS(2,3,4,TRUE,"it\'s")', context())).toBe("'it''s'!C2");
  });

  it("treats skipped optional arguments as their defaults", () => {
    expect(evaluateFormula("=ADDRESS(2,3,,TRUE)", context())).toBe("$C$2");
    expect(evaluateFormula("=ADDRESS(2,3,2,)", context())).toBe("C$2");
  });

  it("truncates fractional numbers and coerces numeric text", () => {
    expect(evaluateFormula("=ADDRESS(2.9,3.9)", context())).toBe("$C$2");
    expect(evaluateFormula('=ADDRESS("2","3")', context())).toBe("$C$2");
  });

  it("is #VALUE! outside the grid or for a bad abs_num", () => {
    for (const formula of [
      "=ADDRESS(0,1)",
      "=ADDRESS(1,0)",
      "=ADDRESS(1048577,1)",
      "=ADDRESS(1,16385)",
      "=ADDRESS(-1,1)",
      "=ADDRESS(1,1,0)",
      "=ADDRESS(1,1,5)",
      '=ADDRESS("x",1)',
    ]) {
      expect(code(formula), formula).toBe("#VALUE!");
    }
  });

  it("propagates errors and works inside INDIRECT", () => {
    expect(code("=ADDRESS(#N/A,1)")).toBe("#N/A");
    expect(evaluateFormula("=INDIRECT(ADDRESS(2,1))", context({ A2: 42 }))).toBe(42);
  });
});

describe("HYPERLINK", () => {
  it("returns the friendly name (Microsoft example)", () => {
    const link = "http://example.microsoft.com/report/budget report.xlsx";
    expect(evaluateFormula(`=HYPERLINK("${link}","Click for report")`, context())).toBe("Click for report");
  });

  it("returns the link text when there is no friendly name", () => {
    expect(evaluateFormula('=HYPERLINK("https://example.com")', context())).toBe("https://example.com");
  });

  it("keeps the type of the friendly name", () => {
    expect(evaluateFormula('=HYPERLINK("https://example.com",42)', context())).toBe(42);
    expect(evaluateFormula('=HYPERLINK("#Sheet1!A1",A1)', context({ A1: "Top" }))).toBe("Top");
  });

  it("propagates an error in either argument", () => {
    expect(code('=HYPERLINK(#N/A,"x")')).toBe("#N/A");
    expect(code('=HYPERLINK("x",1/0)')).toBe("#DIV/0!");
  });
});

describe("ISFORMULA", () => {
  const formulas: Record<string, string> = { A2: "=A1*2", B2: "=SUM(A1:A3)" };
  const getFormula = (_sheet: string | null, address: string) => formulas[address] ?? null;
  const ctx = context({ A1: 1, A2: 2, A3: 3 }, { getFormula });

  it("is TRUE for a cell that holds a formula and FALSE for a constant or an empty cell", () => {
    expect(evaluateFormula("=ISFORMULA(A2)", ctx)).toBe(true);
    expect(evaluateFormula("=ISFORMULA(A1)", ctx)).toBe(false);
    expect(evaluateFormula("=ISFORMULA(Z99)", ctx)).toBe(false);
  });

  it("answers per cell for a range", () => {
    expect(evaluateToMatrix("=ISFORMULA(A1:B2)", ctx)).toEqual([
      [false, false],
      [true, true],
    ]);
  });

  it("follows OFFSET and INDIRECT", () => {
    expect(evaluateFormula("=ISFORMULA(OFFSET(A1,1,0))", ctx)).toBe(true);
    expect(evaluateFormula('=ISFORMULA(INDIRECT("A1"))', ctx)).toBe(false);
  });

  it("is #VALUE! for a value that is not a reference and passes errors through", () => {
    expect(code("=ISFORMULA(5)", { getFormula })).toBe("#VALUE!");
    expect(code('=ISFORMULA("A2")', { getFormula })).toBe("#VALUE!");
    expect(code("=ISFORMULA(#N/A)", { getFormula })).toBe("#N/A");
  });

  it("is FALSE everywhere when the caller exposes no formulas", () => {
    expect(evaluateFormula("=ISFORMULA(A2)", context({ A2: 2 }))).toBe(false);
  });
});

describe("FORMULATEXT", () => {
  const formulas: Record<string, string> = { A2: "=A1*2", B2: "SUM(A1:A3)", C2: '=IF(A1>0,"yes","no")' };
  const ctx = context({ A1: 1 }, { getFormula: (_sheet, address) => formulas[address] ?? null });

  it("returns the formula as text, with its leading equals sign", () => {
    expect(evaluateFormula("=FORMULATEXT(A2)", ctx)).toBe("=A1*2");
    expect(evaluateFormula("=FORMULATEXT(B2)", ctx)).toBe("=SUM(A1:A3)");
    expect(evaluateFormula("=FORMULATEXT(C2)", ctx)).toBe('=IF(A1>0,"yes","no")');
  });

  it("is #N/A for a cell without a formula", () => {
    expect(code("=FORMULATEXT(A1)", { getFormula: () => null })).toBe("#N/A");
    expect(evaluateFormula('=IFNA(FORMULATEXT(A1),"none")', ctx)).toBe("none");
  });

  it("reads the top-left cell of a range", () => {
    expect(evaluateFormula("=FORMULATEXT(A2:C2)", ctx)).toBe("=A1*2");
  });

  it("is #VALUE! for a value that is not a reference", () => {
    expect(code("=FORMULATEXT(5)", { getFormula: () => null })).toBe("#VALUE!");
    expect(code("=FORMULATEXT(#REF!)", { getFormula: () => null })).toBe("#REF!");
  });
});

describe("registration", () => {
  it("lists the functions with a signature for the picker", () => {
    for (const name of ["ROW", "COLUMN", "ADDRESS", "HYPERLINK", "ISFORMULA", "FORMULATEXT"]) {
      const spec = lookupFunction(name);
      expect(spec, name).toBeDefined();
      expect(spec?.signature, name).toMatch(new RegExp(`^${name}\\(`));
    }
  });
});

describe("workbook integration", () => {
  function book(): Workbook {
    return { ...newWorkbook("Info"), sheets: [newSheet("Sheet1"), newSheet("Data")] };
  }

  function enter(workbook: Workbook, sheet: number, row: number, col: number, input: string): Workbook {
    return applyCellEdit(workbook, sheet, row, col, input);
  }

  function valueAt(workbook: Workbook, address: string, sheet = 0): Scalar | undefined {
    return computeSheetValues(workbook, workbook.sheets[sheet]).get(address);
  }

  it("gives ROW and COLUMN the position of their own cell", () => {
    let workbook = book();
    workbook = enter(workbook, 0, 6, 3, "=ROW()");
    workbook = enter(workbook, 0, 6, 4, "=COLUMN()");
    expect(valueAt(workbook, "D7")).toBe(7);
    expect(valueAt(workbook, "E7")).toBe(5);
  });

  it("spills ROW over a range like any dynamic array", () => {
    let workbook = book();
    workbook = enter(workbook, 0, 0, 2, "=ROW(A3:A5)");
    expect(valueAt(workbook, "C1")).toBe(3);
    expect(valueAt(workbook, "C2")).toBe(4);
    expect(valueAt(workbook, "C3")).toBe(5);
  });

  it("tells formulas from constants and recalculates when a constant becomes a formula", () => {
    let workbook = book();
    workbook = enter(workbook, 0, 0, 0, "5");
    workbook = enter(workbook, 0, 0, 1, "=ISFORMULA(A1)");
    workbook = enter(workbook, 0, 0, 2, "=FORMULATEXT(A1)");
    expect(valueAt(workbook, "B1")).toBe(false);
    const missing = valueAt(workbook, "C1");
    expect(isError(missing) && missing.code).toBe("#N/A");
    workbook = enter(workbook, 0, 0, 0, "=2+3");
    expect(valueAt(workbook, "B1")).toBe(true);
    expect(valueAt(workbook, "C1")).toBe("=2+3");
  });

  it("reads formulas on another sheet", () => {
    let workbook = book();
    workbook = enter(workbook, 1, 0, 0, "=1+1");
    workbook = enter(workbook, 0, 0, 0, "=ISFORMULA(Data!A1)");
    workbook = enter(workbook, 0, 1, 0, "=FORMULATEXT(Data!A1)");
    expect(valueAt(workbook, "A1")).toBe(true);
    expect(valueAt(workbook, "A2")).toBe("=1+1");
  });
});
