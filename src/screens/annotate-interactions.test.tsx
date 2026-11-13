import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

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

let savedSignatures: { name: string; imageBase64: string }[] = [];
let existingAnnotations: Record<string, unknown>[] = [];

const invoke = vi.fn(async (command: string, payload?: Record<string, unknown>) => {
  switch (command) {
    case "pdf_info":
      return pdfInfo;
    case "page_preview":
    case "page_thumbnail":
      return { dataUrl: "data:image/png;base64,iVBORw0KGgo=", width: 595, height: 842 };
    case "file_sizes":
      return [2048];
    case "suggest_output":
      return { token: "C:/docs/out.pdf", path: "C:/docs/out.pdf" };
    case "output_exists":
      return false;
    case "signature_list":
      return savedSignatures;
    case "signature_save": {
      const entry = { name: String(payload?.name ?? "Signature"), imageBase64: String(payload?.imageBase64 ?? "") };
      savedSignatures = [...savedSignatures.filter((item) => item.name !== entry.name), entry];
      return null;
    }
    case "signature_delete":
      savedSignatures = savedSignatures.filter((item) => item.name !== payload?.name);
      return null;
    case "file_token_pick":
      return [{ token: "C:/img/stamp.png", name: "stamp.png" }];
    case "pdf_list_annotations":
      return existingAnnotations;
    case "pdf_annotate_editable":
    case "annotate_pdf":
      return { path: "C:/docs/out.pdf", pageCount: 2, originalBytes: 4096, outputBytes: 2048, message: "ok" };
    case "pdf_edit_annotations":
      return { edited: 1, deleted: 1, warnings: [] };
    default:
      return null;
  }
});

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invoke(...(args as [string, Record<string, unknown>])),
}));
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

if (typeof window.PointerEvent === "undefined") {
  window.PointerEvent = MouseEvent as unknown as typeof PointerEvent;
}
if (!HTMLElement.prototype.setPointerCapture) {
  HTMLElement.prototype.setPointerCapture = () => undefined;
}

import { Annotate } from "./Annotate";
import { useSettings, useToasts } from "../lib/store";
import { DEFAULT_SETTINGS } from "../lib/types";

const props = { dragging: false, initialFiles: ["C:/docs/a.pdf"] };

