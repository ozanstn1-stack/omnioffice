import { describe, expect, it } from "vitest";
import { clampCrop, cropPreview, FULL_CROP, isFullCrop, MAX_CROP } from "./crop-preview";

describe("clampCrop", () => {
  it("keeps valid fractions", () => {
    expect(clampCrop({ left: 0.1, top: 0.2, right: 0.3, bottom: 0.4 })).toEqual({
      left: 0.1,
      top: 0.2,
      right: 0.3,
      bottom: 0.4,
    });
  });

  it("clamps each side into 0..0.95", () => {
    const clamped = clampCrop({ left: -1, top: 0, right: 2, bottom: Number.NaN });
    expect(clamped.left).toBe(0);
    expect(clamped.right).toBe(MAX_CROP);
    expect(clamped.bottom).toBe(0);
  });

  it("scales opposite sides down when they would sum past the limit", () => {
    const clamped = clampCrop({ left: 0.8, right: 0.8, top: 0, bottom: 0 });
    expect(clamped.left + clamped.right).toBeCloseTo(MAX_CROP, 10);
    expect(clamped.left / clamped.right).toBeCloseTo(1, 10);
    // left+right must stay below 1 so a visible window always remains.
    expect(clamped.left + clamped.right).toBeLessThan(1);
  });

  it("treats missing and null crops as the full image", () => {
    expect(clampCrop(null)).toEqual(FULL_CROP);
    expect(clampCrop(undefined)).toEqual(FULL_CROP);
    expect(clampCrop({})).toEqual(FULL_CROP);
  });
});

describe("isFullCrop", () => {
  it("is true for no crop and false for any visible trim", () => {
    expect(isFullCrop(null)).toBe(true);
    expect(isFullCrop(FULL_CROP)).toBe(true);
    expect(isFullCrop({ ...FULL_CROP, top: 0.01 })).toBe(false);
  });
});

describe("cropPreview", () => {
  it("scales and offsets the image for a symmetric crop", () => {
    const preview = cropPreview({ left: 0.1, top: 0, right: 0.1, bottom: 0 });
    expect(preview.widthPct).toBeCloseTo(125, 10);
    expect(preview.offsetXPct).toBeCloseTo(-12.5, 10);
    expect(preview.heightPct).toBeCloseTo(100, 10);
    expect(preview.offsetYPct).toBeCloseTo(0, 10);
  });

  it("shifts the image up and left for top/left trims", () => {
    const preview = cropPreview({ left: 0.25, top: 0.5, right: 0, bottom: 0 });
    expect(preview.widthPct).toBeCloseTo(133.333, 2);
    expect(preview.offsetXPct).toBeCloseTo(-33.333, 2);
    expect(preview.heightPct).toBeCloseTo(200, 10);
    expect(preview.offsetYPct).toBeCloseTo(-100, 10);
  });

  it("renders the full image at 100% with no offset", () => {
    const preview = cropPreview(null);
    expect(preview).toEqual({ widthPct: 100, heightPct: 100, offsetXPct: 0, offsetYPct: 0 });
  });

  it("never divides by an empty window after clamping", () => {
    const preview = cropPreview({ left: 0.95, top: 0.95, right: 0.95, bottom: 0.95 });
    expect(preview.widthPct).toBeLessThanOrEqual(100 / 0.05);
    expect(Number.isFinite(preview.heightPct)).toBe(true);
  });
});
