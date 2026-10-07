import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

/**
 * Calc AI actions: "Suggest formula" only writes the active cell on Accept
 * (one undo step), "Summarize column" offers Copy and Insert below, and both
 * wait for the consent step and for a configured provider.
 */
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null), save: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-fs", () => ({ readFile: vi.fn(async () => new Uint8Array()) }));

import { invoke } from "@tauri-apps/api/core";
import { CalcEditor } from "./CalcEditor";
import { resetAiConsent } from "./ai/editor-ai";
import { applyCellEdit } from "./calc/cells";
import { useOfficeTabs, type OfficeTab } from "../lib/office-store";
import { cellText, type Workbook } from "../lib/office-types";
import { useToasts } from "../lib/store";

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

function Harness({ id }: { id: string }) {
  const tab = useOfficeTabs((state) => state.tabs.find((candidate) => candidate.id === id));
  if (!tab) return null;
  return <CalcEditor tab={tab as OfficeTab & { model: Workbook }} />;
}

function seedWorkbook(values: Record<string, string>): string {
  const id = useOfficeTabs.getState().create("calc", "Untitled");
  let model = useOfficeTabs.getState().tabs[0].model as Workbook;
  for (const [address, value] of Object.entries(values)) {
    model = applyCellEdit(model, 0, Number(address.slice(1)) - 1, address.charCodeAt(0) - 65, value);
  }
  useOfficeTabs.setState((state) => ({ tabs: state.tabs.map((tab) => (tab.id === id ? { ...tab, model } : tab)) }));
  return id;
}

const workbookOf = () => useOfficeTabs.getState().tabs[0].model as Workbook;

function selectRange(range: string) {
  fireEvent.change(document.querySelector<HTMLInputElement>(".name-box")!, { target: { value: range } });
}

function mockBackend(options: { configured?: boolean; reply?: string } = {}) {
  const configured = options.configured ?? true;
  vi.mocked(invoke).mockImplementation((async (command: string) => {
    if (command === "ai_get_settings") {
      return {
        configured,
        keyStorage: "plain",
        maskedKey: "",
        baseUrl: "https://api.deepseek.com",
        model: "deepseek-chat",
        temperature: 0.3,
        maxTokens: 1000,
        thinking: false,
        reasoningEffort: "high",
        contextTokens: 1000,
        maxOutputTokens: 1000,
        provider: "deepseek",
        providerLabel: "DeepSeek",
      };
    }
    if (command === "ai_edit_text") return { text: options.reply ?? "", slides: null, model: "m", elapsedMs: 1 };
    return null;
  }) as never);
}

const editCalls = () => vi.mocked(invoke).mock.calls.filter(([command]) => command === "ai_edit_text");

async function openAiTab(user: ReturnType<typeof userEvent.setup>, action: string) {
  await user.click(screen.getByRole("button", { name: "Data" }));
  const button = screen.getByRole("button", { name: action });
  await waitFor(() => expect(button).toBeEnabled());
  await user.click(button);
}

