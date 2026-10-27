import { describe, expect, it } from "vitest";

import { cornersToPixels, defaultCorners, isFullFrame, moveCorner, pixelsToCorners } from "./scan-corners";

describe("scanner corner helpers", () => {
  it("starts with the four corners a little inside the photo", () => {
    expect(defaultCorners()).toEqual([
      [0.06, 0.06],
      [0.94, 0.06],
      [0.94, 0.94],
      [0.06, 0.94],
    ]);
  });

  it("converts between the picker's fractions and the photo's pixels", () => {
    const corners = defaultCorners();
    const pixels = cornersToPixels(corners, 1000, 2000);
    expect(pixels[0]).toEqual([60, 120]);
    expect(pixels[2]).toEqual([940, 1880]);
    expect(pixelsToCorners(pixels, 1000, 2000)).toEqual(corners);
  });

  it("keeps a dragged corner inside the photo", () => {
    const moved = moveCorner(defaultCorners(), 1, [1.4, -0.2]);
    expect(moved[1]).toEqual([1, 0]);
    // The other corners stay where they were.
    expect(moved[0]).toEqual([0.06, 0.06]);
  });

  it("recognises corners that cover the whole photo", () => {
    expect(
      isFullFrame([
        [0, 0],
        [1, 0],
        [1, 1],
        [0, 1],
      ]),
    ).toBe(true);
    expect(isFullFrame(defaultCorners())).toBe(false);
  });
});
