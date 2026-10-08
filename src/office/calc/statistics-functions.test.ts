/**
 * Descriptive statistics and regression added for Excel parity: MODE.*,
 * GEOMEAN/HARMEAN, AVEDEV/DEVSQ, PERCENTRANK.*, the *.EXC percentiles, and
 * SLOPE/INTERCEPT/RSQ/STEYX/FORECAST/TREND/GROWTH.
 *
 * Expected values are the examples Microsoft documents for each function; the
 * comment on a case names the example it comes from. Where the help page only
 * lists the inputs (TREND), the result is checked against an independent
 * least-squares fit.
 */
import { describe, expect, it } from "vitest";
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

function n(formula: string, values: Record<string, Scalar> = {}): number {
  const result = evaluateFormula(formula, context(values));
  if (isError(result)) throw new Error(`${formula} returned ${result.code}`);
  if (typeof result !== "number") throw new Error(`${formula} returned ${String(result)}`);
  return result;
}

function code(formula: string, values: Record<string, Scalar> = {}): string {
  const result = evaluateFormula(formula, context(values));
  return isError(result) ? result.code : "";
}

function matrix(formula: string, values: Record<string, Scalar> = {}): Scalar[][] {
  return evaluateToMatrix(formula, context(values));
}

/** Rounds every number in a matrix, so a whole table can be compared at once. */
function rounded(grid: Scalar[][], digits: number): Scalar[][] {
  return grid.map((row) => row.map((value) => (typeof value === "number" ? Number(value.toFixed(digits)) : value)));
}

describe("MODE, MODE.SNGL and MODE.MULT", () => {
  it("returns the most frequent number (Microsoft example)", () => {
    expect(n("=MODE({5.6,4,4,3,2,4})")).toBe(4);
    expect(n("=MODE.SNGL({5.6,4,4,3,2,4})")).toBe(4);
  });

  it("breaks a tie with the number that appears first", () => {
    expect(n("=MODE.SNGL({3,1,1,3,2})")).toBe(3);
    expect(n("=MODE({2,1,2,1})")).toBe(2);
  });

  it("is #N/A when no number repeats", () => {
    expect(code("=MODE.SNGL({1,2,3})")).toBe("#N/A");
    expect(code("=MODE(A1:A3)")).toBe("#N/A");
  });

  it("ignores text, booleans and blanks in a range and joins several arguments", () => {
    const values = { A1: 7, A2: "7", A3: true, A4: "", A5: 7, B1: 2, B2: 2, B3: 2 };
    expect(n("=MODE.SNGL(A1:A5)", values)).toBe(7);
    expect(n("=MODE.SNGL(A1:A5,B1:B3)", values)).toBe(2);
  });

  it("MODE.MULT lists every mode as a column (Microsoft example)", () => {
    // Microsoft: 1,2,3,4,3,2,1,2,3,5,6,1 repeats 1, 2 and 3 three times each.
    expect(matrix("=MODE.MULT({1,2,3,4,3,2,1,2,3,5,6,1})")).toEqual([[1], [2], [3]]);
    expect(matrix("=MODE.MULT({4,4,9})")).toEqual([[4]]);
  });

  it("MODE.MULT is #N/A when nothing repeats", () => {
    expect(code("=MODE.MULT({1,2,3})")).toBe("#N/A");
  });
});

