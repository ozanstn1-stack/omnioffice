/**
 * Math functions added for Excel parity: factorials and combinatorics,
 * rounding to a multiple, ATAN2, sums of squares and the *.MATH rounding family.
 *
 * Expected values are the examples Microsoft documents for each function; the
 * comment on a case names the documented example it comes from.
 */
import { describe, expect, it } from "vitest";
import { evaluateFormula, isError, lookupFunction, type FormulaContext, type Scalar } from "./formula";

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

describe("FACT and FACTDOUBLE", () => {
  it("FACT follows the Microsoft examples", () => {
    expect(n("=FACT(5)")).toBe(120);
    expect(n("=FACT(1.9)")).toBe(1);
    expect(n("=FACT(0)")).toBe(1);
    expect(n("=FACT(1)")).toBe(1);
    expect(code("=FACT(-1)")).toBe("#NUM!");
  });

  it("FACT stays exact where doubles can be and gives up where they cannot", () => {
    expect(n("=FACT(20)")).toBe(2.43290200817664e18);
    expect(n("=FACT(170)") / 7.257415615307999e306).toBeCloseTo(1, 12);
    expect(code("=FACT(171)")).toBe("#NUM!");
  });

  it("FACT coerces text and boolean and passes errors through", () => {
    expect(n('=FACT("4")')).toBe(24);
    expect(n("=FACT(TRUE)")).toBe(1);
    expect(code('=FACT("x")')).toBe("#VALUE!");
    expect(code("=FACT(#N/A)")).toBe("#N/A");
  });

  it("FACTDOUBLE follows the Microsoft examples", () => {
    expect(n("=FACTDOUBLE(6)")).toBe(48);
    expect(n("=FACTDOUBLE(7)")).toBe(105);
    expect(n("=FACTDOUBLE(0)")).toBe(1);
    expect(n("=FACTDOUBLE(6.9)")).toBe(48);
  });

  it("FACTDOUBLE(-1) is 1 like Excel; anything lower is #NUM!", () => {
    expect(n("=FACTDOUBLE(-1)")).toBe(1);
    expect(code("=FACTDOUBLE(-2)")).toBe("#NUM!");
  });
});

describe("COMBIN, COMBINA, PERMUT and PERMUTATIONA", () => {
  it("COMBIN counts combinations (Microsoft example: COMBIN(8,2) is 28)", () => {
    expect(n("=COMBIN(8,2)")).toBe(28);
    expect(n("=COMBIN(5,0)")).toBe(1);
    expect(n("=COMBIN(5,5)")).toBe(1);
    expect(n("=COMBIN(52,5)")).toBe(2598960);
    expect(n("=COMBIN(8.9,2.9)")).toBe(28);
  });

  it("COMBIN is #NUM! for negative arguments or k > n", () => {
    expect(code("=COMBIN(2,3)")).toBe("#NUM!");
    expect(code("=COMBIN(-1,0)")).toBe("#NUM!");
    expect(code("=COMBIN(3,-1)")).toBe("#NUM!");
  });

  it("COMBIN stays accurate for large inputs", () => {
    expect(n("=COMBIN(100,50)") / 1.008913445455642e29).toBeCloseTo(1, 12);
    expect(code("=COMBIN(2000,1000)")).toBe("#NUM!");
  });

  it("COMBINA allows repetition (Microsoft examples)", () => {
    expect(n("=COMBINA(4,3)")).toBe(20);
    expect(n("=COMBINA(10,3)")).toBe(220);
    expect(n("=COMBINA(5,0)")).toBe(1);
    expect(n("=COMBINA(0,0)")).toBe(1);
  });

  it("COMBINA is #NUM! for negative arguments or an empty set with a pick", () => {
    expect(code("=COMBINA(-1,2)")).toBe("#NUM!");
    expect(code("=COMBINA(2,-1)")).toBe("#NUM!");
    expect(code("=COMBINA(0,2)")).toBe("#NUM!");
  });

  it("PERMUT counts ordered picks (Microsoft examples)", () => {
    expect(n("=PERMUT(100,3)")).toBe(970200);
    expect(n("=PERMUT(3,2)")).toBe(6);
    expect(n("=PERMUT(5,0)")).toBe(1);
  });

  it("PERMUT is #NUM! for a non-positive set, a negative pick or k > n", () => {
    expect(code("=PERMUT(0,0)")).toBe("#NUM!");
    expect(code("=PERMUT(3,-1)")).toBe("#NUM!");
    expect(code("=PERMUT(2,3)")).toBe("#NUM!");
  });

  it("PERMUTATIONA counts ordered picks with repetition (Microsoft examples)", () => {
    expect(n("=PERMUTATIONA(3,2)")).toBe(9);
    expect(n("=PERMUTATIONA(2,2)")).toBe(4);
    expect(n("=PERMUTATIONA(0,0)")).toBe(1);
    expect(code("=PERMUTATIONA(-1,2)")).toBe("#NUM!");
    expect(code("=PERMUTATIONA(2,-1)")).toBe("#NUM!");
  });

  it("MULTINOMIAL divides the factorial of the sum by the product of the factorials", () => {
    // Microsoft: MULTINOMIAL(2,3,4) is 1260.
    expect(n("=MULTINOMIAL(2,3,4)")).toBe(1260);
    expect(n("=MULTINOMIAL(1)")).toBe(1);
    expect(n("=MULTINOMIAL(A1:A3)", { A1: 2, A2: 3, A3: 4 })).toBe(1260);
    expect(n("=MULTINOMIAL(2.9,3,4)")).toBe(1260);
    expect(code("=MULTINOMIAL(2,-3)")).toBe("#NUM!");
  });
});

