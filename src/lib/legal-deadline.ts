/**
 * Notification deadline calculator (issue #341, C2) for Turkish court
 * periods. Dates are calendar days as "YYYY-MM-DD", computed in UTC so the
 * device time zone never shifts a day.
 *
 * Rules, in order:
 * 1. Electronic notification (Tebligat Kanunu 7/a): served at the end of the
 *    fifth day after the message reached the electronic address.
 * 2. The period runs from the day after service (HMK 92/1); a period in weeks
 *    or months ends on the same weekday / day of the month (HMK 92/2), the
 *    month's last day when that month is shorter.
 * 3. Judicial recess (HMK 102-104): a period ending between 20 July and
 *    31 August ends one week after the recess, on 7 September.
 * 4. An end on a weekend or an official holiday moves to the next working
 *    day (HMK 93; Saturday counts as a non-working day for the courts).
 *
 * The result lists every step so the lawyer can check the reasoning; it is
 * an aid, not legal advice, and the screen says so.
 */
import { dateToSerial } from "../office/calc/numberFormat";
import { turkishHalfDaySerials, turkishHolidaySerials, TR_HOLIDAY_YEARS } from "../office/calc/tr-holidays";

export type PeriodUnit = "day" | "week" | "month";

export interface Period {
  amount: number;
  unit: PeriodUnit;
}

export interface DeadlineInput {
  /** The day the notification reached the addressee (or was served, when not electronic). */
  received: string;
  /** Electronic notification: service is deemed five days later (7/a). */
  electronic: boolean;
  period: Period;
  /** Apply the judicial recess extension (HMK 104). */
  judicialRecess: boolean;
}

export type DeadlineStepKind = "served" | "periodEnd" | "recess" | "nonWorking";

export interface DeadlineStep {
  kind: DeadlineStepKind;
  date: string;
  /** For "nonWorking": the days skipped. */
  skipped?: string[];
}

export interface DeadlineResult {
  served: string;
  deadline: string;
  steps: DeadlineStep[];
  /** The deadline is a half-day eve (arife); offices close at noon. */
  halfDay: boolean;
  /** A year outside the holiday table was crossed; religious holidays may be missing. */
  outsideHolidayTable: boolean;
}

/** Common periods offered as presets; the screen labels them with examples. */
export const PERIOD_PRESETS: readonly { key: string; period: Period }[] = [
  { key: "twoWeeks", period: { amount: 2, unit: "week" } },
  { key: "oneWeek", period: { amount: 1, unit: "week" } },
  { key: "sevenDays", period: { amount: 7, unit: "day" } },
  { key: "thirtyDays", period: { amount: 30, unit: "day" } },
  { key: "sixtyDays", period: { amount: 60, unit: "day" } },
  { key: "oneMonth", period: { amount: 1, unit: "month" } },
];

const DAY_MS = 86_400_000;

export function parseDay(value: string): Date | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!match) return null;
  const date = new Date(Date.UTC(Number(match[1]), Number(match[2]) - 1, Number(match[3])));
  return date.getUTCMonth() === Number(match[2]) - 1 ? date : null;
}

export function formatDay(date: Date): string {
  return date.toISOString().slice(0, 10);
}

function addDays(date: Date, days: number): Date {
  return new Date(date.getTime() + days * DAY_MS);
}

function addMonths(date: Date, months: number): Date {
  const year = date.getUTCFullYear();
  const month = date.getUTCMonth() + months;
  const last = new Date(Date.UTC(year, month + 1, 0)).getUTCDate();
  return new Date(Date.UTC(year, month, Math.min(date.getUTCDate(), last)));
}

function isHoliday(date: Date): boolean {
  return turkishHolidaySerials(date.getUTCFullYear()).includes(dateToSerial(date));
}

function isWeekend(date: Date): boolean {
  const day = date.getUTCDay();
  return day === 0 || day === 6;
}

