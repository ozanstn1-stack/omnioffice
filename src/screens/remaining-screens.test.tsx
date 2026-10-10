import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

/**
 * Render + interaction coverage for the last screens without tests:
 * Organize, Annotate, Batch, Page Tools and Plugins.
 */
let listAnnotations: unknown[] = [];
let outlineFixture: { title: string; page: number; depth: number }[] = [];

const invoke = vi.fn(async (command: string, _payload?: unknown) => {
  switch (command) {
    case "app_info":
      return { appVersion: "3.5.3", coreVersion: "3.5.3", platform: "windows" };
    case "engine_status":
      return { pdfium: true, qpdf: true, tesseract: true, tesseract_version: "5.0", ocr_languages: ["eng"] };
    case "ocr_languages":
      return [{ code: "eng", name: "English" }];
    case "load_settings":
      return { theme: "dark", language: "en", defaultCompression: "medium", showRecentFiles: true };
    case "save_settings":
    case "load_recent":
    case "jobs_register":
    case "jobs_finish":
    case "log_operation":
    case "log_frontend":
      return null;
    case "file_sizes":
      return [2048, 4096];
    case "suggest_output":
      return "C:/docs/out.pdf";
    case "output_exists":
      return false;
    case "pdf_info":
      return { ...pdfInfo, outline: outlineFixture };
    case "page_preview":
    case "page_thumbnail":
      return { dataUrl: "data:image/png;base64,iVBORw0KGgo=", width: 595, height: 842 };
    case "compress_pdf":
    case "extract_pages":
    case "apply_page_plan":
    case "annotate_pdf":
    case "pdf_annotate_editable":
    case "pdf_set_outline":
    case "stamp_pdf":
    case "nup_pdf":
      return { path: "C:/docs/out.pdf", pageCount: 2, originalBytes: 4096, outputBytes: 2048, message: "ok" };
    case "pdf_edit_annotations":
      return { edited: 1, deleted: 0, warnings: [] };
    case "pdf_list_annotations":
      return listAnnotations;
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

// jsdom has no PointerEvent; MouseEvent carries the client coordinates the
// annotation overlay reads.
if (typeof window.PointerEvent === "undefined") {
  window.PointerEvent = MouseEvent as unknown as typeof PointerEvent;
}

const pdfInfo = {
  path: "C:/docs/a.pdf",
  fileName: "a.pdf",
  fileSizeBytes: 2048,
  pageCount: 2,
  pdfVersion: "1.7",
  encrypted: false,
  hasTextLayer: true,
  metadata: {
    title: "",
    author: "",
    subject: "",
    keywords: "",
    creator: "",
    producer: "",
    creation_date: "",
    mod_date: "",
  },
  pageGeometries: [
    { page: 1, width_pt: 595.28, height_pt: 841.89, display_width_pt: 595.28, display_height_pt: 841.89, rotation: 0 },
    { page: 2, width_pt: 595.28, height_pt: 841.89, display_width_pt: 595.28, display_height_pt: 841.89, rotation: 0 },
  ],
  imageCount: 0,
  title: "",
  author: "",
  producer: "",
};

import { Organize } from "./Organize";
import { Annotate } from "./Annotate";
import { Batch } from "./Batch";
import { PageTools } from "./PageTools";
import { Plugins } from "./Plugins";
import { usePluginStore } from "../lib/plugins";

const props = { dragging: false, initialFiles: ["C:/docs/a.pdf"] };

describe("organize, annotate and batch screens", () => {
  beforeEach(() => {
    invoke.mockClear();
  });

  it("saves a page plan through apply_page_plan", async () => {
    const user = userEvent.setup();
    render(<Organize {...props} />);
    const runButton = await screen.findByRole("button", { name: "Save as PDF" });
    await waitFor(() => expect(runButton).toBeEnabled());
    await user.click(runButton);
    await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "apply_page_plan")).toBe(true));
    const call = (invoke.mock.calls as unknown as [string, { request: { plan: { source_page: number }[] } }][]).find(
      ([name]) => name === "apply_page_plan",
    );
    expect(call?.[1].request.plan).toEqual([
      { source_page: 1, rotation_delta: 0 },
      { source_page: 2, rotation_delta: 0 },
    ]);
  });

  it("places an annotation where the preview was clicked and passes it on", async () => {
    // jsdom has no layout: give the preview a real box so the click maps to a
    // document coordinate.
    const rectSpy = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
      x: 0,
      y: 0,
      left: 0,
      top: 0,
      right: 500,
      bottom: 700,
      width: 500,
      height: 700,
      toJSON: () => ({}),
    } as DOMRect);
    try {
      const user = userEvent.setup();
      render(<Annotate {...props} />);
      await waitFor(() => expect(document.querySelector(".canvas-wrap")).not.toBeNull());
      const wrap = document.querySelector<HTMLElement>(".canvas-wrap")!;
      const textArea = document.querySelector<HTMLTextAreaElement>("textarea.textarea")!;
      await user.type(textArea, "CONFIDENTIAL");
      // PageCanvas reports a click point to the annotation list.
      wrap.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true, clientX: 100, clientY: 150 }));
      wrap.dispatchEvent(new MouseEvent("pointerup", { bubbles: true, clientX: 100, clientY: 150 }));

      // The default mode is editable, so the annotation goes to the real
      // annotation layer command, with the wire kind of the text box tool.
      const runButton = screen.getByRole("button", { name: "Save annotations" });
      await user.click(runButton);
      await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "pdf_annotate_editable")).toBe(true));
      const call = (
        invoke.mock.calls as unknown as [
          string,
          { request: { annotations: { kind: string; text: string; x: number; y: number }[] } },
        ][]
      ).find(([name]) => name === "pdf_annotate_editable");
      expect(call?.[1].request.annotations).toHaveLength(1);
      expect(call?.[1].request.annotations[0].kind).toBe("textbox");
      expect(call?.[1].request.annotations[0].text).toBe("CONFIDENTIAL");
      expect(call?.[1].request.annotations[0].x).toBeCloseTo((100 / 500) * 595.28, 1);
      expect(call?.[1].request.annotations[0].y).toBeCloseTo((150 / 700) * 841.89, 1);
    } finally {
      rectSpy.mockRestore();
    }
  });

  it("runs the batch operation once per selected file", async () => {
    const user = userEvent.setup();
    render(<Batch initialFiles={["C:/docs/a.pdf", "C:/docs/b.pdf"]} dragging={false} />);
    const start = await screen.findByRole("button", { name: "Start batch" });
    await waitFor(() => expect(start).toBeEnabled());
    await user.click(start);
    await waitFor(() => expect(invoke.mock.calls.filter(([name]) => name === "compress_pdf").length).toBe(2));
  });
});

