import { useCallback, useState } from "react";

/** Snapshots kept behind the current one before the oldest is dropped. */
export const SNAPSHOT_HISTORY_LIMIT = 40;

/**
 * Whole-model undo/redo shared by Calc and Impress (Phase E3.2). `record`
 * snapshots the current model before an edit and clears the redo history;
 * `undo`/`redo` swap the current model with the top of the other stack and
 * hand the result to `apply` (the tab's `edit`).
 *
 * The state updaters stay pure: the stacks are read before they are updated
 * and `apply` runs in the event handler, never inside an updater. React may
 * call an updater twice (always under StrictMode in development), and Calc's
 * former hook applied the edit and pushed the redo entry from inside one, so
 * one undo could land twice. Writer keeps its own operation-based history
 * (`office/writer/history.ts`), which stores deltas instead of snapshots.
 */
export function useSnapshotHistory<T>(current: T, apply: (next: T) => void, limit = SNAPSHOT_HISTORY_LIMIT) {
  const [undoStack, setUndoStack] = useState<T[]>([]);
  const [redoStack, setRedoStack] = useState<T[]>([]);

  const record = useCallback(() => {
    setUndoStack((stack) => [...stack.slice(-limit), current]);
    setRedoStack([]);
  }, [current, limit]);

  const undo = () => {
    const previous = undoStack[undoStack.length - 1];
    if (previous === undefined) return;
    setUndoStack((stack) => stack.slice(0, -1));
    setRedoStack((redos) => [...redos, current]);
    apply(previous);
  };

  const redo = () => {
    const next = redoStack[redoStack.length - 1];
    if (next === undefined) return;
    setRedoStack((stack) => stack.slice(0, -1));
    setUndoStack((undos) => [...undos, current]);
    apply(next);
  };

  return { record, undo, redo, canUndo: undoStack.length > 0, canRedo: redoStack.length > 0 };
}
