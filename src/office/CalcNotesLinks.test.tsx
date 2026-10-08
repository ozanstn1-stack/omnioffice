import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null), save: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-fs", () => ({ readFile: vi.fn(async () => new Uint8Array()) }));

import { invoke } from "@tauri-apps/api/core";
import { useOfficeTabs } from "../lib/office-store";
import { useToasts } from "../lib/store";
import type { Cell, Workbook } from "../lib/office-types";
import { addSheetWith, cellAt, Harness, nameBox, seedWorkbook, setModel, workbookOf } from "./calc/ui/testing";

async function mount(id: string) {
  render(<Harness id={id} />);
  await act(async () => undefined);
}

function withAuthor(id: string, author: string) {
  const model = workbookOf();
  setModel(id, { ...model, metadata: { ...model.metadata, author } } as Workbook);
}

/** Changes a cell of the first sheet in the store, the way a loaded document would arrive. */
function patchCell(id: string, address: string, patch: Partial<Cell>) {
  const model = workbookOf();
  act(() =>
    setModel(id, {
      ...model,
      sheets: model.sheets.map((sheet, index) =>
        index === 0 ? { ...sheet, cells: { ...sheet.cells, [address]: { ...sheet.cells[address], ...patch } } } : sheet,
      ),
    }),
  );
}

function noteEditor() {
  return screen.getByRole("dialog", { name: /^Note for / });
}

async function openCellMenu(cell: HTMLElement) {
  fireEvent.contextMenu(cell);
  return screen.getByRole("menu", { name: "Cell actions" });
}