describe("page tools and plugins screens", () => {
  beforeEach(() => {
    invoke.mockClear();
  });

  it("extracts the selected pages through extract_pages", async () => {
    const user = userEvent.setup();
    render(<PageTools tab="extract" initialFiles={["C:/docs/a.pdf"]} dragging={false} />);
    const runButton = await screen.findByRole("button", { name: "Extract pages" });
    await waitFor(() => expect(runButton).toBeEnabled());
    await user.click(runButton);
    await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "extract_pages")).toBe(true));
    const call = (invoke.mock.calls as unknown as [string, { request: { selection: string } }][]).find(
      ([name]) => name === "extract_pages",
    );
    expect(call?.[1].request.selection).toBe("1");
  });

  it("lists installed plugins and removes one after confirmation", async () => {
    const user = userEvent.setup();
    const remove = vi.fn(async () => undefined);
    usePluginStore.setState({
      plugins: [
        {
          manifest: {
            id: "sample.wordfreq",
            name: "Word Frequency",
            version: "1.0.0",
            apiVersion: 1,
            compatibility: { app: ">=3.0.0" },
            permissions: ["read_document"],
            capabilities: [],
            commands: [{ id: "count", title: "Count words" }],
          },
          status: "idle",
          error: null,
          lastError: null,
          logs: [],
          lastResult: null,
        },
      ],
      loaded: true,
      load: vi.fn(async () => undefined),
      remove,
    });
    render(<Plugins />);
    expect(await screen.findByText("Word Frequency")).toBeInTheDocument();
    expect(screen.getByText("v1.0.0")).toBeInTheDocument();

    // The first Remove press only asks for confirmation.
    await user.click(screen.getAllByRole("button", { name: "Remove" })[0]);
    expect(remove).not.toHaveBeenCalled();
    const buttons = screen.getAllByRole("button", { name: "Remove" });
    await user.click(buttons[buttons.length - 1]);
    await waitFor(() => expect(remove).toHaveBeenCalledWith("sample.wordfreq"));
  });
});

