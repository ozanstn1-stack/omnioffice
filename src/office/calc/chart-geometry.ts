/**
 * The geometry behind the chart overlay: pie and doughnut rings, the XY scatter
 * plot with its axis scale, and the flavours of `c:scatterStyle`. Pure
 * functions, so the SVG the editor draws can be checked without rendering it.
 */

/** A doughnut's hole as a percentage of the radius when the chart does not say. */
export const DEFAULT_HOLE_SIZE = 50;
const MIN_HOLE_SIZE = 10;
const MAX_HOLE_SIZE = 90;

/** The hole of a doughnut as a whole percentage inside 10..90, 50 when unset. */
export function holeSizeOf(chart: { holeSize?: number | null }): number {
  const size = chart.holeSize;
  if (typeof size !== "number" || !Number.isFinite(size)) return DEFAULT_HOLE_SIZE;
  return Math.min(MAX_HOLE_SIZE, Math.max(MIN_HOLE_SIZE, Math.round(size)));
}

const round = (value: number) => Math.round(value * 100) / 100;

export interface RingSlice {
  /** SVG path data of the slice (a wedge for a pie, an annular sector for a ring). */
  path: string;
  color: string;
  /** Share of the ring this slice takes, 0..1. */
  fraction: number;
  /** Where a label for the slice goes: the middle of the slice. */
  label: { x: number; y: number };
}

/**
 * Slices of a pie (`inner` 0) or of a ring between two radii. Negative and
 * non-finite values take no space; a ring with nothing to show has no slices.
 * A lone slice is drawn as a full circle, which a single SVG arc cannot do.
 */
export function ringSlices(
  values: readonly number[],
  palette: readonly string[],
  ring: { cx: number; cy: number; inner: number; outer: number },
): RingSlice[] {
  const { cx, cy, inner, outer } = ring;
  const shares = values.map((value) => (Number.isFinite(value) && value > 0 ? value : 0));
  const total = shares.reduce((sum, value) => sum + value, 0);
  if (total <= 0) return [];
  const at = (radius: number, angle: number) =>
    `${round(cx + radius * Math.cos(angle))} ${round(cy + radius * Math.sin(angle))}`;
  const slices: RingSlice[] = [];
  let start = -Math.PI / 2;
  const nonEmpty = shares.filter((share) => share > 0).length;
  shares.forEach((share, index) => {
    if (share <= 0) return;
    const sweep = (share / total) * Math.PI * 2;
    const end = start + sweep;
    const middle = start + sweep / 2;
    const labelRadius = inner > 0 ? (inner + outer) / 2 : outer * 0.65;
    let path: string;
    if (nonEmpty === 1) {
      const circle = (radius: number, direction: 0 | 1) =>
        `M ${at(radius, 0)} A ${radius} ${radius} 0 1 ${direction} ${at(radius, Math.PI)} A ${radius} ${radius} 0 1 ${direction} ${at(radius, 0)} Z`;
      path = inner > 0 ? `${circle(outer, 1)} ${circle(inner, 0)}` : circle(outer, 1);
    } else {
      const large = sweep > Math.PI ? 1 : 0;
      path =
        inner > 0
          ? `M ${at(outer, start)} A ${outer} ${outer} 0 ${large} 1 ${at(outer, end)} L ${at(inner, end)} A ${inner} ${inner} 0 ${large} 0 ${at(inner, start)} Z`
          : `M ${round(cx)} ${round(cy)} L ${at(outer, start)} A ${outer} ${outer} 0 ${large} 1 ${at(outer, end)} Z`;
    }
    slices.push({
      path,
      color: palette[index % palette.length],
      fraction: share / total,
      label: { x: round(cx + labelRadius * Math.cos(middle)), y: round(cy + labelRadius * Math.sin(middle)) },
    });
    start = end;
  });
  return slices;
}

/**
 * The radii of the concentric rings of a multi-series doughnut. The first
 * series is the innermost ring, as in Excel; one series fills the whole band.
 */
export function ringBands(count: number, radius: number, holeSize: number): Array<{ inner: number; outer: number }> {
  const rings = Math.max(1, count);
  const hole = (radius * holeSize) / 100;
  const thickness = (radius - hole) / rings;
  return Array.from({ length: rings }, (_unused, index) => ({
    inner: round(hole + thickness * index),
    outer: round(hole + thickness * (index + 1)),
  }));
}

// ---------------------------------------------------------------------------
// Scatter
// ---------------------------------------------------------------------------

