/**
 * Linear regression: SLOPE, INTERCEPT, RSQ, STEYX, PEARSON, FORECAST and
 * FORECAST.LINEAR on paired data, and TREND and GROWTH on whole tables.
 *
 * The paired functions read their data like Excel: a text or blank cell drops
 * its partner, and two arrays of different size are `#N/A`. TREND and GROWTH
 * are stricter - every cell has to be a number - and may fit several x columns.
 */
import { registerFunction } from "../registry";
import { ERR, isError, optionalBool, toNumber, type CellMatrix, type FormulaError, type Scalar } from "../scalars";
import { meanOf, pairedNumbers } from "./stat-support";

interface Line {
  n: number;
  slope: number;
  intercept: number;
  /** Sums of squared deviations of x and y, and of their cross product. */
  sxx: number;
  syy: number;
  sxy: number;
}

/** Fits y on x by least squares; `#DIV/0!` when x does not vary. */
function fitLine(known: Scalar[][], given: Scalar[][]): Line | FormulaError {
  const paired = pairedNumbers(known, given);
  if (isError(paired)) return paired;
  const { first: y, second: x } = paired;
  const n = x.length;
  if (n === 0) return ERR.div();
  const meanX = meanOf(x);
  const meanY = meanOf(y);
  let sxx = 0;
  let syy = 0;
  let sxy = 0;
  for (let index = 0; index < n; index += 1) {
    sxx += (x[index] - meanX) ** 2;
    syy += (y[index] - meanY) ** 2;
    sxy += (x[index] - meanX) * (y[index] - meanY);
  }
  if (sxx === 0) return ERR.div();
  const slope = sxy / sxx;
  return { n, slope, intercept: meanY - slope * meanX, sxx, syy, sxy };
}

/** Registers a function of (known_y's, known_x's) that reads one number off the fitted line. */
function lineStatistic(name: string, signature: string, read: (line: Line) => number | FormulaError): void {
  registerFunction(
    name,
    (args) => {
      const line = fitLine(args[0] ?? [], args[1] ?? []);
      return isError(line) ? line : read(line);
    },
    2,
    2,
    false,
    { signature, category: "Statistics" },
  );
}

lineStatistic("SLOPE", "SLOPE(known_y's, known_x's)", (line) => line.slope);
lineStatistic("INTERCEPT", "INTERCEPT(known_y's, known_x's)", (line) => line.intercept);
lineStatistic("RSQ", "RSQ(known_y's, known_x's)", (line) =>
  line.syy === 0 ? ERR.div() : (line.sxy * line.sxy) / (line.sxx * line.syy),
);
lineStatistic("STEYX", "STEYX(known_y's, known_x's)", (line) =>
  // A perfect fit leaves a residual of exactly 0, not a tiny negative number.
  line.n < 3 ? ERR.div() : Math.sqrt(Math.max(0, line.syy - (line.sxy * line.sxy) / line.sxx) / (line.n - 2)),
);
lineStatistic("PEARSON", "PEARSON(array1, array2)", (line) =>
  line.syy === 0 ? ERR.div() : line.sxy / Math.sqrt(line.sxx * line.syy),
);

for (const name of ["FORECAST", "FORECAST.LINEAR"]) {
  registerFunction(
    name,
    (args) => {
      const x = toNumber(args[0]?.[0]?.[0] ?? "");
      if (isError(x)) return x;
      const line = fitLine(args[1] ?? [], args[2] ?? []);
      return isError(line) ? line : line.intercept + line.slope * x;
    },
    3,
    3,
    false,
    { signature: `${name}(x, known_y's, known_x's)`, category: "Statistics" },
  );
}

// ---------------------------------------------------------------------------
// TREND and GROWTH
// ---------------------------------------------------------------------------

/** The numbers in a table, or `#VALUE!` when any cell is text, a boolean or blank. */
function numericTable(table: Scalar[][]): number[][] | FormulaError {
  const out: number[][] = [];
  for (const row of table) {
    const line: number[] = [];
    for (const value of row) {
      if (typeof value !== "number") return ERR.value();
      line.push(value);
    }
    out.push(line);
  }
  return out;
}

/** An argument left out, or skipped with an empty slot (`TREND(y,,new_x)`). */
function isMissing(arg: Scalar[][] | undefined): boolean {
  return arg === undefined || (arg.length === 1 && arg[0].length === 1 && arg[0][0] === "");
}

