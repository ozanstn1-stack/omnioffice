/**
 * Number-format rendering for Calc cells.
 *
 * Handles the subset of Excel number-format codes the ribbon exposes (General,
 * fixed decimals, percent, currency, thousands separators, accounting
 * parentheses and date/time patterns) plus the serial-date conversion the
 * financial and date functions share.
 */

/** Days between the 1899-12-30 epoch and 1970-01-01, per the Excel spec. */
export const SERIAL_EPOCH_OFFSET = 25_569;

/** A date/time that a serial number represents, or null when out of range. */
export function serialToDate(value: number): Date | null {
  if (!Number.isFinite(value) || value < -69_217 || value > 2_958_465) return null;
  return new Date(Math.round((value - SERIAL_EPOCH_OFFSET) * 86_400_000));
}

/** Inverse of `serialToDate`. */
export function dateToSerial(date: Date): number {
  return date.getTime() / 86_400_000 + SERIAL_EPOCH_OFFSET;
}

/** Whole days between two serial dates, ignoring the time of day. */
export function serialDay(serial: number): number {
  return Math.floor(serial);
}

export function formatPlainNumber(value: number): string {
  if (Number.isInteger(value)) return String(value);
  return String(Number(value.toFixed(10)));
}

export function formatNumber(value: number, format: string): string {
  const trimmed = format.trim();
  if (!trimmed || trimmed.toLowerCase() === "general") return formatPlainNumber(value);
  const datePattern = /(yyyy|yy|dd|mm|hh|ss|mmm)/i.test(trimmed) && !/[#0]/.test(trimmed);
  if (datePattern) return formatDateSerial(value, trimmed);
  const percent = trimmed.includes("%");
  const currency = /[$€£₺]/.exec(trimmed)?.[0] ?? "";
  const decimalsMatch = /\.(0+)/.exec(trimmed);
  const decimals = decimalsMatch ? decimalsMatch[1].length : 0;
  const grouped = trimmed.includes(",");
  let display = percent ? value * 100 : value;
  const sign = display < 0 ? "-" : "";
  display = Math.abs(display);
  let text = display.toFixed(decimals);
  if (grouped) {
    const [whole, fraction] = text.split(".");
    text = whole.replace(/\B(?=(\d{3})+(?!\d))/g, ",") + (fraction ? `.${fraction}` : "");
  }
  const negativePattern = trimmed.includes("(") && trimmed.includes(")");
  const suffix = percent ? "%" : "";
  if (sign === "-" && negativePattern) return `(${currency}${text}${suffix})`;
  return `${sign}${currency}${text}${suffix}`;
}

const MONTH_NAMES = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

const DATE_TOKEN = /yyyy|yy|mmm|mm|m|dd|hh|h|ss|s/gi;

function formatDateSerial(value: number, pattern: string): string {
  const date = serialToDate(value);
  if (!date) return formatPlainNumber(value);
  const pad = (input: number) => String(input).padStart(2, "0");
  // Tokenize in one pass and disambiguate `m`/`mm` by position: minutes when
  // the previous token was an hour or the next token is seconds, month
  // otherwise. The old ordered `.replace()` chain substituted the month into
  // `hh:mm` (and the minute branch could then never match), so every time
  // format showed the month number as minutes.
  const parts: { text: string; index: number }[] = [];
  DATE_TOKEN.lastIndex = 0;
  for (let match = DATE_TOKEN.exec(pattern); match; match = DATE_TOKEN.exec(pattern)) {
    parts.push({ text: match[0].toLowerCase(), index: match.index });
  }
  let result = "";
  let cursor = 0;
  for (let index = 0; index < parts.length; index += 1) {
    const token = parts[index];
    result += pattern.slice(cursor, token.index);
    cursor = token.index + token.text.length;
    const previous = parts[index - 1]?.text;
    const next = parts[index + 1]?.text;
    switch (token.text) {
      case "yyyy":
        result += String(date.getUTCFullYear());
        break;
      case "yy":
        result += String(date.getUTCFullYear()).slice(-2);
        break;
      case "mmm":
        result += MONTH_NAMES[date.getUTCMonth()];
        break;
      case "m":
      case "mm": {
        const isMinutes = previous === "hh" || previous === "h" || next === "ss" || next === "s";
        const minutes = isMinutes ? date.getUTCMinutes() : date.getUTCMonth() + 1;
        result += token.text === "mm" ? pad(minutes) : String(minutes);
        break;
      }
      case "dd":
        result += pad(date.getUTCDate());
        break;
      case "h":
      case "hh":
        result += token.text === "hh" ? pad(date.getUTCHours()) : String(date.getUTCHours());
        break;
      case "s":
      case "ss":
        result += token.text === "ss" ? pad(date.getUTCSeconds()) : String(date.getUTCSeconds());
        break;
      default:
        result += token.text;
        break;
    }
  }
  return result + pattern.slice(cursor);
}
