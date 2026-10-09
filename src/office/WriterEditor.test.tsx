import { act, fireEvent, render, screen } from "@testing-library/react";
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

// jsdom has no PointerEvent, so testing-library would fall back to a plain
// Event and drop button/clientX/pointerId. A MouseEvent subclass carries those
// fields plus pointerId, which the table grip, image handle and ruler gestures
// branch on.
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

import { WriterEditor } from "./WriterEditor";
import { PROBE_TIMEOUT_MS } from "./writer/regex-probe";
import { useOfficeTabs, type OfficeTab } from "../lib/office-store";
import { useSettings } from "../lib/store";
import { DEFAULT_SETTINGS } from "../lib/types";
import type { Block, Run, TableCell, TextDocument } from "../lib/office-types";
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

describe("Writer find & replace and quick styles", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
  });

  /** Creates a document whose paragraphs hold the given runs (or plain text). */
  function seed(paragraphsRuns: Array<string | Partial<Run>[]>, header: string[] = []): string {
    const id = useOfficeTabs.getState().create("writer", "Untitled");
    const model = useOfficeTabs.getState().tabs[0].model as TextDocument;
    const first = model.blocks.find((block) => block.type === "paragraph") as Extract<Block, { type: "paragraph" }>;
    const make = (runs: string | Partial<Run>[]) => ({
      type: "paragraph" as const,
      props: { ...first.props },
      runs: (typeof runs === "string" ? [{ text: runs }] : runs).map((run) => ({ ...first.runs[0], ...run })),
    });
    useOfficeTabs.setState((state) => ({
      tabs: state.tabs.map((entry) =>
        entry.id === id
          ? { ...entry, model: { ...model, blocks: paragraphsRuns.map(make), header: header.map(make) } }
          : entry,
      ),
    }));
    return id;
  }

  async function openFind(user: ReturnType<typeof userEvent.setup>) {
    await user.click(screen.getByRole("button", { name: "Find" }));
    return screen.getByLabelText("Find");
  }

  it("counts matches live and reports an invalid regular expression inline", async () => {
    const user = userEvent.setup();
    render(<Harness id={seed(["Alpha beta", "beta gamma Beta"])} />);
    const query = await openFind(user);

    await user.type(query, "beta");
    expect(screen.getByRole("status")).toHaveTextContent("3 matches");
    await user.click(screen.getByLabelText("Match case"));
    expect(screen.getByRole("status")).toHaveTextContent("2 matches");

    await user.click(screen.getByLabelText("Regular expression"));
    await user.clear(query);
    await user.type(query, "(");
    // Inline, not a thrown error, and nothing can run on a broken pattern.
    expect(screen.getByRole("alert")).toHaveTextContent("Invalid regular expression");
    expect(screen.getByRole("button", { name: "Replace all" })).toBeDisabled();
    await user.type(query, "Al|ga)");
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByRole("status")).toHaveTextContent("2 matches");
  });

  it("stops a regular expression that takes too long instead of freezing the editor", async () => {
    // A worker that never answers stands in for a catastrophic pattern.
    class HangingWorker {
      onmessage: (() => void) | null = null;
      onerror: (() => void) | null = null;
      postMessage() {}
      terminate() {}
    }
    vi.stubGlobal("Worker", HangingWorker);
    try {
      const user = userEvent.setup({ advanceTimers: (ms) => vi.advanceTimersByTime(ms) });
      vi.useFakeTimers({ shouldAdvanceTime: true });
      render(<Harness id={seed(["Alpha beta gamma"])} />);
      const query = await openFind(user);
      await user.click(screen.getByLabelText("Regular expression"));
      await user.click(query);
      await user.paste("(\\p{L}+\\s?)+;");
      expect(screen.getByRole("status")).toHaveTextContent("Searching");
      await act(async () => {
        vi.advanceTimersByTime(250 + PROBE_TIMEOUT_MS);
      });
      expect(screen.getByRole("status")).toHaveTextContent("takes too long");
      await user.click(screen.getByRole("button", { name: "Replace all" }));
      expect(blockTexts()).toEqual(["Alpha beta gamma"]);
    } finally {
      vi.useRealTimers();
      vi.unstubAllGlobals();
    }
  });

  it("replaces the current match and moves to the next one", async () => {
    const user = userEvent.setup();
    render(<Harness id={seed(["cat and cat", "a cat"])} />);
    const query = await openFind(user);
    await user.type(query, "cat");
    await user.type(screen.getByLabelText("Replace with"), "dog");

    await user.click(screen.getByRole("button", { name: "Find next" }));
    expect(screen.getByRole("status")).toHaveTextContent("Match 1 of 3");
    expect(window.getSelection()?.toString()).toBe("cat");

    await user.click(screen.getByRole("button", { name: "Replace" }));
    expect(blockTexts()).toEqual(["dog and cat", "a cat"]);
    expect(screen.getByRole("status")).toHaveTextContent("Match 1 of 2");

    await user.click(screen.getByRole("button", { name: "Replace" }));
    expect(blockTexts()).toEqual(["dog and dog", "a cat"]);
    await user.click(screen.getByRole("button", { name: "Replace" }));
    expect(blockTexts()).toEqual(["dog and dog", "a dog"]);
    expect(screen.getByRole("status")).toHaveTextContent("No matches");
  });

  it("only selects the next match when Replace is pressed without one", async () => {
    const user = userEvent.setup();
    render(<Harness id={seed(["one two one"])} />);
    await user.type(await openFind(user), "one");
    await user.type(screen.getByLabelText("Replace with"), "1");
    await user.click(screen.getByRole("button", { name: "Replace" }));
    expect(blockTexts()).toEqual(["one two one"]);
    expect(screen.getByRole("status")).toHaveTextContent("Match 1 of 2");
  });

  it("replaces every regex match with $1 groups and keeps the run formatting", async () => {
    const user = userEvent.setup();
    const due = [
      { text: "Due: ", bold: true },
      { text: "2026-10-06", italic: true },
    ];
    render(<Harness id={seed([due], ["Printed 2026-01-31"])} />);
    const query = await openFind(user);
    await user.click(screen.getByLabelText("Regular expression"));
    await user.click(query);
    await user.paste("(\\d{4})-(\\d{2})-(\\d{2})");
    await user.type(screen.getByLabelText("Replace with"), "$3.$2.$1");
    expect(screen.getByRole("status")).toHaveTextContent("2 matches");

    await user.click(screen.getByRole("button", { name: "Replace all" }));
    const saved = documentOf();
    const body = saved.blocks[0];
    expect(body.type === "paragraph" ? body.runs.map((run) => [run.text, run.bold, run.italic]) : []).toEqual([
      ["Due: ", true, false],
      ["06.10.2026", false, true],
    ]);
    expect(blockText(saved.header[0])).toBe("Printed 31.01.2026");
    expect(screen.getByRole("status")).toHaveTextContent("No matches");
  });

  it("applies a quick style to the paragraph with the caret and marks it active", async () => {
    const user = userEvent.setup();
    render(<Harness id={seed(["First", "Second"])} />);
    const editable = await openPageEditor(user, 1);
    expect(editable.dataset.blockIndex).toBe("1");
    expect(screen.getByRole("button", { name: "Normal" })).toHaveAttribute("aria-pressed", "true");

    await user.click(screen.getByRole("button", { name: "Heading 1" }));
    const blocks = documentOf().blocks;
    expect(blocks.map((block) => (block.type === "paragraph" ? block.props.style : ""))).toEqual([
      "Normal",
      "Heading1",
    ]);
    // The caret stayed in the paragraph and the gallery follows the change.
    expect(document.activeElement).toBe(pageEditables()[0]);
    expect(screen.getByRole("button", { name: "Heading 1" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "Normal" })).toHaveAttribute("aria-pressed", "false");
  });
});

