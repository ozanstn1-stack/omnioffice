/**
 * Value types and coercion rules for the Calc formula engine.
 *
 * A cell or expression is a `Scalar`: a number, a string, a boolean or a
 * `FormulaError`. Ranges evaluate to a `CellMatrix` (a 2D array of scalars).
 * Every function implementation converts its arguments through the helpers
 * here, so `=SUM("12")` and `=SUM(12)` agree with each other and with the
 * spreadsheet formats.
 */

export type Scalar = number | string | boolean | FormulaError;
export type CellMatrix = Scalar[][];

export class FormulaError {
  constructor(
    readonly code: string,
    readonly detail = "",
  ) {}

  toString(): string {
    return this.code;
  }
}

/** The seven spreadsheet error values, plus the circular-reference guard. */
export const ERR = {
  name: () => new FormulaError("#NAME?"),
  value: () => new FormulaError("#VALUE!"),
  ref: () => new FormulaError("#REF!"),
  div: () => new FormulaError("#DIV/0!"),
  na: () => new FormulaError("#N/A"),
  num: () => new FormulaError("#NUM!"),
  // Cycles are reported as #REF! (what Excel and LibreOffice both display) with
  // a detail that only the UI surfaces.
  circular: () => new FormulaError("#REF!", "circular reference"),
  // A dynamic array whose spill range is blocked by existing data.
  spill: () => new FormulaError("#SPILL!", "the spill range is not empty"),
};

export function isError(value: unknown): value is FormulaError {
  return value instanceof FormulaError;
}

export function toNumber(value: Scalar): number | FormulaError {
  if (isError(value)) return value;
  if (typeof value === "number") return value;
  if (typeof value === "boolean") return value ? 1 : 0;
  const text = String(value).trim();
  if (text === "") return 0;
  // Accept both decimal separators: a Turkish locale types 3,5.
  const parsed = Number(text.replace(/,(?=\d)/g, ".").replace(/,/g, ""));
  return Number.isFinite(parsed) ? parsed : ERR.value();
}

export function toText(value: Scalar): string {
  if (isError(value)) return value.code;
  if (typeof value === "boolean") return value ? "TRUE" : "FALSE";
  if (typeof value === "number") return formatPlainNumber(value);
  return value;
}

export function formatPlainNumber(value: number): string {
  if (Number.isInteger(value)) return String(value);
  return String(Number(value.toFixed(10)));
}

export function toBool(value: Scalar): boolean | FormulaError {
  if (isError(value)) return value;
  if (typeof value === "boolean") return value;
  if (typeof value === "number") return value !== 0;
  const text = String(value).trim().toUpperCase();
  if (text === "TRUE") return true;
  if (text === "FALSE") return false;
  return ERR.value();
}

/**
 * Reads an optional boolean argument such as `UNIQUE(A1:A9,,TRUE)`.
 *
 * An omitted argument arrives as an empty string, which must read as "not
 * supplied" rather than as a text value that fails to convert.
 */
export function optionalBool(value: Scalar | undefined, fallback = false): boolean | FormulaError {
  if (value === undefined || value === "") return fallback;
  return toBool(value);
}

/** First scalar of a matrix, or `undefined` when the matrix is empty. */
export function scalarOf(matrix: Scalar | CellMatrix | undefined): Scalar | undefined {
  if (matrix === undefined) return undefined;
  if (!Array.isArray(matrix)) return matrix;
  return matrix[0]?.[0];
}

export function flatten(args: Scalar[][][]): Scalar[] {
  const out: Scalar[] = [];
  for (const arg of args) for (const row of arg) out.push(...row);
  return out;
}

/** Numeric values only, skipping blanks, text and errors. */
export function numbers(args: Scalar[][][]): number[] {
  const out: number[] = [];
  for (const value of flatten(args)) {
    if (isError(value)) continue;
    if (typeof value === "number") out.push(value);
    else if (typeof value === "string" && value.trim() !== "" && Number.isFinite(Number(value))) out.push(Number(value));
  }
  return out;
}

