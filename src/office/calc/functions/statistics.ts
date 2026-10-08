/**
 * Statistical aggregates, conditional aggregation and ranking.
 */
import { registerFunction } from "../registry";
import {
  ERR,
  criteriaMatcher,
  flatten,
  isError,
  numbers,
  toNumber,
  toText,
  type CellMatrix,
  type FormulaError,
  type Scalar,
} from "../scalars";
import {
  meanOf,
  modesOf,
  numericValues,
  pairedNumbers,
  percentileExc,
  percentileInc,
  quartileFraction,
  sumSquaredDeviations,
  tidy,
} from "./stat-support";

/** Sums the values in `sumMatrix` whose position passes every criteria pair. */
function conditionalAggregate(
  sumMatrix: Scalar[],
  pairs: Array<{ range: Scalar[]; matches: (value: Scalar) => boolean }>,
): number {
  let total = 0;
  sumMatrix.forEach((value, index) => {
    if (!pairs.every((pair) => pair.matches(pair.range[index] ?? ""))) return;
    const number = toNumber(value);
    if (!isError(number)) total += number;
  });
  return total;
}

function conditionPairs(
  args: Scalar[][][],
  start: number,
): Array<{ range: Scalar[]; matches: (value: Scalar) => boolean }> {
  const pairs: Array<{ range: Scalar[]; matches: (value: Scalar) => boolean }> = [];
  for (let position = start; position + 1 < args.length; position += 2) {
    pairs.push({
      range: flatten([args[position] ?? []]),
      matches: criteriaMatcher(args[position + 1]?.[0]?.[0] ?? ""),
    });
  }
  return pairs;
}

registerFunction("SUM", (args) => numbers(args).reduce((sum, value) => sum + value, 0), 1, 64, false, {
  signature: "SUM(number1, ...)",
  category: "Statistics",
});
registerFunction("PRODUCT", (args) => numbers(args).reduce((product, value) => product * value, 1), 1, 64, false, {
  signature: "PRODUCT(number1, ...)",
  category: "Statistics",
});
registerFunction(
  "AVERAGE",
  (args) => {
    const values = numbers(args);
    return values.length === 0 ? ERR.div() : values.reduce((sum, value) => sum + value, 0) / values.length;
  },
  1,
  64,
  false,
  { signature: "AVERAGE(number1, ...)", category: "Statistics" },
);
registerFunction(
  "MIN",
  (args) => {
    const values = numbers(args);
    return values.length === 0 ? 0 : Math.min(...values);
  },
  1,
  64,
  false,
  { signature: "MIN(number1, ...)", category: "Statistics" },
);
registerFunction(
  "MAX",
  (args) => {
    const values = numbers(args);
    return values.length === 0 ? 0 : Math.max(...values);
  },
  1,
  64,
  false,
  { signature: "MAX(number1, ...)", category: "Statistics" },
);
registerFunction("COUNT", (args) => numbers(args).length, 1, 64, false, {
  signature: "COUNT(value1, ...)",
  category: "Statistics",
  ignoresRangeErrors: true,
});
registerFunction(
  "COUNTA",
  (args) => flatten(args).filter((value) => !isError(value) && toText(value) !== "").length,
  1,
  64,
  false,
  { signature: "COUNTA(value1, ...)", category: "Statistics", ignoresRangeErrors: true },
);
registerFunction("COUNTBLANK", (args) => flatten(args).filter((value) => toText(value) === "").length, 1, 64, false, {
  signature: "COUNTBLANK(range)",
  category: "Statistics",
  ignoresRangeErrors: true,
});
registerFunction(
  "MEDIAN",
  (args) => {
    const values = numbers(args).sort((a, b) => a - b);
    if (values.length === 0) return ERR.num();
    const middle = Math.floor(values.length / 2);
    return values.length % 2 === 0 ? (values[middle - 1] + values[middle]) / 2 : values[middle];
  },
  1,
  64,
  false,
  { signature: "MEDIAN(number1, ...)", category: "Statistics" },
);
const sampleVariance = (args: Scalar[][][]) => {
  const values = numbers(args);
  if (values.length < 2) return ERR.div();
  const mean = values.reduce((sum, value) => sum + value, 0) / values.length;
  return values.reduce((sum, value) => sum + (value - mean) ** 2, 0) / (values.length - 1);
};
const populationVariance = (args: Scalar[][][]) => {
  const values = numbers(args);
  if (values.length === 0) return ERR.div();
  const mean = values.reduce((sum, value) => sum + value, 0) / values.length;
  return values.reduce((sum, value) => sum + (value - mean) ** 2, 0) / values.length;
};
registerFunction(
  "STDEV",
  (args) => {
    const variance = sampleVariance(args);
    return isError(variance) ? variance : Math.sqrt(variance);
  },
  1,
  64,
  false,
  { signature: "STDEV(number1, ...)", category: "Statistics" },
);
registerFunction(
  "STDEV.S",
  (args) => {
    const variance = sampleVariance(args);
    return isError(variance) ? variance : Math.sqrt(variance);
  },
  1,
  64,
  false,
  { signature: "STDEV.S(number1, ...)", category: "Statistics" },
);
registerFunction(
  "STDEVP",
  (args) => {
    const variance = populationVariance(args);
    return isError(variance) ? variance : Math.sqrt(variance);
  },
  1,
  64,
  false,
  { signature: "STDEVP(number1, ...)", category: "Statistics" },
);
registerFunction(
  "STDEV.P",
  (args) => {
    const variance = populationVariance(args);
    return isError(variance) ? variance : Math.sqrt(variance);
  },
  1,
  64,
  false,
  { signature: "STDEV.P(number1, ...)", category: "Statistics" },
);
registerFunction("VAR", sampleVariance, 1, 64, false, { signature: "VAR(number1, ...)", category: "Statistics" });
registerFunction("VAR.S", sampleVariance, 1, 64, false, { signature: "VAR.S(number1, ...)", category: "Statistics" });
registerFunction("VARP", populationVariance, 1, 64, false, { signature: "VARP(number1, ...)", category: "Statistics" });
registerFunction("VAR.P", populationVariance, 1, 64, false, {
  signature: "VAR.P(number1, ...)",
  category: "Statistics",
});

