import { describe, expect, it } from "vitest";
import { computeDeadline, deadlineIcs, findNotificationDates, type DeadlineInput } from "./legal-deadline";

const base: DeadlineInput = {
  received: "2026-03-02",
  electronic: true,
  period: { amount: 2, unit: "week" },
  judicialRecess: true,
};

describe("notification deadline", () => {
  it("serves an e-notice five days after it arrives and moves past a feast and a weekend", () => {
    // Reached the address Monday 2 March 2026: served Saturday 7 March. Two
    // weeks end Saturday 21 March, inside Ramazan Bayramı (20-22 March), so
    // the period ends Monday 23 March.
    const result = computeDeadline(base);
    expect(result?.served).toBe("2026-03-07");
    expect(result?.deadline).toBe("2026-03-23");
    expect(result?.steps).toEqual([
      { kind: "served", date: "2026-03-07" },
      { kind: "periodEnd", date: "2026-03-21" },
      { kind: "nonWorking", date: "2026-03-23", skipped: ["2026-03-21", "2026-03-22"] },
    ]);
    expect(result?.halfDay).toBe(false);
  });

  it("counts a period in days from the day after a paper service", () => {
    const result = computeDeadline({
      ...base,
      received: "2026-01-05",
      electronic: false,
      period: { amount: 7, unit: "day" },
    });
    expect(result?.served).toBe("2026-01-05");
    expect(result?.deadline).toBe("2026-01-12");
  });

  it("ends a month on the last day of a shorter month, then on the next working day", () => {
    const result = computeDeadline({
      ...base,
      received: "2026-01-31",
      electronic: false,
      period: { amount: 1, unit: "month" },
    });
    expect(result?.steps[1]).toEqual({ kind: "periodEnd", date: "2026-02-28" });
    expect(result?.deadline).toBe("2026-03-02");
  });

  it("extends a period ending in the judicial recess to 7 September, only when asked", () => {
    const inRecess = { ...base, received: "2026-07-10", electronic: false };
    expect(computeDeadline(inRecess)?.deadline).toBe("2026-09-07");
    expect(computeDeadline(inRecess)?.steps.map((step) => step.kind)).toContain("recess");
    expect(computeDeadline({ ...inRecess, judicialRecess: false })?.deadline).toBe("2026-07-24");
  });

  it("flags a deadline on a half-day eve", () => {
    const result = computeDeadline({ ...base, received: "2026-10-14", electronic: false });
    expect(result?.deadline).toBe("2026-10-28");
    expect(result?.halfDay).toBe(true);
  });

  it("warns when a year has no religious holiday table and refuses bad input", () => {
    expect(computeDeadline({ ...base, received: "2031-03-03" })?.outsideHolidayTable).toBe(true);
    expect(computeDeadline(base)?.outsideHolidayTable).toBe(false);
    expect(computeDeadline({ ...base, received: "2026-02-30" })).toBeNull();
    expect(computeDeadline({ ...base, period: { amount: 0, unit: "day" } })).toBeNull();
  });
});

describe("dates in an e-notice", () => {
  it("puts the arrival date first, then the deemed service, then the rest", () => {
    const text = [
      "Gönderim Tarihi: 01.03.2026 09:12",
      "Elektronik Adrese Ulaşma Tarihi: 02.03.2026 14:31",
      "Tebliğ Edilmiş Sayılma Tarihi: 7.3.2026",
      "Geçersiz: 31.02.2026",
    ].join("\n");
    expect(findNotificationDates(text)).toEqual([
      { date: "2026-03-02", kind: "received" },
      { date: "2026-03-07", kind: "served" },
      { date: "2026-03-01", kind: "other" },
    ]);
    expect(findNotificationDates("tarih yok")).toEqual([]);
  });
});

describe("calendar file", () => {
  it("writes one all-day event with a reminder and escaped text", () => {
    const ics = deadlineIcs({
      title: "İstinaf, 2026/123 E.",
      deadline: "2026-03-23",
      description: "Tebliğ: 2026-03-07; süre: 2 hafta",
      uid: "deadline-1@omnioffice",
      now: new Date("2026-03-02T10:00:00Z"),
    });
    expect(ics).toContain("DTSTART;VALUE=DATE:20260323\r\n");
    expect(ics).toContain("DTEND;VALUE=DATE:20260324\r\n");
    expect(ics).toContain("SUMMARY:İstinaf\\, 2026/123 E.\r\n");
    expect(ics).toContain("DESCRIPTION:Tebliğ: 2026-03-07\\; süre: 2 hafta\r\n");
    expect(ics).toContain("DTSTAMP:20260302T100000Z\r\n");
    expect(ics.startsWith("BEGIN:VCALENDAR\r\n")).toBe(true);
    expect(ics.trimEnd().endsWith("END:VCALENDAR")).toBe(true);
  });
});
