/**
 * The Calc formula engine: tokenizer, parser and evaluator.
 *
 * The evaluator is intentionally eager - every argument is computed before the
 * function runs - with one exception, `LET`, which needs lazy bindings so a
 * name can be used before it is bound. Errors are values here, not exceptions,
 * so a failure deep inside a range surfaces in the cell that asked for it.
 *
 * Public surface is re-exported from the modules it used to live in, so callers
 * (`cells.ts`, the grid, the tests) keep importing from `./formula`.
 */
import { MAX_COLS, MAX_ROWS, addressesInRange, columnLabel, columnIndex, formatAddress, parseAddress, parseRange, rangeSize, type CellAddress, type RangeParts } from "./addresses";
import { formatNumber, formatPlainNumber, dateToSerial, serialToDate } from "./numberFormat";
import { lookupFunction, registerFunction, functionCount, functionNames, functionCatalogue, type FunctionArgs, type FunctionResult } from "./registry";
import { ERR, FormulaError, compareScalars, criteriaMatcher, flatten, isError, numericGrid, padMatrix, scalarOf, toBool, toNumber, toText, type CellMatrix, type Scalar } from "./scalars";
import { registerBuiltinFunctions } from "./functions";
import { resolveStructuredReference, tableByName } from "./structured";
import type { SpreadsheetTable } from "../../lib/office-types";

registerBuiltinFunctions();

export {
  ERR,
  FormulaError,
  addressesInRange,
  asScalar,
  columnIndex,
  columnLabel,
  compareScalars,
  criteriaMatcher,
  dateToSerial,
  flatten,
  formatAddress,
  formatNumber,
  formatPlainNumber,
  functionCatalogue,
  functionCount,
  functionNames,
  isError,
  lookupFunction,
  numericGrid,
  padMatrix,
  parseAddress,
  parseRange,
  rangeSize,
  registerFunction,
  scalarOf,
  serialToDate,
  toBool,
  toNumber,
  toText,
};
export type { CellAddress, CellMatrix, FunctionArgs, FunctionResult, RangeParts, Scalar };
export { MAX_COLS, MAX_ROWS };

// ---------------------------------------------------------------------------
// Tokenizer
// ---------------------------------------------------------------------------

const OPERATORS = ["<>", "<=", ">=", "=", "<", ">", "+", "-", "*", "/", "^", "&", "%"];

interface Token {
  type: "number" | "string" | "ref" | "range" | "ident" | "name" | "op" | "lparen" | "rparen" | "lbrace" | "rbrace" | "comma" | "semicolon" | "bool" | "error";
  value: string;
  sheet?: string | null;
}

