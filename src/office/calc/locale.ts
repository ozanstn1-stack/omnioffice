/**
 * Locale-aware text, number and date handling for Calc.
 *
 * The module keeps the active locale of the open workbook (the editor sets it
 * from the settings; "auto" follows the interface language), so the text
 * functions, typed input and the formatter do not need a locale argument
 * threaded through every call. Turkish differs from the default rules where it
 * matters: the i/İ and I/ı case pairs, `.`/`,` separators and DD.MM.YYYY dates.
 */
export type CalcLocale = "tr" | "en";

/** The settings value: "auto" follows the interface language. */
export type CalcLocaleSetting = "auto" | CalcLocale;

let active: CalcLocale = "en";

/** Sets the locale for the next evaluations (the editor calls this on mount). */
export function setCalcLocale(locale: CalcLocale): void {
  active = locale;
}

export function getCalcLocale(): CalcLocale {
  return active;
}

/** Resolves the settings value against the interface language. */
export function resolveCalcLocale(setting: CalcLocaleSetting | undefined, uiLanguage: string): CalcLocale {
  if (setting === "tr" || setting === "en") return setting;
  return uiLanguage === "tr" ? "tr" : "en";
}

/** UPPER with the Turkish dotted/dotless i pairs. */
export function upperLocale(text: string, locale: CalcLocale = active): string {
  if (locale !== "tr") return text.toUpperCase();
  return text.replace(/i/g, "İ").replace(/ı/g, "I").toUpperCase();
}

/** LOWER with the Turkish dotted/dotless i pairs. */
export function lowerLocale(text: string, locale: CalcLocale = active): string {
  if (locale !== "tr") return text.toLowerCase();
  return text.replace(/I/g, "ı").replace(/İ/g, "i").toLowerCase();
}

/** PROPER: each word starts upper-case, the rest lower-case, per locale. */
export function properLocale(text: string, locale: CalcLocale = active): string {
  // Unicode-aware word boundaries: `\w` is ASCII-only and would leave a
  // leading `ş`/`ğ` outside the word.
  return text.replace(
    /[\p{L}\p{N}_]+/gu,
    (word) => upperLocale(word.charAt(0), locale) + lowerLocale(word.slice(1), locale),
  );
}

/**
 * Parses a typed number for the locale. Accepts currency symbols/words
 * (`₺1.250`, `1.250 TL`, `$1,250`), percent signs (`%18`, `18%`) and negative
 * forms (`-25,50`, `(1.234,56)`). With both separators present the last one is
 * the decimal separator; with a single one the locale decides:
 *   - tr: `1.234` is 1234 (thousands), `1,5` is 1.5 (decimal comma);
 *   - en: `1,234` is 1234 (thousands), `1.5` is 1.5.
 * `null` when no number can be read.
 */
export function parseLocalizedNumber(input: string, locale: CalcLocale = active): number | null {
  let text = input.trim();
  if (text === "") return null;
  // A percent sign means "per hundred": "18%" and "%18" are 0.18, as in every
  // spreadsheet. The sign used to be stripped and the value left as 18.
  const percent = text.startsWith("%") || text.endsWith("%");
  if (text.startsWith("%")) text = text.slice(1).trim();
  if (text.endsWith("%")) text = text.slice(0, -1).trim();
  text = text
    .replace(/[₺$€£]/g, "")
    .replace(/\s*(tl|try|usd|eur|gbp)\s*$/i, "")
    .trim();
  if (text === "" || !/\d/.test(text)) return null;
  // Only a leading sign (or surrounding parentheses) is allowed: an internal
  // `+`/`-` means the input is an expression, not a number.
  const parenthesized = /^\([0-9.,\s]+\)$/.test(text);
  const signed = /^[+-]?[0-9.,\s]+$/.test(text);
  if (!parenthesized && !signed) return null;
  const negative = parenthesized || text.startsWith("-");
  const body = text.replace(/[()\s+-]/g, "");
  if (!/^[0-9.,]+$/.test(body) || !/\d/.test(body)) return null;

  const lastDot = body.lastIndexOf(".");
  const lastComma = body.lastIndexOf(",");
  let normalized: string;
  if (lastDot >= 0 && lastComma >= 0) {
    normalized = lastDot > lastComma ? body.replace(/,/g, "") : body.replace(/\./g, "").replace(",", ".");
  } else if (lastComma >= 0) {
    if (locale === "tr") {
      normalized = body.replace(",", ".");
    } else {
      const after = body.length - lastComma - 1;
      normalized = after === 3 ? body.replace(/,/g, "") : body.replace(",", ".");
    }
  } else if (lastDot >= 0) {
    if (locale === "tr") {
      const after = body.length - lastDot - 1;
      const before = body.slice(0, lastDot);
      normalized = after === 3 && before !== "" && before !== "0" ? body.replace(/\./g, "") : body;
    } else {
      normalized = body;
    }
  } else {
    normalized = body;
  }
  const value = Number(normalized);
  if (!Number.isFinite(value)) return null;
  const amount = negative ? -value : value;
  return percent ? amount / 100 : amount;
}

/** A date a user typed, before it becomes a serial number. */
export interface LocalizedDateParts {
  year: number;
  month: number;
  day: number;
  /** The number format the cell should carry so the date shows as typed. */
  numberFormat: string;
}

/**
 * Reads a date typed in the locale's shape (`11.10.2026` for tr,
 * `10/11/2026` for en; ISO `2026-10-11` in both) and validates it against the
 * calendar. `null` for anything else, including impossible dates.
 */
export function parseLocalizedDateParts(input: string, locale: CalcLocale = active): LocalizedDateParts | null {
  const text = input.trim();
  let year: number;
  let month: number;
  let day: number;
  const iso = /^(\d{4})-(\d{1,2})-(\d{1,2})$/.exec(text);
  const dotted = /^(\d{1,2})\.(\d{1,2})\.(\d{4})$/.exec(text);
  const slashed = /^(\d{1,2})\/(\d{1,2})\/(\d{4})$/.exec(text);
  if (iso) {
    year = Number(iso[1]);
    month = Number(iso[2]);
    day = Number(iso[3]);
  } else if (locale === "tr" && dotted) {
    day = Number(dotted[1]);
    month = Number(dotted[2]);
    year = Number(dotted[3]);
  } else if (locale === "en" && slashed) {
    month = Number(slashed[1]);
    day = Number(slashed[2]);
    year = Number(slashed[3]);
  } else {
    return null;
  }
  const date = new Date(year, month - 1, day);
  if (date.getFullYear() !== year || date.getMonth() !== month - 1 || date.getDate() !== day) return null;
  return { year, month, day, numberFormat: locale === "tr" ? "DD.MM.YYYY" : "MM/DD/YYYY" };
}
