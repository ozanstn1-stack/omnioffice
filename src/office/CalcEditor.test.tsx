import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

/**
 * Regression tests for the Calc editing loop. The README used to carry this as
 * a known limitation: "After committing a cell with Enter, continuing to type
 * without clicking the next cell is not yet fully reliable."
 *
 * The cause was two-fold. The grid keydown handler closed over the `editing`
 * *state*, so a keystroke that arrived in the same tick as the commit was
 * rejected by the previous render's closure; and focus was restored in a
 * passive effect, which runs after paint - a fast keystroke was dispatched to
 * `<body>` before the grid got focus back. These tests type straight after
 * Enter with no clicking in between and assert both the model and the caret
 * destination.
 */
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null), save: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-fs", () => ({ readFile: vi.fn(async () => new Uint8Array()) }));
// Android layout is a prop of the environment, not of the editor: keep the
// real mobile helpers and only flip the platform probe per test.
vi.mock("../lib/mobile", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/mobile")>();
  return { ...actual, isAndroid: vi.fn(() => false) };
});

import { CalcEditor, clampGridZoom, pinchGridZoom, shiftFormulaColumns } from "./CalcEditor";
import { isAndroid } from "../lib/mobile";

// jsdom has no PointerEvent, so testing-library would fall back to a plain
// Event and drop button/clientX/pointerId. A MouseEvent subclass carries those
// fields plus pointerId/pointerType, which the touch gestures branch on.
if (typeof window.PointerEvent === "undefined") {
  class TestPointerEvent extends MouseEvent {
    readonly pointerId: number;
    readonly pointerType: string;
    constructor(type: string, init: PointerEventInit = {}) {
      super(type, init);
      this.pointerId = init.pointerId ?? 0;
      this.pointerType = init.pointerType ?? "";
    }
  }
  window.PointerEvent = TestPointerEvent as unknown as typeof PointerEvent;
}
import { applyCellEdit } from "./calc/cells";
import { useOfficeTabs, type OfficeTab } from "../lib/office-store";
import { cellText, type Sheet, type Workbook } from "../lib/office-types";
import { useToasts } from "../lib/store";

function Harness({ id }: { id: string }) {
  const tab = useOfficeTabs((state) => state.tabs.find((candidate) => candidate.id === id));
  if (!tab) return null;
  return <CalcEditor tab={tab as OfficeTab & { model: Workbook }} />;
}

function cells(): HTMLElement[] {
  return Array.from(document.querySelectorAll<HTMLElement>(".calc-cell"));
}

function activeCellEditor(): HTMLInputElement | null {
  return document.querySelector<HTMLInputElement>(".cell-editor");
}

function workbookOf(): Workbook {
  return useOfficeTabs.getState().tabs[0].model as Workbook;
}

function cellAt(row: number, col: number): HTMLElement {
  const element = document.querySelector<HTMLElement>(`[data-cell="${row}:${col}"]`);
  if (!element) throw new Error(`cell ${row}:${col} is not rendered`);
  return element;
}

