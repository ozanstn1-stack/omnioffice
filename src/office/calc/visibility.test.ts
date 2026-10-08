import { describe, expect, it } from "vitest";
import {
  edgeVisible,
  hideRange,
  isHiddenIndex,
  revealableIndexes,
  shiftSizes,
  stepVisible,
  unhideRange,
} from "./visibility";

describe("hideRange", () => {
  it("sets the size of every index in the range to 0", () => {
    expect(hideRange({}, 2, 4, 10)).toEqual({ "2": 0, "3": 0, "4": 0 });
  });

  it("keeps the other sizes and does not mutate its input", () => {
    const sizes = { "0": 48, "3": 30 };
    const next = hideRange(sizes, 3, 3, 10);
    expect(next).toEqual({ "0": 48, "3": 0 });
    expect(sizes).toEqual({ "0": 48, "3": 30 });
  });

  it("accepts a reversed range", () => {
    expect(hideRange({}, 4, 2, 10)).toEqual({ "2": 0, "3": 0, "4": 0 });
  });

  it("returns null when nothing would change", () => {
    expect(hideRange({ "2": 0 }, 2, 2, 10)).toBeNull();
  });

  it("refuses to hide every row or column, which would leave nothing to select", () => {
    expect(hideRange({}, 0, 4, 5)).toBeNull();
    expect(hideRange({ "4": 0 }, 0, 3, 5)).toBeNull();
    expect(hideRange({}, 0, 3, 5)).toEqual({ "0": 0, "1": 0, "2": 0, "3": 0 });
  });

  it("ignores indexes beyond the sheet", () => {
    expect(hideRange({}, 3, 99, 5)).toEqual({ "3": 0, "4": 0 });
  });
});

describe("revealableIndexes", () => {
  const sizes = { "2": 0, "3": 0, "7": 0, "9": 0 };

  it("lists the hidden indexes inside the range", () => {
    expect(revealableIndexes(sizes, 1, 3, 20)).toEqual([2, 3]);
  });

  it("includes the hidden run directly before or after a range, like selecting around it", () => {
    // A selection on column E (4) sits next to the hidden run C:D (2..3).
    expect(revealableIndexes(sizes, 4, 4, 20)).toEqual([2, 3]);
    // And one on column B (1) sits on its other side.
    expect(revealableIndexes(sizes, 1, 1, 20)).toEqual([2, 3]);
  });

  it("takes the run on both sides of a range", () => {
    expect(revealableIndexes(sizes, 1, 6, 20)).toEqual([2, 3, 7]);
  });

  it("does not reach past a visible index", () => {
    expect(revealableIndexes(sizes, 5, 5, 20)).toEqual([]);
  });

  it("is empty when nothing near the range is hidden", () => {
    expect(revealableIndexes({}, 0, 3, 20)).toEqual([]);
  });
});

describe("unhideRange", () => {
  it("restores the default size by dropping the entries", () => {
    expect(unhideRange({ "2": 0, "3": 0, "5": 40 }, 1, 4, 20)).toEqual({ "5": 40 });
  });

  it("unhides the run next to a lone selection", () => {
    expect(unhideRange({ "2": 0, "3": 0 }, 4, 4, 20)).toEqual({});
  });

  it("returns null when there is nothing to unhide", () => {
    expect(unhideRange({ "5": 40 }, 1, 4, 20)).toBeNull();
  });
});

describe("isHiddenIndex", () => {
  it("is true only for a size of exactly 0", () => {
    expect(isHiddenIndex({ "1": 0, "2": 24 }, 1)).toBe(true);
    expect(isHiddenIndex({ "1": 0, "2": 24 }, 2)).toBe(false);
    expect(isHiddenIndex({}, 3)).toBe(false);
  });
});

describe("stepVisible", () => {
  const sizes = { "1": 0, "2": 0, "5": 0 };

  it("moves one visible index at a time, hopping over hidden runs", () => {
    expect(stepVisible(sizes, 0, 1, 8)).toBe(3);
    expect(stepVisible(sizes, 3, 1, 8)).toBe(4);
    expect(stepVisible(sizes, 4, 1, 8)).toBe(6);
    expect(stepVisible(sizes, 3, -1, 8)).toBe(0);
    expect(stepVisible(sizes, 6, -1, 8)).toBe(4);
  });

  it("stops at the first or last visible index", () => {
    expect(stepVisible(sizes, 7, 1, 8)).toBe(7);
    expect(stepVisible(sizes, 0, -1, 8)).toBe(0);
    // A hidden run at the very end leaves the last visible index as the limit.
    expect(stepVisible({ "6": 0, "7": 0 }, 5, 1, 8)).toBe(5);
    expect(stepVisible({ "0": 0, "1": 0 }, 2, -1, 8)).toBe(2);
  });

  it("counts several steps and treats 0 as staying put", () => {
    expect(stepVisible(sizes, 0, 3, 8)).toBe(6);
    expect(stepVisible(sizes, 4, 0, 8)).toBe(4);
  });
});

describe("shiftSizes", () => {
  it("moves the sizes at and after an inserted index down by one", () => {
    expect(shiftSizes({ "1": 0, "4": 48 }, 2, 1)).toEqual({ "1": 0, "5": 48 });
    expect(shiftSizes({ "1": 0, "4": 48 }, 1, 1)).toEqual({ "2": 0, "5": 48 });
  });

  it("drops the size of a deleted index and closes the gap", () => {
    expect(shiftSizes({ "1": 0, "4": 48 }, 1, -1)).toEqual({ "3": 48 });
    expect(shiftSizes({ "1": 0, "4": 48 }, 2, -1)).toEqual({ "1": 0, "3": 48 });
  });

  it("returns the same object when nothing moves", () => {
    const sizes = { "1": 0 };
    expect(shiftSizes(sizes, 5, 1)).toBe(sizes);
    expect(shiftSizes(sizes, 5, -1)).toBe(sizes);
  });
});

describe("edgeVisible", () => {
  it("is the first and last index when they are shown", () => {
    expect(edgeVisible({}, 8, "first")).toBe(0);
    expect(edgeVisible({}, 8, "last")).toBe(7);
  });

  it("skips hidden runs at either end", () => {
    const sizes = { "0": 0, "1": 0, "7": 0 };
    expect(edgeVisible(sizes, 8, "first")).toBe(2);
    expect(edgeVisible(sizes, 8, "last")).toBe(6);
  });
});
