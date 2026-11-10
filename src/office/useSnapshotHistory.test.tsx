import { StrictMode, type ReactNode } from "react";
import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { SNAPSHOT_HISTORY_LIMIT, useSnapshotHistory } from "./useSnapshotHistory";

const strict = ({ children }: { children: ReactNode }) => <StrictMode>{children}</StrictMode>;

/** A tiny tab whose model is a string; `apply` writes it back like `edit`. */
function setup(options: { strict?: boolean; limit?: number } = {}) {
  let current = "v0";
  const apply = vi.fn((next: string) => {
    current = next;
  });
  const hook = renderHook(({ model }) => useSnapshotHistory(model, apply, options.limit), {
    initialProps: { model: current },
    wrapper: options.strict ? strict : undefined,
  });
  const edit = (next: string) => {
    act(() => hook.result.current.record());
    current = next;
    hook.rerender({ model: current });
  };
  const sync = () => hook.rerender({ model: current });
  return { hook, apply, edit, sync, now: () => current };
}

describe("shared snapshot history (7.34)", () => {
  it("applies one undo and one redo exactly once under StrictMode", () => {
    const { hook, apply, edit, sync, now } = setup({ strict: true });
    edit("v1");
    act(() => hook.result.current.undo());
    sync();
    expect(apply).toHaveBeenCalledTimes(1);
    expect(now()).toBe("v0");

    act(() => hook.result.current.redo());
    sync();
    expect(apply).toHaveBeenCalledTimes(2);
    expect(now()).toBe("v1");
    // One redo entry was pushed, not two: a second redo has nothing left.
    expect(hook.result.current.canRedo).toBe(false);
  });

  it("walks back and forth and clears redo on a new edit", () => {
    const { hook, edit, sync, now } = setup();
    edit("v1");
    edit("v2");
    act(() => hook.result.current.undo());
    sync();
    act(() => hook.result.current.undo());
    sync();
    expect(now()).toBe("v0");
    expect(hook.result.current.canUndo).toBe(false);
    act(() => hook.result.current.redo());
    sync();
    expect(now()).toBe("v1");
    edit("v9");
    expect(hook.result.current.canRedo).toBe(false);
  });

  it("keeps at most the limit plus one snapshots and ignores an empty stack", () => {
    const { hook, apply, edit, sync } = setup({ limit: 3 });
    act(() => hook.result.current.undo());
    expect(apply).not.toHaveBeenCalled();
    for (let i = 1; i <= 8; i += 1) edit(`v${i}`);
    let undos = 0;
    while (hook.result.current.canUndo) {
      act(() => hook.result.current.undo());
      sync();
      undos += 1;
    }
    expect(undos).toBe(4);
    expect(SNAPSHOT_HISTORY_LIMIT).toBe(40);
  });
});
