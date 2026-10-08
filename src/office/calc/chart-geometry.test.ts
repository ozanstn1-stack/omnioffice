import { describe, expect, it } from "vitest";
import {
  DEFAULT_HOLE_SIZE,
  formatTick,
  holeSizeOf,
  niceScale,
  ringBands,
  ringSlices,
  scatterPoints,
  scatterStyleOf,
  scatterXValues,
  smoothPath,
} from "./chart-geometry";

const palette = ["#111111", "#222222", "#333333"];
const pie = { cx: 100, cy: 100, inner: 0, outer: 50 };

describe("holeSizeOf", () => {
  it("defaults to 50 percent", () => {
    expect(DEFAULT_HOLE_SIZE).toBe(50);
    expect(holeSizeOf({})).toBe(50);
    expect(holeSizeOf({ holeSize: null })).toBe(50);
    expect(holeSizeOf({ holeSize: Number.NaN })).toBe(50);
  });

  it("keeps a size inside 10..90 and rounds it", () => {
    expect(holeSizeOf({ holeSize: 65.4 })).toBe(65);
    expect(holeSizeOf({ holeSize: 0 })).toBe(10);
    expect(holeSizeOf({ holeSize: 100 })).toBe(90);
    expect(holeSizeOf({ holeSize: -20 })).toBe(10);
  });
});

describe("ringSlices", () => {
  it("shares the circle by value and starts at twelve o'clock", () => {
    const slices = ringSlices([1, 1, 2], palette, pie);
    expect(slices.map((slice) => slice.fraction)).toEqual([0.25, 0.25, 0.5]);
    expect(slices.map((slice) => slice.color)).toEqual(palette);
    // The first quarter runs from the top (100, 50) to the right (150, 100).
    expect(slices[0].path).toBe("M 100 100 L 100 50 A 50 50 0 0 1 150 100 Z");
    // A slice of more than half the circle is a large arc.
    expect(ringSlices([1, 3], palette, pie)[1].path).toContain("A 50 50 0 1 1");
  });

  it("places the label inside the slice", () => {
    const [first] = ringSlices([1, 1, 2], palette, pie);
    expect(first.label.x).toBeGreaterThan(100);
    expect(first.label.y).toBeLessThan(100);
  });

  it("skips values that take no space but keeps the colour of the others", () => {
    const slices = ringSlices([0, 3, -2, Number.NaN, 1], palette, pie);
    expect(slices.map((slice) => slice.fraction)).toEqual([0.75, 0.25]);
    expect(slices.map((slice) => slice.color)).toEqual(["#222222", "#222222"]);
  });

  it("has nothing to draw when no value is positive", () => {
    expect(ringSlices([], palette, pie)).toEqual([]);
    expect(ringSlices([0, 0, -1], palette, pie)).toEqual([]);
  });

  it("draws a lone value as a whole circle", () => {
    const [only] = ringSlices([5], palette, pie);
    expect(only.fraction).toBe(1);
    expect(only.path.match(/A 50 50/g)).toHaveLength(2);
    expect(only.path.endsWith("Z")).toBe(true);
  });

  it("cuts the hole out of a ring: annular sectors that go back along the inner radius", () => {
    const ring = { cx: 100, cy: 100, inner: 25, outer: 50 };
    const slices = ringSlices([1, 1], palette, ring);
    expect(slices[0].path).toBe("M 100 50 A 50 50 0 0 1 100 150 L 100 125 A 25 25 0 0 0 100 75 Z");
    // The label sits in the band, between the radii.
    const distance = Math.hypot(slices[0].label.x - 100, slices[0].label.y - 100);
    expect(distance).toBeCloseTo(37.5, 1);
  });

  it("draws a lone ring value as an outer circle with the inner circle wound the other way", () => {
    const [only] = ringSlices([5], palette, { cx: 100, cy: 100, inner: 25, outer: 50 });
    expect(only.path).toContain("A 50 50 0 1 1");
    expect(only.path).toContain("A 25 25 0 1 0");
  });
});

