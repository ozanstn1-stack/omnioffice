/**
 * Probability distributions: normal, binomial, Poisson, exponential and
 * Student's t.
 *
 * The examples are the ones Microsoft documents for each function. For the
 * accuracy checks the references are exact: Python's math.erfc/lgamma and
 * statistics.NormalDist for the normal functions, exact rational sums for the
 * binomial, 60-digit decimals for the Poisson and tails, and the closed forms
 * of the t distribution for 1-10 degrees of freedom.
 */
import { describe, expect, it } from "vitest";
import { evaluateFormula, isError, lookupFunction, type FormulaContext, type Scalar } from "./formula";
import {
  betaIncomplete,
  gammaIncomplete,
  logGamma,
  normalCdf,
  normalQuantile,
  studentCdf,
  studentUpperQuantile,
} from "./functions/special";

function context(): FormulaContext {
  return { getValue: () => "", sheetNames: ["Sheet1"], currentSheet: "Sheet1" };
}

function value(formula: string): Scalar {
  return evaluateFormula(formula, context());
}

function n(formula: string): number {
  const result = value(formula);
  if (isError(result)) throw new Error(`${formula} returned ${result.code}`);
  if (typeof result !== "number") throw new Error(`${formula} returned ${String(result)}`);
  return result;
}

function code(formula: string): string {
  const result = value(formula);
  return isError(result) ? result.code : "";
}

/** Relative distance from the expected value. */
function off(actual: number, expected: number): number {
  return expected === 0 ? Math.abs(actual) : Math.abs(actual / expected - 1);
}

