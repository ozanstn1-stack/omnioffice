import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { newWorkbook, type Workbook } from "../../../lib/office-types";
import { CALC_UNDO_LIMIT, useCalcUndoRedo } from "./useCalcUndoRedo";

/** A tiny tab: the hook sees the current workbook and writes through `apply`. */
function setup() {
  const books: Workbook[] = Array.from({ length: 60 }, (_, index) => newWorkbook(`v${index}`));
  let current = books[0];
  const hook = renderHook(
    ({ workbook }) =>
      useCalcUndoRedo(workbook, (next) => {
        current = next;
      }),
    { initialProps: { workbook: current } },
  );
  const edit = (next: Workbook) => {
    act(() => hook.result.current.record());
    current = next;
    hook.rerender({ workbook: current });
  };
  const sync = () => hook.rerender({ workbook: current });
  return { books, hook, edit, sync, now: () => current };
}

describe("calc undo and redo", () => {
  it("starts empty and records the workbook before each edit", () => {
    const { books, hook, edit, now } = setup();
    expect(hook.result.current.canUndo).toBe(false);
    expect(hook.result.current.canRedo).toBe(false);
    edit(books[1]);
    expect(hook.result.current.canUndo).toBe(true);
    expect(now()).toBe(books[1]);
  });

  it("undoes to the previous workbook and redoes back", () => {
    const { books, hook, edit, sync, now } = setup();
    edit(books[1]);
    edit(books[2]);
    act(() => hook.result.current.undo());
    sync();
    expect(now()).toBe(books[1]);
    expect(hook.result.current.canRedo).toBe(true);
    act(() => hook.result.current.undo());
    sync();
    expect(now()).toBe(books[0]);
    expect(hook.result.current.canUndo).toBe(false);
    act(() => hook.result.current.redo());
    sync();
    expect(now()).toBe(books[1]);
    act(() => hook.result.current.redo());
    sync();
    expect(now()).toBe(books[2]);
    expect(hook.result.current.canRedo).toBe(false);
  });

  it("drops the redo history when a new edit is recorded", () => {
    const { books, hook, edit, sync } = setup();
    edit(books[1]);
    act(() => hook.result.current.undo());
    sync();
    expect(hook.result.current.canRedo).toBe(true);
    edit(books[3]);
    expect(hook.result.current.canRedo).toBe(false);
  });

  it("does nothing when there is nothing to undo or redo", () => {
    const { books, hook, sync, now } = setup();
    act(() => hook.result.current.undo());
    act(() => hook.result.current.redo());
    sync();
    expect(now()).toBe(books[0]);
  });

  it("keeps the last 41 states, so 41 undos reach back that far and no further", () => {
    const { books, hook, edit, sync, now } = setup();
    for (let index = 1; index <= 50; index++) edit(books[index]);
    let undos = 0;
    while (hook.result.current.canUndo) {
      act(() => hook.result.current.undo());
      sync();
      undos += 1;
    }
    expect(undos).toBe(CALC_UNDO_LIMIT + 1);
    expect(now()).toBe(books[50 - (CALC_UNDO_LIMIT + 1)]);
  });
});