describe("Calc keyboard entry is reliable without clicking between cells", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
  });

  it("keeps typing into the next cell after Enter", async () => {
    const user = userEvent.setup();
    const id = useOfficeTabs.getState().create("calc", "Untitled");
    render(<Harness id={id} />);
    expect(cells().length).toBeGreaterThan(2);

    await user.click(cells()[0]);
    await user.keyboard("1");
    expect(activeCellEditor()?.value).toBe("1");

    await user.keyboard("{Enter}");
    // The grid must own focus again in the same event, so the very next
    // keystroke is not swallowed by <body>.
    expect(document.activeElement).toBe(document.querySelector(".calc-grid"));

    await user.keyboard("2");
    expect(activeCellEditor()?.value).toBe("2");
    await user.keyboard("{Enter}");

    const sheet = workbookOf().sheets[0];
    expect(cellText(sheet.cells.A1)).toBe("1");
    expect(cellText(sheet.cells.A2)).toBe("2");
  });

  it("keeps a whole run of consecutive values in one pass", async () => {
    const user = userEvent.setup();
    const id = useOfficeTabs.getState().create("calc", "Untitled");
    render(<Harness id={id} />);

    await user.click(cells()[0]);
    for (const value of ["10", "20", "30", "40"]) {
      await user.keyboard(`${value}{Enter}`);
    }

    const sheet = workbookOf().sheets[0];
    expect(cellText(sheet.cells.A1)).toBe("10");
    expect(cellText(sheet.cells.A2)).toBe("20");
    expect(cellText(sheet.cells.A3)).toBe("30");
    expect(cellText(sheet.cells.A4)).toBe("40");
  });

  it("aims the caret at the edited cell when the next entry starts with a click-free Escape", async () => {
    const user = userEvent.setup();
    const id = useOfficeTabs.getState().create("calc", "Untitled");
    render(<Harness id={id} />);

    await user.click(cells()[0]);
    await user.keyboard("5");
    await user.keyboard("{Enter}");
    await user.keyboard("{ArrowDown}");
    await user.keyboard("{ArrowUp}");
    await user.keyboard("6{Enter}");

    const sheet = workbookOf().sheets[0];
    expect(cellText(sheet.cells.A1)).toBe("5");
    expect(cellText(sheet.cells.A2)).toBe("6");
  });

  it("moves right with Tab and commits into the neighbouring column", async () => {
    const user = userEvent.setup();
    const id = useOfficeTabs.getState().create("calc", "Untitled");
    render(<Harness id={id} />);

    await user.click(cells()[0]);
    await user.keyboard("7{Tab}");
    expect(document.activeElement).toBe(document.querySelector(".calc-grid"));
    await user.keyboard("8{Enter}");

    const sheet = workbookOf().sheets[0];
    expect(cellText(sheet.cells.A1)).toBe("7");
    expect(cellText(sheet.cells.B1)).toBe("8");
  });

  it("applies a data-bar conditional rule and scales the bar to the range maximum", async () => {
    const user = userEvent.setup();
    const id = useOfficeTabs.getState().create("calc", "Untitled");
    render(<Harness id={id} />);

    await user.click(cells()[0]);
    await user.keyboard("50{Enter}");
    await user.keyboard("10{Enter}");

    await user.click(screen.getByRole("button", { name: "Formulas" }));
    await user.click(screen.getByRole("button", { name: "Conditional formatting" }));
    const dialog = screen.getByRole("dialog");
    await user.selectOptions(within(dialog).getByRole("combobox"), "dataBar");
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));

    const rules = workbookOf().sheets[0].conditional;
    expect(rules.at(-1)?.kind).toBe("dataBar");
    const bar = document.querySelector<HTMLElement>(".data-bar");
    expect(bar).not.toBeNull();
    expect(bar!.style.width).toBe("100%");
  });

  it("highlights only the cells a greater-than rule matches", async () => {
    const user = userEvent.setup();
    const id = useOfficeTabs.getState().create("calc", "Untitled");
    render(<Harness id={id} />);

    await user.click(cells()[0]);
    await user.keyboard("50{Enter}");
    await user.keyboard("10{Enter}");

    await user.click(screen.getByRole("button", { name: "Formulas" }));
    await user.click(screen.getByRole("button", { name: "Conditional formatting" }));
    const dialog = screen.getByRole("dialog");
    const valueField = within(dialog).getByRole("textbox", { name: "Value" });
    await user.clear(valueField);
    await user.type(valueField, "20");
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));

    // A1 (50) matches, A2 (10) does not.
    expect(cells()[0].style.background).not.toBe("");
    expect(cells()[1].style.background).toBe("");
  });

  it("inserts a live pivot table and renders its grid", async () => {
    const user = userEvent.setup();
    const id = useOfficeTabs.getState().create("calc", "Untitled");
    // Seed a small table directly: a header row plus two data rows.
    const table = [
      ["Department", "Year", "Sales"],
      ["Hardware", "2025", "100"],
      ["Hardware", "2025", "150"],
    ];
    let model = useOfficeTabs.getState().tabs[0].model as Workbook;
    table.forEach((row, rowIndex) =>
      row.forEach((value, colIndex) => {
        model = applyCellEdit(model, 0, rowIndex, colIndex, value);
      }),
    );
    useOfficeTabs.setState((state) => ({ tabs: state.tabs.map((tab) => (tab.id === id ? { ...tab, model } : tab)) }));
    render(<Harness id={id} />);

    await user.click(screen.getByRole("button", { name: "Insert" }));
    await user.click(screen.getByRole("button", { name: "Pivot table" }));
    const dialog = screen.getByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Insert pivot" }));

    const sheet = workbookOf().sheets[0];
    expect(sheet.pivotTables).toHaveLength(1);
    expect(sheet.pivotTables[0].sourceSheet).toBe(sheet.name);
    const grid = document.querySelector(".pivot-grid");
    expect(grid).not.toBeNull();
    expect(grid!.textContent).toContain("Department");
    expect(grid!.textContent).toContain("250");
  });

  it("commits a formula from the formula bar and continues on the grid", async () => {
    const user = userEvent.setup();
    const id = useOfficeTabs.getState().create("calc", "Untitled");
    render(<Harness id={id} />);

    await user.click(cells()[0]);
    await user.keyboard("2{Enter}");
    await user.keyboard("3{Enter}");

    // After two Enter commits the selection is already on A3.
    const formulaBar = document.querySelector<HTMLInputElement>(".formula-input");
    expect(formulaBar).not.toBeNull();
    // Desktop keeps the stock, non-docked bar under the ribbon.
    expect(document.querySelector(".calc-formula-bar")?.classList.contains("is-docked")).toBe(false);
    await user.click(formulaBar!);
    await user.keyboard("=SUM(A1:A2){Enter}");

    expect(document.activeElement).toBe(document.querySelector(".calc-grid"));
    const sheet = workbookOf().sheets[0];
    expect(sheet.cells.A3?.formula).toBe("=SUM(A1:A2)");
    // The cached result must reflect the edit, not the pre-edit workbook.
    expect(sheet.cells.A3?.value).toEqual({ kind: "number", value: 5 });

    expect(document.querySelector<HTMLInputElement>(".name-box")?.value).toBe("A4");
    await user.keyboard("9{Enter}");
    expect(document.querySelector<HTMLInputElement>(".name-box")?.value).toBe("A5");
    expect(cellText(workbookOf().sheets[0].cells.A4)).toBe("9");
  });
});

