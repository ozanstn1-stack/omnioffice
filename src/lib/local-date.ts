/** Calendar dates and clock times in the user's own time zone. */

const pad2 = (value: number) => String(value).padStart(2, "0");

/**
 * `YYYY-MM-DD` of `date` in the user's own time zone.
 *
 * `toISOString().slice(0, 10)` is the UTC date: in Turkey (UTC+3) it is the
 * previous day from midnight to 03:00, and a calendar built from local
 * midnights is keyed one day early all the time.
 */
export function localIsoDate(date: Date = new Date()): string {
  return `${date.getFullYear()}-${pad2(date.getMonth() + 1)}-${pad2(date.getDate())}`;
}

/** `HH:MM:SS` of `date` in the user's own time zone. */
export function localIsoTime(date: Date = new Date()): string {
  return `${pad2(date.getHours())}:${pad2(date.getMinutes())}:${pad2(date.getSeconds())}`;
}
