/**
 * Logical, information and lookup functions.
 */
import { registerFunction, type FunctionResult } from "../registry";
import {
  ERR,
  compareScalars,
  flatten,
  isError,
  padMatrix,
  toBool,
  toNumber,
  toText,
  type CellMatrix,
  type Scalar,
} from "../scalars";

// LET is evaluated by the interpreter itself, because it needs lazy bindings
// that a registered implementation cannot express. It is registered here only
// so the function picker, the catalogue and `functionNames()` list it like any
// other function; the evaluator intercepts the call before dispatch.
registerFunction("LET", () => ERR.name(), 3, 64, true, {
  signature: "LET(name1, value1, ..., expression)",
  category: "Logical",
});

registerFunction(
  "IF",
  (args) => {
    const condition = toBool(args[0]?.[0]?.[0] ?? false);
    if (isError(condition)) return condition;
    return condition ? (args[1]?.[0]?.[0] ?? true) : (args[2]?.[0]?.[0] ?? false);
  },
  2,
  4,
  true,
  { signature: "IF(test, then, [else])", category: "Logical" },
);
registerFunction(
  "IFS",
  (args) => {
    for (let index = 0; index + 1 < args.length; index += 2) {
      const condition = toBool(args[index]?.[0]?.[0] ?? false);
      if (isError(condition)) return condition;
      if (condition) return args[index + 1]?.[0]?.[0] ?? true;
    }
    return ERR.na();
  },
  2,
  64,
  true,
  { signature: "IFS(test1, value1, ...)", category: "Logical" },
);
registerFunction(
  "IFERROR",
  (args) => {
    const value = args[0]?.[0]?.[0];
    return isError(value) ? (args[1]?.[0]?.[0] ?? "") : (value ?? "");
  },
  2,
  2,
  true,
  { signature: "IFERROR(value, fallback)", category: "Logical" },
);
registerFunction(
  "IFNA",
  (args) => {
    const value = args[0]?.[0]?.[0];
    if (isError(value) && value.code === "#N/A") return args[1]?.[0]?.[0] ?? "";
    return value ?? "";
  },
  2,
  2,
  true,
  { signature: "IFNA(value, fallback)", category: "Logical" },
);
registerFunction(
  "SWITCH",
  (args) => {
    const target = args[0]?.[0]?.[0];
    for (let index = 1; index + 1 < args.length; index += 2) {
      if (compareScalars(target ?? "", args[index]?.[0]?.[0] ?? "") === 0) return args[index + 1]?.[0]?.[0] ?? "";
    }
    // An odd trailing argument is the default case.
    return args.length % 2 === 0 ? (args[args.length - 1]?.[0]?.[0] ?? "") : ERR.na();
  },
  3,
  64,
  false,
  { signature: "SWITCH(expression, value1, result1, ..., [default])", category: "Logical" },
);
registerFunction(
  "AND",
  (args) => {
    const values = flatten(args).filter((value) => toText(value) !== "");
    if (values.length === 0) return ERR.value();
    return values.every((value) => {
      const flag = toBool(value);
      return isError(flag) ? false : flag;
    });
  },
  1,
  64,
  false,
  { signature: "AND(logical1, ...)", category: "Logical" },
);
registerFunction(
  "OR",
  (args) => {
    const values = flatten(args).filter((value) => toText(value) !== "");
    if (values.length === 0) return ERR.value();
    return values.some((value) => {
      const flag = toBool(value);
      return isError(flag) ? false : flag;
    });
  },
  1,
  64,
  false,
  { signature: "OR(logical1, ...)", category: "Logical" },
);
registerFunction(
  "XOR",
  (args) => {
    const values = flatten(args).filter((value) => toText(value) !== "");
    if (values.length === 0) return ERR.value();
    let odd = 0;
    for (const value of values) {
      const flag = toBool(value);
      if (!isError(flag) && flag) odd += 1;
    }
    return odd % 2 === 1;
  },
  1,
  64,
  false,
  { signature: "XOR(logical1, ...)", category: "Logical" },
);
registerFunction(
  "NOT",
  (args) => {
    const flag = toBool(args[0]?.[0]?.[0] ?? false);
    return isError(flag) ? flag : !flag;
  },
  1,
  1,
  false,
  { signature: "NOT(logical)", category: "Logical" },
);
registerFunction("TRUE", () => true, 0, 0, false, { signature: "TRUE()", category: "Logical" });
registerFunction("FALSE", () => false, 0, 0, false, { signature: "FALSE()", category: "Logical" });

