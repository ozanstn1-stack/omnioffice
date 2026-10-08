/**
 * Helpers for reading the arguments of a context function (see `registry.ts`),
 * shared by the reference functions, SUBTOTAL and AGGREGATE.
 */
import type { ContextArgument } from "../registry";
import type { CellReference } from "../references";
import { ERR, isError, toNumber, type FormulaError, type Scalar } from "../scalars";

/** The first value of an argument; an error value stays an error. */
export function scalarOf(arg: ContextArgument | undefined): Scalar {
  const value = arg?.value();
  if (value === undefined) return "";
  return Array.isArray(value) ? (value[0]?.[0] ?? "") : value;
}

/** A skipped optional argument arrives as an empty string. */
export function isSkipped(arg: ContextArgument | undefined): boolean {
  return arg === undefined || scalarOf(arg) === "";
}

/** A whole-number argument, truncated toward zero like Excel does. */
export function integerArg(arg: ContextArgument): number | FormulaError {
  const value = toNumber(scalarOf(arg));
  return isError(value) ? value : Math.trunc(value);
}

/**
 * The location an argument denotes. A value that is not one is `#VALUE!`; an
 * error value (or an unresolvable reference) passes through.
 */
export function locationOf(arg: ContextArgument): CellReference | FormulaError {
  const reference = arg.reference();
  if (reference !== null) return reference;
  const value = scalarOf(arg);
  return isError(value) ? value : ERR.value();
}
