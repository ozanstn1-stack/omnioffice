import { describe, expect, it } from "vitest";

import { fitWidthZoom, initialWriterZoom } from "./zoom";

const A4_WIDTH_PT = 595.3;

describe("Writer zoom", () => {
  it("fits an A4 page into a phone's width", () => {
    // Pixel 8 Pro: 1344 device px at dpr 3 = 448 CSS px.
    const zoom = fitWidthZoom(A4_WIDTH_PT, 448);
    expect(zoom).toBeGreaterThan(0.5);
    expect(A4_WIDTH_PT * (96 / 72) * zoom).toBeLessThanOrEqual(448 - 32);
  });

  it("never zooms past 100 % or below 25 %", () => {
    expect(fitWidthZoom(A4_WIDTH_PT, 4000)).toBe(1);
    expect(fitWidthZoom(A4_WIDTH_PT, 60)).toBe(0.25);
    expect(fitWidthZoom(0, 448)).toBe(1);
  });

  it("starts fitted on a phone and at 100 % on a desktop window", () => {
    expect(initialWriterZoom(A4_WIDTH_PT, 448)).toBeLessThan(1);
    expect(initialWriterZoom(A4_WIDTH_PT, 1280)).toBe(1);
  });
});