const percentile = (args: Scalar[][][]) => {
  const values = numbers([args[0] ?? []]).sort((a, b) => a - b);
  const k = toNumber(args[1]?.[0]?.[0] ?? 0);
  if (isError(k)) return k;
  return percentileInc(values, Math.trunc(k * 1e12) / 1e12);
};
registerFunction("PERCENTILE", percentile, 2, 2, false, { signature: "PERCENTILE(array, k)", category: "Statistics" });
registerFunction("PERCENTILE.INC", percentile, 2, 2, false, {
  signature: "PERCENTILE.INC(array, k)",
  category: "Statistics",
});
const quartile = (args: Scalar[][][]) => {
  const values = numbers([args[0] ?? []]).sort((a, b) => a - b);
  const quart = toNumber(args[1]?.[0]?.[0] ?? 0);
  if (isError(quart)) return quart;
  const index = Math.trunc(quart);
  if (index !== quart || index < 0 || index > 4) return ERR.num();
  return percentileInc(values, index / 4);
};
registerFunction("QUARTILE", quartile, 2, 2, false, { signature: "QUARTILE(array, quart)", category: "Statistics" });
registerFunction("QUARTILE.INC", quartile, 2, 2, false, {
  signature: "QUARTILE.INC(array, quart)",
  category: "Statistics",
});

/**
 * Pairs two ranges into x/y series. Different sizes are `#N/A`; text and blank
 * cells drop their partner; fewer than `minimum` pairs is `#DIV/0!`.
 */
function pairedValues(args: Scalar[][][], minimum: number): { x: number[]; y: number[] } | FormulaError {
  const paired = pairedNumbers(args[0] ?? [], args[1] ?? []);
  if (isError(paired)) return paired;
  if (paired.first.length < minimum) return ERR.div();
  return { x: paired.first, y: paired.second };
}

