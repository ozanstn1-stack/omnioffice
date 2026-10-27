/**
 * Geometry for the scanner's corner picker.
 *
 * The picker works in fractions of the photo (0..1 on both axes) so it does
 * not care how large the preview is drawn; `office_scan_pages` wants the
 * corners in pixels of the upright full-size photo, which the preview command
 * reports (`office_scan_preview`). The Rust side orders the corners itself,
 * so a quad dragged into any order still comes out flat.
 */

/** A corner as fractions of the photo's width and height. */
export type Corner = [number, number];

/** Where the corners start: a little inside the photo, clockwise from top-left. */
const INSET = 0.06;

export function defaultCorners(): Corner[] {
  return [
    [INSET, INSET],
    [1 - INSET, INSET],
    [1 - INSET, 1 - INSET],
    [INSET, 1 - INSET],
  ];
}

const clamp = (value: number) => Math.min(1, Math.max(0, value));

/** Moves one corner, keeping it inside the photo. */
export function moveCorner(corners: Corner[], index: number, to: Corner): Corner[] {
  return corners.map((corner, i) => (i === index ? [clamp(to[0]), clamp(to[1])] : corner));
}

/** The corners in pixels of a `width`x`height` photo. */
export function cornersToPixels(corners: Corner[], width: number, height: number): [number, number][] {
  return corners.map(([x, y]) => [Math.round(x * width), Math.round(y * height)]);
}

/** Pixel corners back to the picker's fractions. */
export function pixelsToCorners(pixels: [number, number][], width: number, height: number): Corner[] {
  return pixels.map(([x, y]) => [width ? x / width : 0, height ? y / height : 0]);
}

/** True when the corners frame the whole photo: nothing to crop. */
export function isFullFrame(corners: Corner[]): boolean {
  const near = (value: number, target: number) => Math.abs(value - target) < 0.005;
  const frame: Corner[] = [
    [0, 0],
    [1, 0],
    [1, 1],
    [0, 1],
  ];
  return frame.every((target) => corners.some(([x, y]) => near(x, target[0]) && near(y, target[1])));
}
