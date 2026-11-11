import { createRef } from "react";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { FormulaBar } from "./FormulaBar";

function setup(overrides: Partial<Parameters<typeof FormulaBar>[0]> = {}) {
  const props = {
    docked: false,
    nameBox: "A1",
    onNameBox: vi.fn(),
    value: "=SUM(A1:A3)",
    inputRef: createRef<HTMLInputElement>(),
    onFocus: vi.fn(),
    onCaret: vi.fn(),
    onBlur: vi.fn(),
    onChange: vi.fn(),
    assistKey: vi.fn(() => false),
    onEnter: vi.fn(),
    onTab: vi.fn(),
    onEscape: vi.fn(),
    onCancel: vi.fn(),
    onApply: vi.fn(),
    ...overrides,
  };
  const view = render(<FormulaBar {...props} />);
  const [name, formula] = view.container.querySelectorAll("input");
  return { props, name, formula };
}

describe("Calc formula bar (3.3f)", () => {
  it("passes the trimmed name box and turns Enter, Tab and Escape into intents", () => {
    const { props, name, formula } = setup();
    fireEvent.change(name, { target: { value: "  B2:C3 " } });
    expect(props.onNameBox).toHaveBeenCalledWith("B2:C3");

    fireEvent.keyDown(formula, { key: "Enter", shiftKey: true });
    fireEvent.keyDown(formula, { key: "Tab" });
    fireEvent.keyDown(formula, { key: "Escape" });
    expect(props.onEnter).toHaveBeenCalledWith(true);
    expect(props.onTab).toHaveBeenCalledWith(false);
    expect(props.onEscape).toHaveBeenCalledTimes(1);
  });

  it("lets the formula assist consume a key first", () => {
    const { props, formula } = setup({ assistKey: vi.fn(() => true) });
    fireEvent.keyDown(formula, { key: "Enter" });
    expect(props.assistKey).toHaveBeenCalledTimes(1);
    expect(props.onEnter).not.toHaveBeenCalled();
  });

  it("reports the new text with its caret and shows phone buttons only when docked", () => {
    const { props, formula } = setup();
    fireEvent.change(formula, { target: { value: "=1+2" } });
    expect(props.onChange).toHaveBeenCalledWith("=1+2", expect.any(Number));
    expect(screen.queryByRole("button", { name: "Apply" })).toBeNull();
  });

  it("cancels and applies from the docked bar", () => {
    const { props } = setup({ docked: true });
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    expect(props.onCancel).toHaveBeenCalledTimes(1);
    expect(props.onApply).toHaveBeenCalledTimes(1);
  });
});
