/**
 * DOM measurement for the pagination engine.
 *
 * The engine needs numbers, not markup: line-box bottoms for paragraphs, row
 * bottoms for tables, total heights for everything else. Measurement runs
 * against a hidden probe column rendered at the exact content width, so the
 * numbers match what the pages will show.
 */
import type { Block } from "../../lib/office-types";
import type { BlockMetrics } from "./pagination";

/** Cumulative bottom offsets of an element's line boxes, in px from its top. */
export function lineBottoms(element: HTMLElement): number[] {
  const top = element.getBoundingClientRect().top;
  const height = element.getBoundingClientRect().height;
  const range = document.createRange();
  range.selectNodeContents(element);
  // jsdom has no layout engine and no Range#getClientRects; fall back to the
  // element height so the editor still functions in tests.
  if (typeof range.getClientRects !== "function") return [Math.max(16, height)];
  const rects = Array.from(range.getClientRects()).filter((rect) => rect.height > 0);
  if (rects.length === 0) return [Math.max(16, height)];
  // Several inline boxes can share one line; keep the lowest bottom per line.
  const lines = new Map<number, number>();
  for (const rect of rects) {
    const key = Math.round(rect.top - top);
    lines.set(key, Math.max(lines.get(key) ?? 0, rect.bottom - top));
  }
  const bottoms = [...lines.entries()].sort((left, right) => left[0] - right[0]).map(([, bottom]) => bottom);
  return bottoms.length > 0 ? bottoms : [Math.max(16, height)];
}

/** Row bottoms (from the table top) and the header row height of a table. */
export function tableRows(element: HTMLElement, headerRow: boolean): { rows: number[]; headerHeightPx: number } {
  const tableTop = element.getBoundingClientRect().top;
  const rows = Array.from(element.querySelectorAll<HTMLTableRowElement>("tbody tr"));
  const bottoms = rows.map((row) => row.getBoundingClientRect().bottom - tableTop);
  const headerHeightPx = headerRow && rows[0] ? rows[0].getBoundingClientRect().height : 0;
  return { rows: bottoms, headerHeightPx };
}

/** Vertical margins of an element; jsdom and hidden probes may return none. */
function verticalMargins(element: HTMLElement): { top: number; bottom: number } {
  const style = window.getComputedStyle(element);
  const top = Number.parseFloat(style.marginTop);
  const bottom = Number.parseFloat(style.marginBottom);
  return { top: Number.isFinite(top) ? top : 0, bottom: Number.isFinite(bottom) ? bottom : 0 };
}

function measureBlock(element: HTMLElement, block: Block, index: number): BlockMetrics {
  const margins = block.type === "paragraph" ? verticalMargins(element) : { top: 0, bottom: 0 };
  const heightPx = element.getBoundingClientRect().height + margins.top + margins.bottom;
  const base: BlockMetrics = {
    index,
    kind: "paragraph",
    heightPx,
    lines: [],
    rows: [],
    headerHeightPx: 0,
    keepWithNext: false,
    keepTogether: false,
    pageBreakBefore: false,
  };
  switch (block.type) {
    case "paragraph":
      return {
        ...base,
        kind: "paragraph",
        keepWithNext: Boolean(block.props.keepWithNext),
        keepTogether: Boolean(block.props.keepTogether),
        pageBreakBefore: Boolean(block.props.pageBreakBefore),
        // Space-before belongs above the first line box; space-after is inside
        // the measured height already for the paragraph element.
        lines: lineBottoms(element).map((bottom) => bottom + margins.top),
      };
    case "table": {
      const { rows, headerHeightPx } = tableRows(element, Boolean(block.table.rows[0]?.header));
      return { ...base, kind: "table", rows, headerHeightPx };
    }
    case "image":
      return { ...base, kind: "image" };
    case "pageBreak":
      return { ...base, kind: "pageBreak", heightPx: 0 };
    case "toc":
      return { ...base, kind: "toc" };
    default:
      return { ...base, kind: "rule" };
  }
}

/** Synthetic metrics for a block the probe did not render (should not happen). */
function fallbackMetric(block: Block, index: number): BlockMetrics {
  return {
    index,
    kind:
      block.type === "pageBreak"
        ? "pageBreak"
        : block.type === "table"
          ? "table"
          : block.type === "image"
            ? "image"
            : block.type === "toc"
              ? "toc"
              : "paragraph",
    heightPx: block.type === "pageBreak" ? 0 : 16,
    lines: [16],
    rows: [],
    headerHeightPx: 0,
    keepWithNext: false,
    keepTogether: false,
    pageBreakBefore: block.type === "paragraph" ? Boolean(block.props.pageBreakBefore) : false,
  };
}

/**
 * Measures a probe element that rendered all body blocks in order.
 *
 * When `dirty` and `cache` are supplied, only the dirty block indices touch the
 * DOM; clean indices reuse the metric stored by an earlier pass, which keeps
 * typing in a long document from re-measuring (and re-querying) every block.
 * Blocks the probe did not render fall back to a one-line paragraph.
 */
export function measureBlocks(
  probe: HTMLElement,
  blocks: Block[],
  dirty?: ReadonlySet<number>,
  cache?: Map<number, BlockMetrics>,
): BlockMetrics[] {
  // Nested cell paragraphs also carry data-block-index, so only the outermost
  // probe scope counts; the lookup below is built lazily on the first miss.
  let byIndex: Map<number, HTMLElement> | null = null;
  const elementFor = (index: number): HTMLElement | null => {
    const direct = probe.querySelector<HTMLElement>(`:scope > [data-block-index="${index}"][data-scope="probe"]`);
    if (direct) return direct;
    if (!byIndex) {
      byIndex = new Map<number, HTMLElement>();
      for (const element of Array.from(probe.querySelectorAll<HTMLElement>("[data-block-index]"))) {
        const elementIndex = Number(element.dataset.blockIndex);
        if (element.dataset.scope === undefined || element.dataset.scope === "probe")
          byIndex.set(elementIndex, element);
      }
    }
    return byIndex.get(index) ?? null;
  };
  return blocks.map((block, index) => {
    const cached = cache?.get(index);
    if (dirty && cached && !dirty.has(index)) return cached;
    const element = elementFor(index);
    const metric = element ? measureBlock(element, block, index) : fallbackMetric(block, index);
    cache?.set(index, metric);
    return metric;
  });
}
