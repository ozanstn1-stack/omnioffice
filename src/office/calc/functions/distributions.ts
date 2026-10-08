/**
 * Probability distributions: normal, binomial, Poisson, exponential and
 * Student's t, with the older names Excel keeps for compatibility (NORMDIST,
 * BINOMDIST, TDIST, ...). The numerics live in `special.ts`.
 *
 * Arguments are read the way Excel reads them: numbers are coerced from text
 * and booleans, a count (trials, degrees of freedom) is truncated, and an
 * argument outside its domain is `#NUM!`.
 */
import { registerFunction } from "../registry";
import { ERR, FormulaError, isError, toBool, toNumber, type Scalar } from "../scalars";
import {
  betaIncomplete,
  gammaIncomplete,
  logGamma,
  normalCdf,
  normalPdf,
  normalQuantile,
  studentCdf,
  studentPdf,
  studentUpperQuantile,
  studentUpperTail,
} from "./special";

type Args = Scalar[][][];

/** The first `count` arguments as numbers, or the first conversion error. */
function numberArgs(args: Args, count: number): number[] | FormulaError {
  const out: number[] = [];
  for (let index = 0; index < count; index += 1) {
    const value = toNumber(args[index]?.[0]?.[0] ?? "");
    if (isError(value)) return value;
    out.push(value);
  }
  return out;
}

/** The `cumulative` flag: TRUE/FALSE, or any number (0 is FALSE). */
function flagArg(args: Args, index: number): boolean | FormulaError {
  return toBool(args[index]?.[0]?.[0] ?? "");
}

function define(
  name: string,
  signature: string,
  min: number,
  max: number,
  fn: (args: Args) => number | FormulaError,
): void {
  registerFunction(name, fn, min, max, false, { signature, category: "Statistics" });
}

/** Registers `fn` under the modern name and each legacy alias. */
function defineAll(
  names: string[],
  signature: (name: string) => string,
  min: number,
  max: number,
  fn: (args: Args) => number | FormulaError,
): void {
  for (const name of names) define(name, signature(name), min, max, fn);
}

/** A probability read as a result: anything that overflowed or underflowed to NaN is #NUM!. */
function finite(value: number): number | FormulaError {
  return Number.isFinite(value) ? value : ERR.num();
}

// ---------------------------------------------------------------------------
// Normal
// ---------------------------------------------------------------------------

define("NORM.S.DIST", "NORM.S.DIST(z, cumulative)", 2, 2, (args) => {
  const values = numberArgs(args, 1);
  if (isError(values)) return values;
  const cumulative = flagArg(args, 1);
  if (isError(cumulative)) return cumulative;
  return cumulative ? normalCdf(values[0]) : normalPdf(values[0]);
});
define("NORMSDIST", "NORMSDIST(z)", 1, 1, (args) => {
  const z = numberArgs(args, 1);
  return isError(z) ? z : normalCdf(z[0]);
});
define("PHI", "PHI(x)", 1, 1, (args) => {
  const x = numberArgs(args, 1);
  return isError(x) ? x : normalPdf(x[0]);
});
define("GAUSS", "GAUSS(z)", 1, 1, (args) => {
  const z = numberArgs(args, 1);
  return isError(z) ? z : normalCdf(z[0]) - 0.5;
});

defineAll(
  ["NORM.DIST", "NORMDIST"],
  (name) => `${name}(x, mean, standard_dev, cumulative)`,
  4,
  4,
  (args) => {
    const values = numberArgs(args, 3);
    if (isError(values)) return values;
    const cumulative = flagArg(args, 3);
    if (isError(cumulative)) return cumulative;
    const [x, mean, deviation] = values;
    if (deviation <= 0) return ERR.num();
    const z = (x - mean) / deviation;
    return cumulative ? normalCdf(z) : normalPdf(z) / deviation;
  },
);

/** p must leave room on both sides: 0 < p < 1. */
function openProbability(p: number): boolean {
  return p > 0 && p < 1;
}

defineAll(
  ["NORM.S.INV", "NORMSINV"],
  (name) => `${name}(probability)`,
  1,
  1,
  (args) => {
    const values = numberArgs(args, 1);
    if (isError(values)) return values;
    return openProbability(values[0]) ? normalQuantile(values[0]) : ERR.num();
  },
);
defineAll(
  ["NORM.INV", "NORMINV"],
  (name) => `${name}(probability, mean, standard_dev)`,
  3,
  3,
  (args) => {
    const values = numberArgs(args, 3);
    if (isError(values)) return values;
    const [p, mean, deviation] = values;
    if (!openProbability(p) || deviation <= 0) return ERR.num();
    return mean + deviation * normalQuantile(p);
  },
);