registerFunction("ISNUMBER", (args) => typeof args[0]?.[0]?.[0] === "number", 1, 1, true, {
  signature: "ISNUMBER(value)",
  category: "Information",
});
registerFunction("ISTEXT", (args) => typeof args[0]?.[0]?.[0] === "string", 1, 1, true, {
  signature: "ISTEXT(value)",
  category: "Information",
});
registerFunction("ISNONTEXT", (args) => typeof args[0]?.[0]?.[0] !== "string", 1, 1, true, {
  signature: "ISNONTEXT(value)",
  category: "Information",
});
registerFunction("ISLOGICAL", (args) => typeof args[0]?.[0]?.[0] === "boolean", 1, 1, true, {
  signature: "ISLOGICAL(value)",
  category: "Information",
});
registerFunction(
  "ISBLANK",
  (args) => {
    const value = args[0]?.[0]?.[0];
    return value === undefined || value === "";
  },
  1,
  1,
  true,
  { signature: "ISBLANK(value)", category: "Information" },
);
registerFunction("ISERROR", (args) => isError(args[0]?.[0]?.[0]), 1, 1, true, {
  signature: "ISERROR(value)",
  category: "Information",
});
registerFunction(
  "ISERR",
  (args) => {
    const value = args[0]?.[0]?.[0];
    return isError(value) && value.code !== "#N/A";
  },
  1,
  1,
  true,
  { signature: "ISERR(value)", category: "Information" },
);
registerFunction(
  "ISNA",
  (args) => {
    const value = args[0]?.[0]?.[0];
    return isError(value) && value.code === "#N/A";
  },
  1,
  1,
  true,
  { signature: "ISNA(value)", category: "Information" },
);
registerFunction(
  "ISEVEN",
  (args) => {
    const value = toNumber(args[0]?.[0]?.[0] ?? 0);
    return isError(value) ? value : Math.trunc(value) % 2 === 0;
  },
  1,
  1,
  false,
  { signature: "ISEVEN(number)", category: "Information" },
);
registerFunction(
  "ISODD",
  (args) => {
    const value = toNumber(args[0]?.[0]?.[0] ?? 0);
    return isError(value) ? value : Math.abs(Math.trunc(value) % 2) === 1;
  },
  1,
  1,
  false,
  { signature: "ISODD(number)", category: "Information" },
);
registerFunction(
  "N",
  (args) => {
    // Numbers stay, TRUE/FALSE are 1/0, text (even "7") is 0, an error stays an error.
    const value = args[0]?.[0]?.[0] ?? 0;
    if (typeof value === "boolean") return value ? 1 : 0;
    return typeof value === "string" ? 0 : value;
  },
  1,
  1,
  true,
  { signature: "N(value)", category: "Information" },
);
registerFunction(
  "T",
  (args) => {
    // Text stays, a number or boolean becomes "", an error stays an error.
    const value = args[0]?.[0]?.[0];
    return typeof value === "string" || isError(value) ? (value ?? "") : "";
  },
  1,
  1,
  true,
  { signature: "T(value)", category: "Information" },
);
registerFunction("NA", () => ERR.na(), 0, 0, false, { signature: "NA()", category: "Information" });
registerFunction("PI", () => Math.PI, 0, 0, false, { signature: "PI()", category: "Math" });
registerFunction("RAND", () => Math.random(), 0, 0, false, { signature: "RAND()", category: "Math" });
registerFunction(
  "RANDBETWEEN",
  (args) => {
    const low = toNumber(args[0]?.[0]?.[0] ?? 0);
    const high = toNumber(args[1]?.[0]?.[0] ?? 0);
    if (isError(low)) return low;
    if (isError(high)) return high;
    return Math.floor(low + Math.random() * (Math.floor(high) - Math.ceil(low) + 1));
  },
  2,
  2,
  false,
  { signature: "RANDBETWEEN(low, high)", category: "Math" },
);

registerFunction("ROWS", (args) => args[0]?.length ?? 0, 1, 1, false, { signature: "ROWS(array)", category: "Lookup" });
registerFunction("COLUMNS", (args) => args[0]?.[0]?.length ?? 0, 1, 1, false, {
  signature: "COLUMNS(array)",
  category: "Lookup",
});
registerFunction(
  "CHOOSE",
  (args) => {
    const index = toNumber(args[0]?.[0]?.[0] ?? 0);
    if (isError(index)) return index;
    const position = Math.trunc(index);
    if (position < 1 || position >= args.length) return ERR.value();
    return args[position]?.[0]?.[0] ?? "";
  },
  2,
  64,
  false,
  { signature: "CHOOSE(index, value1, ...)", category: "Lookup" },
);

/** Case-insensitive exact match, like `MATCH` with match_type 0. */
function findInMatrix(matrix: CellMatrix, needle: Scalar): { row: number; col: number } | null {
  for (let row = 0; row < matrix.length; row += 1) {
    for (let col = 0; col < (matrix[row]?.length ?? 0); col += 1) {
      if (compareScalars(matrix[row][col] ?? "", needle) === 0) return { row, col };
    }
  }
  return null;
}