describe("Writer table cell menu", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
    useSettings.setState({ settings: DEFAULT_SETTINGS });
  });

  /** A one-row, two-cell table as the only block. */
  function seedTable(): string {
    const id = useOfficeTabs.getState().create("writer", "Untitled");
    const model = useOfficeTabs.getState().tabs[0].model as TextDocument;
    const first = model.blocks.find((block) => block.type === "paragraph") as Extract<Block, { type: "paragraph" }>;
    const cell = (text: string): TableCell => ({
      blocks: [{ type: "paragraph", props: { ...first.props }, runs: [{ ...first.runs[0], text }] }],
      colspan: 1,
      rowspan: 1,
      background: null,
      align: "left",
      valign: "top",
      widthPt: null,
    });
    const table: Block = {
      type: "table",
      table: {
        rows: [{ cells: [cell("A1"), cell("B1")], heightPt: null, header: false }],
        columnWidthsPt: [200, 200],
        borders: true,
        borderColor: "#000000",
        align: "left",
      },
    };
    useOfficeTabs.setState((state) => ({
      tabs: state.tabs.map((entry) => (entry.id === id ? { ...entry, model: { ...model, blocks: [table] } } : entry)),
    }));
    return id;
  }

  /** Tables are static in the page view; clicking one opens the editable surface. */
  async function openTable(user: ReturnType<typeof userEvent.setup>) {
    await user.click(document.querySelector(".writer-fragment") as HTMLElement);
  }

  it("shows English labels and adds a row below", async () => {
    const user = userEvent.setup();
    render(<Harness id={seedTable()} />);
    await openTable(user);

    fireEvent.contextMenu(document.querySelector(".writer-table td") as HTMLElement);
    expect(screen.getByText("Row 1 · Cell 1")).toBeInTheDocument();
    for (const label of ["Delete row", "Add column", "Delete column", "Toggle cell shade", "Toggle borders"]) {
      expect(screen.getByRole("button", { name: label })).toBeInTheDocument();
    }
    await user.click(screen.getByRole("button", { name: "Add row below" }));
    const saved = documentOf().blocks[0];
    expect(saved.type === "table" ? saved.table.rows.length : 0).toBe(2);
  });

  it("translates every label when the interface language is Turkish", async () => {
    const user = userEvent.setup();
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS, language: "tr" } });
    render(<Harness id={seedTable()} />);
    await openTable(user);

    fireEvent.contextMenu(document.querySelectorAll(".writer-table td")[1] as HTMLElement);
    expect(screen.getByText("Satır 1 · Hücre 2")).toBeInTheDocument();
    for (const label of [
      "Altına satır ekle",
      "Satırı sil",
      "Sütun ekle",
      "Sütunu sil",
      "Hücre gölgesini aç/kapat",
      "Kenarlıkları aç/kapat",
    ]) {
      expect(screen.getByRole("button", { name: label })).toBeInTheDocument();
    }
    expect(screen.queryByRole("button", { name: "Add row below" })).toBeNull();
  });

  it("merges two selected cells and splits them back", async () => {
    const user = userEvent.setup();
    render(<Harness id={seedTable()} />);
    await openTable(user);

    const cells = document.querySelectorAll<HTMLElement>(".writer-table td");
    // A drag or a shift-click builds the rectangle; the menu then offers Merge.
    fireEvent.pointerDown(cells[0], { button: 0 });
    fireEvent.pointerDown(cells[1], { button: 0, shiftKey: true });
    fireEvent.contextMenu(cells[1]);
    await user.click(screen.getByRole("button", { name: "Merge cells" }));

    let saved = documentOf().blocks[0];
    if (saved.type !== "table") throw new Error("table missing");
    expect(saved.table.rows[0].cells).toHaveLength(1);
    expect(saved.table.rows[0].cells[0].colspan).toBe(2);
    expect(saved.table.rows[0].cells[0].rowspan).toBe(1);
    expect(
      saved.table.rows[0].cells[0].blocks.map((block) => (block.type === "paragraph" ? block.runs[0].text : "")),
    ).toEqual(["A1", "", "B1"]);

    // Split puts the covered cell back; the model is 1x1 again.
    fireEvent.contextMenu(document.querySelector(".writer-table td") as HTMLElement);
    await user.click(screen.getByRole("button", { name: "Split cell" }));
    saved = documentOf().blocks[0];
    if (saved.type !== "table") throw new Error("table missing");
    expect(saved.table.rows[0].cells).toHaveLength(2);
    expect(saved.table.rows[0].cells.map((cell) => cell.colspan)).toEqual([1, 1]);
  });

  it("resizes a table column from its grip in one undoable drag", async () => {
    const user = userEvent.setup();
    render(<Harness id={seedTable()} />);
    await openTable(user);

    const grip = document.querySelector<HTMLElement>('[data-col-resize="0"]');
    expect(grip).not.toBeNull();
    fireEvent.pointerDown(grip as HTMLElement, { button: 0, pointerId: 11, clientX: 100 });
    fireEvent.pointerMove(window, { pointerId: 11, clientX: 148 });
    fireEvent.pointerUp(window, { pointerId: 11 });

    const saved = documentOf().blocks[0];
    if (saved.type !== "table") throw new Error("table missing");
    // 48px at 100% zoom is 36pt on top of the seeded 200pt column.
    expect(saved.table.columnWidthsPt[0]).toBeCloseTo(236);
    expect(saved.table.columnWidthsPt[1]).toBe(200);
  });
});

