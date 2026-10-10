/**
 * Display-space helpers for the annotation overlay. Display space has its
 * origin at the top-left of the rendered page and is measured in PDF points;
 * it is the format `pdf_list_annotations` reports and every annotation
 * command accepts. Everything here is pure so the coordinate math can be
 * unit-tested without a DOM.
 */

export interface DisplayPoint {
  x: number;
  y: number;
}

/** Client coordinates -> display-space points, clamped to the page. */
export function pointToDisplay(
  point: { clientX: number; clientY: number },
  bounds: { left: number; top: number; width: number; height: number },
  displayWidth: number,
  displayHeight: number,
): DisplayPoint | null {
  if (bounds.width < 1 || bounds.height < 1 || displayWidth < 1 || displayHeight < 1) return null;
  const x = ((point.clientX - bounds.left) / bounds.width) * displayWidth;
  const y = ((point.clientY - bounds.top) / bounds.height) * displayHeight;
  return {
    x: Math.max(0, Math.min(displayWidth, x)),
    y: Math.max(0, Math.min(displayHeight, y)),
  };
}

/** SVG path strings for freehand strokes in absolute display points. */
export function strokesToPaths(strokes: number[][][] | null | undefined): string[] {
  if (!strokes?.length) return [];
  const paths: string[] = [];
  for (const stroke of strokes) {
    if (stroke.length < 2) continue;
    paths.push(stroke.map(([x, y], index) => `${index === 0 ? "M" : "L"} ${x} ${y}`).join(" "));
  }
  return paths;
}
