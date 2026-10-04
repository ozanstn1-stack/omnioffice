import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

/**
 * Regression tests for the Writer's structural editing.
 *
 * A focused contentEditable is not re-rendered by React - that is what keeps
 * native typing, IME and the clipboard working - so after Enter/Backspace the
 * element still showed the pre-edit text. When focus moved, `blur` read that
 * stale DOM and wrote it back into the model, undoing the edit (paragraphs
 * were duplicated on Enter, merges resurrected the removed paragraph). The fix
 * repaints the affected paragraph before focus moves; these tests type without
 * clicking and assert the model text after every structural key.
 *
 * V3.1 moved the editing surface into the paginated page itself, so the tests
 * below drive the real page fragments: clicking one must place a caret in an
 * editable hosted by the sheet, and every structural key must still reach the
 * model through the same code path as the continuous view.
 */
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null), save: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-fs", () => ({ readFile: vi.fn(async () => new Uint8Array()) }));

import { WriterEditor } from "./WriterEditor";
import { useOfficeTabs, type OfficeTab } from "../lib/office-store";
import type { Block, TextDocument } from "../lib/office-types";
import { caretOffset, setCaretOffset } from "./writer/caret";

function Harness({ id }: { id: string }) {
  const tab = useOfficeTabs((state) => state.tabs.find((candidate) => candidate.id === id));
  if (!tab) return null;
  return <WriterEditor tab={tab as OfficeTab & { model: TextDocument }} />;
}

function paragraphs(): HTMLElement[] {
  return Array.from(document.querySelectorAll<HTMLElement>(".writer-body .para"));
}

function pageFragments(): HTMLElement[] {
  return Array.from(document.querySelectorAll<HTMLElement>(".writer-fragment"));
}

function pageEditables(): HTMLElement[] {
  return Array.from(document.querySelectorAll<HTMLElement>('.writer-page-sheet .para[contenteditable="true"]'));
}

/** Clicks a page fragment and returns the editable the page opened for it. */
async function openPageEditor(user: ReturnType<typeof userEvent.setup>, fragmentIndex = 0): Promise<HTMLElement> {
  await user.click(pageFragments()[fragmentIndex]);
  const editable = pageEditables()[0];
  if (!editable) throw new Error("no editable opened in the page sheet");
  return editable;
}

function documentOf(): TextDocument {
  return useOfficeTabs.getState().tabs[0].model as TextDocument;
}

function blockText(block: Block | undefined): string {
  if (!block || block.type !== "paragraph") return "";
  return block.runs.map((run) => run.text).join("");
}

function blockTexts(): string[] {
  return documentOf().blocks.map(blockText);
}

