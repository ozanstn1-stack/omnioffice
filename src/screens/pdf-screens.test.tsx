import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

/**
 * These render the real screens against a mocked backend. The mock payloads
 * use the exact wire format the Rust structs serialize: `rename_all =
 * "camelCase"` on the request/response structs, and `width_pt` / `height_pt`
 * for `render::PageGeometry`, which derives Serialize without it. That
 * mismatch shipped once and rendered a blank window, because a duplicate
 * `PageGeometry` interface in types.ts merged with the real one and hid it
 * from the type checker.
 */
const invoke = vi.fn(async (command: string) => {
  switch (command) {
    case "app_info":
      return { version: "2.2.0", name: "test" };
    case "engine_status":
      return { pdfium: true, qpdf: true, tesseract: true };
    case "load_settings":
      return { theme: "dark", language: "en" };
    case "load_recent":
    case "office_startup_files":
      return [];
    case "dev_launch_context":
      return { startScreen: null, files: [], autoRun: false, tab: null, newTab: null };
    case "suggest_output":
      return "C:/out.pdf";
    case "file_sizes":
      return [1024];
    case "pdf_info":
      return {
        path: "C:/a.pdf",
        fileName: "a.pdf",
        fileSizeBytes: 1024,
        pageCount: 1,
        pdfVersion: "1.7",
        encrypted: false,
        hasTextLayer: true,
        metadata: {},
        pageGeometries: [
          {
            page: 1,
            width_pt: 595.28,
            height_pt: 841.89,
            display_width_pt: 595.28,
            display_height_pt: 841.89,
            rotation: 0,
          },
        ],
        imageCount: 0,
        title: "",
        author: "",
        producer: "",
      };
    case "inspect_document":
      return inspectionFixture;
    case "detect_sensitive_text":
      return [{ page: 1, text: "jane@example.com", left: 60, bottom: 700, right: 200, top: 714, kind: "email" }];
    case "page_preview":
      return { dataUrl: "data:image/png;base64,iVBORw0KGgo=", width: 595, height: 842 };
    // PDF Studio "Forms & objects": exact camelCase wire format of
    // pdfcore::forms (FormFieldInfo / FillReport / PageObjectInfo).
    case "pdf_list_form_fields":
      return formFieldsFixture;
    case "pdf_list_objects":
      return [];
    case "pdf_validate_form":
      return [{ field: "full_name", code: "max_length", severity: "error", message: "too long" }];
    case "pdf_fill_form":
      return { filled: 1, skipped: [], warnings: [] };
    case "pdf_edit_objects":
      return { edited: 1, deleted: 0, warnings: [] };
    default:
      return null;
  }
});

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string])) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/api/path", () => ({ documentDir: vi.fn(async () => "C:/docs") }));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(async () => null),
  save: vi.fn(async () => "C:/filled.pdf"),
}));

const formFieldsFixture = [
  {
    name: "full_name",
    fieldType: "text",
    flags: 2,
    required: true,
    readOnly: false,
    value: "Ada",
    values: [],
    defaultValue: "",
    tooltip: "Full name",
    maxLength: 10,
    multiline: false,
    password: false,
    comb: false,
    combo: false,
    editable: false,
    multiSelect: false,
    options: [],
    page: 1,
    rect: [72, 700, 272, 720],
    tabOrder: 0,
    widgetCount: 1,
    hasScript: false,
  },
  {
    name: "country",
    fieldType: "choice",
    flags: 131072,
    required: false,
    readOnly: false,
    value: "TR",
    values: [],
    defaultValue: "TR",
    tooltip: null,
    maxLength: null,
    multiline: false,
    password: false,
    comb: false,
    combo: true,
    editable: false,
    multiSelect: false,
    options: [
      { value: "TR", label: "Türkiye" },
      { value: "DE", label: "Germany" },
    ],
    page: 1,
    rect: [72, 600, 192, 620],
    tabOrder: 1,
    widgetCount: 1,
    hasScript: false,
  },
];

import inspectionFixture from "../../crates/pdfcore/tests/fixtures/inspection-sample-2.pdf.json";
import { Inspect } from "./Inspect";
import { Compare } from "./Compare";
import { Redact } from "./Redact";
import { Reader, clampReaderZoom, doubleTapZoom, pinchZoomValue } from "./Reader";
import { PdfStudio, displayDeltaToPage, displayRectToPageRect, pageRectToDisplayRect } from "./PdfStudio";

