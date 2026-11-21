import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

/**
 * Regression tests for the Writer ribbon's paragraph and font controls.
 *
 * Clicking an alignment button or choosing a font size, font or line spacing
 * moves focus from the paragraph to the ribbon before the handler runs. The
 * handlers looked the paragraph up through `document.activeElement`, found
 * none, and silently did nothing: centring, sizing and spacing text in a new
 * document had no effect. On a blank paragraph the chosen size was also lost
 * as soon as the user typed, because the typed text landed outside the empty
 * styled run.
 */
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null), save: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-fs", () => ({ readFile: vi.fn(async () => new Uint8Array()) }));

import { WriterEditor } from "./WriterEditor";
import { useOfficeTabs, type OfficeTab } from "../lib/office-store";
import type { Block, TextDocument } from "../lib/office-types";

function Harness({ id }: { id: string }) {
  const tab = useOfficeTabs((state) => state.tabs.find((candidate) => candidate.id === id));
  if (!tab) return null;
  return <WriterEditor tab={tab as OfficeTab & { model: TextDocument }} />;
}

function firstParagraph(): HTMLElement {
  const element = document.querySelector<HTMLElement>(".writer-body .para");
  if (!element) throw new Error("no paragraph");
  return element;
}

function firstBlock(): Extract<Block, { type: "paragraph" }> {
  const block = (useOfficeTabs.getState().tabs[0].model as TextDocument).blocks[0];
  if (block?.type !== "paragraph") throw new Error("first block is not a paragraph");
  return block;
}

async function newDocumentWithText(text: string) {
  const user = userEvent.setup();
  const id = useOfficeTabs.getState().create("writer", "Untitled");
  render(<Harness id={id} />);
  await user.click(firstParagraph());
  if (text) await user.keyboard(text);
  return user;
}

describe("Writer ribbon formatting reaches the paragraph after focus moves to the ribbon", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
  });

  it("centres the paragraph from the alignment button", async () => {
    const user = await newDocumentWithText("Hello");
    await user.click(screen.getByTitle("Centre"));
    expect(firstBlock().props.align).toBe("center");
    expect(firstParagraph().style.textAlign).toBe("center");
    // The text typed before stays.
    expect(
      firstBlock()
        .runs.map((run) => run.text)
        .join(""),
    ).toBe("Hello");
  });

  it("sets the font size from the size dropdown", async () => {
    const user = await newDocumentWithText("Hello");
    await user.selectOptions(screen.getByTitle("Size"), "24");
    expect(firstBlock().runs.every((run) => run.sizePt === 24)).toBe(true);
    expect(firstParagraph().style.fontSize).toBe("24pt");
  });

  it("sets the font from the font dropdown", async () => {
    const user = await newDocumentWithText("Hello");
    await user.selectOptions(screen.getByTitle("Font"), "Arial");
    expect(firstBlock().runs.every((run) => run.font === "Arial")).toBe(true);
  });

  it("sets the line spacing from the line spacing dropdown", async () => {
    const user = await newDocumentWithText("Hello");
    await user.selectOptions(screen.getByTitle("Line spacing"), "2");
    expect(firstBlock().props.lineSpacing).toBe(2);
  });

  it("keeps a size and font chosen on a blank paragraph for the text typed next", async () => {
    const user = await newDocumentWithText("");
    await user.selectOptions(screen.getByTitle("Size"), "20");
    await user.selectOptions(screen.getByTitle("Font"), "Georgia");
    await user.click(firstParagraph());
    await user.keyboard("Title");
    const block = firstBlock();
    expect(block.runs.map((run) => run.text).join("")).toBe("Title");
    expect(block.runs.filter((run) => run.text).every((run) => run.sizePt === 20 && run.font === "Georgia")).toBe(true);
  });
});
