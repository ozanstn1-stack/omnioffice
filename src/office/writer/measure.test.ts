import { afterEach, describe, expect, it, vi } from "vitest";
import type { Block } from "../../lib/office-types";
import type { BlockMetrics } from "./pagination";
import { measureBlocks } from "./measure";

function paragraph(text: string): Block {
  return {
    type: "paragraph",
    props: {
      style: "Normal",
      align: "left",
      lineSpacing: 1.15,
      spaceBeforePt: 0,
      spaceAfterPt: 8,
      indentLeftPt: 0,
      indentRightPt: 0,
      firstLinePt: 0,
      list: null,
      pageBreakBefore: false,
    },
    runs: [
      {
        text,
        bold: false,
        italic: false,
        underline: false,
        strike: false,
        color: null,
        highlight: null,
        font: null,
        sizePt: null,
        link: null,
        comment: null,
        superscript: false,
        subscript: false,
      },
    ],
  };
}

function probeFor(count: number): HTMLDivElement {
  const probe = document.createElement("div");
  for (let index = 0; index < count; index += 1) {
    const element = document.createElement("div");
    element.dataset.blockIndex = String(index);
    element.dataset.scope = "probe";
    probe.appendChild(element);
  }
  document.body.appendChild(probe);
  return probe;
}

afterEach(() => {
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

describe("measureBlocks dirty indices", () => {
  it("measures every block when no cache or dirty set is supplied", () => {
    const probe = probeFor(3);
    const blocks = [paragraph("a"), paragraph("b"), paragraph("c")];
    const query = vi.spyOn(probe, "querySelector");
    const metrics = measureBlocks(probe, blocks);
    expect(metrics.map((metric) => metric.index)).toEqual([0, 1, 2]);
    expect(query).toHaveBeenCalledTimes(3);
  });

  it("reuses cached metrics and only queries the dirty index", () => {
    const probe = probeFor(3);
    const blocks = [paragraph("a"), paragraph("b"), paragraph("c")];
    const cache = new Map<number, BlockMetrics>();
    const first = measureBlocks(probe, blocks, new Set([0, 1, 2]), cache);
    expect(cache.size).toBe(3);

    const query = vi.spyOn(probe, "querySelector");
    const second = measureBlocks(probe, blocks, new Set([1]), cache);
    expect(query).toHaveBeenCalledTimes(1);
    expect(query).toHaveBeenCalledWith(':scope > [data-block-index="1"][data-scope="probe"]');
    // Clean indices come back as the very same cached objects; the dirty one
    // was measured again.
    expect(second[0]).toBe(first[0]);
    expect(second[2]).toBe(first[2]);
    expect(second[1]).not.toBe(first[1]);
  });

  it("measures an index missing from the cache even when it is not dirty", () => {
    const probe = probeFor(2);
    const blocks = [paragraph("a"), paragraph("b")];
    const cache = new Map<number, BlockMetrics>();
    const metrics = measureBlocks(probe, blocks, new Set([1]), cache);
    expect(metrics.map((metric) => metric.index)).toEqual([0, 1]);
    expect(cache.has(0)).toBe(true);
    expect(cache.has(1)).toBe(true);
  });

  it("falls back to synthetic metrics for an unrendered block", () => {
    const probe = probeFor(1);
    const blocks = [paragraph("a"), paragraph("b")];
    const metrics = measureBlocks(probe, blocks);
    expect(metrics[1].heightPx).toBe(16);
    expect(metrics[1].kind).toBe("paragraph");
  });
});
