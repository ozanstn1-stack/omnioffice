/**
 * Coordinate conversion for redaction boxes.
 *
 * The page canvas reports a rectangle normalized to the rendered image with the
 * origin at the top-left, but PDF user space has its origin at the bottom-left
 * and y growing upwards. Redaction areas are sent to the engine in user space,
 * so the vertical axis has to be flipped here. Doing this with the real page
 * geometry rather than the render size is what makes a box land on the text
 * the user drew it over.
 */

export interface PageSize {
  width: number;
  height: number;
}

/**
 * The page geometry as the backend sends it.
 *
 * `render::PageGeometry` derives Serialize without `rename_all`, so its fields
 * arrive as `width_pt` / `height_pt` rather than camelCase. Going through this
 * one function keeps that quirk in a single place instead of spreading it
 * across components, where a wrong property silently reads as `undefined`.
 */
export interface WirePageGeometry {
  page: number;
  width_pt?: number;
  height_pt?: number;
  width?: number;
  height?: number;
}

/** Normalizes either spelling of a page geometry into a usable page size. */
export function normalizePageSize(geometry: WirePageGeometry): PageSize {
  const width = geometry.width_pt ?? geometry.width ?? 0;
  const height = geometry.height_pt ?? geometry.height ?? 0;
  return { width, height };
}

export interface NormalizedRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

import { clamp } from "./format";

export interface UserSpaceRect {
  left: number;
  bottom: number;
  right: number;
  top: number;
}

/**
 * Converts a normalized canvas rectangle to PDF user space.
 *
 * Returns null when the page size is unusable or the drag was a stray click
 * rather than a real box: a redaction area of zero area would remove nothing
 * while still reporting success, so it is rejected at the source.
 */
export function rectToUserSpace(rect: NormalizedRect, page: PageSize): UserSpaceRect | null {
  if (!(page.width > 0) || !(page.height > 0)) return null;
  const left = rect.x * page.width;
  const right = (rect.x + rect.w) * page.width;
  const top = (1 - rect.y) * page.height;
  const bottom = (1 - rect.y - rect.h) * page.height;
  const result: UserSpaceRect = {
    left: clamp(left, 0, page.width),
    right: clamp(right, 0, page.width),
    bottom: clamp(bottom, 0, page.height),
    top: clamp(top, 0, page.height),
  };
  if (result.right - result.left < 2 || result.top - result.bottom < 2) return null;
  return result;
}

/** The inverse mapping, used to draw a stored box back over the page. */
export function userSpaceToRect(area: UserSpaceRect, page: PageSize): NormalizedRect {
  return {
    x: area.left / page.width,
    y: 1 - area.top / page.height,
    w: (area.right - area.left) / page.width,
    h: (area.top - area.bottom) / page.height,
  };
}

/** Grows a box on every side, used for the "padding" option. */
export function padArea(area: UserSpaceRect, paddingPt: number, page: PageSize): UserSpaceRect {
  return {
    left: clamp(area.left - paddingPt, 0, page.width),
    right: clamp(area.right + paddingPt, 0, page.width),
    bottom: clamp(area.bottom - paddingPt, 0, page.height),
    top: clamp(area.top + paddingPt, 0, page.height),
  };
}
