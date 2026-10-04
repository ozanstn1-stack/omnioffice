/**
 * Dynamic-array functions: FILTER, SORT, SORTBY, UNIQUE, SEQUENCE, TRANSPOSE.
 *
 * These return a matrix rather than a scalar, so a formula that uses one can
 * spill across several cells - the same idea as Excel's spilled arrays, only
 * without relying on the browser for the spill.
 */
import { MAX_COLS, MAX_ROWS } from "../addresses";
import { registerFunction } from "../registry";
import {
  ERR,
  compareScalars,
  flatten,
  isError,
  optionalBool,
  toNumber,
  toText,
  type CellMatrix,
  type Scalar,
} from "../scalars";

/** Hard ceiling on a spilled array, mirroring the evaluator's range guard. */
const MAX_SPILL_CELLS = 20_000;

function guard(cells: number): boolean {
  return cells > MAX_SPILL_CELLS;
}

function firstError(matrix: CellMatrix): Scalar | null {
  for (const row of matrix) {
    for (const value of row) {
      if (isError(value)) return value;
    }
  }
  return null;
}

registerFunction(
  "SEQUENCE",
  (args) => {
    const rowsArg = toNumber(args[0]?.[0]?.[0] ?? 1);
    if (isError(rowsArg)) return rowsArg;
    const colsArg = args[1] ? toNumber(args[1]?.[0]?.[0] ?? 1) : 1;
    if (isError(colsArg)) return colsArg;
    const start = args[2] ? toNumber(args[2]?.[0]?.[0] ?? 1) : 1;
    if (isError(start)) return start;
    const step = args[3] ? toNumber(args[3]?.[0]?.[0] ?? 1) : 1;
    if (isError(step)) return step;
    const rows = Math.trunc(rowsArg);
    const cols = Math.trunc(colsArg);
    if (rows < 1 || cols < 1) return ERR.value();
    if (rows > MAX_ROWS || cols > MAX_COLS || guard(rows * cols)) return ERR.num();
    const out: CellMatrix = [];
    let value = start;
    for (let row = 0; row < rows; row += 1) {
      const line: Scalar[] = [];
      for (let col = 0; col < cols; col += 1) {
        line.push(value);
        value += step;
      }
      out.push(line);
    }
    return out;
  },
  1,
  4,
  false,
  { signature: "SEQUENCE(rows, [columns], [start], [step])", category: "Array" },
);

registerFunction(
  "UNIQUE",
  (args) => {
    const matrix = args[0] ?? [];
    if (matrix.length === 0) return ERR.value();
    const byColumn = optionalBool(args[1]?.[0]?.[0]);
    if (isError(byColumn)) return byColumn;
    const exactlyOnce = optionalBool(args[2]?.[0]?.[0]);
    if (isError(exactlyOnce)) return exactlyOnce;

    const keyOf = (row: Scalar[]) => row.map((value) => toText(value)).join("\u0000");
    if (byColumn) {
      const width = Math.max(...matrix.map((row) => row.length));
      const columns: Scalar[][] = [];
      for (let col = 0; col < width; col += 1) columns.push(matrix.map((row) => row[col] ?? ""));
      const counts = new Map<string, number>();
      for (const column of columns) {
        const key = keyOf(column);
        counts.set(key, (counts.get(key) ?? 0) + 1);
      }
      const seen = new Set<string>();
      const kept: Scalar[][] = [];
      for (const column of columns) {
        const key = keyOf(column);
        if (seen.has(key)) continue;
        if (exactlyOnce && (counts.get(key) ?? 0) !== 1) continue;
        seen.add(key);
        kept.push(column);
      }
      if (kept.length === 0) return ERR.na();
      return kept[0].map((_value, rowIndex) => kept.map((column) => column[rowIndex] ?? ""));
    }

    const counts = new Map<string, number>();
    for (const row of matrix) {
      const key = keyOf(row);
      counts.set(key, (counts.get(key) ?? 0) + 1);
    }
    const seen = new Set<string>();
    const out: CellMatrix = [];
    for (const row of matrix) {
      const key = keyOf(row);
      if (seen.has(key)) continue;
      if (exactlyOnce && (counts.get(key) ?? 0) !== 1) continue;
      seen.add(key);
      out.push([...row]);
    }
    return out.length === 0 ? ERR.na() : out;
  },
  1,
  3,
  false,
  { signature: "UNIQUE(array, [by_col], [exactly_once])", category: "Array" },
);