// ---------------------------------------------------------------------------
// Annotate v4.6: editable annotations, ink, signatures and existing edits
// ---------------------------------------------------------------------------

/** One existing highlight, in the camelCase shape `pdf_list_annotations` sends. */
const existingAnnotationsFixture = [
  {
    page: 1,
    index: 0,
    kind: "highlight",
    x: 50,
    y: 100,
    w: 200,
    h: 18,
    text: "Original",
    color: "#facc15",
    opacity: 0.4,
    lineWidthPt: 2,
    fontSizePt: 12,
    bold: false,
    strokes: [],
  },
];

/** jsdom has no layout: every measured element is a 500 x 700 page. */
function installPreviewRect() {
  return vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
    x: 0,
    y: 0,
    left: 0,
    top: 0,
    right: 500,
    bottom: 700,
    width: 500,
    height: 700,
    toJSON: () => ({}),
  } as DOMRect);
}

function invokeCall<T>(command: string): T | undefined {
  const call = invoke.mock.calls.find(([name]) => name === command);
  return call as unknown as T | undefined;
}

describe("annotate v4.6", () => {
  beforeEach(() => {
    invoke.mockClear();
    listAnnotations = [];
  });

  it("places a sticky note at the clicked point and saves it editable", async () => {
    const rectSpy = installPreviewRect();
    try {
      const user = userEvent.setup();
      render(<Annotate {...props} />);
      await waitFor(() => expect(document.querySelector(".canvas-wrap img")).not.toBeNull());
      await user.click(screen.getByRole("button", { name: "Sticky note" }));
      const wrap = document.querySelector<HTMLElement>(".canvas-wrap")!;
      fireEvent.pointerDown(wrap, { clientX: 100, clientY: 150 });
      fireEvent.pointerUp(wrap, { clientX: 100, clientY: 150 });

      await user.click(screen.getByRole("button", { name: "Save annotations" }));
      await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "pdf_annotate_editable")).toBe(true));
      const call =
        invokeCall<[string, { request: { annotations: { kind: string; x: number; y: number }[] } }]>(
          "pdf_annotate_editable",
        );
      expect(call?.[1].request.annotations).toHaveLength(1);
      expect(call?.[1].request.annotations[0].kind).toBe("note");
      expect(call?.[1].request.annotations[0].x).toBeCloseTo((100 / 500) * 595.28, 1);
      expect(call?.[1].request.annotations[0].y).toBeCloseTo((150 / 700) * 841.89, 1);
    } finally {
      rectSpy.mockRestore();
    }
  });

  it("captures a freehand stroke in display space", async () => {
    const rectSpy = installPreviewRect();
    try {
      const user = userEvent.setup();
      render(<Annotate {...props} />);
      await waitFor(() => expect(document.querySelector(".canvas-wrap img")).not.toBeNull());
      await user.click(screen.getByRole("button", { name: "Freehand" }));
      const layer = document.querySelector<HTMLElement>(".annotate-draw-layer");
      expect(layer).not.toBeNull();
      fireEvent.pointerDown(layer!, { clientX: 50, clientY: 100 });
      fireEvent.pointerMove(layer!, { clientX: 100, clientY: 180 });
      fireEvent.pointerMove(layer!, { clientX: 150, clientY: 220 });
      fireEvent.pointerUp(layer!, { clientX: 150, clientY: 220 });

      await user.click(screen.getByRole("button", { name: "Save annotations" }));
      await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "pdf_annotate_editable")).toBe(true));
      const call =
        invokeCall<[string, { request: { annotations: { kind: string; strokes: number[][][] }[] } }]>(
          "pdf_annotate_editable",
        );
      const annotation = call?.[1].request.annotations[0];
      expect(annotation?.kind).toBe("ink");
      expect(annotation?.strokes).toHaveLength(1);
      expect(annotation?.strokes[0]).toHaveLength(3);
      expect(annotation?.strokes[0][0][0]).toBeCloseTo((50 / 500) * 595.28, 1);
      expect(annotation?.strokes[0][0][1]).toBeCloseTo((100 / 700) * 841.89, 1);
    } finally {
      rectSpy.mockRestore();
    }
  });

  it("turns a drawn signature into a base64 signature annotation", async () => {
    const context = {
      beginPath: vi.fn(),
      moveTo: vi.fn(),
      lineTo: vi.fn(),
      stroke: vi.fn(),
      clearRect: vi.fn(),
      strokeStyle: "",
      lineWidth: 0,
      lineCap: "",
      lineJoin: "",
    } as unknown as CanvasRenderingContext2D;
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(context);
    vi.spyOn(HTMLCanvasElement.prototype, "toDataURL").mockReturnValue("data:image/png;base64,U0lH");
    const rectSpy = installPreviewRect();
    try {
      const user = userEvent.setup();
      render(<Annotate {...props} />);
      await waitFor(() => expect(document.querySelector(".canvas-wrap img")).not.toBeNull());
      await user.click(screen.getByRole("button", { name: "Signature" }));
      await user.click(screen.getByRole("button", { name: "Draw your signature" }));
      const pad = document.querySelector<HTMLCanvasElement>(".annotate-signature-pad");
      expect(pad).not.toBeNull();
      fireEvent.pointerDown(pad!, { clientX: 40, clientY: 60 });
      fireEvent.pointerMove(pad!, { clientX: 120, clientY: 90 });
      fireEvent.pointerUp(pad!, { clientX: 120, clientY: 90 });
      await user.click(screen.getByRole("button", { name: "Use signature" }));

      const wrap = document.querySelector<HTMLElement>(".canvas-wrap")!;
      fireEvent.pointerDown(wrap, { clientX: 200, clientY: 250 });
      fireEvent.pointerUp(wrap, { clientX: 200, clientY: 250 });

      await user.click(screen.getByRole("button", { name: "Save annotations" }));
      await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "pdf_annotate_editable")).toBe(true));
      const call =
        invokeCall<[string, { request: { annotations: { kind: string; image_base64: string }[] } }]>(
          "pdf_annotate_editable",
        );
      expect(call?.[1].request.annotations[0].kind).toBe("signature");
      expect(call?.[1].request.annotations[0].image_base64).toBe("U0lH");
    } finally {
      rectSpy.mockRestore();
    }
  });

  it("loads existing annotations and queues a move edit", async () => {
    listAnnotations = existingAnnotationsFixture;
    const rectSpy = installPreviewRect();
    try {
      const user = userEvent.setup();
      render(<Annotate {...props} />);
      await user.click(await screen.findByText("highlight"));
      const box = await waitFor(() => {
        const element = document.querySelector<HTMLElement>(".annotate-box-selected");
        if (!element) throw new Error("the selected annotation is not rendered");
        return element;
      });
      fireEvent.pointerDown(box, { clientX: 100, clientY: 100 });
      fireEvent.pointerMove(box, { clientX: 160, clientY: 130 });
      fireEvent.pointerUp(box, { clientX: 160, clientY: 130 });

      await user.click(screen.getByRole("button", { name: "Save annotations" }));
      await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "pdf_edit_annotations")).toBe(true));
      const call =
        invokeCall<[string, { request: { edits: { action: string; dx?: number; dy?: number }[] } }]>(
          "pdf_edit_annotations",
        );
      const move = call?.[1].request.edits.find((edit) => edit.action === "move");
      expect(move?.dx).toBeCloseTo((60 / 500) * 595.28, 1);
      expect(move?.dy).toBeCloseTo((30 / 700) * 841.89, 1);
      // No new annotations: the edit pass writes the output directly.
      expect(invoke.mock.calls.some(([name]) => name === "pdf_annotate_editable")).toBe(false);
    } finally {
      rectSpy.mockRestore();
    }
  });

  it("queues text, colour and size edits for the selected annotation", async () => {
    listAnnotations = existingAnnotationsFixture;
    const rectSpy = installPreviewRect();
    try {
      const user = userEvent.setup();
      render(<Annotate {...props} />);
      await user.click(await screen.findByText("highlight"));
      const editor = await screen.findByDisplayValue("Original");
      await user.clear(editor);
      await user.type(editor, "Changed");
      const spin = screen.getAllByRole("spinbutton") as HTMLInputElement[];
      fireEvent.change(spin[0], { target: { value: "300" } });
      fireEvent.change(spin[1], { target: { value: "40" } });
      await user.click(screen.getByRole("button", { name: "Resize" }));

      await user.click(screen.getByRole("button", { name: "Save annotations" }));
      await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "pdf_edit_annotations")).toBe(true));
      const call = invokeCall<[string, { request: { edits: Record<string, unknown>[] } }]>("pdf_edit_annotations");
      expect(call?.[1].request.edits).toContainEqual(
        expect.objectContaining({ page: 1, index: 0, action: "update", text: "Changed" }),
      );
      expect(call?.[1].request.edits).toContainEqual(
        expect.objectContaining({ page: 1, index: 0, action: "resize", x: 50, y: 100, w: 300, h: 40 }),
      );
    } finally {
      rectSpy.mockRestore();
    }
  });

  it("deletes a loaded annotation and sends the delete with the save", async () => {
    listAnnotations = existingAnnotationsFixture;
    const rectSpy = installPreviewRect();
    try {
      const user = userEvent.setup();
      render(<Annotate {...props} />);
      await user.click(await screen.findByText("highlight"));
      await user.click(screen.getByRole("button", { name: "Delete" }));
      expect(screen.getByText("This document has no annotations.")).toBeInTheDocument();

      await user.click(screen.getByRole("button", { name: "Save annotations" }));
      await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "pdf_edit_annotations")).toBe(true));
      const call = invokeCall<[string, { request: { edits: unknown[] } }]>("pdf_edit_annotations");
      expect(call?.[1].request.edits).toEqual([{ page: 1, index: 0, action: "delete" }]);
    } finally {
      rectSpy.mockRestore();
    }
  });

  it("applies existing-annotation edits before adding the new stamps", async () => {
    listAnnotations = existingAnnotationsFixture;
    const rectSpy = installPreviewRect();
    try {
      const user = userEvent.setup();
      render(<Annotate {...props} />);
      await user.click(await screen.findByText("highlight"));
      const box = await waitFor(() => {
        const element = document.querySelector<HTMLElement>(".annotate-box-selected");
        if (!element) throw new Error("the selected annotation is not rendered");
        return element;
      });
      fireEvent.pointerDown(box, { clientX: 100, clientY: 100 });
      fireEvent.pointerMove(box, { clientX: 110, clientY: 110 });
      fireEvent.pointerUp(box, { clientX: 110, clientY: 110 });

      await user.click(screen.getByRole("button", { name: "Sticky note" }));
      const wrap = document.querySelector<HTMLElement>(".canvas-wrap")!;
      fireEvent.pointerDown(wrap, { clientX: 300, clientY: 400 });
      fireEvent.pointerUp(wrap, { clientX: 300, clientY: 400 });

      await user.click(screen.getByRole("button", { name: "Save annotations" }));
      await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "pdf_edit_annotations")).toBe(true));
      const editCall =
        invokeCall<[string, { request: { input: string; output: { path: string }; edits: { action: string }[] } }]>(
          "pdf_edit_annotations",
        );
      expect(editCall?.[1].request.input).toBe("C:/docs/a.pdf");
      expect(editCall?.[1].request.edits[0].action).toBe("move");
      const staged = editCall?.[1].request.output.path ?? "";
      expect(staged).toContain("AnnotateStaging");

      await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "pdf_annotate_editable")).toBe(true));
      const annotateCall =
        invokeCall<[string, { request: { input: string; annotations: { kind: string }[] } }]>("pdf_annotate_editable");
      // The annotate pass reads the file the edit pass wrote.
      expect(annotateCall?.[1].request.input).toBe(staged);
      expect(annotateCall?.[1].request.annotations[0].kind).toBe("note");
    } finally {
      rectSpy.mockRestore();
    }
  });

  it("flattens the stamps through annotate_pdf when editable mode is off", async () => {
    const rectSpy = installPreviewRect();
    try {
      const user = userEvent.setup();
      render(<Annotate {...props} />);
      await waitFor(() => expect(document.querySelector(".canvas-wrap img")).not.toBeNull());
      await user.click(screen.getByRole("tab", { name: "Flatten into the page" }));
      const wrap = document.querySelector<HTMLElement>(".canvas-wrap")!;
      fireEvent.pointerDown(wrap, { clientX: 100, clientY: 150 });
      fireEvent.pointerUp(wrap, { clientX: 100, clientY: 150 });

      await user.click(screen.getByRole("button", { name: "Apply annotations" }));
      await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "annotate_pdf")).toBe(true));
      expect(invoke.mock.calls.some(([name]) => name === "pdf_annotate_editable")).toBe(false);
    } finally {
      rectSpy.mockRestore();
    }
  });
});