describe("Calc pointer gestures", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
    vi.mocked(isAndroid).mockReturnValue(false);
  });

  it("extends the selection while a pointer is dragged across cells", () => {
    const id = useOfficeTabs.getState().create("calc", "Untitled");
    render(<Harness id={id} />);

    fireEvent.pointerDown(cellAt(0, 0), { pointerId: 1, pointerType: "mouse", button: 0, clientX: 4, clientY: 4 });
    // The move bubbles to the window listener, whose target still carries the
    // cell data attributes; no hit-testing stub is needed in jsdom.
    fireEvent.pointerMove(cellAt(0, 2), { pointerId: 1, pointerType: "mouse", clientX: 300, clientY: 4 });
    fireEvent.pointerUp(window, { pointerId: 1, pointerType: "mouse" });

    expect(document.querySelector<HTMLInputElement>(".name-box")?.value).toBe("C1:A1");
    expect(
      cells()
        .slice(0, 3)
        .every((cell) => cell.classList.contains("is-selected")),
    ).toBe(true);
    expect(cells()[3].classList.contains("is-selected")).toBe(false);
  });

  it("pans with one finger on a cell and selects the cell on a tap", () => {
    const id = useOfficeTabs.getState().create("calc", "Untitled");
    render(<Harness id={id} />);
    const nameBox = () => document.querySelector<HTMLInputElement>(".name-box")?.value;

    // A drag that starts on a cell scrolls the sheet and leaves the selection.
    fireEvent.pointerDown(cellAt(1, 1), { pointerId: 3, pointerType: "touch", button: 0, clientX: 200, clientY: 200 });
    fireEvent.pointerMove(cellAt(0, 0), { pointerId: 3, pointerType: "touch", clientX: 120, clientY: 150 });
    fireEvent.pointerUp(window, { pointerId: 3, pointerType: "touch" });
    expect(nameBox()).toBe("A1");

    // A tap (no movement) selects the cell under the finger.
    fireEvent.pointerDown(cellAt(1, 1), { pointerId: 4, pointerType: "touch", button: 0, clientX: 200, clientY: 200 });
    fireEvent.pointerUp(window, { pointerId: 4, pointerType: "touch" });
    expect(nameBox()).toBe("B2");
  });

  it("extends the selection with the touch selection handles", () => {
    const id = useOfficeTabs.getState().create("calc", "Untitled");
    render(<Harness id={id} />);
    fireEvent.pointerDown(cellAt(1, 1), { pointerId: 5, pointerType: "touch", button: 0, clientX: 200, clientY: 200 });
    fireEvent.pointerUp(window, { pointerId: 5, pointerType: "touch" });

    const end = document.querySelector<HTMLElement>('[data-select-handle="end"]');
    expect(end).not.toBeNull();
    fireEvent.pointerDown(end!, { pointerId: 6, pointerType: "touch", button: 0, clientX: 260, clientY: 230 });
    fireEvent.pointerMove(cellAt(3, 2), { pointerId: 6, pointerType: "touch", clientX: 300, clientY: 300 });
    fireEvent.pointerUp(window, { pointerId: 6, pointerType: "touch" });
    expect(document.querySelector<HTMLInputElement>(".name-box")?.value).toBe("C4:B2");

    // The start handle moves the other corner; the bottom-right one stays put.
    const start = document.querySelector<HTMLElement>('[data-select-handle="start"]');
    fireEvent.pointerDown(start!, { pointerId: 8, pointerType: "touch", button: 0, clientX: 150, clientY: 180 });
    fireEvent.pointerMove(cellAt(0, 0), { pointerId: 8, pointerType: "touch", clientX: 60, clientY: 30 });
    fireEvent.pointerUp(window, { pointerId: 8, pointerType: "touch" });
    expect(document.querySelector<HTMLInputElement>(".name-box")?.value).toBe("A1:C4");
  });

  it("fills formula cells down when the touch fill handle is dragged", async () => {
    const user = userEvent.setup();
    const id = useOfficeTabs.getState().create("calc", "Untitled");
    render(<Harness id={id} />);

    await user.click(cellAt(0, 0));
    await user.keyboard("=B1+1{Enter}");
    // The commit moved to A2; go back so the fill source is A1.
    await user.click(cellAt(0, 0));

    const handle = document.querySelector<HTMLElement>("[data-fill-handle]");
    expect(handle).not.toBeNull();
    fireEvent.pointerDown(handle!, { pointerId: 7, pointerType: "touch", button: 0, clientX: 100, clientY: 40 });
    fireEvent.pointerMove(cellAt(1, 0), { pointerId: 7, pointerType: "touch", clientX: 100, clientY: 64 });
    fireEvent.pointerUp(window, { pointerId: 7, pointerType: "touch" });

    const sheet = workbookOf().sheets[0];
    expect(sheet.cells.A1?.formula).toBe("=B1+1");
    // Filled down: the row reference follows, the column does not.
    expect(sheet.cells.A2?.formula).toBe("=B2+1");
  });

  it("commits the docked formula bar with its check button on Android", async () => {
    vi.mocked(isAndroid).mockReturnValue(true);
    const user = userEvent.setup();
    const id = useOfficeTabs.getState().create("calc", "Untitled");
    render(<Harness id={id} />);

    const bar = document.querySelector<HTMLElement>(".calc-formula-bar");
    expect(bar).not.toBeNull();
    expect(bar!.classList.contains("is-docked")).toBe(true);
    // Only one bar is rendered on Android; the desktop one sits above the grid.
    expect(document.querySelectorAll(".calc-formula-bar")).toHaveLength(1);

    const input = bar!.querySelector<HTMLInputElement>(".formula-input")!;
    await user.click(input);
    await user.keyboard("=1+1");
    await user.click(within(bar!).getByRole("button", { name: "Apply" }));

    expect(workbookOf().sheets[0].cells.A1?.formula).toBe("=1+1");
    expect(document.querySelector<HTMLInputElement>(".calc-formula-bar .formula-input")?.value).toBe("=1+1");
  });

  it("applies an autocomplete suggestion on pointerdown for touch", async () => {
    const user = userEvent.setup();
    const id = useOfficeTabs.getState().create("calc", "Untitled");
    render(<Harness id={id} />);

    const input = document.querySelector<HTMLInputElement>(".formula-input")!;
    await user.click(input);
    await user.type(input, "=SU");
    const assist = await waitFor(() => {
      const element = document.querySelector<HTMLElement>(".calc-assist");
      expect(element).not.toBeNull();
      return element!;
    });
    // The catalogue is alphabetical, so pick the SUM row explicitly.
    const option = within(assist)
      .getAllByRole("option")
      .find((entry) => entry.textContent?.startsWith("SUM"));
    expect(option).toBeDefined();
    fireEvent.pointerDown(option!);

    expect(input.value).toBe("=SUM(");
  });
});

