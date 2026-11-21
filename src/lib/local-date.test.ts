import { describe, expect, it } from "vitest";
import { localIsoDate, localIsoTime } from "./local-date";

describe("local calendar dates", () => {
  it("uses the local day, not the UTC one", () => {
    // 00:30 local on 21 Nov: in UTC+3 the UTC date is still the 20th.
    const lateNight = new Date(2026, 10, 21, 0, 30, 5);
    expect(localIsoDate(lateNight)).toBe("2026-11-21");
    expect(localIsoTime(lateNight)).toBe("00:30:05");
  });

  it("pads months and days", () => {
    expect(localIsoDate(new Date(2026, 0, 5))).toBe("2026-01-05");
  });
});
