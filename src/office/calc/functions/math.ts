/**
 * Arithmetic functions that complement the operator set (+ - * / ^).
 */
import { registerFunction } from "../registry";
import { formatNumber } from "../numberFormat";
import { ERR, FormulaError, flatten, isError, toNumber, toText, type Scalar } from "../scalars";
import { numericValues, sumOf, tidy } from "./stat-support";

function scalarOf(args: Scalar[][][], index: number): Scalar {
  return args[index]?.[0]?.[0] ?? 0;
}

function unary(name: string, fn: (value: number) => number, guard?: (value: number) => boolean) {
  registerFunction(
    name,
    (args) => {
      const value = toNumber(scalarOf(args, 0));
      if (isError(value)) return value;
      if (guard && !guard(value)) return ERR.num();
      return fn(value);
    },
    1,
    1,
    false,
    { signature: `${name}(number)`, category: "Math" },
  );
}

unary("ABS", Math.abs);
unary("SQRT", Math.sqrt, (value) => value >= 0);
unary("INT", Math.floor);
unary("SIGN", Math.sign);
unary("LN", Math.log, (value) => value > 0);
unary("LOG10", Math.log10, (value) => value > 0);
unary("EXP", Math.exp);
unary("SIN", Math.sin);
unary("COS", Math.cos);
unary("TAN", Math.tan);
unary("ASIN", Math.asin, (value) => value >= -1 && value <= 1);
unary("ACOS", Math.acos, (value) => value >= -1 && value <= 1);
unary("ATAN", Math.atan);
unary("DEGREES", (value) => (value * 180) / Math.PI);
unary("RADIANS", (value) => (value * Math.PI) / 180);

