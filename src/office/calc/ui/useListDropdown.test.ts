import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useListDropdown, type ListDropdownOptions } from "./useListDropdown";

const items = ["Ankara", "İstanbul", "İzmir", "Bursa"];

function setup(overrides: Partial<ListDropdownOptions> = {}) {
  const grid = document.createElement("div");
  grid.tabIndex = 0;
  document.body.append(grid);
  const onChoose = vi.fn();
  const initial: ListDropdownOptions = {
    id: "list",
    items,
    resetKey: "0:A1:",
    currentText: () => "",
    onChoose,
    gridRef: { current: grid },
    popupRef: { current: null },
    ...overrides,
  };
  const hook = renderHook((options: ListDropdownOptions) => useListDropdown(options), { initialProps: initial });
  const key = (name: string, altKey = false) => {
    const event = {
      key: name,
      altKey,
      preventDefault: vi.fn(),
      stopPropagation: vi.fn(),
    } as unknown as React.KeyboardEvent<HTMLDivElement>;
    act(() => hook.result.current.handleKey(event));
    return event;
  };
  return { hook, key, onChoose, grid, initial };
}

describe("calc list dropdown", () => {
  it("opens on the cell's current value, or on the first choice", () => {
    const { hook } = setup({ currentText: () => " BURSA " });
    act(() => hook.result.current.show());
    expect(hook.result.current.open).toBe(true);
    expect(hook.result.current.index).toBe(3);

    const other = setup({ currentText: () => "Antalya" });
    act(() => other.hook.result.current.show());
    expect(other.hook.result.current.index).toBe(0);
  });

  it("moves with the arrows, Home and End, and stays inside the list", () => {
    const { hook, key } = setup();
    act(() => hook.result.current.show());
    key("ArrowUp");
    expect(hook.result.current.index).toBe(0);
    key("ArrowDown");
    key("ArrowDown");
    expect(hook.result.current.index).toBe(2);
    key("End");
    key("ArrowDown");
    expect(hook.result.current.index).toBe(3);
    key("Home");
    expect(hook.result.current.index).toBe(0);
  });

  it("writes the highlighted choice on Enter and gives the keyboard back to the grid", () => {
    const { hook, key, onChoose, grid } = setup();
    act(() => hook.result.current.show());
    key("ArrowDown");
    const event = key("Enter");
    expect(onChoose).toHaveBeenCalledWith("İstanbul");
    expect(hook.result.current.open).toBe(false);
    expect(document.activeElement).toBe(grid);
    expect(event.preventDefault).toHaveBeenCalled();
  });

  it("closes on Escape, Tab and Alt+Up, and ignores other keys", () => {
    for (const [name, alt] of [
      ["Escape", false],
      ["Tab", false],
      ["ArrowUp", true],
    ] as const) {
      const { hook, key } = setup();
      act(() => hook.result.current.show());
      key(name, alt);
      expect(hook.result.current.open, name).toBe(false);
    }
    const { hook, key } = setup();
    act(() => hook.result.current.show());
    const event = key("a");
    expect(hook.result.current.open).toBe(true);
    expect(event.preventDefault).not.toHaveBeenCalled();
  });

  it("closes when the cell, the sheet or the editing state changes", () => {
    const { hook, initial } = setup();
    act(() => hook.result.current.show());
    hook.rerender({ ...initial, resetKey: "0:A2:" });
    expect(hook.result.current.open).toBe(false);
  });

  it("is never open without choices, and clamps the index to the list", () => {
    const { hook, initial } = setup();
    act(() => hook.result.current.show());
    act(() => hook.result.current.highlight(3));
    hook.rerender({ ...initial, items: items.slice(0, 2) });
    expect(hook.result.current.index).toBe(1);
    hook.rerender({ ...initial, items: [] });
    expect(hook.result.current.open).toBe(false);
  });
});