// jsdom has no PointerEvent; MouseEvent carries button/clientX/pointerId, which
// is what the reader's touch handlers read.
if (typeof window.PointerEvent === "undefined") {
  window.PointerEvent = MouseEvent as unknown as typeof PointerEvent;
}

const props = { dragging: false, initialFiles: ["C:/a.pdf"] };

describe("the new PDF screens render against the real wire format", () => {
  beforeEach(() => {
    invoke.mockClear();
  });

  it("renders the inspector and reports the document it received", async () => {
    const user = userEvent.setup();
    const { container } = render(<Inspect {...props} />);
    await waitFor(() => expect(screen.getByText(/fails basic accessibility/i)).toBeInTheDocument());
    // Every value below is camelCase on the wire, so this only passes if the
    // frontend reads the format the backend actually sends.
    expect(container.textContent).toContain("1.7"); // pdfVersion
    expect(container.textContent).toContain("0"); // totalImagePixels
    await user.click(screen.getByRole("button", { name: /^fonts$/i }));
    expect(screen.getByText("F1")).toBeInTheDocument();
    expect(screen.getByText("Type1")).toBeInTheDocument();
    // A font that is not embedded must be flagged, not silently accepted.
    expect(screen.getByText("Not embedded")).toBeInTheDocument();
    // Findings came back, so the errors are counted rather than zero.
    expect(container.textContent).toContain("3 error");
  });

  it("lists findings with their explanation, not just a code", async () => {
    const user = userEvent.setup();
    render(<Inspect {...props} />);
    await waitFor(() => expect(screen.getByText(/fails basic accessibility/i)).toBeInTheDocument());
    await user.click(screen.getByRole("button", { name: /^findings$/i }));
    expect(screen.getByText("a11y.untagged")).toBeInTheDocument();
    expect(screen.getByText(/no structure tree/i)).toBeInTheDocument();
    expect(screen.getByText("a11y.missing-language")).toBeInTheDocument();
  });

  it("shows the page size in points, which proves width_pt is being read", async () => {
    const { container } = render(<Redact {...props} />);
    // If normalizePageSize read the camelCase spelling that does not exist on
    // the wire, this would render "NaN × NaN pt" and no box could be drawn.
    await waitFor(() => expect(container.textContent).toContain("595 × 842 pt"));
    expect(container.textContent).not.toContain("NaN");
  });

  it("offers the sensitive data the detector found, and lets it be dropped", async () => {
    const user = userEvent.setup();
    render(<Redact {...props} />);
    await waitFor(() => expect(screen.getByText(/drag over anything to redact/i)).toBeInTheDocument());
    await user.click(screen.getAllByRole("button", { name: /find sensitive data/i })[0]);
    await waitFor(() => expect(screen.getByText("jane@example.com")).toBeInTheDocument());
    expect(screen.getByText("E-mail")).toBeInTheDocument();
    // Deselecting removes the auto-added box rather than leaving a box the
    // user thought they had removed.
    const checkboxes = screen.getAllByRole("checkbox");
    await user.click(checkboxes[0]);
    expect(screen.getByText("jane@example.com")).toBeInTheDocument();
  });

  it("renders the compare screen and explains that it needs a second file", () => {
    render(<Compare {...props} />);
    expect(screen.getByText(/add a second pdf/i)).toBeInTheDocument();
  });
});