defineAll(
  ["CONFIDENCE.NORM", "CONFIDENCE"],
  (name) => `${name}(alpha, standard_dev, size)`,
  3,
  3,
  (args) => {
    const values = numberArgs(args, 3);
    if (isError(values)) return values;
    const [alpha, deviation, rawSize] = values;
    const size = Math.trunc(rawSize);
    if (!openProbability(alpha) || deviation <= 0 || size < 1) return ERR.num();
    // The upper alpha/2 point, taken from the lower tail to keep tiny alphas exact.
    return (-normalQuantile(alpha / 2) * deviation) / Math.sqrt(size);
  },
);

// ---------------------------------------------------------------------------
// Binomial
// ---------------------------------------------------------------------------

/** ln of the binomial coefficient C(n, k). */
function logChoose(n: number, k: number): number {
  return logGamma(n + 1) - logGamma(k + 1) - logGamma(n - k + 1);
}

/** P(X = k) for X ~ Binomial(n, p). */
function binomialPmf(k: number, n: number, p: number): number {
  if (p === 0) return k === 0 ? 1 : 0;
  if (p === 1) return k === n ? 1 : 0;
  return Math.exp(logChoose(n, k) + k * Math.log(p) + (n - k) * Math.log1p(-p));
}

/** P(X <= k) for X ~ Binomial(n, p), through the incomplete beta function. */
function binomialCdf(k: number, n: number, p: number): number {
  if (k < 0) return 0;
  if (k >= n) return 1;
  return betaIncomplete(1 - p, n - k, k + 1);
}

defineAll(
  ["BINOM.DIST", "BINOMDIST"],
  (name) => `${name}(number_s, trials, probability_s, cumulative)`,
  4,
  4,
  (args) => {
    const values = numberArgs(args, 3);
    if (isError(values)) return values;
    const cumulative = flagArg(args, 3);
    if (isError(cumulative)) return cumulative;
    const successes = Math.trunc(values[0]);
    const trials = Math.trunc(values[1]);
    const p = values[2];
    if (successes < 0 || successes > trials || p < 0 || p > 1) return ERR.num();
    return finite(cumulative ? binomialCdf(successes, trials, p) : binomialPmf(successes, trials, p));
  },
);

define("BINOM.DIST.RANGE", "BINOM.DIST.RANGE(trials, probability_s, number_s, [number_s2])", 3, 4, (args) => {
  const values = numberArgs(args, 3);
  if (isError(values)) return values;
  const trials = Math.trunc(values[0]);
  const p = values[1];
  const low = Math.trunc(values[2]);
  let high = low;
  if (args[3] !== undefined && args[3][0]?.[0] !== "") {
    const second = toNumber(args[3][0][0]);
    if (isError(second)) return second;
    high = Math.trunc(second);
  }
  if (trials < 0 || p < 0 || p > 1 || low < 0 || low > trials || high < low || high > trials) return ERR.num();
  // A short range adds its terms; a long one subtracts two cumulative sums.
  if (high - low <= 1000) {
    let total = 0;
    for (let k = low; k <= high; k += 1) total += binomialPmf(k, trials, p);
    return finite(Math.min(1, total));
  }
  return finite(binomialCdf(high, trials, p) - binomialCdf(low - 1, trials, p));
});

defineAll(
  ["BINOM.INV", "CRITBINOM"],
  (name) => `${name}(trials, probability_s, alpha)`,
  3,
  3,
  (args) => {
    const values = numberArgs(args, 3);
    if (isError(values)) return values;
    const trials = Math.trunc(values[0]);
    const [, p, alpha] = values;
    if (trials < 0 || p < 0 || p > 1 || alpha < 0 || alpha > 1) return ERR.num();
    // The smallest k whose cumulative probability reaches alpha.
    let low = 0;
    let high = trials;
    while (low < high) {
      const middle = Math.floor((low + high) / 2);
      if (binomialCdf(middle, trials, p) >= alpha) high = middle;
      else low = middle + 1;
    }
    return low;
  },
);

// ---------------------------------------------------------------------------
// Poisson and exponential
// ---------------------------------------------------------------------------

