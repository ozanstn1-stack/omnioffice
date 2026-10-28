import { useEffect, useLayoutEffect, useRef } from "react";
import type { Sheet } from "../../../lib/office-types";
import { cellEditText } from "../cells";
import { formatAddress } from "../formula";
import { pinchGridZoom } from "../grid-math";
import type { CellPosition, GridSelection } from "../grid-types";
import {
  attributeAtEvent,
  cellAtEvent,
  TAP_SLOP,
  type DragBounds,
  type GridGesture,
  type PinchGesture,
} from "../gestures";
import { DEFAULT_COL_WIDTH, HEADER_WIDTH, ROW_HEIGHT } from "../grid-metrics";

/** The editor state the grid gestures read and write. */
export interface GridGestureOptions {
  gridRef: React.RefObject<HTMLDivElement | null>;
  gridZoom: number;
  /** The zoom the canvas is laid out with; the editor keeps it in sync. */
  gridZoomRef: React.MutableRefObject<number>;
  setGridZoom: (zoom: number) => void;
  sheet: Sheet;
  selection: GridSelection;
  selectionBounds: DragBounds;
  setSelection: (selection: GridSelection) => void;
  updateSheet: (mutate: (sheet: Sheet) => Sheet, recordUndo?: boolean) => void;
  fillFromHandle: (source: DragBounds, target: CellPosition) => void;
  annotations: { handleCtrlClick: (event: React.PointerEvent, position: CellPosition) => boolean };
  setEditing: (next: { row: number; col: number; value: string } | null) => void;
}

/**
 * Pointer, touch and pinch handling for the Calc grid (#415, slice 2):
 * selection drags, header row/column selection, the column resize grip, the
 * fill and selection handles, one-finger pan, two-finger pinch zoom and pan,
 * and double-tap to edit. The code is the editor's, moved as it was; the
 * window-level move/up listeners keep a gesture alive when the pointer
 * leaves the grid.
 */
