/**
 * Reference functions: OFFSET, INDIRECT, CELL, INFO, ROW, COLUMN, ADDRESS,
 * HYPERLINK, ISFORMULA and FORMULATEXT.
 *
 * Most are registered as context functions because they need the reference an
 * argument denotes (OFFSET, CELL, ROW, ISFORMULA) or have to resolve text into
 * one (INDIRECT), which a plain "values in, value out" function cannot do.
 * OFFSET, INDIRECT, CELL and INFO are volatile (see `VOLATILE_FUNCTIONS` in
 * formula.ts): their result depends on cells the formula text does not name.
 * ROW and the others are not - what they read is named by their arguments.
 */
import { MAX_COLS, MAX_ROWS, columnLabel } from "../addresses";
import { registerContextFunction, registerFunction, type ContextArgument, type FunctionHost } from "../registry";
import {
  cellTypeLetter,
  firstCell,
  isCellReference,
  offsetReference,
  referenceAddress,
  referenceSize,
  sheetPrefix,
  type CellReference,
} from "../references";
import {
  ERR,
  isError,
  optionalBool,
  toBool,
  toNumber,
  toText,
  type CellMatrix,
  type FormulaError,
  type Scalar,
} from "../scalars";

/** The first value of an argument; an error value stays an error. */
function scalarOf(arg: ContextArgument | undefined): Scalar {
  const value = arg?.value();
  if (value === undefined) return "";
  return Array.isArray(value) ? (value[0]?.[0] ?? "") : value;
}

/** A skipped optional argument arrives as an empty string. */
function isSkipped(arg: ContextArgument | undefined): boolean {
  return arg === undefined || scalarOf(arg) === "";
}

/** A whole-number argument, truncated toward zero like Excel does. */
function integerArg(arg: ContextArgument): number | FormulaError {
  const value = toNumber(scalarOf(arg));
  return isError(value) ? value : Math.trunc(value);
}

registerContextFunction(
  "OFFSET",
  (args) => {
    const base = args[0].reference();
    if (isError(base)) return base;
    if (base === null) {
      // Not a location: an error value passes through, anything else is #VALUE!.
      const value = scalarOf(args[0]);
      return isError(value) ? value : ERR.value();
    }
    const rows = integerArg(args[1]);
    if (isError(rows)) return rows;
    const cols = integerArg(args[2]);
    if (isError(cols)) return cols;
    let height: number | undefined;
    if (!isSkipped(args[3])) {
      const parsed = integerArg(args[3]);
      if (isError(parsed)) return parsed;
      height = parsed;
    }
    let width: number | undefined;
    if (!isSkipped(args[4])) {
      const parsed = integerArg(args[4]);
      if (isError(parsed)) return parsed;
      width = parsed;
    }
    return offsetReference(base, rows, cols, height, width);
  },
  3,
  5,
  { signature: "OFFSET(reference, rows, cols, [height], [width])", category: "Lookup" },
);

registerContextFunction(
  "INDIRECT",
  (args, host) => {
    const text = scalarOf(args[0]);
    if (isError(text)) return text;
    // R1C1 notation is not supported: asking for it is a reference error
    // instead of a silent misread of "R1C1" as a cell address.
    if (!isSkipped(args[1])) {
      const style = scalarOf(args[1]);
      if (isError(style)) return style;
      if (!toBool(style)) return ERR.ref();
    }
    return host.parseReference(toText(text));
  },
  1,
  2,
  { signature: "INDIRECT(ref_text, [a1])", category: "Lookup" },
);

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

/** The reference CELL describes: its argument, or the formula's own cell. */
function cellTarget(args: ContextArgument[], host: FunctionHost): CellReference | FormulaError {
  if (args.length > 1) return locationOf(args[1]);
  return host.currentAddress === null ? ERR.value() : host.parseReference(host.currentAddress);
}

