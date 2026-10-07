import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

/** Impress "Slides from outline": consent, validated preview, one undo step. */
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null), save: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-fs", () => ({ readFile: vi.fn(async () => new Uint8Array()) }));

import { invoke } from "@tauri-apps/api/core";
import { ImpressEditor, outlineSlides } from "./ImpressEditor";
import { resetAiConsent } from "./ai/editor-ai";
import { useOfficeTabs, type OfficeTab } from "../lib/office-store";
import { newDeck, type Deck } from "../lib/office-types";
import { useToasts } from "../lib/store";

if (typeof window.PointerEvent === "undefined") {
  window.PointerEvent = MouseEvent as unknown as typeof PointerEvent;
}

function Harness({ id }: { id: string }) {
  const tab = useOfficeTabs((state) => state.tabs.find((candidate) => candidate.id === id));
  if (!tab) return null;
  return <ImpressEditor tab={tab as OfficeTab & { model: Deck }} />;
}

const deckOf = () => useOfficeTabs.getState().tabs[0].model as Deck;
const slideTexts = (index: number) =>
  deckOf().slides[index].objects.map((object) => (object.text?.paragraphs ?? []).map((paragraph) => paragraph.text));

const SLIDES = [
  { title: "Welcome", bullets: ["Who we are", "Why now"] },
  { title: "Plan", bullets: [] },
];

function mockBackend(configured = true) {
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
    if (command === "ai_edit_text") return { text: JSON.stringify(SLIDES), slides: SLIDES, model: "m", elapsedMs: 1 };
    return null;
  }) as never);
}

const editCalls = () => vi.mocked(invoke).mock.calls.filter(([command]) => command === "ai_edit_text");

describe("outlineSlides", () => {
  it("builds title + bullet slides as plain text and tolerates an empty bullet list", () => {
    const deck = newDeck();
    const slides = outlineSlides(deck, [
      { title: "<b>Hi</b>", bullets: ["one", "two"] },
      { title: "Bare", bullets: [] },
    ]);
    expect(slides).toHaveLength(2);
    const texts = slides.map((slide) => slide.objects.map((object) => object.text?.paragraphs.map((p) => p.text)));
    expect(texts[0]).toEqual([["<b>Hi</b>"], ["one", "two"]]);
    expect(texts[1]).toEqual([["Bare"], [""]]);
    expect(slides[0].objects[1].text?.paragraphs.every((paragraph) => paragraph.bullet)).toBe(true);
    expect(new Set(slides.map((slide) => slide.id)).size).toBe(2);
  });
});

describe("Impress AI slides from outline", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
    useToasts.setState({ toasts: [] });
    resetAiConsent();
    vi.mocked(invoke).mockReset();
  });

  it("previews the validated slides and appends them on Accept as one undo step", async () => {
    mockBackend();
    const user = userEvent.setup();
    const id = useOfficeTabs.getState().create("impress", "Untitled");
    render(<Harness id={id} />);
    const before = deckOf().slides.length;

    await user.click(screen.getByRole("button", { name: "Insert" }));
    const button = screen.getByRole("button", { name: "Slides from outline" });
    await waitFor(() => expect(button).toBeEnabled());
    await user.click(button);

    const dialog = await screen.findByRole("dialog", { name: "Slides from an outline with AI" });
    expect(editCalls()).toHaveLength(0);
    await user.click(within(dialog).getByRole("button", { name: "Send and continue" }));
    expect(within(dialog).getByRole("button", { name: "Run" })).toBeDisabled();
    await user.type(within(dialog).getByLabelText(/^Outline/), "Welcome\nPlan");
    await user.click(within(dialog).getByRole("button", { name: "Run" }));

    const preview = await within(dialog).findByTestId("ai-slides");
    expect(preview).toHaveTextContent("Welcome");
    expect(preview).toHaveTextContent("Why now");
    expect(editCalls()[0][1]).toMatchObject({ request: { task: "outline_to_slides", text: "Welcome\nPlan" } });
    // Nothing is added before Accept.
    expect(deckOf().slides).toHaveLength(before);

    await user.click(within(dialog).getByRole("button", { name: "Add 2 slides" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(deckOf().slides).toHaveLength(before + 2);
    expect(slideTexts(before)).toEqual([["Welcome"], ["Who we are", "Why now"]]);
    expect(slideTexts(before + 1)[0]).toEqual(["Plan"]);

    await user.click(screen.getByRole("button", { name: "Home" }));
    await user.click(screen.getByRole("button", { name: "Undo" }));
    expect(deckOf().slides).toHaveLength(before);
  });

  it("disables the action with a hint while AI is not configured", async () => {
    mockBackend(false);
    const user = userEvent.setup();
    render(<Harness id={useOfficeTabs.getState().create("impress", "Untitled")} />);
    await user.click(screen.getByRole("button", { name: "Insert" }));
    await waitFor(() =>
      expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "ai_get_settings")).toBe(true),
    );
    const button = screen.getByRole("button", { name: /^Slides from outline/ });
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute("title", expect.stringContaining("Set up AI in Settings"));
  });
});
