/**
 * Frozen panes: which rows and columns stay put, and where they are drawn.
 *
 * `freezeRows`/`freezeCols` count the rows above and the columns left of the
 * cell that was selected when the panes were frozen. The grid keeps those
 * pinned under the headers while everything else scrolls beneath them.
 */
import { describe, expect, it } from "vitest";
import { buildRowLayout } from "./row-layout";
import {
  isUnderBand,
  mergeFrozen,
  nextFreeze,
  pinnedPosition,
  resolveFrozenBands,
  type FrozenBandInput,
} from "./freeze";

function bands(patch: Partial<FrozenBandInput> = {}) {
  const layout = buildRowLayout(100, {});
  return resolveFrozenBands({
    freezeRows: 0,
    freezeCols: 0,
    rowCount: 100,
    colCount: 26,
    rowOffset: (count) => layout.offsetOf(count),
    colOffset: (count) => count * 96,
    viewportHeight: 600,
    viewportWidth: 1000,
    ...patch,
  });
}

describe("resolveFrozenBands", () => {
  it("is empty when nothing is frozen", () => {
    expect(bands()).toEqual({ rows: 0, cols: 0, height: 0, width: 0 });
    expect(bands({ freezeRows: -3, freezeCols: Number.NaN })).toEqual({ rows: 0, cols: 0, height: 0, width: 0 });
  });

  it("measures the frozen band in canvas pixels", () => {
    expect(bands({ freezeRows: 2, freezeCols: 1 })).toEqual({ rows: 2, cols: 1, height: 48, width: 96 });
  });

  it("measures with the real row heights, hidden rows included", () => {
    const layout = buildRowLayout(100, { "0": 40, "1": 0 });
    const result = bands({ freezeRows: 3, rowOffset: (count) => layout.offsetOf(count) });
    expect(result.rows).toBe(3);
    expect(result.height).toBe(40 + 0 + 24);
  });

  it("never freezes the whole sheet", () => {
    const result = bands({ freezeRows: 100, freezeCols: 26, viewportHeight: 0, viewportWidth: 0 });
    expect(result.rows).toBe(99);
    expect(result.cols).toBe(25);
  });

  it("keeps part of the viewport scrollable", () => {
    // 60% of a 600px viewport is 360px: 15 rows of 24px.
    const result = bands({ freezeRows: 40 });
    expect(result.rows).toBe(15);
    expect(result.height).toBe(360);
    // 60% of 1000px is 600px: 6 columns of 96px.
    expect(bands({ freezeCols: 20 }).cols).toBe(6);
  });

  it("does not clamp while the viewport is unmeasured", () => {
    expect(bands({ freezeRows: 40, viewportHeight: 0 }).rows).toBe(40);
  });
});

describe("mergeFrozen", () => {
  it("puts the frozen indices in front of a window that has scrolled away", () => {
    expect(mergeFrozen(2, [40, 41, 42])).toEqual([0, 1, 40, 41, 42]);
  });

  it("does not repeat an index the window already holds", () => {
    expect(mergeFrozen(3, [1, 2, 3, 4])).toEqual([0, 1, 2, 3, 4]);
    expect(mergeFrozen(2, [0, 1, 2])).toEqual([0, 1, 2]);
  });

  it("returns the window untouched when nothing is frozen", () => {
    const window = [5, 6];
    expect(mergeFrozen(0, window)).toBe(window);
  });

  it("leaves out frozen indices the caller marks as hidden", () => {
    expect(mergeFrozen(3, [9], (index) => index === 1)).toEqual([0, 2, 9]);
  });
});

describe("pinnedPosition", () => {
  it("makes a frozen item follow the scroll and leaves the others on the sheet", () => {
    expect(pinnedPosition(0, 2, 0, 300)).toBe(300);
    expect(pinnedPosition(1, 2, 24, 300)).toBe(324);
    expect(pinnedPosition(2, 2, 48, 300)).toBe(48);
  });

  it("is the plain position when nothing is frozen", () => {
    expect(pinnedPosition(0, 0, 0, 300)).toBe(0);
  });
});

describe("isUnderBand", () => {
  it("is true for a scrolled item whose position is covered by the frozen band", () => {
    // Band 48px tall, scrolled 300px: sheet positions below 348 are covered.
    expect(isUnderBand(10, 2, 240, 300, 48)).toBe(true);
    expect(isUnderBand(10, 2, 348, 300, 48)).toBe(false);
    expect(isUnderBand(10, 2, 400, 300, 48)).toBe(false);
  });

  it("is never true for a frozen item or without a band", () => {
    expect(isUnderBand(1, 2, 24, 300, 48)).toBe(false);
    expect(isUnderBand(10, 0, 0, 300, 0)).toBe(false);
  });
});

describe("nextFreeze", () => {
  it("freezes above and left of the selected cell", () => {
    expect(nextFreeze({ rows: 0, cols: 0 }, { row: 2, col: 1 })).toEqual({ rows: 2, cols: 1 });
  });

  it("unfreezes whenever something is frozen, wherever the selection is", () => {
    expect(nextFreeze({ rows: 2, cols: 1 }, { row: 2, col: 1 })).toEqual({ rows: 0, cols: 0 });
    expect(nextFreeze({ rows: 2, cols: 1 }, { row: 7, col: 4 })).toEqual({ rows: 0, cols: 0 });
    expect(nextFreeze({ rows: 0, cols: 3 }, { row: 0, col: 0 })).toEqual({ rows: 0, cols: 0 });
  });

  it("freezes nothing at A1", () => {
    expect(nextFreeze({ rows: 0, cols: 0 }, { row: 0, col: 0 })).toEqual({ rows: 0, cols: 0 });
  });
});
