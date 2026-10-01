/**
 * Text functions: slicing, searching, joining and formatting.
 */
import { registerFunction } from "../registry";
import { formatNumber } from "../numberFormat";
import { ERR, FormulaError, flatten, isError, toNumber, toText, type Scalar } from "../scalars";

function textArg(args: Scalar[][][], index: number): string {
  return toText(args[index]?.[0]?.[0] ?? "");
}

function numberArg(args: Scalar[][][], index: number, fallback: number): number | FormulaError {
  if (args[index] === undefined) return fallback;
  const value = toNumber(args[index]?.[0]?.[0] ?? 0);
  return isError(value) ? value : value;
}

registerFunction("LEN", (args) => textArg(args, 0).length, 1, 1, false, { signature: "LEN(text)", category: "Text" });
registerFunction("TRIM", (args) => textArg(args, 0).trim().replace(/\s+/g, " "), 1, 1, false, { signature: "TRIM(text)", category: "Text" });
// CLEAN() is defined as removing the non-printing control characters, so the
// control-character class is the function, not an accident.
// eslint-disable-next-line no-control-regex
registerFunction("CLEAN", (args) => textArg(args, 0).replace(/[\x00-\x08\x0b\x0c\x0e-\x1f]/g, ""), 1, 1, false, { signature: "CLEAN(text)", category: "Text" });
registerFunction("UPPER", (args) => textArg(args, 0).toUpperCase(), 1, 1, false, { signature: "UPPER(text)", category: "Text" });
registerFunction("LOWER", (args) => textArg(args, 0).toLowerCase(), 1, 1, false, { signature: "LOWER(text)", category: "Text" });
registerFunction("PROPER", (args) =>
  textArg(args, 0).replace(/\w\S*/g, (word) => word.charAt(0).toUpperCase() + word.slice(1).toLowerCase()),
  1, 1, false, { signature: "PROPER(text)", category: "Text" });