function tokenize(input: string): Token[] {
  const tokens: Token[] = [];
  let index = 0;
  while (index < input.length) {
    const ch = input[index];
    if (ch === " " || ch === "\t" || ch === "\n") {
      index += 1;
      continue;
    }
    if (ch === '"') {
      let value = "";
      index += 1;
      while (index < input.length) {
        if (input[index] === '"') {
          if (input[index + 1] === '"') {
            value += '"';
            index += 2;
            continue;
          }
          index += 1;
          break;
        }
        value += input[index];
        index += 1;
      }
      tokens.push({ type: "string", value });
      continue;
    }
    if (ch === "{") {
      tokens.push({ type: "lbrace", value: ch });
      index += 1;
      continue;
    }
    if (ch === "}") {
      tokens.push({ type: "rbrace", value: ch });
      index += 1;
      continue;
    }
    if (/[0-9]/.test(ch) || (ch === "." && /[0-9]/.test(input[index + 1] ?? ""))) {
      let value = "";
      while (index < input.length && /[0-9.eE+-]/.test(input[index])) {
        // Stop a trailing +/- that is an operator, not an exponent sign.
        if ((input[index] === "+" || input[index] === "-") && !/[eE]$/.test(value)) break;
        value += input[index];
        index += 1;
      }
      tokens.push({ type: "number", value });
      continue;
    }
    if (ch === "(") {
      tokens.push({ type: "lparen", value: ch });
      index += 1;
      continue;
    }
    if (ch === ")") {
      tokens.push({ type: "rparen", value: ch });
      index += 1;
      continue;
    }
    if (ch === ",") {
      tokens.push({ type: "comma", value: "," });
      index += 1;
      continue;
    }
    if (ch === ";") {
      // Inside an array literal `;` starts a new row; inside a call it is an
      // argument separator (European locales use it instead of a comma).
      tokens.push({ type: "semicolon", value: ";" });
      index += 1;
      continue;
    }
    if (ch === "'") {
      // Quoted sheet name: 'My Sheet'!A1
      let sheet = "";
      index += 1;
      while (index < input.length && input[index] !== "'") {
        sheet += input[index];
        index += 1;
      }
      index += 1;
      if (input[index] === "!") index += 1;
      const reference = readReference(input, index);
      if (reference) {
        tokens.push({ type: reference.range ? "range" : "ref", value: reference.address, sheet });
        index = reference.next;
        continue;
      }
      tokens.push({ type: "name", value: sheet });
      continue;
    }
    if (/[A-Za-z_$]/.test(ch)) {
      const start = index;
      while (index < input.length && /[A-Za-z0-9_$.]/.test(input[index])) index += 1;
      let word = input.slice(start, index);
      let sheet: string | null = null;
      if (input[index] === "!") {
        sheet = word;
        index += 1;
        const reference = readReference(input, index);
        if (reference) {
          tokens.push({ type: reference.range ? "range" : "ref", value: reference.address, sheet });
          index = reference.next;
          continue;
        }
      }
      const upper = word.toUpperCase();
      if (upper === "TRUE" || upper === "FALSE") {
        tokens.push({ type: "bool", value: upper });
        continue;
      }
      if (/^#(REF|VALUE|NAME|DIV\/0|N\/A|NUM|CIRC|SPILL)[!?]$/i.test(upper)) {
        tokens.push({ type: "error", value: upper });
        continue;
      }
      // A function call is an identifier immediately followed by "(".
      const afterWord = word;
      if (input[index] === "(") {
        tokens.push({ type: "ident", value: afterWord.toUpperCase() });
        continue;
      }
      // A structured table reference (`Sales[Amount]`, `Sales[@[Net]]`) is one
      // token; the bracket body may nest and contain spaces.
      if (input[index] === "[") {
        const brackets = readStructuredBrackets(input, index);
        if (brackets) {
          tokens.push({ type: "name", value: word + brackets });
          index += brackets.length;
          continue;
        }
      }
      // Bare cell reference?
      const reference = readReference(input, start);
      if (reference && (reference.range || /^\$?[A-Za-z]+\$?\d+$/.test(reference.address))) {
        tokens.push({ type: reference.range ? "range" : "ref", value: reference.address, sheet: null });
        index = reference.next;
        continue;
      }
      // Anything else is a defined name (or an unknown name, resolved later).
      word = input.slice(start, index);
      tokens.push({ type: "name", value: word });
      continue;
    }
    const doubleOperator = OPERATORS.find((operator) => operator.length === 2 && input.startsWith(operator, index));
    if (doubleOperator) {
      tokens.push({ type: "op", value: doubleOperator });
      index += 2;
      continue;
    }
    const singleOperator = OPERATORS.find((operator) => operator.length === 1 && input.startsWith(operator, index));
    if (singleOperator) {
      tokens.push({ type: "op", value: singleOperator });
      index += 1;
      continue;
    }
    // Unknown character: treat as a name error.
    tokens.push({ type: "error", value: "#NAME?" });
    index += 1;
  }
  return tokens;
}

function readReference(input: string, index: number): { address: string; range: boolean; next: number } | null {
  const pattern = /^\$?[A-Za-z]{1,3}\$?\d{1,7}/;
  const match = pattern.exec(input.slice(index));
  if (!match) return null;
  const start = index;
  let end = index + match[0].length;
  let range = false;
  if (input[end] === ":") {
    const second = pattern.exec(input.slice(end + 1));
    if (second) {
      end = end + 1 + second[0].length;
      range = true;
    }
  }
  void start;
  // Excel references are case-insensitive: `a1` has to read the same cell as
  // `A1`, so the address is normalised here rather than silently missing the
  // value map later.
  return { address: input.slice(index, end).replace(/\$/g, "").toUpperCase(), range, next: end };
}

/**
 * Consumes a balanced `[...]` body starting at `index`.
 *
 * Structured references nest brackets (`Sales[@[Net]]`), so a plain regex
 * cannot find where the reference ends.
 */