describe("special functions", () => {
  it("the normal CDF is accurate to a few ulps in the centre and relatively in the tails", () => {
    const cases: Array<[number, number, number]> = [
      [0, 0.5, 1e-15],
      [1, 0.8413447460685429, 1e-15],
      [-1, 0.15865525393145705, 1e-15],
      [2, 0.9772498680518208, 1e-15],
      [-2, 0.0227501319481792, 1e-15],
      [-3, 0.0013498980316300957, 1e-15],
      [-4, 0.000031671241833119965, 1e-14],
      [-5, 2.866515718791939e-7, 1e-14],
      [-6, 9.86587645037698e-10, 1e-14],
      [-8, 6.220960574271784e-16, 1e-14],
      [-10, 7.619853024160525e-24, 1e-14],
      [-20, 2.7536241186062337e-89, 1e-14],
      [-30, 4.906713927148187e-198, 1e-14],
      [0.5, 0.6914624612740131, 1e-15],
      [-1.5, 0.06680720126885809, 1e-15],
      [1e-5, 0.500003989422804, 1e-15],
    ];
    for (const [z, expected, tolerance] of cases) {
      expect(off(normalCdf(z), expected), `Φ(${z})`).toBeLessThan(tolerance);
    }
    expect(normalCdf(-50)).toBe(0);
    expect(normalCdf(50)).toBe(1);
  });

  it("is symmetric: Φ(z) + Φ(-z) = 1", () => {
    for (const z of [0.1, 0.9, 1.7, 2.6, 3.9, 6]) expect(normalCdf(z) + normalCdf(-z)).toBeCloseTo(1, 15);
  });

  it("the normal quantile matches Wichura's AS241 to full precision", () => {
    const cases: Array<[number, number]> = [
      [0.975, 1.9599639845400536],
      [0.9, 1.2815515655446008],
      [0.25, -0.6744897501960817],
      [0.001, -3.090232306167813],
      [1e-5, -4.2648907939228256],
      [1e-10, -6.361340902404056],
      [1e-20, -9.262340089798405],
      [1e-100, -21.27345356096532],
      [1e-300, -37.0470962993612],
      [0.5, 0],
    ];
    for (const [p, expected] of cases) expect(off(normalQuantile(p), expected), `Φ⁻¹(${p})`).toBeLessThan(2e-16 * 4);
  });

  it("the quantile inverts the CDF", () => {
    for (const p of [1e-12, 1e-6, 0.01, 0.3, 0.5, 0.77, 0.999, 1 - 1e-9]) {
      expect(off(normalCdf(normalQuantile(p)), p)).toBeLessThan(1e-9 / (p < 0.5 ? 1 : 1e3));
    }
  });

  it("log-gamma matches the reference", () => {
    const cases: Array<[number, number]> = [
      [0.1, 2.2527126517342055],
      [0.5, 0.5723649429247004],
      [1.5, -0.12078223763524543],
      [5.5, 3.9578139676187165],
      [10, 12.801827480081467],
      [50, 144.5657439463449],
      [100.5, 361.4355404677776],
      [1000, 5905.220423209181],
    ];
    for (const [x, expected] of cases) expect(off(logGamma(x), expected), `lnΓ(${x})`).toBeLessThan(2e-14);
    expect(Math.abs(logGamma(1))).toBeLessThan(1e-14);
    expect(Math.abs(logGamma(2))).toBeLessThan(1e-14);
  });

  it("the incomplete beta function reproduces simple closed forms", () => {
    // Beta(2, 3) has CDF 6x² - 8x³ + 3x⁴; at 1/2 that is 11/16.
    expect(betaIncomplete(0.5, 2, 3)).toBeCloseTo(0.6875, 14);
    expect(betaIncomplete(0.3, 1, 1)).toBeCloseTo(0.3, 15);
    expect(betaIncomplete(0, 2, 3)).toBe(0);
    expect(betaIncomplete(1, 2, 3)).toBe(1);
    // I_x(a, b) = 1 - I_{1-x}(b, a)
    expect(betaIncomplete(0.2, 3, 7) + betaIncomplete(0.8, 7, 3)).toBeCloseTo(1, 14);
  });

  it("the incomplete gamma function reproduces simple closed forms", () => {
    // P(1, x) = 1 - e^-x; P(1/2, x) = erf(sqrt x).
    expect(gammaIncomplete(1, 2).lower).toBeCloseTo(0.8646647167633873, 14);
    expect(gammaIncomplete(0.5, 1).lower).toBeCloseTo(0.8427007929497149, 14);
    expect(gammaIncomplete(3, 50).upper).toBeLessThan(1e-17);
    const { lower, upper } = gammaIncomplete(7, 6);
    expect(lower + upper).toBeCloseTo(1, 15);
    expect(gammaIncomplete(2, 0)).toEqual({ lower: 0, upper: 1 });
  });

  it("Student's t matches the closed forms for 1-10 degrees of freedom", () => {
    const cases: Array<[number, number, number]> = [
      [2.0, 1, 0.8524163823495667],
      [1.5, 2, 0.8638034375544994],
      [2.5, 3, 0.9561466764959672],
      [-1.2, 4, 0.1481756966561767],
      [3.0, 5, 0.9849503760512688],
      [2.228, 10, 0.9749941140914444],
      [-0.7, 10, 0.24994378508644216],
      [1.7, 30, 0.9502610622057415],
      [1.959999998, 60, 0.9726775350120395],
      [60, 2, 0.9998611689547027],
      [60, 1, 0.9946953263673768],
      [8, 3, 0.9979617112061072],
    ];
    for (const [t, df, expected] of cases) expect(studentCdf(t, df), `t=${t} df=${df}`).toBeCloseTo(expected, 13);
  });

  it("the t quantile inverts the CDF, from the Cauchy case to nearly normal", () => {
    for (const df of [1, 2, 3, 5, 10, 30, 120]) {
      for (const tail of [1e-8, 1e-3, 0.025, 0.3, 0.49]) {
        const t = studentUpperQuantile(tail, df);
        expect(off(1 - studentCdf(t, df), tail), `df=${df} tail=${tail}`).toBeLessThan(tail < 1e-5 ? 1e-6 : 1e-9);
      }
    }
  });
});

