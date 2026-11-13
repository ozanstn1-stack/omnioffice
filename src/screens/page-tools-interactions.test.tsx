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

const opResult = {
  path: "C:/docs/out.pdf",
  pageCount: 2,
  originalBytes: 4096,
  outputBytes: 2048,
  message: "ok",
};

const invoke = vi.fn(async (command: string, _payload?: unknown) => {
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
    case "delete_pages":
    case "crop_pages":
    case "stamp_pdf":
    case "add_page_numbers":
    case "nup_pdf":
      return opResult;
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

if (typeof window.PointerEvent === "undefined") {
  window.PointerEvent = MouseEvent as unknown as typeof PointerEvent;
}
if (!HTMLElement.prototype.setPointerCapture) {
  HTMLElement.prototype.setPointerCapture = () => undefined;
}

import { PageTools } from "./PageTools";
import { useSettings, useToasts } from "../lib/store";
import { DEFAULT_SETTINGS } from "../lib/types";

const props = { dragging: false, initialFiles: ["C:/docs/a.pdf"] };

describe("PageTools interactions", () => {
  beforeEach(() => {
    invoke.mockClear();
    useToasts.setState({ toasts: [] });
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS, language: "en" } });
  });

  it("confirms and deletes selected pages through delete_pages", async () => {
    const user = userEvent.setup();
    render(<PageTools {...props} tab="delete" />);

    const selectionInput = await screen.findByPlaceholderText("1,3,5-8");
    const runButton = screen.getByRole("button", { name: "Delete pages" });
    expect(runButton).toBeDisabled();

    await user.type(selectionInput, "1,2");
    await waitFor(() => expect(runButton).toBeEnabled());
    await user.click(runButton);

    expect(screen.getByText("Delete 2 selected pages?")).toBeInTheDocument();
    expect(invoke.mock.calls.some(([name]) => name === "delete_pages")).toBe(false);

    await user.click(screen.getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "delete_pages")).toBe(true));

    const call = (invoke.mock.calls as [string, { request: { selection: string } }][]).find(
      ([name]) => name === "delete_pages",
    );
    expect(call?.[1].request.selection).toBe("1,2");
  });

  it("draws a crop rectangle, applies All pages and clears selection", async () => {
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
      render(<PageTools {...props} tab="crop" />);

      await waitFor(() => expect(document.querySelector(".canvas-wrap img")).not.toBeNull());
      const wrap = document.querySelector<HTMLElement>(".canvas-wrap")!;

      const applyButton = screen.getByRole("button", { name: "Crop and save" });
      const clearButton = screen.getByRole("button", { name: "Clear selection" });
      expect(applyButton).toBeDisabled();
      expect(clearButton).toBeDisabled();

      fireEvent.pointerDown(wrap, { clientX: 50, clientY: 70, pointerId: 1 });
      fireEvent.pointerMove(wrap, { clientX: 250, clientY: 350, pointerId: 1 });
      fireEvent.pointerUp(wrap, { clientX: 250, clientY: 350, pointerId: 1 });

      await waitFor(() => expect(applyButton).toBeEnabled());
      expect(clearButton).toBeEnabled();

      await user.click(screen.getByRole("checkbox", { name: "All" }));
      await user.click(applyButton);

      await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "crop_pages")).toBe(true));
      const call = (
        invoke.mock.calls as [string, { request: { crops: { page: number; w: number; h: number }[] } }][]
      ).find(([name]) => name === "crop_pages");
      expect(call?.[1].request.crops).toHaveLength(2);
      expect(call?.[1].request.crops.map((item) => item.page)).toEqual([1, 2]);
      expect(call?.[1].request.crops[0].w).toBeGreaterThan(0);

      await user.click(clearButton);
      expect(applyButton).toBeDisabled();
    } finally {
      rectSpy.mockRestore();
    }
  });

  it("stamps headers and Bates prefixes with keepSignatures toggled off", async () => {
    const user = userEvent.setup();
    render(<PageTools {...props} tab="stamp" />);

    const stampButton = await screen.findByRole("button", { name: "Stamp header/footer and Bates" });
    expect(stampButton).toBeDisabled();

    await user.type(screen.getByLabelText("Header left"), "CONFIDENTIAL");
    await user.type(screen.getByLabelText("Bates prefix"), "DOC-");
    await user.click(screen.getByRole("checkbox", { name: "Keep existing signatures" }));

    await waitFor(() => expect(stampButton).toBeEnabled());
    await user.click(stampButton);

    await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "stamp_pdf")).toBe(true));
    const call = (
      invoke.mock.calls as [
        string,
        {
          request: {
            headerFooter: { headerLeft: string } | null;
            bates: { prefix: string } | null;
            keepSignatures: boolean;
          };
        },
      ][]
    ).find(([name]) => name === "stamp_pdf");
    expect(call?.[1].request.headerFooter?.headerLeft).toBe("CONFIDENTIAL");
    expect(call?.[1].request.bates?.prefix).toBe("DOC-");
    expect(call?.[1].request.keepSignatures).toBe(false);
  });

  it("applies page numbers with chosen format and keepSignatures state", async () => {
    const user = userEvent.setup();
    render(<PageTools {...props} tab="numbering" />);

    const runButton = await screen.findByRole("button", { name: "Add page numbers" });
    await waitFor(() => expect(runButton).toBeEnabled());

    const formatSelect = screen.getByDisplayValue("1, 2, 3");
    await user.selectOptions(formatSelect, "n_of_total");
    await user.click(screen.getByRole("checkbox", { name: "Keep existing signatures" }));
    await user.click(runButton);

    await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "add_page_numbers")).toBe(true));
    const call = (
      invoke.mock.calls as [string, { request: { options: { format: string }; keepSignatures: boolean } }][]
    ).find(([name]) => name === "add_page_numbers");
    expect(call?.[1].request.options.format).toBe("n_of_total");
    expect(call?.[1].request.keepSignatures).toBe(false);
  });

  it("clears and disables booklet order when switching N-up from 2 to 4 per sheet", async () => {
    const user = userEvent.setup();
    render(<PageTools {...props} tab="nup" />);

    const bookletToggle = await screen.findByRole("checkbox", { name: "Booklet order (saddle stitch)" });
    await user.click(bookletToggle);
    expect(bookletToggle).toBeChecked();

    await user.click(screen.getByRole("tab", { name: "4" }));
    expect(bookletToggle).not.toBeChecked();
    expect(bookletToggle.closest("[aria-disabled='true']")).not.toBeNull();

    const runButton = screen.getByRole("button", { name: "Create N-up PDF" });
    await waitFor(() => expect(runButton).toBeEnabled());
    await user.click(runButton);

    await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "nup_pdf")).toBe(true));
    const call = (
      invoke.mock.calls as [string, { request: { options: { perSheet: number; booklet: boolean } } }][]
    ).find(([name]) => name === "nup_pdf");
    expect(call?.[1].request.options.perSheet).toBe(4);
    expect(call?.[1].request.options.booklet).toBe(false);
  });
});