describe("Calc cell notes", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
    useToasts.setState({ toasts: [] });
    vi.mocked(invoke).mockClear();
  });

  it("inserts a note from the cell menu, defaulting the author to the document author", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "x" });
    withAuthor(id, "Ada");
    await mount(id);

    const menu = await openCellMenu(cellAt(0, 0));
    await user.click(within(menu).getByRole("menuitem", { name: "Insert note" }));
    const editor = screen.getByRole("dialog", { name: "Note for A1" });
    expect(within(editor).getByRole("textbox", { name: "Author" })).toHaveValue("Ada");
    await user.type(within(editor).getByRole("textbox", { name: "Note" }), "Check this");
    await user.click(within(editor).getByRole("button", { name: "Save" }));

    const cell = workbookOf().sheets[0].cells.A1;
    expect(cell).toMatchObject({ comment: "Check this", commentAuthor: "Ada" });
    expect(cell.value).toEqual({ kind: "text", value: "x" });
    expect(cellAt(0, 0).querySelector(".cell-comment-dot")).not.toBeNull();
    expect(screen.queryByRole("dialog", { name: "Note for A1" })).toBeNull();
  });

  it("leaves the author empty when the document has none", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "x" });
    await mount(id);
    const menu = await openCellMenu(cellAt(0, 0));
    await user.click(within(menu).getByRole("menuitem", { name: "Insert note" }));
    const editor = screen.getByRole("dialog", { name: "Note for A1" });
    expect(within(editor).getByRole("textbox", { name: "Author" })).toHaveValue("");
    await user.type(within(editor).getByRole("textbox", { name: "Note" }), "anon");
    await user.keyboard("{Control>}{Enter}{/Control}");
    expect(workbookOf().sheets[0].cells.A1.comment).toBe("anon");
    expect(workbookOf().sheets[0].cells.A1.commentAuthor ?? null).toBeNull();
  });

  it("is one undo step per edit", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "x" });
    await mount(id);
    const menu = await openCellMenu(cellAt(0, 0));
    await user.click(within(menu).getByRole("menuitem", { name: "Insert note" }));
    await user.type(screen.getByRole("textbox", { name: "Note" }), "one");
    await user.click(within(noteEditor()).getByRole("button", { name: "Save" }));
    expect(workbookOf().sheets[0].cells.A1.comment).toBe("one");

    await user.click(screen.getByRole("button", { name: "Undo" }));
    expect(workbookOf().sheets[0].cells.A1.comment ?? null).toBeNull();
    expect(workbookOf().sheets[0].cells.A1.value).toEqual({ kind: "text", value: "x" });
  });

  it("edits, hides/shows and deletes an existing note from the menu", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "x" });
    await mount(id);
    const first = await openCellMenu(cellAt(0, 0));
    await user.click(within(first).getByRole("menuitem", { name: "Insert note" }));
    await user.type(screen.getByRole("textbox", { name: "Note" }), "first");
    await user.click(within(noteEditor()).getByRole("button", { name: "Save" }));

    // Edit
    let menu = await openCellMenu(cellAt(0, 0));
    await user.click(within(menu).getByRole("menuitem", { name: "Edit note" }));
    const text = screen.getByRole("textbox", { name: "Note" });
    expect(text).toHaveValue("first");
    await user.clear(text);
    await user.type(text, "second");
    await user.click(within(noteEditor()).getByRole("button", { name: "Save" }));
    expect(workbookOf().sheets[0].cells.A1.comment).toBe("second");

    // Show on screen, then hide again
    menu = await openCellMenu(cellAt(0, 0));
    await user.click(within(menu).getByRole("menuitem", { name: "Show note" }));
    expect(workbookOf().sheets[0].cells.A1.commentVisible).toBe(true);
    const box = document.querySelector(".cell-note-box");
    expect(box).toHaveTextContent("second");
    menu = await openCellMenu(cellAt(0, 0));
    await user.click(within(menu).getByRole("menuitem", { name: "Hide note" }));
    expect(document.querySelector(".cell-note-box")).toBeNull();

    // Delete
    menu = await openCellMenu(cellAt(0, 0));
    await user.click(within(menu).getByRole("menuitem", { name: "Delete note" }));
    expect(workbookOf().sheets[0].cells.A1.comment ?? null).toBeNull();
    expect(cellAt(0, 0).querySelector(".cell-comment-dot")).toBeNull();
  });

  it("shows the author and text while the pointer is on a cell that has a note", async () => {
    const id = seedWorkbook({ A1: "x", B1: "y" });
    withAuthor(id, "Ada");
    await mount(id);
    patchCell(id, "A1", { comment: "Look here", commentAuthor: "Ada" });

    expect(screen.queryByRole("tooltip")).toBeNull();
    fireEvent.mouseOver(cellAt(0, 0));
    const tip = screen.getByRole("tooltip");
    expect(tip).toHaveTextContent("Ada");
    expect(tip).toHaveTextContent("Look here");
    fireEvent.mouseOver(cellAt(0, 1));
    expect(screen.queryByRole("tooltip")).toBeNull();
  });

  it("announces the note of the active cell", async () => {
    const id = seedWorkbook({ A1: "x" });
    await mount(id);
    patchCell(id, "A1", { comment: "Mind the gap" });
    expect(screen.getByRole("status")).toHaveTextContent("Note: Mind the gap");
  });

  it("refuses to add, edit or delete a note on a protected sheet", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "x" }, (sheet) => ({ ...sheet, sheetProtection: "ABCD" }));
    await mount(id);
    const menu = await openCellMenu(cellAt(0, 0));
    await user.click(within(menu).getByRole("menuitem", { name: "Insert note" }));
    expect(screen.queryByRole("dialog", { name: "Note for A1" })).toBeNull();
    expect(useToasts.getState().toasts.map((toast) => toast.title)).toContain(
      "This sheet is protected, so this change is not allowed.",
    );
    expect(workbookOf().sheets[0].cells.A1.comment ?? null).toBeNull();
  });

  it("opens the note editor with Shift+F2", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "x" });
    await mount(id);
    await user.click(cellAt(0, 0));
    await user.keyboard("{Shift>}{F2}{/Shift}");
    expect(screen.getByRole("dialog", { name: "Note for A1" })).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Note for A1" })).toBeNull();
    expect(workbookOf().sheets[0].cells.A1.comment ?? null).toBeNull();
  });
});

