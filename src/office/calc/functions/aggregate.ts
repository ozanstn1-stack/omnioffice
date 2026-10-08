/**
 * SUBTOTAL and AGGREGATE.
 *
 * Both work on references because what they total depends on where the cells
 * are: rows a filter hides and cells that are subtotals themselves are left
 * out. A row counts as hidden when the evaluation context says so (see
 * `FunctionHost.hiddenRow`); a caller that exposes no row layout gets every row.
 *
 * Neither is volatile. Hiding rows rebuilds the sheet model, which recalculates
 * the workbook from scratch, so the totals follow a filter without help.
 */
import { registerContextFunction, type ContextArgument, type FunctionHost } from "../registry";
import type { CellReference } from "../references";
import { ERR, isError, toNumber, type CellMatrix, type FormulaError, type Scalar } from "../scalars";
import { isSkipped, locationOf, scalarOf } from "./context-support";
import {
  meanOf,
  medianOf,
  modesOf,
  nthValue,
  percentileExc,
  percentileInc,
  quartileFraction,
  sumOf,
  varianceOf,
} from "./stat-support";

/** A cell whose formula calls SUBTOTAL; SUBTOTAL never adds another one up. */
const NESTED_SUBTOTAL = /(?<![A-Za-z0-9_.])SUBTOTAL\s*\(/i;
/** AGGREGATE (options 0-3) also steps over cells that call AGGREGATE. */
const NESTED_TOTAL = /(?<![A-Za-z0-9_.])(?:SUBTOTAL|AGGREGATE)\s*\(/i;

/** What to leave out while gathering the cells a total reads. */
interface Skip {
  /** Rows to drop: only those a filter hides, or every hidden row. */
  rows: "none" | "filtered" | "hidden";
  /** Cells whose formula matches are subtotals of their own. */
  nested: RegExp | null;
  errors: boolean;
}

interface Gathered {
  numbers: number[];
  /** Non-empty cells, which is what COUNTA counts. */
  filled: number;
  /** The first error that was not skipped. */
  error: FormulaError | null;
}

function take(value: Scalar, skip: Skip, into: Gathered): void {
  if (value === "") return;
  if (isError(value)) {
    if (skip.errors) return;
    into.filled += 1;
    into.error ??= value;
    return;
  }
  into.filled += 1;
  if (typeof value === "number") into.numbers.push(value);
}

function gatherReference(
  reference: CellReference,
  host: FunctionHost,
  skip: Skip,
  into: Gathered,
): FormulaError | null {
  const matrix = host.read(reference);
  if (isError(matrix)) return matrix;
  matrix.forEach((line, offset) => {
    const row = reference.start.row + offset;
    if (skip.rows !== "none") {
      const why = host.hiddenRow(reference.sheet, row);
      if (why === "filtered" || (why === "hidden" && skip.rows === "hidden")) return;
    }
    line.forEach((value, at) => {
      if (value === "") return;
      if (skip.nested) {
        const formula = host.formulaAt(reference.sheet, { row, col: reference.start.col + at });
        if (formula !== null && skip.nested.test(formula)) return;
      }
      take(value, skip, into);
    });
  });
  return null;
}

function gatherValues(matrix: Scalar | CellMatrix, skip: Skip, into: Gathered): void {
  if (!Array.isArray(matrix)) return take(matrix, skip, into);
  for (const line of matrix) for (const value of line) take(value, skip, into);
}

function emptyGathered(): Gathered {
  return { numbers: [], filled: 0, error: null };
}

function largest(values: number[]): number {
  return values.reduce((best, value) => Math.max(best, value), Number.NEGATIVE_INFINITY);
}

function smallest(values: number[]): number {
  return values.reduce((best, value) => Math.min(best, value), Number.POSITIVE_INFINITY);
}

/** The function behind SUBTOTAL 1-11 and AGGREGATE 1-19, applied to what was gathered. */
function reduceGathered(num: number, data: Gathered, k: number): Scalar {
  // The counting functions look at cells, not at what is in them.
  if (num === 3) return data.filled;
  if (num === 2) return data.numbers.length;
  if (data.error) return data.error;
  const values = data.numbers;
  const sorted = (): number[] => [...values].sort((a, b) => a - b);
  switch (num) {
    case 1:
      return values.length === 0 ? ERR.div() : meanOf(values);
    case 4:
      return values.length === 0 ? 0 : largest(values);
    case 5:
      return values.length === 0 ? 0 : smallest(values);
    case 6:
      return values.length === 0 ? 0 : values.reduce((product, value) => product * value, 1);
    case 7:
    case 8:
    case 10:
    case 11: {
      const variance = varianceOf(values, num === 7 || num === 10);
      if (isError(variance)) return variance;
      return num === 7 || num === 8 ? Math.sqrt(variance) : variance;
    }
    case 9:
      return sumOf(values);
    case 12:
      return medianOf(values);
    case 13: {
      const { modes } = modesOf(values);
      return modes.length === 0 ? ERR.na() : modes[0];
    }
    case 14:
    case 15:
      return nthValue(values, k, num === 14);
    case 16:
      return percentileInc(sorted(), k);
    case 17:
    case 19: {
      const fraction = quartileFraction(k, num === 17);
      if (isError(fraction)) return fraction;
      return num === 17 ? percentileInc(sorted(), fraction) : percentileExc(sorted(), fraction);
    }
    case 18:
      return percentileExc(sorted(), k);
    default:
      return ERR.value();
  }
}

/** The number in `arg`, truncated; a skipped optional argument is `fallback`. */
function wholeNumber(arg: ContextArgument | undefined, fallback: number): number | FormulaError {
  if (arg === undefined || isSkipped(arg)) return fallback;
  const value = toNumber(scalarOf(arg));
  return isError(value) ? value : Math.trunc(value);
}

registerContextFunction(
  "SUBTOTAL",
  (args, host) => {
    const num = wholeNumber(args[0], Number.NaN);
    if (isError(num)) return num;
    const known = (num >= 1 && num <= 11) || (num >= 101 && num <= 111);
    if (!known) return ERR.value();
    // 1-11 skip rows a filter hides; 101-111 skip rows hidden by hand as well.
    const skip: Skip = { rows: num > 100 ? "hidden" : "filtered", nested: NESTED_SUBTOTAL, errors: false };
    const data = emptyGathered();
    for (const arg of args.slice(1)) {
      const reference = locationOf(arg);
      if (isError(reference)) return reference;
      const failure = gatherReference(reference, host, skip, data);
      if (failure) return failure;
    }
    return reduceGathered(num % 100, data, 0);
  },
  2,
  255,
  {
    signature: "SUBTOTAL(function_num, ref1, ...)  function_num 1-11 (101-111 also skips hidden rows)",
    category: "Math",
  },
);

registerContextFunction(
  "AGGREGATE",
  (args, host) => {
    const num = wholeNumber(args[0], Number.NaN);
    if (isError(num)) return num;
    if (!(num >= 1 && num <= 19)) return ERR.value();
    const option = wholeNumber(args[1], 0);
    if (isError(option)) return option;
    if (option < 0 || option > 7) return ERR.value();
    const skip: Skip = {
      rows: [1, 3, 5, 7].includes(option) ? "hidden" : "none",
      nested: option <= 3 ? NESTED_TOTAL : null,
      errors: [2, 3, 6, 7].includes(option),
    };
    const data = emptyGathered();
    let k = 0;
    if (num >= 14) {
      // Array form: AGGREGATE(function_num, options, array, k).
      if (args.length !== 4) return ERR.value();
      const rank = toNumber(scalarOf(args[3]));
      if (isError(rank)) return rank;
      k = rank;
      const reference = args[2].reference();
      if (isError(reference)) return reference;
      if (reference) {
        const failure = gatherReference(reference, host, skip, data);
        if (failure) return failure;
      } else {
        gatherValues(args[2].value(), skip, data);
      }
    } else {
      // Reference form: every argument after the options is a location.
      for (const arg of args.slice(2)) {
        const reference = locationOf(arg);
        if (isError(reference)) return reference;
        const failure = gatherReference(reference, host, skip, data);
        if (failure) return failure;
      }
    }
    return reduceGathered(num, data, k);
  },
  3,
  255,
  {
    signature: "AGGREGATE(function_num, options, ref1, ...)  function_num 1-13; 14-19 take (array, k)",
    category: "Math",
  },
);