describe("ringBands", () => {
  it("fills the band between the hole and the edge with one series", () => {
    expect(ringBands(1, 80, 50)).toEqual([{ inner: 40, outer: 80 }]);
    expect(ringBands(0, 80, 25)).toEqual([{ inner: 20, outer: 80 }]);
  });

  it("puts the first series innermost and splits the band evenly", () => {
    expect(ringBands(2, 100, 50)).toEqual([
      { inner: 50, outer: 75 },
      { inner: 75, outer: 100 },
    ]);
  });
});

describe("scatterStyleOf", () => {
  it("reads each c:scatterStyle spelling", () => {
    expect(scatterStyleOf("lineMarker")).toEqual({ markers: true, line: true, smooth: false });
    expect(scatterStyleOf("line")).toEqual({ markers: false, line: true, smooth: false });
    expect(scatterStyleOf("smoothMarker")).toEqual({ markers: true, line: true, smooth: true });
    expect(scatterStyleOf("smooth")).toEqual({ markers: false, line: true, smooth: true });
  });

  it("draws markers only when the style is unset, marker or unknown", () => {
    for (const style of [null, undefined, "marker", "none", "sparkle"]) {
      expect(scatterStyleOf(style)).toEqual({ markers: true, line: false, smooth: false });
    }
  });
});

describe("scatterXValues", () => {
  it("uses numeric categories as they are", () => {
    expect(scatterXValues([1.5, "2", 10], 3)).toEqual([1.5, 2, 10]);
  });

  it("falls back to 1, 2, 3 when the X range holds text", () => {
    expect(scatterXValues(["a", "b", "c"], 3)).toEqual([1, 2, 3]);
    expect(scatterXValues([1, "b", 3], 3)).toEqual([1, 2, 3]);
  });

  it("falls back to positions for an empty X range and pads a short one", () => {
    expect(scatterXValues([], 3)).toEqual([1, 2, 3]);
    expect(scatterXValues(["", ""], 2)).toEqual([1, 2]);
    expect(scatterXValues([4, 5], 3)[2]).toBeNaN();
  });
});

describe("scatterPoints", () => {
  it("pairs the coordinates and drops pairs with a missing one", () => {
    expect(scatterPoints([1, 2, 3, 4], [10, Number.NaN, 30])).toEqual([
      { x: 1, y: 10 },
      { x: 3, y: 30 },
    ]);
    expect(scatterPoints([1, Number.NaN], [5, 6])).toEqual([{ x: 1, y: 5 }]);
  });
});

describe("niceScale", () => {
  it("rounds the range outwards to a 1-2-5 step", () => {
    expect(niceScale(0, 97)).toEqual({ min: 0, max: 100, ticks: [0, 20, 40, 60, 80, 100] });
    expect(niceScale(1.2, 8.7)).toEqual({ min: 0, max: 10, ticks: [0, 2, 4, 6, 8, 10] });
    expect(niceScale(-3, 12).ticks).toEqual([-5, 0, 5, 10, 15]);
  });

  it("copes with a flat or broken range", () => {
    const flat = niceScale(5, 5);
    expect(flat.min).toBeLessThan(5);
    expect(flat.max).toBeGreaterThan(5);
    expect(niceScale(0, 0).ticks.length).toBeGreaterThan(1);
    expect(niceScale(Number.NaN, 4)).toEqual({ min: 0, max: 1, ticks: [0, 1] });
  });

  it("has no floating-point noise in the ticks", () => {
    expect(niceScale(0, 0.3).ticks).toEqual([0, 0.1, 0.2, 0.3]);
    expect(formatTick(0.1 + 0.2)).toBe("0.3");
    expect(formatTick(1234567)).toBe("1234570");
  });
});

describe("smoothPath", () => {
  it("is a straight line for two points and a curve through every point beyond", () => {
    expect(smoothPath([])).toBe("");
    expect(smoothPath([{ x: 0, y: 0 }])).toBe("M 0 0");
    expect(
      smoothPath([
        { x: 0, y: 0 },
        { x: 10, y: 10 },
      ]),
    ).toBe("M 0 0 L 10 10");
    const curve = smoothPath([
      { x: 0, y: 0 },
      { x: 10, y: 10 },
      { x: 20, y: 0 },
    ]);
    expect(curve.startsWith("M 0 0 C")).toBe(true);
    expect(curve.match(/C/g)).toHaveLength(2);
    expect(curve.endsWith("20 0")).toBe(true);
  });
});