describe("GEOMEAN, HARMEAN, AVEDEV and DEVSQ", () => {
  it("GEOMEAN follows the Microsoft example", () => {
    expect(n("=GEOMEAN(4,5,8,7,11,4,3)")).toBeCloseTo(5.476987, 6);
  });

  it("GEOMEAN is #NUM! for a non-positive value or no data", () => {
    expect(code("=GEOMEAN(4,0,8)")).toBe("#NUM!");
    expect(code("=GEOMEAN(4,-1,8)")).toBe("#NUM!");
    expect(code("=GEOMEAN(A1:A3)")).toBe("#NUM!");
  });

  it("HARMEAN follows the Microsoft example", () => {
    expect(n("=HARMEAN(4,5,8,7,11,4,3)")).toBeCloseTo(5.028376, 6);
  });

  it("HARMEAN is #NUM! for a non-positive value or no data", () => {
    expect(code("=HARMEAN(4,0,8)")).toBe("#NUM!");
    expect(code("=HARMEAN(A1:A3)")).toBe("#NUM!");
  });

  it("AVEDEV follows the Microsoft example", () => {
    expect(n("=AVEDEV(4,5,6,7,5,4,3)")).toBeCloseTo(1.020408, 6);
  });

  it("DEVSQ follows the Microsoft example", () => {
    expect(n("=DEVSQ(4,5,8,7,11,4,3)")).toBeCloseTo(48, 10);
  });

  it("AVEDEV and DEVSQ are #NUM! without data and skip non-numbers in a range", () => {
    expect(code("=AVEDEV(A1:A3)")).toBe("#NUM!");
    expect(code("=DEVSQ(A1:A3)")).toBe("#NUM!");
    expect(n("=DEVSQ(A1:A4)", { A1: 1, A2: 3, A3: "x", A4: true })).toBe(2);
  });

  it("the means skip text and blanks in a range", () => {
    const values = { A1: 4, A2: "x", A3: 9, A4: "" };
    expect(n("=GEOMEAN(A1:A4)", values)).toBeCloseTo(6, 10);
    expect(n("=HARMEAN(A1:A4)", values)).toBeCloseTo(2 / (1 / 4 + 1 / 9), 10);
  });
});

describe("regression on paired data", () => {
  // Microsoft's sample for SLOPE, RSQ and STEYX.
  const y = "{2,3,9,1,8,7,5}";
  const x = "{6,5,11,7,5,4,4}";

  it("SLOPE follows the Microsoft example", () => {
    expect(n(`=SLOPE(${y},${x})`)).toBeCloseTo(0.305556, 6);
  });

  it("INTERCEPT follows the Microsoft example", () => {
    expect(n("=INTERCEPT({2,3,9,1,8},{6,5,11,7,5})")).toBeCloseTo(0.0483871, 7);
    expect(n(`=INTERCEPT(${y},${x})`)).toBeCloseTo(3.166667, 6);
  });

  it("RSQ follows the Microsoft example", () => {
    expect(n(`=RSQ(${y},${x})`)).toBeCloseTo(0.05795, 5);
  });

  it("STEYX follows the Microsoft example", () => {
    expect(n(`=STEYX(${y},${x})`)).toBeCloseTo(3.305719, 6);
  });

  it("FORECAST and FORECAST.LINEAR follow the Microsoft example", () => {
    const known = "{6,7,9,15,21},{20,28,31,38,40}";
    expect(n(`=FORECAST(30,${known})`)).toBeCloseTo(10.607253, 6);
    expect(n(`=FORECAST.LINEAR(30,${known})`)).toBeCloseTo(10.607253, 6);
  });

  it("PEARSON and CORREL agree (Microsoft examples)", () => {
    expect(n("=PEARSON({9,7,5,3,1},{10,6,1,5,3})")).toBeCloseTo(0.699379, 6);
    expect(n("=CORREL({3,2,4,5,6},{9,7,12,15,17})")).toBeCloseTo(0.997054, 6);
  });

  it("COVARIANCE.P and COVARIANCE.S follow the Microsoft example", () => {
    expect(n("=COVARIANCE.P({3,2,4,5,6},{9,7,12,15,17})")).toBeCloseTo(5.2, 10);
    expect(n("=COVARIANCE.S({3,2,4,5,6},{9,7,12,15,17})")).toBeCloseTo(6.5, 10);
  });

  it("are #N/A when the two arrays have a different number of points", () => {
    for (const name of ["SLOPE", "INTERCEPT", "RSQ", "STEYX", "CORREL", "PEARSON", "COVARIANCE.P", "COVARIANCE.S"]) {
      expect(code(`=${name}({1,2,3},{1,2})`), name).toBe("#N/A");
    }
    expect(code("=FORECAST(1,{1,2,3},{1,2})")).toBe("#N/A");
  });

  it("skip a pair when either side is text, a boolean or blank", () => {
    const values = { A1: 1, A2: 2, A3: "x", A4: 4, A5: 5, B1: 2, B2: 4, B3: 6, B4: "", B5: 10 };
    // Pairs 3 and 4 drop out, leaving (1,2), (2,4), (5,10): a perfect line y = 2x.
    expect(n("=SLOPE(B1:B5,A1:A5)", values)).toBeCloseTo(2, 10);
    expect(n("=INTERCEPT(B1:B5,A1:A5)", values)).toBeCloseTo(0, 10);
    expect(n("=RSQ(B1:B5,A1:A5)", values)).toBeCloseTo(1, 10);
  });

  it("are #DIV/0! when x does not vary or there are too few points", () => {
    expect(code("=SLOPE({1,2,3},{4,4,4})")).toBe("#DIV/0!");
    expect(code("=INTERCEPT({1,2,3},{4,4,4})")).toBe("#DIV/0!");
    expect(code("=RSQ({1,2,3},{4,4,4})")).toBe("#DIV/0!");
    expect(code("=SLOPE({1},{2})")).toBe("#DIV/0!");
    expect(code("=STEYX({1,2},{3,4})")).toBe("#DIV/0!");
    expect(code("=FORECAST(5,{1,2,3},{4,4,4})")).toBe("#DIV/0!");
    expect(code("=CORREL(A1:A3,B1:B3)")).toBe("#DIV/0!");
  });

  it("STEYX is exactly 0 for a perfect fit", () => {
    expect(n("=STEYX({2,4,6,8},{1,2,3,4})")).toBe(0);
  });

  it("FORECAST needs a numeric x", () => {
    expect(code('=FORECAST("x",{1,2,3},{1,2,3})')).toBe("#VALUE!");
    expect(code("=FORECAST(#N/A,{1,2,3},{1,2,3})")).toBe("#N/A");
  });
});

