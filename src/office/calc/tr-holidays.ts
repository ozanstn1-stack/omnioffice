/**
 * Turkish official holidays for the `WORKDAY` / `NETWORKDAYS` `"TR"` flag.
 *
 * Civil holidays are fixed by law 2429 (19 March 1981) and are the same every
 * year. The religious feasts move with the Hijri calendar; the first day of
 * each feast is listed below.
 *
 * 2020-2029 are copied from Diyanet İşleri Başkanlığı's published calendar:
 *   https://vakithesaplama.diyanet.gov.tr/dinigunler.php?yil=YYYY
 * (checked against the Diyanet pages for 2022, 2024, 2025, 2026 and 2027).
 *
 * 2030-2050 are calculated, not copied: Diyanet starts each Hijri month by the
 * 2016 International Hijri Calendar Union Congress rule (a month begins the
 * next day when, somewhere on Earth before 00:00 UTC, the Moon stands at least
 * 5° above the horizon at sunset with at least 8° elongation; a sighting in
 * the Americas after 00:00 UTC counts when the conjunction precedes dawn in
 * New Zealand). Applied to the Moon and Sun positions this rule reproduces
 * all twenty 2020-2029 feast dates above; see docs/tr-holidays.md. Because
 * they are calculated, `TR_HOLIDAY_YEARS` lists only the copied years and the
 * deadline calculator still asks the user to check the last day after 2029.
 * A year can hold two feasts of one kind: Ramazan Bayramı falls on 2 January
 * and 23 December 2033.
 */
import { dateToSerial, serialToDate } from "./numberFormat";

/** Fixed national holidays: [month, day]. */
const CIVIL: ReadonlyArray<readonly [number, number]> = [
  [1, 1], // Yılbaşı
  [4, 23], // Ulusal Egemenlik ve Çocuk Bayramı
  [5, 1], // Emek ve Dayanışma Günü
  [5, 19], // Atatürk'ü Anma, Gençlik ve Spor Bayramı
  [7, 15], // Demokrasi ve Millî Birlik Günü
  [8, 30], // Zafer Bayramı
  [10, 29], // Cumhuriyet Bayramı
];

/** First day of Ramazan Bayramı (3 days), ISO dates; Diyanet to 2029, calculated after. */
export const RAMAZAN_STARTS: readonly string[] = [
  "2020-05-24",
  "2021-05-13",
  "2022-05-02",
  "2023-04-21",
  "2024-04-10",
  "2025-03-30",
  "2026-03-20",
  "2027-03-09",
  "2028-02-26",
  "2029-02-14",
  "2030-02-04",
  "2031-01-24",
  "2032-01-14",
  "2033-01-02",
  "2033-12-23",
  "2034-12-12",
  "2035-12-01",
  "2036-11-19",
  "2037-11-08",
  "2038-10-29",
  "2039-10-19",
  "2040-10-07",
  "2041-09-26",
  "2042-09-15",
  "2043-09-04",
  "2044-08-24",
  "2045-08-14",
  "2046-08-03",
  "2047-07-24",
  "2048-07-12",
  "2049-07-01",
  "2050-06-20",
];

/** First day of Kurban Bayramı (4 days), ISO dates; Diyanet to 2029, calculated after. */
export const KURBAN_STARTS: readonly string[] = [
  "2020-07-31",
  "2021-07-20",
  "2022-07-09",
  "2023-06-28",
  "2024-06-16",
  "2025-06-06",
  "2026-05-27",
  "2027-05-16",
  "2028-05-05",
  "2029-04-24",
  "2030-04-13",
  "2031-04-02",
  "2032-03-21",
  "2033-03-11",
  "2034-03-01",
  "2035-02-18",
  "2036-02-07",
  "2037-01-26",
  "2038-01-15",
  "2039-01-05",
  "2039-12-26",
  "2040-12-14",
  "2041-12-04",
  "2042-11-23",
  "2043-11-12",
  "2044-10-31",
  "2045-10-20",
  "2046-10-10",
  "2047-09-30",
  "2048-09-18",
  "2049-09-07",
  "2050-08-27",
];

/** Years whose religious feasts are copied from Diyanet's calendar. */
export const TR_HOLIDAY_YEARS: readonly number[] = [2020, 2021, 2022, 2023, 2024, 2025, 2026, 2027, 2028, 2029];

/** Last year whose religious feasts are listed at all (copied or calculated). */
export const TR_FEAST_LAST_YEAR = 2050;

function serialOf(year: number, month: number, day: number): number {
  return dateToSerial(new Date(Date.UTC(year, month - 1, day)));
}

function isoSerial(iso: string, offset = 0): number {
  const [year, month, day] = iso.split("-").map(Number);
  return serialOf(year, month, day + offset);
}

function yearOf(serial: number): number | undefined {
  return serialToDate(serial)?.getUTCFullYear();
}

/** Feast days (start + following days) that fall in `year`. */
function feastDays(starts: readonly string[], length: number, year: number): number[] {
  const out: number[] = [];
  for (const start of starts) {
    const startYear = Number(start.slice(0, 4));
    if (startYear < year - 1 || startYear > year) continue;
    for (let offset = 0; offset < length; offset += 1) {
      const serial = isoSerial(start, offset);
      if (yearOf(serial) === year) out.push(serial);
    }
  }
  return out;
}

/** Serial days of every TR official holiday in `year`. */
export function turkishHolidaySerials(year: number): number[] {
  return [
    ...CIVIL.map(([month, day]) => serialOf(year, month, day)),
    ...feastDays(RAMAZAN_STARTS, 3, year),
    ...feastDays(KURBAN_STARTS, 4, year),
  ];
}

/**
 * Half-day eves (arife), a holiday from 13:00: 28 October and the day before
 * each religious feast (the feast eves only for listed years).
 */
export function turkishHalfDaySerials(year: number): number[] {
  const eves = [...RAMAZAN_STARTS, ...KURBAN_STARTS]
    .map((start) => isoSerial(start, -1))
    .filter((serial) => yearOf(serial) === year);
  return [serialOf(year, 10, 28), ...eves];
}

/** Serial days of the TR holidays inside `[from, to]` (either order). */
export function turkishHolidaySerialsBetween(from: number, to: number): number[] {
  const low = Math.min(from, to);
  const high = Math.max(from, to);
  const firstYear = serialToDate(low)?.getUTCFullYear() ?? new Date().getUTCFullYear();
  const lastYear = serialToDate(high)?.getUTCFullYear() ?? firstYear;
  const out: number[] = [];
  for (let year = firstYear; year <= lastYear; year += 1) {
    for (const serial of turkishHolidaySerials(year)) {
      if (serial >= low && serial <= high) out.push(serial);
    }
  }
  return out;
}
