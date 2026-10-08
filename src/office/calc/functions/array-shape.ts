/**
 * Array-shaping functions: HSTACK, VSTACK, TAKE, DROP, CHOOSECOLS, CHOOSEROWS,
 * EXPAND, TOCOL, TOROW, WRAPROWS and WRAPCOLS.
 *
 * Like the other dynamic-array functions they return a matrix that the
 * workbook spills. They are registered as accepting errors because an error
 * *inside* an array is just another value to move around (`HSTACK({1,#N/A})`);
 * an error in a count or index argument is returned as the result.
 */
import { registerFunction } from "../registry";
import { ERR, isError, optionalBool, toNumber, type CellMatrix, type FormulaError, type Scalar } from "../scalars";
import { MAX_SPILL_CELLS } from "./arrays";

type Args = Scalar[][][];

function widthOf(matrix: CellMatrix): number {
  return matrix.reduce((widest, row) => Math.max(widest, row.length), 0);
}

/** An empty slot (`TAKE(a,,2)`) or a missing argument reads as "not supplied". */
function isOmitted(arg: Scalar[][] | undefined): boolean {
  return arg === undefined || (arg.length === 1 && arg[0].length === 1 && arg[0][0] === "");
}

/** A whole-number argument, `null` when it was left out. */
function countArg(arg: Scalar[][] | undefined): number | null | FormulaError {
  if (isOmitted(arg)) return null;
  const value = toNumber(arg![0][0]);
  return isError(value) ? value : Math.trunc(value);
}

/** A matrix with nothing in it is `#CALC!`, what Excel shows for an empty array. */
function nonEmpty(matrix: CellMatrix): CellMatrix | FormulaError {
  return matrix.length === 0 || widthOf(matrix) === 0 ? ERR.calc() : matrix;
}

function define(
  name: string,
  signature: string,
  min: number,
  max: number,
  fn: (args: Args) => CellMatrix | FormulaError,
) {
  registerFunction(name, fn, min, max, true, { signature, category: "Array" });
}

// ---------------------------------------------------------------------------
// Stacking
// ---------------------------------------------------------------------------

define("HSTACK", "HSTACK(array1, [array2], ...)", 1, 254, (args) => {
  const height = Math.max(...args.map((arg) => arg.length));
  return Array.from({ length: height }, (_, row) =>
    args.flatMap((arg) => {
      const width = widthOf(arg);
      // A shorter array is padded with #N/A below its last row.
      return Array.from({ length: width }, (__, col) => (row < arg.length ? (arg[row][col] ?? "") : ERR.na()));
    }),
  );
});

define("VSTACK", "VSTACK(array1, [array2], ...)", 1, 254, (args) => {
  const width = Math.max(...args.map(widthOf));
  return args.flatMap((arg) =>
    arg.map((row) => Array.from({ length: width }, (_, col) => (col < widthOf(arg) ? (row[col] ?? "") : ERR.na()))),
  );
});

// ---------------------------------------------------------------------------
// Taking and dropping
// ---------------------------------------------------------------------------

/** The slice [start, end) a TAKE count keeps out of `size` items: from the front, or the back if negative. */
function takeRange(count: number | null, size: number): [number, number] {
  if (count === null) return [0, size];
  return count >= 0 ? [0, Math.min(count, size)] : [Math.max(0, size + count), size];
}

/** The slice a DROP count keeps: it removes from the front, or from the back if negative. */
function dropRange(count: number | null, size: number): [number, number] {
  if (count === null) return [0, size];
  return count >= 0 ? [Math.min(count, size), size] : [0, Math.max(0, size + count)];
}

function slice(args: Args, range: (count: number | null, size: number) => [number, number]): CellMatrix | FormulaError {
  const matrix = args[0];
  const rows = countArg(args[1]);
  if (isError(rows)) return rows;
  const cols = countArg(args[2]);
  if (isError(cols)) return cols;
  const [top, bottom] = range(rows, matrix.length);
  const [left, right] = range(cols, widthOf(matrix));
  return nonEmpty(
    matrix
      .slice(top, bottom)
      .map((row) => Array.from({ length: Math.max(0, right - left) }, (_, i) => row[left + i] ?? "")),
  );
}

define("TAKE", "TAKE(array, rows, [columns])", 2, 3, (args) => slice(args, takeRange));
define("DROP", "DROP(array, rows, [columns])", 2, 3, (args) => slice(args, dropRange));

// ---------------------------------------------------------------------------
// Choosing
// ---------------------------------------------------------------------------

