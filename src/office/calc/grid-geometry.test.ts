/**
 * Scroll-into-view maths for the Calc grid.
 *
 * Cell coordinates are canvas pixels measured from the top-left cell (the row
 * and column headers sit in front of that origin); scroll offsets are outer
 * pixels, so the zoom maps one onto the other.
 */
import { describe, expect, it } from "vitest";
import { revealScroll, type RevealInput } from "./grid-geometry";

function input(patch: Partial<RevealInput> = {}): RevealInput {
  return {
    cell: { top: 0, left: 0, height: 24, width: 96 },
    scrollTop: 0,
    scrollLeft: 0,
    clientHeight: 300,
    clientWidth: 500,
    zoom: 1,
    headerHeight: 24,
    headerWidth: 56,
    ...patch,
  };
}

describe("revealScroll", () => {
  it("leaves a cell that is already visible alone", () => {
    expect(revealScroll(input({ cell: { top: 48, left: 96, height: 24, width: 96 } }))).toEqual({
      scrollTop: 0,
      scrollLeft: 0,
    });
  });

  it("scrolls down until the whole cell is above the bottom edge", () => {
    // The cell's bottom on screen is headerHeight + top + height.
    expect(revealScroll(input({ cell: { top: 500, left: 0, height: 24, width: 96 } })).scrollTop).toBe(
      24 + 500 + 24 - 300,
    );
  });

  it("accounts for the column header band at the bottom edge", () => {
    // Regression: row 11 ends at canvas y 324, past a 300px viewport, but the
    // header offset used to be left out and the row stayed clipped.
    expect(revealScroll(input({ cell: { top: 276, left: 0, height: 24, width: 96 } })).scrollTop).toBe(24);
    // Row 10 ends exactly at the edge and needs no scrolling.
    expect(revealScroll(input({ cell: { top: 252, left: 0, height: 24, width: 96 } })).scrollTop).toBe(0);
  });

  it("scrolls up so the cell sits just under the header", () => {
    expect(revealScroll(input({ scrollTop: 1000, cell: { top: 120, left: 0, height: 24, width: 96 } })).scrollTop).toBe(
      120,
    );
  });

  it("uses the real top of a cell below custom-height rows", () => {
    // 40 rows of 24px plus one of 80px above: the cell starts at 1040, not at
    // row * its own height.
    const result = revealScroll(input({ cell: { top: 1040, left: 0, height: 30, width: 96 } }));
    expect(result.scrollTop).toBe(24 + 1040 + 30 - 300);
  });

  it("shows the top of a cell taller than the viewport", () => {
    const result = revealScroll(input({ cell: { top: 800, left: 0, height: 600, width: 96 } }));
    expect(result.scrollTop).toBe(800);
  });

  it("scrolls horizontally with the row header on the left", () => {
    expect(revealScroll(input({ cell: { top: 0, left: 900, height: 24, width: 96 } })).scrollLeft).toBe(
      56 + 900 + 96 - 500,
    );
    expect(
      revealScroll(input({ scrollLeft: 700, cell: { top: 0, left: 200, height: 24, width: 96 } })).scrollLeft,
    ).toBe(200);
  });

  it("maps canvas pixels to outer pixels with the zoom", () => {
    const zoomed = revealScroll(input({ zoom: 2, cell: { top: 500, left: 900, height: 24, width: 96 } }));
    expect(zoomed.scrollTop).toBe((24 + 500 + 24) * 2 - 300);
    expect(zoomed.scrollLeft).toBe((56 + 900 + 96) * 2 - 500);
    const up = revealScroll(
      input({ zoom: 0.5, scrollTop: 400, scrollLeft: 400, cell: { top: 100, left: 100, height: 24, width: 96 } }),
    );
    expect(up).toEqual({ scrollTop: 50, scrollLeft: 50 });
  });

  it("never scrolls to a negative offset", () => {
    expect(revealScroll(input({ scrollTop: 10, cell: { top: 0, left: 0, height: 24, width: 96 } }))).toEqual({
      scrollTop: 0,
      scrollLeft: 0,
    });
  });

  describe("with frozen panes", () => {
    it("keeps a scrolled-to cell clear of the frozen rows and columns", () => {
      // 48px of frozen rows and 96px of frozen columns cover the top-left.
      const result = revealScroll(
        input({
          scrollTop: 200,
          scrollLeft: 300,
          frozenHeight: 48,
          frozenWidth: 96,
          cell: { top: 220, left: 320, height: 24, width: 96 },
        }),
      );
      // Row top 220 is visible only below the frozen band when scrollTop <= 220 - 48.
      expect(result.scrollTop).toBe(172);
      expect(result.scrollLeft).toBe(224);
    });

    it("does not move for a cell in a frozen row or column", () => {
      const frozenRow = revealScroll(
        input({ scrollTop: 400, frozenHeight: 48, rowFrozen: true, cell: { top: 24, left: 0, height: 24, width: 96 } }),
      );
      expect(frozenRow.scrollTop).toBe(400);
      const frozenCol = revealScroll(
        input({ scrollLeft: 400, frozenWidth: 96, colFrozen: true, cell: { top: 0, left: 0, height: 24, width: 96 } }),
      );
      expect(frozenCol.scrollLeft).toBe(400);
    });

    it("still scrolls the free axis for a cell frozen on the other one", () => {
      const result = revealScroll(
        input({ frozenWidth: 96, colFrozen: true, cell: { top: 900, left: 10, height: 24, width: 96 } }),
      );
      expect(result.scrollTop).toBe(24 + 900 + 24 - 300);
      expect(result.scrollLeft).toBe(0);
    });
  });
});