describe("normal distribution", () => {
  it("NORM.DIST follows the Microsoft examples", () => {
    expect(n("=NORM.DIST(42,40,1.5,TRUE)")).toBeCloseTo(0.908789, 6);
    expect(n("=NORM.DIST(42,40,1.5,FALSE)")).toBeCloseTo(0.10934, 5);
    expect(n("=NORMDIST(42,40,1.5,TRUE)")).toBeCloseTo(0.908789, 6);
  });

  it("NORM.S.DIST follows the Microsoft examples", () => {
    expect(n("=NORM.S.DIST(1.333333,TRUE)")).toBeCloseTo(0.908789, 6);
    expect(n("=NORM.S.DIST(1.333333,FALSE)")).toBeCloseTo(0.16401, 5);
    expect(n("=NORMSDIST(1.333333)")).toBeCloseTo(0.908789, 6);
  });

  it("NORM.INV and NORM.S.INV follow the Microsoft examples", () => {
    expect(n("=NORM.INV(0.908789,40,1.5)")).toBeCloseTo(42.00000201, 6);
    expect(n("=NORMINV(0.908789,40,1.5)")).toBeCloseTo(42.00000201, 6);
    expect(n("=NORM.S.INV(0.908789)")).toBeCloseTo(1.333334673, 6);
    expect(n("=NORMSINV(0.908789)")).toBeCloseTo(1.333334673, 6);
  });

  it("round-trips through the inverse", () => {
    expect(n("=NORM.S.INV(NORM.S.DIST(-2.5,TRUE))")).toBeCloseTo(-2.5, 12);
    expect(n("=NORM.INV(NORM.DIST(7.25,5,2,TRUE),5,2)")).toBeCloseTo(7.25, 12);
  });

  it("reads the cumulative flag as TRUE/FALSE, text or a number", () => {
    expect(n('=NORM.S.DIST(0,"TRUE")')).toBe(0.5);
    expect(n("=NORM.S.DIST(0,1)")).toBe(0.5);
    expect(n("=NORM.S.DIST(0,0)")).toBeCloseTo(0.3989422804014327, 15);
    expect(code('=NORM.S.DIST(0,"maybe")')).toBe("#VALUE!");
  });

  it("is #NUM! outside the domain and passes other errors through", () => {
    expect(code("=NORM.DIST(1,0,0,TRUE)")).toBe("#NUM!");
    expect(code("=NORM.DIST(1,0,-1,TRUE)")).toBe("#NUM!");
    for (const p of ["0", "1", "-0.1", "1.5"]) {
      expect(code(`=NORM.S.INV(${p})`), p).toBe("#NUM!");
      expect(code(`=NORM.INV(${p},0,1)`), p).toBe("#NUM!");
    }
    expect(code("=NORM.INV(0.5,0,0)")).toBe("#NUM!");
    expect(code('=NORM.DIST("x",0,1,TRUE)')).toBe("#VALUE!");
    expect(code("=NORM.DIST(#N/A,0,1,TRUE)")).toBe("#N/A");
  });

  it("CONFIDENCE.NORM follows the Microsoft example", () => {
    expect(n("=CONFIDENCE.NORM(0.05,2.5,50)")).toBeCloseTo(0.692952, 6);
    expect(n("=CONFIDENCE(0.05,2.5,50)")).toBeCloseTo(0.692952, 6);
    expect(n("=CONFIDENCE.NORM(0.05,2.5,50.9)")).toBeCloseTo(0.692952, 6);
  });

  it("CONFIDENCE.NORM is #NUM! for alpha outside (0,1), a bad deviation or size", () => {
    expect(code("=CONFIDENCE.NORM(0,2.5,50)")).toBe("#NUM!");
    expect(code("=CONFIDENCE.NORM(1,2.5,50)")).toBe("#NUM!");
    expect(code("=CONFIDENCE.NORM(0.05,0,50)")).toBe("#NUM!");
    expect(code("=CONFIDENCE.NORM(0.05,2.5,0)")).toBe("#NUM!");
    expect(code("=CONFIDENCE.NORM(0.05,2.5,0.5)")).toBe("#NUM!");
  });

  it("PHI is the standard normal density and GAUSS the area from the mean", () => {
    expect(n("=PHI(0.75)")).toBeCloseTo(0.301137432, 8);
    expect(n("=GAUSS(2)")).toBeCloseTo(0.477249868, 8);
  });
});

