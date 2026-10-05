import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

/**
 * Smoke tests for the local productivity tools that are mounted by the app
 * shell but had no render coverage. The stores persist through `store_load` /
 * `store_save`; the mock keeps them empty so the default state is exercised.
 */
const invoke = vi.fn(async (command: string) => {
  switch (command) {
    case "store_load":
      return null;
    case "store_save":
    case "store_clear":
      return null;
    case "load_settings":
      return { theme: "dark", language: "en" };
    case "load_recent":
      return [];
    case "app_info":
      return { appVersion: "3.5.0", coreVersion: "3.5.0", platform: "windows" };
    default:
      return null;
  }
});

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string])) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/api/path", () => ({
  documentDir: vi.fn(async () => "C:/docs"),
  appDataDir: vi.fn(async () => "C:/appdata"),
  join: vi.fn(async (...parts: string[]) => parts.join("/")),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(async () => null),
  save: vi.fn(async () => "C:/docs/out.pdf"),
}));

import {
  NotesScreen,
  PlannerScreen,
  DataScreen,
  DrawScreen,
  TemplatesScreen,
  ConverterScreen,
  CleanerScreen,
  PdfFormsScreen,
} from "./ToolsScreens";
import { TEMPLATES } from "./templates";
import { useOfficeTabs } from "../lib/office-store";

describe("local productivity tools render and act on their stores", () => {
  beforeEach(() => {
    invoke.mockClear();
    useOfficeTabs.setState({ tabs: [], activeId: null });
  });

  it("creates and edits a note", async () => {
    const user = userEvent.setup();
    render(<NotesScreen />);
    expect(await screen.findByRole("heading", { name: "Notes" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /new note/i }));
    const title = await screen.findByPlaceholderText(/write/i);
    expect(title).toBeInTheDocument();
    // Loading went through the persisted store, not a hard-coded default.
    expect(invoke.mock.calls.some(([name]) => name === "store_load")).toBe(true);
  });

  it("adds a task to the planner", async () => {
    const user = userEvent.setup();
    render(<PlannerScreen />);
    expect(await screen.findByRole("heading", { name: "Planner" })).toBeInTheDocument();
    const input = screen.getByPlaceholderText("Add a task");
    await user.type(input, "Write the release notes{Enter}");
    // The planner can show the task in more than one panel (day cell + agenda),
    // so require at least one rendered occurrence rather than an exact match.
    expect((await screen.findAllByText("Write the release notes")).length).toBeGreaterThan(0);
  });

  it("renders the data grid and adds rows and columns", async () => {
    const user = userEvent.setup();
    const promptSpy = vi.spyOn(window, "prompt").mockReturnValue("Notes");
    try {
      render(<DataScreen />);
      expect(await screen.findByRole("heading", { name: "Data" })).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: /new table/i }));
      await user.click(screen.getByRole("button", { name: /^Row$/ }));
      await user.click(screen.getByRole("button", { name: /^Column$/ }));
      expect(screen.getByDisplayValue("Notes")).toBeInTheDocument();
      expect(screen.getAllByRole("cell").length).toBeGreaterThan(0);
    } finally {
      promptSpy.mockRestore();
    }
  });

  it("renders the drawing surface with its tools", async () => {
    const user = userEvent.setup();
    render(<DrawScreen />);
    expect(await screen.findByRole("heading", { name: "Draw" })).toBeInTheDocument();
    await user.click(screen.getAllByRole("button", { name: /new drawing/i })[0]);
    expect(document.querySelector(".draw-canvas")).not.toBeNull();
  });

  it("lists templates and creates a tab from one", async () => {
    const user = userEvent.setup();
    const onOpen = vi.fn();
    render(<TemplatesScreen onOpen={onOpen} />);
    expect(await screen.findByRole("heading", { name: "Templates" })).toBeInTheDocument();
    // Every built-in template is offered.
    const cards = screen.getAllByRole("button").filter((button) => button.className.includes("template-card"));
    expect(cards.length).toBe(TEMPLATES.length);

    await user.click(screen.getByRole("button", { name: "Documents" }));
    const writerCards = screen.getAllByRole("button").filter((button) => button.className.includes("template-card"));
    expect(writerCards.length).toBe(TEMPLATES.filter((template) => template.kind === "writer").length);

    await user.click(writerCards[0]);
    await waitFor(() => expect(useOfficeTabs.getState().tabs.length).toBe(1));
    // The workspace must be opened too: without it the click looked dead.
    expect(onOpen).toHaveBeenCalledTimes(1);
  });

  it("renders the converter, cleaner and PDF forms screens", async () => {
    const { unmount } = render(<ConverterScreen />);
    expect(await screen.findByRole("heading", { name: "Universal converter" })).toBeInTheDocument();
    unmount();

    const cleaner = render(<CleanerScreen />);
    expect(await screen.findByRole("heading", { name: "Document cleaner" })).toBeInTheDocument();
    cleaner.unmount();

    render(<PdfFormsScreen />);
    expect(await screen.findByRole("heading", { name: "PDF forms" })).toBeInTheDocument();
  });
});