export interface ScatterStyle {
  markers: boolean;
  line: boolean;
  smooth: boolean;
}

/** The look a `c:scatterStyle` spelling stands for; markers only when unset or unknown. */
export function scatterStyleOf(style: string | null | undefined): ScatterStyle {
  switch (style) {
    case "lineMarker":
      return { markers: true, line: true, smooth: false };
    case "line":
      return { markers: false, line: true, smooth: false };
    case "smoothMarker":
      return { markers: true, line: true, smooth: true };
    case "smooth":
      return { markers: false, line: true, smooth: true };
    default:
      return { markers: true, line: false, smooth: false };
  }
}

/** The spellings the chart dialog offers, in menu order. */
export const SCATTER_STYLES = ["marker", "lineMarker", "line", "smoothMarker", "smooth"] as const;

/**
 * The X values of a scatter chart. They are the category cells when every
 * non-empty one is a number; otherwise the points sit at 1, 2, 3 ... like a
 * line chart, which is what Excel does with text in the X range.
 */
export function scatterXValues(categories: readonly unknown[], count: number): number[] {
  const numeric = categories.map((value) =>
    typeof value === "number" ? value : typeof value === "string" && value.trim() !== "" ? Number(value) : NaN,
  );
  const usable = categories.some((value) => value !== "" && value !== undefined && value !== null);
  const allNumbers = usable && categories.every((value, index) => value === "" || Number.isFinite(numeric[index]));
  return Array.from({ length: count }, (_unused, index) => (allNumbers ? (numeric[index] ?? NaN) : index + 1));
}

export interface Point {
  x: number;
  y: number;
}

/** The points of one series: the pairs where both coordinates are numbers. */
export function scatterPoints(xs: readonly number[], ys: readonly number[]): Point[] {
  const points: Point[] = [];
  const length = Math.min(xs.length, ys.length);
  for (let index = 0; index < length; index += 1) {
    if (Number.isFinite(xs[index]) && Number.isFinite(ys[index])) points.push({ x: xs[index], y: ys[index] });
  }
  return points;
}

export interface Scale {
  min: number;
  max: number;
  ticks: number[];
}

/** A round axis range around the data with about `target` evenly spaced ticks. */
export function niceScale(min: number, max: number, target = 5): Scale {
  if (!Number.isFinite(min) || !Number.isFinite(max)) return { min: 0, max: 1, ticks: [0, 1] };
  let low = min;
  let high = max;
  if (low === high) {
    const pad = low === 0 ? 1 : Math.abs(low) * 0.1;
    low -= pad;
    high += pad;
  }
  const rawStep = (high - low) / Math.max(1, target);
  const magnitude = 10 ** Math.floor(Math.log10(rawStep));
  const residual = rawStep / magnitude;
  const step = (residual <= 1 ? 1 : residual <= 2 ? 2 : residual <= 5 ? 5 : 10) * magnitude;
  const first = Math.floor(low / step + 1e-9) * step;
  const last = Math.ceil(high / step - 1e-9) * step;
  const ticks: number[] = [];
  for (let value = first; value <= last + step / 2; value += step) ticks.push(Number(value.toPrecision(12)));
  return { min: ticks[0], max: ticks[ticks.length - 1], ticks };
}

/** A tick label without floating-point noise. */
export function formatTick(value: number): string {
  return String(Number(value.toPrecision(6)));
}

/** A smooth curve through the points (Catmull-Rom segments written as cubic Béziers). */
export function smoothPath(points: readonly Point[]): string {
  if (points.length === 0) return "";
  if (points.length < 3) return `M ${points.map((point) => `${round(point.x)} ${round(point.y)}`).join(" L ")}`;
  let path = `M ${round(points[0].x)} ${round(points[0].y)}`;
  for (let index = 0; index < points.length - 1; index += 1) {
    const p0 = points[Math.max(0, index - 1)];
    const p1 = points[index];
    const p2 = points[index + 1];
    const p3 = points[Math.min(points.length - 1, index + 2)];
    const c1 = { x: p1.x + (p2.x - p0.x) / 6, y: p1.y + (p2.y - p0.y) / 6 };
    const c2 = { x: p2.x - (p3.x - p1.x) / 6, y: p2.y - (p3.y - p1.y) / 6 };
    path += ` C ${round(c1.x)} ${round(c1.y)} ${round(c2.x)} ${round(c2.y)} ${round(p2.x)} ${round(p2.y)}`;
  }
  return path;
}
