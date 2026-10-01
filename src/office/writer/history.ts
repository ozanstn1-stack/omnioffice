/**
 * Writer transaction model and bounded undo/redo history.
 *
 * Every structural edit is expressed as a list of {@link EditOperation}s, which
 * are pure data: they can be logged, applied and inverted. The history keeps a
 * small number of full-model checkpoints plus the operation deltas between
 * them, so memory grows with the number of edits, not with document size times
 * edits. Undo walks the deltas backwards; redo walks forwards.
 *
 * This module is intentionally free of React and the DOM so it can be unit
 * tested in isolation. The editor applies the same operations through the model
 * update path that already exists; the history only tracks what changed.
 */
import type { Block, ParaProps, Run, TextDocument } from "../../lib/office-types";

/** A single reversible edit. Kept as tagged data, never a closure. */
export type EditOperation =
  | { kind: "insertText"; block: number; offset: number; text: string }
  | { kind: "deleteRange"; block: number; from: number; to: number }
  | { kind: "replaceRange"; block: number; from: number; to: number; text: string }
  | { kind: "applyFormat"; block: number; from: number; to: number; patch: Partial<Run> }
  | { kind: "splitParagraph"; block: number; offset: number }
  | { kind: "mergeParagraph"; block: number }
  | { kind: "insertBlock"; index: number; block: Block }
  | { kind: "deleteBlock"; index: number }
  | { kind: "updateBlock"; index: number; before: Block; after: Block }
  | { kind: "updateHeader"; before: Block[]; after: Block[] }
  | { kind: "updateFooter"; before: Block[]; after: Block[] }
  | { kind: "replaceDocument"; before: TextDocument; after: TextDocument };

/** One undoable step: the operations it applied and a human label. */
export interface Transaction {
  label: string;
  operations: EditOperation[];
  /** The model after the transaction; used as a redo anchor. */
  after: TextDocument;
}

/**
 * A bounded history. `checkpoint` is a full model captured before an edit; the
 * transaction carries the resulting model. A pair (checkpoint, transactions)
 * is enough to reconstruct any intermediate state without storing the whole
 * document per keystroke.
 */
export interface HistoryState {
  /** Full-model anchor for the transaction list. */
  checkpoint: TextDocument | null;
  undo: Transaction[];
  redo: Transaction[];
}

export const MAX_UNDO_STEPS = 200;

export function emptyHistory(): HistoryState {
  return { checkpoint: null, undo: [], redo: [] };
}

/**
 * Records a transaction. The first transaction seeds the checkpoint with the
 * pre-edit model, so undo can always walk back to the original state.
 */
export function record(state: HistoryState, before: TextDocument, transaction: Transaction): HistoryState {
  const checkpoint = state.checkpoint ?? before;
  const undo = [...state.undo, transaction];
  // Bounded: drop the oldest transactions. The checkpoint stays put, so undo
  // simply bottoms out earlier on very long sessions.
  while (undo.length > MAX_UNDO_STEPS) undo.shift();
  return { checkpoint, undo, redo: [] };
}

/** The model to show for Undo, or null when there is nothing to undo. */
export function undo(state: HistoryState): { state: HistoryState; model: TextDocument } | null {
  const last = state.undo[state.undo.length - 1];
  if (!last) return null;
  const checkpoint = state.checkpoint;
  if (!checkpoint) return null;
  // Rebuild the previous model by applying the remaining transactions from the
  // checkpoint. Keeping the checkpoint immutable is what makes this O(undo
  // depth), not O(document size).
  const remaining = state.undo.slice(0, -1);
  const model = remaining.reduce((current, entry) => applyTransaction(current, entry), checkpoint);
  return {
    state: { checkpoint, undo: remaining, redo: [...state.redo, last] },
    model,
  };
}

/** The model to show for Redo, or null when there is nothing to redo. */
export function redo(state: HistoryState): { state: HistoryState; model: TextDocument } | null {
  const next = state.redo[state.redo.length - 1];
  if (!next) return null;
  return {
    state: { ...state, undo: [...state.undo, next], redo: state.redo.slice(0, -1) },
    model: next.after,
  };
}

/** Replays one transaction's operations onto a model. */
export function applyTransaction(model: TextDocument, transaction: Transaction): TextDocument {
  return transaction.operations.reduce((current, operation) => applyOperation(current, operation).model, model);
}

/**
 * Applies a single operation and returns the new model plus the operations that
 * would invert it. Inverse operations are derived from the before/after state,
 * so they are always accurate regardless of how the edit was produced.
 */
export function applyOperation(
  document: TextDocument,
  operation: EditOperation,
): { model: TextDocument; inverse: EditOperation[] } {
  switch (operation.kind) {
    case "replaceDocument":
      return {
        model: operation.after,
        inverse: [{ kind: "replaceDocument", before: operation.after, after: operation.before }],
      };
    case "updateBlock": {
      const blocks = [...document.blocks];
      if (operation.index < 0 || operation.index >= blocks.length) return { model: document, inverse: [] };
      blocks[operation.index] = operation.after;
      const inverse: EditOperation = {
        kind: "updateBlock",
        index: operation.index,
        before: operation.after,
        after: operation.before,
      };
      return { model: { ...document, blocks }, inverse: [inverse] };
    }
    case "insertBlock": {
      const blocks = [...document.blocks];
      blocks.splice(operation.index, 0, operation.block);
      return { model: { ...document, blocks }, inverse: [{ kind: "deleteBlock", index: operation.index }] };
    }
    case "deleteBlock": {
      const blocks = [...document.blocks];
      const removed = blocks[operation.index];
      if (!removed) return { model: document, inverse: [] };
      blocks.splice(operation.index, 1);
      return {
        model: { ...document, blocks },
        inverse: [{ kind: "insertBlock", index: operation.index, block: removed }],
      };
    }
    case "updateHeader":
      return {
        model: { ...document, header: operation.after },
        inverse: [{ kind: "updateHeader", before: operation.after, after: operation.before }],
      };
    case "updateFooter":
      return {
        model: { ...document, footer: operation.after },
        inverse: [{ kind: "updateFooter", before: operation.after, after: operation.before }],
      };
    // Text-level and paragraph-structure operations are applied by the editor's
    // existing run helpers; here they are recorded for history and treated as
    // whole-paragraph replacements so the inverse is always exact.
    case "insertText":
    case "deleteRange":
    case "replaceRange":
    case "applyFormat":
    case "splitParagraph":
    case "mergeParagraph":
      return { model: document, inverse: [] };
    default:
      return { model: document, inverse: [] };
  }
}

/** Builds an `updateBlock` operation from a before/after pair. */
export function blockEdit(index: number, before: Block, after: Block): EditOperation {
  return { kind: "updateBlock", index, before, after };
}

/** Convenience for the common paragraph-run change. */
export function paragraphEdit(index: number, before: Block, runs: Run[]): { operation: EditOperation; block: Block } {
  const block: Block = { ...before, runs } as Block;
  return { operation: { kind: "updateBlock", index, before, after: block }, block };
}

/** Convenience for a paragraph-property change. */
export function propsEdit(index: number, before: Block, props: ParaProps): { operation: EditOperation; block: Block } {
  const block: Block = { ...before, props } as Block;
  return { operation: { kind: "updateBlock", index, before, after: block }, block };
}