describe("MROUND", () => {
  it("rounds to the nearest multiple (Microsoft examples)", () => {
    expect(n("=MROUND(10,3)")).toBe(9);
    expect(n("=MROUND(-10,-3)")).toBe(-9);
    expect(n("=MROUND(1.3,0.2)")).toBe(1.4);
  });

  it("rounds halves away from zero", () => {
    expect(n("=MROUND(7.5,5)")).toBe(10);
    expect(n("=MROUND(-7.5,-5)")).toBe(-10);
    expect(n("=MROUND(2.5,1)")).toBe(3);
  });

  it("is #NUM! when the signs differ and 0 for a zero multiple", () => {
    // Microsoft: MROUND(5,-2) returns #NUM!.
    expect(code("=MROUND(5,-2)")).toBe("#NUM!");
    expect(code("=MROUND(-5,2)")).toBe("#NUM!");
    expect(n("=MROUND(5,0)")).toBe(0);
    expect(n("=MROUND(0,-3)")).toBe(0);
  });
});

describe("EVEN and ODD", () => {
  it("EVEN rounds away from zero to an even integer (Microsoft examples)", () => {
    expect(n("=EVEN(1.5)")).toBe(2);
    expect(n("=EVEN(3)")).toBe(4);
    expect(n("=EVEN(2)")).toBe(2);
    expect(n("=EVEN(-1)")).toBe(-2);
    expect(n("=EVEN(0)")).toBe(0);
  });

  it("ODD rounds away from zero to an odd integer (Microsoft examples)", () => {
    expect(n("=ODD(1.5)")).toBe(3);
    expect(n("=ODD(3)")).toBe(3);
    expect(n("=ODD(2)")).toBe(3);
    expect(n("=ODD(-1)")).toBe(-1);
    expect(n("=ODD(-2)")).toBe(-3);
    expect(n("=ODD(0)")).toBe(1);
  });

  it("report text that is not a number as #VALUE!", () => {
    expect(code('=EVEN("x")')).toBe("#VALUE!");
    expect(code('=ODD("x")')).toBe("#VALUE!");
  });
});

describe("ATAN2", () => {
  it("takes x first, then y (Microsoft examples)", () => {
    expect(n("=ATAN2(1,1)")).toBeCloseTo(0.785398163, 9);
    expect(n("=ATAN2(-1,-1)")).toBeCloseTo(-2.35619449, 8);
    expect(n("=ATAN2(-1,-1)*180/PI()")).toBeCloseTo(-135, 10);
    expect(n("=ATAN2(1,0)")).toBe(0);
    expect(n("=ATAN2(0,1)")).toBeCloseTo(Math.PI / 2, 12);
  });

  it("is #DIV/0! when both are zero", () => {
    expect(code("=ATAN2(0,0)")).toBe("#DIV/0!");
  });
});