// ---------------------------------------------------------------------------
// Organize v4.6: blank pages and the bookmark (outline) editor
// ---------------------------------------------------------------------------

describe("organize v4.6 blank pages and bookmarks", () => {
  beforeEach(() => {
    invoke.mockClear();
    outlineFixture = [];
    listAnnotations = [];
  });

  it("adds a blank page without a thumbnail request and sends it in the plan", async () => {
    // Fire every IntersectionObserver immediately so real thumbnails are
    // actually requested; page 0 must never be among them.
    const originalObserver = globalThis.IntersectionObserver;
    globalThis.IntersectionObserver = class {
      root = null;
      rootMargin = "";
      thresholds: number[] = [];
      constructor(private readonly callback: IntersectionObserverCallback) {}
      observe(target: Element) {
        this.callback(
          [{ isIntersecting: true, target } as unknown as IntersectionObserverEntry],
          this as unknown as IntersectionObserver,
        );
      }
      unobserve() {}
      disconnect() {}
      takeRecords() {
        return [];
      }
    } as unknown as typeof IntersectionObserver;
    try {
      const user = userEvent.setup();
      render(<Organize {...props} />);
      await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "page_thumbnail")).toBe(true));

      await user.click(await screen.findByRole("button", { name: "Add blank page" }));
      expect(screen.getByText("Blank page")).toBeInTheDocument();
      const requestedPages = (
        invoke.mock.calls.filter(([name]) => name === "page_thumbnail") as unknown as [string, { page: number }][]
      ).map(([, payload]) => payload.page);
      expect([...requestedPages].sort()).toEqual([1, 2]);

      const runButton = screen.getByRole("button", { name: "Save as PDF" });
      await waitFor(() => expect(runButton).toBeEnabled());
      await user.click(runButton);
      await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "apply_page_plan")).toBe(true));
      const call = invokeCall<[string, { request: { plan: unknown[] } }]>("apply_page_plan");
      expect(call?.[1].request.plan).toEqual([
        { source_page: 1, rotation_delta: 0 },
        { source_page: 2, rotation_delta: 0 },
        { source_page: 0, rotation_delta: 0, blank: true, width_pt: null, height_pt: null },
      ]);
    } finally {
      globalThis.IntersectionObserver = originalObserver;
    }
  });

  it("loads bookmark rows and saves them through pdf_set_outline", async () => {
    outlineFixture = [
      { title: "Intro", page: 1, depth: 0 },
      { title: "Details", page: 2, depth: 1 },
    ];
    const user = userEvent.setup();
    render(<Organize {...props} />);
    expect(await screen.findByDisplayValue("Intro")).toBeInTheDocument();
    expect(screen.getByDisplayValue("Details")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Save bookmarks" }));
    await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "pdf_set_outline")).toBe(true));
    const call =
      invokeCall<[string, { request: { input: string; output: { path: string }; entries: unknown[]; jobId: string } }]>(
        "pdf_set_outline",
      );
    expect(call?.[1].request).toEqual({
      input: "C:/docs/a.pdf",
      output: { path: "C:/docs/out.pdf", overwrite: "error" },
      entries: [
        { title: "Intro", page: 1, depth: 0 },
        { title: "Details", page: 2, depth: 1 },
      ],
      password: undefined,
      jobId: expect.any(String),
    });
    expect(await screen.findByText("Bookmarks saved")).toBeInTheDocument();
  });

  it("shows the empty outline state and supports adding and removing rows", async () => {
    const user = userEvent.setup();
    render(<Organize {...props} />);
    expect(await screen.findByText("No bookmarks in this document.")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Add bookmark" }));
    expect(screen.queryByText("No bookmarks in this document.")).toBeNull();
    const title = screen.getByLabelText("Title 1");
    await user.type(title, "Chapter 1");
    expect(title).toHaveValue("Chapter 1");

    await user.click(screen.getByRole("button", { name: "Remove" }));
    expect(screen.getByText("No bookmarks in this document.")).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Page tools v4.6: header/footer + Bates stamping and N-up imposition
// ---------------------------------------------------------------------------

describe("page tools v4.6 stamp and nup tabs", () => {
  beforeEach(() => {
    invoke.mockClear();
  });

  it("switches between the stamp and N-up forms", async () => {
    const user = userEvent.setup();
    render(<PageTools tab="extract" initialFiles={["C:/docs/a.pdf"]} dragging={false} />);
    await screen.findByRole("button", { name: "Extract pages" });

    await user.click(screen.getByRole("tab", { name: "Header, footer & Bates" }));
    expect(await screen.findByRole("button", { name: "Stamp header/footer and Bates" })).toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: "N-up & booklet" }));
    expect(await screen.findByRole("button", { name: "Create N-up PDF" })).toBeInTheDocument();
    expect(screen.getByText("Pages per sheet")).toBeInTheDocument();
  });

  it("stamps header text with the exact camelCase payload and a null Bates part", async () => {
    const user = userEvent.setup();
    render(<PageTools tab="stamp" initialFiles={["C:/docs/a.pdf"]} dragging={false} />);
    const runButton = await screen.findByRole("button", { name: "Stamp header/footer and Bates" });
    // Both the header/footer and Bates are empty: nothing to apply.
    expect(runButton).toBeDisabled();

    fireEvent.change(screen.getByLabelText("Header left"), { target: { value: "Internal" } });
    await waitFor(() => expect(runButton).toBeEnabled());
    await user.click(runButton);
    await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "stamp_pdf")).toBe(true));

    const request =
      invokeCall<[string, { request: { headerFooter: unknown; bates: unknown } }]>("stamp_pdf")?.[1].request;
    expect(request?.headerFooter).toEqual({
      headerLeft: "Internal",
      headerCenter: "",
      headerRight: "",
      footerLeft: "",
      footerCenter: "",
      footerRight: "",
      fontSizePt: 10,
      color: "#333333",
      marginPt: 28,
      pages: [],
      startNumber: 1,
      countFromStart: true,
    });
    expect(request?.bates).toBeNull();
  });

  it("stamps Bates numbering alone, leaving the header/footer part null", async () => {
    const user = userEvent.setup();
    render(<PageTools tab="stamp" initialFiles={["C:/docs/a.pdf"]} dragging={false} />);
    const runButton = await screen.findByRole("button", { name: "Stamp header/footer and Bates" });

    fireEvent.change(screen.getByLabelText("Bates prefix"), { target: { value: "CASE-" } });
    await waitFor(() => expect(runButton).toBeEnabled());
    await user.click(runButton);
    await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "stamp_pdf")).toBe(true));

    const request =
      invokeCall<[string, { request: { headerFooter: unknown; bates: unknown } }]>("stamp_pdf")?.[1].request;
    expect(request?.headerFooter).toBeNull();
    expect(request?.bates).toEqual({
      prefix: "CASE-",
      suffix: "",
      start: 1,
      digits: 6,
      position: "bottomRight",
      fontSizePt: 10,
      color: "#333333",
      marginPt: 28,
      pages: [],
    });
  });

  it("creates an N-up PDF with the exact options payload", async () => {
    const user = userEvent.setup();
    render(<PageTools tab="nup" initialFiles={["C:/docs/a.pdf"]} dragging={false} />);
    const runButton = await screen.findByRole("button", { name: "Create N-up PDF" });
    await waitFor(() => expect(runButton).toBeEnabled());

    await user.click(screen.getByRole("tab", { name: "4" }));
    await user.click(screen.getByRole("tab", { name: "Landscape" }));
    await user.selectOptions(screen.getByRole("combobox"), "a4");
    await user.click(screen.getByRole("checkbox", { name: "Draw page borders" }));
    await user.type(screen.getByPlaceholderText("All"), "1");
    await user.click(runButton);
    await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "nup_pdf")).toBe(true));

    const request = invokeCall<[string, { request: { options: unknown } }]>("nup_pdf")?.[1].request;
    expect(request?.options).toEqual({
      perSheet: 4,
      booklet: false,
      orientation: "landscape",
      pageSize: "a4",
      marginPt: 18,
      gutterPt: 0,
      border: true,
      pages: [1],
    });
  });

  it("disables booklet ordering for 4-up sheets", async () => {
    const user = userEvent.setup();
    render(<PageTools tab="nup" initialFiles={["C:/docs/a.pdf"]} dragging={false} />);
    await screen.findByRole("button", { name: "Create N-up PDF" });
    const booklet = screen.getByRole("checkbox", { name: "Booklet order (saddle stitch)" });
    await user.click(booklet);
    expect(booklet).toBeChecked();

    await user.click(screen.getByRole("tab", { name: "4" }));
    await waitFor(() => expect(booklet).not.toBeChecked());
    expect(booklet.closest('[aria-disabled="true"]')).not.toBeNull();

    // Even a direct click cannot re-enable it while 4-up is selected.
    await user.click(booklet);
    expect(booklet).not.toBeChecked();
  });
});
