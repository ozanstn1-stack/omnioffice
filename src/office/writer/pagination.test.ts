/**
 * Pagination rule tests. The engine is pure, so every Word-like rule -
 * splitting at line boundaries, widow/orphan control, keep-with-next,
 * keep-together, repeated table headers - is pinned down here without a
 * browser.
 */
import { describe, expect, it } from "vitest";
import { paginate, pageOfBlock, type BlockMetrics } from "./pagination";

function paragraph(index: number, lineHeights: number[], overrides: Partial<BlockMetrics> = {}): BlockMetrics {
  const lines: number[] = [];
  let total = 0;
  for (const height of lineHeights) {
    total += height;
    lines.push(total);
  }
  return {
    index,
    kind: "paragraph",
    heightPx: total,
    lines,
    rows: [],
    headerHeightPx: 0,
    keepWithNext: false,
    keepTogether: false,
    pageBreakBefore: false,
    ...overrides,
  };
}

function table(
  index: number,
  rowHeights: number[],
  headerHeightPx: number,
  overrides: Partial<BlockMetrics> = {},
): BlockMetrics {
  const rows: number[] = [];
  let total = headerHeightPx;
  for (const height of rowHeights) {
    total += height;
    rows.push(total);
  }
  return {
    index,
    kind: "table",
    heightPx: total,
    lines: [],
    rows,
    headerHeightPx,
    keepWithNext: false,
    keepTogether: false,
    pageBreakBefore: false,
    ...overrides,
  };
}

function image(index: number, height: number): BlockMetrics {
  return {
    index,
    kind: "image",
    heightPx: height,
    lines: [],
    rows: [],
    headerHeightPx: 0,
    keepWithNext: false,
    keepTogether: false,
    pageBreakBefore: false,
  };
}

function breakBlock(index: number): BlockMetrics {
  return {
    index,
    kind: "pageBreak",
    heightPx: 0,
    lines: [],
    rows: [],
    headerHeightPx: 0,
    keepWithNext: false,
    keepTogether: false,
    pageBreakBefore: false,
  };
}

describe("pagination", () => {
  it("fills a page and moves the block that no longer fits", () => {
    const pages = paginate([paragraph(0, [100]), paragraph(1, [100]), paragraph(2, [100])], 250);
    expect(pages).toHaveLength(2);
    expect(pages[0].fragments.map((fragment) => fragment.index)).toEqual([0, 1]);
    expect(pages[1].fragments.map((fragment) => fragment.index)).toEqual([2]);
  });

  it("splits a paragraph at a line boundary", () => {
    const pages = paginate([paragraph(0, [100, 100, 100, 100])], 250);
    expect(pages).toHaveLength(2);
    expect(pages[0].fragments[0]).toMatchObject({
      index: 0,
      mode: "lines",
      from: 0,
      to: 2,
      heightPx: 200,
      offsetPx: 0,
    });
    expect(pages[1].fragments[0]).toMatchObject({
      index: 0,
      mode: "lines",
      from: 2,
      to: 4,
      heightPx: 200,
      offsetPx: 200,
    });
  });

  it("keeps widows and orphans together", () => {
    // A 150px block leaves 100px on the page; the three-line paragraph would
    // split 1+2, leaving an orphan at the bottom, so it moves whole.
    const pages = paginate([paragraph(0, [150]), paragraph(1, [50, 50, 50])], 250);
    expect(pages).toHaveLength(2);
    expect(pages[0].fragments.map((fragment) => fragment.index)).toEqual([0]);
    expect(pages[1].fragments[0]).toMatchObject({ index: 1, from: 0, to: 3 });
  });

  it("still splits a paragraph that is taller than a whole page", () => {
    const pages = paginate([paragraph(0, [100, 100, 100])], 250);
    expect(pages).toHaveLength(2);
    expect(pages[0].fragments[0]).toMatchObject({ mode: "lines", from: 0, to: 2 });
    expect(pages[1].fragments[0]).toMatchObject({ mode: "lines", from: 2, to: 3, offsetPx: 200 });
  });

  it("honours pageBreakBefore", () => {
    const pages = paginate([paragraph(0, [100]), paragraph(1, [100], { pageBreakBefore: true })], 500);
    expect(pages).toHaveLength(2);
    expect(pages[0].fragments.map((fragment) => fragment.index)).toEqual([0]);
    expect(pages[1].fragments.map((fragment) => fragment.index)).toEqual([1]);
  });

  it("moves a keep-with-next pair together", () => {
    // P fills most of page 1; A wants to stay with B, and A+B do not fit.
    const pages = paginate([paragraph(0, [200]), paragraph(1, [60], { keepWithNext: true }), paragraph(2, [60])], 300);
    expect(pages).toHaveLength(2);
    expect(pages[0].fragments.map((fragment) => fragment.index)).toEqual([0]);
    expect(pages[1].fragments.map((fragment) => fragment.index)).toEqual([1, 2]);
  });

  it("moves a keep-together block to the next page", () => {
    const pages = paginate([paragraph(0, [200]), paragraph(1, [80], { keepTogether: true })], 300);
    expect(pages).toHaveLength(2);
    expect(pages[0].fragments.map((fragment) => fragment.index)).toEqual([0]);
    expect(pages[1].fragments.map((fragment) => fragment.index)).toEqual([1]);
  });

  it("breaks between blocks at a page break marker", () => {
    const pages = paginate([paragraph(0, [50]), breakBlock(1), paragraph(2, [50])], 500);
    expect(pages).toHaveLength(2);
    expect(pages[0].fragments.map((fragment) => fragment.index)).toEqual([0]);
    expect(pages[1].fragments.map((fragment) => fragment.index)).toEqual([2]);
  });

  it("splits a table at row boundaries and repeats its header", () => {
    // Header 30px, four 50px rows, 150px page.
    const pages = paginate([table(0, [50, 50, 50, 50], 30)], 150);
    expect(pages).toHaveLength(2);
    expect(pages[0].fragments[0]).toMatchObject({ mode: "rows", from: 0, to: 2, repeatHeader: false, heightPx: 130 });
    expect(pages[1].fragments[0]).toMatchObject({ mode: "rows", from: 2, to: 4, repeatHeader: true });
    // The continuation pays for the repeated header.
    expect(pages[1].fragments[0].heightPx).toBe(130);
  });

  it("moves an image that does not fit instead of splitting it", () => {
    const pages = paginate([paragraph(0, [200]), image(1, 200)], 300);
    expect(pages).toHaveLength(2);
    expect(pages[1].fragments[0]).toMatchObject({ index: 1, mode: "whole" });
  });

  it("reports the page a block lands on", () => {
    const pages = paginate([paragraph(0, [200]), paragraph(1, [200]), paragraph(2, [100])], 300);
    expect(pageOfBlock(pages, 0)).toBe(1);
    expect(pageOfBlock(pages, 1)).toBe(2);
    expect(pageOfBlock(pages, 2)).toBe(2);
  });

  it("numbers pages monotonically inside one section", () => {
    // Regression: `pushPage` read `page.sectionPage` after replacing the page,
    // so every page after the first was numbered 2 (odd/even headers wrong).
    const pages = paginate(
      [paragraph(0, [50]), breakBlock(1), paragraph(2, [50]), breakBlock(3), paragraph(4, [50])],
      500,
    );
    expect(pages.map((page) => page.sectionPage)).toEqual([1, 2, 3]);
  });
});
