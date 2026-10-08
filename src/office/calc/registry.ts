/**
 * The function registry.
 *
 * Function implementations live in themed modules under `./functions`; each one
 * calls `registerFunction` at import time and `registerBuiltinFunctions` pulls
 * them all in. `formula.ts` stays the parser/evaluator and does not know how
 * any individual function works.
 */
import type { CellAddress } from "./addresses";
import type { CellReference } from "./references";
import { ERR, type FormulaError } from "./scalars";
import type { CellMatrix, Scalar } from "./scalars";

/** Arguments arrive already grouped per parameter, each a 2D matrix. */
export type FunctionArgs = Scalar[][][];
/** A function returns a scalar, or a matrix for the dynamic-array family. */
export type FunctionResult = Scalar | CellMatrix;
export type FunctionImplementation = (args: FunctionArgs) => FunctionResult;

/**
 * One argument of a context function. Where a plain function receives the
 * evaluated matrix, these keep the argument unevaluated so a function can ask
 * for the *reference* an expression denotes (`OFFSET(A1, ...)`) or for its
 * value, and only pays for the one it uses.
 */
export interface ContextArgument {
  /**
   * The location the argument denotes. `null` when it is an ordinary value, an
   * error when it names a reference that cannot be resolved (unknown sheet).
   */
  reference(): CellReference | FormulaError | null;
  /** The argument evaluated to values: a scalar, or a matrix for a range. */
  value(): Scalar | CellMatrix;
}

/** What the evaluator lends a context function besides its arguments. */
export interface FunctionHost {
  currentSheet: string;
  /** A1 address of the cell holding the formula, when the caller knows it. */
  currentAddress: string | null;
  sheetNames: string[];
  /** Resolves reference text as INDIRECT reads it: `B2`, `Data!A1:B3`, a name. */
  parseReference(text: string): CellReference | FormulaError;
  /** The values a reference covers: a matrix, bounded by the range guard. */
  read(reference: CellReference): CellMatrix | FormulaError;
  /**
   * Formula text of a cell with its leading `=`; null for a constant or an
   * empty cell, and when the caller does not expose formulas.
   */
  formulaAt(sheet: string | null, address: CellAddress): string | null;
  /**
   * Why a 0-based row is out of sight: an AutoFilter hid it, or it was hidden
   * by hand. Null when it shows, and when the caller exposes no row layout.
   */
  hiddenRow(sheet: string | null, row: number): "filtered" | "hidden" | null;
}

/** A context function may answer with a location; the evaluator reads it. */
export type ContextImplementation = (args: ContextArgument[], host: FunctionHost) => FunctionResult | CellReference;

export interface FunctionSpec {
  fn: FunctionImplementation;
  /**
   * Set for functions that need references or the evaluation context (OFFSET,
   * INDIRECT, CELL, INFO). The evaluator calls this instead of `fn`.
   */
  contextFn?: ContextImplementation;
  min: number;
  max: number;
  /** Functions that inspect errors themselves (IFERROR, ISERROR, ...). */
  acceptsErrors?: boolean;
  /**
   * Counting functions, which skip error cells inside a range instead of
   * failing on them - the same exception Excel makes for COUNT/COUNTIF.
   */
  ignoresRangeErrors?: boolean;
  /** Extra help shown by the function picker, keyed by language-neutral text. */
  signature?: string;
  category?: string;
}

const FUNCTIONS = new Map<string, FunctionSpec>();

export interface FunctionMeta {
  signature?: string;
  category?: string;
  /** Opt out of the "an error inside a range poisons the result" rule. */
  ignoresRangeErrors?: boolean;
}

export function registerFunction(
  name: string,
  fn: FunctionImplementation,
  min = 0,
  max = 32,
  acceptsErrors = false,
  meta: FunctionMeta = {},
): void {
  FUNCTIONS.set(name.toUpperCase(), { fn, min, max, acceptsErrors, ...meta });
}

/**
 * Registers a function that works on references, not on the values they hold.
 * `fn` stays callable for code that bypasses the evaluator and reports
 * `#VALUE!`, so a spec is never half-formed.
 */
export function registerContextFunction(
  name: string,
  contextFn: ContextImplementation,
  min = 0,
  max = 32,
  meta: FunctionMeta = {},
): void {
  FUNCTIONS.set(name.toUpperCase(), { fn: () => ERR.value(), contextFn, min, max, acceptsErrors: true, ...meta });
}

export function lookupFunction(name: string): FunctionSpec | undefined {
  return FUNCTIONS.get(name.toUpperCase());
}

export function functionNames(): string[] {
  return [...FUNCTIONS.keys()].sort();
}

export function functionCount(): number {
  return FUNCTIONS.size;
}

export function functionCatalogue(): Array<{ name: string; signature: string; category: string }> {
  return [...FUNCTIONS.entries()]
    .map(([name, spec]) => ({ name, signature: spec.signature ?? `${name}()`, category: spec.category ?? "Other" }))
    .sort((a, b) => a.name.localeCompare(b.name));
}