describe("SUMSQ and the SUMX* family", () => {
  it("SUMSQ squares and adds (Microsoft example: SUMSQ(3,4) is 25)", () => {
    expect(n("=SUMSQ(3,4)")).toBe(25);
    expect(n("=SUMSQ(A1:A4)", { A1: 1, A2: 2, A3: "x", A4: true })).toBe(5);
    expect(n("=SUMSQ({1,2;3,4},5)")).toBe(1 + 4 + 9 + 16 + 25);
    expect(code("=SUMSQ(1,#N/A)")).toBe("#N/A");
  });

  // Microsoft's sample arrays for SUMX2MY2, SUMX2PY2 and SUMXMY2.
  const x = "{2,3,9,1,8,7,5}";
  const y = "{6,5,11,7,5,4,4}";

  it("SUMX2MY2, SUMX2PY2 and SUMXMY2 follow the Microsoft examples", () => {
    expect(n(`=SUMX2MY2(${x},${y})`)).toBe(-55);
    expect(n(`=SUMX2PY2(${x},${y})`)).toBe(521);
    expect(n(`=SUMXMY2(${x},${y})`)).toBe(79);
  });

  it("they are #N/A when the arrays differ in size", () => {
    expect(code("=SUMX2MY2({1,2,3},{1,2})")).toBe("#N/A");
    expect(code("=SUMX2PY2({1,2,3},{1,2})")).toBe("#N/A");
    expect(code("=SUMXMY2({1,2,3},{1,2})")).toBe("#N/A");
  });

  it("they skip pairs where either side is not a number", () => {
    const values = { A1: 3, A2: "x", A3: 5, B1: 1, B2: 2, B3: "" };
    expect(n("=SUMX2MY2(A1:A3,B1:B3)", values)).toBe(9 - 1);
    expect(n("=SUMXMY2(A1:A3,B1:B3)", values)).toBe(4);
    expect(n("=SUMX2PY2(A1:A3,B1:B3)", values)).toBe(10);
  });
});

describe("GCD, LCM and QUOTIENT", () => {
  it("GCD follows the Microsoft examples", () => {
    expect(n("=GCD(5,2)")).toBe(1);
    expect(n("=GCD(24,36)")).toBe(12);
    expect(n("=GCD(7,1)")).toBe(1);
    expect(n("=GCD(5,0)")).toBe(5);
    expect(n("=GCD(12,18,30)")).toBe(6);
    expect(n("=GCD(A1:A2)", { A1: 12, A2: 18 })).toBe(6);
  });

  it("LCM follows the Microsoft examples", () => {
    expect(n("=LCM(5,2)")).toBe(10);
    expect(n("=LCM(24,36)")).toBe(72);
    expect(n("=LCM(4,6,10)")).toBe(60);
    expect(n("=LCM(5,0)")).toBe(0);
  });

  it("truncate their arguments and reject negatives", () => {
    expect(n("=GCD(24.9,36.1)")).toBe(12);
    expect(n("=LCM(4.9,6.1)")).toBe(12);
    expect(code("=GCD(-4,6)")).toBe("#NUM!");
    expect(code("=LCM(4,-6)")).toBe("#NUM!");
    expect(code('=GCD("x",6)')).toBe("#VALUE!");
  });

  it("QUOTIENT follows the Microsoft examples", () => {
    expect(n("=QUOTIENT(5,2)")).toBe(2);
    expect(n("=QUOTIENT(4.5,3.1)")).toBe(1);
    expect(n("=QUOTIENT(-10,3)")).toBe(-3);
    expect(code("=QUOTIENT(5,0)")).toBe("#DIV/0!");
  });
});

