import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

// A picked document arrives as a token (no extension): the screen must decide
// PDF or office by the token's name, and never fall back to a raw-path dialog (#337).
const invoke = vi.fn(async (command: string, _payload?: unknown) => {
  switch (command) {
    case "file_token_pick":
      return [{ token: "tok-health", name: "dilekce.pdf" }];
    case "inspect_document":
      return {
        pageCount: 1,
        fonts: [],
        language: "tr",
        titleOverride: "Dilekçe",
        subjectOverride: "",
        authorOverride: "",
        producer: "",
        creator: "",
      };
    case "pdf_verify_signatures":
      return { signatures: [], warnings: [] };
    case "page_text":
      return "";
    default:
      return null;
  }
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string, unknown])) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
const { dialogOpen } = vi.hoisted(() => ({ dialogOpen: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: dialogOpen, save: vi.fn(async () => null) }));

import { DocumentHealthScreen } from "./DocumentHealthScreen";

describe("DocumentHealthScreen with file tokens", () => {
  it("picks through the Rust dialog and inspects a PDF token as a PDF", async () => {
    const user = userEvent.setup();
    render(<DocumentHealthScreen />);
    await user.click(screen.getByRole("button", { name: /Belge Seç/ }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("inspect_document", expect.anything()));
    expect(invoke).toHaveBeenCalledWith("file_token_pick", expect.anything());
    expect(dialogOpen).not.toHaveBeenCalled();
    // A PDF is not opened through the office reader.
    expect(invoke.mock.calls.some(([command]) => command === "office_open_document")).toBe(false);
    expect(await screen.findByRole("button", { name: /Belge: dilekce\.pdf/ })).toBeInTheDocument();
  });
});
