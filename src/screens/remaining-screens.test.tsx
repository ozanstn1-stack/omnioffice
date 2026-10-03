import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

/**
 * Render + interaction coverage for the last screens without tests:
 * Organize, Annotate, Batch, Page Tools and Plugins.
 */
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
      return pdfInfo;
    case "page_preview":
    case "page_thumbnail":
      return { dataUrl: "data:image/png;base64,iVBORw0KGgo=", width: 595, height: 842 };
    case "compress_pdf":
    case "extract_pages":
    case "apply_page_plan":
    case "annotate_pdf":
      return { path: "C:/docs/out.pdf", pageCount: 2, originalBytes: 4096, outputBytes: 2048, message: "ok" };
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

      const runButton = screen.getByRole("button", { name: "Apply annotations" });
      await user.click(runButton);
      await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "annotate_pdf")).toBe(true));
      const call = (invoke.mock.calls as unknown as [string, { request: { annotations: { text: string }[] } }][]).find(
        ([name]) => name === "annotate_pdf",
      );
      expect(call?.[1].request.annotations).toHaveLength(1);
      expect(call?.[1].request.annotations[0].text).toBe("CONFIDENTIAL");
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