registerFunction(
  "MATCH",
  (args) => {
    const needle = args[0]?.[0]?.[0] ?? "";
    const matrix = args[1] ?? [];
    const mode = toNumber(args[2]?.[0]?.[0] ?? 1);
    if (isError(mode)) return mode;
    if (mode === 0) {
      const horizontal = matrix.length === 1 && (matrix[0]?.length ?? 0) > 1;
      if (horizontal) {
        const index = (matrix[0] ?? []).findIndex((value) => compareScalars(value, needle) === 0);
        return index < 0 ? ERR.na() : index + 1;
      }
      const found = findInMatrix(matrix, needle);
      return found ? found.row + 1 : ERR.na();
    }
    // Approximate modes scan the *original* array in order and return its
    // position. The previous version sorted a copy, so an unsorted range
    // returned an index that pointed at a different cell (silently wrong row).
    const flat = flatten([matrix]);
    const target = toNumber(needle);
    if (isError(target)) return ERR.na();
    let best = -1;
    flat.forEach((value, index) => {
      if (isError(value)) return;
      const numeric =
        typeof value === "number"
          ? value
          : typeof value === "string" && value.trim() !== ""
            ? Number(value)
            : Number.NaN;
      if (!Number.isFinite(numeric)) return;
      if (mode > 0 ? numeric <= target : numeric >= target) best = index;
    });
    return best < 0 ? ERR.na() : best + 1;
  },
  2,
  3,
  false,
  { signature: "MATCH(lookup, array, [type])", category: "Lookup" },
);

registerFunction(
  "XMATCH",
  (args) => {
    const needle = args[0]?.[0]?.[0] ?? "";
    const matrix = args[1] ?? [];
    const mode = toNumber(args[2]?.[0]?.[0] ?? 0);
    if (isError(mode)) return mode;
    const flat = flatten([matrix]);
    if (mode === 0) {
      const found = findInMatrix(matrix, needle);
      return found ? found.row + 1 : ERR.na();
    }
    if (mode === -1) {
      let best = -1;
      flat.forEach((value, index) => {
        if (compareScalars(value, needle) <= 0) best = index;
      });
      return best < 0 ? ERR.na() : best + 1;
    }
    if (mode === 1) {
      let best = -1;
      flat.forEach((value, index) => {
        if (compareScalars(value, needle) >= 0) best = index;
      });
      return best < 0 ? ERR.na() : best + 1;
    }
    const found = flat.findIndex((value) => compareScalars(value, needle) === 0);
    return found < 0 ? ERR.na() : found + 1;
  },
  2,
  4,
  false,
  { signature: "XMATCH(lookup, array, [mode], [search])", category: "Lookup" },
);

registerFunction(
  "INDEX",
  (args): FunctionResult => {
    const matrix = args[0] ?? [];
    if (matrix.length === 0) return ERR.ref();
    const rowArg = toNumber(args[1]?.[0]?.[0] ?? 0);
    if (isError(rowArg)) return rowArg;
    const colArg = toNumber(args[2]?.[0]?.[0] ?? 0);
    if (isError(colArg)) return colArg;
    // INDEX(array, n) on a single row/column means "the nth element".
    if (args.length === 2) {
      if (matrix.length === 1) {
        const index = Math.trunc(rowArg) - 1;
        if (index < 0 || index >= (matrix[0]?.length ?? 0)) return ERR.ref();
        return matrix[0][index] ?? "";
      }
      const index = Math.trunc(rowArg) - 1;
      if (index < 0 || index >= matrix.length) return ERR.ref();
      return [[...matrix[index]]];
    }
    const row = Math.trunc(rowArg) - 1;
    const col = Math.trunc(colArg) - 1;
    if (row < 0 || row >= matrix.length) return ERR.ref();
    if (col < 0) return [[...matrix[row]]];
    if (col >= (matrix[row]?.length ?? 0)) return ERR.ref();
    return matrix[row][col] ?? "";
  },
  2,
  3,
  false,
  { signature: "INDEX(array, row, [column])", category: "Lookup" },
);

registerFunction(
  "VLOOKUP",
  (args) => {
    const needle = args[0]?.[0]?.[0] ?? "";
    const table = args[1] ?? [];
    const columnArg = toNumber(args[2]?.[0]?.[0] ?? 1);
    if (isError(columnArg)) return columnArg;
    const column = Math.trunc(columnArg);
    if (column < 1) return ERR.value();
    const approximate = args[3] ? toNumber(args[3]?.[0]?.[0] ?? 1) !== 0 : true;
    for (const row of table) {
      const key = row?.[0] ?? "";
      if (compareScalars(key, needle) === 0) {
        if (column > (row?.length ?? 0)) return ERR.ref();
        return row[column - 1] ?? "";
      }
    }
    if (!approximate) return ERR.na();
    let best: Scalar[] | null = null;
    for (const row of table) {
      const key = row?.[0] ?? "";
      // A blank key must not become the "closest match": with a table that runs
      // past the data, the last empty row would otherwise win.
      if (String(key) === "") continue;
      if (compareScalars(key, needle) <= 0) best = row;
    }
    if (!best) return ERR.na();
    return best[column - 1] ?? ERR.ref();
  },
  3,
  4,
  false,
  { signature: "VLOOKUP(lookup, table, column, [approx])", category: "Lookup" },
);

