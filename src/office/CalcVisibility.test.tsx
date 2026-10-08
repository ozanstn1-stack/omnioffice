import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null), save: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-fs", () => ({ readFile: vi.fn(async () => new Uint8Array()) }));

import { useOfficeTabs } from "../lib/office-store";
import { useToasts } from "../lib/store";
import { cellAt, Harness, nameBox, seedWorkbook, selectRange, workbookOf } from "./calc/ui/testing";

const rendered = (row: number, col: number) => document.querySelector(`[data-cell="${row}:${col}"]`) !== null;

async function openView(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole("button", { name: "View" }));
}

describe("Calc hide and unhide rows and columns", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
    useToasts.setState({ toasts: [] });
  });

  it("hides the selected rows from the View tab and shows them again", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "one", A2: "two", A3: "three", A4: "four" });
    render(<Harness id={id} />);

    selectRange("A2:A3");
    await openView(user);
    await user.click(screen.getByRole("button", { name: "Hide rows" }));
    expect(workbookOf().sheets[0].rowHeights).toEqual({ "1": 0, "2": 0 });
    expect(rendered(1, 0)).toBe(false);
    expect(rendered(2, 0)).toBe(false);
    // The selection leaves the hidden cells for the next visible row.
    expect(nameBox()).toBe("A4");
    // The row header after the hidden run carries the marker.
    expect(document.querySelector('[data-row-header="3"]')).toHaveClass("has-hidden-before");

    // The neighbours of the hidden run are selected to reach it again.
    selectRange("A1:A4");
    await user.click(screen.getByRole("button", { name: "Unhide rows" }));
    expect(workbookOf().sheets[0].rowHeights).toEqual({});
    expect(rendered(1, 0)).toBe(true);
    expect(rendered(2, 0)).toBe(true);
  });

  it("hides and unhides columns, which draw nothing while hidden", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "a", B1: "b", C1: "c", D1: "d" });
    render(<Harness id={id} />);

    selectRange("B1:C1");
    await openView(user);
    await user.click(screen.getByRole("button", { name: "Hide columns" }));
    expect(workbookOf().sheets[0].colWidths).toMatchObject({ "1": 0, "2": 0 });
    expect(rendered(0, 1)).toBe(false);
    expect(rendered(0, 2)).toBe(false);
    expect(document.querySelector('[data-col-header="1"]')).toBeNull();
    expect(document.querySelector('[data-col-header="3"]')).toHaveClass("has-hidden-before");
    expect(nameBox()).toBe("D1");

    selectRange("A1:D1");
    await user.click(screen.getByRole("button", { name: "Unhide columns" }));
    expect(workbookOf().sheets[0].colWidths["1"]).toBeUndefined();
    expect(rendered(0, 1)).toBe(true);
    expect(rendered(0, 2)).toBe(true);
  });

  it("hides the columns of a one-click selection on the neighbour's header menu and offers Unhide there", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "a", B1: "b", C1: "c" });
    render(<Harness id={id} />);

    fireEvent.contextMenu(document.querySelector('[data-col-header="1"]')!);
    let menu = screen.getByRole("menu", { name: "Column actions" });
    expect(within(menu).getByRole("menuitem", { name: "Insert column" })).toBeInTheDocument();
    expect(within(menu).getByRole("menuitem", { name: "Delete column" })).toBeInTheDocument();
    // Nothing is hidden yet, so there is nothing to unhide.
    expect(within(menu).queryByRole("menuitem", { name: "Unhide columns" })).toBeNull();
    await user.click(within(menu).getByRole("menuitem", { name: "Hide columns" }));
    expect(screen.queryByRole("menu")).toBeNull();
    expect(workbookOf().sheets[0].colWidths["1"]).toBe(0);

    // Column C sits next to the hidden column B: its menu offers Unhide.
    fireEvent.contextMenu(document.querySelector('[data-col-header="2"]')!);
    menu = screen.getByRole("menu", { name: "Column actions" });
    await user.click(within(menu).getByRole("menuitem", { name: "Unhide columns" }));
    expect(workbookOf().sheets[0].colWidths["1"]).toBeUndefined();
    expect(rendered(0, 1)).toBe(true);
  });

  it("opens a row menu on a row header and closes it with Escape", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "a", A2: "b" });
    render(<Harness id={id} />);

    fireEvent.contextMenu(document.querySelector('[data-row-header="1"]')!);
    const menu = screen.getByRole("menu", { name: "Row actions" });
    expect(within(menu).getByRole("menuitem", { name: "Hide rows" })).toBeInTheDocument();
    // The menu takes the keyboard: the first item is focused and the arrows move.
    expect(within(menu).getByRole("menuitem", { name: "Insert row" })).toHaveFocus();
    await user.keyboard("{ArrowDown}");
    expect(within(menu).getByRole("menuitem", { name: "Delete row" })).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBeNull();
    expect(document.querySelector(".calc-grid")).toHaveFocus();
  });

  it("closes the menu on a press outside it", async () => {
    const id = seedWorkbook({ A1: "a" });
    render(<Harness id={id} />);
    // Let the editor's own async set-up (AI status) settle before the sync steps.
    await act(async () => undefined);
    fireEvent.contextMenu(document.querySelector('[data-col-header="0"]')!);
    expect(screen.getByRole("menu")).toBeInTheDocument();
    act(() => {
      fireEvent.pointerDown(document.body);
    });
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("inserts a row from the header menu in front of the clicked row", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "a", A2: "b" });
    render(<Harness id={id} />);
    fireEvent.contextMenu(document.querySelector('[data-row-header="1"]')!);
    await user.click(screen.getByRole("menuitem", { name: "Insert row" }));
    expect(workbookOf().sheets[0].cells.A3?.value).toEqual({ kind: "text", value: "b" });
    expect(workbookOf().sheets[0].cells.A2).toBeUndefined();
  });

  it("keeps a hidden row hidden on the content it hid when a row is inserted above", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "a", A2: "b", A3: "c" }, (sheet) => ({ ...sheet, rowHeights: { "1": 0 } }));
    render(<Harness id={id} />);
    fireEvent.contextMenu(document.querySelector('[data-row-header="0"]')!);
    await user.click(screen.getByRole("menuitem", { name: "Insert row" }));
    // "b" moved from row 2 to row 3, and its hidden state moved with it.
    expect(workbookOf().sheets[0].cells.A3?.value).toEqual({ kind: "text", value: "b" });
    expect(workbookOf().sheets[0].rowHeights).toEqual({ "2": 0 });
    expect(rendered(2, 0)).toBe(false);
  });

  it("skips hidden columns with the arrow keys, Tab and Home / End", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "a" }, (sheet) => ({ ...sheet, colWidths: { "0": 0, "2": 0, "25": 0 } }));
    render(<Harness id={id} />);

    // A is hidden: the first visible cell is B.
    await user.click(cellAt(0, 1));
    expect(nameBox()).toBe("B1");
    await user.keyboard("{ArrowRight}");
    expect(nameBox()).toBe("D1");
    await user.keyboard("{ArrowLeft}");
    expect(nameBox()).toBe("B1");
    await user.keyboard("{ArrowLeft}");
    expect(nameBox()).toBe("B1");
    await user.keyboard("{Tab}");
    expect(nameBox()).toBe("D1");
    await user.keyboard("{Shift>}{Tab}{/Shift}");
    expect(nameBox()).toBe("B1");
    await user.keyboard("{End}");
    expect(nameBox()).toBe("Y1");
    await user.keyboard("{Home}");
    expect(nameBox()).toBe("B1");
  });

  it("moves right over a hidden column after typing a value and pressing Tab", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({}, (sheet) => ({ ...sheet, colWidths: { "1": 0 } }));
    render(<Harness id={id} />);
    await user.click(cellAt(0, 0));
    await user.keyboard("7{Tab}");
    expect(nameBox()).toBe("C1");
    expect(workbookOf().sheets[0].cells.A1?.value).toEqual({ kind: "number", value: 7 });
  });

  it("hides with Ctrl+9 and Ctrl+0 and restores with the Shift variants as one undo step each", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "a", A2: "b", B1: "c" });
    render(<Harness id={id} />);
    await user.click(cellAt(1, 1));

    await user.keyboard("{Control>}9{/Control}");
    expect(workbookOf().sheets[0].rowHeights).toEqual({ "1": 0 });
    await user.keyboard("{Control>}0{/Control}");
    expect(workbookOf().sheets[0].colWidths["1"]).toBe(0);

    await user.click(screen.getByRole("button", { name: "Home" }));
    await user.click(screen.getByRole("button", { name: "Undo" }));
    expect(workbookOf().sheets[0].colWidths["1"]).toBeUndefined();
    expect(workbookOf().sheets[0].rowHeights).toEqual({ "1": 0 });
    await user.click(screen.getByRole("button", { name: "Undo" }));
    expect(workbookOf().sheets[0].rowHeights).toEqual({});
  });

  it("refuses to hide on a protected sheet", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "a", A2: "b" }, (sheet) => ({ ...sheet, sheetProtection: "ABCD" }));
    render(<Harness id={id} />);
    selectRange("A2");
    await openView(user);
    await user.click(screen.getByRole("button", { name: "Hide rows" }));
    expect(workbookOf().sheets[0].rowHeights).toEqual({});
    expect(useToasts.getState().toasts.map((toast) => toast.title)).toContain(
      "This sheet is protected, so this change is not allowed.",
    );
  });

  it("explains when there is nothing to unhide", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "a" });
    render(<Harness id={id} />);
    selectRange("A1:B2");
    await openView(user);
    await user.click(screen.getByRole("button", { name: "Unhide rows" }));
    expect(useToasts.getState().toasts[0]?.title).toMatch(/No hidden rows or columns/);
  });
});