describe("binomial distribution", () => {
  it("BINOM.DIST follows the Microsoft example", () => {
    expect(n("=BINOM.DIST(6,10,0.5,FALSE)")).toBeCloseTo(0.205078125, 12);
    expect(n("=BINOMDIST(6,10,0.5,FALSE)")).toBeCloseTo(0.205078125, 12);
  });

  it("BINOM.DIST is exact for small cumulative sums and accurate for large ones", () => {
    expect(n("=BINOM.DIST(3,10,0.2,TRUE)")).toBeCloseTo(0.8791261184, 12);
    expect(off(n("=BINOM.DIST(45,50,0.9,TRUE)"), 0.5688015931709384)).toBeLessThan(1e-12);
    expect(off(n("=BINOM.DIST(250,1000,0.3,TRUE)"), 0.00025980303652893067)).toBeLessThan(1e-12);
    expect(off(n("=BINOM.DIST(10,2000,0.01,TRUE)"), 0.010522510740822038)).toBeLessThan(1e-12);
    expect(off(n("=BINOM.DIST(300,1000,0.3,FALSE)"), 0.027521003821268385)).toBeLessThan(1e-12);
  });

  it("BINOM.DIST handles p of 0 and 1 and truncates the counts", () => {
    expect(n("=BINOM.DIST(0,5,0,FALSE)")).toBe(1);
    expect(n("=BINOM.DIST(1,5,0,FALSE)")).toBe(0);
    expect(n("=BINOM.DIST(5,5,1,FALSE)")).toBe(1);
    expect(n("=BINOM.DIST(3,5,1,TRUE)")).toBe(0);
    expect(n("=BINOM.DIST(5,5,0.3,TRUE)")).toBe(1);
    expect(n("=BINOM.DIST(6.9,10.9,0.5,FALSE)")).toBeCloseTo(0.205078125, 12);
  });

  it("BINOM.DIST is #NUM! for impossible counts or probabilities", () => {
    expect(code("=BINOM.DIST(11,10,0.5,FALSE)")).toBe("#NUM!");
    expect(code("=BINOM.DIST(-1,10,0.5,FALSE)")).toBe("#NUM!");
    expect(code("=BINOM.DIST(1,10,1.5,FALSE)")).toBe("#NUM!");
    expect(code("=BINOM.DIST(1,10,-0.1,FALSE)")).toBe("#NUM!");
  });

  it("BINOM.DIST.RANGE follows the Microsoft examples", () => {
    expect(n("=BINOM.DIST.RANGE(60,0.75,48)")).toBeCloseTo(0.083975, 6);
    expect(n("=BINOM.DIST.RANGE(60,0.75,45,50)")).toBeCloseTo(0.52363, 5);
    expect(n("=BINOM.DIST.RANGE(10,0.5,0,10)")).toBeCloseTo(1, 12);
  });

  it("BINOM.DIST.RANGE is #NUM! for an inverted or out-of-range interval", () => {
    expect(code("=BINOM.DIST.RANGE(60,0.75,50,45)")).toBe("#NUM!");
    expect(code("=BINOM.DIST.RANGE(60,0.75,61)")).toBe("#NUM!");
    expect(code("=BINOM.DIST.RANGE(60,0.75,5,61)")).toBe("#NUM!");
    expect(code("=BINOM.DIST.RANGE(60,1.5,5)")).toBe("#NUM!");
  });

  it("BINOM.INV follows the Microsoft example", () => {
    expect(n("=BINOM.INV(6,0.5,0.75)")).toBe(4);
    expect(n("=CRITBINOM(6,0.5,0.75)")).toBe(4);
  });

  it("BINOM.INV returns the smallest count that reaches alpha", () => {
    expect(n("=BINOM.INV(10,0.5,0)")).toBe(0);
    expect(n("=BINOM.INV(10,0.5,1)")).toBe(10);
    // P(X <= 3) = 0.8791..., P(X <= 2) = 0.6777...
    expect(n("=BINOM.INV(10,0.2,0.8)")).toBe(3);
    expect(n("=BINOM.INV(10,0.2,0.6)")).toBe(2);
    // The median of a fair coin flipped a million times.
    expect(n("=BINOM.INV(1000000,0.5,0.5)")).toBe(500000);
    // A billion trials still answers, to within a hundredth of a standard deviation.
    expect(Math.abs(n("=BINOM.INV(1000000000,0.5,0.5)") - 500000000)).toBeLessThan(200);
  });

  it("BINOM.INV is #NUM! outside its domain", () => {
    expect(code("=BINOM.INV(-1,0.5,0.5)")).toBe("#NUM!");
    expect(code("=BINOM.INV(10,1.5,0.5)")).toBe("#NUM!");
    expect(code("=BINOM.INV(10,0.5,1.5)")).toBe("#NUM!");
  });
});

