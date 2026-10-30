import { describe, expect, it } from "vitest";

import { detectedCorners, type ScanDetection } from "./scan-detect";

const detection: ScanDetection = {
  detected: true,
  corners: [
    [10, 20],
    [300, 22],
    [298, 400],
    [8, 398],
  ],
  score: 0.77,
};

describe("scanner detection helpers", () => {
  it("hands the four corners back when the page was found", () => {
    expect(detectedCorners(detection)).toEqual([
      [10, 20],
      [300, 22],
      [298, 400],
      [8, 398],
    ]);
  });

  it("ignores a miss, an empty answer and a wrong corner count", () => {
    expect(detectedCorners(null)).toBeUndefined();
    expect(detectedCorners(undefined)).toBeUndefined();
    expect(detectedCorners({ detected: false, corners: [], score: 0 })).toBeUndefined();
    expect(detectedCorners({ detected: true, corners: [[1, 2]], score: 0.4 })).toBeUndefined();
  });
});
