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
// Event and drop button/clientX/pointerId. MouseEvent carries those fields and
// is the standard stand-in for pointer event tests.
if (typeof window.PointerEvent === "undefined") {
  window.PointerEvent = MouseEvent as unknown as typeof PointerEvent;
}
import { applyCellEdit } from "./calc/cells";
import { useOfficeTabs, type OfficeTab } from "../lib/office-store";
import { cellText, type Workbook } from "../lib/office-types";

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