describe("Poisson and exponential distributions", () => {
  it("POISSON.DIST follows the Microsoft examples", () => {
    expect(n("=POISSON.DIST(2,5,FALSE)")).toBeCloseTo(0.084224, 6);
    expect(n("=POISSON.DIST(2,5,TRUE)")).toBeCloseTo(0.124652, 6);
    expect(n("=POISSON(2,5,TRUE)")).toBeCloseTo(0.124652, 6);
  });

  it("POISSON.DIST is accurate for large means and tiny ones", () => {
    expect(off(n("=POISSON.DIST(80,100,FALSE)"), 0.00519785412598018)).toBeLessThan(1e-12);
    expect(off(n("=POISSON.DIST(80,100,TRUE)"), 0.02264917664225561)).toBeLessThan(1e-12);
    expect(off(n("=POISSON.DIST(120,100,TRUE)"), 0.9773306709216473)).toBeLessThan(1e-12);
    expect(off(n("=POISSON.DIST(0,0.5,TRUE)"), 0.6065306597126334)).toBeLessThan(1e-13);
    expect(off(n("=POISSON.DIST(3,0.001,TRUE)"), 0.9999999999999584)).toBeLessThan(1e-14);
  });

  it("POISSON.DIST handles a mean of 0 and truncates x", () => {
    expect(n("=POISSON.DIST(0,0,FALSE)")).toBe(1);
    expect(n("=POISSON.DIST(3,0,FALSE)")).toBe(0);
    expect(n("=POISSON.DIST(3,0,TRUE)")).toBe(1);
    expect(n("=POISSON.DIST(2.9,5,FALSE)")).toBeCloseTo(0.084224, 6);
  });

  it("POISSON.DIST is #NUM! for a negative x or mean", () => {
    expect(code("=POISSON.DIST(-1,5,TRUE)")).toBe("#NUM!");
    expect(code("=POISSON.DIST(1,-5,TRUE)")).toBe("#NUM!");
  });

  it("EXPON.DIST follows the Microsoft examples", () => {
    expect(n("=EXPON.DIST(0.2,10,TRUE)")).toBeCloseTo(0.864665, 6);
    expect(n("=EXPON.DIST(0.2,10,FALSE)")).toBeCloseTo(1.353353, 6);
    expect(n("=EXPONDIST(0.2,10,TRUE)")).toBeCloseTo(0.864665, 6);
  });

  it("EXPON.DIST is #NUM! for a negative x or a non-positive rate", () => {
    expect(code("=EXPON.DIST(-1,10,TRUE)")).toBe("#NUM!");
    expect(code("=EXPON.DIST(1,0,TRUE)")).toBe("#NUM!");
    expect(n("=EXPON.DIST(0,2,TRUE)")).toBe(0);
    expect(n("=EXPON.DIST(0,2,FALSE)")).toBe(2);
  });
});

