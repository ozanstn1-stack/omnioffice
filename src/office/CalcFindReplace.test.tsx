import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null), save: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-fs", () => ({ readFile: vi.fn(async () => new Uint8Array()) }));

// jsdom has no workers: the probe is stubbed so the panel's reaction to a
// pattern that is too slow can be exercised. `null` means "no worker".
const probe = vi.hoisted(() => ({ result: null as "ok" | "slow" | null }));
vi.mock("./writer/regex-probe", () => ({
  canProbeRegex: () => probe.result !== null,
  probeRegex: () => ({ promise: Promise.resolve(probe.result), cancel: () => undefined }),
}));

import { useOfficeTabs } from "../lib/office-store";
import { cellText } from "../lib/office-types";
import { useToasts } from "../lib/store";
import { addSheetWith, cellAt, Harness, nameBox, seedWorkbook, workbookOf } from "./calc/ui/testing";

function panel() {
  return screen.getByRole("dialog", { name: /^Find/ });
}

async function openFind(user: ReturnType<typeof userEvent.setup>, shortcut: "f" | "h") {
  await user.click(cellAt(0, 0));
  await user.keyboard(`{Control>}${shortcut}{/Control}`);
  return panel();
}

describe("Calc find and replace panel", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
    useToasts.setState({ toasts: [] });
    probe.result = null;
  });

  it("opens with Ctrl+F, counts the cells found and steps through them with Enter", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "apple", B2: "pear", C3: "Apple pie" });
    render(<Harness id={id} />);

    const dialog = await openFind(user, "f");
    const input = within(dialog).getByRole("textbox", { name: "Find" });
    expect(input).toHaveFocus();
    // The Replace field only shows for Ctrl+H.
    expect(within(dialog).queryByRole("textbox", { name: "Replace with" })).toBeNull();

    await user.type(input, "apple");
    await waitFor(() => expect(within(dialog).getByRole("status")).toHaveTextContent("Cell 1 of 2"));
    await user.keyboard("{Enter}");
    expect(nameBox()).toBe("C3");
    await user.keyboard("{Enter}");
    expect(nameBox()).toBe("A1");
    await user.keyboard("{Shift>}{Enter}{/Shift}");
    expect(nameBox()).toBe("C3");
  });

  it("opens the replace field with Ctrl+H", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "x" });
    render(<Harness id={id} />);
    const dialog = await openFind(user, "h");
    expect(within(dialog).getByRole("textbox", { name: "Replace with" })).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Replace all" })).toBeInTheDocument();
  });

  it("matches the whole cell only when asked to", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "cat", A2: "cats", A3: "dog" });
    render(<Harness id={id} />);
    const dialog = await openFind(user, "f");
    await user.type(within(dialog).getByRole("textbox", { name: "Find" }), "cat");
    await waitFor(() => expect(within(dialog).getByRole("status")).toHaveTextContent("Cell 1 of 2"));
    await user.click(within(dialog).getByRole("checkbox", { name: "Match entire cell contents" }));
    await waitFor(() => expect(within(dialog).getByRole("status")).toHaveTextContent("Cell 1 of 1"));
  });

  it("jumps to a match on another sheet when searching the workbook", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "apple" });
    addSheetWith(id, "Data", { B2: "apple" });
    render(<Harness id={id} />);
    const dialog = await openFind(user, "f");
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Within" }), "workbook");
    await user.type(within(dialog).getByRole("textbox", { name: "Find" }), "apple");
    await waitFor(() => expect(within(dialog).getByRole("status")).toHaveTextContent("Cell 1 of 2"));

    await user.keyboard("{Enter}");
    expect(screen.getByRole("tab", { name: /Data/ })).toHaveAttribute("aria-selected", "true");
    expect(nameBox()).toBe("B2");
    await user.keyboard("{Enter}");
    expect(screen.getByRole("tab", { name: /Sheet1/ })).toHaveAttribute("aria-selected", "true");
    expect(nameBox()).toBe("A1");
  });

  it("replaces all as one undo step and recalculates formulas that read the cells", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "red apple", A2: "green apple", B1: "=LEN(A1)" });
    render(<Harness id={id} />);
    expect(cellAt(0, 1)).toHaveTextContent("9");

    const dialog = await openFind(user, "h");
    await user.type(within(dialog).getByRole("textbox", { name: "Find" }), "apple");
    await user.type(within(dialog).getByRole("textbox", { name: "Replace with" }), "plum");
    await waitFor(() => expect(within(dialog).getByRole("button", { name: "Replace all" })).toBeEnabled());
    await user.click(within(dialog).getByRole("button", { name: "Replace all" }));

    let sheet = workbookOf().sheets[0];
    expect(cellText(sheet.cells.A1)).toBe("red plum");
    expect(cellText(sheet.cells.A2)).toBe("green plum");
    expect(cellAt(0, 1)).toHaveTextContent("8");
    expect(useToasts.getState().toasts.map((toast) => toast.title)).toContain("Replaced 2 cells.");

    await user.click(screen.getByRole("button", { name: "Home" }));
    await user.click(screen.getByRole("button", { name: "Undo" }));
    sheet = workbookOf().sheets[0];
    expect(cellText(sheet.cells.A1)).toBe("red apple");
    expect(cellText(sheet.cells.A2)).toBe("green apple");
  });

  it("re-parses a replaced formula and shows the new result", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "5", B1: "7", C1: "=A1*2" });
    render(<Harness id={id} />);
    expect(cellAt(0, 2)).toHaveTextContent("10");

    const dialog = await openFind(user, "h");
    await user.type(within(dialog).getByRole("textbox", { name: "Find" }), "A1");
    await user.type(within(dialog).getByRole("textbox", { name: "Replace with" }), "B1");
    await waitFor(() => expect(within(dialog).getByRole("button", { name: "Replace all" })).toBeEnabled());
    await user.click(within(dialog).getByRole("button", { name: "Replace all" }));

    expect(workbookOf().sheets[0].cells.C1.formula).toBe("=B1*2");
    expect(cellAt(0, 2)).toHaveTextContent("14");
  });

  it("replaces the current cell and moves on to the next match", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "x1", A2: "x2", A3: "x3" });
    render(<Harness id={id} />);
    const dialog = await openFind(user, "h");
    await user.type(within(dialog).getByRole("textbox", { name: "Find" }), "x");
    await user.type(within(dialog).getByRole("textbox", { name: "Replace with" }), "y");
    await waitFor(() => expect(within(dialog).getByRole("button", { name: "Replace" })).toBeEnabled());

    await user.click(within(dialog).getByRole("button", { name: "Replace" }));
    expect(cellText(workbookOf().sheets[0].cells.A1)).toBe("y1");
    expect(nameBox()).toBe("A2");
    await user.click(within(dialog).getByRole("button", { name: "Replace" }));
    expect(cellText(workbookOf().sheets[0].cells.A2)).toBe("y2");
    expect(nameBox()).toBe("A3");
    expect(cellText(workbookOf().sheets[0].cells.A3)).toBe("x3");
  });

  it("never replaces inside a protected sheet", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "secret" }, (sheet) => ({ ...sheet, sheetProtection: "ABCD" }));
    render(<Harness id={id} />);
    const dialog = await openFind(user, "h");
    await user.type(within(dialog).getByRole("textbox", { name: "Find" }), "secret");
    await user.type(within(dialog).getByRole("textbox", { name: "Replace with" }), "open");
    await waitFor(() => expect(within(dialog).getByText(/cannot be replaced/)).toBeInTheDocument());

    await user.click(within(dialog).getByRole("button", { name: "Replace all" }));
    expect(cellText(workbookOf().sheets[0].cells.A1)).toBe("secret");
    expect(useToasts.getState().toasts.map((toast) => toast.title)).toContain("Nothing was replaced.");
  });

  it("reports an invalid regular expression instead of searching", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "a(b" });
    render(<Harness id={id} />);
    const dialog = await openFind(user, "f");
    await user.click(within(dialog).getByRole("checkbox", { name: "Regular expression" }));
    await user.type(within(dialog).getByRole("textbox", { name: "Find" }), "(");
    await waitFor(() => expect(within(dialog).getByRole("alert")).toHaveTextContent("Invalid regular expression"));
    expect(within(dialog).getByRole("button", { name: "Find next" })).toBeDisabled();
  });

  it("stops a pattern the probe finds too slow instead of running it on the grid", async () => {
    probe.result = "slow";
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "aaaaaaaaaaaaaaaaaaaaaaaaaaaa!" });
    render(<Harness id={id} />);
    const dialog = await openFind(user, "f");
    await user.click(within(dialog).getByRole("checkbox", { name: "Regular expression" }));
    await user.type(within(dialog).getByRole("textbox", { name: "Find" }), "(a+)+$");
    await waitFor(() => expect(within(dialog).getByRole("alert")).toHaveTextContent("takes too long"), {
      timeout: 2000,
    });
    expect(within(dialog).getByRole("button", { name: "Find next" })).toBeDisabled();
  });

  it("closes with Escape and hands the keyboard back to the grid", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "x" });
    render(<Harness id={id} />);
    await openFind(user, "f");
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: /^Find/ })).toBeNull();
    expect(document.querySelector(".calc-grid")).toHaveFocus();
  });
});