describe("Calc hyperlinks", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
    useToasts.setState({ toasts: [] });
    vi.mocked(invoke).mockClear();
  });

  it("inserts a web link with Ctrl+K, normalising the address, and draws the cell as a link", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "Docs" });
    await mount(id);
    await user.click(cellAt(0, 0));
    await user.keyboard("{Control>}k{/Control}");

    const dialog = screen.getByRole("dialog", { name: "Insert link" });
    expect(within(dialog).getByRole("textbox", { name: "Text to display" })).toHaveValue("Docs");
    await user.type(within(dialog).getByRole("textbox", { name: "Address" }), "example.org/docs");
    await user.type(within(dialog).getByRole("textbox", { name: "Screen tip" }), "Open the docs");
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));

    const cell = workbookOf().sheets[0].cells.A1;
    expect(cell).toMatchObject({ link: "https://example.org/docs", linkTooltip: "Open the docs" });
    expect(cell.value).toEqual({ kind: "text", value: "Docs" });
    expect(cellAt(0, 0).style.textDecoration).toBe("underline");
    expect(cellAt(0, 0).style.color).toBe("var(--accent)");
    expect(cellAt(0, 0)).toHaveAttribute("title", expect.stringContaining("Open the docs"));
  });

  it("changes the cell text when the text to display is edited", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "Docs" });
    await mount(id);
    await user.click(screen.getByRole("button", { name: "Insert" }));
    await user.click(screen.getByRole("button", { name: "Link" }));
    const dialog = screen.getByRole("dialog", { name: "Insert link" });
    const text = within(dialog).getByRole("textbox", { name: "Text to display" });
    await user.clear(text);
    await user.type(text, "Read the manual");
    await user.type(within(dialog).getByRole("textbox", { name: "Address" }), "https://example.org");
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));
    expect(workbookOf().sheets[0].cells.A1.value).toEqual({ kind: "text", value: "Read the manual" });
  });

  it("refuses an address outside the allow-list and keeps the dialog open", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "Docs" });
    await mount(id);
    await user.click(cellAt(0, 0));
    await user.keyboard("{Control>}k{/Control}");
    const dialog = screen.getByRole("dialog", { name: "Insert link" });
    for (const bad of ["javascript:alert(1)", "file:///etc/passwd", "ftp://example.org"]) {
      const address = within(dialog).getByRole("textbox", { name: "Address" });
      await user.clear(address);
      await user.type(address, bad);
      await user.click(within(dialog).getByRole("button", { name: "Apply" }));
      expect(within(dialog).getByRole("alert")).toHaveTextContent("Only web");
      expect(workbookOf().sheets[0].cells.A1.link ?? null).toBeNull();
    }
  });

  it("links to a place in this document and jumps there with Ctrl+click", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "Go" });
    addSheetWith(id, "My Sheet", { B3: "target" });
    await mount(id);
    await user.click(cellAt(0, 0));
    await user.keyboard("{Control>}k{/Control}");
    const dialog = screen.getByRole("dialog", { name: "Insert link" });
    await user.click(within(dialog).getByRole("radio", { name: "Place in this document" }));
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Sheet" }), "My Sheet");
    const reference = within(dialog).getByRole("combobox", { name: "Cell or name" });
    await user.clear(reference);
    await user.type(reference, "b3");
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));
    expect(workbookOf().sheets[0].cells.A1.link).toBe("#'My Sheet'!B3");

    fireEvent.pointerDown(cellAt(0, 0), { button: 0, ctrlKey: true, pointerId: 1 });
    await act(async () => undefined);
    expect(nameBox()).toBe("B3");
    expect(document.querySelector(".sheet-tab.is-active")).toHaveTextContent("My Sheet");
    expect(invoke).not.toHaveBeenCalledWith("open_external_link", expect.anything());
  });

  it("opens a web link through the native opener on Ctrl+click and not on a plain click", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "Docs", A2: "plain" });
    await mount(id);
    patchCell(id, "A1", { link: "https://example.org/docs" });

    await user.click(cellAt(0, 0));
    expect(invoke).not.toHaveBeenCalledWith("open_external_link", expect.anything());
    fireEvent.pointerDown(cellAt(0, 0), { button: 0, ctrlKey: true, pointerId: 1 });
    await act(async () => undefined);
    expect(invoke).toHaveBeenCalledWith("open_external_link", { url: "https://example.org/docs" });

    // A cell that is not a link ignores Ctrl+click.
    vi.mocked(invoke).mockClear();
    fireEvent.pointerDown(cellAt(1, 0), { button: 0, ctrlKey: true, pointerId: 2 });
    await act(async () => undefined);
    expect(invoke).not.toHaveBeenCalledWith("open_external_link", expect.anything());
  });

  it("never hands a hostile stored link to the opener", async () => {
    const id = seedWorkbook({ A1: "Evil" });
    await mount(id);
    patchCell(id, "A1", { link: "javascript:alert(1)" });
    expect(cellAt(0, 0).style.textDecoration).toBe("");
    fireEvent.pointerDown(cellAt(0, 0), { button: 0, ctrlKey: true, pointerId: 1 });
    await act(async () => undefined);
    expect(invoke).not.toHaveBeenCalledWith("open_external_link", expect.anything());
  });

  it("tells the user when the system could not open the link", async () => {
    vi.mocked(invoke).mockImplementation(async (command: string) => {
      if (command === "open_external_link") throw new Error("no opener");
      return null;
    });
    const id = seedWorkbook({ A1: "Docs" });
    await mount(id);
    patchCell(id, "A1", { link: "mailto:ada@example.org" });
    fireEvent.pointerDown(cellAt(0, 0), { button: 0, ctrlKey: true, pointerId: 1 });
    await act(async () => undefined);
    expect(invoke).toHaveBeenCalledWith("open_external_link", { url: "mailto:ada@example.org" });
    expect(useToasts.getState().toasts.map((toast) => toast.title)).toContain("The link could not be opened.");
  });

  it("follows a HYPERLINK formula on Ctrl+click", async () => {
    const id = seedWorkbook({ A1: "https://example.org/x", B1: '=HYPERLINK(A1,"Site")' });
    await mount(id);
    expect(cellAt(0, 1)).toHaveTextContent("Site");
    expect(cellAt(0, 1).style.textDecoration).toBe("underline");
    fireEvent.pointerDown(cellAt(0, 1), { button: 0, ctrlKey: true, pointerId: 1 });
    await act(async () => undefined);
    expect(invoke).toHaveBeenCalledWith("open_external_link", { url: "https://example.org/x" });
  });

  it("removes a link from the menu and keeps the text", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "Docs" });
    await mount(id);
    await user.click(cellAt(0, 0));
    await user.keyboard("{Control>}k{/Control}");
    await user.type(screen.getByRole("textbox", { name: "Address" }), "https://example.org");
    await user.click(screen.getByRole("button", { name: "Apply" }));
    expect(workbookOf().sheets[0].cells.A1.link).toBe("https://example.org");

    const menu = await openCellMenu(cellAt(0, 0));
    expect(within(menu).getByRole("menuitem", { name: "Open link" })).toBeInTheDocument();
    await user.click(within(menu).getByRole("menuitem", { name: "Remove link" }));
    const cell = workbookOf().sheets[0].cells.A1;
    expect(cell.link ?? null).toBeNull();
    expect(cell.value).toEqual({ kind: "text", value: "Docs" });
    expect(cellAt(0, 0).style.textDecoration).toBe("");
  });

  it("refuses to insert a link on a protected sheet", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "x" }, (sheet) => ({ ...sheet, sheetProtection: "ABCD" }));
    await mount(id);
    await user.click(cellAt(0, 0));
    await user.keyboard("{Control>}k{/Control}");
    expect(screen.queryByRole("dialog", { name: "Insert link" })).toBeNull();
    expect(useToasts.getState().toasts.length).toBe(1);
  });
});