function readStructuredBrackets(input: string, index: number): string | null {
  if (input[index] !== "[") return null;
  let depth = 0;
  for (let at = index; at < input.length; at += 1) {
    if (input[at] === "[") depth += 1;
    else if (input[at] === "]") {
      depth -= 1;
      if (depth === 0) return input.slice(index, at + 1);
    }
  }
  return null;
}

/** Functions whose value can change without any cell being edited. */
const VOLATILE_FUNCTIONS = new Set(["TODAY", "NOW", "RAND", "RANDBETWEEN", "OFFSET", "INDIRECT", "CELL", "INFO"]);

/**
 * The cells a formula reads, extracted without evaluating it.
 *
 * Used by the dependency graph so an edit only recalculates the formulas that
 * can actually change. `null` means the reference set could not be read
 * confidently; the caller treats such a formula as depending on everything.
 */
export interface FormulaReferenceSummary {
  refs: Array<{ sheet: string | null; address: string }>;
  ranges: Array<{ sheet: string | null; range: string }>;
  names: string[];
  volatile: boolean;
}

export function collectReferences(formula: string): FormulaReferenceSummary | null {
  const tokens = tokenize(formula.trim().replace(/^=/, ""));
  const summary: FormulaReferenceSummary = { refs: [], ranges: [], names: [], volatile: false };
  for (const token of tokens) {
    if (token.type === "ref") {
      const address = parseAddress(token.value);
      if (!address) return null;
      summary.refs.push({ sheet: token.sheet ?? null, address: formatAddress(address.row, address.col) });
    } else if (token.type === "range") {
      if (!parseRange(token.value)) return null;
      summary.ranges.push({ sheet: token.sheet ?? null, range: token.value.toUpperCase() });
    } else if (token.type === "name") {
      // `Sales[Amount]` is a structured reference, not a defined name; the
      // dependency graph resolves it against the sheet's tables instead.
      if (!token.value.includes("[")) summary.names.push(token.value.toUpperCase());
    } else if (token.type === "ident" && VOLATILE_FUNCTIONS.has(token.value)) {
      summary.volatile = true;
    }
  }
  return summary;
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

export type Node =
  | { type: "number"; value: number }
  | { type: "string"; value: string }
  | { type: "bool"; value: boolean }
  | { type: "error"; value: string }
  | { type: "ref"; address: string; sheet: string | null }
  | { type: "range"; range: string; sheet: string | null }
  | { type: "array"; rows: Node[][] }
  | { type: "name"; name: string }
  | { type: "unary"; op: string; operand: Node }
  | { type: "binary"; op: string; left: Node; right: Node }
  | { type: "percent"; operand: Node }
  | { type: "call"; name: string; args: Node[] };

class Parser {
  private position = 0;

  constructor(private tokens: Token[]) {}

  parse(): Node {
    return this.parseComparison();
  }

  peek(): Token | undefined {
    return this.tokens[this.position];
  }

  private next(): Token | undefined {
    return this.tokens[this.position++];
  }

  private parseComparison(): Node {
    let left = this.parseConcat();
    while (this.peek()?.type === "op" && ["=", "<>", "<", ">", "<=", ">="].includes(this.peek()!.value)) {
      const op = this.next()!.value;
      const right = this.parseConcat();
      left = { type: "binary", op, left, right };
    }
    return left;
  }

  private parseConcat(): Node {
    let left = this.parseAdditive();
    while (this.peek()?.type === "op" && this.peek()!.value === "&") {
      this.next();
      const right = this.parseAdditive();
      left = { type: "binary", op: "&", left, right };
    }
    return left;
  }

  private parseAdditive(): Node {
    let left = this.parseMultiplicative();
    while (this.peek()?.type === "op" && ["+", "-"].includes(this.peek()!.value)) {
      const op = this.next()!.value;
      const right = this.parseMultiplicative();
      left = { type: "binary", op, left, right };
    }
    return left;
  }

  private parseMultiplicative(): Node {
    let left = this.parsePower();
    while (this.peek()?.type === "op" && ["*", "/"].includes(this.peek()!.value)) {
      const op = this.next()!.value;
      const right = this.parsePower();
      left = { type: "binary", op, left, right };
    }
    return left;
  }

  private parsePower(): Node {
    const left = this.parseUnary();
    if (this.peek()?.type === "op" && this.peek()!.value === "^") {
      this.next();
      const right = this.parsePower();
      return { type: "binary", op: "^", left, right };
    }
    return left;
  }

  private parseUnary(): Node {
    const token = this.peek();
    if (token?.type === "op" && (token.value === "-" || token.value === "+")) {
      this.next();
      const operand = this.parseUnary();
      return { type: "unary", op: token.value, operand };
    }
    return this.parsePostfix();
  }

  private parsePostfix(): Node {
    let node = this.parsePrimary();
    while (this.peek()?.type === "op" && this.peek()!.value === "%") {
      this.next();
      node = { type: "percent", operand: node };
    }
    return node;
  }

  private parsePrimary(): Node {
    const token = this.next();
    if (!token) return { type: "error", value: "#VALUE!" };
    switch (token.type) {
      case "number": {
        const value = Number(token.value);
        return Number.isFinite(value) ? { type: "number", value } : { type: "error", value: "#VALUE!" };
      }
      case "string":
        return { type: "string", value: token.value };
      case "bool":
        return { type: "bool", value: token.value === "TRUE" };
      case "error":
        return { type: "error", value: token.value };
      case "ref":
        return { type: "ref", address: token.value, sheet: token.sheet ?? null };
      case "range":
        return { type: "range", range: token.value, sheet: token.sheet ?? null };
      case "name":
        return { type: "name", name: token.value };
      case "lbrace":
        return this.parseArrayLiteral();
      case "ident": {
        if (this.peek()?.type === "lparen") {
          this.next();
          const args: Node[] = [];
          if (this.peek()?.type !== "rparen") {
            args.push(this.parseComparison());
            while (this.peek()?.type === "comma" || this.peek()?.type === "semicolon") {
              this.next();
              // `UNIQUE(A1:A9,,TRUE)` skips an optional argument; an empty slot
              // is an empty string, not a syntax error.
              const next = this.peek();
              if (next?.type === "comma" || next?.type === "semicolon" || next?.type === "rparen") {
                args.push({ type: "string", value: "" });
                continue;
              }
              args.push(this.parseComparison());
            }
          }
          if (this.peek()?.type === "rparen") this.next();
          return { type: "call", name: token.value, args };
        }
        return { type: "name", name: token.value };
      }
      case "lparen": {
        const inner = this.parseComparison();
        if (this.peek()?.type === "rparen") this.next();
        return inner;
      }
      default:
        return { type: "error", value: "#VALUE!" };
    }
  }

  /**
   * Parses an inline array literal: `{1,2;3,4}`.
   *
   * `,` separates columns and `;` separates rows, as in Excel. Rows are padded
   * to a common width so the result is always a well-formed matrix.
   */
  private parseArrayLiteral(): Node {
    const rows: Node[][] = [];
    let row: Node[] = [];
    while (this.peek() && this.peek()!.type !== "rbrace") {
      row.push(this.parseComparison());
      const next = this.peek();
      if (next?.type === "comma") {
        this.next();
        continue;
      }
      if (next?.type === "semicolon") {
        // Row break.
        this.next();
        rows.push(row);
        row = [];
        continue;
      }
      if (next?.type === "rbrace") break;
      // Nothing usable to separate on: the literal is malformed.
      while (this.peek() && this.peek()!.type !== "rbrace") this.next();
      break;
    }
    if (row.length > 0) rows.push(row);
    if (this.peek()?.type === "rbrace") this.next();
    return { type: "array", rows };
  }
}

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

export interface NamedRange {
  /** The raw definition: a range, a reference, a constant or a formula. */
  definition: string;
  /** Sheet the name is scoped to, or null for a workbook-level name. */
  sheet?: string | null;
}

export interface FormulaContext {
  getValue: (sheet: string | null, address: string) => Scalar;
  sheetNames: string[];
  /** Current sheet name ("" when single-sheet). */
  currentSheet: string;
  /** Guard against runaway ranges. */
  maxRangeCells?: number;
  /** Defined names, keyed by upper-case name. */
  names?: Record<string, NamedRange | string>;
  /** Structured tables visible from the formula's sheet. */
  tables?: SpreadsheetTable[];
  /** 1-based row of the formula's own cell, for `@` this-row references. */
  currentRow?: number;
}

interface EvalState {
  context: FormulaContext;
  /** LET bindings for the current expression. */
  bindings: Map<string, Scalar | CellMatrix>;
  /** Names currently being resolved, to stop a self-referential name. */
  resolving: Set<string>;
}

function asScalar(value: Scalar | CellMatrix): Scalar {
  return Array.isArray(value) ? (value[0]?.[0] ?? "") : value;
}

/** The first error cell inside a matrix, or null when the matrix is clean. */
function firstErrorIn(matrix: CellMatrix): FormulaError | null {
  for (const row of matrix) {
    for (const value of row) {
      if (isError(value)) return value;
    }
  }
  return null;
}

/** Resolves a defined name by evaluating its definition in the same context. */
function resolveName(name: string, state: EvalState): Scalar | CellMatrix {
  const key = name.toUpperCase();
  const bound = state.bindings.get(key);
  if (bound !== undefined) return bound;
  const entry = state.context.names?.[key];
  if (entry === undefined) return ERR.name();
  if (state.resolving.has(key)) return ERR.circular();
  const definition = typeof entry === "string" ? entry : entry.definition;
  if (!definition || definition.trim() === "") return ERR.name();
  state.resolving.add(key);
  let result: Scalar | CellMatrix;
  try {
    result = evaluateWithState(definition.startsWith("=") ? definition.slice(1) : definition, state);
  } catch {
    result = ERR.name();
  } finally {
    state.resolving.delete(key);
  }
  // A name that resolves to itself once (directly or through a cycle) is an
  // error, not a value.
  return result;
}

/**
 * Resolves one structured reference such as `Sales[Amount]`.
 *
 * A declared table that cannot answer the reference is `#REF!`; a reference to
 * a table that does not exist at all is `#NAME?`, matching Excel.
 */
function evaluateStructuredReference(reference: string, state: EvalState): Scalar | CellMatrix {
  const resolved = resolveStructuredReference(reference, state.context.tables, state.context.currentRow);
  if (resolved) return evaluateRange({ type: "range", range: resolved, sheet: null }, state);
  const bracket = reference.indexOf("[");
  const tableName = bracket > 0 ? reference.slice(0, bracket) : reference;
  return tableByName(state.context.tables, tableName) ? ERR.ref() : ERR.name();
}

/**
 * Resolves the sheet part of a reference to the workbook's own spelling, so
 * `data!A1` finds the sheet named `Data`. `null` means "current sheet";
 * `undefined` means no sheet with that name exists.
 */
function resolveSheetName(sheet: string | null, context: FormulaContext): string | null | undefined {
  if (!sheet) return null;
  if (context.sheetNames.includes(sheet)) return sheet;
  return context.sheetNames.find((name) => name.toLowerCase() === sheet.toLowerCase());
}

function evaluateRange(node: Node, state: EvalState): CellMatrix | Scalar {
  const context = state.context;
  const node2 = node as Extract<Node, { type: "range" }>;
  const sheet = resolveSheetName(node2.sheet ?? null, context);
  if (sheet === undefined) return ERR.ref();
  const parts = parseRange(node2.range);
  if (!parts) return ERR.ref();
  const limit = context.maxRangeCells ?? 200_000;
  if ((parts.end.row - parts.start.row + 1) * (parts.end.col - parts.start.col + 1) > limit) return ERR.ref();
  const matrix: CellMatrix = [];
  for (let row = parts.start.row; row <= parts.end.row; row += 1) {
    const line: Scalar[] = [];
    for (let col = parts.start.col; col <= parts.end.col; col += 1) {
      line.push(context.getValue(sheet, formatAddress(row, col)));
    }
    matrix.push(line);
  }
  return matrix;
}

/** Hard ceiling on an inline literal, so `{1,1,1,...}` cannot exhaust memory. */
const MAX_LITERAL_CELLS = 20_000;

function evaluateArrayLiteral(node: Extract<Node, { type: "array" }>, state: EvalState): CellMatrix | Scalar {
  const rows: CellMatrix = node.rows.map((row) => row.map((cell) => asScalar(evaluateNode(cell, state))));
  if (rows.length === 0) return ERR.value();
  const width = Math.max(...rows.map((row) => row.length));
  if (rows.length * width > MAX_LITERAL_CELLS) return ERR.num();
  return rows.map((row) => {
    const line = [...row];
    while (line.length < width) line.push("");
    return line;
  });
}

function evaluateNode(node: Node, state: EvalState): Scalar | CellMatrix {
  switch (node.type) {
    case "number":
      return node.value;
    case "string":
      return node.value;
    case "bool":
      return node.value;
    case "error":
      return new FormulaError(node.value);
    case "name": {
      const key = node.name.toUpperCase();
      // Not a function, cell or defined name: a bracketed name is a structured
      // table reference and must resolve against the sheet's tables.
      if (node.name.includes("[") && !state.bindings.has(key) && state.context.names?.[key] === undefined) {
        return evaluateStructuredReference(node.name, state);
      }
      return resolveName(node.name, state);
    }
    case "ref": {
      const sheet = resolveSheetName(node.sheet ?? null, state.context);
      if (sheet === undefined) return ERR.ref();
      // A token that looks like a reference but is not a valid A1 address
      // (`A0`, `AAAA1`) must fail loudly; returning an empty value here used to
      // make `=A0+1` silently evaluate to 1.
      if (!parseAddress(node.address)) return ERR.ref();
      return state.context.getValue(sheet, node.address);
    }
    case "range":
      return evaluateRange(node, state);
    case "array":
      return evaluateArrayLiteral(node, state);
    case "unary": {
      const operand = asScalar(evaluateNode(node.operand, state));
      if (isError(operand)) return operand;
      const number = toNumber(operand);
      if (isError(number)) return number;
      return node.op === "-" ? -number : number;
    }
    case "percent": {
      const operand = asScalar(evaluateNode(node.operand, state));
      if (isError(operand)) return operand;
      const number = toNumber(operand);
      return isError(number) ? number : number / 100;
    }
    case "binary":
      return evaluateBinary(node, state);
    case "call":
      return evaluateCall(node, state);
    default:
      return ERR.name();
  }
}

/**
 * Applies one binary operator to one pair of scalars.
 *
 * Split out of `evaluateBinary` so array broadcasting can run the same rules
 * cell by cell - `A1:A3>1` has to produce a column of booleans, which is what
 * FILTER and the other dynamic-array functions expect.
 */
function applyBinary(op: string, left: Scalar, right: Scalar): Scalar {
  if (isError(left)) return left;
  if (isError(right)) return right;

  if (op === "&") return toText(left) + toText(right);
  if (["=", "<>", "<", ">", "<=", ">="].includes(op)) {
    const comparison = compareScalars(left, right);
    switch (op) {
      case "=":
        return comparison === 0;
      case "<>":
        return comparison !== 0;
      case "<":
        return comparison < 0;
      case ">":
        return comparison > 0;
      case "<=":
        return comparison <= 0;
      default:
        return comparison >= 0;
    }
  }

  const a = toNumber(left);
  if (isError(a)) return a;
  const b = toNumber(right);
  if (isError(b)) return b;
  switch (op) {
    case "+":
      return a + b;
    case "-":
      return a - b;
    case "*":
      return a * b;
    case "/":
      return b === 0 ? ERR.div() : a / b;
    case "^": {
      const result = a ** b;
      return Number.isFinite(result) ? result : ERR.num();
    }
    default:
      return ERR.value();
  }
}

/** Element-wise operator over one or two matrices, padding to a common shape. */
function broadcastBinary(op: string, left: CellMatrix, right: CellMatrix): CellMatrix {
  const height = Math.max(left.length, right.length);
  const width = Math.max(1, ...left.map((row) => row.length), ...right.map((row) => row.length));
  const out: CellMatrix = [];
  for (let row = 0; row < height; row += 1) {
    const line: Scalar[] = [];
    for (let col = 0; col < width; col += 1) {
      line.push(applyBinary(op, left[row]?.[col] ?? "", right[row]?.[col] ?? ""));
    }
    out.push(line);
  }
  return out;
}

function evaluateBinary(node: Extract<Node, { type: "binary" }>, state: EvalState): Scalar | CellMatrix {
  const left = evaluateNode(node.left, state);
  const right = evaluateNode(node.right, state);
  if (Array.isArray(left) || Array.isArray(right)) {
    return broadcastBinary(node.op, Array.isArray(left) ? left : [[asScalar(left)]], Array.isArray(right) ? right : [[asScalar(right)]]);
  }
  const scalarLeft = asScalar(left);
  const scalarRight = asScalar(right);
  return applyBinary(node.op, scalarLeft, scalarRight);
}

/**
 * `LET(name1, value1, ..., result)` binds names lazily.
 *
 * It is the one function the evaluator cannot treat as eager: each value
 * expression has to see the names bound before it, and the trailing result
 * expression has to see all of them.
 */
function evaluateLet(node: Extract<Node, { type: "call" }>, state: EvalState): Scalar | CellMatrix {
  const args = node.args;
  if (args.length < 3 || args.length % 2 === 0) return ERR.value();
  const bindings = new Map(state.bindings);
  for (let index = 0; index + 2 < args.length; index += 2) {
    const nameNode = args[index];
    if (nameNode.type !== "name") return ERR.name();
    const value = evaluateNode(args[index + 1], { ...state, bindings });
    bindings.set(nameNode.name.toUpperCase(), value);
  }
  return evaluateNode(args[args.length - 1], { ...state, bindings });
}

function evaluateCall(node: Extract<Node, { type: "call" }>, state: EvalState): Scalar | CellMatrix {
  if (node.name === "LET") return evaluateLet(node, state);
  const spec = lookupFunction(node.name);
  if (!spec) return ERR.name();
  if (node.args.length < spec.min || node.args.length > spec.max) return ERR.value();
  const args: FunctionArgs = [];
  for (const argument of node.args) {
    const value = evaluateNode(argument, state);
    const head = asScalar(value);
    if (!spec.acceptsErrors && isError(head)) {
      // A failed range (runaway size, unknown sheet, circular reference) must
      // surface instead of being silently skipped by numeric aggregates.
      return head;
    }
    if (!spec.acceptsErrors && !spec.ignoresRangeErrors && Array.isArray(value)) {
      // Excel's rule: an error *inside* a range poisons SUM/AVERAGE/... but is
      // skipped by the counting family, which opts out via `ignoresRangeErrors`.
      const inner = firstErrorIn(value);
      if (inner) return inner;
    }
    args.push(Array.isArray(value) ? value : [[value]]);
  }
  try {
    return spec.fn(args);
  } catch {
    // A function must never take the whole workbook down with a JS exception.
    return ERR.value();
  }
}

function evaluateWithState(source: string, state: EvalState): Scalar | CellMatrix {
  const tokens = tokenize(source);
  if (tokens.length === 0) return "";
  const parser = new Parser(tokens);
  const node = parser.parse();
  // Trailing junk means the formula was malformed; do not silently drop it.
  if (parser.peek() !== undefined) return ERR.value();
  return evaluateNode(node, state);
}

/** Parses and evaluates a formula body (no leading `=`). */
export function evaluateSource(source: string, context: FormulaContext): Scalar | CellMatrix {
  const state: EvalState = { context, bindings: new Map(), resolving: new Set() };
  try {
    return evaluateWithState(source, state);
  } catch {
    return ERR.value();
  }
}

/**
 * Evaluates a formula string such as `=SUM(A1:A9)` to a single scalar.
 *
 * Array results collapse to their first cell, which is what a single cell can
 * hold; the spill-aware path lives in the workbook evaluator.
 */
export function evaluateFormula(formula: string, context: FormulaContext): Scalar {
  const source = formula.startsWith("=") ? formula.slice(1) : formula;
  return asScalar(evaluateSource(source, context));
}

/**
 * The raw evaluation result: a scalar, or a matrix for the dynamic-array
 * family. `evaluateFormula` keeps the scalar contract for existing callers;
 * the dependency-aware value pass uses this one so a spill can be planned.
 */
export function evaluateFormulaResult(formula: string, context: FormulaContext): Scalar | CellMatrix {
  const source = formula.startsWith("=") ? formula.slice(1) : formula;
  return evaluateSource(source, context);
}

/** Evaluates a formula that may spill into a range. */
export function evaluateToMatrix(formula: string, context: FormulaContext): CellMatrix {
  const source = formula.startsWith("=") ? formula.slice(1) : formula;
  const result = evaluateSource(source, context);
  return Array.isArray(result) ? result : [[result]];
}

export function isFormula(text: string): boolean {
  const trimmed = text.trimStart();
  // "=" alone is a formula the user has not finished typing yet.
  return trimmed.startsWith("=") && trimmed.length > 1;
}

/** Function names starting with `prefix`, for the formula-bar picker. */
export function suggestFunctions(prefix: string, limit = 8): string[] {
  const needle = prefix.trim().toUpperCase();
  if (!needle) return functionNames().slice(0, limit);
  return functionNames()
    .filter((name) => name.startsWith(needle))
    .slice(0, limit);
}

export { parseAddress as parseCellAddress, rangeSize as measureRange };