defineAll(
  ["POISSON.DIST", "POISSON"],
  (name) => `${name}(x, mean, cumulative)`,
  3,
  3,
  (args) => {
    const values = numberArgs(args, 2);
    if (isError(values)) return values;
    const cumulative = flagArg(args, 2);
    if (isError(cumulative)) return cumulative;
    const x = Math.trunc(values[0]);
    const mean = values[1];
    if (x < 0 || mean < 0) return ERR.num();
    if (cumulative) return mean === 0 ? 1 : gammaIncomplete(x + 1, mean).upper;
    if (mean === 0) return x === 0 ? 1 : 0;
    return finite(Math.exp(-mean + x * Math.log(mean) - logGamma(x + 1)));
  },
);

defineAll(
  ["EXPON.DIST", "EXPONDIST"],
  (name) => `${name}(x, lambda, cumulative)`,
  3,
  3,
  (args) => {
    const values = numberArgs(args, 2);
    if (isError(values)) return values;
    const cumulative = flagArg(args, 2);
    if (isError(cumulative)) return cumulative;
    const [x, lambda] = values;
    if (x < 0 || lambda <= 0) return ERR.num();
    return cumulative ? -Math.expm1(-lambda * x) : lambda * Math.exp(-lambda * x);
  },
);

// ---------------------------------------------------------------------------
// Student's t
// ---------------------------------------------------------------------------

/** Degrees of freedom: truncated, and at least 1. */
function freedom(value: number): number | null {
  const df = Math.trunc(value);
  return df >= 1 ? df : null;
}

define("T.DIST", "T.DIST(x, deg_freedom, cumulative)", 3, 3, (args) => {
  const values = numberArgs(args, 2);
  if (isError(values)) return values;
  const cumulative = flagArg(args, 2);
  if (isError(cumulative)) return cumulative;
  const df = freedom(values[1]);
  if (df === null) return ERR.num();
  return cumulative ? studentCdf(values[0], df) : studentPdf(values[0], df);
});
define("T.DIST.RT", "T.DIST.RT(x, deg_freedom)", 2, 2, (args) => {
  const values = numberArgs(args, 2);
  if (isError(values)) return values;
  const df = freedom(values[1]);
  if (df === null) return ERR.num();
  return studentRightTail(values[0], df);
});
define("T.DIST.2T", "T.DIST.2T(x, deg_freedom)", 2, 2, (args) => {
  const values = numberArgs(args, 2);
  if (isError(values)) return values;
  const df = freedom(values[1]);
  if (df === null || values[0] < 0) return ERR.num();
  return 2 * studentUpperTail(values[0], df);
});
define("TDIST", "TDIST(x, deg_freedom, tails)", 3, 3, (args) => {
  const values = numberArgs(args, 3);
  if (isError(values)) return values;
  const df = freedom(values[1]);
  const tails = Math.trunc(values[2]);
  if (df === null || values[0] < 0 || (tails !== 1 && tails !== 2)) return ERR.num();
  return tails * studentUpperTail(values[0], df);
});

/** P(T > x) for any x, keeping the precision of a tiny upper tail. */
function studentRightTail(x: number, df: number): number {
  return x >= 0 ? studentUpperTail(x, df) : 1 - studentUpperTail(-x, df);
}

define("T.INV", "T.INV(probability, deg_freedom)", 2, 2, (args) => {
  const values = numberArgs(args, 2);
  if (isError(values)) return values;
  const df = freedom(values[1]);
  const p = values[0];
  if (df === null || !openProbability(p)) return ERR.num();
  if (p === 0.5) return 0;
  return p < 0.5 ? -studentUpperQuantile(p, df) : studentUpperQuantile(1 - p, df);
});
defineAll(
  ["T.INV.2T", "TINV"],
  (name) => `${name}(probability, deg_freedom)`,
  2,
  2,
  (args) => {
    const values = numberArgs(args, 2);
    if (isError(values)) return values;
    const df = freedom(values[1]);
    const p = values[0];
    if (df === null || !(p > 0 && p <= 1)) return ERR.num();
    return p === 1 ? 0 : studentUpperQuantile(p / 2, df);
  },
);

define("CONFIDENCE.T", "CONFIDENCE.T(alpha, standard_dev, size)", 3, 3, (args) => {
  const values = numberArgs(args, 3);
  if (isError(values)) return values;
  const [alpha, deviation, rawSize] = values;
  const size = Math.trunc(rawSize);
  if (!openProbability(alpha) || deviation <= 0 || size < 1) return ERR.num();
  if (size === 1) return ERR.div();
  return (studentUpperQuantile(alpha / 2, size - 1) * deviation) / Math.sqrt(size);
});
