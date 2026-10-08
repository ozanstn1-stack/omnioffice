/**
 * Hide / unhide for the selected rows or columns, shared by the ribbon, the
 * header context menu and the Ctrl+9 / Ctrl+0 shortcuts. A protected sheet
 * refuses, and the selection moves off what was just hidden.
 */
import { useT } from "../../../lib/i18n";
import type { Sheet } from "../../../lib/office-types";
import { useToasts } from "../../../lib/store";
import type { CellPosition } from "../grid-types";
import { isSheetProtected } from "../protection";
import { hideRange, isHiddenIndex, revealableIndexes, stepVisible, unhideRange } from "../visibility";

export type Axis = "row" | "col";

export function useVisibilityActions(host: {
  sheet: Sheet;
  /** The selected cells, top-left and bottom-right. */
  bounds: { start: CellPosition; end: CellPosition };
  updateSheet: (mutate: (sheet: Sheet) => Sheet) => void;
  select: (position: CellPosition) => void;
}) {
  const t = useT();
  const { sheet, bounds, updateSheet, select } = host;

  const target = (axis: Axis) =>
    axis === "row"
      ? { from: bounds.start.row, to: bounds.end.row, total: sheet.rowCount, sizes: sheet.rowHeights }
      : { from: bounds.start.col, to: bounds.end.col, total: sheet.colCount, sizes: sheet.colWidths };

  /** True when hidden rows / columns touch the selection, so Unhide has something to do. */
  const canUnhide = (axis: Axis): boolean => {
    const { from, to, total, sizes } = target(axis);
    return revealableIndexes(sizes, from, to, total).length > 0;
  };

  const change = (axis: Axis, action: "hide" | "unhide") => {
    const push = useToasts.getState().push;
    if (isSheetProtected(sheet)) {
      push({ kind: "info", title: t("calc.sheetProtected") });
      return;
    }
    const { from, to, total, sizes } = target(axis);
    const next = action === "hide" ? hideRange(sizes, from, to, total) : unhideRange(sizes, from, to, total);
    if (!next) {
      push({ kind: "info", title: t(action === "hide" ? "calc.hideNothing" : "calc.unhideNothing") });
      return;
    }
    updateSheet((current) => (axis === "row" ? { ...current, rowHeights: next } : { ...current, colWidths: next }));
    if (action === "unhide") return;
    // Leave the hidden cells: the next visible row / column, or the one before when none follows.
    const after = stepVisible(next, to, 1, total);
    const landing = isHiddenIndex(next, after) ? stepVisible(next, from, -1, total) : after;
    select(axis === "row" ? { row: landing, col: bounds.start.col } : { row: bounds.start.row, col: landing });
  };

  return { change, canUnhide };
}