function covarianceOf(x: number[], y: number[], divisor: number): number {
  const meanX = x.reduce((sum, value) => sum + value, 0) / x.length;
  const meanY = y.reduce((sum, value) => sum + value, 0) / y.length;
  let total = 0;
  for (let index = 0; index < x.length; index += 1) total += (x[index] - meanX) * (y[index] - meanY);
  return total / divisor;
}

registerFunction(
  "CORREL",
  (args) => {
    const paired = pairedValues(args, 2);
    if (isError(paired)) return paired;
    const covariance = covarianceOf(paired.x, paired.y, paired.x.length - 1);
    const spreadX = covarianceOf(paired.x, paired.x, paired.x.length - 1);
    const spreadY = covarianceOf(paired.y, paired.y, paired.y.length - 1);
    if (spreadX === 0 || spreadY === 0) return ERR.div();
    return covariance / Math.sqrt(spreadX * spreadY);
  },
  2,
  2,
  false,
  { signature: "CORREL(array1, array2)", category: "Statistics" },
);
registerFunction(
  "COVARIANCE.P",
  (args) => {
    const paired = pairedValues(args, 1);
    if (isError(paired)) return paired;
    return covarianceOf(paired.x, paired.y, paired.x.length);
  },
  2,
  2,
  false,
  { signature: "COVARIANCE.P(array1, array2)", category: "Statistics" },
);
registerFunction(
  "COVARIANCE.S",
  (args) => {
    const paired = pairedValues(args, 2);
    if (isError(paired)) return paired;
    return covarianceOf(paired.x, paired.y, paired.x.length - 1);
  },
  2,
  2,
  false,
  { signature: "COVARIANCE.S(array1, array2)", category: "Statistics" },
);
registerFunction(
  "COVAR",
  (args) => {
    const paired = pairedValues(args, 1);
    if (isError(paired)) return paired;
    return covarianceOf(paired.x, paired.y, paired.x.length);
  },
  2,
  2,
  false,
  { signature: "COVAR(array1, array2)", category: "Statistics" },
);

registerFunction(
  "LARGE",
  (args) => {
    const values = numbers([args[0] ?? []]).sort((a, b) => b - a);
    const k = toNumber(args[1]?.[0]?.[0] ?? 1);
    if (isError(k)) return k;
    const index = Math.trunc(k);
    if (index < 1 || index > values.length) return ERR.num();
    return values[index - 1];
  },
  2,
  2,
  false,
  { signature: "LARGE(array, k)", category: "Statistics" },
);
registerFunction(
  "SMALL",
  (args) => {
    const values = numbers([args[0] ?? []]).sort((a, b) => a - b);
    const k = toNumber(args[1]?.[0]?.[0] ?? 1);
    if (isError(k)) return k;
    const index = Math.trunc(k);
    if (index < 1 || index > values.length) return ERR.num();
    return values[index - 1];
  },
  2,
  2,
  false,
  { signature: "SMALL(array, k)", category: "Statistics" },
);
registerFunction(
  "RANK",
  (args) => {
    const value = toNumber(args[0]?.[0]?.[0] ?? 0);
    if (isError(value)) return value;
    const values = numbers([args[1] ?? []]).sort((a, b) => b - a);
    const ascending = toNumber(args[2]?.[0]?.[0] ?? 0) !== 0;
    const ordered = ascending ? [...values].sort((a, b) => a - b) : values;
    const index = ordered.indexOf(value);
    return index < 0 ? ERR.na() : index + 1;
  },
  2,
  3,
  false,
  { signature: "RANK(number, ref, [order])", category: "Statistics" },
);
registerFunction(
  "RANK.EQ",
  (args) => {
    const value = toNumber(args[0]?.[0]?.[0] ?? 0);
    if (isError(value)) return value;
    const values = numbers([args[1] ?? []]);
    const ascending = toNumber(args[2]?.[0]?.[0] ?? 0) !== 0;
    const ordered = ascending ? [...values].sort((a, b) => a - b) : [...values].sort((a, b) => b - a);
    const index = ordered.indexOf(value);
    return index < 0 ? ERR.na() : index + 1;
  },
  2,
  3,
  false,
  { signature: "RANK.EQ(number, ref, [order])", category: "Statistics" },
);