describe("TREND", () => {
  // Twelve monthly sales figures for months 1-12, extended two months ahead.
  const sales = [133890, 135000, 135790, 137300, 138130, 139100, 139900, 141120, 141890, 143230, 144000, 145290];
  const months: Record<string, Scalar> = { D1: 13, D2: 14 };
  sales.forEach((value, index) => {
    months[`A${index + 1}`] = index + 1;
    months[`B${index + 1}`] = value;
  });

  it("extends a trend to new x values, in a column (Microsoft sample data)", () => {
    // Least-squares line through the twelve points, evaluated at 13 and 14.
    const result = rounded(matrix("=TREND(B1:B12,A1:A12,D1:D2)", months), 1);
    expect(result).toEqual([[146171.5], [147189.7]]);
  });

  it("returns the fitted values when new_x is omitted", () => {
    expect(matrix("=TREND({2;4;6;8})")).toEqual([[2], [4], [6], [8]]);
    expect(rounded(matrix("=TREND({3;5;4},{1;2;3})"), 6)).toEqual([[3.5], [4], [4.5]]);
  });

  it("uses 1, 2, 3... as x when known_x is omitted", () => {
    expect(rounded(matrix("=TREND({2;4;6},,{4;5})"), 9)).toEqual([[8], [10]]);
  });

  it("keeps the orientation of the data: a row of y gives a row", () => {
    expect(rounded(matrix("=TREND({2,4,6},{1,2,3},{4,5})"), 9)).toEqual([[8, 10]]);
  });

  it("fits several x columns", () => {
    // y = 1 + 2*x1 + 3*x2 exactly, so the forecast at (6,7) is 1 + 12 + 21.
    const values: Record<string, Scalar> = { E1: 6, F1: 7 };
    [
      [2, 1],
      [1, 2],
      [3, 4],
      [4, 3],
      [6, 5],
    ].forEach(([x1, x2], index) => {
      values[`A${index + 1}`] = 1 + 2 * x1 + 3 * x2;
      values[`B${index + 1}`] = x1;
      values[`C${index + 1}`] = x2;
    });
    expect(rounded(matrix("=TREND(A1:A5,B1:C5,E1:F1)", values), 6)).toEqual([[34]]);
  });

  it("fits through the origin when const is FALSE", () => {
    // Slope = sum(xy) / sum(x^2) = 23 / 14; at x = 4 that is 92 / 14.
    expect(rounded(matrix("=TREND({2;3;5},{1;2;3},{4},FALSE)"), 9)).toEqual([[Number((92 / 14).toFixed(9))]]);
    expect(rounded(matrix("=TREND({2;3;5},{1;2;3},{4},TRUE)"), 6)).not.toEqual(
      rounded(matrix("=TREND({2;3;5},{1;2;3},{4},FALSE)"), 6),
    );
  });

  it("is #REF! when the shapes disagree and #VALUE! for text", () => {
    expect(code("=TREND({1;2;3},{1;2})")).toBe("#REF!");
    expect(code('=TREND({1;2;"x"},{1;2;3})')).toBe("#VALUE!");
    expect(code('=TREND({1;2;3},{1;2;3},{"x"})')).toBe("#VALUE!");
  });

  it("drops an x that never varies, as LINEST does, and falls back on the mean", () => {
    expect(rounded(matrix("=TREND({1;2;3},{5;5;5},{6})"), 9)).toEqual([[2]]);
  });
});