describe("Student's t distribution", () => {
  it("T.DIST follows the Microsoft examples", () => {
    expect(n("=T.DIST(60,1,TRUE)")).toBeCloseTo(0.99469533, 8);
    expect(n("=T.DIST(8,3,FALSE)")).toBeCloseTo(0.00073691, 8);
  });

  it("T.DIST.2T and T.DIST.RT follow the Microsoft examples", () => {
    expect(n("=T.DIST.2T(1.959999998,60)")).toBeCloseTo(0.054645, 6);
    expect(n("=T.DIST.RT(1.959999998,60)")).toBeCloseTo(0.027322, 6);
    expect(n("=TDIST(1.959999998,60,2)")).toBeCloseTo(0.054645, 6);
    expect(n("=TDIST(1.959999998,60,1)")).toBeCloseTo(0.027322, 6);
  });

  it("T.DIST.RT accepts a negative x and keeps the small tail small", () => {
    expect(n("=T.DIST.RT(-1.959999998,60)")).toBeCloseTo(1 - 0.027322, 6);
    expect(n("=T.DIST.RT(0,5)")).toBe(0.5);
    expect(n("=T.DIST.RT(40,3)")).toBeLessThan(1e-4);
  });

  it("T.INV and T.INV.2T follow the Microsoft examples", () => {
    expect(n("=T.INV(0.75,2)")).toBeCloseTo(0.8164966, 7);
    expect(n("=T.INV.2T(0.546449,60)")).toBeCloseTo(0.606533, 6);
    expect(n("=TINV(0.546449,60)")).toBeCloseTo(0.606533, 6);
    expect(n("=TINV(0.054645,60)")).toBeCloseTo(1.96, 5);
  });

  it("T.INV is antisymmetric and 0 at one half", () => {
    expect(n("=T.INV(0.5,7)")).toBe(0);
    expect(n("=T.INV(0.1,7)")).toBeCloseTo(-n("=T.INV(0.9,7)"), 12);
    expect(n("=T.INV(0.025,1)")).toBeCloseTo(-12.7062047361747, 9);
    expect(n("=T.INV.2T(1,5)")).toBe(0);
  });

  it("T.INV inverts T.DIST", () => {
    expect(n("=T.INV(T.DIST(1.7,30,TRUE),30)")).toBeCloseTo(1.7, 10);
    expect(n("=T.INV(T.DIST(-4.2,3,TRUE),3)")).toBeCloseTo(-4.2, 9);
  });

  it("truncates the degrees of freedom", () => {
    expect(n("=T.DIST(2,5.9,TRUE)")).toBe(n("=T.DIST(2,5,TRUE)"));
  });

  it("is #NUM! outside the domain", () => {
    expect(code("=T.DIST(1,0,TRUE)")).toBe("#NUM!");
    expect(code("=T.DIST(1,0.5,TRUE)")).toBe("#NUM!");
    expect(code("=T.DIST.2T(-1,5)")).toBe("#NUM!");
    expect(code("=T.DIST.RT(1,0)")).toBe("#NUM!");
    expect(code("=TDIST(-1,5,2)")).toBe("#NUM!");
    expect(code("=TDIST(1,5,3)")).toBe("#NUM!");
    expect(code("=T.INV(0,5)")).toBe("#NUM!");
    expect(code("=T.INV(1,5)")).toBe("#NUM!");
    expect(code("=T.INV(0.5,0)")).toBe("#NUM!");
    expect(code("=T.INV.2T(0,5)")).toBe("#NUM!");
    expect(code("=T.INV.2T(1.5,5)")).toBe("#NUM!");
  });

  it("CONFIDENCE.T follows the Microsoft example", () => {
    expect(n("=CONFIDENCE.T(0.05,1,50)")).toBeCloseTo(0.284196855, 8);
  });

  it("CONFIDENCE.T is #DIV/0! for a sample of one and #NUM! for a bad alpha", () => {
    expect(code("=CONFIDENCE.T(0.05,1,1)")).toBe("#DIV/0!");
    expect(code("=CONFIDENCE.T(0,1,50)")).toBe("#NUM!");
    expect(code("=CONFIDENCE.T(0.05,0,50)")).toBe("#NUM!");
  });
});

describe("registration", () => {
  it("lists every function with a signature for the picker", () => {
    const names = [
      "NORM.S.DIST",
      "NORM.DIST",
      "NORM.INV",
      "NORM.S.INV",
      "NORMDIST",
      "NORMSDIST",
      "NORMINV",
      "NORMSINV",
      "CONFIDENCE.NORM",
      "CONFIDENCE",
      "CONFIDENCE.T",
      "PHI",
      "GAUSS",
      "BINOM.DIST",
      "BINOMDIST",
      "BINOM.DIST.RANGE",
      "BINOM.INV",
      "CRITBINOM",
      "POISSON.DIST",
      "POISSON",
      "EXPON.DIST",
      "EXPONDIST",
      "T.DIST",
      "T.DIST.2T",
      "T.DIST.RT",
      "T.INV",
      "T.INV.2T",
      "TDIST",
      "TINV",
    ];
    for (const name of names) {
      const spec = lookupFunction(name);
      expect(spec, name).toBeDefined();
      expect(spec?.signature, name).toMatch(new RegExp(`^${name.replaceAll(".", "\\.")}\\(`));
      expect(spec?.category, name).toBe("Statistics");
    }
  });
});