describe("CEILING.MATH, FLOOR.MATH and the precise variants", () => {
  it("CEILING.MATH follows the Microsoft examples", () => {
    expect(n("=CEILING.MATH(24.3,5)")).toBe(25);
    expect(n("=CEILING.MATH(6.7)")).toBe(7);
    expect(n("=CEILING.MATH(-8.1,2)")).toBe(-8);
    expect(n("=CEILING.MATH(-5.5,2,-1)")).toBe(-6);
  });

  it("FLOOR.MATH follows the Microsoft examples", () => {
    expect(n("=FLOOR.MATH(24.3,5)")).toBe(20);
    expect(n("=FLOOR.MATH(6.7)")).toBe(6);
    expect(n("=FLOOR.MATH(-8.1,2)")).toBe(-10);
    expect(n("=FLOOR.MATH(-5.5,2,-1)")).toBe(-4);
  });

  it("ignore the sign of the significance and give 0 for a zero significance", () => {
    expect(n("=CEILING.MATH(24.3,-5)")).toBe(25);
    expect(n("=FLOOR.MATH(24.3,-5)")).toBe(20);
    expect(n("=CEILING.MATH(5,0)")).toBe(0);
    expect(n("=FLOOR.MATH(5,0)")).toBe(0);
  });

  it("do not drift on binary fractions", () => {
    expect(n("=CEILING.MATH(0.6,0.2)")).toBe(0.6);
    expect(n("=FLOOR.MATH(0.6,0.2)")).toBe(0.6);
    expect(n("=CEILING.MATH(0.7,0.1)")).toBe(0.7);
    expect(n("=FLOOR.MATH(1.1,0.1)")).toBe(1.1);
  });

  it("ISO.CEILING and CEILING.PRECISE follow the Microsoft examples", () => {
    for (const name of ["ISO.CEILING", "CEILING.PRECISE"]) {
      expect(n(`=${name}(4.3)`), name).toBe(5);
      expect(n(`=${name}(-4.3)`), name).toBe(-4);
      expect(n(`=${name}(4.3,2)`), name).toBe(6);
      expect(n(`=${name}(4.3,-2)`), name).toBe(6);
      expect(n(`=${name}(-4.3,2)`), name).toBe(-4);
      expect(n(`=${name}(-4.3,-2)`), name).toBe(-4);
    }
  });

  it("FLOOR.PRECISE rounds toward negative infinity whatever the significance's sign", () => {
    // Microsoft: FLOOR.PRECISE(-3.2,-1) is -4, FLOOR.PRECISE(3.2,1) is 3,
    // FLOOR.PRECISE(-3.2,1) is -4 and FLOOR.PRECISE(3.2,-1) is 3.
    expect(n("=FLOOR.PRECISE(-3.2,-1)")).toBe(-4);
    expect(n("=FLOOR.PRECISE(3.2,1)")).toBe(3);
    expect(n("=FLOOR.PRECISE(-3.2,1)")).toBe(-4);
    expect(n("=FLOOR.PRECISE(3.2,-1)")).toBe(3);
    expect(n("=FLOOR.PRECISE(3.2)")).toBe(3);
  });

  it("propagate errors", () => {
    expect(code("=CEILING.MATH(#N/A,2)")).toBe("#N/A");
    expect(code('=FLOOR.MATH(5,"x")')).toBe("#VALUE!");
  });
});

describe("registration", () => {
  it("lists every function with a signature for the picker", () => {
    const names = [
      "FACT",
      "FACTDOUBLE",
      "COMBIN",
      "COMBINA",
      "PERMUT",
      "PERMUTATIONA",
      "MULTINOMIAL",
      "MROUND",
      "EVEN",
      "ODD",
      "ATAN2",
      "SUMSQ",
      "CEILING.MATH",
      "FLOOR.MATH",
      "ISO.CEILING",
      "CEILING.PRECISE",
      "FLOOR.PRECISE",
    ];
    for (const name of names) {
      const spec = lookupFunction(name);
      expect(spec, name).toBeDefined();
      expect(spec?.signature, name).toMatch(new RegExp(`^${name.replace(".", "\\.")}\\(`));
      expect(spec?.category, name).toBe("Math");
    }
  });
});
