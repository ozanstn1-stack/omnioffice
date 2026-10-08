/**
 * Row geometry: offsets, hit-testing and the virtualised window.
 *
 * The grid used to place every row at `row * 24`, so an imported row height
 * overlapped its neighbours and a row hidden by a filter (height 0) left a
 * 24px gap. These tests pin the prefix-sum table that replaces that formula.
 */
import { describe, expect, it } from "vitest";
import { buildRowLayout, rowLayoutFor } from "./row-layout";

/** Reference implementation: the obvious O(n) prefix sum. */
function naiveOffsets(rowCount: number, heights: Record<string, number>, fallback = 24): number[] {
  const offsets = [0];
  for (let row = 0; row < rowCount; row += 1) offsets.push(offsets[row] + (heights[String(row)] ?? fallback));
  return offsets;
}

describe("row layout without overrides", () => {
  const layout = buildRowLayout(100, {});

  it("is the uniform grid", () => {
    expect(layout.total).toBe(2400);
    expect(layout.offsetOf(0)).toBe(0);
    expect(layout.offsetOf(7)).toBe(168);
    expect(layout.heightOf(7)).toBe(24);
    expect(layout.isHidden(7)).toBe(false);
  });

  it("finds the row under a y coordinate", () => {
    expect(layout.rowAtY(0)).toBe(0);
    expect(layout.rowAtY(23.9)).toBe(0);
    expect(layout.rowAtY(24)).toBe(1);
    expect(layout.rowAtY(2399)).toBe(99);
  });

  it("clamps rows and coordinates outside the sheet", () => {
    expect(layout.rowAtY(-50)).toBe(0);
    expect(layout.rowAtY(1e9)).toBe(99);
    expect(layout.offsetOf(-3)).toBe(0);
    expect(layout.offsetOf(100)).toBe(2400);
    expect(layout.offsetOf(5000)).toBe(2400);
    expect(layout.heightOf(-1)).toBe(24);
  });

  it("honours another default height", () => {
    const tall = buildRowLayout(10, {}, 30);
    expect(tall.total).toBe(300);
    expect(tall.rowAtY(61)).toBe(2);
  });
});

describe("custom row heights", () => {
  const layout = buildRowLayout(10, { "1": 48, "3": 10 });

  it("places each row after the real height of the rows above it", () => {
    // Heights: 24, 48, 24, 10, 24 ...
    expect(layout.offsetOf(0)).toBe(0);
    expect(layout.offsetOf(1)).toBe(24);
    expect(layout.offsetOf(2)).toBe(72);
    expect(layout.offsetOf(3)).toBe(96);
    expect(layout.offsetOf(4)).toBe(106);
    expect(layout.heightOf(1)).toBe(48);
    expect(layout.total).toBe(24 * 8 + 48 + 10);
  });

  it("hit-tests inside tall and short rows", () => {
    expect(layout.rowAtY(30)).toBe(1);
    expect(layout.rowAtY(71.9)).toBe(1);
    expect(layout.rowAtY(72)).toBe(2);
    expect(layout.rowAtY(100)).toBe(3);
    expect(layout.rowAtY(106)).toBe(4);
  });

  it("supports fractional heights", () => {
    const fractional = buildRowLayout(5, { "0": 17.5, "2": 30.25 });
    expect(fractional.offsetOf(3)).toBeCloseTo(17.5 + 24 + 30.25);
    expect(fractional.rowAtY(17.4)).toBe(0);
    expect(fractional.rowAtY(17.5)).toBe(1);
  });

  it("ignores garbage and rows beyond the sheet", () => {
    const messy = buildRowLayout(5, {
      "1": Number.NaN,
      "2": -10,
      x: 99,
      "1.5": 99,
      "9": 500,
      "-1": 500,
      "3": Number.POSITIVE_INFINITY,
    });
    expect(messy.total).toBe(120);
    expect(messy.heightOf(1)).toBe(24);
    expect(messy.heightOf(9)).toBe(24);
  });

  it("agrees with a naive prefix sum on a random sheet", () => {
    const heights: Record<string, number> = {};
    let seed = 7;
    const random = () => {
      seed = (seed * 1664525 + 1013904223) % 4294967296;
      return seed / 4294967296;
    };
    for (let row = 0; row < 1000; row += 1) {
      const roll = random();
      if (roll < 0.15) heights[String(row)] = 0;
      else if (roll < 0.4) heights[String(row)] = Math.round(random() * 90 + 8);
    }
    const random1000 = buildRowLayout(1000, heights);
    const expected = naiveOffsets(1000, heights);
    expect(random1000.total).toBe(expected[1000]);
    for (let row = 0; row <= 1000; row += 1) expect(random1000.offsetOf(row)).toBe(expected[row]);
    for (let probe = 0; probe < 2000; probe += 1) {
      const y = random() * expected[1000];
      let row = 0;
      while (row < 999 && !(expected[row] <= y && y < expected[row + 1])) row += 1;
      expect(random1000.rowAtY(y)).toBe(row);
    }
  });
});

