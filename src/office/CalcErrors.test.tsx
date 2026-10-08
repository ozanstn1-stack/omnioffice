import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null), save: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-fs", () => ({ readFile: vi.fn(async () => new Uint8Array()) }));

import { useOfficeTabs } from "../lib/office-store";
import { cellAt, Harness, seedWorkbook } from "./calc/ui/testing";

describe("Calc error cells", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
  });

  it("marks a #CALC! cell as an error with its explanation, like the other error values", async () => {
    const id = seedWorkbook({ A1: "=TAKE({1,2},0)", A2: "=1/0", A3: "7" });
    render(<Harness id={id} />);
    await act(async () => undefined);

    const calc = cellAt(0, 0);
    expect(calc).toHaveTextContent("#CALC!");
    expect(calc).toHaveClass("is-error");
    expect(calc).toHaveAttribute("title", "#CALC!: The array is empty.");
    expect(calc.querySelector(".cell-error-flag")).not.toBeNull();

    expect(cellAt(1, 0)).toHaveClass("is-error");
    expect(cellAt(1, 0)).toHaveAttribute("title", "#DIV/0!: The formula divides by zero.");

    expect(cellAt(2, 0)).not.toHaveClass("is-error");
    expect(cellAt(2, 0).querySelector(".cell-error-flag")).toBeNull();
  });

  it("explains the selected error under the ribbon", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "=TAKE({1,2},0)", A2: "5" });
    render(<Harness id={id} />);
    await act(async () => undefined);

    await user.click(cellAt(1, 0));
    expect(document.querySelector(".calc-audit-banner")).toBeNull();
    await user.click(cellAt(0, 0));
    expect(
      screen.getByText("#CALC!: The array is empty.", { selector: ".calc-audit-banner span" }),
    ).toBeInTheDocument();
  });
});
