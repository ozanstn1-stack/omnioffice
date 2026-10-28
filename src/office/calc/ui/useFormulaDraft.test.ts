import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { newWorkbook } from "../../../lib/office-types";
import { functionCatalogue } from "../formula";
import { useFormulaDraft, type FormulaDraftOptions } from "./useFormulaDraft";

const workbook = newWorkbook("Book");
const catalogue = new Map(functionCatalogue().map((entry) => [entry.name, entry] as const));
const t = (key: string) => key;

function setup() {
  const input = document.createElement("input");
  document.body.append(input);
  const options: FormulaDraftOptions = {
    editing: null,
    setEditing: vi.fn(),
    editingRef: { current: null },
    cellInputRef: { current: null },
    formulaInputRef: { current: input },
    pendingCaretRef: { current: null },
    workbook,
    sheet: workbook.sheets[0],
    catalogue,
    t,
  };
  const hook = renderHook(() => useFormulaDraft(options));
  /** Types into the formula bar: the draft, the caret and the focus mode. */
  const type = (text: string) =>
    act(() => {
      hook.result.current.setFocusMode("bar");
      hook.result.current.setFormulaDraft(text);
      hook.result.current.setDraftCaret(text.length);
    });
  const key = (name: string) => {
    const event = { key: name, preventDefault: vi.fn() } as unknown as React.KeyboardEvent<HTMLInputElement>;
    let handled = false;
    act(() => {
      handled = hook.result.current.handleAssistKey(event);
    });
    return { handled, event };
  };
  return { hook, type, key, options };
}

describe("calc formula draft and assist", () => {
  it("offers functions for the token under the caret, only while an input has the focus", () => {
    const { hook, type } = setup();
    expect(hook.result.current.suggestions).toBeNull();
    type("=SU");
    const labels = hook.result.current.suggestions?.items.map((item) => item.label) ?? [];
    expect(labels).toContain("SUM");
    act(() => hook.result.current.setFocusMode(null));
    expect(hook.result.current.suggestions).toBeNull();
  });

  it("cycles the highlighted suggestion with the arrow keys", () => {
    const { hook, type, key } = setup();
    type("=SU");
    const count = hook.result.current.suggestions!.items.length;
    expect(count).toBeGreaterThan(1);
    expect(key("ArrowDown").handled).toBe(true);
    expect(hook.result.current.suggestIndex).toBe(1);
    key("ArrowUp");
    key("ArrowUp");
    expect(hook.result.current.suggestIndex).toBe(count - 1);
  });

  it("inserts the picked function on Enter and asks for the caret inside the brackets", () => {
    const { hook, type, key, options } = setup();
    type("=SU");
    const first = hook.result.current.suggestions!.items[0];
    expect(key("Enter").handled).toBe(true);
    expect(hook.result.current.formulaDraft).toBe(`=${first.insert}`);
    expect(options.pendingCaretRef.current).toBe(1 + (first.caret ?? first.insert.length));
  });

  it("hides the suggestions on Escape until the draft is typed into again", () => {
    const { hook, type, key } = setup();
    type("=SU");
    expect(key("Escape").handled).toBe(true);
    expect(hook.result.current.suggestions).toBeNull();
    expect(key("ArrowDown").handled).toBe(false);
  });

  it("shows the argument hint inside a function call", () => {
    const { hook, type } = setup();
    type("=SUM(");
    expect(hook.result.current.argumentHint).not.toBeNull();
  });
});