/** Excel-like ordering: numbers before text, text case-insensitive. */
export function compareScalars(a: Scalar, b: Scalar): number {
  const numberA = typeof a === "string" && a.trim() !== "" && Number.isFinite(Number(a)) ? Number(a) : typeof a === "boolean" ? (a ? 1 : 0) : a;
  const numberB = typeof b === "string" && b.trim() !== "" && Number.isFinite(Number(b)) ? Number(b) : typeof b === "boolean" ? (b ? 1 : 0) : b;
  if (typeof numberA === "number" && typeof numberB === "number") return numberA - numberB;
  const textA = String(numberA).toUpperCase();
  const textB = String(numberB).toUpperCase();
  return textA < textB ? -1 : textA > textB ? 1 : 0;
}

/**
 * Builds a predicate from a `*IF*` criteria value.
 *
 * Supports the comparison prefixes Excel accepts (`">=10"`, `"<>x"`, `"=y"`)
 * and plain equality, and falls back to case-insensitive text comparison when
 * the criterion is not a number.
 */
export function criteriaMatcher(criteria: Scalar): (value: Scalar) => boolean {
  const text = String(isError(criteria) ? criteria.code : (criteria ?? ""));
  const match = /^(<=|>=|<>|=|<|>)(.*)$/.exec(text);
  const operator = match?.[1] ?? "=";
  const operand = match ? match[2] : text;
  const operandNumber = Number(operand);
  const isNumeric = operand.trim() !== "" && Number.isFinite(operandNumber);
  // `*` and `?` are wildcards for text criteria in Excel; `~` escapes the next
  // wildcard. Only build the regex when the operand actually contains one, so
  // plain text keeps the exact (case-insensitive) comparison.
  const hasWildcard = !isNumeric && /[*?]/.test(operand);
  const wildcard = hasWildcard ? wildcardRegex(operand) : null;
  return (value: Scalar) => {
    if (isError(value)) return false;
    const valueNumber = typeof value === "number" ? value : typeof value === "boolean" ? (value ? 1 : 0) : Number(String(value));
    const numeric = isNumeric && Number.isFinite(valueNumber);
    if (wildcard && (operator === "=" || operator === "<>")) {
      const isMatch = wildcard.test(String(value));
      return operator === "=" ? isMatch : !isMatch;
    }
    switch (operator) {
      case "=":
        return numeric ? valueNumber === operandNumber : String(value).toUpperCase() === operand.toUpperCase();
      case "<>":
        return numeric ? valueNumber !== operandNumber : String(value).toUpperCase() !== operand.toUpperCase();
      case "<":
        return numeric ? valueNumber < operandNumber : compareScalars(value, operand) < 0;
      case ">":
        return numeric ? valueNumber > operandNumber : compareScalars(value, operand) > 0;
      case "<=":
        return numeric ? valueNumber <= operandNumber : compareScalars(value, operand) <= 0;
      case ">=":
        return numeric ? valueNumber >= operandNumber : compareScalars(value, operand) >= 0;
      default:
        return false;
    }
  };
}

/** Compiles an Excel wildcard pattern (`*`, `?`, `~` escape) to a RegExp. */
function wildcardRegex(pattern: string): RegExp {
  let out = "^";
  for (let index = 0; index < pattern.length; index += 1) {
    const ch = pattern[index];
    if (ch === "~") {
      const next = pattern[index + 1];
      if (next === "*" || next === "?" || next === "~") {
        out += escapeRegex(next);
        index += 1;
        continue;
      }
      out += "~";
      continue;
    }
    if (ch === "*") out += ".*";
    else if (ch === "?") out += ".";
    else out += escapeRegex(ch);
  }
  return new RegExp(`${out}$`, "i");
}

function escapeRegex(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/** Pads ragged matrices to a common width so element-wise ops line up. */
export function padMatrix(args: Scalar[][][]): Scalar[][][] {
  const matrices = args.map((arg) => (arg.length > 0 ? arg : [["" as Scalar]]));
  const width = Math.max(1, ...matrices.map((matrix) => matrix[0]?.length ?? 1));
  return matrices.map((matrix) => matrix.map((row) => (row.length >= width ? row : [...row, ...(Array(width - row.length).fill("") as Scalar[])])));
}

/** A rectangular, error-free numeric view of a matrix. */
export function numericGrid(matrix: CellMatrix): number[][] {
  return matrix.map((row) =>
    row.map((value) => {
      const number = toNumber(value);
      return isError(number) ? Number.NaN : number;
    }),
  );
}