describe("hidden rows", () => {
  const layout = buildRowLayout(10, { "1": 0, "2": 0, "9": 0 });

  it("take no room", () => {
    expect(layout.isHidden(1)).toBe(true);
    expect(layout.heightOf(1)).toBe(0);
    expect(layout.offsetOf(1)).toBe(24);
    expect(layout.offsetOf(3)).toBe(24);
    expect(layout.total).toBe(24 * 7);
  });

  it("are never the answer of a hit test", () => {
    expect(layout.rowAtY(24)).toBe(3);
    expect(layout.rowAtY(30)).toBe(3);
    // The trailing hidden row 9 does not extend the sheet.
    expect(layout.rowAtY(1e9)).toBe(8);
  });

  it("are skipped from a leading position too", () => {
    const leading = buildRowLayout(5, { "0": 0, "1": 0 });
    expect(leading.rowAtY(-10)).toBe(2);
    expect(leading.rowAtY(0)).toBe(2);
  });

  it("are all there is when the whole sheet is hidden", () => {
    const hidden = buildRowLayout(3, { "0": 0, "1": 0, "2": 0 });
    expect(hidden.total).toBe(0);
    expect(hidden.rowAtY(10)).toBe(0);
    expect(hidden.visibleRows(0, 100)).toEqual([]);
  });

  it("let selection steps jump over them", () => {
    expect(layout.nextVisible(0, 1)).toBe(3);
    expect(layout.nextVisible(3, -1)).toBe(0);
    expect(layout.nextVisible(8, 1)).toBe(8);
    expect(layout.nextVisible(0, -1)).toBe(0);
    expect(layout.nextVisible(0, 3)).toBe(5);
    expect(layout.nextVisible(5, -2)).toBe(3);
    expect(layout.nextVisible(5, 0)).toBe(5);
    expect(layout.nextVisible(5, 50)).toBe(8);
  });
});

describe("visible window", () => {
  it("covers the viewport plus the overscan, in order", () => {
    const layout = buildRowLayout(100, {});
    // Rows 5..7 are on screen; 2 rows before and 4 after are kept.
    expect(layout.visibleRows(120, 192)).toEqual([3, 4, 5, 6, 7, 8, 9, 10, 11]);
  });

  it("clamps at the ends of the sheet", () => {
    const layout = buildRowLayout(10, {});
    expect(layout.visibleRows(0, 48)).toEqual([0, 1, 2, 3, 4, 5]);
    expect(layout.visibleRows(10_000, 10_100)).toEqual([7, 8, 9]);
  });

  it("measures the window in pixels, not in rows", () => {
    const layout = buildRowLayout(100, { "10": 200 });
    // The tall row 10 fills the whole viewport on its own.
    expect(layout.visibleRows(300, 400, 0, 0)).toEqual([10]);
    expect(layout.visibleRows(300, 400, 1, 1)).toEqual([9, 10, 11]);
  });

  it("leaves hidden rows out and does not count them as overscan", () => {
    const layout = buildRowLayout(20, { "4": 0, "5": 0, "6": 0 });
    // Offsets: row 3 ends at 96, row 7 starts there.
    expect(layout.visibleRows(48, 96, 1, 1)).toEqual([1, 2, 3, 7]);
  });

  it("shows nothing for an empty sheet", () => {
    expect(buildRowLayout(0, {}).visibleRows(0, 100)).toEqual([]);
    expect(buildRowLayout(0, {}).rowAtY(10)).toBe(0);
    expect(buildRowLayout(0, {}).total).toBe(0);
  });
});

describe("large sheets", () => {
  const rowCount = 100_000;
  const heights: Record<string, number> = {};
  for (let row = 0; row < rowCount; row += 7) heights[String(row)] = row % 14 === 0 ? 40 : 0;
  // One huge hidden block in the middle, as a filter leaves behind.
  for (let row = 20_000; row < 90_000; row += 1) heights[String(row)] = 0;

  it("builds quickly and stays consistent", () => {
    const started = performance.now();
    const layout = buildRowLayout(rowCount, heights);
    expect(performance.now() - started).toBeLessThan(500);
    const expected = naiveOffsets(rowCount, heights);
    expect(layout.total).toBe(expected[rowCount]);
    for (const row of [0, 1, 7, 19_999, 20_000, 90_000, 99_999, 100_000]) {
      expect(layout.offsetOf(row)).toBe(expected[row]);
    }
  });

  it("answers hit tests and windows in logarithmic time", () => {
    const layout = buildRowLayout(rowCount, heights);
    const started = performance.now();
    for (let probe = 0; probe < 20_000; probe += 1) {
      const row = layout.rowAtY((probe * 997) % layout.total);
      expect(layout.isHidden(row)).toBe(false);
    }
    // A window that straddles the 70 000 hidden rows must not walk them.
    const edge = layout.offsetOf(20_000);
    for (let probe = 0; probe < 2000; probe += 1) {
      const rows = layout.visibleRows(edge - 200, edge + 400);
      expect(rows.length).toBeLessThan(80);
    }
    expect(performance.now() - started).toBeLessThan(1500);
  });

  it("jumps over a hidden block in one step", () => {
    const layout = buildRowLayout(rowCount, heights);
    const last = layout.nextVisible(19_999, 1);
    expect(last).toBeGreaterThanOrEqual(20_000);
    expect(layout.isHidden(last)).toBe(false);
    expect(layout.nextVisible(last, -1)).toBeLessThan(20_000);
  });
});

describe("memoisation", () => {
  it("reuses the layout while the heights object and row count are unchanged", () => {
    const heights = { "2": 40 };
    const first = rowLayoutFor(50, heights);
    expect(rowLayoutFor(50, heights)).toBe(first);
    expect(rowLayoutFor(51, heights)).not.toBe(first);
    expect(rowLayoutFor(50, { ...heights })).not.toBe(first);
  });

  it("is keyed by the default height as well", () => {
    const heights = {};
    expect(rowLayoutFor(5, heights, 20)).not.toBe(rowLayoutFor(5, heights, 24));
  });
});
