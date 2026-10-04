/**
 * Arithmetic functions that complement the operator set (+ - * / ^).
 */
import { registerFunction } from "../registry";
import { formatNumber } from "../numberFormat";
import { ERR, FormulaError, isError, numbers, toNumber, toText, type Scalar } from "../scalars";

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
registerFunction(
  "GCD",
  (args) => {
    const values = numbers(args);
    if (values.length === 0) return 0;
    const gcd = (a: number, b: number): number => (b === 0 ? Math.abs(a) : gcd(b, a % b));
    return values.map((value) => Math.abs(Math.trunc(value))).reduce(gcd);
  },
  1,
  64,
  false,
  { signature: "GCD(number1, ...)", category: "Math" },
);
registerFunction(
  "LCM",
  (args) => {
    const values = numbers(args);
    if (values.length === 0) return 0;
    const gcd = (a: number, b: number): number => (b === 0 ? Math.abs(a) : gcd(b, a % b));
    return values
      .map((value) => Math.abs(Math.trunc(value)))
      .reduce((a, b) => (a === 0 || b === 0 ? 0 : Math.abs(a * b) / gcd(a, b)));
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
registerFunction("SUMX2MY2", (args) => pairAggregate(args, (a, b) => a * a - b * b), 1, 2, false, {
  signature: "SUMX2MY2(array_x, array_y)",
  category: "Math",
});
registerFunction("SUMXMY2", (args) => pairAggregate(args, (a, b) => (a - b) * (a - b)), 1, 2, false, {
  signature: "SUMXMY2(array_x, array_y)",
  category: "Math",
});
registerFunction("SUMX2PY2", (args) => pairAggregate(args, (a, b) => a * a + b * b), 1, 2, false, {
  signature: "SUMX2PY2(array_x, array_y)",
  category: "Math",
});

function pairAggregate(args: Scalar[][][], fn: (a: number, b: number) => number): number | FormulaError {
  const left = numbers([args[0] ?? []]);
  const right = numbers([args[1] ?? []]);
  const length = Math.min(left.length, right.length);
  if (length === 0) return ERR.num();
  let total = 0;
  for (let index = 0; index < length; index += 1) total += fn(left[index], right[index]);
  return total;
}

/** Used by the number-format picker to preview TEXT() output. */
export function previewFormat(value: Scalar, pattern: string): string {
  const number = toNumber(value);
  if (isError(number)) return toText(value);
  return formatNumber(number, pattern);
}
