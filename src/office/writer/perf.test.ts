/**
 * Writer pagination performance guard.
 *
 * `paginate()` is the pure layout engine the Writer pages and PDF export both
 * run on. These tests drive it with pre-measured `BlockMetrics` (no DOM is
 * touched) at a document size of roughly 100 printed pages and assert that the
 * pass stays comfortably interactive. The bound is deliberately generous so a
 * loaded CI machine does not fail a correct build; a real regression (say a
 * quadratic split loop) blows past it by an order of magnitude.
 *
 * The heavy 500-page export path lives on the Rust side
 * (`crates/officecore/tests/perf_test.rs`, `#[ignore]`), where the real
 * layout+pdfcanvas engine runs.
 */
import { describe, expect, it } from "vitest";
import { paginate, type BlockMetrics } from "./pagination";

/** A paragraph whose lines are `lineCount` boxes of `lineHeightPx`. */
function paragraph(index: number, lineCount: number, lineHeightPx = 16, keepWithNext = false): BlockMetrics {
  const lines: number[] = [];
  let total = 0;
  for (let line = 0; line < lineCount; line += 1) {
    total += lineHeightPx;
    lines.push(total);
  }
  return {
    index,
    kind: "paragraph",
    heightPx: total + 8,
    lines,
    rows: [],
    headerHeightPx: 0,
    keepWithNext,
    keepTogether: false,
    pageBreakBefore: false,
  };
}

/** A table with `rowCount` 18px rows under a 24px repeated header. */
function table(index: number, rowCount: number): BlockMetrics {
  const rows: number[] = [];
  let total = 24;
  for (let row = 0; row < rowCount; row += 1) {
    total += 18;
    rows.push(total);
  }
  return {
    index,
    kind: "table",
    heightPx: total,
    lines: [],
    rows,
    headerHeightPx: 24,
    keepWithNext: false,
    keepTogether: false,
    pageBreakBefore: false,
  };
}

/**
 * About 110 printed pages: per page a keep-with-next heading plus 16 two-line
 * paragraphs, and every 25th page a 60-row table that has to split and repeat
 * its header.
 */
function hundredPageBook(): BlockMetrics[] {
  const blocks: BlockMetrics[] = [];
  let index = 0;
  for (let page = 0; page < 110; page += 1) {
    blocks.push(paragraph(index, 1, 20, true));
    index += 1;
    if (page % 25 === 24) {
      blocks.push(table(index, 60));
      index += 1;
    }
    for (let line = 0; line < 16; line += 1) {
      blocks.push(paragraph(index, 2));
      index += 1;
    }
  }
  return blocks;
}

describe("Writer pagination performance", () => {
  it("paginates a 100-page-equivalent document under a generous bound", () => {
    const blocks = hundredPageBook();
    const started = Date.now();
    const pages = paginate(blocks, 720, { orphans: 2, widows: 2 });
    const elapsed = Date.now() - started;
    // The sanity checks make sure we measured a real layout, not an early exit.
    expect(pages.length).toBeGreaterThanOrEqual(100);
    expect(pages[0].fragments.length).toBeGreaterThan(0);
    expect(elapsed).toBeLessThan(2_000);
  });

  it("splits a 5 000-row table across pages under a bound", () => {
    const blocks = [table(0, 5_000)];
    const started = Date.now();
    const pages = paginate(blocks, 720);
    const elapsed = Date.now() - started;
    expect(pages.length).toBeGreaterThanOrEqual(50);
    // Continuation fragments must repeat the table header.
    expect(pages[1].fragments[0]?.repeatHeader).toBe(true);
    expect(elapsed).toBeLessThan(2_000);
  });
});
