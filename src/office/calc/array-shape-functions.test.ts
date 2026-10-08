/**
 * Array-shaping functions: HSTACK, VSTACK, TAKE, DROP, CHOOSECOLS, CHOOSEROWS,
 * EXPAND, TOCOL, TOROW, WRAPROWS and WRAPCOLS.
 *
 * They return matrices, which the workbook spills over the cells below and to
 * the right of the formula. The cases follow the behaviour Microsoft documents
 * for each function: padding with #N/A, negative counts that count from the
 * end, #CALC! for an empty result and #VALUE! for an index that is not there.
 */
import { describe, expect, it } from "vitest";
import { newSheet, newWorkbook, type Workbook } from "../../lib/office-types";
import { applyCellEdit, computeSheetValues, parseInputValue, spillTargetsOf } from "./cells";
import {
  evaluateFormula,
  evaluateToMatrix,
  isError,
  lookupFunction,
  type FormulaContext,
  type Scalar,
} from "./formula";

function context(values: Record<string, Scalar> = {}): FormulaContext {
  return {
    getValue: (_sheet, address) => (address in values ? values[address] : ""),
    sheetNames: ["Sheet1"],
    currentSheet: "Sheet1",
  };
}

/** The result as plain data, with error values written as their codes. */
function grid(formula: string, values: Record<string, Scalar> = {}): Array<Array<number | string | boolean>> {
  return evaluateToMatrix(formula, context(values)).map((row) =>
    row.map((value) => (isError(value) ? value.code : value)),
  );
}

function code(formula: string, values: Record<string, Scalar> = {}): string {
  const result = evaluateFormula(formula, context(values));
  return isError(result) ? result.code : "";
}

const NINE = "{1,2,3;4,5,6;7,8,9}";

describe("HSTACK and VSTACK", () => {
  it("HSTACK joins arrays side by side and pads a shorter one with #N/A", () => {
    expect(grid("=HSTACK({1;2;3},{4,5;6,7})")).toEqual([
      [1, 4, 5],
      [2, 6, 7],
      [3, "#N/A", "#N/A"],
    ]);
  });

  it("VSTACK joins arrays top to bottom and pads a narrower one with #N/A", () => {
    expect(grid("=VSTACK({1,2,3},{4,5})")).toEqual([
      [1, 2, 3],
      [4, 5, "#N/A"],
    ]);
  });

  it("treat single values as one-cell arrays", () => {
    expect(grid("=HSTACK(1,2,3)")).toEqual([[1, 2, 3]]);
    expect(grid("=VSTACK(1,2,3)")).toEqual([[1], [2], [3]]);
    expect(grid('=VSTACK("a",{1,2})')).toEqual([
      ["a", "#N/A"],
      [1, 2],
    ]);
  });

  it("return a lone argument unchanged", () => {
    expect(grid(`=HSTACK(${NINE})`)).toEqual(grid(`=${NINE}`));
    expect(grid("=VSTACK({1,2})")).toEqual([[1, 2]]);
  });

  it("keep errors inside the arrays as values", () => {
    expect(grid("=HSTACK({1,#N/A},2)")).toEqual([[1, "#N/A", 2]]);
    expect(grid("=VSTACK(#DIV/0!,1)")).toEqual([["#DIV/0!"], [1]]);
  });

  it("read ranges", () => {
    const values = { A1: 1, A2: 2, B1: "x" };
    expect(grid("=HSTACK(A1:A2,B1:B2)", values)).toEqual([
      [1, "x"],
      [2, ""],
    ]);
  });
});