describe("Writer format painter", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
    useSettings.setState({ settings: DEFAULT_SETTINGS });
  });

  function seedParagraphs(): string {
    const id = useOfficeTabs.getState().create("writer", "Untitled");
    const model = useOfficeTabs.getState().tabs[0].model as TextDocument;
    const first = model.blocks.find((block) => block.type === "paragraph") as Extract<Block, { type: "paragraph" }>;
    const make = (run: Partial<Run>) => ({
      type: "paragraph" as const,
      props: { ...first.props },
      runs: [{ ...first.runs[0], ...run }],
    });
    useOfficeTabs.setState((state) => ({
      tabs: state.tabs.map((entry) =>
        entry.id === id
          ? {
              ...entry,
              model: {
                ...model,
                blocks: [make({ text: "A", bold: true, color: "#ff0000" }), make({ text: "B" })],
              },
            }
          : entry,
      ),
    }));
    return id;
  }

  it("paints the source formatting onto the next clicked paragraph and turns off", async () => {
    const user = userEvent.setup();
    render(<Harness id={seedParagraphs()} />);

    // Focus the source paragraph: its props/runs become the selection state.
    await openPageEditor(user, 0);
    const painter = screen.getByRole("button", { name: "Format painter" });
    await user.click(painter);
    expect(painter).toHaveAttribute("aria-pressed", "true");

    // One-shot policy: the painter applies to the next paragraph and disarms.
    await user.click(pageFragments()[1]);
    const blocks = documentOf().blocks;
    const target = blocks[1];
    expect(target.type === "paragraph" ? target.runs[0].bold : false).toBe(true);
    // The DOM round-trip through the editable normalises `#ff0000`, so accept
    // the computed form too.
    const color = target.type === "paragraph" ? target.runs[0].color : null;
    expect(color === "#ff0000" || color === "rgb(255, 0, 0)").toBe(true);
    expect(screen.getByRole("button", { name: "Format painter" })).toHaveAttribute("aria-pressed", "false");
  });

  it("disarms with Escape without changing anything", async () => {
    const user = userEvent.setup();
    render(<Harness id={seedParagraphs()} />);
    await openPageEditor(user, 0);

    await user.click(screen.getByRole("button", { name: "Format painter" }));
    await user.keyboard("{Escape}");
    expect(screen.getByRole("button", { name: "Format painter" })).toHaveAttribute("aria-pressed", "false");
    await user.click(pageFragments()[1]);
    const target = documentOf().blocks[1];
    expect(target.type === "paragraph" ? target.runs[0].bold : true).toBe(false);
  });
});