describe("Calc touch maths", () => {
  it("clamps the pinched grid zoom to 0.6 - 2.0", () => {
    expect(clampGridZoom(0.1)).toBe(0.6);
    expect(clampGridZoom(9)).toBe(2);
    expect(clampGridZoom(1.25)).toBe(1.25);
  });

  it("scales the starting zoom by the pinch distance ratio", () => {
    expect(pinchGridZoom(1, 100, 200)).toBe(2);
    // 1 * 50/100 would be 0.5; the floor keeps the grid usable.
    expect(pinchGridZoom(1, 100, 50)).toBe(0.6);
    // A degenerate gesture keeps the zoom it started with.
    expect(pinchGridZoom(1.5, 0, 0)).toBe(1.5);
  });

  it("shifts relative column references and leaves absolute ones", () => {
    expect(shiftFormulaColumns("=SUM(A1:B2)", 2)).toBe("=SUM(C1:D2)");
    expect(shiftFormulaColumns("=$A1+B$2", 1)).toBe("=$A1+C$2");
    // Function names and typed-in lower case survive untouched.
    expect(shiftFormulaColumns("=sum(a1:b1)", 1)).toBe("=sum(b1:c1)");
    expect(shiftFormulaColumns(null, 1)).toBeNull();
  });
});

/** Creates a workbook tab with typed values (`{ A1: "x" }`) and an optional sheet patch. */
function seedWorkbook(values: Record<string, string>, patch: (sheet: Sheet) => Sheet = (sheet) => sheet): string {
  const id = useOfficeTabs.getState().create("calc", "Untitled");
  let model = useOfficeTabs.getState().tabs[0].model as Workbook;
  for (const [address, value] of Object.entries(values)) {
    const col = address.charCodeAt(0) - 65;
    const row = Number(address.slice(1)) - 1;
    model = applyCellEdit(model, 0, row, col, value);
  }
  model = { ...model, sheets: model.sheets.map((sheet, index) => (index === 0 ? patch(sheet) : sheet)) };
  useOfficeTabs.setState((state) => ({ tabs: state.tabs.map((tab) => (tab.id === id ? { ...tab, model } : tab)) }));
  return id;
}

