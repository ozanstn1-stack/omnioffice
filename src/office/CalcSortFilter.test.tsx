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
import { cellAt, Harness, seedWorkbook, selectRange, workbookOf } from "./calc/ui/testing";

const people = {
  A1: "Name",
  B1: "Age",
  A2: "Cy",
  B2: "30",
  A3: "Al",
  B3: "25",
  A4: "Bo",
  B4: "30",
  A5: "Di",
  B5: "41",
};

const column = (letter: string, rows: number) =>
  Array.from({ length: rows }, (_, row) => cellText(workbookOf().sheets[0].cells[`${letter}${row + 1}`]));

async function openData(user: ReturnType<typeof userEvent.setup>) {
  await act(async () => undefined);
  await user.click(screen.getByRole("button", { name: "Data" }));
}

describe("Calc custom sort", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
    useToasts.setState({ toasts: [] });
  });

  it("sorts by two keys, each in its own direction, as one undo step", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook(people);
    render(<Harness id={id} />);
    await user.click(cellAt(1, 0));
    await openData(user);
    await user.click(screen.getByRole("button", { name: "Custom sort..." }));

    const dialog = screen.getByRole("dialog", { name: "Sort" });
    // A text row above numbers looks like a header: the box starts checked and keys carry its names.
    expect(within(dialog).getByRole("checkbox", { name: "My data has headers" })).toBeChecked();
    expect(within(dialog).getByText("Range: A1:B5")).toBeInTheDocument();
    const keys = within(dialog).getAllByRole("combobox");
    // Sort by Age (descending), then by Name (ascending).
    await user.selectOptions(keys[0], "Age");
    await user.selectOptions(keys[1], "Descending");
    await user.selectOptions(keys[2], "Name");
    await user.click(within(dialog).getByRole("button", { name: "Sort" }));

    expect(screen.queryByRole("dialog")).toBeNull();
    expect(column("A", 5)).toEqual(["Name", "Di", "Bo", "Cy", "Al"]);
    expect(column("B", 5)).toEqual(["Age", "41", "30", "30", "25"]);

    await user.click(screen.getByRole("button", { name: "Home" }));
    await user.click(screen.getByRole("button", { name: "Undo" }));
    expect(column("A", 5)).toEqual(["Name", "Cy", "Al", "Bo", "Di"]);
  });

  it("keeps the header out of the sort only while the box is checked", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook(people);
    render(<Harness id={id} />);
    await user.click(cellAt(1, 0));
    await openData(user);
    await user.click(screen.getByRole("button", { name: "Custom sort..." }));
    const dialog = screen.getByRole("dialog", { name: "Sort" });
    await user.click(within(dialog).getByRole("checkbox", { name: "My data has headers" }));
    // Without headers the keys are named by column.
    await user.selectOptions(within(dialog).getAllByRole("combobox")[0], "Column A");
    await user.click(within(dialog).getByRole("button", { name: "Sort" }));
    // "Name" sorts among the names now (A..Z: Al, Bo, Cy, Di, Name).
    expect(column("A", 5)).toEqual(["Al", "Bo", "Cy", "Di", "Name"]);
  });

  it("moves formulas with their rows and adjusts their relative references", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "3", B1: "=A1*10", A2: "1", B2: "=A2*10", A3: "2", B3: "=A3*10" });
    render(<Harness id={id} />);
    selectRange("A1:B3");
    await openData(user);
    await user.click(screen.getByRole("button", { name: "Custom sort..." }));
    const dialog = screen.getByRole("dialog", { name: "Sort" });
    await user.click(within(dialog).getByRole("button", { name: "Sort" }));

    expect(column("A", 3)).toEqual(["1", "2", "3"]);
    expect(column("B", 3)).toEqual(["=A1*10", "=A2*10", "=A3*10"]);
    expect(cellAt(0, 1)).toHaveTextContent("10");
    expect(cellAt(2, 1)).toHaveTextContent("30");
  });

  it("sorts a single column from the Ascending button, treating plain numbers as data", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "3", A2: "1", A3: "2" });
    render(<Harness id={id} />);
    await user.click(cellAt(0, 0));
    await openData(user);
    await user.click(screen.getByRole("button", { name: "Ascending" }));
    expect(column("A", 3)).toEqual(["1", "2", "3"]);
    await user.click(screen.getByRole("button", { name: "Descending" }));
    expect(column("A", 3)).toEqual(["3", "2", "1"]);
  });

  it("sorts numbers before text and leaves blanks last", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "10", A2: "pear", A4: "apple", A5: "2" });
    render(<Harness id={id} />);
    selectRange("A1:A5");
    await openData(user);
    await user.click(screen.getByRole("button", { name: "Ascending" }));
    expect(column("A", 5)).toEqual(["2", "10", "apple", "pear", ""]);
  });

  it("sorts only the selected rows when several cells are selected", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook(people);
    render(<Harness id={id} />);
    // The header is outside the selection; the active cell (B5) names the key: Age.
    selectRange("A2:B5");
    await openData(user);
    await user.click(screen.getByRole("button", { name: "Ascending" }));
    expect(column("A", 5)).toEqual(["Name", "Al", "Cy", "Bo", "Di"]);
    expect(column("B", 5)).toEqual(["Age", "25", "30", "30", "41"]);
  });

  it("says so when the rows are already in order", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "1", A2: "2" });
    render(<Harness id={id} />);
    await user.click(cellAt(0, 0));
    await openData(user);
    await user.click(screen.getByRole("button", { name: "Ascending" }));
    expect(useToasts.getState().toasts.map((toast) => toast.title)).toContain(
      "Nothing to sort: the rows are already in this order.",
    );
  });

  it("refuses ranges with merged cells and protected sheets", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "b", A2: "a" }, (sheet) => ({ ...sheet, merges: [{ start: "A1", end: "B1" }] }));
    render(<Harness id={id} />);
    await user.click(cellAt(0, 0));
    await openData(user);
    await user.click(screen.getByRole("button", { name: "Ascending" }));
    expect(useToasts.getState().toasts.map((toast) => toast.title)).toContain(
      "Sort cannot rearrange rows that contain merged cells.",
    );
    expect(column("A", 2)).toEqual(["b", "a"]);
  });

  it("refuses to sort a protected sheet", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "b", A2: "a" }, (sheet) => ({ ...sheet, sheetProtection: "ABCD" }));
    render(<Harness id={id} />);
    await user.click(cellAt(0, 0));
    await openData(user);
    await user.click(screen.getByRole("button", { name: "Custom sort..." }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(useToasts.getState().toasts.map((toast) => toast.title)).toContain(
      "This sheet is protected, so this change is not allowed.",
    );
  });
});

