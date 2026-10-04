/**
 * Date and time functions.
 *
 * Dates are Excel serial numbers (days since 1899-12-30) so they stay numeric
 * and can be subtracted; `serialToDate` / `dateToSerial` in `numberFormat.ts`
 * are the single conversion point.
 */
import { dateToSerial, serialDay, serialToDate } from "../numberFormat";
import { registerFunction } from "../registry";
import { ERR, FormulaError, flatten, isError, toNumber, type Scalar } from "../scalars";

/** A date argument that may be a serial number or an ISO-8601 string. */
function dateArg(args: Scalar[][][], index: number): Date | FormulaError {
  if (args[index] === undefined) return ERR.value();
  const raw = args[index]?.[0]?.[0];
  if (typeof raw === "string" && raw.trim() !== "" && !Number.isFinite(Number(raw))) {
    const parsed = new Date(raw.trim());
    if (!Number.isNaN(parsed.getTime())) return parsed;
  }
  const number = toNumber(raw ?? 0);
  if (isError(number)) return number;
  const date = serialToDate(number);
  return date ?? ERR.num();
}

function numberArg(args: Scalar[][][], index: number, fallback: number): number | FormulaError {
  if (args[index] === undefined) return fallback;
  const value = toNumber(args[index]?.[0]?.[0] ?? 0);
  return isError(value) ? value : value;
}

function utc(date: Date): Date {
  return new Date(Date.UTC(date.getUTCFullYear(), date.getUTCMonth(), date.getUTCDate()));
}

function addMonths(date: Date, months: number): Date {
  const day = date.getUTCDate();
  const shifted = new Date(Date.UTC(date.getUTCFullYear(), date.getUTCMonth() + months, 1));
  // Clamp to the last valid day: 31 Jan plus one month is 28/29 Feb, not 3 Mar.
  const lastDay = new Date(Date.UTC(shifted.getUTCFullYear(), shifted.getUTCMonth() + 1, 0)).getUTCDate();
  shifted.setUTCDate(Math.min(day, lastDay));
  return shifted;
}

function addDays(date: Date, days: number): Date {
  return new Date(date.getTime() + days * 86_400_000);
}

/** Monday = 0 ... Sunday = 6, the basis for both weekday functions. */
function weekdayIndex(date: Date): number {
  return (date.getUTCDay() + 6) % 7;
}