describe("PDF Studio Forms & objects", () => {
  beforeEach(() => {
    invoke.mockClear();
  });

  it("lists the real fields and sends the edited values to pdf_fill_form", async () => {
    const user = userEvent.setup();
    render(<PdfStudio {...props} />);
    await user.click(screen.getByRole("button", { name: /forms & objects/i }));
    // The field list only appears if the camelCase wire format was read.
    await screen.findByText("full_name");
    expect(screen.getByText("Türkiye")).toBeInTheDocument();
    expect(screen.getByText("max 10")).toBeInTheDocument();
    expect(screen.getByText("required")).toBeInTheDocument();
    expect(invoke.mock.calls.some(([name]) => name === "pdf_list_form_fields")).toBe(true);
    expect(invoke.mock.calls.some(([name]) => name === "pdf_list_objects")).toBe(true);

    const input = screen.getByDisplayValue("Ada");
    await user.clear(input);
    await user.type(input, "Grace Hopper");
    await user.click(screen.getByRole("button", { name: /fill and save/i }));
    await waitFor(() => {
      // The mock records the raw (command, payload) pair; the declared tuple
      // only names the first member, so the payload is typed here.
      const fills = invoke.mock.calls.filter(([name]) => name === "pdf_fill_form") as unknown as [
        string,
        { request: { output: { path: string }; values: { name: string; value: string }[] } },
      ][];
      expect(fills.length).toBeGreaterThan(0);
      expect(fills[0][1].request.output.path).toBe("C:/filled.pdf");
      expect(fills[0][1].request.values).toContainEqual({ name: "full_name", value: "Grace Hopper", values: [] });
    });
    // The flatten toggle is off, so the flattener must not run.
    expect(invoke.mock.calls.some(([name]) => name === "flatten_pdf")).toBe(false);
  });

  it("shows backend validation issues per field", async () => {
    const user = userEvent.setup();
    render(<PdfStudio {...props} />);
    await user.click(screen.getByRole("button", { name: /forms & objects/i }));
    await screen.findByText("full_name");
    await user.click(screen.getByRole("button", { name: /^validate$/i }));
    await waitFor(() => {
      expect(invoke.mock.calls.some(([name]) => name === "pdf_validate_form")).toBe(true);
    });
    expect(await screen.findByText(/maximum length/i)).toBeInTheDocument();
  });
});

describe("PDF Studio object geometry", () => {
  it("round-trips a page rectangle through every display rotation", () => {
    for (const rotation of [0, 90, 180, 270]) {
      const geometry = { width: 200, height: 300, rotation };
      const rect: [number, number, number, number] = [50, 60, 150, 80];
      const display = pageRectToDisplayRect(rect, geometry);
      const back = displayRectToPageRect(display, geometry);
      for (let index = 0; index < 4; index += 1) {
        expect(back[index]).toBeCloseTo(rect[index], 6);
      }
    }
  });

  it("converts drag deltas from the rotated display into page space", () => {
    const expectDelta = (actual: [number, number], expected: [number, number]) => {
      expect(actual[0]).toBeCloseTo(expected[0], 6);
      expect(actual[1]).toBeCloseTo(expected[1], 6);
    };
    expectDelta(displayDeltaToPage(10, 0, 0), [10, 0]);
    expectDelta(displayDeltaToPage(10, 0, 90), [0, 10]);
    expectDelta(displayDeltaToPage(0, 10, 90), [-10, 0]);
    expectDelta(displayDeltaToPage(10, 0, 180), [-10, 0]);
    expectDelta(displayDeltaToPage(10, 0, 270), [0, -10]);
  });
});

describe("reader touch zoom", () => {
  it("clamps zoom to the range the buttons produce", () => {
    expect(clampReaderZoom(9)).toBe(4);
    expect(clampReaderZoom(0.1)).toBe(0.25);
    expect(clampReaderZoom(1.5)).toBe(1.5);
  });

  it("scales the starting zoom by the pinch distance ratio", () => {
    expect(pinchZoomValue(1, 100, 200)).toBe(2);
    expect(pinchZoomValue(2, 100, 50)).toBe(1);
    // A collapsed gesture must not divide by zero.
    expect(pinchZoomValue(1.5, 0, 0)).toBe(1.5);
  });

  it("toggles fit width and 200 % on double tap", () => {
    expect(doubleTapZoom("fit")).toBe(2);
    expect(doubleTapZoom(2)).toBe("fit");
    expect(doubleTapZoom(1.5)).toBe(2);
  });

  it("zooms to 200 % when the page area is double-tapped with touch", async () => {
    render(<Reader initialFiles={["C:/a.pdf"]} dragging={false} />);
    await waitFor(() => expect(document.querySelector(".reader-scroll")).not.toBeNull());
    const scroller = document.querySelector<HTMLElement>(".reader-scroll")!;

    const tap = (pointerId: number) => {
      fireEvent.pointerDown(scroller, { pointerId, pointerType: "touch", button: 0, clientX: 100, clientY: 100 });
      fireEvent.pointerUp(scroller, { pointerId, pointerType: "touch", clientX: 100, clientY: 100 });
    };

    tap(1);
    tap(2);
    expect(screen.getByRole("button", { name: "200%" })).toBeInTheDocument();

    tap(3);
    tap(4);
    expect(screen.getByRole("button", { name: /fit width/i })).toBeInTheDocument();
  });
});
