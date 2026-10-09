/**
 * Alignment and distribution math for Impress selections.
 *
 * One object aligns to the slide (edge or centre); a multi-object selection
 * aligns to its bounding box, so "align left" lines every object up with the
 * leftmost one. Distribution needs three or more objects and equalises the gaps
 * between their edges, keeping the outermost objects where they are.
 */
export interface ArrangeBox {
  id: string;
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface ArrangeBounds {
  x: number;
  y: number;
  w: number;
  h: number;
}

export type AlignMode = "left" | "center" | "right" | "top" | "middle" | "bottom";

/** The aligned position of a box inside `bounds` for the requested mode. */
export function alignBox(box: ArrangeBox, bounds: ArrangeBounds, mode: AlignMode): { x: number; y: number } {
  switch (mode) {
    case "left":
      return { x: Math.round(bounds.x), y: box.y };
    case "center":
      return { x: Math.round(bounds.x + (bounds.w - box.w) / 2), y: box.y };
    case "right":
      return { x: Math.round(bounds.x + bounds.w - box.w), y: box.y };
    case "top":
      return { x: box.x, y: Math.round(bounds.y) };
    case "middle":
      return { x: box.x, y: Math.round(bounds.y + (bounds.h - box.h) / 2) };
    default:
      return { x: box.x, y: Math.round(bounds.y + bounds.h - box.h) };
  }
}

/**
 * New positions on one axis (`x` for horizontal, `y` for vertical) with equal
 * gaps between the edges. Fewer than three boxes are returned unchanged.
 */
export function distributePositions(boxes: ArrangeBox[], axis: "x" | "y"): Map<string, number> {
  const positions = new Map<string, number>();
  for (const box of boxes) positions.set(box.id, axis === "x" ? box.x : box.y);
  if (boxes.length < 3) return positions;
  const size = (box: ArrangeBox) => (axis === "x" ? box.w : box.h);
  const sorted = [...boxes].sort((a, b) => (axis === "x" ? a.x - b.x : a.y - b.y));
  const first = sorted[0];
  const last = sorted[sorted.length - 1];
  const start = axis === "x" ? first.x : first.y;
  const end = axis === "x" ? last.x + last.w : last.y + last.h;
  const totalSize = sorted.reduce((sum, box) => sum + size(box), 0);
  const gap = (end - start - totalSize) / (sorted.length - 1);
  let cursor = start;
  for (const box of sorted) {
    positions.set(box.id, Math.round(cursor));
    cursor += size(box) + gap;
  }
  return positions;
}
