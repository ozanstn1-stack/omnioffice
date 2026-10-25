import { beforeEach, describe, expect, it } from "vitest";

import { openWithDiagnosticsText, recordOpenWith, resetOpenWithDiagnostics } from "./open-with-diagnostics";

describe("open-with diagnostics ring", () => {
  beforeEach(() => resetOpenWithDiagnostics());

  it("renders a readable note when nothing was recorded", () => {
    const text = openWithDiagnosticsText();
    expect(text).toContain("== open-with: frontend ==");
    expect(text).toMatch(/version: v\d+\.\d+\.\d+/);
    expect(text).toMatch(/platform: .+/);
    expect(text).toContain("events: 0/100");
    expect(text).toContain("(none)");
  });

  it("records events with a timestamp, oldest first", () => {
    recordOpenWith("drain:1");
    recordOpenWith("route:office");
    const lines = openWithDiagnosticsText().split("\n");
    const events = lines.filter((line) => line.includes("drain:1") || line.includes("route:office"));
    expect(events).toHaveLength(2);
    expect(events[0]).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z drain:1$/);
    expect(events[1]).toMatch(/ route:office$/);
    expect(lines).toContain("events: 2/100");
  });

  it("keeps only the last 100 events", () => {
    for (let index = 0; index < 130; index += 1) recordOpenWith(`event-${index}`);
    const text = openWithDiagnosticsText();
    expect(text).toContain("events: 100/100");
    const recorded = text.split("\n").filter((line) => line.includes("event-"));
    expect(recorded).toHaveLength(100);
    expect(recorded[0]).toMatch(/ event-30$/);
    expect(recorded[99]).toMatch(/ event-129$/);
  });

  it("flattens line breaks and caps a single event", () => {
    recordOpenWith(`open-failed:${"x".repeat(500)}\nsecond line`);
    const [event] = openWithDiagnosticsText()
      .split("\n")
      .filter((line) => line.includes("open-failed:"));
    expect(event).not.toContain("second line");
    expect(event.replace(/^\S+ /, "")).toHaveLength(400);
  });

  it("reset drops the recorded events", () => {
    recordOpenWith("drain:1");
    resetOpenWithDiagnostics();
    expect(openWithDiagnosticsText()).toContain("(none)");
  });
});