registerFunction(
  "PERCENTILE.EXC",
  (args) => {
    const values = numericValues(args[0] ?? []).sort((a, b) => a - b);
    const k = toNumber(args[1]?.[0]?.[0] ?? 0);
    return isError(k) ? k : percentileExc(values, k);
  },
  2,
  2,
  false,
  { signature: "PERCENTILE.EXC(array, k)", category: "Statistics" },
);
registerFunction(
  "QUARTILE.EXC",
  (args) => {
    const values = numericValues(args[0] ?? []).sort((a, b) => a - b);
    const quart = toNumber(args[1]?.[0]?.[0] ?? 0);
    if (isError(quart)) return quart;
    const fraction = quartileFraction(quart, false);
    return isError(fraction) ? fraction : percentileExc(values, fraction);
  },
  2,
  2,
  false,
  { signature: "QUARTILE.EXC(array, quart)", category: "Statistics" },
);

/**
 * PERCENTRANK: where `x` sits in `array` as a fraction. The inclusive version
 * spreads ranks over 0..1, the exclusive one over 1/(n+1)..n/(n+1); a value
 * between two data points interpolates, and the result is truncated (not
 * rounded) to `significance` digits.
 */
function percentRank(args: Scalar[][][], inclusive: boolean): number | FormulaError {
  const values = numericValues(args[0] ?? []).sort((a, b) => a - b);
  const x = toNumber(args[1]?.[0]?.[0] ?? "");
  if (isError(x)) return x;
  const digits = args[2] === undefined || args[2][0]?.[0] === "" ? 3 : toNumber(args[2][0][0]);
  if (isError(digits)) return digits;
  if (values.length === 0 || digits < 1) return ERR.num();
  const n = values.length;
  if (x < values[0] || x > values[n - 1]) return ERR.na();
  const rankOf = (index: number) => (inclusive ? (n === 1 ? 1 : index / (n - 1)) : (index + 1) / (n + 1));
  const below = values.filter((value) => value < x).length;
  let rank: number;
  if (values[below] === x) {
    rank = rankOf(below);
  } else {
    // x lies strictly between values[below - 1] and values[below]; the lower
    // neighbour's rank is that of its first occurrence.
    const lower = values[below - 1];
    const lowerRank = rankOf(values.indexOf(lower));
    rank = lowerRank + ((x - lower) / (values[below] - lower)) * (rankOf(below) - lowerRank);
  }
  const factor = 10 ** Math.trunc(digits);
  return Math.floor(tidy(rank * factor)) / factor;
}
const percentRankInc = (args: Scalar[][][]) => percentRank(args, true);
registerFunction("PERCENTRANK", percentRankInc, 2, 3, false, {
  signature: "PERCENTRANK(array, x, [significance])",
  category: "Statistics",
});
registerFunction("PERCENTRANK.INC", percentRankInc, 2, 3, false, {
  signature: "PERCENTRANK.INC(array, x, [significance])",
  category: "Statistics",
});
registerFunction("PERCENTRANK.EXC", (args) => percentRank(args, false), 2, 3, false, {
  signature: "PERCENTRANK.EXC(array, x, [significance])",
  category: "Statistics",
});

const modeSngl = (args: Scalar[][][]) => {
  const { modes } = modesOf(args.flatMap(numericValues));
  return modes.length === 0 ? ERR.na() : modes[0];
};
registerFunction("MODE", modeSngl, 1, 64, false, { signature: "MODE(number1, ...)", category: "Statistics" });
registerFunction("MODE.SNGL", modeSngl, 1, 64, false, {
  signature: "MODE.SNGL(number1, ...)",
  category: "Statistics",
});
registerFunction(
  "MODE.MULT",
  (args) => {
    const { modes } = modesOf(args.flatMap(numericValues));
    return modes.length === 0 ? ERR.na() : modes.map((mode) => [mode]);
  },
  1,
  64,
  false,
  { signature: "MODE.MULT(number1, ...)", category: "Statistics" },
);

