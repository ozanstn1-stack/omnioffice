/**
 * OFFSET, INDIRECT, CELL and INFO.
 *
 * The first two return references, not values, so they cannot live in the
 * plain "matrices in, scalar out" function registry: the evaluator hands them
 * the argument nodes through the context-function contract. The examples are
 * the ones Microsoft documents for each function.
 */
import { describe, expect, it } from "vitest";
import { newSheet, newWorkbook, type Workbook } from "../../lib/office-types";
import { applyCellEdit, computeSheetValues, computeWorkbookValues, spillTargetsOf } from "./cells";
import {
  collectReferences,
  evaluateFormula,
  isError,
  lookupFunction,
  type FormulaContext,
  type Scalar,
} from "./formula";
import { osVersionText, systemName } from "./functions/reference";
import { offsetReference, referenceAddress, type CellReference } from "./references";

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

function code(formula: string, values: Record<string, Scalar> = {}, extra: Partial<FormulaContext> = {}): string {
  const result = evaluateFormula(formula, context(values, extra));
  return isError(result) ? result.code : "";
}

/** The documented sample grid: numbers around C3:E5 so every offset lands on data. */
const GRID: Record<string, Scalar> = {
  C2: 1,
  D2: 2,
  E2: 3,
  C3: 4,
  D3: 5,
  E3: 6,
  C4: 7,
  D4: 8,
  E4: 9,
  C5: 10,
  D5: 11,
  E5: 12,
  F5: 99,
};

describe("offsetReference", () => {
  const base: CellReference = { kind: "reference", sheet: null, start: { row: 2, col: 2 }, end: { row: 2, col: 2 } };

  it("moves a reference and keeps its size by default", () => {
    const moved = offsetReference(base, 2, 3);
    expect(isError(moved)).toBe(false);
    expect(moved).toMatchObject({ start: { row: 4, col: 5 }, end: { row: 4, col: 5 } });
  });

  it("resizes with height and width", () => {
    expect(offsetReference(base, 0, 0, 3, 2)).toMatchObject({ start: { row: 2, col: 2 }, end: { row: 4, col: 3 } });
  });

  it("keeps the size of a multi-cell base", () => {
    const range: CellReference = { ...base, end: { row: 4, col: 4 } };
    expect(offsetReference(range, -1, 0)).toMatchObject({ start: { row: 1, col: 2 }, end: { row: 3, col: 4 } });
  });

  it("is #REF! when the result leaves the grid or has no size", () => {
    for (const result of [
      offsetReference(base, -3, 0),
      offsetReference(base, 0, -3),
      offsetReference(base, 0, 0, 0, 1),
      offsetReference(base, 0, 0, 1, -1),
      offsetReference(base, 1_048_576, 0),
      offsetReference(base, 0, 16_384),
      offsetReference(base, 0, 0, 2_000_000, 1),
    ]) {
      expect(isError(result) && result.code).toBe("#REF!");
    }
  });

  it("formats the top-left cell as an absolute address", () => {
    expect(referenceAddress(base, "Sheet1")).toBe("$C$3");
    expect(referenceAddress({ ...base, sheet: "Sheet1" }, "Sheet1")).toBe("$C$3");
    expect(referenceAddress({ ...base, sheet: "Data" }, "Sheet1")).toBe("Data!$C$3");
    expect(referenceAddress({ ...base, sheet: "My Sheet" }, "Sheet1")).toBe("'My Sheet'!$C$3");
  });
});

