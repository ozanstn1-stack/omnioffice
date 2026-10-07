/**
 * Row geometry for the Calc grid.
 *
 * A sheet stores a height only for the rows that differ from the default, and
 * a height of 0 is how a filter hides a row. Placing a row at `row * 24`
 * ignored both: imported heights overlapped their neighbours and hidden rows
 * left a gap. This module turns those overrides into a prefix-sum table so the
 * grid can ask where a row starts, which row sits under a y coordinate, and
 * which rows to render for a scroll position.
 *
 * Only the overridden rows are tabulated: the offset of a row is
 * `row * defaultHeight` plus the running sum of every override above it, found
 * by binary search. Building costs O(k log k) for k custom rows (not O(rows)),
 * a lookup O(log k), and a hit test O(log rows * log k), so a 100 000 row sheet
 * with a filter on it stays cheap.
 */

export const DEFAULT_ROW_HEIGHT = 24;

export interface RowLayout {
  readonly rowCount: number;
  readonly defaultHeight: number;
  /** Combined height of all rows, in px. Hidden rows contribute nothing. */
  readonly total: number;
  /** Height of one row; 0 when it is hidden. */
  heightOf(row: number): number;
  isHidden(row: number): boolean;
  /** Top edge of a row; the bottom edge of the last row for `rowCount`. */
  offsetOf(row: number): number;
  /**
   * The visible row under a y coordinate. Hidden rows are never returned;
   * coordinates outside the sheet resolve to the first or last visible row.
   */
  rowAtY(y: number): number;
  /**
   * Moves `step` visible rows from `row` (negative goes up), skipping hidden
   * rows and stopping at the first or last visible row.
   */
  nextVisible(row: number, step: number): number;
  /**
   * The visible rows to render for the viewport `[top, bottom)`: the rows that
   * intersect it plus `before`/`after` extra visible rows as overscan.
   */
  visibleRows(top: number, bottom: number, before?: number, after?: number): number[];
}

/** Number of entries in `sorted` that are smaller than `value`. */
function lowerBound(sorted: readonly number[], value: number): number {
  let low = 0;
  let high = sorted.length;
  while (low < high) {
    const middle = (low + high) >>> 1;
    if (sorted[middle] < value) low = middle + 1;
    else high = middle;
  }
  return low;
}