registerFunction(
  "SORT",
  (args) => {
    const matrix = args[0] ?? [];
    if (matrix.length === 0) return ERR.value();
    const byColumn = optionalBool(args[3]?.[0]?.[0]);
    if (isError(byColumn)) return byColumn;
    const indexArg = toNumber(args[1]?.[0]?.[0] ?? 1);
    if (isError(indexArg)) return indexArg;
    const orderArg = toNumber(args[2]?.[0]?.[0] ?? 1);
    if (isError(orderArg)) return orderArg;
    const direction = orderArg < 0 ? -1 : 1;
    if (byColumn) {
      const width = Math.max(...matrix.map((row) => row.length));
      const index = Math.trunc(indexArg) - 1;
      if (index < 0 || index >= width) return ERR.value();
      const columns: Scalar[][] = [];
      for (let col = 0; col < width; col += 1) columns.push(matrix.map((row) => row[col] ?? ""));
      columns.sort((a, b) => compareScalars(a[index] ?? "", b[index] ?? "") * direction);
      return columns[0]?.map((_value, rowIndex) => columns.map((column) => column[rowIndex] ?? "")) ?? ERR.value();
    }
    const index = Math.trunc(indexArg) - 1;
    if (index < 0) return ERR.value();
    return [...matrix]
      .map((row) => [...row])
      .sort((a, b) => {
        // Excel sorts blanks last regardless of direction.
        const leftEmpty = toText(a[index] ?? "") === "";
        const rightEmpty = toText(b[index] ?? "") === "";
        if (leftEmpty !== rightEmpty) return leftEmpty ? 1 : -1;
        return compareScalars(a[index] ?? "", b[index] ?? "") * direction;
      });
  },
  1,
  4,
  false,
  { signature: "SORT(array, [sort_index], [sort_order], [by_col])", category: "Array" },
);

registerFunction(
  "SORTBY",
  (args) => {
    const matrix = args[0] ?? [];
    if (matrix.length === 0) return ERR.value();
    const keys: Scalar[][] = [];
    const orders: number[] = [];
    for (let position = 1; position < args.length; position += 2) {
      const flat = flatten([args[position] ?? []]);
      if (flat.length !== matrix.length) return ERR.value();
      keys.push(flat);
      const order = toNumber(args[position + 1]?.[0]?.[0] ?? 1);
      orders.push(isError(order) ? 1 : order < 0 ? -1 : 1);
    }
    const rows = matrix.map((row, index) => ({ row: [...row], index }));
    rows.sort((a, b) => {
      for (let key = 0; key < keys.length; key += 1) {
        const comparison = compareScalars(keys[key][a.index] ?? "", keys[key][b.index] ?? "");
        if (comparison !== 0) return comparison * orders[key];
      }
      return 0;
    });
    return rows.map((entry) => entry.row);
  },
  2,
  64,
  false,
  { signature: "SORTBY(array, by_array1, [order1], ...)", category: "Array" },
);

registerFunction(
  "FILTER",
  (args) => {
    const matrix = args[0] ?? [];
    if (matrix.length === 0) return ERR.value();
    const includeMatrix = args[1] ?? [];
    const include = flatten([includeMatrix]);
    // Excel's contract: the include argument is a boolean mask, either one row
    // tall (one entry per data row) or the same shape as the data. A single
    // boolean broadcasts to every row. Blank/empty cells count as FALSE (the
    // old code treated "" as TRUE, which returned unselected rows).
    const maskMatchesRows = include.length === matrix.length;
    const maskSameShape = include.length === flatten([matrix]).length;
    const singleBoolean = include.length === 1 && typeof include[0] === "boolean";
    if (!maskMatchesRows && !maskSameShape && !singleBoolean) return ERR.value();
    const truthy = (value: Scalar) => value === true;
    let predicate: (row: Scalar[], index: number) => boolean;
    if (singleBoolean) {
      predicate = () => truthy(include[0]);
    } else if (maskSameShape && !maskMatchesRows) {
      const width = matrix[0]?.length ?? 0;
      predicate = (row, index) => row.every((_cell, col) => truthy(include[index * width + col] ?? false));
    } else {
      predicate = (_row, index) => truthy(include[index] ?? false);
    }
    const out = matrix.filter((row, index) => predicate(row, index));
    if (out.length === 0) return args[2] ? (args[2][0]?.[0] ?? "") : ERR.na();
    return out.map((row) => [...row]);
  },
  2,
  3,
  false,
  { signature: "FILTER(array, include, [if_empty])", category: "Array" },
);

registerFunction(
  "TRANSPOSE",
  (args) => {
    const matrix = args[0] ?? [];
    if (matrix.length === 0) return ERR.value();
    const width = Math.max(...matrix.map((row) => row.length));
    const out: CellMatrix = [];
    for (let col = 0; col < width; col += 1) out.push(matrix.map((row) => row[col] ?? ""));
    return out;
  },
  1,
  1,
  false,
  { signature: "TRANSPOSE(array)", category: "Array" },
);

/** Flattens a matrix into a single row (used by the pivot and chart code). */
export function toRow(matrix: CellMatrix): CellMatrix {
  return [flatten([matrix])];
}

/** Flattens a matrix into a single column. */
export function toColumn(matrix: CellMatrix): CellMatrix {
  return flatten([matrix]).map((value) => [value]);
}

/** Propagates the first error found in a matrix, or null. */
export { firstError as firstMatrixError };