registerFunction(
  "HLOOKUP",
  (args) => {
    const needle = args[0]?.[0]?.[0] ?? "";
    const table = args[1] ?? [];
    const rowArg = toNumber(args[2]?.[0]?.[0] ?? 1);
    if (isError(rowArg)) return rowArg;
    const row = Math.trunc(rowArg);
    if (row < 1 || row > table.length) return ERR.ref();
    const header = table[0] ?? [];
    const approximate = args[3] ? toNumber(args[3]?.[0]?.[0] ?? 1) !== 0 : true;
    let index = header.findIndex((value) => compareScalars(value, needle) === 0);
    if (index < 0) {
      if (!approximate) return ERR.na();
      // Largest header value <= needle, scanning left to right like Excel.
      let best = -1;
      header.forEach((value, position) => {
        if (String(value) === "") return;
        if (compareScalars(value, needle) <= 0) best = position;
      });
      index = best;
    }
    if (index < 0) return ERR.na();
    return table[row - 1]?.[index] ?? "";
  },
  3,
  4,
  false,
  { signature: "HLOOKUP(lookup, table, row, [approx])", category: "Lookup" },
);

registerFunction(
  "XLOOKUP",
  (args) => {
    const needle = args[0]?.[0]?.[0] ?? "";
    const lookup = args[1] ?? [];
    const result = args[2] ?? [];
    const notFound = args[3]?.[0]?.[0];
    const mode = toNumber(args[4]?.[0]?.[0] ?? 0);
    if (isError(mode)) return mode;
    const flatLookup = flatten([lookup]);
    const flatResult = flatten([result]);
    const fallback = notFound !== undefined ? notFound : ERR.na();
    const exact = (value: Scalar) => compareScalars(value, needle) === 0;
    if (mode === 0) {
      const index = flatLookup.findIndex(exact);
      return index < 0 ? fallback : (flatResult[index] ?? fallback);
    }
    if (mode === -1) {
      let best = -1;
      flatLookup.forEach((value, index) => {
        if (compareScalars(value, needle) <= 0) best = index;
      });
      return best < 0 ? fallback : (flatResult[best] ?? fallback);
    }
    if (mode === 1) {
      let best = -1;
      flatLookup.forEach((value, index) => {
        if (compareScalars(value, needle) >= 0) best = index;
      });
      return best < 0 ? fallback : (flatResult[best] ?? fallback);
    }
    if (mode === 2) {
      // Smallest value >= needle (ascending approximate match).
      let best = -1;
      flatLookup.forEach((value, index) => {
        if (compareScalars(value, needle) >= 0 && (best < 0 || compareScalars(value, flatLookup[best]) < 0))
          best = index;
      });
      return best < 0 ? fallback : (flatResult[best] ?? fallback);
    }
    if (mode === -2) {
      // Largest value <= needle (descending approximate match).
      let best = -1;
      flatLookup.forEach((value, index) => {
        if (compareScalars(value, needle) <= 0 && (best < 0 || compareScalars(value, flatLookup[best]) > 0))
          best = index;
      });
      return best < 0 ? fallback : (flatResult[best] ?? fallback);
    }
    return fallback;
  },
  3,
  6,
  false,
  { signature: "XLOOKUP(lookup, lookup_array, return_array, [if_not_found], [mode])", category: "Lookup" },
);

registerFunction(
  "LOOKUP",
  (args) => {
    const needle = args[0]?.[0]?.[0] ?? "";
    const lookup = flatten([args[1] ?? []]);
    const result = args[2] ? flatten([args[2]]) : lookup;
    let best = -1;
    lookup.forEach((value, index) => {
      if (compareScalars(value, needle) <= 0) best = index;
    });
    if (best < 0) return ERR.na();
    return result[best] ?? ERR.na();
  },
  2,
  3,
  false,
  { signature: "LOOKUP(value, lookup_vector, [result_vector])", category: "Lookup" },
);

/** Element-wise helper shared by the array functions. */
export function mapMatrix(matrix: CellMatrix, fn: (value: Scalar, row: number, col: number) => Scalar): CellMatrix {
  return matrix.map((row, rowIndex) => row.map((value, colIndex) => fn(value, rowIndex, colIndex)));
}

export { padMatrix };