describe("Writer ruler tab stops", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
    useSettings.setState({ settings: DEFAULT_SETTINGS });
  });

  it("adds a tab stop by clicking the ruler and removes it on double-click", async () => {
    const user = userEvent.setup();
    const id = useOfficeTabs.getState().create("writer", "Untitled");
    render(<Harness id={id} />);
    await openPageEditor(user, 0);

    const ruler = document.querySelector<HTMLElement>(".writer-ruler");
    expect(ruler).not.toBeNull();
    // 200px from the sheet edge at 100% zoom: 78pt past the 72pt left margin.
    fireEvent.click(ruler as HTMLElement, { clientX: 200 });
    const withStop = documentOf().blocks[0];
    expect(withStop.type === "paragraph" ? withStop.props.tabs : []).toEqual([{ posPt: 78, align: "left" }]);

    const marker = document.querySelector<HTMLElement>('[data-tab-index="0"]');
    expect(marker).not.toBeNull();
    fireEvent.doubleClick(marker as HTMLElement);
    const removed = documentOf().blocks[0];
    expect(removed.type === "paragraph" ? (removed.props.tabs ?? []) : []).toEqual([]);
  });
});

describe("Writer image options and handles", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
    useSettings.setState({ settings: DEFAULT_SETTINGS });
  });

  function seedImage(): string {
    const id = useOfficeTabs.getState().create("writer", "Untitled");
    const model = useOfficeTabs.getState().tabs[0].model as TextDocument;
    const image: Block = {
      type: "image",
      image: { name: "pic.png", mime: "image/png", dataBase64: "", alt: "pic" },
      widthPt: 320,
      heightPt: 220,
      align: "center",
      caption: "Figure",
      wrap: "inline",
    };
    useOfficeTabs.setState((state) => ({
      tabs: state.tabs.map((entry) => (entry.id === id ? { ...entry, model: { ...model, blocks: [image] } } : entry)),
    }));
    return id;
  }

  async function openImageOptions(user: ReturnType<typeof userEvent.setup>) {
    // Images are static in the page view: clicking one opens the continuous
    // surface, where the selectable figure and its dialog live.
    await user.click(document.querySelector(".writer-fragment") as HTMLElement);
    await user.click(document.querySelector(".writer-image img") as HTMLElement);
  }

  it("persists the text wrapping choice in the model", async () => {
    const user = userEvent.setup();
    render(<Harness id={seedImage()} />);
    await openImageOptions(user);
    await user.selectOptions(screen.getByLabelText("Text wrapping"), "square");
    const saved = documentOf().blocks[0];
    expect(saved.type === "image" ? saved.wrap : "").toBe("square");
  });

  it("resizes from a corner handle preserving the aspect ratio", async () => {
    const user = userEvent.setup();
    render(<Harness id={seedImage()} />);
    await openImageOptions(user);

    const handle = document.querySelector<HTMLElement>('[data-image-handle="se"]');
    expect(handle).not.toBeNull();
    fireEvent.pointerDown(handle as HTMLElement, { button: 0, pointerId: 21, clientX: 100, clientY: 100 });
    fireEvent.pointerMove(window, { pointerId: 21, clientX: 148, clientY: 130 });
    fireEvent.pointerUp(window, { pointerId: 21 });

    const saved = documentOf().blocks[0];
    if (saved.type !== "image") throw new Error("image missing");
    // +48px is +36pt; the height follows the 320:220 aspect.
    expect(Math.round(saved.widthPt)).toBe(356);
    expect(Math.round(saved.heightPt)).toBe(Math.round((356 * 220) / 320));
  });
});

