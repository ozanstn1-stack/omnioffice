/** Pointer hit-testing and gesture state for the Calc grid. */
import type { CellPosition } from "./grid-types";

export function attributeAtEvent(event: PointerEvent | React.PointerEvent, attribute: string): HTMLElement | null {
  const closest = (element: Element | null | undefined) => element?.closest?.<HTMLElement>(`[${attribute}]`) ?? null;
  const fromTarget = closest(event.target as Element | null);
  if (fromTarget) return fromTarget;
  if (typeof document.elementFromPoint !== "function") return null;
  return closest(document.elementFromPoint(event.clientX, event.clientY));
}

/** The cell under a pointer event, or null. */
export function cellAtEvent(event: PointerEvent | React.PointerEvent): CellPosition | null {
  const element = attributeAtEvent(event, "data-cell");
  return element ? { row: Number(element.dataset.row), col: Number(element.dataset.col) } : null;
}

export interface DragBounds {
  start: CellPosition;
  end: CellPosition;
}

export type GridGesture =
  | { kind: "cells"; pointerId: number; anchor: CellPosition; startX: number; startY: number; moved: boolean }
  | { kind: "rows"; pointerId: number; anchorRow: number }
  | { kind: "cols"; pointerId: number; anchorCol: number }
  | { kind: "resize"; pointerId: number; col: number; startX: number; startWidth: number }
  | { kind: "fill"; pointerId: number; source: DragBounds; target: CellPosition }
  | { kind: "pan"; pointerId: number; startX: number; startY: number; scrollLeft: number; scrollTop: number }
  // A finger on a cell: it pans the sheet once it moves and selects the cell
  // when it lifts without moving (ranges come from the selection handles).
  | {
      kind: "tap";
      pointerId: number;
      cell: CellPosition;
      startX: number;
      startY: number;
      scrollLeft: number;
      scrollTop: number;
      moved: boolean;
    }
  // A touch selection handle: the opposite corner stays, the dragged one follows.
  | { kind: "extend"; pointerId: number; fixed: CellPosition };

/** Movement (px) after which a finger on a cell pans instead of tapping. */
export const TAP_SLOP = 8;

export interface PinchGesture {
  startDistance: number;
  startZoom: number;
  /** Midpoint of the previous move, so two-finger pans are incremental and
   * survive a zoom change in the middle of the gesture. */
  lastMidX: number;
  lastMidY: number;
  /** Canvas coordinates under the pinch midpoint at the last move. */
  contentX: number;
  contentY: number;
  /** Midpoint relative to the grid viewport at the last move. */
  localX: number;
  localY: number;
}