registerFunction(
  "GEOMEAN",
  (args) => {
    const values = args.flatMap(numericValues);
    if (values.length === 0 || values.some((value) => value <= 0)) return ERR.num();
    return Math.exp(meanOf(values.map(Math.log)));
  },
  1,
  64,
  false,
  { signature: "GEOMEAN(number1, ...)", category: "Statistics" },
);
registerFunction(
  "HARMEAN",
  (args) => {
    const values = args.flatMap(numericValues);
    if (values.length === 0 || values.some((value) => value <= 0)) return ERR.num();
    return values.length / values.reduce((total, value) => total + 1 / value, 0);
  },
  1,
  64,
  false,
  { signature: "HARMEAN(number1, ...)", category: "Statistics" },
);
registerFunction(
  "AVEDEV",
  (args) => {
    const values = args.flatMap(numericValues);
    if (values.length === 0) return ERR.num();
    const mean = meanOf(values);
    return meanOf(values.map((value) => Math.abs(value - mean)));
  },
  1,
  64,
  false,
  { signature: "AVEDEV(number1, ...)", category: "Statistics" },
);
registerFunction(
  "DEVSQ",
  (args) => {
    const values = args.flatMap(numericValues);
    return values.length === 0 ? ERR.num() : sumSquaredDeviations(values);
  },
  1,
  64,
  false,
  { signature: "DEVSQ(number1, ...)", category: "Statistics" },
);
registerFunction(
  "STANDARDIZE",
  (args) => {
    const x = toNumber(args[0]?.[0]?.[0] ?? "");
    if (isError(x)) return x;
    const mean = toNumber(args[1]?.[0]?.[0] ?? "");
    if (isError(mean)) return mean;
    const deviation = toNumber(args[2]?.[0]?.[0] ?? "");
    if (isError(deviation)) return deviation;
    return deviation <= 0 ? ERR.num() : (x - mean) / deviation;
  },
  3,
  3,
  false,
  { signature: "STANDARDIZE(x, mean, standard_dev)", category: "Statistics" },
);

