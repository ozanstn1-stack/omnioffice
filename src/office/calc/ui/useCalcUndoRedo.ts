import type { Workbook } from "../../../lib/office-types";
import { useSnapshotHistory } from "../../useSnapshotHistory";

/** Workbooks kept behind the current one before the oldest is dropped (41 states in all). */
export const CALC_UNDO_LIMIT = 40;

/**
 * Whole-workbook undo/redo for the Calc editor (#415, slice 1), on the
 * shared snapshot history (7.34): `record` snapshots the workbook before an
 * edit; `undo`/`redo` hand the swapped workbook to `apply` (the tab's `edit`).
 */
export function useCalcUndoRedo(workbook: Workbook, apply: (next: Workbook) => void) {
  return useSnapshotHistory(workbook, apply, CALC_UNDO_LIMIT);
}
