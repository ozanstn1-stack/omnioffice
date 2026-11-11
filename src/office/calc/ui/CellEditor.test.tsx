import { createRef } from "react";
import { createEvent, fireEvent, render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { CellEditor } from "./CellEditor";

function setup(assist = false) {
  const props = {
    value: "12",
    inputRef: createRef<HTMLInputElement>(),
    onFocus: vi.fn(),
    onCaret: vi.fn(),
    onChange: vi.fn(),
    onBlur: vi.fn(),
    assistKey: vi.fn(() => assist),
    onEnter: vi.fn(),
    onTab: vi.fn(),
    onEscape: vi.fn(),
  };
  const { container } = render(<CellEditor {...props} />);
  return { props, input: container.querySelector("input")! };
}

describe("Calc cell editor (3.3h)", () => {
  it("turns Enter and Shift+Tab into intents and reports changes with the caret", () => {
    const { props, input } = setup();
    expect(document.activeElement).toBe(input);
    fireEvent.keyDown(input, { key: "Enter" });
    fireEvent.keyDown(input, { key: "Tab", shiftKey: true });
    expect(props.onEnter).toHaveBeenCalledWith(false);
    expect(props.onTab).toHaveBeenCalledWith(true);
    fireEvent.change(input, { target: { value: "=A1" } });
    expect(props.onChange).toHaveBeenCalledWith("=A1", expect.any(Number));
  });

  it("keeps Escape from reaching the grid", () => {
    const { props, input } = setup();
    const escape = createEvent.keyDown(input, { key: "Escape" });
    const stop = vi.spyOn(escape, "stopPropagation");
    fireEvent(input, escape);
    expect(props.onEscape).toHaveBeenCalledTimes(1);
    expect(stop).toHaveBeenCalled();
  });

  it("lets the formula assist and IME composition go first", () => {
    const assisted = setup(true);
    fireEvent.keyDown(assisted.input, { key: "Enter" });
    expect(assisted.props.onEnter).not.toHaveBeenCalled();

    const composing = setup();
    fireEvent.keyDown(composing.input, { key: "Enter", isComposing: true });
    expect(composing.props.onEnter).not.toHaveBeenCalled();
    expect(composing.props.assistKey).not.toHaveBeenCalled();
  });
});
