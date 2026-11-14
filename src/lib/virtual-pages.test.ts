import { describe, expect, it } from "vitest";
import { computeVirtualPageRange, pageOffsetTop } from "./virtual-pages";

describe("computeVirtualPageRange", () => {
  it("returns empty range for zero pages", () => {
    expect(computeVirtualPageRange([], 20, 0, 800)).toEqual({
      startIndex: 0,
      endIndex: -1,
      topSpacerPx: 0,
      bottomSpacerPx: 0,
    });
  });

  it("mounts initial window with overscan and bottom spacer on a 100-page document", () => {
    const heights = Array.from({ length: 100 }, () => 1000);
    const range = computeVirtualPageRange(heights, 20, 0, 900, 3);
    expect(range.startIndex).toBe(0);
    expect(range.endIndex).toBe(3);
    expect(range.topSpacerPx).toBe(0);
    expect(range.bottomSpacerPx).toBeGreaterThan(90_000);
  });

  it("windows middle pages with both top and bottom spacers", () => {
    const heights = Array.from({ length: 100 }, () => 1000);
    // Page 50 starts at 50 * 1020 = 51000
    const range = computeVirtualPageRange(heights, 20, 51_000, 900, 2);
    expect(range.startIndex).toBe(48);
    expect(range.endIndex).toBe(52);
    expect(range.topSpacerPx).toBe(48 * 1020 - 20);
    expect(range.bottomSpacerPx).toBe(47 * 1020 - 20);
  });

  it("computes cumulative pageOffsetTop accurately", () => {
    const heights = [800, 1200, 600];
    expect(pageOffsetTop(heights, 20, 0)).toBe(0);
    expect(pageOffsetTop(heights, 20, 1)).toBe(820);
    expect(pageOffsetTop(heights, 20, 2)).toBe(2040);
  });
});