describe("OFFSET", () => {
  it("returns the cell at a row and column offset (Microsoft example 1)", () => {
    // =OFFSET(C3,2,3,1,1) displays the value in cell F5.
    expect(evaluateFormula("=OFFSET(C3,2,3,1,1)", context(GRID))).toBe(99);
    expect(evaluateFormula("=OFFSET(C3,2,3)", context(GRID))).toBe(99);
  });

  it("returns a range that SUM can total (Microsoft example 2)", () => {
    // =SUM(OFFSET(C3:E5,-1,0,3,3)) sums the range C2:E4.
    expect(evaluateFormula("=SUM(OFFSET(C3:E5,-1,0,3,3))", context(GRID))).toBe(1 + 2 + 3 + 4 + 5 + 6 + 7 + 8 + 9);
  });

  it("is #REF! when the reference falls off the sheet (Microsoft example 3)", () => {
    // =OFFSET(C3:E5,0,-3,3,3) refers to a range that is not on the worksheet.
    expect(code("=OFFSET(C3:E5,0,-3,3,3)", GRID)).toBe("#REF!");
  });

  it("defaults the size to that of the reference", () => {
    // C3:C4 moved down one row is C4:C5.
    expect(evaluateFormula("=SUM(OFFSET(C3:C4,1,0))", context(GRID))).toBe(7 + 10);
  });

  it("lets height and width be skipped independently", () => {
    expect(evaluateFormula("=SUM(OFFSET(C2,0,0,,2))", context(GRID))).toBe(1 + 2);
    expect(evaluateFormula("=SUM(OFFSET(C2,0,0,2,))", context(GRID))).toBe(1 + 4);
  });

  it("rejects an empty or negative size", () => {
    expect(code("=OFFSET(C3,0,0,0,1)", GRID)).toBe("#REF!");
    expect(code("=OFFSET(C3,0,0,1,-2)", GRID)).toBe("#REF!");
  });

  it("truncates fractional offsets toward zero", () => {
    expect(evaluateFormula("=OFFSET(C2,1.9,0.9)", context(GRID))).toBe(4);
    expect(evaluateFormula("=OFFSET(D3,-0.5,0)", context(GRID))).toBe(5);
  });

  it("evaluates computed offsets", () => {
    expect(evaluateFormula("=OFFSET(C2,COUNTA(C3:C5),1)", context({ ...GRID, C3: 4 }))).toBe(11);
  });

  it("chains: OFFSET of an OFFSET is a reference again", () => {
    expect(evaluateFormula("=OFFSET(OFFSET(C2,1,1),1,1)", context(GRID))).toBe(9);
  });

  it("reads another sheet through a sheet-qualified base", () => {
    const ctx = context(GRID, { sheets: { Data: { A1: 10, A2: 20 } } });
    expect(evaluateFormula("=OFFSET(Data!A1,1,0)", ctx)).toBe(20);
    expect(evaluateFormula("=SUM(OFFSET(Data!A1,0,0,2,1))", ctx)).toBe(30);
  });

  it("follows a defined name that denotes a range", () => {
    const ctx = context(GRID, { names: { BLOCK: "C2:E2" } });
    expect(evaluateFormula("=SUM(OFFSET(BLOCK,1,0))", ctx)).toBe(4 + 5 + 6);
  });

  it("is #VALUE! when the first argument is not a reference", () => {
    expect(code("=OFFSET(5,1,1)", GRID)).toBe("#VALUE!");
    expect(code('=OFFSET("C3",1,1)', GRID)).toBe("#VALUE!");
  });

  it("propagates an error from the offsets and from the base", () => {
    expect(code("=OFFSET(C3,1/0,0)", GRID)).toBe("#DIV/0!");
    expect(code('=OFFSET(C3,"x",0)', GRID)).toBe("#VALUE!");
    expect(code("=OFFSET(#N/A,1,1)", GRID)).toBe("#N/A");
  });

  it("respects the runaway range guard", () => {
    expect(code("=SUM(OFFSET(A1,0,0,1000,1000))", {}, { maxRangeCells: 100 })).toBe("#REF!");
  });

  it("needs between three and five arguments", () => {
    expect(code("=OFFSET(C3,1)", GRID)).toBe("#VALUE!");
    expect(code("=OFFSET(C3,1,1,1,1,1)", GRID)).toBe("#VALUE!");
  });
});

