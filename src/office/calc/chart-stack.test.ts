import { describe, expect, it } from "vitest";
import { spanRange, valueSpans } from "./chart-stack";

describe("chart value spans (4.1c)", () => {
  const values = [
    [3, -2, 5],
    [4, -1, Number.NaN],
  ];

  it("starts every value at zero side by side", () => {
    expect(valueSpans(values, false)[1]).toEqual([
      { from: 0, to: 4 },
      { from: 0, to: -1 },
      { from: 0, to: 0 },
    ]);
  });

  it("stacks positives on positives and negatives on negatives", () => {
    const spans = valueSpans(values, true);
    expect(spans[1][0]).toEqual({ from: 3, to: 7 });
    expect(spans[1][1]).toEqual({ from: -2, to: -3 });
    expect(spanRange(spans)).toEqual({ min: -3, max: 7 });
  });

  it("keeps zero on the axis and never returns an empty range", () => {
    expect(spanRange(valueSpans([[2, 4]], false))).toEqual({ min: 0, max: 4 });
    expect(spanRange([])).toEqual({ min: 0, max: 1 });
  });
});