registerFunction(
  "SUMIF",
  (args) => {
    const range = flatten([args[0] ?? []]);
    const sumRange = args[2] ? flatten([args[2]]) : range;
    return conditionalAggregate(sumRange, [{ range, matches: criteriaMatcher(args[1]?.[0]?.[0] ?? "") }]);
  },
  2,
  3,
  false,
  { signature: "SUMIF(range, criteria, [sum_range])", category: "Statistics" },
);
registerFunction(
  "COUNTIF",
  (args) => {
    const matches = criteriaMatcher(args[1]?.[0]?.[0] ?? "");
    return flatten([args[0] ?? []]).filter(matches).length;
  },
  2,
  2,
  false,
  { signature: "COUNTIF(range, criteria)", category: "Statistics", ignoresRangeErrors: true },
);
registerFunction(
  "AVERAGEIF",
  (args) => {
    const range = flatten([args[0] ?? []]);
    const averageRange = args[2] ? flatten([args[2]]) : range;
    const values: number[] = [];
    range.forEach((value, index) => {
      if (!criteriaMatcher(args[1]?.[0]?.[0] ?? "")(value)) return;
      const number = toNumber(averageRange[index] ?? 0);
      if (!isError(number)) values.push(number);
    });
    return values.length === 0 ? ERR.div() : values.reduce((sum, value) => sum + value, 0) / values.length;
  },
  2,
  3,
  false,
  { signature: "AVERAGEIF(range, criteria, [average_range])", category: "Statistics" },
);
registerFunction(
  "SUMIFS",
  (args) => conditionalAggregate(flatten([args[0] ?? []]), conditionPairs(args, 1)),
  3,
  64,
  false,
  {
    signature: "SUMIFS(sum_range, criteria_range1, criteria1, ...)",
    category: "Statistics",
  },
);
registerFunction(
  "COUNTIFS",
  (args) => {
    const pairs = conditionPairs(args, 0);
    if (pairs.length === 0) return 0;
    return pairs[0].range.filter((_value, index) => pairs.every((pair) => pair.matches(pair.range[index] ?? "")))
      .length;
  },
  2,
  64,
  false,
  { signature: "COUNTIFS(criteria_range1, criteria1, ...)", category: "Statistics", ignoresRangeErrors: true },
);
registerFunction(
  "AVERAGEIFS",
  (args) => {
    const target = flatten([args[0] ?? []]);
    const values: number[] = [];
    target.forEach((value, index) => {
      if (!conditionPairs(args, 1).every((pair) => pair.matches(pair.range[index] ?? ""))) return;
      const number = toNumber(value);
      if (!isError(number)) values.push(number);
    });
    return values.length === 0 ? ERR.div() : values.reduce((sum, value) => sum + value, 0) / values.length;
  },
  3,
  64,
  false,
  { signature: "AVERAGEIFS(average_range, criteria_range1, criteria1, ...)", category: "Statistics" },
);
registerFunction(
  "MAXIFS",
  (args) => {
    const target = flatten([args[0] ?? []]);
    const values: number[] = [];
    target.forEach((value, index) => {
      if (!conditionPairs(args, 1).every((pair) => pair.matches(pair.range[index] ?? ""))) return;
      const number = toNumber(value);
      if (!isError(number)) values.push(number);
    });
    return values.length === 0 ? 0 : Math.max(...values);
  },
  3,
  64,
  false,
  { signature: "MAXIFS(max_range, criteria_range1, criteria1, ...)", category: "Statistics" },
);
registerFunction(
  "MINIFS",
  (args) => {
    const target = flatten([args[0] ?? []]);
    const values: number[] = [];
    target.forEach((value, index) => {
      if (!conditionPairs(args, 1).every((pair) => pair.matches(pair.range[index] ?? ""))) return;
      const number = toNumber(value);
      if (!isError(number)) values.push(number);
    });
    return values.length === 0 ? 0 : Math.min(...values);
  },
  3,
  64,
  false,
  { signature: "MINIFS(min_range, criteria_range1, criteria1, ...)", category: "Statistics" },
);

registerFunction(
  "SUMPRODUCT",
  (args) => {
    const matrices = args.map((arg) => (arg.length > 0 ? arg : [["" as Scalar]]));
    if (matrices.length === 0) return 0;
    const first = matrices[0];
    let total = 0;
    for (let row = 0; row < first.length; row += 1) {
      for (let col = 0; col < (first[row]?.length ?? 0); col += 1) {
        let product = 1;
        for (const matrix of matrices) {
          const number = toNumber(matrix[row]?.[col] ?? 0);
          if (isError(number)) return number;
          product *= number;
        }
        total += product;
      }
    }
    return total;
  },
  1,
  32,
  false,
  { signature: "SUMPRODUCT(array1, ...)", category: "Statistics" },
);

registerFunction(
  "COUNTUNIQUE",
  (args) =>
    new Set(
      flatten(args)
        .filter((value) => !isError(value))
        .map(toText),
    ).size,
  1,
  8,
  false,
  {
    signature: "COUNTUNIQUE(range)",
    category: "Statistics",
    ignoresRangeErrors: true,
  },
);

/** Ranks a value inside a 2D matrix, highest first. */
export function rankInMatrix(matrix: CellMatrix, value: number): number | null {
  const flat = matrix
    .flat()
    .map((entry) => toNumber(entry))
    .filter((entry): entry is number => !isError(entry));
  const sorted = [...flat].sort((a, b) => b - a);
  const index = sorted.indexOf(value);
  return index < 0 ? null : index + 1;
}