registerFunction("LEFT", (args) => {
  const count = numberArg(args, 1, 1);
  if (typeof count !== "number") return count;
  if (count < 0) return ERR.value();
  return textArg(args, 0).slice(0, count);
}, 1, 2, false, { signature: "LEFT(text, [count])", category: "Text" });
registerFunction("RIGHT", (args) => {
  const count = numberArg(args, 1, 1);
  if (typeof count !== "number") return count;
  if (count < 0) return ERR.value();
  return count === 0 ? "" : textArg(args, 0).slice(-count);
}, 1, 2, false, { signature: "RIGHT(text, [count])", category: "Text" });
registerFunction("MID", (args) => {
  const start = numberArg(args, 1, 1);
  if (typeof start !== "number") return start;
  const count = numberArg(args, 2, 0);
  if (typeof count !== "number") return count;
  if (start < 1 || count < 0) return ERR.value();
  return textArg(args, 0).slice(start - 1, start - 1 + count);
}, 3, 3, false, { signature: "MID(text, start, count)", category: "Text" });
registerFunction("CONCAT", (args) => flatten(args).map(toText).join(""), 1, 64, false, { signature: "CONCAT(text1, ...)", category: "Text" });
registerFunction("CONCATENATE", (args) => flatten(args).map(toText).join(""), 1, 64, false, { signature: "CONCATENATE(text1, ...)", category: "Text" });
registerFunction("TEXTJOIN", (args) => {
  const delimiter = textArg(args, 0);
  const skipEmpty = args[1] ? toText(args[1]?.[0]?.[0] ?? "") !== "FALSE" : true;
  const parts = flatten(args.slice(2)).map(toText).filter((value) => (skipEmpty ? value !== "" : true));
  return parts.join(delimiter);
}, 3, 64, false, { signature: "TEXTJOIN(delimiter, ignore_empty, text1, ...)", category: "Text" });
registerFunction("REPT", (args) => {
  const count = numberArg(args, 1, 0);
  if (typeof count !== "number") return count;
  if (count < 0) return ERR.value();
  // Bound the result so =REPT("x", 1E9) cannot exhaust memory.
  return textArg(args, 0).repeat(Math.min(Math.trunc(count), 32_767));
}, 2, 2, false, { signature: "REPT(text, count)", category: "Text" });
registerFunction("SUBSTITUTE", (args) => {
  const text = textArg(args, 0);
  const from = textArg(args, 1);
  const to = textArg(args, 2);
  if (from === "") return text;
  if (args[3] === undefined) return text.split(from).join(to);
  const which = numberArg(args, 3, 1);
  if (typeof which !== "number") return which;
  const index = Math.trunc(which);
  if (index < 1) return ERR.value();
  let seen = 0;
  let position = text.indexOf(from);
  while (position >= 0) {
    seen += 1;
    if (seen === index) return text.slice(0, position) + to + text.slice(position + from.length);
    position = text.indexOf(from, position + from.length);
  }
  return text;
}, 3, 4, false, { signature: "SUBSTITUTE(text, old, new, [instance])", category: "Text" });
registerFunction("REPLACE", (args) => {
  const text = textArg(args, 0);
  const start = numberArg(args, 1, 1);
  if (typeof start !== "number") return start;
  const count = numberArg(args, 2, 0);
  if (typeof count !== "number") return count;
  if (start < 1 || count < 0) return ERR.value();
  return text.slice(0, start - 1) + textArg(args, 3) + text.slice(start - 1 + count);
}, 4, 4, false, { signature: "REPLACE(text, start, count, new)", category: "Text" });
registerFunction("FIND", (args) => {
  const needle = textArg(args, 0);
  const haystack = textArg(args, 1);
  const startArg = numberArg(args, 2, 1);
  if (typeof startArg !== "number") return startArg;
  const start = Math.trunc(startArg);
  if (start < 1 || start > haystack.length + 1) return ERR.value();
  const index = haystack.indexOf(needle, start - 1);
  return index < 0 ? ERR.value() : index + 1;
}, 2, 3, false, { signature: "FIND(needle, text, [start])", category: "Text" });
registerFunction("SEARCH", (args) => {
  const needle = textArg(args, 0);
  const haystack = textArg(args, 1);
  const startArg = numberArg(args, 2, 1);
  if (typeof startArg !== "number") return startArg;
  const start = Math.trunc(startArg);
  if (start < 1) return ERR.value();
  const index = haystack.toLowerCase().indexOf(needle.toLowerCase(), start - 1);
  return index < 0 ? ERR.value() : index + 1;
}, 2, 3, false, { signature: "SEARCH(needle, text, [start])", category: "Text" });
registerFunction("EXACT", (args) => textArg(args, 0) === textArg(args, 1), 2, 2, false, { signature: "EXACT(text1, text2)", category: "Text" });
registerFunction("CHAR", (args) => {
  const code = numberArg(args, 0, 0);
  if (typeof code !== "number") return code;
  if (code < 1 || code > 0x10ffff) return ERR.value();
  return String.fromCodePoint(Math.trunc(code));
}, 1, 1, false, { signature: "CHAR(code)", category: "Text" });
registerFunction("UNICHAR", (args) => {
  const code = numberArg(args, 0, 0);
  if (typeof code !== "number") return code;
  if (code < 1 || code > 0x10ffff) return ERR.value();
  return String.fromCodePoint(Math.trunc(code));
}, 1, 1, false, { signature: "UNICHAR(code)", category: "Text" });
registerFunction("CODE", (args) => {
  const text = textArg(args, 0);
  if (text === "") return ERR.value();
  return text.codePointAt(0) ?? ERR.value();
}, 1, 1, false, { signature: "CODE(text)", category: "Text" });
registerFunction("UNICODE", (args) => {
  const text = textArg(args, 0);
  if (text === "") return ERR.value();
  return text.codePointAt(0) ?? ERR.value();
}, 1, 1, false, { signature: "UNICODE(text)", category: "Text" });
registerFunction("TEXT", (args) => {
  const value = args[0]?.[0]?.[0];
  const pattern = textArg(args, 1);
  if (isError(value)) return value;
  const number = toNumber(value);
  if (isError(number)) return toText(value);
  return formatNumber(number, pattern);
}, 2, 2, false, { signature: "TEXT(value, format)", category: "Text" });
registerFunction("VALUE", (args) => {
  const number = toNumber(args[0]?.[0]?.[0] ?? "");
  if (isError(number)) {
    const text = textArg(args, 0).replace(/\s/g, "");
    // Accept a trailing percent and a thousands separator.
    const percent = text.endsWith("%");
    const cleaned = percent ? text.slice(0, -1).replace(/,/g, "") : text.replace(/,/g, "");
    const parsed = Number(cleaned);
    if (!Number.isFinite(parsed)) return ERR.value();
    return percent ? parsed / 100 : parsed;
  }
  return number;
}, 1, 1, false, { signature: "VALUE(text)", category: "Text" });
registerFunction("NUMBERVALUE", (args) => {
  const raw = textArg(args, 0);
  const decimalSep = textArg(args, 1) || ".";
  // Only strip a group separator when it is not also the decimal separator;
  // otherwise `NUMBERVALUE("3.5", ".")` removed the decimal point itself.
  const groupArg = args[2] !== undefined ? textArg(args, 2) : decimalSep === "." ? "," : "";
  const groupSep = groupArg && groupArg !== decimalSep ? groupArg : "";
  let text = raw;
  if (groupSep) text = text.split(groupSep).join("");
  // Normalize a non-dot decimal separator to a dot for Number().
  if (decimalSep !== ".") text = text.split(decimalSep).join(".");
  const parsed = Number(text);
  return Number.isFinite(parsed) ? parsed : ERR.value();
}, 1, 3, false, { signature: "NUMBERVALUE(text, [decimal_sep], [group_sep])", category: "Text" });