describe("TAKE and DROP", () => {
  it("TAKE with a positive count keeps the first rows or columns", () => {
    expect(grid(`=TAKE(${NINE},2)`)).toEqual([
      [1, 2, 3],
      [4, 5, 6],
    ]);
    expect(grid(`=TAKE(${NINE},,2)`)).toEqual([
      [1, 2],
      [4, 5],
      [7, 8],
    ]);
    expect(grid(`=TAKE(${NINE},1,1)`)).toEqual([[1]]);
  });

  it("TAKE with a negative count keeps the last rows or columns", () => {
    expect(grid(`=TAKE(${NINE},-1)`)).toEqual([[7, 8, 9]]);
    expect(grid(`=TAKE(${NINE},2,-2)`)).toEqual([
      [2, 3],
      [5, 6],
    ]);
  });

  it("TAKE asks for more than there is: it returns everything", () => {
    expect(grid(`=TAKE(${NINE},10)`)).toEqual(grid(`=${NINE}`));
    expect(grid(`=TAKE(${NINE},-10,10)`)).toEqual(grid(`=${NINE}`));
  });

  it("TAKE of nothing is #CALC!", () => {
    expect(code(`=TAKE(${NINE},0)`)).toBe("#CALC!");
    expect(code(`=TAKE(${NINE},,0)`)).toBe("#CALC!");
  });

  it("TAKE truncates fractions and rejects text", () => {
    expect(grid(`=TAKE(${NINE},1.9)`)).toEqual([[1, 2, 3]]);
    expect(code(`=TAKE(${NINE},"x")`)).toBe("#VALUE!");
    expect(code(`=TAKE(${NINE},#N/A)`)).toBe("#N/A");
  });

  it("DROP with a positive count removes the first rows or columns", () => {
    expect(grid(`=DROP(${NINE},1)`)).toEqual([
      [4, 5, 6],
      [7, 8, 9],
    ]);
    expect(grid(`=DROP(${NINE},,1)`)).toEqual([
      [2, 3],
      [5, 6],
      [8, 9],
    ]);
  });

  it("DROP with a negative count removes the last rows or columns", () => {
    expect(grid(`=DROP(${NINE},-1)`)).toEqual([
      [1, 2, 3],
      [4, 5, 6],
    ]);
    expect(grid(`=DROP(${NINE},1,-1)`)).toEqual([
      [4, 5],
      [7, 8],
    ]);
  });

  it("DROP of everything is #CALC!", () => {
    expect(code(`=DROP(${NINE},3)`)).toBe("#CALC!");
    expect(code(`=DROP(${NINE},-5)`)).toBe("#CALC!");
    expect(code(`=DROP(${NINE},,3)`)).toBe("#CALC!");
  });

  it("DROP of 0 keeps the array", () => {
    expect(grid(`=DROP(${NINE},0)`)).toEqual(grid(`=${NINE}`));
  });
});

describe("CHOOSECOLS and CHOOSEROWS", () => {
  it("CHOOSECOLS picks columns in the order given, repeats allowed", () => {
    expect(grid(`=CHOOSECOLS(${NINE},3,1,3)`)).toEqual([
      [3, 1, 3],
      [6, 4, 6],
      [9, 7, 9],
    ]);
  });

  it("CHOOSECOLS counts negative indexes from the end and takes an array of indexes", () => {
    expect(grid(`=CHOOSECOLS(${NINE},-1,1)`)).toEqual([
      [3, 1],
      [6, 4],
      [9, 7],
    ]);
    expect(grid(`=CHOOSECOLS(${NINE},{1,2})`)).toEqual([
      [1, 2],
      [4, 5],
      [7, 8],
    ]);
  });

  it("CHOOSEROWS picks rows in the order given", () => {
    expect(grid(`=CHOOSEROWS(${NINE},3,1)`)).toEqual([
      [7, 8, 9],
      [1, 2, 3],
    ]);
    expect(grid(`=CHOOSEROWS(${NINE},-1)`)).toEqual([[7, 8, 9]]);
    expect(grid(`=CHOOSEROWS(${NINE},{1;2})`)).toEqual([
      [1, 2, 3],
      [4, 5, 6],
    ]);
  });

  it("an index of 0 or past the edge is #VALUE!", () => {
    for (const index of [0, 4, -4]) expect(code(`=CHOOSECOLS(${NINE},${index})`), String(index)).toBe("#VALUE!");
    for (const index of [0, 4, -4]) expect(code(`=CHOOSEROWS(${NINE},${index})`), String(index)).toBe("#VALUE!");
    expect(code(`=CHOOSECOLS(${NINE},1,9)`)).toBe("#VALUE!");
    expect(code(`=CHOOSECOLS(${NINE},"x")`)).toBe("#VALUE!");
  });
});