registerFunction("TODAY", () => Math.floor(Date.now() / 86_400_000) + 25_569, 0, 0, false, {
  signature: "TODAY()",
  category: "Date",
});
registerFunction("NOW", () => Date.now() / 86_400_000 + 25_569, 0, 0, false, { signature: "NOW()", category: "Date" });
registerFunction(
  "DATE",
  (args) => {
    const year = numberArg(args, 0, 0);
    if (isError(year)) return year;
    const month = numberArg(args, 1, 1);
    if (isError(month)) return month;
    const day = numberArg(args, 2, 1);
    if (isError(day)) return day;
    // Excel maps month 0 to December of the previous year and month 13 to January
    // of the next one; Date.UTC does the same through its overflow handling.
    const base = new Date(Date.UTC(year, month - 1, day));
    return isNaN(base.getTime()) ? ERR.num() : dateToSerial(base);
  },
  3,
  3,
  false,
  { signature: "DATE(year, month, day)", category: "Date" },
);
registerFunction(
  "TIME",
  (args) => {
    const hours = numberArg(args, 0, 0);
    if (isError(hours)) return hours;
    const minutes = numberArg(args, 1, 0);
    if (isError(minutes)) return minutes;
    const seconds = numberArg(args, 2, 0);
    if (isError(seconds)) return seconds;
    // Excel's documented limits: the hour may not exceed 32767 and negative
    // components are rejected; everything else wraps modulo 24h.
    if (hours < 0 || minutes < 0 || seconds < 0 || hours > 32767) return ERR.num();
    const totalSeconds = Math.trunc(hours) * 3600 + Math.trunc(minutes) * 60 + Math.trunc(seconds);
    return (totalSeconds % 86_400) / 86_400;
  },
  3,
  3,
  false,
  { signature: "TIME(hour, minute, second)", category: "Date" },
);
registerFunction(
  "DATEVALUE",
  (args) => {
    const raw = args[0]?.[0]?.[0];
    const text = String(raw ?? "").trim();
    if (text === "") return ERR.value();
    const parsed = new Date(text);
    if (Number.isNaN(parsed.getTime())) return ERR.value();
    return dateToSerial(utc(parsed));
  },
  1,
  1,
  false,
  { signature: "DATEVALUE(text)", category: "Date" },
);
registerFunction(
  "TIMEVALUE",
  (args) => {
    const match = /^(\d{1,2}):(\d{2})(?::(\d{2}(?:\.\d+)?))?$/.exec(String(args[0]?.[0]?.[0] ?? "").trim());
    if (!match) return ERR.value();
    return (Number(match[1]) * 3600 + Number(match[2]) * 60 + Number(match[3] ?? 0)) / 86_400;
  },
  1,
  1,
  false,
  { signature: "TIMEVALUE(text)", category: "Date" },
);
registerFunction(
  "YEAR",
  (args) => {
    const date = dateArg(args, 0);
    return !isError(date) ? date.getUTCFullYear() : date;
  },
  1,
  1,
  false,
  { signature: "YEAR(serial)", category: "Date" },
);
registerFunction(
  "MONTH",
  (args) => {
    const date = dateArg(args, 0);
    return !isError(date) ? date.getUTCMonth() + 1 : date;
  },
  1,
  1,
  false,
  { signature: "MONTH(serial)", category: "Date" },
);
registerFunction(
  "DAY",
  (args) => {
    const date = dateArg(args, 0);
    return !isError(date) ? date.getUTCDate() : date;
  },
  1,
  1,
  false,
  { signature: "DAY(serial)", category: "Date" },
);
registerFunction(
  "HOUR",
  (args) => {
    const date = dateArg(args, 0);
    return !isError(date) ? date.getUTCHours() : date;
  },
  1,
  1,
  false,
  { signature: "HOUR(serial)", category: "Date" },
);
registerFunction(
  "MINUTE",
  (args) => {
    const date = dateArg(args, 0);
    return !isError(date) ? date.getUTCMinutes() : date;
  },
  1,
  1,
  false,
  { signature: "MINUTE(serial)", category: "Date" },
);
registerFunction(
  "SECOND",
  (args) => {
    const date = dateArg(args, 0);
    return !isError(date) ? date.getUTCSeconds() : date;
  },
  1,
  1,
  false,
  { signature: "SECOND(serial)", category: "Date" },
);
registerFunction(
  "WEEKDAY",
  (args) => {
    const date = dateArg(args, 0);
    if (isError(date)) return date;
    const type = numberArg(args, 1, 1);
    if (isError(type)) return type;
    const index = weekdayIndex(date);
    if (Math.trunc(type) === 1) return index + 1;
    if (Math.trunc(type) === 2) return index === 0 ? 7 : index;
    if (Math.trunc(type) === 3) return index === 0 ? 6 : index - 1;
    return index + 1;
  },
  1,
  2,
  false,
  { signature: "WEEKDAY(serial, [type])", category: "Date" },
);
/** ISO week: the week containing the first Thursday of the year. */
function isoWeekNumber(date: Date): number {
  const thursday = new Date(date.getTime());
  thursday.setUTCDate(thursday.getUTCDate() + (3 - weekdayIndex(date)));
  const firstThursday = new Date(Date.UTC(thursday.getUTCFullYear(), 0, 4));
  firstThursday.setUTCDate(firstThursday.getUTCDate() + (3 - weekdayIndex(firstThursday)));
  return Math.round((thursday.getTime() - firstThursday.getTime()) / (7 * 86_400_000)) + 1;
}

