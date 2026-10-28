import { useCallback, useState } from "react";
import type { Workbook } from "../../../lib/office-types";

/** Workbooks kept behind the current one before the oldest is dropped (41 states in all). */
export const CALC_UNDO_LIMIT = 40;

/**
 * Whole-workbook undo/redo for the Calc editor (#415, slice 1). `record`
 * snapshots the current workbook before an edit and clears the redo history;
 * `undo`/`redo` swap the current workbook with the top of the other stack and
 * hand the result to `apply` (the tab's `edit`).
 */
export function useCalcUndoRedo(workbook: Workbook, apply: (next: Workbook) => void) {
  const [undoStack, setUndoStack] = useState<Workbook[]>([]);
  const [redoStack, setRedoStack] = useState<Workbook[]>([]);

  const record = useCallback(() => {
    setUndoStack((stack) => [...stack.slice(-CALC_UNDO_LIMIT), workbook]);
    setRedoStack([]);
  }, [workbook]);

  const undo = () => {
    setUndoStack((stack) => {
      const previous = stack[stack.length - 1];
      if (!previous) return stack;
      setRedoStack((redos) => [...redos, workbook]);
      apply(previous);
      return stack.slice(0, -1);
    });
  };

  const redo = () => {
    setRedoStack((stack) => {
      const next = stack[stack.length - 1];
      if (!next) return stack;
      setUndoStack((undos) => [...undos, workbook]);
      apply(next);
      return stack.slice(0, -1);
    });
  };

  return { record, undo, redo, canUndo: undoStack.length > 0, canRedo: redoStack.length > 0 };
}
