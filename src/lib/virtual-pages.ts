export interface VirtualPageRange {
  startIndex: number;
  endIndex: number;
  topSpacerPx: number;
  bottomSpacerPx: number;
}

/**
 * Computes the visible `[startIndex, endIndex]` slice (inclusive) and spacer
 * heights for a vertical stack of pages with `gapPx` between items.
 * For short documents (`<= overscanPages * 2 + 1` pages), all pages stay mounted.
 */
export function computeVirtualPageRange(
  pageHeightsPx: readonly number[],
  gapPx: number,
  scrollTop: number,
  viewportHeight: number,
  overscanPages = 3,
): VirtualPageRange {
  const count = pageHeightsPx.length;
  if (count === 0) {
    return { startIndex: 0, endIndex: -1, topSpacerPx: 0, bottomSpacerPx: 0 };
  }
  const safeViewport = Math.max(1, viewportHeight);
  const safeScroll = Math.max(0, scrollTop);
  const viewBottom = safeScroll + safeViewport;

  let y = 0;
  let firstVisible = count - 1;
  let lastVisible = 0;
  let foundFirst = false;

  for (let i = 0; i < count; i++) {
    const height = Math.max(1, pageHeightsPx[i] ?? 0);
    const bottom = y + height;
    if (!foundFirst && bottom >= safeScroll) {
      firstVisible = i;
      foundFirst = true;
    }
    if (y <= viewBottom) {
      lastVisible = i;
    }
    y = bottom + gapPx;
  }

  const startIndex = Math.max(0, firstVisible - overscanPages);
  const endIndex = Math.min(count - 1, Math.max(firstVisible, lastVisible) + overscanPages);

  let topSpacerPx = 0;
  for (let i = 0; i < startIndex; i++) {
    topSpacerPx += Math.max(1, pageHeightsPx[i] ?? 0) + gapPx;
  }

  let bottomSpacerPx = 0;
  for (let i = endIndex + 1; i < count; i++) {
    bottomSpacerPx += Math.max(1, pageHeightsPx[i] ?? 0) + gapPx;
  }

  return {
    startIndex,
    endIndex,
    topSpacerPx: Math.max(0, topSpacerPx - (startIndex > 0 ? gapPx : 0)),
    bottomSpacerPx: Math.max(0, bottomSpacerPx - (endIndex < count - 1 ? gapPx : 0)),
  };
}

/** Returns the top Y offset (in px, relative to the page stack) for page `index`. */
export function pageOffsetTop(pageHeightsPx: readonly number[], gapPx: number, index: number): number {
  const clamped = Math.max(0, Math.min(index, pageHeightsPx.length - 1));
  let y = 0;
  for (let i = 0; i < clamped; i++) {
    y += Math.max(1, pageHeightsPx[i] ?? 0) + gapPx;
  }
  return y;
}