describe("Writer structural editing stays in sync with the model", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
  });

  it("splits the paragraph on Enter and keeps typing in the new one", async () => {
    const user = userEvent.setup();
    const id = useOfficeTabs.getState().create("writer", "Untitled");
    render(<Harness id={id} />);

    await user.click(paragraphs()[0]);
    await user.keyboard("Hello");
    expect(paragraphs()[0].textContent).toBe("Hello");

    await user.keyboard("{Enter}");
    // The new paragraph exists and already owns the caret: no clicking needed.
    expect(paragraphs()).toHaveLength(2);
    expect(document.activeElement).toBe(paragraphs()[1]);

    await user.keyboard("World");
    expect(blockTexts()).toEqual(["Hello", "World"]);
    expect(paragraphs()[0].textContent).toBe("Hello");
    expect(paragraphs()[1].textContent).toBe("World");
  });

  it("merges into the previous paragraph on Backspace at the start", async () => {
    const user = userEvent.setup();
    const id = useOfficeTabs.getState().create("writer", "Untitled");
    render(<Harness id={id} />);

    await user.click(paragraphs()[0]);
    await user.keyboard("Hello{Enter}World");
    expect(blockTexts()).toEqual(["Hello", "World"]);

    await user.keyboard("{Home}");
    await user.keyboard("{Backspace}");
    expect(blockTexts()).toEqual(["HelloWorld"]);
    expect(paragraphs()).toHaveLength(1);
    expect(paragraphs()[0].textContent).toBe("HelloWorld");

    // Word behaviour: the caret lands at the join point, which is where the
    // removed paragraph started.
    await user.keyboard("!");
    expect(blockTexts()).toEqual(["Hello!World"]);
  });

  it("inserts a line break in place on Shift+Enter and continues typing after it", async () => {
    const user = userEvent.setup();
    const id = useOfficeTabs.getState().create("writer", "Untitled");
    render(<Harness id={id} />);

    await user.click(paragraphs()[0]);
    await user.keyboard("Line1{Shift>}{Enter}{/Shift}Line2");
    expect(blockTexts()).toEqual(["Line1\nLine2"]);
    expect(paragraphs()).toHaveLength(1);
  });

  it("merges the following paragraph on Delete at the end", async () => {
    const user = userEvent.setup();
    const id = useOfficeTabs.getState().create("writer", "Untitled");
    render(<Harness id={id} />);

    await user.click(paragraphs()[0]);
    await user.keyboard("Hello{Enter}World{Home}{Backspace}");
    expect(blockTexts()).toEqual(["HelloWorld"]);

    await user.keyboard("{End}{Delete}");
    expect(blockTexts()).toEqual(["HelloWorld"]);
  });

  it("renders a real page container per page and counts them", async () => {
    const user = userEvent.setup();
    const id = useOfficeTabs.getState().create("writer", "Untitled");
    const tab = useOfficeTabs.getState().tabs[0];
    const model = tab.model as TextDocument;
    const first = model.blocks.find((block) => block.type === "paragraph") as Extract<Block, { type: "paragraph" }>;
    const blocks = [
      ...model.blocks,
      { type: "pageBreak" as const },
      { type: "paragraph" as const, props: { ...first.props }, runs: [{ ...first.runs[0], text: "Page two" }] },
      { type: "pageBreak" as const },
      { type: "paragraph" as const, props: { ...first.props }, runs: [{ ...first.runs[0], text: "Page three" }] },
    ];
    useOfficeTabs.setState((state) => ({
      tabs: state.tabs.map((entry) => (entry.id === id ? { ...entry, model: { ...model, blocks } } : entry)),
    }));
    render(<Harness id={id} />);

    const sheets = document.querySelectorAll(".writer-page-sheet");
    expect(sheets.length).toBe(3);
    expect(sheets[1].textContent).toContain("Page two");
    expect(sheets[2].textContent).toContain("Page three");
    expect(document.querySelector(".editor-status")?.textContent).toContain("3 pages");
    // Clicking a fragment keeps the page view and opens a real editable inside
    // the sheet (V3.1: no jump to the continuous surface).
    await user.click(document.querySelectorAll(".writer-fragment")[1]);
    expect(document.querySelectorAll(".writer-page-sheet").length).toBe(3);
    const editable = document.querySelector<HTMLElement>('.writer-page-sheet .para[contenteditable="true"]');
    expect(editable).not.toBeNull();
    expect(editable?.dataset.blockIndex).toBe("2");
    expect(document.activeElement).toBe(editable);
  });

  it("inserts a table of contents from the headings", async () => {
    const user = userEvent.setup();
    const id = useOfficeTabs.getState().create("writer", "Untitled");
    const tab = useOfficeTabs.getState().tabs[0];
    const model = tab.model as TextDocument;
    const first = model.blocks.find((block) => block.type === "paragraph") as Extract<Block, { type: "paragraph" }>;
    const blocks = [
      {
        type: "paragraph" as const,
        props: { ...first.props, style: "Heading1" },
        runs: [{ ...first.runs[0], text: "Introduction" }],
      },
      {
        type: "paragraph" as const,
        props: { ...first.props, style: "Heading2" },
        runs: [{ ...first.runs[0], text: "Background" }],
      },
      { type: "paragraph" as const, props: { ...first.props }, runs: [{ ...first.runs[0], text: "Body" }] },
    ];
    useOfficeTabs.setState((state) => ({
      tabs: state.tabs.map((entry) => (entry.id === id ? { ...entry, model: { ...model, blocks } } : entry)),
    }));
    render(<Harness id={id} />);

    await user.click(screen.getByRole("button", { name: "Insert" }));
    await user.click(screen.getByRole("button", { name: "Insert TOC" }));

    const saved = useOfficeTabs.getState().tabs[0].model as TextDocument;
    const toc = saved.blocks.find((block) => block.type === "toc");
    expect(toc).toBeDefined();
    if (toc?.type !== "toc") throw new Error("toc missing");
    expect(toc.entries.map((entry) => entry.text)).toEqual(["Introduction", "Background"]);
    expect(toc.entries.map((entry) => entry.level)).toEqual([1, 2]);
    expect(document.querySelector(".writer-toc")?.textContent).toContain("Introduction");
  });

  it("lists the outline in the navigation pane", async () => {
    const user = userEvent.setup();
    const id = useOfficeTabs.getState().create("writer", "Untitled");
    const tab = useOfficeTabs.getState().tabs[0];
    const model = tab.model as TextDocument;
    const first = model.blocks.find((block) => block.type === "paragraph") as Extract<Block, { type: "paragraph" }>;
    const blocks = [
      {
        type: "paragraph" as const,
        props: { ...first.props, style: "Heading1" },
        runs: [{ ...first.runs[0], text: "Chapter one" }],
      },
      { type: "paragraph" as const, props: { ...first.props }, runs: [{ ...first.runs[0], text: "Text" }] },
    ];
    useOfficeTabs.setState((state) => ({
      tabs: state.tabs.map((entry) => (entry.id === id ? { ...entry, model: { ...model, blocks } } : entry)),
    }));
    render(<Harness id={id} />);

    await user.click(screen.getByRole("button", { name: "View" }));
    await user.click(screen.getByRole("button", { name: "Navigation" }));
    expect(document.querySelector(".writer-nav-pane")?.textContent).toContain("Chapter one");
  });

  it("does not resurrect the split tail when typing continues fast", async () => {
    const user = userEvent.setup();
    const id = useOfficeTabs.getState().create("writer", "Untitled");
    render(<Harness id={id} />);

    await user.click(paragraphs()[0]);
    await user.keyboard("One{Enter}Two{Enter}Three");
    expect(blockTexts()).toEqual(["One", "Two", "Three"]);
  });
});