describe("GROWTH", () => {
  // Microsoft's sample: units sold in months 11-16 follow an exponential curve.
  const units = [33100, 47300, 69000, 102000, 150000, 220000];
  const data: Record<string, Scalar> = { D1: 17, D2: 18 };
  units.forEach((value, index) => {
    data[`A${index + 1}`] = 11 + index;
    data[`B${index + 1}`] = value;
  });

  it("returns the fitted exponential at the known x values (Microsoft example)", () => {
    expect(rounded(matrix("=GROWTH(B1:B6,A1:A6)", data), 0)).toEqual([
      [32618],
      [47729],
      [69841],
      [102197],
      [149542],
      [218822],
    ]);
  });

  it("extends the curve to new x values (Microsoft example)", () => {
    expect(rounded(matrix("=GROWTH(B1:B6,A1:A6,D1:D2)", data), 0)).toEqual([[320197], [468536]]);
  });

  it("recovers an exact exponential", () => {
    // y = 3 * 2^x
    expect(rounded(matrix("=GROWTH({6;12;24;48},{1;2;3;4},{5})"), 9)).toEqual([[96]]);
    expect(rounded(matrix("=GROWTH({6,12,24,48},{1,2,3,4},{5,6})"), 9)).toEqual([[96, 192]]);
  });

  it("is #NUM! for a non-positive y", () => {
    expect(code("=GROWTH({1;0;4},{1;2;3})")).toBe("#NUM!");
    expect(code("=GROWTH({1;-2;4},{1;2;3})")).toBe("#NUM!");
  });

  it("fits y = m^x when const is FALSE", () => {
    // y = 2^x passes through (0, 1), so the fit with b fixed at 1 is exact.
    expect(rounded(matrix("=GROWTH({2;4;8},{1;2;3},{4},FALSE)"), 9)).toEqual([[16]]);
  });
});

describe("PERCENTRANK", () => {
  // Microsoft's sample for the inclusive version.
  const inc = "{13,12,11,8,4,3,2,1,1,1}";

  it("PERCENTRANK.INC follows the Microsoft examples", () => {
    expect(n(`=PERCENTRANK.INC(${inc},2)`)).toBe(0.333);
    expect(n(`=PERCENTRANK.INC(${inc},4)`)).toBe(0.555);
    expect(n(`=PERCENTRANK.INC(${inc},8)`)).toBe(0.666);
    expect(n(`=PERCENTRANK.INC(${inc},5)`)).toBe(0.583);
    expect(n(`=PERCENTRANK(${inc},5)`)).toBe(0.583);
  });

  it("truncates to the requested number of digits instead of rounding", () => {
    expect(n(`=PERCENTRANK.INC(${inc},4,1)`)).toBe(0.5);
    expect(n(`=PERCENTRANK.INC(${inc},4,6)`)).toBe(0.555555);
    expect(n(`=PERCENTRANK.INC(${inc},8,2)`)).toBe(0.66);
  });

  it("PERCENTRANK.EXC never reaches 0 or 1", () => {
    expect(n("=PERCENTRANK.EXC({1,2,3},1)")).toBe(0.25);
    expect(n("=PERCENTRANK.EXC({1,2,3},3)")).toBe(0.75);
  });

  it("PERCENTRANK.INC is 0 at the minimum and 1 at the maximum", () => {
    expect(n(`=PERCENTRANK.INC(${inc},1)`)).toBe(0);
    expect(n(`=PERCENTRANK.INC(${inc},13)`)).toBe(1);
    expect(n("=PERCENTRANK.INC({5},5)")).toBe(1);
  });

  it("PERCENTRANK.EXC follows the Microsoft examples", () => {
    const exc = "{1,2,3,6,6,6,7,8,9}";
    expect(n(`=PERCENTRANK.EXC(${exc},7)`)).toBe(0.7);
    expect(n(`=PERCENTRANK.EXC(${exc},5.43)`)).toBe(0.381);
    expect(n(`=PERCENTRANK.EXC(${exc},5.43,1)`)).toBe(0.3);
  });

  it("is #N/A outside the data, #NUM! for a bad significance or no data", () => {
    expect(code(`=PERCENTRANK.INC(${inc},0)`)).toBe("#N/A");
    expect(code(`=PERCENTRANK.INC(${inc},14)`)).toBe("#N/A");
    expect(code("=PERCENTRANK.EXC({1,2,3},0)")).toBe("#N/A");
    expect(code(`=PERCENTRANK.INC(${inc},4,0)`)).toBe("#NUM!");
    expect(code("=PERCENTRANK.INC(A1:A3,4)")).toBe("#NUM!");
    expect(code('=PERCENTRANK.INC({1,2,3},"x")')).toBe("#VALUE!");
  });
});