function selectRange(range: string) {
  fireEvent.change(document.querySelector<HTMLInputElement>(".name-box")!, { target: { value: range } });
}

describe("Calc data tools", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
    useToasts.setState({ toasts: [] });
    vi.mocked(isAndroid).mockReturnValue(false);
  });

  it("splits a column on commas into numbers and text as one undo step", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "Ada,Lovelace,1815", A2: "Alan,Turing,1912", A3: "Grace" });
    render(<Harness id={id} />);

    selectRange("A1:A3");
    await user.click(screen.getByRole("button", { name: "Data" }));
    await user.click(screen.getByRole("button", { name: "Text to columns" }));
    const dialog = screen.getByRole("dialog", { name: "Text to columns" });
    expect(within(dialog).getByRole("radio", { name: "Comma" })).toBeChecked();
    const preview = within(dialog).getByRole("table", { name: "Preview" });
    expect(
      within(preview)
        .getAllByRole("columnheader")
        .map((cell) => cell.textContent),
    ).toEqual(["A", "B", "C"]);
    expect(preview.textContent).toContain("Lovelace");
    await user.click(within(dialog).getByRole("button", { name: "Split" }));

    expect(screen.queryByRole("dialog")).toBeNull();
    let sheet = workbookOf().sheets[0];
    expect(sheet.cells.A1?.value).toEqual({ kind: "text", value: "Ada" });
    expect(sheet.cells.B1?.value).toEqual({ kind: "text", value: "Lovelace" });
    expect(sheet.cells.C1?.value).toEqual({ kind: "number", value: 1815 });
    expect(sheet.cells.C2?.value).toEqual({ kind: "number", value: 1912 });
    // A row without the delimiter is left alone.
    expect(cellText(sheet.cells.A3)).toBe("Grace");
    expect(sheet.cells.B3).toBeUndefined();

    await user.click(screen.getByRole("button", { name: "Home" }));
    await user.click(screen.getByRole("button", { name: "Undo" }));
    sheet = workbookOf().sheets[0];
    expect(cellText(sheet.cells.A1)).toBe("Ada,Lovelace,1815");
    expect(sheet.cells.B1).toBeUndefined();
    expect(sheet.cells.C2).toBeUndefined();
  });

  it("asks before a split replaces data to the right", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "x;;y", B1: "old", C1: "keep" });
    render(<Harness id={id} />);

    selectRange("A1");
    await user.click(screen.getByRole("button", { name: "Data" }));
    await user.click(screen.getByRole("button", { name: "Text to columns" }));
    const dialog = screen.getByRole("dialog", { name: "Text to columns" });
    // The comma does not split this text at all.
    expect(within(dialog).getByRole("button", { name: "Split" })).toBeDisabled();
    await user.click(within(dialog).getByRole("radio", { name: "Semicolon" }));
    await user.click(within(dialog).getByRole("checkbox", { name: "Treat consecutive delimiters as one" }));
    await user.click(within(dialog).getByRole("button", { name: "Split" }));

    // Nothing is written until the replacement is confirmed.
    expect(within(dialog).getByRole("alert").textContent).toContain("(1)");
    expect(cellText(workbookOf().sheets[0].cells.B1)).toBe("old");
    await user.click(within(dialog).getByRole("button", { name: "Replace" }));

    const sheet = workbookOf().sheets[0];
    expect(cellText(sheet.cells.A1)).toBe("x");
    expect(cellText(sheet.cells.B1)).toBe("y");
    expect(cellText(sheet.cells.C1)).toBe("keep");
  });

  it("splits on a custom delimiter and refuses a multi-column selection", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "2024|05|17" });
    render(<Harness id={id} />);

    selectRange("A1:B1");
    await user.click(screen.getByRole("button", { name: "Data" }));
    await user.click(screen.getByRole("button", { name: "Text to columns" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(useToasts.getState().toasts.at(-1)?.title).toBe("Select cells in a single column to split them.");

    selectRange("A1");
    await user.click(screen.getByRole("button", { name: "Text to columns" }));
    const dialog = screen.getByRole("dialog", { name: "Text to columns" });
    await user.type(within(dialog).getByRole("textbox", { name: "Custom delimiter" }), "|");
    expect(within(dialog).getByRole("radio", { name: "Other" })).toBeChecked();
    await user.click(within(dialog).getByRole("button", { name: "Split" }));

    const sheet = workbookOf().sheets[0];
    expect(sheet.cells.A1?.value).toEqual({ kind: "number", value: 2024 });
    expect(sheet.cells.B1?.value).toEqual({ kind: "number", value: 5 });
    expect(sheet.cells.C1?.value).toEqual({ kind: "number", value: 17 });
  });

  it("removes duplicate rows inside the selection and reports the counts", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({
      A1: "Name",
      B1: "City",
      A2: "Ada",
      B2: "London",
      A3: "ada",
      B3: "Paris",
      A4: "Alan",
      B4: "Paris",
      A5: "Ada",
      B5: "Rome",
      C3: "outside",
      A7: "below",
    });
    render(<Harness id={id} />);

    selectRange("A1:B5");
    await user.click(screen.getByRole("button", { name: "Data" }));
    await user.click(screen.getByRole("button", { name: "Remove duplicates" }));
    const dialog = screen.getByRole("dialog", { name: "Remove duplicates" });
    expect(within(dialog).getByRole("checkbox", { name: "Column A" })).toBeChecked();
    expect(within(dialog).getByRole("checkbox", { name: "Column B" })).toBeChecked();
    await user.click(within(dialog).getByRole("checkbox", { name: "My data has headers" }));
    // With headers the columns are listed by their header text.
    await user.click(within(dialog).getByRole("checkbox", { name: "City" }));
    await user.click(within(dialog).getByRole("button", { name: "Remove duplicates" }));

    let sheet = workbookOf().sheets[0];
    // "ada" and the second "Ada" repeat row 2 (case-insensitively); Alan moves up.
    expect(["A1", "A2", "A3", "B3"].map((address) => cellText(sheet.cells[address]))).toEqual([
      "Name",
      "Ada",
      "Alan",
      "Paris",
    ]);
    expect(sheet.cells.A4).toBeUndefined();
    expect(sheet.cells.B5).toBeUndefined();
    // Cells outside the selected range are untouched.
    expect(cellText(sheet.cells.C3)).toBe("outside");
    expect(cellText(sheet.cells.A7)).toBe("below");
    expect(useToasts.getState().toasts.at(-1)?.title).toBe("Duplicate rows removed: 2. Unique rows remaining: 2.");

    await user.click(screen.getByRole("button", { name: "Home" }));
    await user.click(screen.getByRole("button", { name: "Undo" }));
    sheet = workbookOf().sheets[0];
    expect(cellText(sheet.cells.A3)).toBe("ada");
    expect(cellText(sheet.cells.B5)).toBe("Rome");
  });

  it("compares whole rows without headers and reports when nothing repeats", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "x", B1: "1", A2: "X", B2: "1", A3: "x", B3: "2" });
    render(<Harness id={id} />);

    selectRange("A1:B3");
    await user.click(screen.getByRole("button", { name: "Data" }));
    await user.click(screen.getByRole("button", { name: "Remove duplicates" }));
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Remove duplicates" }));

    let sheet = workbookOf().sheets[0];
    expect(["A1", "B1", "A2", "B2"].map((address) => cellText(sheet.cells[address]))).toEqual(["x", "1", "x", "2"]);
    expect(sheet.cells.A3).toBeUndefined();

    await user.click(screen.getByRole("button", { name: "Remove duplicates" }));
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Remove duplicates" }));
    expect(useToasts.getState().toasts.at(-1)?.title).toBe("No duplicate rows found.");
    sheet = workbookOf().sheets[0];
    expect(cellText(sheet.cells.A2)).toBe("x");
  });
});