registerFunction(
  "POWER",
  (args) => {
    const base = toNumber(scalarOf(args, 0));
    if (isError(base)) return base;
    const exponent = toNumber(scalarOf(args, 1));
    if (isError(exponent)) return exponent;
    const result = base ** exponent;
    return Number.isFinite(result) ? result : ERR.num();
  },
  2,
  2,
  false,
  { signature: "POWER(number, power)", category: "Math" },
);
registerFunction(
  "MOD",
  (args) => {
    const value = toNumber(scalarOf(args, 0));
    if (isError(value)) return value;
    const divisor = toNumber(scalarOf(args, 1));
    if (isError(divisor)) return divisor;
    if (divisor === 0) return ERR.div();
    // Excel's MOD keeps the sign of the divisor, unlike JavaScript's `%`.
    return value - divisor * Math.floor(value / divisor);
  },
  2,
  2,
  false,
  { signature: "MOD(number, divisor)", category: "Math" },
);
registerFunction(
  "QUOTIENT",
  (args) => {
    const value = toNumber(scalarOf(args, 0));
    if (isError(value)) return value;
    const divisor = toNumber(scalarOf(args, 1));
    if (isError(divisor)) return divisor;
    if (divisor === 0) return ERR.div();
    return Math.trunc(value / divisor);
  },
  2,
  2,
  false,
  { signature: "QUOTIENT(number, divisor)", category: "Math" },
);
registerFunction(
  "CEILING",
  (args) => {
    const value = toNumber(scalarOf(args, 0));
    if (isError(value)) return value;
    const significance = args[1] === undefined ? 1 : toNumber(scalarOf(args, 1));
    if (isError(significance)) return significance;
    if (significance === 0) return 0;
    return Math.ceil(value / significance) * significance;
  },
  1,
  2,
  false,
  { signature: "CEILING(number, [significance])", category: "Math" },
);
registerFunction(
  "FLOOR",
  (args) => {
    const value = toNumber(scalarOf(args, 0));
    if (isError(value)) return value;
    const significance = args[1] === undefined ? 1 : toNumber(scalarOf(args, 1));
    if (isError(significance)) return significance;
    if (significance === 0) return ERR.div();
    return Math.floor(value / significance) * significance;
  },
  1,
  2,
  false,
  { signature: "FLOOR(number, [significance])", category: "Math" },
);
registerFunction(
  "ROUND",
  (args) => {
    const value = toNumber(scalarOf(args, 0));
    if (isError(value)) return value;
    const digits = toNumber(scalarOf(args, 1));
    if (isError(digits)) return digits;
    const factor = 10 ** Math.trunc(digits);
    // Nudge by an epsilon first so 2.675 rounds to 2.68 rather than 2.67.
    return Math.round((value + Number.EPSILON * Math.sign(value || 1)) * factor) / factor;
  },
  1,
  2,
  false,
  { signature: "ROUND(number, digits)", category: "Math" },
);
registerFunction(
  "ROUNDUP",
  (args) => {
    const value = toNumber(scalarOf(args, 0));
    if (isError(value)) return value;
    const digits = toNumber(scalarOf(args, 1));
    if (isError(digits)) return digits;
    const factor = 10 ** Math.trunc(digits);
    return ((value < 0 ? -1 : 1) * Math.ceil(Math.abs(value) * factor)) / factor;
  },
  1,
  2,
  false,
  { signature: "ROUNDUP(number, digits)", category: "Math" },
);
registerFunction(
  "ROUNDDOWN",
  (args) => {
    const value = toNumber(scalarOf(args, 0));
    if (isError(value)) return value;
    const digits = toNumber(scalarOf(args, 1));
    if (isError(digits)) return digits;
    const factor = 10 ** Math.trunc(digits);
    return ((value < 0 ? -1 : 1) * Math.floor(Math.abs(value) * factor)) / factor;
  },
  1,
  2,
  false,
  { signature: "ROUNDDOWN(number, digits)", category: "Math" },
);
registerFunction(
  "TRUNC",
  (args) => {
    const value = toNumber(scalarOf(args, 0));
    if (isError(value)) return value;
    const digits = args[1] === undefined ? 0 : toNumber(scalarOf(args, 1));
    if (isError(digits)) return digits;
    const factor = 10 ** Math.trunc(digits);
    return Math.trunc(value * factor) / factor;
  },
  1,
  2,
  false,
  { signature: "TRUNC(number, [digits])", category: "Math" },
);
/** The whole numbers of a GCD/LCM argument list: empty cells are skipped, negatives are #NUM!. */
function wholeNumbers(args: Scalar[][][]): number[] | FormulaError {
  const out: number[] = [];
  for (const value of flatten(args)) {
    if (value === "") continue;
    const number = toNumber(value);
    if (isError(number)) return number;
    const whole = Math.trunc(number);
    if (whole < 0 || whole >= 2 ** 53) return ERR.num();
    out.push(whole);
  }
  return out;
}
const gcd = (a: number, b: number): number => (b === 0 ? a : gcd(b, a % b));
registerFunction(
  "GCD",
  (args) => {
    const values = wholeNumbers(args);
    return isError(values) ? values : values.reduce(gcd, 0);
  },
  1,
  64,
  false,
  { signature: "GCD(number1, ...)", category: "Math" },
);
registerFunction(
  "LCM",
  (args) => {
    const values = wholeNumbers(args);
    if (isError(values)) return values;
    if (values.length === 0) return 0;
    return values.reduce((a, b) => (a === 0 || b === 0 ? 0 : (a / gcd(a, b)) * b));
  },
  1,
  64,
  false,
  { signature: "LCM(number1, ...)", category: "Math" },
);
registerFunction(
  "LOG",
  (args) => {
    const value = toNumber(scalarOf(args, 0));
    if (isError(value)) return value;
    const base = args[1] === undefined ? 10 : toNumber(scalarOf(args, 1));
    if (isError(base)) return base;
    if (value <= 0 || base <= 0 || base === 1) return ERR.num();
    return Math.log(value) / Math.log(base);
  },
  1,
  2,
  false,
  { signature: "LOG(number, [base])", category: "Math" },
);
registerFunction("SUMX2MY2", (args) => pairAggregate(args, (a, b) => a * a - b * b), 2, 2, false, {
  signature: "SUMX2MY2(array_x, array_y)",
  category: "Math",
});
registerFunction("SUMXMY2", (args) => pairAggregate(args, (a, b) => (a - b) * (a - b)), 2, 2, false, {
  signature: "SUMXMY2(array_x, array_y)",
  category: "Math",
});
registerFunction("SUMX2PY2", (args) => pairAggregate(args, (a, b) => a * a + b * b), 2, 2, false, {
  signature: "SUMX2PY2(array_x, array_y)",
  category: "Math",
});

/**
 * Adds `fn(x, y)` over two same-shaped arrays. A pair is skipped when either
 * side is text, a boolean or empty; arrays of different shape are #N/A.
 */
function pairAggregate(args: Scalar[][][], fn: (a: number, b: number) => number): number | FormulaError {
  const x = args[0] ?? [];
  const y = args[1] ?? [];
  if (x.length !== y.length || (x[0]?.length ?? 0) !== (y[0]?.length ?? 0)) return ERR.na();
  let total = 0;
  x.forEach((line, row) =>
    line.forEach((left, col) => {
      const right = y[row]?.[col];
      if (typeof left === "number" && typeof right === "number") total += fn(left, right);
    }),
  );
  return total;
}

