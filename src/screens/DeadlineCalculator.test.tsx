import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

let pages: string[] = [];
const invoke = vi.fn(async (command: string, payload?: unknown) => {
  if (command === "page_text") {
    const page = (payload as { page: number }).page;
    if (page > pages.length) throw new Error("out of range");
    return pages[page - 1];
  }
  return null;
});
const saveBytes = vi.fn(async (_bytes: Uint8Array, _name: string, _filter: unknown) => "/out/deadline.ics");

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invoke(...(args as [string, unknown])),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("../lib/mobile", async (original) => ({
  ...(await original<typeof import("../lib/mobile")>()),
  saveFileBytes: (bytes: Uint8Array, name: string, filter: unknown) => saveBytes(bytes, name, filter),
}));

import { DeadlineCalculatorScreen } from "./DeadlineCalculator";
import { usePlanner } from "../lib/office-store";
import { useSettings } from "../lib/store";
import { DEFAULT_SETTINGS } from "../lib/types";

describe("notification deadline screen", () => {
  beforeEach(() => {
    invoke.mockClear();
    saveBytes.mockClear();
    pages = [];
    usePlanner.setState({ tasks: [], loaded: true });
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS }, loaded: true });
  });

  it("computes the last day from an e-notice arrival and explains each step", () => {
    render(<DeadlineCalculatorScreen />);
    fireEvent.change(screen.getByLabelText(/Reached the electronic address on/), { target: { value: "2026-03-02" } });
    expect(screen.getByTestId("deadline-result")).toHaveTextContent("2026-03-23");
    expect(screen.getByText(/Served on 2026-03-07/)).toBeInTheDocument();
    expect(screen.getByText(/2026-03-21, 2026-03-22/)).toBeInTheDocument();
  });

  it("reads the arrival date from a notification PDF opened with the screen", async () => {
    pages = ["Elektronik Adrese Ulaşma Tarihi: 02.03.2026 14:31\nGönderim Tarihi: 01.03.2026"];
    render(<DeadlineCalculatorScreen initialFiles={["/docs/teblig.pdf"]} />);
    await waitFor(() => expect(screen.getByTestId("deadline-result")).toHaveTextContent("2026-03-23"));
    expect(screen.getByRole("button", { name: /2026-03-02 · arrival/ })).toBeInTheDocument();
  });

  it("puts the deadline in the planner and saves a calendar file", async () => {
    const user = userEvent.setup();
    render(<DeadlineCalculatorScreen />);
    fireEvent.change(screen.getByLabelText(/Reached the electronic address on/), { target: { value: "2026-03-02" } });
    await user.type(screen.getByLabelText(/Case or task/), "2026/123 E. istinaf");

    await user.click(screen.getByRole("button", { name: /Add to the planner/ }));
    expect(usePlanner.getState().tasks).toEqual([
      expect.objectContaining({ date: "2026-03-23", title: expect.stringContaining("2026/123 E. istinaf") }),
    ]);

    await user.click(screen.getByRole("button", { name: /Save as .ics/ }));
    await waitFor(() => expect(saveBytes).toHaveBeenCalled());
    const [bytes, name] = saveBytes.mock.calls[0];
    expect(name).toBe("2026-03-23.ics");
    expect(new TextDecoder().decode(bytes)).toContain("DTSTART;VALUE=DATE:20260323");
  });

  it("drops the five days for a paper service", async () => {
    const user = userEvent.setup();
    render(<DeadlineCalculatorScreen />);
    await user.click(screen.getByRole("checkbox", { name: /Electronic notification/ }));
    fireEvent.change(screen.getByLabelText(/Served on/), { target: { value: "2026-01-05" } });
    expect(screen.getByTestId("deadline-result")).toHaveTextContent("2026-01-19");
  });
});
