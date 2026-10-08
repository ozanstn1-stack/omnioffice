/**
 * Error values in the grid: every code the engine produces is classified and
 * explained in both languages, and `#CALC!` behaves like the other errors in
 * the functions that look at errors.
 */
import { describe, expect, it } from "vitest";
import { makeTranslate } from "../../lib/i18n";
import { newSheet, newWorkbook, type Workbook } from "../../lib/office-types";
import { applyCellEdit, computeSheetValues } from "./cells";
import { errorExplanationKey, errorTitle } from "./error-info";
import { ERR, evaluateFormula, isError, type FormulaContext, type Scalar } from "./formula";

function context(values: Record<string, Scalar> = {}): FormulaContext {
  return {
    getValue: (_sheet, address) => (address in values ? values[address] : ""),
    sheetNames: ["Sheet1"],
    currentSheet: "Sheet1",
  };
}

describe("error explanations", () => {
  it("covers every error code the engine produces", () => {
    const codes = Object.values(ERR).map((make) => make().code);
    expect(codes).toContain("#CALC!");
    for (const code of codes) expect(errorExplanationKey(code), code).not.toBeNull();
  });

  it("has an English and a Turkish text for each code", () => {
    const en = makeTranslate("en");
    const tr = makeTranslate("tr");
    for (const code of Object.values(ERR).map((make) => make().code)) {
      const key = errorExplanationKey(code)!;
      expect(en(key), `${code} en`).not.toBe(key);
      expect(tr(key), `${code} tr`).not.toBe(key);
      expect(tr(key)).not.toBe(en(key));
    }
  });

  it("titles a code with its explanation and ignores unknown codes", () => {
    const en = makeTranslate("en");
    expect(errorTitle(en, "#CALC!")).toBe("#CALC!: The array is empty.");
    expect(errorTitle(en, "#calc!")).toBe("#calc!: The array is empty.");
    expect(errorTitle(en, "#WHAT?")).toBeNull();
  });
});

describe("#CALC! among the error-aware functions", () => {
  const calc: Record<string, Scalar> = { A1: ERR.calc(), A2: 1, A3: ERR.calc() };

  it("is an error to the ISERROR family but not to ISNA", () => {
    expect(evaluateFormula("=ISERROR(A1)", context(calc))).toBe(true);
    expect(evaluateFormula("=ISERR(A1)", context(calc))).toBe(true);
    expect(evaluateFormula("=ISNA(A1)", context(calc))).toBe(false);
    expect(evaluateFormula("=ISERROR(A2)", context(calc))).toBe(false);
  });

  it("is replaced by IFERROR and left alone by IFNA", () => {
    expect(evaluateFormula("=IFERROR(A1,0)", context(calc))).toBe(0);
    const passed = evaluateFormula("=IFNA(A1,0)", context(calc));
    expect(isError(passed) && passed.code).toBe("#CALC!");
  });

  it("is counted by COUNTIF when the criterion names it, like the other errors", () => {
    expect(evaluateFormula('=COUNTIF(A1:A3,"#CALC!")', context(calc))).toBe(2);
    expect(evaluateFormula('=COUNTIF(A1:A3,"=#CALC!")', context(calc))).toBe(2);
    expect(evaluateFormula('=COUNTIF(A1:A3,"#N/A")', context({ ...calc, A2: ERR.na() }))).toBe(1);
    // A number criterion still skips error cells.
    expect(evaluateFormula("=COUNTIF(A1:A3,1)", context(calc))).toBe(1);
  });

  it("propagates through arithmetic and aggregates like any error", () => {
    const sum = evaluateFormula("=SUM(A1:A3)", context(calc));
    expect(isError(sum) && sum.code).toBe("#CALC!");
  });

  it("is what an empty array result leaves in the cell", () => {
    let book: Workbook = { ...newWorkbook("Book"), sheets: [newSheet("Sheet1")] };
    book = applyCellEdit(book, 0, 0, 0, "=TAKE({1,2},0)");
    const value = computeSheetValues(book, book.sheets[0]).get("A1");
    expect(isError(value) && value.code).toBe("#CALC!");
  });
});
