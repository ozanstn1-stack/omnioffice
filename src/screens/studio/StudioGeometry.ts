/**
 * The page geometry the PDF Studio overlay needs. Mirroring `render::PageGeometry`
 * (which serializes with `width_pt`), this is normalized once here.
 */
export interface StudioPageGeometry {
  width: number;
  height: number;
  rotation: number;
}

/** Displayed page size (rotation 90/270 swaps width and height). */
export function displayedPageSize(geometry: StudioPageGeometry): { width: number; height: number } {
  const rotation = ((geometry.rotation % 360) + 360) % 360;
  return rotation === 90 || rotation === 270
    ? { width: geometry.height, height: geometry.width }
    : { width: geometry.width, height: geometry.height };
}

/**
 * Maps a page-space rectangle (bottom-left origin, y up) to a normalized
 * rectangle over the rendered page image (top-left origin, y down). The page
 * rotation is applied exactly like `docutil::Matrix::display_to_page`, only
 * inverted - getting this wrong selects the wrong object.
 */
export function pageRectToDisplayRect(
  rect: [number, number, number, number],
  geometry: StudioPageGeometry,
): { x: number; y: number; w: number; h: number } {
  const x0 = Math.min(rect[0], rect[2]);
  const y0 = Math.min(rect[1], rect[3]);
  const x1 = Math.max(rect[0], rect[2]);
  const y1 = Math.max(rect[1], rect[3]);
  const rotation = ((geometry.rotation % 360) + 360) % 360;
  const map = (x: number, y: number): [number, number] => {
    switch (rotation) {
      case 90:
        return [y, geometry.width - x];
      case 180:
        return [geometry.width - x, geometry.height - y];
      case 270:
        return [geometry.height - y, x];
      default:
        return [x, y];
    }
  };
  const corners = [map(x0, y0), map(x1, y0), map(x0, y1), map(x1, y1)];
  const minX = Math.min(...corners.map((corner) => corner[0]));
  const maxX = Math.max(...corners.map((corner) => corner[0]));
  const minY = Math.min(...corners.map((corner) => corner[1]));
  const maxY = Math.max(...corners.map((corner) => corner[1]));
  const displayed = displayedPageSize(geometry);
  return {
    x: minX / displayed.width,
    y: 1 - maxY / displayed.height,
    w: (maxX - minX) / displayed.width,
    h: (maxY - minY) / displayed.height,
  };
}

/** Inverse of `pageRectToDisplayRect`, used when a resize handle is released. */
export function displayRectToPageRect(
  rect: { x: number; y: number; w: number; h: number },
  geometry: StudioPageGeometry,
): [number, number, number, number] {
  const rotation = ((geometry.rotation % 360) + 360) % 360;
  const displayed = displayedPageSize(geometry);
  const left = rect.x * displayed.width;
  const right = (rect.x + rect.w) * displayed.width;
  const bottom = (1 - rect.y - rect.h) * displayed.height;
  const top = (1 - rect.y) * displayed.height;
  const map = (x: number, y: number): [number, number] => {
    switch (rotation) {
      case 90:
        return [geometry.width - y, x];
      case 180:
        return [geometry.width - x, geometry.height - y];
      case 270:
        return [y, geometry.height - x];
      default:
        return [x, y];
    }
  };
  const corners = [map(left, bottom), map(right, bottom), map(left, top), map(right, top)];
  const minX = Math.min(...corners.map((corner) => corner[0]));
  const maxX = Math.max(...corners.map((corner) => corner[0]));
  const minY = Math.min(...corners.map((corner) => corner[1]));
  const maxY = Math.max(...corners.map((corner) => corner[1]));
  return [minX, minY, maxX, maxY];
}

/** Deltas measured on the displayed (rotated) page back into page space. */
export function displayDeltaToPage(dx: number, dy: number, rotation: number): [number, number] {
  switch (((rotation % 360) + 360) % 360) {
    case 90:
      return [-dy, dx];
    case 180:
      return [-dx, -dy];
    case 270:
      return [dy, -dx];
    default:
      return [dx, dy];
  }
}
