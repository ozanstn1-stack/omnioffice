import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null), save: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-fs", () => ({ readFile: vi.fn(async () => new Uint8Array()) }));

import { useOfficeTabs } from "../lib/office-store";
import { cellText } from "../lib/office-types";
import { useToasts } from "../lib/store";
import { cellAt, Harness, nameBox, seedWorkbook, selectRange, workbookOf } from "./calc/ui/testing";

type User = ReturnType<typeof userEvent.setup>;

const text = (address: string) => cellText(workbookOf().sheets[0].cells[address]);

/** Selects a range through the name box and gives the grid the keyboard (columns past C are not drawn in jsdom). */
function focusRange(range: string) {
  selectRange(range);
  document.querySelector<HTMLElement>(".calc-grid")!.focus();
}

async function copy(user: User, range: string) {
  focusRange(range);
  await user.keyboard("{Control>}c{/Control}");
}

async function openPasteSpecial(user: User, address: string) {
  focusRange(address);
  await user.keyboard("{Control>}{Alt>}v{/Alt}{/Control}");
  return await screen.findByRole("dialog", { name: "Paste special" });
}

describe("Calc paste special", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
    useToasts.setState({ toasts: [] });
  });

  it("pastes the shown values, not the formulas", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "2", B1: "=A1*3" });
    render(<Harness id={id} />);
    await copy(user, "B1");
    const dialog = await openPasteSpecial(user, "D3");
    await user.click(within(dialog).getByRole("radio", { name: "Values" }));
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));

    expect(screen.queryByRole("dialog")).toBeNull();
    expect(workbookOf().sheets[0].cells.D3.formula).toBeNull();
    expect(text("D3")).toBe("6");
  });

  it("pastes formulas with their references moved by the distance", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "2", B1: "=A1*3", A3: "5" });
    render(<Harness id={id} />);
    await copy(user, "B1");
    const dialog = await openPasteSpecial(user, "B3");
    await user.click(within(dialog).getByRole("radio", { name: "Formulas" }));
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));

    expect(text("B3")).toBe("=A3*3");
    expect(cellAt(2, 1)).toHaveTextContent("15");
  });

  it("pastes only the formatting and leaves the target's content", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "src", D1: "keep" }, (sheet) => ({
      ...sheet,
      cells: {
        ...sheet.cells,
        A1: { ...sheet.cells.A1, style: { ...sheet.cells.A1.style, bold: true, fill: "#ffff00" } },
      },
    }));
    render(<Harness id={id} />);
    await copy(user, "A1");
    const dialog = await openPasteSpecial(user, "D1");
    await user.click(within(dialog).getByRole("radio", { name: "Formatting" }));
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));

    const cell = workbookOf().sheets[0].cells.D1;
    expect(cell.value).toEqual({ kind: "text", value: "keep" });
    expect(cell.style).toMatchObject({ bold: true, fill: "#ffff00" });
  });

  it("transposes a block and selects the pasted area, as one undo step", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "a", B1: "b", C1: "c" });
    render(<Harness id={id} />);
    await copy(user, "A1:C1");
    const dialog = await openPasteSpecial(user, "E3");
    await user.click(within(dialog).getByRole("checkbox", { name: "Transpose rows and columns" }));
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));

    expect([text("E3"), text("E4"), text("E5")]).toEqual(["a", "b", "c"]);
    expect(nameBox()).toBe("E5:E3");

    await user.click(screen.getByRole("button", { name: "Undo" }));
    expect(workbookOf().sheets[0].cells.E3).toBeUndefined();
    expect(workbookOf().sheets[0].cells.E5).toBeUndefined();
  });

  it("pastes text from another application as values and says so", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "mine" });
    render(<Harness id={id} />);
    await copy(user, "A1");
    await navigator.clipboard.writeText("1\t2\n3\t4");
    const dialog = await openPasteSpecial(user, "A3");
    expect(within(dialog).getByText(/text from another application/)).toBeInTheDocument();
    expect(within(dialog).getByRole("radio", { name: "Values" })).toBeChecked();
    expect(within(dialog).getByRole("radio", { name: /^All/ })).toBeDisabled();
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));
    expect([text("A3"), text("B3"), text("A4"), text("B4")]).toEqual(["1", "2", "3", "4"]);
  });

  it("says so when there is nothing to paste", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "x" });
    render(<Harness id={id} />);
    await user.click(cellAt(0, 0));
    await user.keyboard("{Control>}{Alt>}v{/Alt}{/Control}");
    await act(async () => undefined);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(useToasts.getState().toasts.map((toast) => toast.title)).toContain(
      "The clipboard is empty. Copy some cells first.",
    );
  });

  it("is also on the Home tab", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "x" });
    render(<Harness id={id} />);
    await copy(user, "A1");
    await user.click(screen.getByRole("button", { name: "Paste special" }));
    expect(await screen.findByRole("dialog", { name: "Paste special" })).toBeInTheDocument();
  });

  it("refuses on a protected sheet", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "x" }, (sheet) => ({ ...sheet, sheetProtection: "ABCD" }));
    render(<Harness id={id} />);
    await copy(user, "A1");
    await user.keyboard("{Control>}{Alt>}v{/Alt}{/Control}");
    await act(async () => undefined);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(useToasts.getState().toasts.map((toast) => toast.title)).toContain(
      "This sheet is protected, so this change is not allowed.",
    );
  });
});

