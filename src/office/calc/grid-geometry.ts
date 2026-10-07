/**
 * Pure geometry of the Calc grid's scroll container.
 *
 * Cell coordinates are canvas pixels measured from the top-left cell, in front
 * of which sit the column and row headers; scroll offsets and the viewport size
 * are outer pixels, so the grid zoom maps the one onto the other.
 */

export interface RevealInput {
  /** The cell to bring into view, in unscaled canvas pixels from cell A1. */
  cell: { top: number; left: number; height: number; width: number };
  scrollTop: number;
  scrollLeft: number;
  /** Visible size of the scroll container, without scrollbars. */
  clientHeight: number;
  clientWidth: number;
  zoom: number;
  /** Height of the column header band and width of the row header column. */
  headerHeight: number;
  headerWidth: number;
  /**
   * Size of the frozen band under the column headers and beside the row
   * headers. Scrolled cells slide beneath it, so it counts as covered space.
   */
  frozenHeight?: number;
  frozenWidth?: number;
  /** The cell is itself inside the frozen rows / columns: it never scrolls away. */
  rowFrozen?: boolean;
  colFrozen?: boolean;
}

/** One axis: the scroll offset that shows `[start, start + size)` of the content. */
function revealAxis(
  scroll: number,
  client: number,
  zoom: number,
  header: number,
  covered: number,
  start: number,
  size: number,
): number {
  // Aligned with the edge of the covered area, the cell's top is
  // `start - covered` canvas pixels into the scrolled content.
  const topAligned = (start - covered) * zoom;
  if (topAligned < scroll) return Math.max(0, topAligned);
  const end = (header + start + size) * zoom;
  if (end > scroll + client) {
    // A cell taller than the viewport shows its start, not its end.
    return Math.max(0, Math.min(end - client, topAligned));
  }
  return scroll;
}

/**
 * The scroll offsets that bring a cell fully into view, or the current ones
 * when it already is. Frozen cells need no scrolling on their frozen axis.
 */
export function revealScroll(input: RevealInput): { scrollTop: number; scrollLeft: number } {
  const { cell, zoom } = input;
  const scrollTop = input.rowFrozen
    ? input.scrollTop
    : revealAxis(
        input.scrollTop,
        input.clientHeight,
        zoom,
        input.headerHeight,
        input.frozenHeight ?? 0,
        cell.top,
        cell.height,
      );
  const scrollLeft = input.colFrozen
    ? input.scrollLeft
    : revealAxis(
        input.scrollLeft,
        input.clientWidth,
        zoom,
        input.headerWidth,
        input.frozenWidth ?? 0,
        cell.left,
        cell.width,
      );
  return { scrollTop, scrollLeft };
}