/** The 0-based positions asked for by the index arguments; 0 or out of range is `#VALUE!`. */
function indexes(args: Args, size: number): number[] | FormulaError {
  const out: number[] = [];
  for (const arg of args) {
    for (const row of arg) {
      for (const item of row) {
        const value = toNumber(item);
        if (isError(value)) return value;
        const index = Math.trunc(value);
        if (index === 0 || Math.abs(index) > size) return ERR.value();
        out.push(index > 0 ? index - 1 : size + index);
      }
    }
  }
  return out;
}

define("CHOOSECOLS", "CHOOSECOLS(array, col_num1, [col_num2], ...)", 2, 254, (args) => {
  const matrix = args[0];
  const picks = indexes(args.slice(1), widthOf(matrix));
  if (isError(picks)) return picks;
  return matrix.map((row) => picks.map((col) => row[col] ?? ""));
});

define("CHOOSEROWS", "CHOOSEROWS(array, row_num1, [row_num2], ...)", 2, 254, (args) => {
  const matrix = args[0];
  const picks = indexes(args.slice(1), matrix.length);
  if (isError(picks)) return picks;
  return picks.map((row) => [...matrix[row]]);
});

// ---------------------------------------------------------------------------
// Expanding, flattening and wrapping
// ---------------------------------------------------------------------------

define("EXPAND", "EXPAND(array, rows, [columns], [pad_with])", 2, 4, (args) => {
  const matrix = args[0];
  const rows = countArg(args[1]);
  if (isError(rows)) return rows;
  const cols = countArg(args[2]);
  if (isError(cols)) return cols;
  const height = rows ?? matrix.length;
  const width = cols ?? widthOf(matrix);
  // EXPAND only grows an array.
  if (height < matrix.length || width < widthOf(matrix)) return ERR.value();
  if (height * width > MAX_SPILL_CELLS) return ERR.num();
  const pad: Scalar = isOmitted(args[3]) ? ERR.na() : args[3]![0][0];
  return Array.from({ length: height }, (_, row) =>
    Array.from({ length: width }, (__, col) => matrix[row]?.[col] ?? pad),
  );
});

/** The cells of `matrix` in scan order, minus whatever `ignore` (0-3) leaves out. */
function scanCells(args: Args): Scalar[] | FormulaError {
  const matrix = args[0];
  const ignore = countArg(args[1]);
  if (isError(ignore)) return ignore;
  const byColumn = optionalBool(args[2]?.[0]?.[0], false);
  if (isError(byColumn)) return byColumn;
  if (ignore !== null && (ignore < 0 || ignore > 3)) return ERR.value();
  const skipBlanks = ignore === 1 || ignore === 3;
  const skipErrors = ignore === 2 || ignore === 3;
  const height = matrix.length;
  const width = widthOf(matrix);
  const out: Scalar[] = [];
  const total = height * width;
  for (let step = 0; step < total; step += 1) {
    const row = byColumn ? step % height : Math.floor(step / width);
    const col = byColumn ? Math.floor(step / height) : step % width;
    const value = matrix[row][col] ?? "";
    if (skipBlanks && value === "") continue;
    if (skipErrors && isError(value)) continue;
    out.push(value);
  }
  return out;
}

define("TOCOL", "TOCOL(array, [ignore], [scan_by_column])", 1, 3, (args) => {
  const cells = scanCells(args);
  return isError(cells) ? cells : nonEmpty(cells.map((value) => [value]));
});

define("TOROW", "TOROW(array, [ignore], [scan_by_column])", 1, 3, (args) => {
  const cells = scanCells(args);
  return isError(cells) ? cells : nonEmpty([cells]);
});

/** WRAPROWS/WRAPCOLS: lays a row or column vector out in chunks of `wrap_count`. */
function wrap(args: Args, byRow: boolean): CellMatrix | FormulaError {
  const vector = args[0];
  if (vector.length > 1 && widthOf(vector) > 1) return ERR.value();
  const count = countArg(args[1]);
  if (isError(count)) return count;
  if (count === null) return ERR.value();
  if (count < 1) return ERR.num();
  const items = vector.flat();
  const pad: Scalar = isOmitted(args[2]) ? ERR.na() : args[2]![0][0];
  const chunks = Math.ceil(items.length / count);
  const at = (chunk: number, offset: number): Scalar => items[chunk * count + offset] ?? pad;
  if (chunks * count > MAX_SPILL_CELLS) return ERR.num();
  return byRow
    ? Array.from({ length: chunks }, (_, chunk) => Array.from({ length: count }, (__, offset) => at(chunk, offset)))
    : Array.from({ length: count }, (_, offset) => Array.from({ length: chunks }, (__, chunk) => at(chunk, offset)));
}

define("WRAPROWS", "WRAPROWS(vector, wrap_count, [pad_with])", 2, 3, (args) => wrap(args, true));
define("WRAPCOLS", "WRAPCOLS(vector, wrap_count, [pad_with])", 2, 3, (args) => wrap(args, false));