describe("INDIRECT", () => {
  // Microsoft's sample: A2 = "B2", A3 = "B3", A4 = "George" (a name for B4),
  // A5 = 5 and B2:B5 hold 1.333, 45, 10 and 62.
  const docs: Record<string, Scalar> = { A2: "B2", A3: "B3", A4: "George", A5: 5, B2: 1.333, B3: 45, B4: 10, B5: 62 };
  const names = { GEORGE: "B4" };

  it("reads the cell named by text (Microsoft examples)", () => {
    expect(evaluateFormula("=INDIRECT($A$2)", context(docs, { names }))).toBe(1.333);
    expect(evaluateFormula("=INDIRECT($A$3)", context(docs, { names }))).toBe(45);
    expect(evaluateFormula("=INDIRECT($A$4)", context(docs, { names }))).toBe(10);
    expect(evaluateFormula('=INDIRECT("B"&$A$5)', context(docs, { names }))).toBe(62);
  });

  it("reads ranges and absolute or lower-case spellings", () => {
    expect(evaluateFormula('=SUM(INDIRECT("B2:B5"))', context(docs))).toBeCloseTo(1.333 + 45 + 10 + 62);
    expect(evaluateFormula('=INDIRECT("$B$3")', context(docs))).toBe(45);
    expect(evaluateFormula('=INDIRECT("b3")', context(docs))).toBe(45);
    expect(evaluateFormula('=INDIRECT(" B3 ")', context(docs))).toBe(45);
  });

  it("reads other sheets, quoted or not", () => {
    const ctx = context(docs, { sheets: { Data: { B2: "data", C3: 7 }, "My Sheet": { A1: "quoted" } } });
    expect(evaluateFormula('=INDIRECT("Data!B2")', ctx)).toBe("data");
    expect(evaluateFormula('=INDIRECT("data!c3")', ctx)).toBe(7);
    expect(evaluateFormula("=INDIRECT(\"'My Sheet'!A1\")", ctx)).toBe("quoted");
    expect(evaluateFormula('=SUM(INDIRECT("Data!B2:C3"))', ctx)).toBe(7);
  });

  it("is #REF! for text that is not a reference", () => {
    for (const text of ["", "   ", "not a ref", "A0", "B2+1", "NoSuchName", "Nowhere!A1", "SUM(B2)", "1", "TRUE"]) {
      expect(code(`=INDIRECT("${text}")`, docs), text).toBe("#REF!");
    }
  });

  it("coerces non-text arguments and propagates errors", () => {
    expect(code("=INDIRECT(7)", docs)).toBe("#REF!");
    expect(code("=INDIRECT(#N/A)", docs)).toBe("#N/A");
    expect(code("=INDIRECT(1/0)", docs)).toBe("#DIV/0!");
  });

  it("accepts the A1 flag; R1C1 style is not supported and is #REF!", () => {
    expect(evaluateFormula('=INDIRECT("B3",TRUE)', context(docs))).toBe(45);
    expect(evaluateFormula('=INDIRECT("B3",1)', context(docs))).toBe(45);
    expect(evaluateFormula('=INDIRECT("B3",)', context(docs))).toBe(45);
    expect(code('=INDIRECT("R3C2",FALSE)', docs)).toBe("#REF!");
    expect(code('=INDIRECT("B3",0)', docs)).toBe("#REF!");
  });

  it("composes with OFFSET", () => {
    expect(evaluateFormula('=OFFSET(INDIRECT("B2"),1,0)', context(docs))).toBe(45);
    expect(evaluateFormula('=INDIRECT("B"&CELL("row",OFFSET(A1,2,0)))', context(docs))).toBe(45);
  });
});

describe("CELL", () => {
  const values: Record<string, Scalar> = { A2: "label", A3: 42, A4: true, B5: 3.5 };

  it("reports the row and the column (Microsoft examples)", () => {
    expect(evaluateFormula('=CELL("row",A20)', context(values))).toBe(20);
    expect(evaluateFormula('=CELL("col",B5)', context(values))).toBe(2);
    expect(evaluateFormula('=CELL("col",AA1)', context(values))).toBe(27);
  });

  it("formats the address as an absolute reference of the first cell", () => {
    expect(evaluateFormula('=CELL("address",A1)', context(values))).toBe("$A$1");
    expect(evaluateFormula('=CELL("address",B2:C3)', context(values))).toBe("$B$2");
    expect(evaluateFormula('=CELL("address",Data!C3)', context(values))).toBe("Data!$C$3");
  });

  it("reads the contents of the first cell", () => {
    expect(evaluateFormula('=CELL("contents",A2)', context(values))).toBe("label");
    expect(evaluateFormula('=CELL("contents",A3:A4)', context(values))).toBe(42);
  });

  it("classifies the cell as blank, label or value", () => {
    expect(evaluateFormula('=CELL("type",Z9)', context(values))).toBe("b");
    expect(evaluateFormula('=CELL("type",A2)', context(values))).toBe("l");
    expect(evaluateFormula('=CELL("type",A3)', context(values))).toBe("v");
    expect(evaluateFormula('=CELL("type",A4)', context(values))).toBe("v");
  });

  it("works on the result of OFFSET and INDIRECT", () => {
    expect(evaluateFormula('=CELL("address",OFFSET(A1,2,2))', context(values))).toBe("$C$3");
    expect(evaluateFormula('=CELL("row",INDIRECT("D7"))', context(values))).toBe(7);
  });

  it("describes the formula's own cell when the reference is omitted", () => {
    const ctx = context(values, { currentAddress: "C4" });
    expect(evaluateFormula('=CELL("address")', ctx)).toBe("$C$4");
    expect(evaluateFormula('=CELL("row")', ctx)).toBe(4);
    expect(evaluateFormula('=CELL("col")', ctx)).toBe(3);
    expect(code('=CELL("row")', values)).toBe("#VALUE!");
  });

  it("ignores the case of the info type and rejects unknown ones", () => {
    expect(evaluateFormula('=CELL("ROW",A20)', context(values))).toBe(20);
    expect(code('=CELL("nonsense",A1)', values)).toBe("#VALUE!");
    expect(code('=CELL("row",5)', values)).toBe("#VALUE!");
    expect(code("=CELL(1,A1)", values)).toBe("#VALUE!");
  });
});