describe("Calc AutoSum", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
    useToasts.setState({ toasts: [] });
  });

  it("sums the numbers above the active cell from the Home tab", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "1", A2: "2", A3: "3" });
    render(<Harness id={id} />);
    await user.click(cellAt(3, 0));
    await user.click(screen.getByRole("button", { name: "AutoSum" }));
    expect(text("A4")).toBe("=SUM(A1:A3)");
    expect(cellAt(3, 0)).toHaveTextContent("6");
  });

  it("works from the keyboard with Alt+= and is one undo step", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ B1: "4", B2: "5" });
    render(<Harness id={id} />);
    await user.click(cellAt(2, 1));
    await user.keyboard("{Alt>}={/Alt}");
    expect(text("B3")).toBe("=SUM(B1:B2)");
    await user.click(screen.getByRole("button", { name: "Undo" }));
    expect(workbookOf().sheets[0].cells.B3).toBeUndefined();
  });

  it("sums to the left when nothing numeric is above", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "1", B1: "2" });
    render(<Harness id={id} />);
    await user.click(cellAt(0, 2));
    await user.click(screen.getByRole("button", { name: "AutoSum" }));
    expect(text("C1")).toBe("=SUM(A1:B1)");
  });

  it("totals each column of a selected block in the row below", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "1", B1: "2", A2: "3", B2: "4" });
    render(<Harness id={id} />);
    selectRange("A1:B2");
    await user.click(cellAt(0, 0));
    await user.keyboard("{Shift>}");
    await user.click(cellAt(1, 1));
    await user.keyboard("{/Shift}");
    await user.click(screen.getByRole("button", { name: "AutoSum" }));
    expect([text("A3"), text("B3")]).toEqual(["=SUM(A1:A2)", "=SUM(B1:B2)"]);
  });

  it("opens the editor on an empty SUM with the caret inside when there is nothing to add", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "label" });
    render(<Harness id={id} />);
    await user.click(cellAt(4, 2));
    await user.click(screen.getByRole("button", { name: "AutoSum" }));
    const editor = document.querySelector<HTMLInputElement>(".cell-editor")!;
    expect(editor.value).toBe("=SUM()");
    expect(editor.selectionStart).toBe(5);
  });

  it("refuses on a protected sheet", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "1", A2: "2" }, (sheet) => ({ ...sheet, sheetProtection: "ABCD" }));
    render(<Harness id={id} />);
    await user.click(cellAt(2, 0));
    await user.click(screen.getByRole("button", { name: "AutoSum" }));
    expect(workbookOf().sheets[0].cells.A3).toBeUndefined();
  });
});