describe("EXPAND", () => {
  it("pads with #N/A by default", () => {
    expect(grid("=EXPAND({1,2;3,4},3,3)")).toEqual([
      [1, 2, "#N/A"],
      [3, 4, "#N/A"],
      ["#N/A", "#N/A", "#N/A"],
    ]);
  });

  it("pads with the value given", () => {
    expect(grid('=EXPAND({1,2;3,4},3,3,"x")')).toEqual([
      [1, 2, "x"],
      [3, 4, "x"],
      ["x", "x", "x"],
    ]);
    expect(grid("=EXPAND({1,2},2,,0)")).toEqual([
      [1, 2],
      [0, 0],
    ]);
  });

  it("keeps the other dimension when it is skipped", () => {
    expect(grid("=EXPAND({1,2;3,4},,3,0)")).toEqual([
      [1, 2, 0],
      [3, 4, 0],
    ]);
    expect(grid("=EXPAND({1,2;3,4},3)")).toEqual([
      [1, 2],
      [3, 4],
      ["#N/A", "#N/A"],
    ]);
  });

  it("cannot shrink the array", () => {
    expect(code("=EXPAND({1,2;3,4},1,2)")).toBe("#VALUE!");
    expect(code("=EXPAND({1,2;3,4},2,1)")).toBe("#VALUE!");
  });

  it("refuses an absurd size", () => {
    expect(code("=EXPAND({1},1000000,1000)")).toBe("#NUM!");
  });
});

describe("TOCOL and TOROW", () => {
  it("TOCOL scans row by row by default", () => {
    expect(grid("=TOCOL({1,2;3,4})")).toEqual([[1], [2], [3], [4]]);
  });

  it("TOCOL can scan column by column", () => {
    expect(grid("=TOCOL({1,2;3,4},,TRUE)")).toEqual([[1], [3], [2], [4]]);
    expect(grid("=TOCOL({1,2;3,4},0,TRUE)")).toEqual([[1], [3], [2], [4]]);
  });

  it("TOROW lays the values out in one row", () => {
    expect(grid("=TOROW({1,2;3,4})")).toEqual([[1, 2, 3, 4]]);
    expect(grid("=TOROW({1,2;3,4},,TRUE)")).toEqual([[1, 3, 2, 4]]);
  });

  it("ignore skips blanks (1), errors (2) or both (3)", () => {
    // Microsoft: ignore 1 drops blank cells, 2 drops errors, 3 drops both.
    const data = { A1: 1, B1: "", A2: evaluateFormula("=NA()", context()), B2: 4 };
    expect(grid("=TOCOL(A1:B2,1)", data)).toEqual([[1], ["#N/A"], [4]]);
    expect(grid("=TOCOL(A1:B2,2)", data)).toEqual([[1], [""], [4]]);
    expect(grid("=TOCOL(A1:B2,3)", data)).toEqual([[1], [4]]);
    expect(grid("=TOCOL(A1:B2,0)", data)).toEqual([[1], [""], ["#N/A"], [4]]);
  });

  it("an empty result is #CALC!, a bad ignore code is #VALUE!", () => {
    expect(code("=TOCOL(A1:B2,1)")).toBe("#CALC!");
    expect(code("=TOROW(A1:B2,1)")).toBe("#CALC!");
    expect(code("=TOCOL({1,2},4)")).toBe("#VALUE!");
    expect(code("=TOCOL({1,2},-1)")).toBe("#VALUE!");
    expect(code('=TOCOL({1,2},"x")')).toBe("#VALUE!");
    expect(code('=TOCOL({1,2},,"x")')).toBe("#VALUE!");
  });
});

describe("WRAPROWS and WRAPCOLS", () => {
  const ten = "{1,2,3,4,5,6,7,8,9,10}";

  it("WRAPROWS fills rows of the given width and pads the last with #N/A", () => {
    expect(grid(`=WRAPROWS(${ten},3)`)).toEqual([
      [1, 2, 3],
      [4, 5, 6],
      [7, 8, 9],
      [10, "#N/A", "#N/A"],
    ]);
  });

  it("WRAPCOLS fills columns of the given height and pads the last with #N/A", () => {
    expect(grid(`=WRAPCOLS(${ten},3)`)).toEqual([
      [1, 4, 7, 10],
      [2, 5, 8, "#N/A"],
      [3, 6, 9, "#N/A"],
    ]);
  });

  it("pad with the value given", () => {
    expect(grid(`=WRAPROWS(${ten},4,0)`)).toEqual([
      [1, 2, 3, 4],
      [5, 6, 7, 8],
      [9, 10, 0, 0],
    ]);
    expect(grid('=WRAPCOLS({1,2,3},2,"-")')).toEqual([
      [1, 3],
      [2, "-"],
    ]);
  });

  it("accept a column as well as a row, and a wrap count wider than the data", () => {
    expect(grid("=WRAPROWS({1;2;3;4},2)")).toEqual([
      [1, 2],
      [3, 4],
    ]);
    expect(grid("=WRAPROWS({1,2,3},5)")).toEqual([[1, 2, 3, "#N/A", "#N/A"]]);
    expect(grid("=WRAPCOLS({1,2,3},5)")).toEqual([[1], [2], [3], ["#N/A"], ["#N/A"]]);
  });

  it("reject a two-dimensional vector (#VALUE!) and a wrap count below 1 (#NUM!)", () => {
    expect(code("=WRAPROWS({1,2;3,4},2)")).toBe("#VALUE!");
    expect(code("=WRAPCOLS({1,2;3,4},2)")).toBe("#VALUE!");
    expect(code("=WRAPROWS({1,2,3},0)")).toBe("#NUM!");
    expect(code("=WRAPCOLS({1,2,3},-1)")).toBe("#NUM!");
    expect(code('=WRAPROWS({1,2,3},"x")')).toBe("#VALUE!");
  });
});

