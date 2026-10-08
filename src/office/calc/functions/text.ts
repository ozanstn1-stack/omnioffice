/**
 * Text functions: slicing, searching, joining and formatting.
 */
import { registerFunction } from "../registry";
import { formatNumber } from "../numberFormat";
import { ERR, FormulaError, flatten, isError, optionalBool, toNumber, toText, type Scalar } from "../scalars";

function textArg(args: Scalar[][][], index: number): string {
  return toText(args[index]?.[0]?.[0] ?? "");
}

function numberArg(args: Scalar[][][], index: number, fallback: number): number | FormulaError {
  if (args[index] === undefined) return fallback;
  const value = toNumber(args[index]?.[0]?.[0] ?? 0);
  return isError(value) ? value : value;
}

registerFunction("LEN", (args) => textArg(args, 0).length, 1, 1, false, { signature: "LEN(text)", category: "Text" });
registerFunction("TRIM", (args) => textArg(args, 0).trim().replace(/\s+/g, " "), 1, 1, false, {
  signature: "TRIM(text)",
  category: "Text",
});
// CLEAN() is defined as removing the first 32 non-printing characters (0-31),
// tab and line breaks included, so the control-character class is the function,
// not an accident.
// eslint-disable-next-line no-control-regex
registerFunction("CLEAN", (args) => textArg(args, 0).replace(/[\x00-\x1f]/g, ""), 1, 1, false, {
  signature: "CLEAN(text)",
  category: "Text",
});
registerFunction("UPPER", (args) => textArg(args, 0).toUpperCase(), 1, 1, false, {
  signature: "UPPER(text)",
  category: "Text",
});
registerFunction("LOWER", (args) => textArg(args, 0).toLowerCase(), 1, 1, false, {
  signature: "LOWER(text)",
  category: "Text",
});
registerFunction(
  "PROPER",
  (args) => textArg(args, 0).replace(/\w\S*/g, (word) => word.charAt(0).toUpperCase() + word.slice(1).toLowerCase()),
  1,
  1,
  false,
  { signature: "PROPER(text)", category: "Text" },
);
registerFunction(
  "LEFT",
  (args) => {
    const count = numberArg(args, 1, 1);
    if (typeof count !== "number") return count;
    if (count < 0) return ERR.value();
    return textArg(args, 0).slice(0, count);
  },
  1,
  2,
  false,
  { signature: "LEFT(text, [count])", category: "Text" },
);
registerFunction(
  "RIGHT",
  (args) => {
    const count = numberArg(args, 1, 1);
    if (typeof count !== "number") return count;
    if (count < 0) return ERR.value();
    return count === 0 ? "" : textArg(args, 0).slice(-count);
  },
  1,
  2,
  false,
  { signature: "RIGHT(text, [count])", category: "Text" },
);
registerFunction(
  "MID",
  (args) => {
    const start = numberArg(args, 1, 1);
    if (typeof start !== "number") return start;
    const count = numberArg(args, 2, 0);
    if (typeof count !== "number") return count;
    if (start < 1 || count < 0) return ERR.value();
    return textArg(args, 0).slice(start - 1, start - 1 + count);
  },
  3,
  3,
  false,
  { signature: "MID(text, start, count)", category: "Text" },
);
registerFunction("CONCAT", (args) => flatten(args).map(toText).join(""), 1, 64, false, {
  signature: "CONCAT(text1, ...)",
  category: "Text",
});
registerFunction("CONCATENATE", (args) => flatten(args).map(toText).join(""), 1, 64, false, {
  signature: "CONCATENATE(text1, ...)",
  category: "Text",
});
registerFunction(
  "TEXTJOIN",
  (args) => {
    const delimiter = textArg(args, 0);
    const skipEmpty = args[1] ? toText(args[1]?.[0]?.[0] ?? "") !== "FALSE" : true;
    const parts = flatten(args.slice(2))
      .map(toText)
      .filter((value) => (skipEmpty ? value !== "" : true));
    return parts.join(delimiter);
  },
  3,
  64,
  false,
  { signature: "TEXTJOIN(delimiter, ignore_empty, text1, ...)", category: "Text" },
);
registerFunction(
  "REPT",
  (args) => {
    const count = numberArg(args, 1, 0);
    if (typeof count !== "number") return count;
    if (count < 0) return ERR.value();
    // Bound the result so =REPT("x", 1E9) cannot exhaust memory.
    return textArg(args, 0).repeat(Math.min(Math.trunc(count), 32_767));
  },
  2,
  2,
  false,
  { signature: "REPT(text, count)", category: "Text" },
);
registerFunction(
  "SUBSTITUTE",
  (args) => {
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
  },
  3,
  4,
  false,
  { signature: "SUBSTITUTE(text, old, new, [instance])", category: "Text" },
);
registerFunction(
  "REPLACE",
  (args) => {
    const text = textArg(args, 0);
    const start = numberArg(args, 1, 1);
    if (typeof start !== "number") return start;
    const count = numberArg(args, 2, 0);
    if (typeof count !== "number") return count;
    if (start < 1 || count < 0) return ERR.value();
    return text.slice(0, start - 1) + textArg(args, 3) + text.slice(start - 1 + count);
  },
  4,
  4,
  false,
  { signature: "REPLACE(text, start, count, new)", category: "Text" },
);
registerFunction(
  "FIND",
  (args) => {
    const needle = textArg(args, 0);
    const haystack = textArg(args, 1);
    const startArg = numberArg(args, 2, 1);
    if (typeof startArg !== "number") return startArg;
    const start = Math.trunc(startArg);
    if (start < 1 || start > haystack.length + 1) return ERR.value();
    const index = haystack.indexOf(needle, start - 1);
    return index < 0 ? ERR.value() : index + 1;
  },
  2,
  3,
  false,
  { signature: "FIND(needle, text, [start])", category: "Text" },
);
registerFunction(
  "SEARCH",
  (args) => {
    const needle = textArg(args, 0);
    const haystack = textArg(args, 1);
    const startArg = numberArg(args, 2, 1);
    if (typeof startArg !== "number") return startArg;
    const start = Math.trunc(startArg);
    if (start < 1) return ERR.value();
    const index = haystack.toLowerCase().indexOf(needle.toLowerCase(), start - 1);
    return index < 0 ? ERR.value() : index + 1;
  },
  2,
  3,
  false,
  { signature: "SEARCH(needle, text, [start])", category: "Text" },
);
registerFunction("EXACT", (args) => textArg(args, 0) === textArg(args, 1), 2, 2, false, {
  signature: "EXACT(text1, text2)",
  category: "Text",
});
registerFunction(
  "CHAR",
  (args) => {
    const code = numberArg(args, 0, 0);
    if (typeof code !== "number") return code;
    if (code < 1 || code > 0x10ffff) return ERR.value();
    return String.fromCodePoint(Math.trunc(code));
  },
  1,
  1,
  false,
  { signature: "CHAR(code)", category: "Text" },
);
registerFunction(
  "UNICHAR",
  (args) => {
    const code = numberArg(args, 0, 0);
    if (typeof code !== "number") return code;
    const point = Math.trunc(code);
    if (point < 1 || point > 0x10ffff) return ERR.value();
    // Half of a surrogate pair is not a character on its own.
    if (point >= 0xd800 && point <= 0xdfff) return ERR.na();
    return String.fromCodePoint(point);
  },
  1,
  1,
  false,
  { signature: "UNICHAR(code)", category: "Text" },
);
registerFunction(
  "CODE",
  (args) => {
    const text = textArg(args, 0);
    if (text === "") return ERR.value();
    return text.codePointAt(0) ?? ERR.value();
  },
  1,
  1,
  false,
  { signature: "CODE(text)", category: "Text" },
);
registerFunction(
  "UNICODE",
  (args) => {
    const text = textArg(args, 0);
    if (text === "") return ERR.value();
    return text.codePointAt(0) ?? ERR.value();
  },
  1,
  1,
  false,
  { signature: "UNICODE(text)", category: "Text" },
);
registerFunction(
  "TEXT",
  (args) => {
    const value = args[0]?.[0]?.[0];
    const pattern = textArg(args, 1);
    if (isError(value)) return value;
    const number = toNumber(value);
    if (isError(number)) return toText(value);
    return formatNumber(number, pattern);
  },
  2,
  2,
  false,
  { signature: "TEXT(value, format)", category: "Text" },
);
registerFunction(
  "VALUE",
  (args) => {
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
  },
  1,
  1,
  false,
  { signature: "VALUE(text)", category: "Text" },
);
/**
 * Reads text as a number with the separators given, the way NUMBERVALUE does:
 * spaces are ignored anywhere, every trailing `%` divides by 100, a group
 * separator is dropped (but not after the decimal one), and a second decimal
 * separator or anything else that is not a number is `#VALUE!`.
 */
