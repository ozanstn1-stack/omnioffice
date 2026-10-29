import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const REPORT = "OmniOffice diagnostics\nversion 6.5.1\n<path>/app.log";

let openFails = false;
const invoke = vi.fn(async (command: string, _payload?: unknown) => {
  switch (command) {
    case "diagnostics_report":
      return REPORT;
    case "app_info":
      return { appVersion: "6.5.1", coreVersion: "6.5.1", platform: "linux" };
    case "open_external_link":
      if (openFails) throw new Error("no browser");
      return null;
    default:
      return null;
  }
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string, unknown])) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null), save: vi.fn(async () => null) }));

import { useToasts } from "../lib/store";
import { ReportProblem } from "./report-problem";

/** `userEvent.setup()` installs its own clipboard stub, so the recorder goes in after it. */
function installClipboard() {
  const calls: string[] = [];
  Object.defineProperty(window.navigator, "clipboard", {
    value: {
      writeText: async (value: string) => {
        calls.push(value);
      },
    },
    configurable: true,
  });
  return calls;
}

function toastTitles(): string[] {
  return useToasts.getState().toasts.map((toast) => toast.title);
}

describe("ReportProblem (C15)", () => {
  beforeEach(() => {
    invoke.mockClear();
    openFails = false;
    useToasts.setState({ toasts: [] });
    installClipboard();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("shows the report for review before anything is copied", async () => {
    const user = userEvent.setup();
    render(<ReportProblem />);

    await user.click(screen.getByRole("button", { name: "Report a problem" }));
    const preview = await screen.findByLabelText("Diagnostics report");
    await waitFor(() => expect(preview).toHaveValue(REPORT));
    // Opening the dialog collects the report but copies nothing.
    expect(screen.getByRole("button", { name: "Copy report" })).toBeInTheDocument();
  });

  it("copies the report to the clipboard", async () => {
    const user = userEvent.setup();
    const clipboard = installClipboard();
    render(<ReportProblem />);

    await user.click(screen.getByRole("button", { name: "Report a problem" }));
    await waitFor(() => expect(screen.getByLabelText("Diagnostics report")).toHaveValue(REPORT));
    await user.click(screen.getByRole("button", { name: "Copy report" }));

    await waitFor(() => expect(clipboard).toEqual([REPORT]));
    expect(toastTitles()).toContain("Report copied to the clipboard.");
  });

  it("copies the report and opens the pre-filled GitHub issue", async () => {
    const user = userEvent.setup();
    render(<ReportProblem />);

    await user.click(screen.getByRole("button", { name: "Report a problem" }));
    await waitFor(() => expect(screen.getByLabelText("Diagnostics report")).toHaveValue(REPORT));
    await user.click(screen.getByRole("button", { name: "Copy and open GitHub issue" }));

    await waitFor(() => expect(toastTitles()).toContain("Report copied to the clipboard."));
    const call = invoke.mock.calls.find(([command]) => command === "open_external_link");
    expect(call).toBeTruthy();
    const url = new URL((call?.[1] as { url: string }).url);
    expect(url.host).toBe("github.com");
    expect(url.pathname).toBe("/ozanstn1-stack/omnioffice/issues/new");
    expect(url.searchParams.get("title")).toContain("v6.5.1");
    expect(url.searchParams.get("labels")).toBe("feedback");
    expect(url.searchParams.get("body")).toContain("Version: 6.5.1");
  });

  it("says so when the browser cannot be opened", async () => {
    openFails = true;
    const user = userEvent.setup();
    render(<ReportProblem />);

    await user.click(screen.getByRole("button", { name: "Report a problem" }));
    await waitFor(() => expect(screen.getByLabelText("Diagnostics report")).toHaveValue(REPORT));
    await user.click(screen.getByRole("button", { name: "Copy and open GitHub issue" }));

    await waitFor(() =>
      expect(toastTitles()).toContain(
        "Could not open the browser; open github.com/ozanstn1-stack/omnioffice/issues yourself.",
      ),
    );
  });
});