describe("INFO", () => {
  it("answers the minimal set", () => {
    expect(evaluateFormula('=INFO("recalc")', context())).toBe("Automatic");
    expect(evaluateFormula('=INFO("numfile")', context())).toBe(3);
    expect(typeof evaluateFormula('=INFO("osversion")', context())).toBe("string");
    expect(["pcdos", "mac"]).toContain(evaluateFormula('=INFO("system")', context()));
    expect(evaluateFormula('=INFO("OSVERSION")', context())).toBe(evaluateFormula('=INFO("osversion")', context()));
  });

  it("is #VALUE! for an unknown type", () => {
    expect(code('=INFO("nonsense")')).toBe("#VALUE!");
    expect(code("=INFO(1)")).toBe("#VALUE!");
  });

  it("derives the operating system from the user agent", () => {
    const windows = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/120.0 Safari/537.36";
    expect(osVersionText(windows)).toBe("Windows (64-bit) NT 10.00");
    expect(osVersionText("Mozilla/5.0 (Windows NT 6.1; WOW64)")).toBe("Windows (64-bit) NT 6.10");
    expect(osVersionText("Mozilla/5.0 (Windows NT 5.1)")).toBe("Windows (32-bit) NT 5.10");
    expect(systemName(windows)).toBe("pcdos");
    const mac = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15";
    expect(osVersionText(mac)).toBe("Macintosh (Intel) 10.15.7");
    expect(systemName(mac)).toBe("mac");
    expect(osVersionText("Mozilla/5.0 (X11; Linux x86_64)")).toBe("Linux");
    expect(osVersionText("Mozilla/5.0 (Linux; Android 14; Pixel 8)")).toBe("Android 14");
    expect(systemName("Mozilla/5.0 (X11; Linux x86_64)")).toBe("pcdos");
    expect(osVersionText("")).toBe("Unknown");
  });
});

describe("registration", () => {
  it("lists the four functions with a signature for the picker", () => {
    for (const name of ["OFFSET", "INDIRECT", "CELL", "INFO"]) {
      const spec = lookupFunction(name);
      expect(spec, name).toBeDefined();
      expect(spec?.signature, name).toMatch(new RegExp(`^${name}\\(`));
    }
  });

  it("treats all of them as volatile for the dependency graph", () => {
    for (const formula of ["=OFFSET(A1,1,1)", '=INDIRECT("A1")', '=CELL("row",A1)', '=INFO("system")']) {
      expect(collectReferences(formula)?.volatile, formula).toBe(true);
    }
  });
});