function parseNumberText(raw: string, decimalSep: string, groupSep: string): number | FormulaError {
  let text = raw.replace(/\s/g, "");
  if (text === "") return 0;
  let percents = 0;
  while (text.endsWith("%")) {
    text = text.slice(0, -1);
    percents += 1;
  }
  if (text.includes("%")) return ERR.value();
  const decimalAt = text.indexOf(decimalSep);
  if (decimalAt >= 0 && text.indexOf(decimalSep, decimalAt + 1) >= 0) return ERR.value();
  if (groupSep) {
    if (decimalAt >= 0 && text.lastIndexOf(groupSep) > decimalAt) return ERR.value();
    text = text.split(groupSep).join("");
  }
  if (decimalSep !== ".") {
    // A stray dot would otherwise be read as the decimal point.
    if (text.includes(".")) return ERR.value();
    text = text.replace(decimalSep, ".");
  }
  if (!/^[+-]?(\d+\.?\d*|\.\d+)(e[+-]?\d+)?$/i.test(text)) return ERR.value();
  return Number(text) / 100 ** percents;
}

registerFunction(
  "NUMBERVALUE",
  (args) => {
    // Only the first character of a separator counts.
    const decimalSep = textArg(args, 1).charAt(0) || ".";
    // Only strip a group separator when it is not also the decimal separator;
    // otherwise `NUMBERVALUE("3.5", ".")` removed the decimal point itself.
    const groupArg = textArg(args, 2).charAt(0) || (decimalSep === "." ? "," : "");
    return parseNumberText(textArg(args, 0), decimalSep, groupArg === decimalSep ? "" : groupArg);
  },
  1,
  3,
  false,
  { signature: "NUMBERVALUE(text, [decimal_separator], [group_separator])", category: "Text" },
);