/**
 * Least squares of `y` on the columns of `rows`. A column that adds nothing
 * (it never varies, or repeats another) gets coefficient 0, as LINEST does.
 */
function leastSquares(rows: number[][], y: number[], intercept: boolean): { slopes: number[]; constant: number } {
  const p = rows[0]?.length ?? 0;
  const n = rows.length;
  const means = Array<number>(p).fill(0);
  let meanY = 0;
  if (intercept) {
    for (let column = 0; column < p; column += 1) means[column] = meanOf(rows.map((row) => row[column]));
    meanY = meanOf(y);
  }
  // Normal equations on centred data, with the right-hand side as column p.
  const system = Array.from({ length: p }, () => Array<number>(p + 1).fill(0));
  for (let index = 0; index < n; index += 1) {
    for (let a = 0; a < p; a += 1) {
      const left = rows[index][a] - means[a];
      for (let b = 0; b < p; b += 1) system[a][b] += left * (rows[index][b] - means[b]);
      system[a][p] += left * (y[index] - meanY);
    }
  }
  const scale = system.map((row, index) => row[index]);
  const slopes = Array<number>(p).fill(0);
  const used: number[] = [];
  for (let pivot = 0; pivot < p; pivot += 1) {
    if (!(system[pivot][pivot] > 1e-10 * scale[pivot])) continue;
    used.push(pivot);
    const divisor = system[pivot][pivot];
    for (let column = 0; column <= p; column += 1) system[pivot][column] /= divisor;
    for (let row = 0; row < p; row += 1) {
      if (row === pivot) continue;
      const factor = system[row][pivot];
      for (let column = 0; column <= p; column += 1) system[row][column] -= factor * system[pivot][column];
    }
  }
  for (const pivot of used) slopes[pivot] = system[pivot][p];
  const constant = intercept ? meanY - slopes.reduce((total, slope, column) => total + slope * means[column], 0) : 0;
  return { slopes, constant };
}

/** TREND (`exponential` false) and GROWTH (true): y = b + Σ m·x, or y = b · Π m^x. */
function project(args: Scalar[][][], exponential: boolean): CellMatrix | FormulaError {
  const known = numericTable(args[0] ?? []);
  if (isError(known)) return known;
  const rowsOfY = known.length;
  const colsOfY = known[0]?.length ?? 0;
  if (rowsOfY === 0 || colsOfY === 0 || (rowsOfY > 1 && colsOfY > 1)) return ERR.ref();
  // Observations run down a column of y, or along a row of y.
  const byRow = colsOfY > 1;
  const y = known.flat();
  const count = y.length;
  if (exponential && y.some((value) => value <= 0)) return ERR.num();

  /** Turns a table of x values into one row per observation. */
  const observations = (table: number[][]): number[][] =>
    byRow ? table[0].map((_, i) => table.map((r) => r[i])) : table;
  let x: number[][];
  if (isMissing(args[1])) {
    x = Array.from({ length: count }, (_, index) => [index + 1]);
  } else {
    const given = numericTable(args[1]);
    if (isError(given)) return given;
    x = observations(given);
    if (x.length !== count) return ERR.ref();
  }
  const width = x[0].length;
  let target = x;
  if (!isMissing(args[2])) {
    const wanted = numericTable(args[2]);
    if (isError(wanted)) return wanted;
    target = observations(wanted);
    if ((target[0]?.length ?? 0) !== width) return ERR.ref();
  }
  const intercept = isMissing(args[3]) ? true : optionalBool(args[3][0][0], true);
  if (isError(intercept)) return intercept;

  const { slopes, constant } = leastSquares(x, exponential ? y.map(Math.log) : y, intercept);
  const predicted = target.map((row) => {
    const value = constant + row.reduce((total, item, column) => total + item * slopes[column], 0);
    return exponential ? Math.exp(value) : value;
  });
  if (predicted.some((value) => !Number.isFinite(value))) return ERR.num();
  return byRow ? [predicted] : predicted.map((value) => [value]);
}

registerFunction("TREND", (args) => project(args, false), 1, 4, false, {
  signature: "TREND(known_y's, [known_x's], [new_x's], [const])",
  category: "Statistics",
});
registerFunction("GROWTH", (args) => project(args, true), 1, 4, false, {
  signature: "GROWTH(known_y's, [known_x's], [new_x's], [const])",
  category: "Statistics",
});
