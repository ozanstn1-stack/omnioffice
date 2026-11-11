import type { MouseEvent } from "react";
import { columnLabel } from "../formula";
import { pinnedPosition } from "../freeze";
import { DEFAULT_COL_WIDTH, HEADER_WIDTH } from "../grid-metrics";
import type { RowLayout } from "../row-layout";
import { isHiddenIndex } from "../visibility";

/**
 * Calc's column letters along the top of the canvas (3.3g). Frozen columns
 * stay pinned while the rest scroll; `data-col-header` and `data-col-resize`
 * are what the grid gestures (calc/ui/useGridGestures.ts) hit-test.
 */
export function ColumnHeaders({
  columns,
  activeCol,
  colWidths,
  frozenCols,
  scrollX,
  offsetTop,
  onMenu,
}: {
  columns: ReadonlyArray<{ col: number; x: number }>;
  activeCol: number;
  colWidths: Record<string, number>;
  frozenCols: number;
  scrollX: number;
  /** The vertical scroll in canvas units, so the strip stays on top. */
  offsetTop: number;
  onMenu: (col: number, event: MouseEvent) => void;
}) {
  return (
    <div
      className="calc-col-headers"
      aria-hidden="true"
      style={{ transform: `translate(${HEADER_WIDTH}px, ${offsetTop}px)` }}
    >
      {columns.map(({ col, x }) => (
        <div
          key={col}
          data-col-header={col}
          className={`calc-col-header${activeCol === col ? " is-active" : ""}${isHiddenIndex(colWidths, col - 1) ? " has-hidden-before" : ""}`}
          style={{
            left: pinnedPosition(col, frozenCols, x, scrollX),
            width: colWidths[String(col)] ?? DEFAULT_COL_WIDTH,
            zIndex: col < frozenCols ? 1 : undefined,
          }}
          onContextMenu={(event) => onMenu(col, event)}
        >
          {columnLabel(col)}
          <span className="col-resize" data-col-resize={col} />
        </div>
      ))}
    </div>
  );
}

/** Calc's row numbers down the left of the canvas; frozen rows stay pinned. */
export function RowHeaders({
  rows,
  activeRow,
  layout,
  frozenRows,
  scrollY,
  offsetLeft,
  onMenu,
}: {
  rows: readonly number[];
  activeRow: number;
  layout: Pick<RowLayout, "offsetOf" | "heightOf" | "isHidden">;
  frozenRows: number;
  scrollY: number;
  /** The horizontal scroll in canvas units, so the strip stays on the left. */
  offsetLeft: number;
  onMenu: (row: number, event: MouseEvent) => void;
}) {
  return (
    <div className="calc-row-headers" aria-hidden="true" style={{ transform: `translate(${offsetLeft}px, 0)` }}>
      {rows.map((row) => (
        <div
          key={row}
          data-row-header={row}
          className={`calc-row-header${activeRow === row ? " is-active" : ""}${layout.isHidden(row - 1) ? " has-hidden-before" : ""}`}
          style={{
            top: pinnedPosition(row, frozenRows, layout.offsetOf(row), scrollY),
            height: layout.heightOf(row),
            zIndex: row < frozenRows ? 1 : undefined,
          }}
          onContextMenu={(event) => onMenu(row, event)}
        >
          {row + 1}
        </div>
      ))}
    </div>
  );
}