describe("Writer watermark", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
    useSettings.setState({ settings: DEFAULT_SETTINGS });
  });

  it("applies a TASLAK preset, previews it and removes it again", async () => {
    const user = userEvent.setup();
    const id = useOfficeTabs.getState().create("writer", "Untitled");
    render(<Harness id={id} />);

    await user.click(screen.getByRole("button", { name: "Layout" }));
    await user.click(screen.getByRole("button", { name: "Watermark" }));
    await user.click(screen.getByRole("button", { name: "TASLAK" }));
    await user.click(screen.getByRole("button", { name: "Apply" }));

    const watermark = documentOf().watermark;
    expect(watermark?.text).toBe("TASLAK");
    expect(document.querySelector(".writer-watermark")?.textContent).toBe("TASLAK");

    await user.click(screen.getByRole("button", { name: "Watermark" }));
    await user.click(screen.getByRole("button", { name: "Remove" }));
    expect(documentOf().watermark ?? null).toBeNull();
    expect(document.querySelector(".writer-watermark")).toBeNull();
  });
});

describe("Writer incremental pagination", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
    useSettings.setState({ settings: DEFAULT_SETTINGS });
  });

  it("typing in the last paragraph keeps earlier pages identical", async () => {
    const user = userEvent.setup();
    const id = useOfficeTabs.getState().create("writer", "Untitled");
    const model = useOfficeTabs.getState().tabs[0].model as TextDocument;
    const first = model.blocks.find((block) => block.type === "paragraph") as Extract<Block, { type: "paragraph" }>;
    const make = (text: string) => ({
      type: "paragraph" as const,
      props: { ...first.props },
      runs: [{ ...first.runs[0], text }],
    });
    useOfficeTabs.setState((state) => ({
      tabs: state.tabs.map((entry) =>
        entry.id === id ? { ...entry, model: { ...model, blocks: [make("A"), make("B"), make("C")] } } : entry,
      ),
    }));
    render(<Harness id={id} />);

    const firstSheet = document.querySelector(".writer-page-sheet");
    const firstParagraph = firstSheet?.querySelector(".para");
    expect(firstParagraph?.textContent).toBe("A");

    await openPageEditor(user, 2);
    await user.keyboard("X");

    // The caret opens at the start of the clicked fragment, so the edit lands
    // at offset 0; the point is that only the last block's metrics changed.
    expect(blockTexts()).toEqual(["A", "B", "XC"]);
    // The first sheet and its paragraph are the same DOM nodes: the dirty
    // measurement did not rebuild (or even reconcile) the untouched page.
    expect(document.querySelectorAll(".writer-page-sheet")[0]).toBe(firstSheet);
    expect(document.querySelectorAll(".writer-page-sheet")[0].querySelector(".para")).toBe(firstParagraph);
    expect(firstParagraph?.textContent).toBe("A");
  });
});