registerContextFunction(
  "CELL",
  (args, host) => {
    const kind = scalarOf(args[0]);
    if (isError(kind)) return kind;
    const target = cellTarget(args, host);
    if (isError(target) || !isCellReference(target)) return target;
    switch (toText(kind).toLowerCase()) {
      case "address":
        return referenceAddress(target, host.currentSheet);
      case "row":
        return target.start.row + 1;
      case "col":
        return target.start.col + 1;
      case "contents":
      case "type": {
        const cell = host.read(firstCell(target));
        if (isError(cell)) return cell;
        const value = cell[0]?.[0] ?? "";
        return toText(kind).toLowerCase() === "type" ? cellTypeLetter(value) : value;
      }
      default:
        return ERR.value();
    }
  },
  1,
  2,
  {
    signature: 'CELL(info_type, [reference])  info_type: "address", "row", "col", "contents", "type"',
    category: "Lookup",
  },
);

/** Excel-style operating system description derived from the user agent. */
export function osVersionText(userAgent: string): string {
  const windows = /Windows NT (\d+)\.(\d+)/.exec(userAgent);
  if (windows) {
    const bits = /Win64|x64|WOW64|arm64/i.test(userAgent) ? "64-bit" : "32-bit";
    return `Windows (${bits}) NT ${windows[1]}.${windows[2].padEnd(2, "0")}`;
  }
  const android = /Android (\d+(?:\.\d+)*)/.exec(userAgent);
  if (android) return `Android ${android[1]}`;
  if (/iPhone|iPad/.test(userAgent)) return "iOS";
  const mac = /Mac OS X (\d+)[._](\d+)(?:[._](\d+))?/.exec(userAgent);
  if (mac) return `Macintosh (Intel) ${mac[1]}.${mac[2]}${mac[3] ? `.${mac[3]}` : ""}`;
  if (/Macintosh/.test(userAgent)) return "Macintosh";
  if (/Linux|X11/.test(userAgent)) return "Linux";
  return "Unknown";
}

/** INFO("system"): the operating environment, "mac" or "pcdos" like Excel. */
export function systemName(userAgent: string): string {
  return /Macintosh|Mac OS X/.test(userAgent) && !/iPhone|iPad/.test(userAgent) ? "mac" : "pcdos";
}

function currentUserAgent(): string {
  return typeof navigator === "undefined" ? "" : (navigator.userAgent ?? "");
}

registerContextFunction(
  "INFO",
  (args, host) => {
    const kind = scalarOf(args[0]);
    if (isError(kind)) return kind;
    switch (toText(kind).toLowerCase()) {
      case "osversion":
        return osVersionText(currentUserAgent());
      case "system":
        return systemName(currentUserAgent());
      case "numfile":
        return host.sheetNames.length;
      case "recalc":
        return "Automatic";
      default:
        return ERR.value();
    }
  },
  1,
  1,
  { signature: 'INFO(type_text)  type_text: "osversion", "system", "numfile", "recalc"', category: "Lookup" },
);

/** Same ceiling as the evaluator's range guard, for results built from a reference's shape. */
const MAX_REFERENCE_CELLS = 200_000;

/** The reference ROW and COLUMN describe: their argument, or the formula's own cell. */
function positionTarget(args: ContextArgument[], host: FunctionHost): CellReference | FormulaError {
  if (args.length > 0) return locationOf(args[0]);
  return host.currentAddress === null ? ERR.value() : host.parseReference(host.currentAddress);
}

registerContextFunction(
  "ROW",
  (args, host) => {
    const target = positionTarget(args, host);
    if (isError(target)) return target;
    const { rows } = referenceSize(target);
    if (rows === 1) return target.start.row + 1;
    if (rows > MAX_REFERENCE_CELLS) return ERR.ref();
    // A multi-row reference gives one row number per row, as a column.
    return Array.from({ length: rows }, (_, offset) => [target.start.row + offset + 1]);
  },
  0,
  1,
  { signature: "ROW([reference])", category: "Lookup" },
);

