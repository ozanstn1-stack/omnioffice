/**
 * Numeric building blocks shared by the statistical functions, SUBTOTAL and
 * AGGREGATE, so each of them reads and reduces data the same way.
 */
import { ERR, FormulaError, type Scalar } from "../scalars";

/**
 * The numbers in a matrix. Text, booleans and empty cells are skipped even
 * when they look like numbers, which is how Excel reads a range (a typed
 * argument is the exception, and arrives here as a one-cell matrix).
 */
export function numericValues(matrix: Scalar[][]): number[] {
  const out: number[] = [];
  for (const row of matrix) for (const value of row) if (typeof value === "number") out.push(value);
  return out;
}

export function sumOf(values: number[]): number {
  return values.reduce((total, value) => total + value, 0);
}

export function meanOf(values: number[]): number {
  return sumOf(values) / values.length;
}

/** Sum of squared deviations from the mean. */
export function sumSquaredDeviations(values: number[]): number {
  const mean = meanOf(values);
  return values.reduce((total, value) => total + (value - mean) ** 2, 0);
}

/** Sample or population variance; `#DIV/0!` when there are too few values. */
export function varianceOf(values: number[], sample: boolean): number | FormulaError {
  if (values.length < (sample ? 2 : 1)) return ERR.div();
  return sumSquaredDeviations(values) / (sample ? values.length - 1 : values.length);
}

export function medianOf(values: number[]): number | FormulaError {
  if (values.length === 0) return ERR.num();
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 0 ? (sorted[middle - 1] + sorted[middle]) / 2 : sorted[middle];
}

/**
 * The most frequent numbers in order of first appearance; empty when no number
 * repeats. MODE.SNGL takes the first, MODE.MULT keeps them all.
 */
export function modesOf(values: number[]): { modes: number[]; count: number } {
  const counts = new Map<number, number>();
  for (const value of values) counts.set(value, (counts.get(value) ?? 0) + 1);
  let best = 0;
  for (const count of counts.values()) best = Math.max(best, count);
  // Map keeps insertion order, so modes come out in order of first appearance.
  const modes = best < 2 ? [] : [...counts].filter(([, count]) => count === best).map(([value]) => value);
  return { modes, count: best };
}

/** Inclusive percentile of ascending `sorted`, interpolating between neighbours. */
export function percentileInc(sorted: number[], k: number): number | FormulaError {
  if (sorted.length === 0 || k < 0 || k > 1) return ERR.num();
  const position = (sorted.length - 1) * k;
  const lower = Math.floor(position);
  const upper = Math.ceil(position);
  if (lower === upper) return sorted[lower];
  return sorted[lower] + (sorted[upper] - sorted[lower]) * (position - lower);
}

/** Exclusive percentile: k must leave room for a value on either side. */
export function percentileExc(sorted: number[], k: number): number | FormulaError {
  const n = sorted.length;
  if (n === 0) return ERR.num();
  const position = k * (n + 1);
  if (position < 1 || position > n) return ERR.num();
  const lower = Math.floor(position);
  if (lower === position) return sorted[lower - 1];
  return sorted[lower - 1] + (sorted[lower] - sorted[lower - 1]) * (position - lower);
}

/** k-th largest (descending) or smallest (ascending) value; `k` is truncated. */
export function nthValue(values: number[], k: number, largest: boolean): number | FormulaError {
  const index = Math.trunc(k);
  if (index < 1 || index > values.length) return ERR.num();
  const sorted = [...values].sort((a, b) => (largest ? b - a : a - b));
  return sorted[index - 1];
}

/** A quartile number as a fraction of the way through the data. */
export function quartileFraction(quart: number, inclusive: boolean): number | FormulaError {
  const index = Math.trunc(quart);
  if (index < (inclusive ? 0 : 1) || index > (inclusive ? 4 : 3)) return ERR.num();
  return index / 4;
}

/** Rounds away binary noise so `1.4000000000000001` reads as `1.4`. */
export function tidy(value: number): number {
  return Number(value.toPrecision(15));
}

/**
 * Two data sets read as x/y pairs. Arrays with a different number of cells
 * (or none) are `#N/A`; a pair is dropped when either side is not a number, so
 * a text or blank cell removes its partner too.
 */
export function pairedNumbers(
  first: Scalar[][],
  second: Scalar[][],
): { first: number[]; second: number[] } | FormulaError {
  const left = first.flat();
  const right = second.flat();
  if (left.length === 0 || left.length !== right.length) return ERR.na();
  const out = { first: [] as number[], second: [] as number[] };
  left.forEach((a, index) => {
    const b = right[index];
    if (typeof a === "number" && typeof b === "number") {
      out.first.push(a);
      out.second.push(b);
    }
  });
  return out;
}