describe("Calc AI actions", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
    useToasts.setState({ toasts: [] });
    resetAiConsent();
    vi.mocked(invoke).mockReset();
  });

  it("inserts a suggested formula into the active cell only on Accept, as one undo step", async () => {
    mockBackend({ reply: "=SUM(B2:B3)" });
    const user = userEvent.setup();
    render(<Harness id={seedWorkbook({ A1: "Item", B1: "Amount", A2: "x", B2: "10", A3: "y", B3: "20" })} />);
    selectRange("B4");
    await openAiTab(user, "Suggest formula");

    const dialog = await screen.findByRole("dialog", { name: "Suggest a formula with AI" });
    expect(within(dialog).getByText(/column headers of this sheet/)).toBeInTheDocument();
    expect(editCalls()).toHaveLength(0);
    await user.click(within(dialog).getByRole("button", { name: "Send and continue" }));
    expect(within(dialog).getByRole("button", { name: "Run" })).toBeDisabled();
    await user.type(within(dialog).getByLabelText("What should the formula calculate?"), "total of Amount");
    await user.click(within(dialog).getByRole("button", { name: "Run" }));

    expect(await within(dialog).findByTestId("ai-suggestion")).toHaveTextContent("=SUM(B2:B3)");
    expect(editCalls()).toHaveLength(1);
    const request = (editCalls()[0][1] as { request: { task: string; text: string; options: { context: string } } })
      .request;
    expect(request.task).toBe("suggest_formula");
    expect(request.text).toBe("total of Amount");
    expect(request.options.context).toContain("A: Item; B: Amount");
    expect(request.options.context).toContain("Active cell: B4");
    // Nothing is written before Accept.
    expect(workbookOf().sheets[0].cells.B4).toBeUndefined();

    await user.click(within(dialog).getByRole("button", { name: "Insert in B4" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(workbookOf().sheets[0].cells.B4?.formula).toBe("=SUM(B2:B3)");

    await user.click(screen.getByRole("button", { name: "Home" }));
    await user.click(screen.getByRole("button", { name: "Undo" }));
    expect(workbookOf().sheets[0].cells.B4?.formula).toBeUndefined();
    expect(workbookOf().sheets[0].cells.B3 && cellText(workbookOf().sheets[0].cells.B3)).toBe("20");
  });

  it("leaves the sheet untouched when the formula suggestion is cancelled", async () => {
    mockBackend({ reply: "=A1" });
    const user = userEvent.setup();
    render(<Harness id={seedWorkbook({ A1: "1" })} />);
    selectRange("B2");
    await openAiTab(user, "Suggest formula");
    await user.click(await screen.findByRole("button", { name: "Send and continue" }));
    await user.type(screen.getByLabelText("What should the formula calculate?"), "copy A1");
    await user.click(screen.getByRole("button", { name: "Run" }));
    await screen.findByTestId("ai-suggestion");
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(workbookOf().sheets[0].cells.B2).toBeUndefined();
  });

  it("summarizes a column, copies it and inserts it below the data", async () => {
    mockBackend({ reply: "Three fruit names." });
    const user = userEvent.setup();
    render(<Harness id={seedWorkbook({ A1: "Fruit", A2: "apple", A3: "pear", B1: "other" })} />);
    selectRange("A1");
    await openAiTab(user, "Summarize column");

    const dialog = await screen.findByRole("dialog", { name: "Summarize column with AI" });
    expect(within(dialog).getByText(/the 3 values of column A/)).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Send and continue" }));
    expect(await within(dialog).findByTestId("ai-suggestion")).toHaveTextContent("Three fruit names.");
    expect(editCalls()[0][1]).toMatchObject({
      request: { task: "summarize_column", text: "Fruit\napple\npear" },
    });

    await user.click(within(dialog).getByRole("button", { name: "Copy" }));
    await waitFor(async () => expect(await navigator.clipboard.readText()).toBe("Three fruit names."));
    expect(screen.getByRole("dialog")).toBeInTheDocument();

    await user.click(within(dialog).getByRole("button", { name: "Insert below" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(cellText(workbookOf().sheets[0].cells.A4)).toBe("Three fruit names.");
    expect(workbookOf().sheets[0].cells.B4).toBeUndefined();
  });

  it("asks again for another kind of data and names what is sent, to whom, and the consented target", async () => {
    mockBackend({ reply: "=A1" });
    const user = userEvent.setup();
    render(<Harness id={seedWorkbook({ A1: "Fruit", A2: "apple", A3: "pear" })} />);
    selectRange("A1");
    await openAiTab(user, "Summarize column");
    await user.click(await screen.findByRole("button", { name: "Send and continue" }));
    await screen.findByTestId("ai-suggestion");
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(editCalls()).toHaveLength(1);
    expect(editCalls()[0][1]).toMatchObject({
      request: { expectedProvider: "deepseek", expectedHost: "api.deepseek.com" },
    });

    // Consent to column values is not consent to headers + a free-text request.
    selectRange("B2");
    await openAiTab(user, "Suggest formula");
    const dialog = await screen.findByRole("dialog", { name: "Suggest a formula with AI" });
    expect(within(dialog).getByRole("button", { name: "Send and continue" })).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Send and continue" }));
    await user.type(within(dialog).getByLabelText("What should the formula calculate?"), "copy A1");
    expect(within(dialog).getByTestId("ai-will-send")).toHaveTextContent(
      /Will send: your request and the column headers, ~\d+ characters, to DeepSeek \(api\.deepseek\.com\)\./,
    );
    await user.click(within(dialog).getByRole("button", { name: "Run" }));
    await within(dialog).findByTestId("ai-suggestion");
    expect(editCalls()).toHaveLength(2);
    expect(editCalls()[1][1]).toMatchObject({
      request: { task: "suggest_formula", expectedProvider: "deepseek", expectedHost: "api.deepseek.com" },
    });
  });

  it("disables both actions with a hint while AI is not configured", async () => {
    mockBackend({ configured: false });
    const user = userEvent.setup();
    render(<Harness id={seedWorkbook({ A1: "1" })} />);
    await user.click(screen.getByRole("button", { name: "Data" }));
    await waitFor(() =>
      expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "ai_get_settings")).toBe(true),
    );
    for (const name of ["Summarize column", "Suggest formula"]) {
      const button = screen.getByRole("button", { name: new RegExp(`^${name}`) });
      expect(button).toBeDisabled();
      expect(button).toHaveAttribute("title", expect.stringContaining("Set up AI in Settings"));
    }
  });
});