/**
 * Shared implementation for TEXTBEFORE / TEXTAFTER.
 *
 * `instance` is 1-based and negative counts from the end, exactly as Excel
 * documents it, which is what makes `TEXTAFTER(A1, ".", -1)` return the
 * file extension.
 */
function relativeSplit(text: string, delimiter: string, instance: number, before: boolean): string | FormulaError {
  if (delimiter === "") return before ? "" : text;
  if (instance === 0) return ERR.value();
  const positions: number[] = [];
  let from = 0;
  while (true) {
    const index = text.indexOf(delimiter, from);
    if (index < 0) break;
    positions.push(index);
    from = index + delimiter.length;
  }
  if (positions.length === 0) return ERR.na();
  const target = instance > 0 ? instance - 1 : positions.length + instance;
  const position = positions[target];
  if (position === undefined) return ERR.na();
  return before ? text.slice(0, position) : text.slice(position + delimiter.length);
}

registerFunction("TEXTBEFORE", (args) => {
  const text = textArg(args, 0);
  const delimiter = textArg(args, 1);
  const instance = args[2] ? numberArg(args, 2, 1) : 1;
  if (typeof instance !== "number") return instance;
  return relativeSplit(text, delimiter, Math.trunc(instance), true);
}, 2, 3, false, { signature: "TEXTBEFORE(text, delimiter, [instance])", category: "Text" });
registerFunction("TEXTAFTER", (args) => {
  const text = textArg(args, 0);
  const delimiter = textArg(args, 1);
  const instance = args[2] ? numberArg(args, 2, 1) : 1;
  if (typeof instance !== "number") return instance;
  return relativeSplit(text, delimiter, Math.trunc(instance), false);
}, 2, 3, false, { signature: "TEXTAFTER(text, delimiter, [instance])", category: "Text" });
registerFunction("TEXTSPLIT", (args) => {
  const text = textArg(args, 0);
  const columnDelimiters = args[1] ? textArg(args, 1) : "";
  const rowDelimiter = args[2] ? textArg(args, 2) : "";
  if (columnDelimiters === "" && rowDelimiter === "") return ERR.value();

  const columnParts = columnDelimiters === "" ? [text] : splitOnAny(text, columnDelimiters);
  const rows = rowDelimiter === "" ? [columnParts] : splitOnAny(text, rowDelimiter).map((line) => (columnDelimiters === "" ? [line] : splitOnAny(line, columnDelimiters)));
  const width = Math.max(...rows.map((row) => row.length));
  return rows.map((row) => {
    const line = [...row];
    while (line.length < width) line.push("");
    return line;
  });
}, 2, 3, false, { signature: "TEXTSPLIT(text, col_delimiter, [row_delimiter])", category: "Text" });

/** Splits on any single character of `delimiters`, left to right. */
function splitOnAny(text: string, delimiters: string): string[] {
  const out: string[] = [];
  let current = "";
  for (const character of text) {
    if (delimiters.includes(character)) {
      out.push(current);
      current = "";
    } else {
      current += character;
    }
  }
  out.push(current);
  return out;
}
