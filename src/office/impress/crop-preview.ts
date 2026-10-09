/**
 * Crop math for slide images.
 *
 * A crop stores the fractions of the source image trimmed from each side. The
 * model keeps the full image data; the preview shows the visible window through
 * an overflow-hidden box with a scaled and shifted `<img>`, so these helpers
 * produce the CSS percentages that arrangement needs.
 */
import type { ImageCrop } from "../../lib/office-types";

/** The largest fraction a single side may trim, leaving a visible sliver. */
export const MAX_CROP = 0.95;

export const FULL_CROP: ImageCrop = { left: 0, top: 0, right: 0, bottom: 0 };

function clamp01(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.max(0, Math.min(MAX_CROP, value));
}

/** Limits opposite sides so they never sum past `MAX_CROP` (left+right < 1). */
function clampSides(a: number, b: number): [number, number] {
  if (a + b <= MAX_CROP) return [a, b];
  const scale = MAX_CROP / (a + b);
  return [a * scale, b * scale];
}

/** Clamps a crop to valid fractions: 0..0.95 per side, left+right and
 * top+bottom each at most 0.95. */
export function clampCrop(crop: Partial<ImageCrop> | null | undefined): ImageCrop {
  const left = clamp01(crop?.left ?? 0);
  const right = clamp01(crop?.right ?? 0);
  const top = clamp01(crop?.top ?? 0);
  const bottom = clamp01(crop?.bottom ?? 0);
  const [clampedLeft, clampedRight] = clampSides(left, right);
  const [clampedTop, clampedBottom] = clampSides(top, bottom);
  return {
    left: clampedLeft,
    top: clampedTop,
    right: clampedRight,
    bottom: clampedBottom,
  };
}

/** True for a crop that trims nothing visible. */
export function isFullCrop(crop: ImageCrop | null | undefined): boolean {
  if (!crop) return true;
  const clamped = clampCrop(crop);
  return clamped.left + clamped.right + clamped.top + clamped.bottom <= 1e-6;
}

/** CSS percentages that position the source image inside the crop window. */
export interface CropPreview {
  /** Width of the image as a percentage of the visible box. */
  widthPct: number;
  /** Height of the image as a percentage of the visible box. */
  heightPct: number;
  /** Horizontal offset of the image as a percentage of the visible box. */
  offsetXPct: number;
  /** Vertical offset of the image as a percentage of the visible box. */
  offsetYPct: number;
}

export function cropPreview(crop: ImageCrop | null | undefined): CropPreview {
  const clamped = clampCrop(crop);
  const visibleWidth = Math.max(0.05, 1 - clamped.left - clamped.right);
  const visibleHeight = Math.max(0.05, 1 - clamped.top - clamped.bottom);
  return {
    widthPct: 100 / visibleWidth,
    heightPct: 100 / visibleHeight,
    offsetXPct: clamped.left === 0 ? 0 : -(clamped.left / visibleWidth) * 100,
    offsetYPct: clamped.top === 0 ? 0 : -(clamped.top / visibleHeight) * 100,
  };
}
