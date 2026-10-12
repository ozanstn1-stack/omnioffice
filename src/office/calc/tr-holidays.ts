/**
 * Turkish official holidays for the `WORKDAY` / `NETWORKDAYS` `"TR"` flag.
 *
 * Civil holidays are fixed by law 2429 (19 March 1981) and are the same every
 * year. The religious feasts move with the Hijri calendar; the first day of
 * each feast is tabulated for 2020-2029 from Diyanet İşleri Başkanlığı's
 * published calendar:
 *   https://vakithesaplama.diyanet.gov.tr/dinigunler.php?yil=YYYY
 * The values were checked against the Diyanet pages for 2022, 2024, 2025,
 * 2026 and 2027 during development.
 *
 * 2030 and later are deliberately not tabulated: the Hijri calendar is
 * anomalous there (2030 has two Ramazans), so the built-in list stays silent
 * for those years and the caller passes the days explicitly.
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

/** First day of Ramazan Bayramı (3 days) and Kurban Bayramı (4 days). */
const RELIGIOUS: Record<number, { ramazan: [number, number]; kurban: [number, number] }> = {
  2020: { ramazan: [5, 24], kurban: [7, 31] },
  2021: { ramazan: [5, 13], kurban: [7, 20] },
  2022: { ramazan: [5, 2], kurban: [7, 9] },
  2023: { ramazan: [4, 21], kurban: [6, 28] },
  2024: { ramazan: [4, 10], kurban: [6, 16] },
  2025: { ramazan: [3, 30], kurban: [6, 6] },
  2026: { ramazan: [3, 20], kurban: [5, 27] },
  2027: { ramazan: [3, 9], kurban: [5, 16] },
  2028: { ramazan: [2, 26], kurban: [5, 5] },
  2029: { ramazan: [2, 14], kurban: [4, 24] },
};

function serialOf(year: number, month: number, day: number): number {
  return dateToSerial(new Date(Date.UTC(year, month - 1, day)));
}

/** Serial days of every TR official holiday in `year`. */
export function turkishHolidaySerials(year: number): number[] {
  const serials: number[] = CIVIL.map(([month, day]) => serialOf(year, month, day));
  const religious = RELIGIOUS[year];
  if (religious) {
    for (let offset = 0; offset < 3; offset += 1) {
      serials.push(serialOf(year, religious.ramazan[0], religious.ramazan[1] + offset));
    }
    for (let offset = 0; offset < 4; offset += 1) {
      serials.push(serialOf(year, religious.kurban[0], religious.kurban[1] + offset));
    }
  }
  return serials;
}

/** Serial days of the TR holidays inside `[from, to]` (either order). */
export function turkishHolidaySerialsBetween(from: number, to: number): number[] {
  const low = Math.min(from, to);
  const high = Math.max(from, to);
  const firstYear = serialToDate(low)?.getUTCFullYear() ?? new Date().getUTCFullYear();
  const lastYear = serialToDate(high)?.getUTCFullYear() ?? firstYear;
  const out: number[] = [];
  for (let year = firstYear - 1; year <= lastYear + 1; year += 1) {
    for (const serial of turkishHolidaySerials(year)) {
      if (serial >= low && serial <= high) out.push(serial);
    }
  }
  return out;
}