describe("PERCENTILE.EXC and QUARTILE.EXC", () => {
  it("PERCENTILE.EXC follows the Microsoft example", () => {
    expect(n("=PERCENTILE.EXC({1,2,3,4},0.25)")).toBe(1.25);
  });

  it("QUARTILE.EXC follows the Microsoft example", () => {
    const data = "{6,7,15,36,39,40,41,42,43,47,49}";
    expect(n(`=QUARTILE.EXC(${data},1)`)).toBe(15);
    expect(n(`=QUARTILE.EXC(${data},2)`)).toBe(40);
    expect(n(`=QUARTILE.EXC(${data},3)`)).toBe(43);
  });

  it("need k inside (1/(n+1), n/(n+1)) and a quartile of 1-3", () => {
    expect(code("=PERCENTILE.EXC({1,2,3,4},0.1)")).toBe("#NUM!");
    expect(code("=PERCENTILE.EXC({1,2,3,4},0.9)")).toBe("#NUM!");
    expect(code("=QUARTILE.EXC({1,2,3,4},0)")).toBe("#NUM!");
    expect(code("=QUARTILE.EXC({1,2,3,4},4)")).toBe("#NUM!");
    expect(code("=PERCENTILE.EXC(A1:A3,0.5)")).toBe("#NUM!");
  });
});

describe("STANDARDIZE", () => {
  it("follows the Microsoft example", () => {
    expect(n("=STANDARDIZE(42,40,1.5)")).toBeCloseTo(1.333333, 6);
  });

  it("is #NUM! unless the standard deviation is positive", () => {
    expect(code("=STANDARDIZE(42,40,0)")).toBe("#NUM!");
    expect(code("=STANDARDIZE(42,40,-1)")).toBe("#NUM!");
  });
});

describe("registration", () => {
  it("lists every function with a signature for the picker", () => {
    const names = [
      "MODE",
      "MODE.SNGL",
      "MODE.MULT",
      "GEOMEAN",
      "HARMEAN",
      "AVEDEV",
      "DEVSQ",
      "SLOPE",
      "INTERCEPT",
      "RSQ",
      "STEYX",
      "FORECAST",
      "FORECAST.LINEAR",
      "PEARSON",
      "TREND",
      "GROWTH",
      "PERCENTRANK",
      "PERCENTRANK.INC",
      "PERCENTRANK.EXC",
      "PERCENTILE.EXC",
      "QUARTILE.EXC",
      "STANDARDIZE",
    ];
    for (const name of names) {
      const spec = lookupFunction(name);
      expect(spec, name).toBeDefined();
      expect(spec?.signature, name).toMatch(new RegExp(`^${name.replace(".", "\\.")}\\(`));
      expect(spec?.category, name).toBe("Statistics");
    }
  });
});