registerContextFunction(
  "COLUMN",
  (args, host) => {
    const target = positionTarget(args, host);
    if (isError(target)) return target;
    const { cols } = referenceSize(target);
    if (cols === 1) return target.start.col + 1;
    return [Array.from({ length: cols }, (_, offset) => target.start.col + offset + 1)];
  },
  0,
  1,
  { signature: "COLUMN([reference])", category: "Lookup" },
);

/** A skipped optional argument reads as its default; anything else is converted. */
function integerOption(arg: Scalar | undefined, fallback: number): number | FormulaError {
  if (arg === undefined || arg === "") return fallback;
  const number = toNumber(arg);
  return isError(number) ? number : Math.trunc(number);
}

registerFunction(
  "ADDRESS",
  (args) => {
    const row = integerOption(args[0]?.[0]?.[0], Number.NaN);
    if (isError(row)) return row;
    const col = integerOption(args[1]?.[0]?.[0], Number.NaN);
    if (isError(col)) return col;
    const mode = integerOption(args[2]?.[0]?.[0], 1);
    if (isError(mode)) return mode;
    const a1 = optionalBool(args[3]?.[0]?.[0], true);
    if (isError(a1)) return a1;
    if (!(row >= 1 && row <= MAX_ROWS && col >= 1 && col <= MAX_COLS && mode >= 1 && mode <= 4)) return ERR.value();
    const rowAbsolute = mode === 1 || mode === 2;
    const colAbsolute = mode === 1 || mode === 3;
    let address: string;
    if (a1) {
      address = `${colAbsolute ? "$" : ""}${columnLabel(col - 1)}${rowAbsolute ? "$" : ""}${row}`;
    } else {
      // R1C1: an absolute part is a plain number, a relative one sits in brackets.
      address = `R${rowAbsolute ? row : `[${row}]`}C${colAbsolute ? col : `[${col}]`}`;
    }
    const sheet = args[4]?.[0]?.[0];
    return sheet === undefined || sheet === "" ? address : `${sheetPrefix(toText(sheet))}${address}`;
  },
  2,
  5,
  false,
  { signature: "ADDRESS(row_num, column_num, [abs_num], [a1], [sheet_text])", category: "Lookup" },
);

registerFunction(
  "HYPERLINK",
  (args) => {
    // The link itself is for the grid to follow; the cell shows the friendly
    // name, or the link text when there is none.
    if (args.length > 1) return args[1]?.[0]?.[0] ?? "";
    return toText(args[0]?.[0]?.[0] ?? "");
  },
  1,
  2,
  false,
  { signature: "HYPERLINK(link_location, [friendly_name])", category: "Lookup" },
);

registerContextFunction(
  "ISFORMULA",
  (args, host) => {
    const target = locationOf(args[0]);
    if (isError(target)) return target;
    const { rows, cols } = referenceSize(target);
    if (rows * cols > MAX_REFERENCE_CELLS) return ERR.ref();
    const out: CellMatrix = [];
    for (let row = target.start.row; row <= target.end.row; row += 1) {
      const line: Scalar[] = [];
      for (let col = target.start.col; col <= target.end.col; col += 1) {
        line.push(host.formulaAt(target.sheet, { row, col }) !== null);
      }
      out.push(line);
    }
    return rows === 1 && cols === 1 ? out[0][0] : out;
  },
  1,
  1,
  { signature: "ISFORMULA(reference)", category: "Information" },
);

registerContextFunction(
  "FORMULATEXT",
  (args, host) => {
    const target = locationOf(args[0]);
    if (isError(target)) return target;
    // A range answers for its top-left cell.
    const formula = host.formulaAt(target.sheet, target.start);
    if (formula === null) return ERR.na();
    return formula.startsWith("=") ? formula : `=${formula}`;
  },
  1,
  1,
  { signature: "FORMULATEXT(reference)", category: "Lookup" },
);