describe("Annotate interactions", () => {
  beforeEach(() => {
    invoke.mockClear();
    savedSignatures = [];
    existingAnnotations = [];
    useToasts.setState({ toasts: [] });
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS, language: "en" } });
  });

  it("draws freehand ink strokes, rectangles and lines on the draw layer", async () => {
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
      await waitFor(() => expect(document.querySelector(".canvas-wrap img")).not.toBeNull());

      await user.click(screen.getByRole("button", { name: "Freehand" }));
      const inkLayer = await waitFor(() => document.querySelector<HTMLElement>(".annotate-draw-layer")!);
      fireEvent.pointerDown(inkLayer, { clientX: 40, clientY: 60, pointerId: 1 });
      fireEvent.pointerMove(inkLayer, { clientX: 120, clientY: 140, pointerId: 1 });
      fireEvent.pointerUp(inkLayer, { clientX: 120, clientY: 140, pointerId: 1 });
      expect(screen.getByText("Annotations · 1")).toBeInTheDocument();

      await user.click(screen.getByRole("button", { name: "Rectangle" }));
      const rectLayer = document.querySelector<HTMLElement>(".annotate-draw-layer")!;
      fireEvent.pointerDown(rectLayer, { clientX: 60, clientY: 80, pointerId: 1 });
      fireEvent.pointerMove(rectLayer, { clientX: 200, clientY: 220, pointerId: 1 });
      fireEvent.pointerUp(rectLayer, { clientX: 200, clientY: 220, pointerId: 1 });
      expect(screen.getByText("Annotations · 2")).toBeInTheDocument();

      await user.click(screen.getByRole("button", { name: "Line" }));
      const lineLayer = document.querySelector<HTMLElement>(".annotate-draw-layer")!;
      fireEvent.pointerDown(lineLayer, { clientX: 80, clientY: 100, pointerId: 1 });
      fireEvent.pointerMove(lineLayer, { clientX: 260, clientY: 260, pointerId: 1 });
      fireEvent.pointerUp(lineLayer, { clientX: 260, clientY: 260, pointerId: 1 });
      expect(screen.getByText("Annotations · 3")).toBeInTheDocument();

      await user.click(screen.getByRole("button", { name: "Save annotations" }));
      await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "pdf_annotate_editable")).toBe(true));

      const call = (invoke.mock.calls as [string, { request: { annotations: { kind: string }[] } }][]).find(
        ([name]) => name === "pdf_annotate_editable",
      );
      expect(call?.[1].request.annotations.map((item) => item.kind)).toEqual(["ink", "rect", "line"]);
    } finally {
      rectSpy.mockRestore();
    }
  });

  it("picks an image token and adds ready text stamps", async () => {
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
      await waitFor(() => expect(document.querySelector(".canvas-wrap img")).not.toBeNull());

      await user.click(screen.getByRole("button", { name: "Image" }));
      await user.click(screen.getByRole("button", { name: "Image file" }));

      await waitFor(() =>
        expect(invoke).toHaveBeenCalledWith("file_token_pick", {
          extensions: ["png", "jpg", "jpeg"],
          multiple: false,
        }),
      );
      expect(await screen.findByRole("button", { name: "stamp.png" })).toBeInTheDocument();

      const wrap = document.querySelector<HTMLElement>(".canvas-wrap")!;
      fireEvent.pointerDown(wrap, { clientX: 150, clientY: 200 });
      await user.click(screen.getByRole("button", { name: "ONAYLANDI" }));

      expect(screen.getByText("Annotations · 2")).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: "Save annotations" }));

      await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "pdf_annotate_editable")).toBe(true));
      const call = (
        invoke.mock.calls as [
          string,
          { request: { annotations: { kind: string; image_path?: string | null; text?: string }[] } },
        ][]
      ).find(([name]) => name === "pdf_annotate_editable");
      expect(call?.[1].request.annotations[0].image_path).toBe("C:/img/stamp.png");
      expect(call?.[1].request.annotations[1].text).toContain("ONAYLANDI");
    } finally {
      rectSpy.mockRestore();
    }
  });

  it("selects, saves, paraphes all pages and deletes saved signatures", async () => {
    savedSignatures = [{ name: "Primary", imageBase64: "AQID" }];
    const user = userEvent.setup();
    render(<Annotate {...props} />);

    await waitFor(() => expect(screen.getByText("a.pdf")).toBeInTheDocument());
    await user.click(screen.getByRole("button", { name: "Signature" }));

    const savedButton = await screen.findByTitle("Primary");
    await user.click(savedButton);

    await user.type(screen.getByPlaceholderText("Name (e.g. signature, paraphe)"), "Initials");
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("signature_save", { name: "Initials", imageBase64: "AQID" }),
    );
    expect(useToasts.getState().toasts.map((toast) => toast.title)).toContain("Signature saved");

    await user.click(screen.getByRole("button", { name: "Paraphe every page" }));
    expect(screen.getByText("Annotations · 2")).toBeInTheDocument();
    expect(useToasts.getState().toasts.map((toast) => toast.title)).toContain("Paraphe applied to 2 pages");

    const deleteButtons = screen.getAllByRole("button", { name: "×" });
    await user.click(deleteButtons[0]);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("signature_delete", { name: "Primary" }));
  });

  it("edits text, resizes and deletes existing annotations via pdf_edit_annotations", async () => {
    existingAnnotations = [
      {
        page: 1,
        index: 0,
        kind: "note",
        x: 40,
        y: 60,
        w: 120,
        h: 40,
        text: "Old note",
        color: "#e11d48",
        opacity: 1,
        fontSizePt: 12,
        bold: false,
        lineWidthPt: 1,
        strokes: [],
      },
      {
        page: 1,
        index: 1,
        kind: "rect",
        x: 100,
        y: 120,
        w: 80,
        h: 50,
        text: "",
        color: "#000000",
        opacity: 1,
        fontSizePt: 12,
        bold: false,
        lineWidthPt: 2,
        strokes: [],
      },
    ];

    const user = userEvent.setup();
    render(<Annotate {...props} />);

    const noteItem = await screen.findByRole("button", { name: /Page 1 · Old note/ });
    await user.click(noteItem);
    expect(screen.getByText("Selected annotation")).toBeInTheDocument();

    const editTextarea = screen.getByDisplayValue("Old note");
    await user.clear(editTextarea);
    await user.type(editTextarea, "Updated");

    const dimensions = screen.getAllByRole("spinbutton");
    await user.clear(dimensions[0]);
    await user.type(dimensions[0], "180");
    await user.click(screen.getByRole("button", { name: "Resize" }));

    const rectItem = screen.getByRole("button", { name: /^rect\s*Page 1$/ });
    await user.click(rectItem);
    await user.click(screen.getByRole("button", { name: "Delete" }));

    await user.click(screen.getByRole("button", { name: "Save annotations" }));
    await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "pdf_edit_annotations")).toBe(true));

    const call = (
      invoke.mock.calls as [string, { request: { edits: { index: number; action: string; text?: string }[] } }][]
    ).find(([name]) => name === "pdf_edit_annotations");
    expect(call?.[1].request.edits).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ page: 1, index: 0, action: "update", text: "Updated" }),
        expect.objectContaining({ page: 1, index: 0, action: "resize" }),
        expect.objectContaining({ page: 1, index: 1, action: "delete" }),
      ]),
    );
  });
});
