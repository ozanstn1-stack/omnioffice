import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { newSheet } from "../../../lib/office-types";
import { useGridGestures, type GridGestureOptions } from "./useGridGestures";

// jsdom has no PointerEvent; a MouseEvent subclass carries pointerId/pointerType.
class TestPointerEvent extends MouseEvent {
  readonly pointerId: number;
  readonly pointerType: string;
  constructor(type: string, init: PointerEventInit = {}) {
    super(type, init);
    this.pointerId = init.pointerId ?? 0;
    this.pointerType = init.pointerType ?? "";
  }
}

function setup() {
  const grid = document.createElement("div");
  const cell = document.createElement("div");
  cell.dataset.cell = "2:3";
  cell.dataset.row = "2";
  cell.dataset.col = "3";
  const rowHeader = document.createElement("div");
  rowHeader.dataset.rowHeader = "4";
  rowHeader.dataset.row = "4";
  grid.append(cell, rowHeader);
  document.body.append(grid);
  const sheet = newSheet("Sheet1");
  const options: GridGestureOptions = {
    gridRef: { current: grid },
    gridZoom: 1,
    gridZoomRef: { current: 1 },
    setGridZoom: vi.fn(),
    sheet,
    selection: { anchor: { row: 0, col: 0 }, focus: { row: 0, col: 0 } },
    selectionBounds: { start: { row: 0, col: 0 }, end: { row: 0, col: 0 } },
    setSelection: vi.fn(),
    updateSheet: vi.fn(),
    fillFromHandle: vi.fn(),
    annotations: { handleCtrlClick: () => false },
    setEditing: vi.fn(),
  };
  const hook = renderHook(() => useGridGestures(options));
  const down = (target: HTMLElement, init: PointerEventInit) =>
    act(() =>
      hook.result.current.handleGridPointerDown({
        ...init,
        button: 0,
        shiftKey: false,
        target,
        currentTarget: grid,
      } as unknown as React.PointerEvent<HTMLDivElement>),
    );
  const fire = (type: string, init: PointerEventInit) =>
    act(() => void window.dispatchEvent(new TestPointerEvent(type, init)));
  return { options, cell, rowHeader, down, fire, sheet };
}

afterEach(() => {
  document.body.innerHTML = "";
});

describe("grid gestures hook", () => {
  it("selects the tapped cell when a finger lifts without moving", () => {
    const { options, cell, down, fire } = setup();
    down(cell, { pointerId: 1, pointerType: "touch", clientX: 10, clientY: 10 });
    fire("pointerup", { pointerId: 1, pointerType: "touch" });
    expect(options.setSelection).toHaveBeenLastCalledWith({ anchor: { row: 2, col: 3 }, focus: { row: 2, col: 3 } });
  });

  it("selects a whole row from its header", () => {
    const { options, rowHeader, down, sheet } = setup();
    down(rowHeader, { pointerId: 2, pointerType: "mouse", clientX: 5, clientY: 5 });
    expect(options.setSelection).toHaveBeenCalledWith({
      anchor: { row: 4, col: 0 },
      focus: { row: 4, col: sheet.colCount - 1 },
    });
  });

  it("turns two fingers into a pinch that sets the zoom", () => {
    const { options, cell, down, fire } = setup();
    down(cell, { pointerId: 3, pointerType: "touch", clientX: 100, clientY: 50 });
    down(cell, { pointerId: 4, pointerType: "touch", clientX: 200, clientY: 50 });
    fire("pointermove", { pointerId: 4, pointerType: "touch", clientX: 300, clientY: 50 });
    expect(options.setGridZoom).toHaveBeenCalledWith(2);
    expect(options.setSelection).not.toHaveBeenCalled();
  });
});