describe("Calc list validation dropdown", () => {
  const withList = (values: string[]) => (sheet: Sheet) => ({
    ...sheet,
    validations: [
      { id: "v1", range: "A1:A3", kind: "list", values, min: null, max: null, message: "", allowBlank: true },
    ],
  });

  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
    vi.mocked(isAndroid).mockReturnValue(false);
  });

  it("offers the list values on the active cell and writes the chosen one", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A5: "elsewhere" }, withList(["Open", "In progress", "Done"]));
    render(<Harness id={id} />);

    const arrow = screen.getByRole("button", { name: "Show list values" });
    expect(arrow).toHaveAttribute("aria-expanded", "false");
    await user.click(arrow);
    const list = screen.getByRole("listbox", { name: "List values" });
    expect(
      within(list)
        .getAllByRole("option")
        .map((option) => option.textContent),
    ).toEqual(["Open", "In progress", "Done"]);
    await user.click(within(list).getByRole("option", { name: "Done" }));

    expect(screen.queryByRole("listbox")).toBeNull();
    expect(cellText(workbookOf().sheets[0].cells.A1)).toBe("Done");
    // Cells without a list validation get no arrow.
    await user.click(cellAt(4, 0));
    expect(screen.queryByRole("button", { name: "Show list values" })).toBeNull();
  });

  it("opens with Alt+Down, moves with the arrows, chooses with Enter and closes with Escape", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({}, withList(["Open", "In progress", "Done"]));
    render(<Harness id={id} />);

    await user.click(cellAt(1, 0));
    await user.keyboard("{Alt>}{ArrowDown}{/Alt}");
    const list = screen.getByRole("listbox");
    expect(document.activeElement).toBe(list);
    expect(within(list).getByRole("option", { name: "Open" })).toHaveAttribute("aria-selected", "true");
    await user.keyboard("{ArrowDown}{ArrowDown}{ArrowDown}");
    expect(within(list).getByRole("option", { name: "Done" })).toHaveAttribute("aria-selected", "true");
    await user.keyboard("{ArrowUp}{Enter}");

    expect(screen.queryByRole("listbox")).toBeNull();
    expect(cellText(workbookOf().sheets[0].cells.A2)).toBe("In progress");
    // Keyboard focus is back on the grid, which still navigates.
    expect(document.activeElement).toBe(document.querySelector(".calc-grid"));

    await user.keyboard("{Alt>}{ArrowDown}{/Alt}");
    // Reopening starts on the cell's current value.
    expect(screen.getByRole("option", { name: "In progress" })).toHaveAttribute("aria-selected", "true");
    await user.keyboard("{ArrowDown}{Escape}");
    expect(screen.queryByRole("listbox")).toBeNull();
    expect(cellText(workbookOf().sheets[0].cells.A2)).toBe("In progress");
    expect(document.activeElement).toBe(document.querySelector(".calc-grid"));
  });

  it("works with taps on Android and reads a list from a cell range", () => {
    vi.mocked(isAndroid).mockReturnValue(true);
    const id = seedWorkbook({ D1: "Yes", D2: "No", A3: "Maybe" }, withList(["=$D$1:$D$2"]));
    render(<Harness id={id} />);
    expect(document.querySelector(".calc-editor")?.classList.contains("is-android")).toBe(true);
    // A value outside the referenced list is flagged, like an inline list.
    expect(cellAt(2, 0).classList.contains("is-invalid")).toBe(true);

    fireEvent.pointerDown(cellAt(1, 0), { pointerId: 3, pointerType: "touch", button: 0, clientX: 80, clientY: 40 });
    fireEvent.pointerUp(window, { pointerId: 3, pointerType: "touch" });
    expect(document.querySelector<HTMLInputElement>(".name-box")?.value).toBe("A2");

    const arrow = screen.getByRole("button", { name: "Show list values" });
    fireEvent.pointerDown(arrow, { pointerId: 4, pointerType: "touch", button: 0 });
    fireEvent.pointerUp(arrow, { pointerId: 4, pointerType: "touch" });
    fireEvent.click(arrow);
    // The tap on the arrow must not have started a grid gesture.
    expect(document.querySelector<HTMLInputElement>(".name-box")?.value).toBe("A2");
    const list = screen.getByRole("listbox");
    expect(
      within(list)
        .getAllByRole("option")
        .map((option) => option.textContent),
    ).toEqual(["Yes", "No"]);
    fireEvent.click(within(list).getByRole("option", { name: "No" }));

    expect(cellText(workbookOf().sheets[0].cells.A2)).toBe("No");
    expect(cellAt(1, 0).classList.contains("is-invalid")).toBe(false);
  });
});

