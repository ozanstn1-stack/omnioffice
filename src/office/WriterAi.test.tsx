import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

/**
 * Writer AI actions: the consent step gates the request, the suggestion is
 * previewed before anything changes, and Accept is one undoable step that keeps
 * the paragraph's formatting.
 */
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null), save: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-fs", () => ({ readFile: vi.fn(async () => new Uint8Array()) }));

import { invoke } from "@tauri-apps/api/core";
import { WriterEditor } from "./WriterEditor";
import { resetAiConsent } from "./ai/editor-ai";
import { useOfficeTabs, type OfficeTab } from "../lib/office-store";
import type { Block, Run, TextDocument } from "../lib/office-types";

function Harness({ id }: { id: string }) {
  const tab = useOfficeTabs((state) => state.tabs.find((candidate) => candidate.id === id));
  if (!tab) return null;
  return <WriterEditor tab={tab as OfficeTab & { model: TextDocument }} />;
}

const settingsView = (configured: boolean) => ({
  configured,
  keyStorage: configured ? "plain" : "none",
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
});

function mockBackend(options: { configured?: boolean; reply?: string } = {}) {
  const configured = options.configured ?? true;
  vi.mocked(invoke).mockImplementation((async (command: string) => {
    if (command === "ai_get_settings") return settingsView(configured);
    if (command === "ai_edit_text") {
      return { text: options.reply ?? "fast fox", slides: null, model: "deepseek-chat", elapsedMs: 3 };
    }
    return null;
  }) as never);
}

const editCalls = () => vi.mocked(invoke).mock.calls.filter(([command]) => command === "ai_edit_text");

function seed(runs: Array<Partial<Run>>): string {
  const id = useOfficeTabs.getState().create("writer", "Untitled");
  const model = useOfficeTabs.getState().tabs[0].model as TextDocument;
  const first = model.blocks.find((block) => block.type === "paragraph") as Extract<Block, { type: "paragraph" }>;
  const paragraph = {
    type: "paragraph" as const,
    props: { ...first.props },
    runs: runs.map((run) => ({ ...first.runs[0], ...run })),
  };
  useOfficeTabs.setState((state) => ({
    tabs: state.tabs.map((entry) =>
      entry.id === id ? { ...entry, model: { ...model, blocks: [paragraph], header: [] } } : entry,
    ),
  }));
  return id;
}

function documentOf(): TextDocument {
  return useOfficeTabs.getState().tabs[0].model as TextDocument;
}

function paragraphRuns(): Array<[string, boolean | undefined]> {
  const block = documentOf().blocks[0];
  return block.type === "paragraph" ? block.runs.map((run) => [run.text, run.bold]) : [];
}

/** Opens the page editable and selects `[from, to)` of its single text node. */
async function selectRange(user: ReturnType<typeof userEvent.setup>, from: number, to: number) {
  await user.click(document.querySelectorAll<HTMLElement>(".writer-fragment")[0]);
  const editable = document.querySelector<HTMLElement>('.writer-page-sheet .para[contenteditable="true"]');
  if (!editable) throw new Error("no editable opened");
  const text = editable.firstChild as Text;
  window.getSelection()?.setBaseAndExtent(text, from, text, to);
  return editable;
}

const aiButton = (name: string) => screen.getByRole("button", { name: new RegExp(`^${name}`) });

