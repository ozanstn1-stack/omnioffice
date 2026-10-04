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
  type Scalar,
} from "../scalars";

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

/** Inclusive percentile with linear interpolation between neighbours. */
function percentileOf(values: number[], k: number): number | ReturnType<typeof ERR.num> {
  if (values.length === 0 || k < 0 || k > 1) return ERR.num();
  const position = (values.length - 1) * k;
  const lower = Math.floor(position);
  const upper = Math.ceil(position);
  if (lower === upper) return values[lower];
  return values[lower] + (values[upper] - values[lower]) * (position - lower);
}

const percentile = (args: Scalar[][][]) => {
  const values = numbers([args[0] ?? []]).sort((a, b) => a - b);
  const k = toNumber(args[1]?.[0]?.[0] ?? 0);
  if (isError(k)) return k;
  return percentileOf(values, Math.trunc(k * 1e12) / 1e12);
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
  return percentileOf(values, index / 4);
};
registerFunction("QUARTILE", quartile, 2, 2, false, { signature: "QUARTILE(array, quart)", category: "Statistics" });
registerFunction("QUARTILE.INC", quartile, 2, 2, false, {
  signature: "QUARTILE.INC(array, quart)",
  category: "Statistics",
});

/** Pairs two ranges into x/y series, stopping at the shorter one. */
function pairedValues(args: Scalar[][][], minimum: number): { x: number[]; y: number[] } | ReturnType<typeof ERR.div> {
  const x = numbers([args[0] ?? []]);
  const y = numbers([args[1] ?? []]);
  const length = Math.min(x.length, y.length);
  if (length < minimum) return ERR.div();
  return { x: x.slice(0, length), y: y.slice(0, length) };
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
    if ("code" in paired) return paired;
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
    if ("code" in paired) return paired;
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
    if ("code" in paired) return paired;
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
    if ("code" in paired) return paired;
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