export function useGridGestures(options: GridGestureOptions) {
  const {
    gridRef,
    gridZoom,
    gridZoomRef,
    setGridZoom,
    sheet,
    selection,
    selectionBounds,
    setSelection,
    updateSheet,
    fillFromHandle,
    annotations,
    setEditing,
  } = options;

  const pointersRef = useRef(new Map<number, { x: number; y: number }>());
  const gestureRef = useRef<GridGesture | null>(null);
  const pinchRef = useRef<PinchGesture | null>(null);
  const pinchFocalRef = useRef<PinchGesture | null>(null);
  const lastTapRef = useRef<{ row: number; col: number; time: number } | null>(null);

  const pointerMoveRef = useRef<(event: PointerEvent) => void>(() => undefined);
  const pointerUpRef = useRef<(event: PointerEvent) => void>(() => undefined);

  /** Touch keeps receiving move events outside the element; the mouse does not
   * capture so double-click still targets the cell it happened on. */
  const captureGridPointer = (event: React.PointerEvent<HTMLDivElement>) => {
    if (event.pointerType === "mouse") return;
    try {
      event.currentTarget.setPointerCapture?.(event.pointerId);
    } catch {
      // Pointer capture is unavailable (older webviews, jsdom).
    }
  };

  const releaseGridPointer = (pointerId: number) => {
    try {
      gridRef.current?.releasePointerCapture?.(pointerId);
    } catch {
      // Not captured.
    }
  };

  /** Scrolls the grid when a selection drag reaches an edge. */
  const autoScrollGrid = (clientX: number, clientY: number) => {
    const grid = gridRef.current;
    if (!grid) return;
    const rect = grid.getBoundingClientRect();
    const margin = 32;
    const step = 24;
    if (clientX < rect.left + HEADER_WIDTH + margin) grid.scrollLeft = Math.max(0, grid.scrollLeft - step);
    else if (clientX > rect.right - margin) grid.scrollLeft += step;
    if (clientY < rect.top + ROW_HEIGHT + margin) grid.scrollTop = Math.max(0, grid.scrollTop - step);
    else if (clientY > rect.bottom - margin) grid.scrollTop += step;
  };

  /** Ends a gesture and applies whatever it previewed. */
  const endGridGesture = (pointerId: number) => {
    const gesture = gestureRef.current;
    if (!gesture || gesture.pointerId !== pointerId) return;
    gestureRef.current = null;
    if (gesture.kind === "fill") fillFromHandle(gesture.source, gesture.target);
  };

  const handleGridPointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    gridRef.current?.focus({ preventScroll: true });
    pointersRef.current.set(event.pointerId, { x: event.clientX, y: event.clientY });
    if (pointersRef.current.size >= 2) {
      // A second finger turns the gesture into pinch zoom plus two-finger pan;
      // that is also how the sheet scrolls, because touch-action: none on the
      // grid keeps the browser from scrolling it natively.
      gestureRef.current = null;
      const [a, b] = [...pointersRef.current.values()];
      const grid = gridRef.current;
      const rect = grid?.getBoundingClientRect();
      const zoom = gridZoomRef.current;
      const localX = (a.x + b.x) / 2 - (rect?.left ?? 0);
      const localY = (a.y + b.y) / 2 - (rect?.top ?? 0);
      pinchRef.current = {
        startDistance: Math.max(1, Math.hypot(a.x - b.x, a.y - b.y)),
        startZoom: zoom,
        lastMidX: (a.x + b.x) / 2,
        lastMidY: (a.y + b.y) / 2,
        contentX: ((grid?.scrollLeft ?? 0) + localX) / zoom,
        contentY: ((grid?.scrollTop ?? 0) + localY) / zoom,
        localX,
        localY,
      };
      pinchFocalRef.current = null;
      return;
    }
    if (event.button !== 0) return;
    const target = event.target as HTMLElement;
    // Text inputs inside cells and the suggestion popup keep their own
    // hit-testing: capturing the pointer for the grid would steal the caret.
    if (target.closest?.("input, textarea, select, [contenteditable=true]")) return;
    const resize = target.closest?.<HTMLElement>("[data-col-resize]");
    if (resize) {
      // Checked before the column header because the grip lives inside it.
      const col = Number(resize.dataset.colResize);
      gestureRef.current = {
        kind: "resize",
        pointerId: event.pointerId,
        col,
        startX: event.clientX,
        startWidth: sheet.colWidths[String(col)] ?? DEFAULT_COL_WIDTH,
      };
      captureGridPointer(event);
      return;
    }
    const handle = target.closest?.<HTMLElement>("[data-fill-handle]");
    if (handle) {
      const bounds = selectionBounds;
      gestureRef.current = {
        kind: "fill",
        pointerId: event.pointerId,
        source: { start: { ...bounds.start }, end: { ...bounds.end } },
        target: { ...bounds.end },
      };
      captureGridPointer(event);
      return;
    }
    const selectHandle = target.closest?.<HTMLElement>("[data-select-handle]");
    if (selectHandle) {
      const bounds = selectionBounds;
      gestureRef.current = {
        kind: "extend",
        pointerId: event.pointerId,
        fixed: selectHandle.dataset.selectHandle === "start" ? { ...bounds.end } : { ...bounds.start },
      };
      captureGridPointer(event);
      return;
    }
    const cell = target.closest?.<HTMLElement>("[data-cell]");
    if (cell && event.pointerType === "touch") {
      const grid = gridRef.current;
      gestureRef.current = {
        kind: "tap",
        pointerId: event.pointerId,
        cell: { row: Number(cell.dataset.row), col: Number(cell.dataset.col) },
        startX: event.clientX,
        startY: event.clientY,
        scrollLeft: grid?.scrollLeft ?? 0,
        scrollTop: grid?.scrollTop ?? 0,
        moved: false,
      };
      captureGridPointer(event);
      return;
    }
    if (cell) {
      const position = { row: Number(cell.dataset.row), col: Number(cell.dataset.col) };
      // Ctrl+click on a link follows it; the cell is selected but no drag starts.
      if (annotations.handleCtrlClick(event, position)) return;
      gestureRef.current = {
        kind: "cells",
        pointerId: event.pointerId,
        anchor: position,
        startX: event.clientX,
        startY: event.clientY,
        moved: false,
      };
      setSelection(
        event.shiftKey ? { anchor: selection.anchor, focus: position } : { anchor: position, focus: position },
      );
      captureGridPointer(event);
      return;
    }
    const rowHeader = target.closest?.<HTMLElement>("[data-row-header]");
    if (rowHeader) {
      const row = Number(rowHeader.dataset.row);
      gestureRef.current = { kind: "rows", pointerId: event.pointerId, anchorRow: row };
      setSelection({ anchor: { row, col: 0 }, focus: { row, col: sheet.colCount - 1 } });
      captureGridPointer(event);
      return;
    }
    const colHeader = target.closest?.<HTMLElement>("[data-col-header]");
    if (colHeader) {
      const col = Number(colHeader.dataset.col);
      gestureRef.current = { kind: "cols", pointerId: event.pointerId, anchorCol: col };
      setSelection({ anchor: { row: 0, col }, focus: { row: sheet.rowCount - 1, col } });
      captureGridPointer(event);
      return;
    }
    // Empty canvas: one-finger pan for touch, drag-pan for the mouse.
    const grid = gridRef.current;
    if (grid) {
      gestureRef.current = {
        kind: "pan",
        pointerId: event.pointerId,
        startX: event.clientX,
        startY: event.clientY,
        scrollLeft: grid.scrollLeft,
        scrollTop: grid.scrollTop,
      };
      captureGridPointer(event);
    }
  };

  const handleWindowPointerMove = (event: PointerEvent) => {
    const points = pointersRef.current;
    if (!points.has(event.pointerId)) return;
    points.set(event.pointerId, { x: event.clientX, y: event.clientY });

    const pinch = pinchRef.current;
    if (pinch && points.size >= 2) {
      const [a, b] = [...points.values()];
      const midX = (a.x + b.x) / 2;
      const midY = (a.y + b.y) / 2;
      const grid = gridRef.current;
      const rect = grid?.getBoundingClientRect();
      const localX = midX - (rect?.left ?? 0);
      const localY = midY - (rect?.top ?? 0);
      const current = gridZoomRef.current;
      const nextZoom = pinchGridZoom(pinch.startZoom, pinch.startDistance, Math.hypot(a.x - b.x, a.y - b.y));
      if (nextZoom !== current) {
        // The zoom commits asynchronously; the focal point is applied in the
        // layout effect once the canvas has its new size.
        pinch.localX = localX;
        pinch.localY = localY;
        pinch.contentX = ((grid?.scrollLeft ?? 0) + localX) / current;
        pinch.contentY = ((grid?.scrollTop ?? 0) + localY) / current;
        pinchFocalRef.current = { ...pinch };
        setGridZoom(nextZoom);
      } else if (grid) {
        // Two-finger pan without a zoom change. Zoom changes pan through the
        // focal point instead, so this branch and the effect never both apply.
        // Panning is relative to the previous midpoint so it composes with a
        // zoom change in the middle of the same gesture.
        grid.scrollLeft = Math.max(0, grid.scrollLeft - (midX - pinch.lastMidX));
        grid.scrollTop = Math.max(0, grid.scrollTop - (midY - pinch.lastMidY));
      }
      pinch.lastMidX = midX;
      pinch.lastMidY = midY;
      return;
    }

    const gesture = gestureRef.current;
    if (!gesture || gesture.pointerId !== event.pointerId) return;
    if (gesture.kind === "pan") {
      const grid = gridRef.current;
      if (!grid) return;
      grid.scrollLeft = gesture.scrollLeft - (event.clientX - gesture.startX);
      grid.scrollTop = gesture.scrollTop - (event.clientY - gesture.startY);
      return;
    }
    if (gesture.kind === "resize") {
      const width = Math.max(32, gesture.startWidth + (event.clientX - gesture.startX) / gridZoomRef.current);
      updateSheet((current) => ({ ...current, colWidths: { ...current.colWidths, [gesture.col]: width } }));
      return;
    }
    if (gesture.kind === "tap") {
      const dx = event.clientX - gesture.startX;
      const dy = event.clientY - gesture.startY;
      if (!gesture.moved && Math.abs(dx) + Math.abs(dy) > TAP_SLOP) gesture.moved = true;
      const grid = gridRef.current;
      if (gesture.moved && grid) {
        grid.scrollLeft = Math.max(0, gesture.scrollLeft - dx);
        grid.scrollTop = Math.max(0, gesture.scrollTop - dy);
      }
      return;
    }
    autoScrollGrid(event.clientX, event.clientY);
    if (gesture.kind === "extend") {
      const cell = cellAtEvent(event);
      if (!cell) return;
      setSelection({
        anchor: gesture.fixed,
        focus: {
          row: Math.min(Math.max(0, cell.row), sheet.rowCount - 1),
          col: Math.min(Math.max(0, cell.col), sheet.colCount - 1),
        },
      });
      return;
    }
    if (gesture.kind === "cells") {
      const cell = cellAtEvent(event);
      if (!cell) return;
      if (Math.abs(event.clientX - gesture.startX) + Math.abs(event.clientY - gesture.startY) > 4) gesture.moved = true;
      setSelection({
        anchor: gesture.anchor,
        focus: {
          row: Math.min(Math.max(0, cell.row), sheet.rowCount - 1),
          col: Math.min(Math.max(0, cell.col), sheet.colCount - 1),
        },
      });
      return;
    }
    if (gesture.kind === "fill") {
      const cell = cellAtEvent(event);
      if (!cell) return;
      const end = {
        row: Math.max(gesture.source.end.row, Math.min(sheet.rowCount - 1, cell.row)),
        col: Math.max(gesture.source.end.col, Math.min(sheet.colCount - 1, cell.col)),
      };
      gesture.target = end;
      setSelection({ anchor: gesture.source.start, focus: end });
      return;
    }
    if (gesture.kind === "rows") {
      const header = attributeAtEvent(event, "data-row-header");
      if (!header) return;
      const row = Math.min(Math.max(0, Number(header.dataset.row)), sheet.rowCount - 1);
      setSelection({ anchor: { row: gesture.anchorRow, col: 0 }, focus: { row, col: sheet.colCount - 1 } });
      return;
    }
    if (gesture.kind === "cols") {
      const header = attributeAtEvent(event, "data-col-header");
      if (!header) return;
      const col = Math.min(Math.max(0, Number(header.dataset.col)), sheet.colCount - 1);
      setSelection({ anchor: { row: 0, col: gesture.anchorCol }, focus: { row: sheet.rowCount - 1, col } });
    }
  };

  const handleWindowPointerUp = (event: PointerEvent) => {
    const points = pointersRef.current;
    if (!points.has(event.pointerId)) return;
    points.delete(event.pointerId);
    releaseGridPointer(event.pointerId);

    if (pinchRef.current) {
      if (points.size < 2) {
        pinchRef.current = null;
        pinchFocalRef.current = null;
        // A finger remains: let it keep panning.
        const [remaining] = points.entries();
        const grid = gridRef.current;
        if (remaining && grid && event.pointerType !== "mouse") {
          gestureRef.current = {
            kind: "pan",
            pointerId: remaining[0],
            startX: remaining[1].x,
            startY: remaining[1].y,
            scrollLeft: grid.scrollLeft,
            scrollTop: grid.scrollTop,
          };
        }
      }
      return;
    }

    const gesture = gestureRef.current;
    if (!gesture || gesture.pointerId !== event.pointerId) return;
    endGridGesture(event.pointerId);
    const tapped =
      gesture.kind === "tap" && !gesture.moved
        ? gesture.cell
        : gesture.kind === "cells" && !gesture.moved && event.pointerType !== "mouse"
          ? gesture.anchor
          : null;
    if (gesture.kind === "tap" && tapped) setSelection({ anchor: tapped, focus: tapped });
    // Double-tap on a cell opens the inline editor: touch never produces a
    // native dblclick once the grid claims the gesture.
    if (tapped) {
      const now = event.timeStamp;
      const previous = lastTapRef.current;
      if (previous && now - previous.time < 350 && previous.row === tapped.row && previous.col === tapped.col) {
        lastTapRef.current = null;
        const cell = sheet.cells[formatAddress(tapped.row, tapped.col)];
        setEditing({ row: tapped.row, col: tapped.col, value: cell?.formula ?? cellEditText(cell) });
      } else {
        lastTapRef.current = { row: tapped.row, col: tapped.col, time: now };
      }
    }
  };

  useEffect(() => {
    pointerMoveRef.current = handleWindowPointerMove;
    pointerUpRef.current = handleWindowPointerUp;
  });

  useEffect(() => {
    const move = (event: PointerEvent) => pointerMoveRef.current(event);
    const up = (event: PointerEvent) => pointerUpRef.current(event);
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", up);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", up);
    };
  }, []);

  // Keeps the sheet point that sat under the pinch midpoint under it after the
  // canvas has been laid out at the new zoom (CSS zoom resizes the scroll
  // extent, so adjusting the scroll before the commit would be clamped).
  useLayoutEffect(() => {
    const focal = pinchFocalRef.current;
    if (!focal || !pinchRef.current) return;
    const grid = gridRef.current;
    if (!grid) return;
    const zoom = gridZoomRef.current;
    grid.scrollLeft = focal.contentX * zoom - focal.localX;
    grid.scrollTop = focal.contentY * zoom - focal.localY;
  }, [gridZoom, gridRef, gridZoomRef]);

  return { handleGridPointerDown };
}