describe("Calc filter conditions", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
    useToasts.setState({ toasts: [] });
  });

  async function openFilter(user: ReturnType<typeof userEvent.setup>, col: number) {
    await user.click(cellAt(0, col));
    await openData(user);
    await user.click(screen.getByRole("button", { name: "Filter" }));
    return screen.getByRole("dialog", { name: "Filter" });
  }

  it("keeps the rows that meet a number condition and hides the others", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook(people);
    render(<Harness id={id} />);
    const dialog = await openFilter(user, 1);
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Condition" }), "Greater than");
    await user.type(within(dialog).getByRole("textbox", { name: "Value" }), "28");
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));

    // Rows 2 and 4 (30), 5 (41) stay; row 3 (25) is hidden. The header stays.
    expect(workbookOf().sheets[0].rowHeights).toEqual({ "2": 0 });
    expect(document.querySelector('[data-cell="2:0"]')).toBeNull();
    expect(document.querySelector('[data-cell="0:0"]')).not.toBeNull();
  });

  it("filters text with begins-with and combines it with the value list", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "Fruit", A2: "apple", A3: "apricot", A4: "banana", A5: "avocado" });
    render(<Harness id={id} />);
    const dialog = await openFilter(user, 0);
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Condition" }), "Begins with");
    await user.type(within(dialog).getByRole("textbox", { name: "Value" }), "AP");
    // Uncheck "apple": the condition alone would keep it, the list removes it (AND).
    await user.click(within(dialog).getByRole("checkbox", { name: "apple" }));
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));

    expect(workbookOf().sheets[0].rowHeights).toEqual({ "1": 0, "3": 0, "4": 0 });
    expect(workbookOf().sheets[0].filter?.values).toEqual(["apricot"]);
  });

  it("filters the top items and blank cells", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "Score", A2: "5", A3: "9", A4: "7", A6: "9" });
    render(<Harness id={id} />);
    let dialog = await openFilter(user, 0);
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Condition" }), "Top items");
    await user.type(within(dialog).getByRole("textbox", { name: "Number of items" }), "1");
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));
    // Both 9s stay (ties); 5, 7 and the empty row go.
    expect(workbookOf().sheets[0].rowHeights).toEqual({ "1": 0, "3": 0, "4": 0 });

    dialog = await openFilter(user, 0);
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Condition" }), "Is blank");
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));
    expect(workbookOf().sheets[0].rowHeights).toEqual({ "1": 0, "2": 0, "3": 0, "5": 0 });
  });

  it("does not hide anything for a condition that has no value yet", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook(people);
    render(<Harness id={id} />);
    const dialog = await openFilter(user, 1);
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Condition" }), "Greater than");
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));
    expect(workbookOf().sheets[0].rowHeights).toEqual({});
  });
});