registerFunction(
  "ISOWEEKNUM",
  (args) => {
    const date = dateArg(args, 0);
    if (isError(date)) return date;
    return isoWeekNumber(date);
  },
  1,
  1,
  false,
  { signature: "ISOWEEKNUM(serial)", category: "Date" },
);
registerFunction(
  "WEEKNUM",
  (args) => {
    const date = dateArg(args, 0);
    if (isError(date)) return date;
    const type = numberArg(args, 1, 1);
    if (isError(type)) return type;
    const scheme = Math.trunc(type);
    const day = utc(date);
    if (scheme === 21) return isoWeekNumber(day);
    // Excel schemes: 1 = Sunday, 2 = Monday, 11..17 = Monday..Sunday.
    const startDay = scheme === 1 ? 0 : scheme === 2 ? 1 : scheme >= 11 && scheme <= 17 ? (scheme - 10) % 7 : null;
    if (startDay === null) return ERR.num();
    // Week 1 is the week that contains January 1, so the count starts from that
    // week's start day, which can be in the previous calendar year.
    const januaryFirst = new Date(Date.UTC(day.getUTCFullYear(), 0, 1));
    const offset = (januaryFirst.getUTCDay() - startDay + 7) % 7;
    return Math.floor((dateToSerial(day) - dateToSerial(januaryFirst) + offset) / 7) + 1;
  },
  1,
  2,
  false,
  { signature: "WEEKNUM(serial, [type])", category: "Date" },
);
registerFunction(
  "DAYS",
  (args) => {
    const end = dateArg(args, 0);
    if (isError(end)) return end;
    const start = dateArg(args, 1);
    if (isError(start)) return start;
    return Math.round((utc(end).getTime() - utc(start).getTime()) / 86_400_000);
  },
  2,
  2,
  false,
  { signature: "DAYS(end, start)", category: "Date" },
);
registerFunction(
  "DATEDIF",
  (args) => {
    const start = dateArg(args, 0);
    if (isError(start)) return start;
    const end = dateArg(args, 1);
    if (isError(end)) return end;
    const unit = String(args[2]?.[0]?.[0] ?? "D").toUpperCase();
    const from = utc(start);
    const to = utc(end);
    if (to < from) return ERR.num();
    switch (unit) {
      case "D":
        return Math.round((to.getTime() - from.getTime()) / 86_400_000);
      case "M": {
        let months = (to.getUTCFullYear() - from.getUTCFullYear()) * 12 + (to.getUTCMonth() - from.getUTCMonth());
        if (to.getUTCDate() < from.getUTCDate()) months -= 1;
        return months;
      }
      case "Y": {
        let years = to.getUTCFullYear() - from.getUTCFullYear();
        if (
          to.getUTCMonth() < from.getUTCMonth() ||
          (to.getUTCMonth() === from.getUTCMonth() && to.getUTCDate() < from.getUTCDate())
        )
          years -= 1;
        return years;
      }
      default:
        return ERR.num();
    }
  },
  3,
  3,
  false,
  { signature: "DATEDIF(start, end, unit)", category: "Date" },
);
registerFunction(
  "EDATE",
  (args) => {
    const start = dateArg(args, 0);
    if (isError(start)) return start;
    const months = numberArg(args, 1, 0);
    if (isError(months)) return months;
    return dateToSerial(addMonths(utc(start), Math.trunc(months)));
  },
  2,
  2,
  false,
  { signature: "EDATE(start, months)", category: "Date" },
);
registerFunction(
  "EOMONTH",
  (args) => {
    const start = dateArg(args, 0);
    if (isError(start)) return start;
    const months = numberArg(args, 1, 0);
    if (isError(months)) return months;
    const shifted = addMonths(utc(start), Math.trunc(months));
    const lastDay = new Date(Date.UTC(shifted.getUTCFullYear(), shifted.getUTCMonth() + 1, 0));
    return dateToSerial(lastDay);
  },
  2,
  2,
  false,
  { signature: "EOMONTH(start, months)", category: "Date" },
);

function holidaySet(args: Scalar[][][]): Set<number> {
  const out = new Set<number>();
  for (const value of flatten([args[0] ?? []])) {
    const number = toNumber(value);
    if (!isError(number)) out.add(serialDay(number));
  }
  return out;
}

registerFunction(
  "NETWORKDAYS",
  (args) => {
    const start = dateArg(args, 0);
    if (isError(start)) return start;
    const end = dateArg(args, 1);
    if (isError(end)) return end;
    const holidays = holidaySet(args.slice(2));
    let from = utc(start);
    let to = utc(end);
    let sign = 1;
    if (from > to) {
      [from, to] = [to, from];
      sign = -1;
    }
    let count = 0;
    for (let day = from.getTime(); day <= to.getTime(); day += 86_400_000) {
      const date = new Date(day);
      // A weekend and a listed holiday are two independent reasons to skip; one
      // `||` mistake here silently counts public holidays as working days.
      if (weekdayIndex(date) > 4) continue;
      if (holidays.has(serialDay(dateToSerial(date)))) continue;
      count += 1;
    }
    return count * sign;
  },
  2,
  3,
  false,
  { signature: "NETWORKDAYS(start, end, [holidays])", category: "Date" },
);
registerFunction(
  "WORKDAY",
  (args) => {
    const start = dateArg(args, 0);
    if (isError(start)) return start;
    const days = numberArg(args, 1, 0);
    if (isError(days)) return days;
    const holidays = holidaySet(args.slice(2));
    const step = days >= 0 ? 1 : -1;
    let remaining = Math.abs(Math.trunc(days));
    let cursor = utc(start);
    while (remaining > 0) {
      cursor = addDays(cursor, step);
      if (weekdayIndex(cursor) > 4) continue;
      if (holidays.has(serialDay(dateToSerial(cursor)))) continue;
      remaining -= 1;
    }
    return dateToSerial(cursor);
  },
  2,
  3,
  false,
  { signature: "WORKDAY(start, days, [holidays])", category: "Date" },
);

/** Converts a date argument straight to a serial number. */
export function toSerial(args: Scalar[][][], index: number): number | FormulaError {
  const date = dateArg(args, index);
  return !isError(date) ? dateToSerial(utc(date)) : date;
}
