import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null), save: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-fs", () => ({ readFile: vi.fn(async () => new Uint8Array()) }));

import { useOfficeTabs } from "../lib/office-store";
import { cellAt, Harness, seedWorkbook, selectRange, workbookOf } from "./calc/ui/testing";

const grid = () => document.querySelector<HTMLElement>(".calc-grid")!;
const live = () => screen.getByRole("status");

/** Renders the editor and lets its own async set-up (AI status) settle. */
async function mount(id: string) {
  render(<Harness id={id} />);
  await act(async () => undefined);
}

describe("Calc grid accessibility", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
  });

  it("describes the grid: its size, and that several cells can be selected", async () => {
    const id = seedWorkbook({ A1: "x" });
    await mount(id);
    const sheet = workbookOf().sheets[0];
    expect(grid()).toHaveAttribute("role", "grid");
    expect(grid()).toHaveAttribute("aria-rowcount", String(sheet.rowCount));
    expect(grid()).toHaveAttribute("aria-colcount", String(sheet.colCount));
    expect(grid()).toHaveAttribute("aria-multiselectable", "true");
  });

  it("gives each cell the gridcell role and its 1-based position, inside a row", async () => {
    const id = seedWorkbook({ C2: "x" });
    await mount(id);
    const cell = cellAt(1, 2);
    expect(cell).toHaveAttribute("role", "gridcell");
    expect(cell).toHaveAttribute("aria-rowindex", "2");
    expect(cell).toHaveAttribute("aria-colindex", "3");
    const row = cell.parentElement!;
    expect(row).toHaveAttribute("role", "row");
    expect(row).toHaveAttribute("aria-rowindex", "2");
    // Every cell sits in the row of its own number.
    for (const element of document.querySelectorAll<HTMLElement>(".calc-cell")) {
      expect(element.parentElement).toHaveAttribute("aria-rowindex", element.getAttribute("aria-rowindex"));
    }
  });

  it("keeps true row and column numbers where rows and columns are hidden", async () => {
    const id = seedWorkbook({ A1: "x" }, (sheet) => ({
      ...sheet,
      rowHeights: { "1": 0 },
      colWidths: { "1": 0 },
    }));
    await mount(id);
    expect(document.querySelector('[data-cell="1:0"]')).toBeNull();
    expect(cellAt(2, 0)).toHaveAttribute("aria-rowindex", "3");
    expect(cellAt(0, 2)).toHaveAttribute("aria-colindex", "3");
  });

  it("points aria-activedescendant at the active cell and follows the keyboard", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "x" });
    await mount(id);
    expect(grid().getAttribute("aria-activedescendant")).toBe(cellAt(0, 0).id);
    expect(cellAt(0, 0).id).not.toBe("");

    await user.click(cellAt(0, 0));
    await user.keyboard("{ArrowRight}");
    expect(grid().getAttribute("aria-activedescendant")).toBe(cellAt(0, 1).id);
    await user.keyboard("{ArrowDown}");
    expect(grid().getAttribute("aria-activedescendant")).toBe(cellAt(1, 1).id);
  });

  it("leaves out aria-activedescendant while the active cell is not drawn", async () => {
    const id = seedWorkbook({ A1: "x" });
    await mount(id);
    selectRange("A150");
    expect(document.querySelector('[data-cell="149:0"]')).toBeNull();
    expect(grid()).not.toHaveAttribute("aria-activedescendant");
  });

  it("marks the selected cells with aria-selected", async () => {
    const id = seedWorkbook({ A1: "x" });
    await mount(id);
    selectRange("A1:B2");
    expect(cellAt(0, 0)).toHaveAttribute("aria-selected", "true");
    expect(cellAt(1, 1)).toHaveAttribute("aria-selected", "true");
    expect(cellAt(2, 2)).toHaveAttribute("aria-selected", "false");
  });

  it("flags a cell that breaks its validation rule", async () => {
    const id = seedWorkbook({ A1: "50", B1: "5" }, (sheet) => ({
      ...sheet,
      validations: [
        { id: "v", range: "A1:B1", kind: "number", values: [], min: 0, max: 10, message: "", allowBlank: true },
      ],
    }));
    await mount(id);
    expect(cellAt(0, 0)).toHaveAttribute("aria-invalid", "true");
    expect(cellAt(0, 1)).not.toHaveAttribute("aria-invalid");
  });

  it("hides the row and column header bands, whose numbers the cells already carry", async () => {
    const id = seedWorkbook({ A1: "x" });
    await mount(id);
    expect(document.querySelector(".calc-col-headers")).toHaveAttribute("aria-hidden", "true");
    expect(document.querySelector(".calc-row-headers")).toHaveAttribute("aria-hidden", "true");
  });
});

describe("Calc grid announcements", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
  });

  it("announces the active cell and its value in a polite live region", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "7", C1: "=A1*2" });
    await mount(id);
    expect(live()).toHaveAttribute("aria-live", "polite");
    expect(live()).toHaveTextContent("Cell A1: 7");

    await user.click(cellAt(0, 0));
    await user.keyboard("{ArrowRight}");
    expect(live()).toHaveTextContent("Cell B1: empty");
    await user.keyboard("{ArrowRight}");
    expect(live()).toHaveTextContent("Cell C1: 14, formula =A1*2");
  });

  it("announces the selected range with its active cell", async () => {
    const id = seedWorkbook({ A1: "x", B2: "y" });
    await mount(id);
    selectRange("A1:B2");
    expect(live()).toHaveTextContent("A1:B2 selected. Active cell B2: y");
  });

  it("announces a new value after the cell is edited", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "old" });
    await mount(id);
    await user.click(cellAt(0, 0));
    await user.keyboard("new{Enter}{ArrowUp}");
    expect(live()).toHaveTextContent("Cell A1: new");
  });
});

describe("Calc toggle buttons", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
  });

  it("report their pressed state", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "x" });
    await mount(id);
    await user.click(cellAt(0, 0));
    const bold = screen.getByRole("button", { name: "Bold" });
    expect(bold).toHaveAttribute("aria-pressed", "false");
    await user.click(bold);
    expect(screen.getByRole("button", { name: "Bold" })).toHaveAttribute("aria-pressed", "true");
    // Actions that are not toggles carry no pressed state.
    expect(screen.getByRole("button", { name: "Copy" })).not.toHaveAttribute("aria-pressed");
  });
});