export function buildRowLayout(
  rowCount: number,
  rowHeights: Readonly<Record<string, number>>,
  defaultHeight = DEFAULT_ROW_HEIGHT,
): RowLayout {
  const count = Math.max(0, Math.floor(rowCount));

  // The overridden rows in ascending order. Keys that are not row indices and
  // heights that are not finite, non-negative numbers fall back to the default.
  const overrides: Array<[number, number]> = [];
  for (const key of Object.keys(rowHeights)) {
    const row = Number(key);
    const height = rowHeights[key];
    if (!Number.isInteger(row) || row < 0 || row >= count) continue;
    if (typeof height !== "number" || !Number.isFinite(height) || height < 0) continue;
    if (height === defaultHeight) continue;
    overrides.push([row, height]);
  }
  overrides.sort((a, b) => a[0] - b[0]);

  const rows = overrides.map(([row]) => row);
  const heights = overrides.map(([, height]) => height);
  // cumulative[i]: how far the overrides 0..i push the rows below them.
  const cumulative: number[] = [];
  let running = 0;
  for (const height of heights) {
    running += height - defaultHeight;
    cumulative.push(running);
  }
  // Contiguous hidden rows form runs; the ends let a step skip one in O(log k).
  const runStart: number[] = rows.slice();
  const runEnd: number[] = rows.slice();
  for (let index = 1; index < rows.length; index += 1) {
    if (heights[index] === 0 && heights[index - 1] === 0 && rows[index] === rows[index - 1] + 1) {
      runStart[index] = runStart[index - 1];
    }
  }
  for (let index = rows.length - 2; index >= 0; index -= 1) {
    if (heights[index] === 0 && heights[index + 1] === 0 && rows[index + 1] === rows[index] + 1) {
      runEnd[index] = runEnd[index + 1];
    }
  }

  const overrideIndex = (row: number): number => {
    const index = lowerBound(rows, row);
    return rows[index] === row ? index : -1;
  };
  const heightOf = (row: number): number => {
    if (row < 0 || row >= count) return defaultHeight;
    const index = overrideIndex(row);
    return index < 0 ? defaultHeight : heights[index];
  };
  const isHidden = (row: number): boolean => row >= 0 && row < count && heightOf(row) === 0;
  const offsetOf = (row: number): number => {
    const clamped = Math.min(Math.max(0, Math.floor(row)), count);
    const above = lowerBound(rows, clamped);
    return clamped * defaultHeight + (above > 0 ? cumulative[above - 1] : 0);
  };
  const total = offsetOf(count);

  /** One visible row down (+1) or up (-1) from `row`, or null at the edge. */
  const stepOnce = (row: number, direction: 1 | -1): number | null => {
    let next = row + direction;
    if (next < 0 || next >= count) return null;
    if (isHidden(next)) {
      const index = overrideIndex(next);
      next = direction === 1 ? runEnd[index] + 1 : runStart[index] - 1;
      if (next < 0 || next >= count) return null;
    }
    return next;
  };
  const nextVisible = (row: number, step: number): number => {
    const direction = step < 0 ? -1 : 1;
    let current = row;
    for (let left = Math.abs(Math.trunc(step)); left > 0; left -= 1) {
      const next = stepOnce(current, direction);
      if (next === null) break;
      current = next;
    }
    return current;
  };

  const rowAtY = (y: number): number => {
    if (count === 0 || total === 0) return 0;
    const target = Number.isNaN(y) ? 0 : y;
    // The last row that starts at or above y. Hidden rows share their offset
    // with the row after them, so the answer is the visible one.
    let low = 0;
    let high = count - 1;
    while (low < high) {
      const middle = (low + high + 1) >>> 1;
      if (offsetOf(middle) <= target) low = middle;
      else high = middle - 1;
    }
    if (!isHidden(low)) return low;
    // Only reachable outside the sheet: above it with leading hidden rows, or
    // below it with trailing ones.
    return stepOnce(low, 1) ?? stepOnce(low, -1) ?? 0;
  };

  const visibleRows = (top: number, bottom: number, before = 2, after = 4): number[] => {
    if (count === 0 || total === 0) return [];
    const first = rowAtY(top);
    let last = first;
    if (bottom > top) {
      last = rowAtY(bottom);
      // A row that starts exactly at the bottom edge is not on screen.
      if (offsetOf(last) >= bottom && last > first) last = nextVisible(last, -1);
      last = Math.max(first, last);
    }
    const start = nextVisible(first, -before);
    const end = nextVisible(last, after);
    const out: number[] = [];
    for (let row: number | null = start; row !== null && row <= end; row = stepOnce(row, 1)) out.push(row);
    return out;
  };

  return {
    rowCount: count,
    defaultHeight,
    total,
    heightOf,
    isHidden,
    offsetOf,
    rowAtY,
    nextVisible,
    visibleRows,
  };
}

const layoutCache = new WeakMap<object, Map<string, RowLayout>>();

/**
 * The layout for a sheet's row heights, memoized on the identity of the
 * `rowHeights` object: the sheet model is immutable, so the same object means
 * the same heights and an edit that touches them produces a new one. Callers
 * outside React (scroll-into-view, gestures) share the table the render built.
 */
export function rowLayoutFor(
  rowCount: number,
  rowHeights: Readonly<Record<string, number>>,
  defaultHeight = DEFAULT_ROW_HEIGHT,
): RowLayout {
  let perHeights = layoutCache.get(rowHeights);
  if (!perHeights) {
    perHeights = new Map();
    layoutCache.set(rowHeights, perHeights);
  }
  const key = `${rowCount}:${defaultHeight}`;
  let layout = perHeights.get(key);
  if (!layout) {
    layout = buildRowLayout(rowCount, rowHeights, defaultHeight);
    perHeights.set(key, layout);
  }
  return layout;
}