/** A number argument truncated toward zero, as the combinatorics functions read theirs. */
function wholeArg(args: Scalar[][][], index: number): number | FormulaError {
  const value = toNumber(scalarOf(args, index));
  return isError(value) ? value : Math.trunc(value);
}

/** A result that overflowed a double is #NUM!, like Excel's. */
function finiteOrNum(value: number): number | FormulaError {
  return Number.isFinite(value) ? value : ERR.num();
}

/** The product n * (n - step) * ... down to 1; `step` 1 is n!, 2 is n!!. */
function descendingProduct(n: number, step: number): number {
  let result = 1;
  for (let factor = n; factor > 1; factor -= step) result *= factor;
  return result;
}

/** Binomial coefficient for 0 <= k <= n, multiplied in an order that stays integral. */
function choose(n: number, k: number): number {
  const pick = Math.min(k, n - k);
  let result = 1;
  for (let step = 1; step <= pick; step += 1) result = (result * (n - pick + step)) / step;
  return result < 2 ** 53 ? Math.round(result) : result;
}

registerFunction(
  "FACT",
  (args) => {
    const n = wholeArg(args, 0);
    if (isError(n)) return n;
    // 171! no longer fits in a double.
    return n < 0 || n > 170 ? ERR.num() : descendingProduct(n, 1);
  },
  1,
  1,
  false,
  { signature: "FACT(number)", category: "Math" },
);
registerFunction(
  "FACTDOUBLE",
  (args) => {
    const n = wholeArg(args, 0);
    if (isError(n)) return n;
    // Excel defines (-1)!! as 1, like 0!!.
    if (n < -1) return ERR.num();
    return finiteOrNum(descendingProduct(n, 2));
  },
  1,
  1,
  false,
  { signature: "FACTDOUBLE(number)", category: "Math" },
);
registerFunction(
  "COMBIN",
  (args) => {
    const n = wholeArg(args, 0);
    if (isError(n)) return n;
    const k = wholeArg(args, 1);
    if (isError(k)) return k;
    return n < 0 || k < 0 || n < k ? ERR.num() : finiteOrNum(choose(n, k));
  },
  2,
  2,
  false,
  { signature: "COMBIN(number, number_chosen)", category: "Math" },
);
registerFunction(
  "COMBINA",
  (args) => {
    const n = wholeArg(args, 0);
    if (isError(n)) return n;
    const k = wholeArg(args, 1);
    if (isError(k)) return k;
    if (n < 0 || k < 0 || (n === 0 && k > 0)) return ERR.num();
    return k === 0 ? 1 : finiteOrNum(choose(n + k - 1, k));
  },
  2,
  2,
  false,
  { signature: "COMBINA(number, number_chosen)", category: "Math" },
);
registerFunction(
  "PERMUT",
  (args) => {
    const n = wholeArg(args, 0);
    if (isError(n)) return n;
    const k = wholeArg(args, 1);
    if (isError(k)) return k;
    if (n <= 0 || k < 0 || n < k) return ERR.num();
    let result = 1;
    for (let factor = n; factor > n - k; factor -= 1) result *= factor;
    return finiteOrNum(result);
  },
  2,
  2,
  false,
  { signature: "PERMUT(number, number_chosen)", category: "Math" },
);
registerFunction(
  "PERMUTATIONA",
  (args) => {
    const n = wholeArg(args, 0);
    if (isError(n)) return n;
    const k = wholeArg(args, 1);
    if (isError(k)) return k;
    return n < 0 || k < 0 ? ERR.num() : finiteOrNum(n ** k);
  },
  2,
  2,
  false,
  { signature: "PERMUTATIONA(number, number_chosen)", category: "Math" },
);
registerFunction(
  "MULTINOMIAL",
  (args) => {
    const values = args.flatMap(numericValues).map(Math.trunc);
    if (values.some((value) => value < 0)) return ERR.num();
    // (a+b+c)! / (a! b! c!) as a product of binomials, which never overflows early.
    let running = 0;
    let result = 1;
    for (const value of values) {
      running += value;
      result *= choose(running, value);
    }
    return finiteOrNum(result);
  },
  1,
  64,
  false,
  { signature: "MULTINOMIAL(number1, ...)", category: "Math" },
);

/** -0 would print as "-0" in a few places; a zero result is always plain 0. */
function plainZero(value: number): number {
  return value === 0 ? 0 : value;
}