describe("Calc data validation", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
    vi.mocked(isAndroid).mockReturnValue(false);
  });

  it("flags an invalid value far down a long validated range", () => {
    // Regression: the grid only looked at the first 100 addresses of each rule,
    // so A151 inside A1:A500 was never validated.
    const id = seedWorkbook({ A151: "99", A150: "5" }, (sheet) => ({
      ...sheet,
      rowCount: 500,
      validations: [
        { id: "v1", range: "A1:A500", kind: "number", values: [], min: 1, max: 10, message: "", allowBlank: true },
      ],
    }));
    render(<Harness id={id} />);
    const grid = document.querySelector<HTMLElement>(".calc-grid")!;
    fireEvent.scroll(grid, { target: { scrollTop: 24 * 148 } });

    expect(cellAt(150, 0).classList.contains("is-invalid")).toBe(true);
    expect(cellAt(149, 0).classList.contains("is-invalid")).toBe(false);
  });
});

describe("Calc row geometry", () => {
  const withHeights = (rowHeights: Record<string, number>) => (sheet: Sheet) => ({ ...sheet, rowHeights });
  const px = (element: HTMLElement, property: "top" | "height" | "left") => element.style[property];
  const rowHeader = (row: number) => document.querySelector<HTMLElement>(`[data-row-header="${row}"]`);

  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
    vi.mocked(isAndroid).mockReturnValue(false);
  });

  it("places each row after the real height of the rows above it", () => {
    // Regression: rows sat at row * 24, so a 48px row overlapped the next one.
    const id = seedWorkbook({}, withHeights({ "1": 48 }));
    render(<Harness id={id} />);

    expect(px(cellAt(1, 0), "top")).toBe("24px");
    expect(px(cellAt(1, 0), "height")).toBe("48px");
    expect(px(cellAt(2, 0), "top")).toBe("72px");
    expect(px(cellAt(3, 0), "top")).toBe("96px");
    expect(px(rowHeader(2)!, "top")).toBe("72px");
    expect(px(rowHeader(1)!, "height")).toBe("48px");
  });

  it("leaves no gap where a filter hid a row", () => {
    // Regression: a hidden row (height 0) still advanced the next row by 24px.
    const id = seedWorkbook({}, withHeights({ "1": 0, "2": 0 }));
    render(<Harness id={id} />);

    expect(document.querySelector('[data-cell="1:0"]')).toBeNull();
    expect(document.querySelector('[data-cell="2:0"]')).toBeNull();
    expect(rowHeader(1)).toBeNull();
    expect(px(cellAt(3, 0), "top")).toBe("24px");
    expect(px(rowHeader(3)!, "top")).toBe("24px");
  });

  it("sizes the canvas to the rows' combined height", () => {
    const id = seedWorkbook({}, withHeights({ "1": 48, "5": 0, "6": 0 }));
    render(<Harness id={id} />);
    const rowCount = workbookOf().sheets[0].rowCount;

    // One taller row adds 24px; the two hidden rows take away 48px.
    const rowsHeight = rowCount * 24 + 24 - 48;
    expect(document.querySelector<HTMLElement>(".calc-canvas")!.style.height).toBe(`${24 + rowsHeight}px`);
    expect(document.querySelector<HTMLElement>(".calc-cells")!.style.height).toBe(`${rowsHeight}px`);
  });

  it("puts the fill handle on the bottom edge of the selection's last row", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({}, withHeights({ "1": 48 }));
    render(<Harness id={id} />);

    await user.click(cellAt(2, 0));
    // Header 24 + rows 0..1 (24 + 48) + row 2 (24), minus half the 10px handle.
    expect(document.querySelector<HTMLElement>("[data-fill-handle]")!.style.top).toBe(`${24 + 72 + 24 - 5}px`);
    expect(document.querySelector<HTMLElement>('[data-select-handle="start"]')!.style.top).toBe(`${24 + 72}px`);
  });

  it("scrolls the keyboard selection into view using the real row positions", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({}, withHeights({ "1": 100 }));
    render(<Harness id={id} />);
    const grid = document.querySelector<HTMLElement>(".calc-grid")!;
    Object.defineProperty(grid, "clientHeight", { configurable: true, value: 200 });
    Object.defineProperty(grid, "clientWidth", { configurable: true, value: 600 });

    await user.click(cellAt(0, 0));
    await user.keyboard("{ArrowDown>7/}");
    expect(document.querySelector<HTMLInputElement>(".name-box")?.value).toBe("A8");
    // Row 7 spans y 244..268 (one 100px row above it); with the 24px header its
    // bottom is at 292 on a 200px viewport.
    expect(grid.scrollTop).toBe(92);
    await user.keyboard("{ArrowUp>7/}");
    expect(document.querySelector<HTMLInputElement>(".name-box")?.value).toBe("A1");
    expect(grid.scrollTop).toBe(0);
  });

  it("steps over hidden rows with the arrow keys", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({}, withHeights({ "1": 0, "2": 0 }));
    render(<Harness id={id} />);

    await user.click(cellAt(0, 0));
    await user.keyboard("{ArrowDown}");
    expect(document.querySelector<HTMLInputElement>(".name-box")?.value).toBe("A4");
    await user.keyboard("{ArrowUp}");
    expect(document.querySelector<HTMLInputElement>(".name-box")?.value).toBe("A1");
  });
});