describe("Writer AI actions", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
    resetAiConsent();
    vi.mocked(invoke).mockReset();
  });

  it("shortens the selection, applies it as one undo step and restores on Undo", async () => {
    mockBackend();
    const user = userEvent.setup();
    render(<Harness id={seed([{ text: "The quick brown fox jumps over the lazy dog" }])} />);
    await selectRange(user, 4, 19);
    await waitFor(() => expect(aiButton("Shorten")).toBeEnabled());
    await user.click(aiButton("Shorten"));

    const dialog = await screen.findByRole("dialog", { name: "Shorten with AI" });
    // Consent first: the selected text is named, nothing was sent yet.
    expect(within(dialog).getByText(/the selected text \(15 characters\)/)).toBeInTheDocument();
    expect(within(dialog).getByText(/DeepSeek \(api\.deepseek\.com\)/)).toBeInTheDocument();
    expect(editCalls()).toHaveLength(0);
    await user.click(within(dialog).getByRole("button", { name: "Send and continue" }));

    expect(await within(dialog).findByTestId("ai-suggestion")).toHaveTextContent("fast fox");
    expect(within(dialog).getByTestId("ai-original")).toHaveTextContent("quick brown fox");
    expect(editCalls()).toHaveLength(1);
    expect(editCalls()[0][1]).toMatchObject({ request: { task: "shorten", text: "quick brown fox" } });
    // Nothing changes before Accept.
    expect(
      paragraphRuns()
        .map(([text]) => text)
        .join(""),
    ).toBe("The quick brown fox jumps over the lazy dog");

    await user.click(within(dialog).getByRole("button", { name: "Accept" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(
      paragraphRuns()
        .map(([text]) => text)
        .join(""),
    ).toBe("The fast fox jumps over the lazy dog");

    // One undo step restores the paragraph.
    await user.click(screen.getByRole("button", { name: "Undo" }));
    expect(
      paragraphRuns()
        .map(([text]) => text)
        .join(""),
    ).toBe("The quick brown fox jumps over the lazy dog");
    expect(screen.getByRole("button", { name: "Undo" })).toBeDisabled();
  });

  it("works on the whole paragraph without a selection and keeps the first run's formatting", async () => {
    mockBackend({ reply: "Hi world" });
    const user = userEvent.setup();
    render(<Harness id={seed([{ text: "Hello ", bold: true }, { text: "big world" }])} />);
    await user.click(document.querySelectorAll<HTMLElement>(".writer-fragment")[0]);
    window.getSelection()?.collapseToStart();
    await user.click(aiButton("Rewrite"));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(/the text of the current paragraph \(15 characters\)/)).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Send and continue" }));
    await user.click(await within(dialog).findByRole("button", { name: "Accept" }));
    expect(paragraphRuns()).toEqual([["Hi world", true]]);
    expect(documentOf().blocks[0]).toMatchObject({ type: "paragraph" });
  });

  it("asks for consent once per document and provider, and cancel sends nothing", async () => {
    mockBackend();
    const user = userEvent.setup();
    render(<Harness id={seed([{ text: "Some text to polish" }])} />);
    await selectRange(user, 0, 9);
    await waitFor(() => expect(aiButton("Rewrite")).toBeEnabled());
    await user.click(aiButton("Rewrite"));
    await user.click(await screen.findByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(editCalls()).toHaveLength(0);

    await selectRange(user, 0, 9);
    await user.click(aiButton("Rewrite"));
    await user.click(await screen.findByRole("button", { name: "Send and continue" }));
    await screen.findByTestId("ai-suggestion");
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(editCalls()).toHaveLength(1);

    // Consent is remembered for this document: no second prompt.
    await selectRange(user, 0, 9);
    await user.click(aiButton("Shorten"));
    await screen.findByTestId("ai-suggestion");
    expect(screen.queryByRole("button", { name: "Send and continue" })).toBeNull();
    expect(editCalls()).toHaveLength(2);
  });

  it("offers a language picker for Translate that defaults to the UI language", async () => {
    mockBackend({ reply: "Merhaba" });
    const user = userEvent.setup();
    render(<Harness id={seed([{ text: "Hello" }])} />);
    await selectRange(user, 0, 5);
    await waitFor(() => expect(aiButton("Translate")).toBeEnabled());
    await user.click(aiButton("Translate"));
    await user.click(await screen.findByRole("button", { name: "Send and continue" }));
    const picker = await screen.findByLabelText("Translate into");
    expect(picker).toHaveValue("en");
    await user.selectOptions(picker, "tr");
    await user.click(screen.getByRole("button", { name: "Run" }));
    await screen.findByTestId("ai-suggestion");
    expect(editCalls()[0][1]).toMatchObject({ request: { task: "translate", options: { language: "Turkish" } } });
  });

  it("disables the actions with a hint while AI is not configured", async () => {
    mockBackend({ configured: false });
    const user = userEvent.setup();
    render(<Harness id={seed([{ text: "Some text" }])} />);
    await selectRange(user, 0, 4);
    const button = aiButton("Rewrite");
    await waitFor(() =>
      expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "ai_get_settings")).toBe(true),
    );
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute("title", expect.stringContaining("Set up AI in Settings"));
    expect(aiButton("Translate")).toBeDisabled();
  });
});