/**
 * |value| rounded half away from zero at `decimals` places, as digit strings.
 * It works on the 15 significant digits Excel keeps, so 2.675 gives 2.68 as it
 * does in a worksheet, not the 2.67 its binary expansion would.
 */
function roundDecimal(value: number, decimals: number): { whole: string; fraction: string } {
  const [mantissa, exponent] = Math.abs(value).toExponential(14).split("e");
  const digits = mantissa.replace(".", "");
  // The value is 0.d1d2d3... x 10^pointAt; `keep` of those digits survive.
  const keep = Number(exponent) + 1 + decimals;
  let scaled: bigint;
  if (value === 0 || keep < 0) scaled = 0n;
  else if (keep >= digits.length) scaled = BigInt(digits + "0".repeat(keep - digits.length));
  else scaled = BigInt(digits.slice(0, keep) || "0") + (digits[keep] >= "5" ? 1n : 0n);
  if (decimals < 0) return { whole: (scaled * 10n ** BigInt(-decimals)).toString(), fraction: "" };
  const text = scaled.toString().padStart(decimals + 1, "0");
  return { whole: text.slice(0, text.length - decimals), fraction: text.slice(text.length - decimals) };
}

/** Number text for DOLLAR and FIXED. A value that rounds to zero carries no sign. */
function fixedNumber(args: Scalar[][][], commas: boolean): { text: string; negative: boolean } | FormulaError {
  const value = toNumber(args[0]?.[0]?.[0] ?? "");
  if (isError(value)) return value;
  const decimalsArg = args[1]?.[0]?.[0];
  const decimals = decimalsArg === undefined || decimalsArg === "" ? 2 : toNumber(decimalsArg);
  if (isError(decimals)) return decimals;
  const places = Math.trunc(decimals);
  if (Math.abs(places) > 127) return ERR.value();
  const { whole, fraction } = roundDecimal(value, places);
  const grouped = commas ? whole.replace(/\B(?=(\d{3})+(?!\d))/g, ",") : whole;
  const isZero = /^0*$/.test(whole + fraction);
  return { text: fraction ? `${grouped}.${fraction}` : grouped, negative: value < 0 && !isZero };
}

registerFunction(
  "DOLLAR",
  (args) => {
    const fixed = fixedNumber(args, true);
    if (isError(fixed)) return fixed;
    // The en-US currency format: a dollar sign, and parentheses for a loss.
    return fixed.negative ? `($${fixed.text})` : `$${fixed.text}`;
  },
  1,
  2,
  false,
  { signature: "DOLLAR(number, [decimals])", category: "Text" },
);
registerFunction(
  "FIXED",
  (args) => {
    const noCommas = optionalBool(args[2]?.[0]?.[0], false);
    if (isError(noCommas)) return noCommas;
    const fixed = fixedNumber(args, !noCommas);
    if (isError(fixed)) return fixed;
    return fixed.negative ? `-${fixed.text}` : fixed.text;
  },
  1,
  3,
  false,
  { signature: "FIXED(number, [decimals], [no_commas])", category: "Text" },
);
registerFunction(
  "ARRAYTOTEXT",
  (args) => {
    const formatArg = args[1]?.[0]?.[0];
    const format = formatArg === undefined || formatArg === "" ? 0 : toNumber(formatArg);
    if (isError(format)) return format;
    if (format !== 0 && format !== 1) return ERR.value();
    const matrix = args[0] ?? [];
    // Concise: the values as they read, in one list. Strict: the array as a
    // formula would write it, with text in quotes.
    const render = (value: Scalar) =>
      format === 1 && typeof value === "string" ? `"${value.replace(/"/g, '""')}"` : toText(value);
    if (format === 0) return flatten([matrix]).map(render).join(", ");
    return `{${matrix.map((row) => row.map(render).join(",")).join(";")}}`;
  },
  1,
  2,
  true,
  { signature: "ARRAYTOTEXT(array, [format])", category: "Text" },
);

