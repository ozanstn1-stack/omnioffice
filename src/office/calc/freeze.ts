/**
 * Frozen panes for the Calc grid.
 *
 * A sheet stores `freezeRows`/`freezeCols`: how many rows at the top and
 * columns at the left stay put. The grid draws those as header-style sticky
 * layers (pinned to the scroll offset) in front of the cells that scroll, so
 * everything here is plain arithmetic over canvas pixels.
 */

/** The share of the viewport a frozen band may cover; the rest keeps scrolling. */
const MAX_BAND_SHARE = 0.6;

export interface FrozenBands {
  /** Rows and columns that are pinned, after clamping to what fits. */
  rows: number;
  cols: number;
  /** Size of the pinned band in canvas pixels (hidden rows take no room). */
  height: number;
  width: number;
}

export interface FrozenBandInput {
  freezeRows: number;
  freezeCols: number;
  rowCount: number;
  colCount: number;
  /** Canvas pixels covered by the first `count` rows / columns. */
  rowOffset: (count: number) => number;
  colOffset: (count: number) => number;
  /** Unscaled size of the scroll viewport; 0 while it is not measured yet. */
  viewportHeight: number;
  viewportWidth: number;
}

/** The largest count in `[0, wanted]` whose extent fits in `limit` pixels. */
function fitCount(wanted: number, limit: number, offset: (count: number) => number): number {
  if (limit <= 0 || offset(wanted) <= limit) return wanted;
  let low = 0;
  let high = wanted;
  while (low < high) {
    const middle = (low + high + 1) >>> 1;
    if (offset(middle) <= limit) low = middle;
    else high = middle - 1;
  }
  return low;
}

/**
 * Turns the stored counts into the bands to draw. A document can ask for more
 * than the sheet has, or for a band bigger than the window: the result is
 * clamped so there is always something left to scroll.
 */
export function resolveFrozenBands(input: FrozenBandInput): FrozenBands {
  const whole = (value: number) => (Number.isFinite(value) ? Math.max(0, Math.floor(value)) : 0);
  const rows = fitCount(
    Math.min(whole(input.freezeRows), Math.max(0, input.rowCount - 1)),
    input.viewportHeight * MAX_BAND_SHARE,
    input.rowOffset,
  );
  const cols = fitCount(
    Math.min(whole(input.freezeCols), Math.max(0, input.colCount - 1)),
    input.viewportWidth * MAX_BAND_SHARE,
    input.colOffset,
  );
  return {
    rows,
    cols,
    height: rows > 0 ? input.rowOffset(rows) : 0,
    width: cols > 0 ? input.colOffset(cols) : 0,
  };
}

/**
 * The indices to render: the frozen ones (they stay on screen however far the
 * window has scrolled) followed by the scrolled window. `window` is ascending.
 */
export function mergeFrozen(
  frozen: number,
  window: readonly number[],
  isHidden?: (index: number) => boolean,
): number[] {
  if (frozen <= 0) return window as number[];
  const out: number[] = [];
  for (let index = 0; index < frozen; index += 1) {
    if (!isHidden?.(index)) out.push(index);
  }
  for (const index of window) {
    if (index >= frozen) out.push(index);
  }
  return out;
}

/**
 * Where an item is drawn along one axis. Frozen items follow the scroll offset
 * (already divided by the zoom); the rest keep their place on the sheet.
 */
export function pinnedPosition(index: number, frozen: number, position: number, scroll: number): number {
  return index < frozen ? position + scroll : position;
}

/** True when a scrolled item sits underneath the frozen band and cannot be seen. */
export function isUnderBand(index: number, frozen: number, position: number, scroll: number, band: number): boolean {
  return frozen > 0 && index >= frozen && position < scroll + band;
}

/**
 * The panes after pressing "Freeze panes": anything frozen is released,
 * otherwise everything above and left of the selected cell is frozen.
 */
export function nextFreeze(
  current: { rows: number; cols: number },
  focus: { row: number; col: number },
): { rows: number; cols: number } {
  if (current.rows > 0 || current.cols > 0) return { rows: 0, cols: 0 };
  return { rows: focus.row, cols: focus.col };
}
