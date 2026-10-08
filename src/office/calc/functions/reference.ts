/**
 * Reference functions: OFFSET, INDIRECT, CELL and INFO.
 *
 * They are registered as context functions because they need the reference an
 * argument denotes (OFFSET, CELL) or have to resolve text into one (INDIRECT),
 * which a plain "values in, value out" function cannot do. All four are
 * volatile (see `VOLATILE_FUNCTIONS` in formula.ts): their result depends on
 * cells the formula text does not name.
 */
import { registerContextFunction, type ContextArgument, type FunctionHost } from "../registry";
import {
  cellTypeLetter,
  firstCell,
  isCellReference,
  offsetReference,
  referenceAddress,
  type CellReference,
} from "../references";
import { ERR, isError, toBool, toNumber, toText, type FormulaError, type Scalar } from "../scalars";

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

/** The reference CELL describes: its argument, or the formula's own cell. */
function cellTarget(args: ContextArgument[], host: FunctionHost): CellReference | FormulaError {
  if (args.length > 1) {
    const reference = args[1].reference();
    if (isError(reference)) return reference;
    if (reference === null) {
      const value = scalarOf(args[1]);
      return isError(value) ? value : ERR.value();
    }
    return reference;
  }
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
