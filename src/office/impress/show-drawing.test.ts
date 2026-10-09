import { describe, expect, it } from "vitest";
import { simplifyStroke, strokeHit, strokePath } from "./show-drawing";

describe("simplifyStroke", () => {
  it("keeps endpoints and drops points closer than the threshold", () => {
    const points = [
      { x: 0, y: 0 },
      { x: 0.5, y: 0.5 },
      { x: 5, y: 0 },
      { x: 5.4, y: 0 },
      { x: 9, y: 4 },
    ];
    expect(simplifyStroke(points, 2)).toEqual([
      { x: 0, y: 0 },
      { x: 5, y: 0 },
      { x: 9, y: 4 },
    ]);
  });

  it("always keeps the final point", () => {
    const points = [
      { x: 0, y: 0 },
      { x: 1, y: 0 },
      { x: 1.2, y: 0 },
    ];
    expect(simplifyStroke(points, 5)).toEqual([
      { x: 0, y: 0 },
      { x: 1.2, y: 0 },
    ]);
  });

  it("returns short strokes untouched", () => {
    expect(
      simplifyStroke([
        { x: 1, y: 1 },
        { x: 1.5, y: 1.5 },
      ]),
    ).toHaveLength(2);
  });
});

describe("strokePath", () => {
  it("builds a move plus line commands", () => {
    expect(
      strokePath([
        { x: 0, y: 0 },
        { x: 10, y: 5.25 },
      ]),
    ).toBe("M 0 0 L 10 5.3");
  });

  it("turns a single point into a dot-sized segment", () => {
    expect(strokePath([{ x: 3, y: 4 }])).toBe("M 3 4 L 3 4");
  });

  it("returns an empty path for no points", () => {
    expect(strokePath([])).toBe("");
  });
});

describe("strokeHit", () => {
  const strokes = [
    [
      { x: 0, y: 0 },
      { x: 100, y: 0 },
    ],
    [
      { x: 0, y: 50 },
      { x: 0, y: 100 },
    ],
  ];

  it("finds the stroke under the point", () => {
    expect(strokeHit(strokes, { x: 50, y: 5 }, 10)).toBe(0);
    expect(strokeHit(strokes, { x: 5, y: 80 }, 10)).toBe(1);
  });

  it("returns -1 when no stroke is near", () => {
    expect(strokeHit(strokes, { x: 50, y: 30 }, 10)).toBe(-1);
    expect(strokeHit([], { x: 0, y: 0 }, 10)).toBe(-1);
  });

  it("matches single-point strokes", () => {
    expect(strokeHit([[{ x: 10, y: 10 }]], { x: 12, y: 10 }, 5)).toBe(0);
  });
});