/** Computes the last day of the period, or null for an unreadable date or period. */
export function computeDeadline(input: DeadlineInput): DeadlineResult | null {
  const received = parseDay(input.received);
  const { amount, unit } = input.period;
  if (!received || !Number.isInteger(amount) || amount < 1 || amount > 3650) return null;

  const steps: DeadlineStep[] = [];
  const served = input.electronic ? addDays(received, 5) : received;
  steps.push({ kind: "served", date: formatDay(served) });

  let end =
    unit === "day"
      ? addDays(served, amount)
      : unit === "week"
        ? addDays(served, 7 * amount)
        : addMonths(served, amount);
  steps.push({ kind: "periodEnd", date: formatDay(end) });

  if (input.judicialRecess) {
    const year = end.getUTCFullYear();
    const recessStart = Date.UTC(year, 6, 20);
    const recessEnd = Date.UTC(year, 7, 31);
    if (end.getTime() >= recessStart && end.getTime() <= recessEnd) {
      end = new Date(Date.UTC(year, 8, 7));
      steps.push({ kind: "recess", date: formatDay(end) });
    }
  }

  const skipped: string[] = [];
  while (isWeekend(end) || isHoliday(end)) {
    skipped.push(formatDay(end));
    end = addDays(end, 1);
  }
  if (skipped.length) steps.push({ kind: "nonWorking", date: formatDay(end), skipped });

  const years = new Set([received.getUTCFullYear(), end.getUTCFullYear()]);
  return {
    served: formatDay(served),
    deadline: formatDay(end),
    steps,
    halfDay: turkishHalfDaySerials(end.getUTCFullYear()).includes(dateToSerial(end)),
    outsideHolidayTable: [...years].some((year) => !TR_HOLIDAY_YEARS.includes(year)),
  };
}

export type NotificationDateKind = "received" | "served" | "other";

export interface NotificationDateCandidate {
  date: string;
  kind: NotificationDateKind;
}

const DATE_PATTERN = /\b(\d{1,2})[./](\d{1,2})[./](\d{4})\b/g;
const RECEIVED_WORDS = /(ula[şs](ma|t[ıi][ğg][ıi])|teslim|okundu|eri[şs]im)/i;
const SERVED_WORDS = /tebli[ğg].{0,30}(tarih|say[ıi]l)/i;

/**
 * Dates in a notification PDF's text, the ones labelled as the arrival or
 * the deemed service first. A heuristic: the screen shows the candidates
 * and the lawyer picks or corrects one.
 */
export function findNotificationDates(text: string): NotificationDateCandidate[] {
  const seen = new Map<string, NotificationDateKind>();
  for (const line of text.split(/\r?\n/)) {
    for (const match of line.matchAll(DATE_PATTERN)) {
      const iso = `${match[3]}-${match[2].padStart(2, "0")}-${match[1].padStart(2, "0")}`;
      if (!parseDay(iso)) continue;
      const before = line.slice(0, match.index ?? 0);
      const kind: NotificationDateKind = RECEIVED_WORDS.test(before)
        ? "received"
        : SERVED_WORDS.test(before)
          ? "served"
          : "other";
      const previous = seen.get(iso);
      if (!previous || previous === "other") seen.set(iso, kind);
    }
  }
  const rank: Record<NotificationDateKind, number> = { received: 0, served: 1, other: 2 };
  return [...seen.entries()].map(([date, kind]) => ({ date, kind })).sort((a, b) => rank[a.kind] - rank[b.kind]);
}

function icsText(value: string): string {
  return value.replace(/\\/g, "\\\\").replace(/;/g, "\\;").replace(/,/g, "\\,").replace(/\r?\n/g, "\\n");
}

/**
 * A one-event calendar file: an all-day event on the deadline with a
 * reminder two days before at 09:00.
 */
export function deadlineIcs(options: {
  title: string;
  deadline: string;
  description: string;
  uid: string;
  now: Date;
}): string {
  const day = options.deadline.replace(/-/g, "");
  const next = formatDay(addDays(parseDay(options.deadline) ?? new Date(0), 1)).replace(/-/g, "");
  const stamp = options.now
    .toISOString()
    .replace(/[-:]/g, "")
    .replace(/\.\d{3}/, "");
  return [
    "BEGIN:VCALENDAR",
    "VERSION:2.0",
    "PRODID:-//OmniOffice//Deadline//TR",
    "CALSCALE:GREGORIAN",
    "BEGIN:VEVENT",
    `UID:${options.uid}`,
    `DTSTAMP:${stamp}`,
    `DTSTART;VALUE=DATE:${day}`,
    `DTEND;VALUE=DATE:${next}`,
    `SUMMARY:${icsText(options.title)}`,
    `DESCRIPTION:${icsText(options.description)}`,
    "BEGIN:VALARM",
    "ACTION:DISPLAY",
    `DESCRIPTION:${icsText(options.title)}`,
    "TRIGGER:-P1DT15H",
    "END:VALARM",
    "END:VEVENT",
    "END:VCALENDAR",
    "",
  ].join("\r\n");
}