describe("Writer paginated in-place editing", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
  });

  it("focuses a real editable inside the page sheet on click", async () => {
    const user = userEvent.setup();
    const id = useOfficeTabs.getState().create("writer", "Untitled");
    render(<Harness id={id} />);

    const editable = await openPageEditor(user);
    expect(editable.closest(".writer-page-sheet")).not.toBeNull();
    // jsdom does not implement `isContentEditable`; the attribute is the DOM truth.
    expect(editable.getAttribute("contenteditable")).toBe("true");
    expect(editable.tabIndex).toBe(0);
    expect(document.activeElement).toBe(editable);
    expect(document.querySelectorAll(".writer-page-sheet").length).toBe(1);
    // Exactly one editable surface per block, even though the page preview
    // renders a static copy of the paragraph.
    expect(pageEditables()).toHaveLength(1);
  });

  it("places the caret at the clicked character inside the fragment", async () => {
    const user = userEvent.setup();
    const id = useOfficeTabs.getState().create("writer", "Untitled");
    const tab = useOfficeTabs.getState().tabs[0];
    const model = tab.model as TextDocument;
    const first = model.blocks.find((block) => block.type === "paragraph") as Extract<Block, { type: "paragraph" }>;
    const blocks = [
      { type: "paragraph" as const, props: { ...first.props }, runs: [{ ...first.runs[0], text: "Hello World" }] },
    ];
    useOfficeTabs.setState((state) => ({
      tabs: state.tabs.map((entry) => (entry.id === id ? { ...entry, model: { ...model, blocks } } : entry)),
    }));
    render(<Harness id={id} />);

    const fragment = pageFragments()[0];
    const preview = fragment.querySelector<HTMLElement>(".para");
    expect(preview).not.toBeNull();
    // jsdom has no hit test; stub `caretRangeFromPoint` with the position a
    // click after "Hello " would produce.
    const doc = document as unknown as { caretRangeFromPoint?: (x: number, y: number) => Range | null };
    const range = document.createRange();
    range.setStart(preview?.firstChild as Text, 6);
    range.collapse(true);
    doc.caretRangeFromPoint = () => range;
    try {
      await user.click(fragment);
    } finally {
      delete doc.caretRangeFromPoint;
    }

    const editable = pageEditables()[0];
    expect(document.activeElement).toBe(editable);
    expect(caretOffset(editable)).toBe(6);
    await user.keyboard("X");
    expect(blockTexts()[0]).toBe("Hello XWorld");
  });

  it("types into a page fragment and updates the model paragraph", async () => {
    const user = userEvent.setup();
    const id = useOfficeTabs.getState().create("writer", "Untitled");
    render(<Harness id={id} />);

    const editable = await openPageEditor(user);
    await user.keyboard("Hello");
    expect(blockTexts()[0]).toBe("Hello");
    expect(editable.textContent).toBe("Hello");
  });

  it("splits on Enter in a page fragment and keeps typing in the new paragraph", async () => {
    const user = userEvent.setup();
    const id = useOfficeTabs.getState().create("writer", "Untitled");
    render(<Harness id={id} />);

    await openPageEditor(user);
    await user.keyboard("Hello{Enter}");
    expect(blockTexts()).toEqual(["Hello", ""]);

    // The new paragraph owns the caret in its own page fragment, no clicking.
    const next = pageEditables()[0];
    expect(next.dataset.blockIndex).toBe("1");
    expect(document.activeElement).toBe(next);
    await user.keyboard("World");
    expect(blockTexts()).toEqual(["Hello", "World"]);
    expect(next.textContent).toBe("World");
  });

  it("merges into the previous paragraph on Backspace at the start of a fragment", async () => {
    const user = userEvent.setup();
    const id = useOfficeTabs.getState().create("writer", "Untitled");
    render(<Harness id={id} />);

    await openPageEditor(user);
    await user.keyboard("Hello{Enter}World");
    await user.keyboard("{Home}{Backspace}");
    expect(blockTexts()).toEqual(["HelloWorld"]);

    // The caret lands at the join point in the merged paragraph.
    await user.keyboard("!");
    expect(blockTexts()).toEqual(["Hello!World"]);
  });

  it("moves the caret to the next page's fragment on ArrowDown at the end", async () => {
    const user = userEvent.setup();
    const id = useOfficeTabs.getState().create("writer", "Untitled");
    const tab = useOfficeTabs.getState().tabs[0];
    const model = tab.model as TextDocument;
    const first = model.blocks.find((block) => block.type === "paragraph") as Extract<Block, { type: "paragraph" }>;
    const blocks = [
      { type: "paragraph" as const, props: { ...first.props }, runs: [{ ...first.runs[0], text: "First" }] },
      { type: "pageBreak" as const },
      { type: "paragraph" as const, props: { ...first.props }, runs: [{ ...first.runs[0], text: "Second" }] },
    ];
    useOfficeTabs.setState((state) => ({
      tabs: state.tabs.map((entry) => (entry.id === id ? { ...entry, model: { ...model, blocks } } : entry)),
    }));
    render(<Harness id={id} />);

    const editable = await openPageEditor(user);
    expect(editable.dataset.blockIndex).toBe("0");
    await user.keyboard("{End}{ArrowDown}");

    const next = pageEditables()[0];
    expect(next.dataset.blockIndex).toBe("2");
    expect(document.activeElement).toBe(next);
    // The active surface moved to the second sheet, not just to another block.
    const sheets = document.querySelectorAll(".writer-page-sheet");
    expect(next.closest(".writer-page-sheet")).toBe(sheets[1]);
  });

  it("keeps header editing reachable from the paginated view", async () => {
    const user = userEvent.setup();
    const id = useOfficeTabs.getState().create("writer", "Untitled");
    const tab = useOfficeTabs.getState().tabs[0];
    const model = tab.model as TextDocument;
    const first = model.blocks.find((block) => block.type === "paragraph") as Extract<Block, { type: "paragraph" }>;
    const header = [
      { type: "paragraph" as const, props: { ...first.props }, runs: [{ ...first.runs[0], text: "Header text" }] },
    ];
    useOfficeTabs.setState((state) => ({
      tabs: state.tabs.map((entry) => (entry.id === id ? { ...entry, model: { ...model, header } } : entry)),
    }));
    render(<Harness id={id} />);

    const preview = document.querySelector<HTMLElement>(".writer-page-sheet .writer-header-zone .para");
    expect(preview?.textContent).toContain("Header text");
    await user.click(preview as HTMLElement);

    // Header editing still lives on the continuous surface, focused on the
    // header paragraph rather than on a body block.
    const editable = document.querySelector<HTMLElement>('[data-scope="header"][contenteditable="true"]');
    expect(editable).not.toBeNull();
    expect(document.activeElement).toBe(editable);
    await user.keyboard("{End}!");
    const saved = useOfficeTabs.getState().tabs[0].model as TextDocument;
    const savedHeader = saved.header[0];
    expect(savedHeader.type === "paragraph" ? savedHeader.runs.map((run) => run.text).join("") : "").toBe(
      "Header text!",
    );
  });

  it("keeps the caret offset and focus when typing causes a reflow", async () => {
    const user = userEvent.setup();
    const id = useOfficeTabs.getState().create("writer", "Untitled");
    render(<Harness id={id} />);

    const editable = await openPageEditor(user);
    await user.keyboard("Hello");
    expect(document.activeElement).toBe(editable);

    // Put the caret between "He" and "llo", then type: the model update
    // re-renders the editor (and recomputes pagination), and the focused
    // element must keep its caret instead of jumping to the start.
    setCaretOffset(editable, 2);
    await user.keyboard("X");
    expect(blockTexts()[0]).toBe("HeXllo");
    expect(document.activeElement).toBe(editable);
    expect(caretOffset(editable)).toBe(3);
  });

  it("numbers ordered lists across the document instead of repeating 1", () => {
    const id = useOfficeTabs.getState().create("writer", "Untitled");
    const tab = useOfficeTabs.getState().tabs[0];
    const model = tab.model as TextDocument;
    const first = model.blocks.find((block) => block.type === "paragraph") as Extract<Block, { type: "paragraph" }>;
    const numbered = (text: string, start = 1) => ({
      type: "paragraph" as const,
      props: { ...first.props, list: { kind: "number" as const, level: 0, start, marker: "" } },
      runs: [{ ...first.runs[0], text }],
    });
    const blocks = [
      numbered("First", 1),
      numbered("Second"),
      {
        type: "paragraph" as const,
        props: { ...first.props, list: null },
        runs: [{ ...first.runs[0], text: "Break" }],
      },
      numbered("Restart", 5),
    ];
    useOfficeTabs.setState((state) => ({
      tabs: state.tabs.map((entry) => (entry.id === id ? { ...entry, model: { ...model, blocks } } : entry)),
    }));
    render(<Harness id={id} />);

    const markers = Array.from(document.querySelectorAll<HTMLElement>(".writer-page-sheet .list-marker")).map(
      (marker) => marker.textContent,
    );
    // Consecutive items increment; a body paragraph ends the series and the
    // next numbered paragraph starts at its own `start`.
    expect(markers).toEqual(["1.", "2.", "5."]);
  });

  it("renders page and title fields from the live document, not the cached text", () => {
    const id = useOfficeTabs.getState().create("writer", "Report Title");
    const tab = useOfficeTabs.getState().tabs[0];
    const model = tab.model as TextDocument;
    const first = model.blocks.find((block) => block.type === "paragraph") as Extract<Block, { type: "paragraph" }>;
    const field = (kind: string, cached: string) => ({
      ...first.runs[0],
      text: "",
      field: { kind, target: "", cached },
    });
    const blocks = [
      {
        type: "paragraph" as const,
        props: { ...first.props },
        runs: [
          field("page", "99"),
          { ...first.runs[0], text: " of " },
          field("pages", "99"),
          { ...first.runs[0], text: " — " },
          field("title", "stale"),
        ],
      },
    ];
    useOfficeTabs.setState((state) => ({
      tabs: state.tabs.map((entry) =>
        entry.id === id
          ? { ...entry, model: { ...model, metadata: { ...model.metadata, title: "Report Title" }, blocks } }
          : entry,
      ),
    }));
    render(<Harness id={id} />);

    const fields = Array.from(document.querySelectorAll<HTMLElement>(".writer-page-sheet .writer-field")).map(
      (element) => element.textContent,
    );
    // PAGE/NUMPAGES/TITLE come from the pagination result and metadata; the
    // cached values are only the fallback.
    expect(fields).toEqual(["1", "1", "Report Title"]);
  });
});