describe("#CALC!", () => {
  it("is a literal IFERROR and friends can test", () => {
    expect(evaluateFormula("=IFERROR(#CALC!,7)", context())).toBe(7);
    expect(code("=#CALC!")).toBe("#CALC!");
    expect(evaluateFormula('=IFERROR(TAKE({1,2},0),"empty")', context())).toBe("empty");
    expect(evaluateFormula("=ISERROR(DROP({1,2},5))", context())).toBe(true);
  });

  it("is read as an error when typed into a cell", () => {
    expect(parseInputValue("#CALC!")).toEqual({ kind: "error", value: "#CALC!" });
  });
});

describe("composition", () => {
  it("shapes work on each other's output", () => {
    expect(grid(`=TAKE(VSTACK(${NINE},{10,11,12}),-1)`)).toEqual([[10, 11, 12]]);
    expect(grid(`=TOROW(CHOOSECOLS(${NINE},1))`)).toEqual([[1, 4, 7]]);
    expect(grid("=TRANSPOSE(HSTACK({1;2},{3;4}))")).toEqual([
      [1, 2],
      [3, 4],
    ]);
    expect(evaluateFormula(`=SUM(DROP(${NINE},1,1))`, context())).toBe(5 + 6 + 8 + 9);
  });
});

describe("registration", () => {
  it("lists every function with a signature for the picker", () => {
    for (const name of [
      "HSTACK",
      "VSTACK",
      "TAKE",
      "DROP",
      "CHOOSECOLS",
      "CHOOSEROWS",
      "EXPAND",
      "TOCOL",
      "TOROW",
      "WRAPROWS",
      "WRAPCOLS",
    ]) {
      const spec = lookupFunction(name);
      expect(spec, name).toBeDefined();
      expect(spec?.signature, name).toMatch(new RegExp(`^${name}\\(`));
      expect(spec?.category, name).toBe("Array");
    }
  });
});

describe("workbook integration", () => {
  function book(): Workbook {
    return { ...newWorkbook("Shapes"), sheets: [newSheet("Sheet1")] };
  }

  function valueAt(workbook: Workbook, address: string): Scalar | undefined {
    return computeSheetValues(workbook, workbook.sheets[0]).get(address);
  }

  it("spills a stacked range over the cells it needs", () => {
    let workbook = book();
    for (const [index, input] of ["1", "2", "3"].entries()) workbook = applyCellEdit(workbook, 0, index, 0, input);
    workbook = applyCellEdit(workbook, 0, 0, 2, "=HSTACK(A1:A3,A1:A3*10)");
    expect(valueAt(workbook, "C1")).toBe(1);
    expect(valueAt(workbook, "D1")).toBe(10);
    expect(valueAt(workbook, "D3")).toBe(30);
    expect(spillTargetsOf(workbook, "Sheet1", "C1")).toEqual([
      "Sheet1!D1",
      "Sheet1!C2",
      "Sheet1!D2",
      "Sheet1!C3",
      "Sheet1!D3",
    ]);
  });

  it("recalculates the spill when the source changes", () => {
    let workbook = book();
    workbook = applyCellEdit(workbook, 0, 0, 0, "5");
    workbook = applyCellEdit(workbook, 0, 1, 0, "6");
    workbook = applyCellEdit(workbook, 0, 0, 2, "=TOROW(A1:A2)");
    expect(valueAt(workbook, "D1")).toBe(6);
    workbook = applyCellEdit(workbook, 0, 1, 0, "60");
    expect(valueAt(workbook, "D1")).toBe(60);
  });

  it("shows #CALC! in the cell for an empty result", () => {
    let workbook = book();
    workbook = applyCellEdit(workbook, 0, 0, 0, "=TAKE({1,2},0)");
    const result = valueAt(workbook, "A1");
    expect(isError(result) && result.code).toBe("#CALC!");
  });
});