describe("workbook integration", () => {
  function book(): Workbook {
    const workbook = newWorkbook("Reference");
    return { ...workbook, sheets: [newSheet("Sheet1"), newSheet("Data")] };
  }

  function enter(workbook: Workbook, sheet: number, address: string, input: string): Workbook {
    const match = address.match(/^([A-Z]+)(\d+)$/)!;
    const col = match[1].split("").reduce((sum, letter) => sum * 26 + letter.charCodeAt(0) - 64, 0) - 1;
    return applyCellEdit(workbook, sheet, Number(match[2]) - 1, col, input);
  }

  function valueAt(workbook: Workbook, address: string, sheet = 0): Scalar | undefined {
    return computeSheetValues(workbook, workbook.sheets[sheet]).get(address);
  }

  it("recalculates OFFSET when a cell it reaches changes", () => {
    let workbook = book();
    workbook = enter(workbook, 0, "A1", "1");
    workbook = enter(workbook, 0, "A2", "5");
    workbook = enter(workbook, 0, "B1", "=OFFSET(A1,1,0)");
    expect(valueAt(workbook, "B1")).toBe(5);
    // A2 is not in B1's static precedents (only A1 is); volatility is what
    // keeps the formula fresh.
    workbook = enter(workbook, 0, "A2", "9");
    expect(valueAt(workbook, "B1")).toBe(9);
  });

  it("recalculates INDIRECT when the text or the target changes", () => {
    let workbook = book();
    workbook = enter(workbook, 0, "A1", "A3");
    workbook = enter(workbook, 0, "A3", "7");
    workbook = enter(workbook, 0, "A4", "8");
    workbook = enter(workbook, 0, "B1", "=INDIRECT(A1)");
    expect(valueAt(workbook, "B1")).toBe(7);
    workbook = enter(workbook, 0, "A3", "70");
    expect(valueAt(workbook, "B1")).toBe(70);
    workbook = enter(workbook, 0, "A1", "A4");
    expect(valueAt(workbook, "B1")).toBe(8);
  });

  it("reads a formula result through INDIRECT after the formula's input changes", () => {
    let workbook = book();
    workbook = enter(workbook, 0, "A1", "2");
    workbook = enter(workbook, 0, "A2", "=A1*10");
    workbook = enter(workbook, 0, "B1", '=INDIRECT("A2")');
    expect(valueAt(workbook, "B1")).toBe(20);
    workbook = enter(workbook, 0, "A1", "3");
    expect(valueAt(workbook, "B1")).toBe(30);
  });

  it("reaches other sheets with INDIRECT and OFFSET", () => {
    let workbook = book();
    workbook = enter(workbook, 1, "A1", "11");
    workbook = enter(workbook, 1, "A2", "22");
    workbook = enter(workbook, 0, "A1", '=INDIRECT("Data!A2")');
    workbook = enter(workbook, 0, "A2", "=OFFSET(Data!A1,1,0)");
    expect(valueAt(workbook, "A1")).toBe(22);
    expect(valueAt(workbook, "A2")).toBe(22);
    workbook = enter(workbook, 1, "A2", "33");
    expect(valueAt(workbook, "A1")).toBe(33);
    expect(valueAt(workbook, "A2")).toBe(33);
  });

  it("spills a multi-cell OFFSET like any dynamic array", () => {
    let workbook = book();
    for (const [address, input] of [
      ["A1", "1"],
      ["A2", "2"],
      ["A3", "3"],
    ] as const) {
      workbook = enter(workbook, 0, address, input);
    }
    workbook = enter(workbook, 0, "C1", "=OFFSET(A1,0,0,3,1)");
    expect(valueAt(workbook, "C1")).toBe(1);
    expect(valueAt(workbook, "C2")).toBe(2);
    expect(valueAt(workbook, "C3")).toBe(3);
    expect(spillTargetsOf(workbook, "Sheet1", "C1")).toEqual(["Sheet1!C2", "Sheet1!C3"]);
  });

  it("reports #SPILL! when the OFFSET spill is blocked", () => {
    let workbook = book();
    workbook = enter(workbook, 0, "A1", "1");
    workbook = enter(workbook, 0, "A2", "2");
    workbook = enter(workbook, 0, "C2", "blocker");
    workbook = enter(workbook, 0, "C1", "=OFFSET(A1,0,0,2,1)");
    const result = valueAt(workbook, "C1");
    expect(isError(result) && result.code).toBe("#SPILL!");
  });

  it("detects a cell that reads itself through INDIRECT", () => {
    let workbook = book();
    workbook = enter(workbook, 0, "A1", '=INDIRECT("A1")');
    const result = valueAt(workbook, "A1");
    expect(isError(result)).toBe(true);
  });

  it("gives CELL the address of its own cell, on edit and on a later recalculation", () => {
    let workbook = book();
    workbook = enter(workbook, 0, "C4", '=CELL("address")');
    expect(valueAt(workbook, "C4")).toBe("$C$4");
    // A full pass (a fresh workbook object) must agree with the edit path.
    expect(computeWorkbookValues({ ...workbook }).get("Sheet1!C4")).toBe("$C$4");
  });
});