registerFunction(
  "MROUND",
  (args) => {
    const value = toNumber(scalarOf(args, 0));
    if (isError(value)) return value;
    const multiple = toNumber(scalarOf(args, 1));
    if (isError(multiple)) return multiple;
    if (multiple === 0) return 0;
    if (value * multiple < 0) return ERR.num();
    const step = Math.abs(multiple);
    // Halves go away from zero; tidy() keeps 1.3 / 0.2 from landing on 6.499999...
    const units = Math.floor(tidy(Math.abs(value) / step) + 0.5);
    return plainZero(tidy(units * step) * (value < 0 ? -1 : 1));
  },
  2,
  2,
  false,
  { signature: "MROUND(number, multiple)", category: "Math" },
);
registerFunction(
  "EVEN",
  (args) => {
    const value = toNumber(scalarOf(args, 0));
    if (isError(value)) return value;
    const magnitude = Math.ceil(Math.abs(value));
    return plainZero((magnitude % 2 === 0 ? magnitude : magnitude + 1) * (value < 0 ? -1 : 1));
  },
  1,
  1,
  false,
  { signature: "EVEN(number)", category: "Math" },
);
registerFunction(
  "ODD",
  (args) => {
    const value = toNumber(scalarOf(args, 0));
    if (isError(value)) return value;
    const magnitude = Math.ceil(Math.abs(value));
    return (magnitude % 2 === 1 ? magnitude : magnitude + 1) * (value < 0 ? -1 : 1);
  },
  1,
  1,
  false,
  { signature: "ODD(number)", category: "Math" },
);
registerFunction(
  "ATAN2",
  (args) => {
    // Excel takes x before y, the reverse of Math.atan2.
    const x = toNumber(scalarOf(args, 0));
    if (isError(x)) return x;
    const y = toNumber(scalarOf(args, 1));
    if (isError(y)) return y;
    return x === 0 && y === 0 ? ERR.div() : Math.atan2(y, x);
  },
  2,
  2,
  false,
  { signature: "ATAN2(x_num, y_num)", category: "Math" },
);
registerFunction("SUMSQ", (args) => sumOf(args.flatMap(numericValues).map((value) => value * value)), 1, 64, false, {
  signature: "SUMSQ(number1, ...)",
  category: "Math",
});

/**
 * CEILING.MATH / FLOOR.MATH. The significance's sign is ignored. For a negative
 * number, `mode` 0 rounds CEILING.MATH toward zero and FLOOR.MATH away from it;
 * any other mode swaps the two.
 */
function mathRounding(args: Scalar[][][], ceiling: boolean): number | FormulaError {
  const value = toNumber(scalarOf(args, 0));
  if (isError(value)) return value;
  const skipped = (index: number) => args[index] === undefined || scalarOf(args, index) === "";
  const significance = skipped(1) ? 1 : toNumber(scalarOf(args, 1));
  if (isError(significance)) return significance;
  const mode = skipped(2) ? 0 : toNumber(scalarOf(args, 2));
  if (isError(mode)) return mode;
  const step = Math.abs(significance);
  if (step === 0) return 0;
  const quotient = tidy(value / step);
  const up = value < 0 ? ceiling !== (mode !== 0) : ceiling;
  return plainZero(tidy((up ? Math.ceil(quotient) : Math.floor(quotient)) * step));
}
registerFunction("CEILING.MATH", (args) => mathRounding(args, true), 1, 3, false, {
  signature: "CEILING.MATH(number, [significance], [mode])",
  category: "Math",
});
registerFunction("FLOOR.MATH", (args) => mathRounding(args, false), 1, 3, false, {
  signature: "FLOOR.MATH(number, [significance], [mode])",
  category: "Math",
});
// The ISO/PRECISE variants round up (down) whatever the signs involved: the
// default mode of the *.MATH functions with the significance's sign ignored.
registerFunction("ISO.CEILING", (args) => mathRounding(args, true), 1, 2, false, {
  signature: "ISO.CEILING(number, [significance])",
  category: "Math",
});
registerFunction("CEILING.PRECISE", (args) => mathRounding(args, true), 1, 2, false, {
  signature: "CEILING.PRECISE(number, [significance])",
  category: "Math",
});
registerFunction("FLOOR.PRECISE", (args) => mathRounding(args, false), 1, 2, false, {
  signature: "FLOOR.PRECISE(number, [significance])",
  category: "Math",
});

/** Used by the number-format picker to preview TEXT() output. */
export function previewFormat(value: Scalar, pattern: string): string {
  const number = toNumber(value);
  if (isError(number)) return toText(value);
  return formatNumber(number, pattern);
}
