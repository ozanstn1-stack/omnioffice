import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null), save: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-fs", () => ({ readFile: vi.fn(async () => new Uint8Array()) }));

import { useOfficeTabs } from "../lib/office-store";
import type { CondRule } from "../lib/office-types";
import { cellAt, Harness, seedWorkbook, selectRange, workbookOf } from "./calc/ui/testing";

const DATA = { A1: "10", A2: "50", A3: "100", B1: "x", B2: "y", B3: "x" };

type User = ReturnType<typeof userEvent.setup>;

async function openDialog(user: User) {
  await user.click(screen.getByRole("button", { name: "Formulas" }));
  await user.click(screen.getByRole("button", { name: "Conditional formatting" }));
  return screen.getByRole("dialog", { name: "Conditional formatting" });
}

const rules = (): CondRule[] => workbookOf().sheets[0].conditional;

function setColor(input: HTMLElement, value: string) {
  fireEvent.change(input, { target: { value } });
}

describe("Calc conditional formatting dialog", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
  });

  it("starts on the selected block, or on everything in use for a single cell", async () => {
    const user = userEvent.setup();
    render(<Harness id={seedWorkbook(DATA)} />);
    let dialog = await openDialog(user);
    expect(within(dialog).getByRole("textbox", { name: "Applies to" })).toHaveValue("A1:B3");
    await user.click(within(dialog).getByRole("button", { name: "Close" }));

    selectRange("A2:A3");
    dialog = await openDialog(user);
    expect(within(dialog).getByRole("textbox", { name: "Applies to" })).toHaveValue("A2:A3");
  });

  it("refuses a bad range and a bad value and says why", async () => {
    const user = userEvent.setup();
    render(<Harness id={seedWorkbook(DATA)} />);
    const dialog = await openDialog(user);
    const range = within(dialog).getByRole("textbox", { name: "Applies to" });
    await user.clear(range);
    await user.type(range, "nonsense");
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));
    expect(within(dialog).getByRole("alert")).toHaveTextContent("Enter a range such as A1:D20.");

    await user.clear(range);
    await user.type(range, "A1:A3");
    const value = within(dialog).getByRole("textbox", { name: "Value" });
    await user.clear(value);
    await user.type(value, "abc");
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));
    expect(within(dialog).getByRole("alert")).toHaveTextContent("Enter a number.");
    expect(rules()).toHaveLength(0);
  });

  it("adds a colour scale and paints the cells from the lowest to the highest colour", async () => {
    const user = userEvent.setup();
    render(<Harness id={seedWorkbook(DATA)} />);
    const dialog = await openDialog(user);
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Rule" }), "colorScale");
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Number of colors" }), "2");
    expect(within(dialog).queryByText("Midpoint")).toBeNull();
    setColor(within(dialog).getByLabelText("Minimum: Color"), "#000000");
    setColor(within(dialog).getByLabelText("Maximum: Color"), "#ffffff");
    const range = within(dialog).getByRole("textbox", { name: "Applies to" });
    await user.clear(range);
    await user.type(range, "A1:A3");
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));

    expect(rules().at(-1)).toMatchObject({
      kind: "colorScale",
      range: "A1:A3",
      thresholds: [
        { kind: "min", value: "", color: "#000000" },
        { kind: "max", value: "", color: "#ffffff" },
      ],
    });
    expect(cellAt(0, 0).style.background).toBe("rgb(0, 0, 0)");
    expect(cellAt(1, 0).style.background).toBe("rgb(113, 113, 113)");
    expect(cellAt(2, 0).style.background).toBe("rgb(255, 255, 255)");
    // Outside the range nothing is painted.
    expect(cellAt(0, 1).style.background).toBe("");
  });

  it("uses a three-colour scale with a midpoint by default and lets the midpoint move", async () => {
    const user = userEvent.setup();
    render(<Harness id={seedWorkbook(DATA)} />);
    const dialog = await openDialog(user);
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Rule" }), "colorScale");
    await user.selectOptions(within(dialog).getByLabelText("Midpoint: Type"), "num");
    await user.clear(within(dialog).getByLabelText("Midpoint: Value"));
    await user.type(within(dialog).getByLabelText("Midpoint: Value"), "50");
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));
    expect(
      rules()
        .at(-1)
        ?.thresholds?.map((stop) => stop.kind),
    ).toEqual(["min", "num", "max"]);
    expect(rules().at(-1)?.thresholds?.[1].value).toBe("50");
  });

  it("adds a data bar that can hide the value", async () => {
    const user = userEvent.setup();
    render(<Harness id={seedWorkbook(DATA)} />);
    const dialog = await openDialog(user);
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Rule" }), "dataBar");
    await user.click(within(dialog).getByRole("checkbox", { name: "Show the value" }));
    setColor(within(dialog).getByLabelText("Bar color"), "#ff0000");
    const range = within(dialog).getByRole("textbox", { name: "Applies to" });
    await user.clear(range);
    await user.type(range, "A1:A3");
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));

    expect(rules().at(-1)).toMatchObject({ kind: "dataBar", fill: "#ff0000", hideValue: true });
    const bar = cellAt(2, 0).querySelector<HTMLElement>(".data-bar")!;
    expect(bar.style.width).toBe("100%");
    expect(bar.style.background).toBe("rgb(255, 0, 0)");
    expect(cellAt(2, 0).querySelector(".cell-text")).toHaveTextContent("");
    expect(cellAt(0, 0).querySelector<HTMLElement>(".data-bar")!.style.width).toBe("10%");
  });

  it("adds an icon set, reversed, with the value kept", async () => {
    const user = userEvent.setup();
    render(<Harness id={seedWorkbook(DATA)} />);
    const dialog = await openDialog(user);
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Rule" }), "iconSet");
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Icon set" }), "3Arrows");
    await user.click(within(dialog).getByRole("checkbox", { name: "Reverse icon order" }));
    const range = within(dialog).getByRole("textbox", { name: "Applies to" });
    await user.clear(range);
    await user.type(range, "A1:A3");
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));

    expect(rules().at(-1)).toMatchObject({ kind: "iconSet", iconSet: "3Arrows", reverseIcons: true });
    const icon = (row: number) => cellAt(row, 0).querySelector<HTMLElement>(".cf-icon");
    // 10 is the lowest tier and 100 the highest; reversed, they swap icons.
    expect(icon(0)).toHaveAttribute("data-tier", "2");
    expect(icon(1)).toHaveAttribute("data-tier", "1");
    expect(icon(2)).toHaveAttribute("data-tier", "0");
    expect(icon(0)).toHaveAccessibleName("Value tier 1 of 3");
    expect(cellAt(0, 0).querySelector(".cell-text")).toHaveTextContent("10");
    // Text cells next to it get no icon.
    expect(cellAt(0, 1).querySelector(".cf-icon")).toBeNull();
  });

  it("adds a custom formula rule with a font colour, bold and italic", async () => {
    const user = userEvent.setup();
    render(<Harness id={seedWorkbook(DATA)} />);
    const dialog = await openDialog(user);
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Rule" }), "expression");
    await user.type(within(dialog).getByRole("textbox", { name: "Formula" }), "=$A1>20");
    await user.click(within(dialog).getByRole("checkbox", { name: "Font color" }));
    setColor(within(dialog).getByLabelText("Font color", { selector: "input[type=color]" }), "#ff0000");
    await user.click(within(dialog).getByRole("checkbox", { name: "Bold" }));
    await user.click(within(dialog).getByRole("checkbox", { name: "Italic" }));
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));

    expect(rules().at(-1)).toMatchObject({
      kind: "expression",
      formula: "$A1>20",
      color: "#ff0000",
      bold: true,
      italic: true,
    });
    // Rows 2 and 3 satisfy $A>20, in both columns of the range.
    for (const [row, col, on] of [
      [0, 0, false],
      [1, 0, true],
      [2, 1, true],
      [0, 1, false],
    ] as const) {
      const cell = cellAt(row, col);
      expect(cell.style.fontWeight === "700", `${row}:${col} bold`).toBe(on);
      expect(cell.style.fontStyle === "italic", `${row}:${col} italic`).toBe(on);
      expect(cell.style.color === "rgb(255, 0, 0)", `${row}:${col} colour`).toBe(on);
    }
  });

  it("asks for a formula before it adds a custom rule", async () => {
    const user = userEvent.setup();
    render(<Harness id={seedWorkbook(DATA)} />);
    const dialog = await openDialog(user);
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Rule" }), "expression");
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));
    expect(within(dialog).getByRole("alert")).toHaveTextContent("Enter a formula.");
    expect(rules()).toHaveLength(0);
  });

  it("adds a bottom-N rule", async () => {
    const user = userEvent.setup();
    render(<Harness id={seedWorkbook(DATA)} />);
    const dialog = await openDialog(user);
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Rule" }), "bottom");
    const value = within(dialog).getByRole("textbox", { name: "Value" });
    await user.clear(value);
    await user.type(value, "1");
    const range = within(dialog).getByRole("textbox", { name: "Applies to" });
    await user.clear(range);
    await user.type(range, "A1:A3");
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));
    expect(rules().at(-1)).toMatchObject({ kind: "bottom", topN: 1 });
    expect(cellAt(0, 0).style.background).not.toBe("");
    expect(cellAt(1, 0).style.background).toBe("");
    expect(cellAt(2, 0).style.background).toBe("");
  });

  it("lists the rules and deletes one as a single undo step", async () => {
    const user = userEvent.setup();
    render(<Harness id={seedWorkbook(DATA)} />);
    let dialog = await openDialog(user);
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));
    expect(rules()).toHaveLength(1);

    dialog = await openDialog(user);
    const list = within(dialog).getByRole("list", { name: "Rules in this sheet" });
    expect(within(list).getByText("Greater than")).toBeInTheDocument();
    await user.click(within(list).getByRole("button", { name: /Delete rule: Greater than/ }));
    expect(rules()).toHaveLength(0);
    expect(cellAt(0, 0).style.background).toBe("");

    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    await user.click(screen.getByRole("button", { name: "Home" }));
    await user.click(screen.getByRole("button", { name: "Undo" }));
    expect(rules()).toHaveLength(1);
  });

  it("recomputes a rule when the data changes", async () => {
    const user = userEvent.setup();
    render(<Harness id={seedWorkbook({ A1: "1", A2: "2" })} />);
    const dialog = await openDialog(user);
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Rule" }), "dataBar");
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));
    expect(cellAt(0, 0).querySelector<HTMLElement>(".data-bar")!.style.width).toBe("50%");

    await user.click(cellAt(1, 0));
    await user.keyboard("4{Enter}");
    await act(async () => undefined);
    expect(cellAt(0, 0).querySelector<HTMLElement>(".data-bar")!.style.width).toBe("25%");
  });
});