/** An optional whole-number argument; an empty slot reads as `fallback`. */
function optionalInteger(args: Scalar[][][], index: number, fallback: number): number | FormulaError {
  const arg = args[index]?.[0]?.[0];
  if (arg === undefined || arg === "") return fallback;
  const value = toNumber(arg);
  return isError(value) ? value : Math.trunc(value);
}

/**
 * Shared implementation for TEXTBEFORE / TEXTAFTER.
 *
 * `instance_num` is 1-based and negative counts from the end, exactly as Excel
 * documents it, which is what makes `TEXTAFTER(A1, ".", -1)` return the file
 * extension. `match_mode` 1 ignores case; `match_end` 1 treats the end of the
 * text (the start, when counting backwards) as one more delimiter; the
 * `if_not_found` value, rather than `#N/A`, answers when there is no match.
 */
function relativeSplit(args: Scalar[][][], before: boolean): Scalar {
  const text = textArg(args, 0);
  const delimiter = textArg(args, 1);
  const instance = optionalInteger(args, 2, 1);
  if (isError(instance)) return instance;
  const mode = optionalInteger(args, 3, 0);
  if (isError(mode)) return mode;
  const matchEnd = optionalInteger(args, 4, 0);
  if (isError(matchEnd)) return matchEnd;
  if ((mode !== 0 && mode !== 1) || (matchEnd !== 0 && matchEnd !== 1)) return ERR.value();
  if (instance === 0 || Math.abs(instance) > text.length) return ERR.value();

  const matches: Array<{ start: number; end: number }> = [];
  if (delimiter === "") {
    // An empty delimiter sits at the start, or at the end when counting back.
    matches.push(instance > 0 ? { start: 0, end: 0 } : { start: text.length, end: text.length });
  } else {
    const haystack = mode === 1 ? text.toLowerCase() : text;
    const needle = mode === 1 ? delimiter.toLowerCase() : delimiter;
    for (let from = haystack.indexOf(needle); from >= 0; from = haystack.indexOf(needle, from + needle.length)) {
      matches.push({ start: from, end: from + needle.length });
    }
  }
  if (matchEnd === 1) {
    if (instance > 0) matches.push({ start: text.length, end: text.length });
    else matches.unshift({ start: 0, end: 0 });
  }
  const found = matches[instance > 0 ? instance - 1 : matches.length + instance];
  if (!found) return args.length > 5 ? (args[5][0]?.[0] ?? "") : ERR.na();
  return before ? text.slice(0, found.start) : text.slice(found.end);
}

registerFunction("TEXTBEFORE", (args) => relativeSplit(args, true), 2, 6, false, {
  signature: "TEXTBEFORE(text, delimiter, [instance_num], [match_mode], [match_end], [if_not_found])",
  category: "Text",
});
registerFunction("TEXTAFTER", (args) => relativeSplit(args, false), 2, 6, false, {
  signature: "TEXTAFTER(text, delimiter, [instance_num], [match_mode], [match_end], [if_not_found])",
  category: "Text",
});
registerFunction(
  "TEXTSPLIT",
  (args) => {
    const text = textArg(args, 0);
    const columnDelimiters = args[1] ? textArg(args, 1) : "";
    const rowDelimiter = args[2] ? textArg(args, 2) : "";
    if (columnDelimiters === "" && rowDelimiter === "") return ERR.value();

    const columnParts = columnDelimiters === "" ? [text] : splitOnAny(text, columnDelimiters);
    const rows =
      rowDelimiter === ""
        ? [columnParts]
        : splitOnAny(text, rowDelimiter).map((line) =>
            columnDelimiters === "" ? [line] : splitOnAny(line, columnDelimiters),
          );
    const width = Math.max(...rows.map((row) => row.length));
    return rows.map((row) => {
      const line = [...row];
      while (line.length < width) line.push("");
      return line;
    });
  },
  2,
  3,
  false,
  { signature: "TEXTSPLIT(text, col_delimiter, [row_delimiter])", category: "Text" },
);

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
