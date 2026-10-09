import { describe, expect, it } from "vitest";
import { addTabStop, moveTabStop, nextTabStop, removeTabStop } from "./tab-stops";

describe("tab stops", () => {
  it("adds stops sorted by position and de-duplicates the same spot", () => {
    let tabs = addTabStop(undefined, 72);
    tabs = addTabStop(tabs, 36);
    expect(tabs).toEqual([
      { posPt: 36, align: "left" },
      { posPt: 72, align: "left" },
    ]);
    // Re-clicking the same position re-aligns instead of stacking.
    tabs = addTabStop(tabs, 36, "center");
    expect(tabs).toEqual([
      { posPt: 36, align: "center" },
      { posPt: 72, align: "left" },
    ]);
  });

  it("clamps negative positions to zero", () => {
    expect(addTabStop(undefined, -12)).toEqual([{ posPt: 0, align: "left" }]);
  });

  it("moves a stop and keeps the list sorted", () => {
    const tabs = [
      { posPt: 36, align: "left" },
      { posPt: 72, align: "right" },
    ];
    const moved = moveTabStop(tabs, 0, 108);
    expect(moved).toEqual([
      { posPt: 72, align: "right" },
      { posPt: 108, align: "left" },
    ]);
    expect(moveTabStop(tabs, 9, 10)).toBe(tabs);
  });

  it("drops the stop the moved one lands on", () => {
    const tabs = [
      { posPt: 36, align: "left" },
      { posPt: 72, align: "right" },
    ];
    expect(moveTabStop(tabs, 0, 72)).toEqual([{ posPt: 72, align: "left" }]);
  });

  it("removes a stop by index", () => {
    const tabs = [
      { posPt: 36, align: "left" },
      { posPt: 72, align: "left" },
    ];
    expect(removeTabStop(tabs, 0)).toEqual([{ posPt: 72, align: "left" }]);
    expect(removeTabStop(tabs, 4)).toBe(tabs);
  });

  it("finds the next custom stop, falling back to the default step", () => {
    expect(nextTabStop(undefined, 10)).toBe(36);
    expect(nextTabStop(undefined, 36)).toBe(72);
    expect(nextTabStop([], 37)).toBe(72);
    const tabs = [
      { posPt: 40, align: "left" },
      { posPt: 90, align: "right" },
    ];
    expect(nextTabStop(tabs, 10)).toBe(40);
    expect(nextTabStop(tabs, 40)).toBe(90);
    expect(nextTabStop(tabs, 90)).toBe(108);
    expect(nextTabStop(tabs, 100, 50)).toBe(150);
  });
});
