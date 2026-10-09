/**
 * Freehand ink helpers for the slideshow's pen and eraser.
 *
 * Strokes live only in the running show (nothing reaches the deck), in slide
 * coordinates. `simplifyStroke` drops redundant points while drawing,
 * `strokePath` builds the SVG path and `strokeHit` finds the stroke under the
 * eraser.
 */
import { distanceToSegment, type Point } from "./connectors";

export interface StrokePoint {
  x: number;
  y: number;
}

/** Keeps only the points that carry shape: a new point is accepted once it is
 * farther than `minDistance` from the previous kept point. */
export function simplifyStroke(points: StrokePoint[], minDistance = 2): StrokePoint[] {
  if (points.length <= 2) return [...points];
  const out: StrokePoint[] = [points[0]];
  for (let index = 1; index < points.length - 1; index += 1) {
    const previous = out[out.length - 1];
    const point = points[index];
    if (Math.hypot(point.x - previous.x, point.y - previous.y) >= minDistance) out.push(point);
  }
  const last = points[points.length - 1];
  const tail = out[out.length - 1];
  if (tail.x !== last.x || tail.y !== last.y) out.push(last);
  return out;
}

/** SVG path data for one stroke (empty for no points, a dot for one). */
export function strokePath(points: StrokePoint[]): string {
  if (points.length === 0) return "";
  const [first, ...rest] = points;
  let path = `M ${round(first.x)} ${round(first.y)}`;
  if (rest.length === 0) path += ` L ${round(first.x)} ${round(first.y)}`;
  for (const point of rest) path += ` L ${round(point.x)} ${round(point.y)}`;
  return path;
}

function round(value: number): number {
  return Math.round(value * 10) / 10;
}

/**
 * Index of the first stroke within `threshold` of `point`, or -1. Distance is
 * measured to the drawn segments, so an eraser catches a long stroke anywhere
 * along its length.
 */
export function strokeHit(strokes: StrokePoint[][], point: StrokePoint, threshold = 12): number {
  for (let index = 0; index < strokes.length; index += 1) {
    const stroke = strokes[index];
    if (stroke.length === 0) continue;
    if (stroke.length === 1) {
      if (Math.hypot(point.x - stroke[0].x, point.y - stroke[0].y) <= threshold) return index;
      continue;
    }
    for (let segment = 0; segment < stroke.length - 1; segment += 1) {
      if (distanceToSegment(point, stroke[segment] as Point, stroke[segment + 1] as Point) <= threshold) return index;
    }
  }
  return -1;
}
