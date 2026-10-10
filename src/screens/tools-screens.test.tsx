import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

/**
 * Smoke and wire-format tests for the tool screens that had no render coverage.
 * The mock answers the backend commands these screens actually call; payload
 * assertions pin the `request` envelope the Rust commands deserialize, so a
 * renamed field fails here instead of at runtime.
 */
const invoke = vi.fn(async (command: string, _payload?: unknown) => {
  switch (command) {
    case "app_info":
      return { appVersion: "3.5.0", coreVersion: "3.5.0", platform: "windows" };
    case "engine_status":
      return { pdfium: true, qpdf: true, tesseract: true, tesseract_version: "5.0", ocr_languages: ["eng"] };
    case "ocr_languages":
      return [{ code: "eng", name: "English" }];
    case "load_settings":
      return { theme: "dark", language: "en", defaultCompression: "medium", showRecentFiles: true };
    case "save_settings":
      return null;
    case "load_recent":
      return [
        { path: "C:/docs/report.pdf", fileName: "report.pdf", tool: "_merged", timestamp: 1_700_000_000 },
        { path: "C:/docs/notes.txt", fileName: "notes.txt", tool: "_info", timestamp: 1_700_000_100 },
        { path: "C:/docs/budget.xlsx", fileName: "budget.xlsx", tool: "_office", timestamp: 1_700_000_200 },
      ];
    case "clear_recent":
      return null;
    case "office_open_document":
      return {
        kind: "calc",
        title: "budget",
        path: "C:/docs/budget.xlsx",
        model: { sheets: [], activeSheet: 0 },
        warnings: [],
        legacy: false,
      };
    case "load_operations":
      return [
        {
          id: "op-1",
          createdAt: 1_700_000_000,
          operation: "merged",
          inputPath: "C:/docs/a.pdf",
          outputPath: "C:/docs/out.pdf",
          pageCount: 2,
          inputBytes: 4096,
          outputBytes: 2048,
          ok: true,
        },
        {
          id: "op-2",
          createdAt: 1_700_000_100,
          operation: "watermark",
          inputPath: "C:/docs/in,voice.pdf",
          outputPath: 'C:/docs/in "final"\n.pdf',
          pageCount: 1,
          inputBytes: 100,
          outputBytes: 120,
          ok: false,
          detail: 'failed, "please retry"',
        },
        {
          id: "op-3",
          createdAt: 1_700_000_200,
          operation: "compressed",
          inputPath: "C:/docs/big.pdf",
          outputPath: "C:/docs/small.pdf",
          pageCount: 10,
          inputBytes: 900,
          outputBytes: 300,
          ok: true,
        },
      ];
    case "clear_operations":
      return null;
    case "file_sizes":
      return [2048];
    case "suggest_output":
      return "C:/docs/out.pdf";
    case "output_exists":
      return false;
    case "pdf_info":
      return pdfInfo;
    case "page_preview":
    case "page_thumbnail":
      return { dataUrl: "data:image/png;base64,iVBORw0KGgo=", width: 595, height: 842 };
    case "estimate_compression":
      return {
        original_bytes: 4096,
        estimated_bytes: 2048,
        page_count: 2,
        reduction: 0.5,
        method: "raster",
        sample_pages: 1,
        accurate: false,
      };
    case "split_pdf":
      return {
        parts: [
          { path: "C:/docs/out_part1.pdf", first_page: 1, last_page: 5 },
          { path: "C:/docs/out_part2.pdf", first_page: 6, last_page: 10 },
        ],
        outputDir: "C:/docs",
      };
    // Every operation returns the same OpResult shape the ResultCard renders.
    case "merge_pdfs":
    case "compress_pdf":
    case "protect_pdf":
    case "unlock_pdf":
    case "watermark_pdf":
    case "annotate_pdf":
    case "edit_metadata":
      return {
        path: "C:/docs/out.pdf",
        pageCount: 2,
        originalBytes: 4096,
        outputBytes: 2048,
        reduction: 0.5,
        message: "ok",
      };
    case "ai_get_settings":
      return {
        provider: "deepseek",
        baseUrl: "https://api.deepseek.com",
        model: "deepseek-v4",
        temperature: 0.7,
        maxTokens: 4096,
        thinking: false,
        reasoningEffort: "high",
        contextTokens: 200_000,
        maxOutputTokens: 384_000,
        keyPresent: false,
        storage: "none",
        capabilities: {},
      };
    case "ai_models":
      return [];
    case "ai_library_default_dir":
      return "C:/docs/AI";
    case "diagnostics_report":
      return "OmniOffice diagnostics\nVersion: 3.9.0\n";
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
const writeFile = vi.fn(async (_path: string, _bytes: Uint8Array) => undefined);
vi.mock("@tauri-apps/plugin-fs", () => ({
  readFile: vi.fn(async () => new Uint8Array()),
  writeFile: (path: string, bytes: Uint8Array) => writeFile(path, bytes),
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
    title: "Quarterly Report",
    author: "Ada Lovelace",
    subject: "Finance",
    keywords: "quarterly",
    creator: "OSAK",
    producer: "OSAK",
    creation_date: "2026-01-01",
    mod_date: "2026-02-01",
  },
  pageGeometries: [
    { page: 1, width_pt: 595.28, height_pt: 841.89, display_width_pt: 595.28, display_height_pt: 841.89, rotation: 0 },
    { page: 2, width_pt: 595.28, height_pt: 841.89, display_width_pt: 595.28, display_height_pt: 841.89, rotation: 0 },
  ],
  imageCount: 0,
  title: "Quarterly Report",
  author: "Ada Lovelace",
  producer: "OSAK",
};

import { Merge } from "./Merge";
import { Split } from "./Split";
import { Compress } from "./Compress";
import { Security } from "./Security";
import { Watermark } from "./Watermark";
import { Metadata } from "./Metadata";
import { InfoScreen } from "./Info";
import { History, csvField, filterOperations, operationsToCsv } from "./History";
import { Home } from "./Home";
import { Settings } from "./Settings";
import { useToasts } from "../lib/store";
import { useOfficeTabs } from "../lib/office-store";

const props = { dragging: false, initialFiles: ["C:/docs/a.pdf"] };

/** The payload of the single invoke call for `command` (fails when absent). */
function requestOf<T>(command: string): T {
  const call = (invoke.mock.calls as unknown as [string, unknown][]).find(([name]) => name === command);
  if (!call) throw new Error(`${command} was not invoked`);
  return call[1] as T;
}

describe("PDF tool screens call their backend with the request envelope", () => {
  beforeEach(() => {
    invoke.mockClear();
  });

  it("merges every listed file in order through merge_pdfs", async () => {
    const user = userEvent.setup();
    render(<Merge initialFiles={["C:/docs/a.pdf", "C:/docs/b.pdf"]} dragging={false} />);
    const runButton = await screen.findByRole("button", { name: "Merge PDFs" });
    await waitFor(() => expect(runButton).toBeEnabled());
    await user.click(runButton);
    await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "merge_pdfs")).toBe(true));
    const request = requestOf<{ request: { inputs: string[]; output: { path: string } } }>("merge_pdfs").request;
    expect(request.inputs).toEqual(["C:/docs/a.pdf", "C:/docs/b.pdf"]);
    expect(request.output.path).toBe("C:/docs/out.pdf");
  });

  it("runs the default ranges split through split_pdf and lists the parts", async () => {
    const user = userEvent.setup();
    render(<Split {...props} />);
    const runButton = await screen.findByRole("button", { name: "Split PDF" });
    await waitFor(() => expect(runButton).toBeEnabled());
    await user.click(runButton);
    await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "split_pdf")).toBe(true));
    const request = requestOf<{ request: { mode: { mode: string; ranges: string[] } } }>("split_pdf").request;
    expect(request.mode).toEqual({ mode: "ranges", ranges: ["1-5", "6-10"] });
    // The parts the backend returned are rendered, not just the result count.
    expect(await screen.findByText(/out_part1\.pdf/)).toBeInTheDocument();
  });

  it("compresses with the preset from settings and the request envelope", async () => {
    const user = userEvent.setup();
    render(<Compress {...props} />);
    const runButton = await screen.findByRole("button", { name: "Compress PDF" });
    await waitFor(() => expect(runButton).toBeEnabled());
    await user.click(runButton);
    await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "compress_pdf")).toBe(true));
    const request = requestOf<{ request: { options: { preset: string; strategy: string }; input: string } }>(
      "compress_pdf",
    ).request;
    expect(request.input).toBe("C:/docs/a.pdf");
    expect(request.options.preset).toBe("medium");
  });

  it("protects a document with the entered password and permissions", async () => {
    const user = userEvent.setup();
    render(<Security tab="protect" initialFiles={["C:/docs/a.pdf"]} dragging={false} />);
    const runButton = await screen.findByRole("button", { name: "Protect PDF" });
    await waitFor(() => expect(runButton).toBeEnabled());
    await user.type(screen.getByPlaceholderText("••••••••"), "secret123");
    await user.type(screen.getByPlaceholderText("Confirm password"), "secret123");
    await user.click(runButton);
    await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "protect_pdf")).toBe(true));
    const request = requestOf<{ request: { userPassword: string; ownerPassword: string; allowPrinting: boolean } }>(
      "protect_pdf",
    ).request;
    expect(request.userPassword).toBe("secret123");
    // The owner password falls back to the user password.
    expect(request.ownerPassword).toBe("secret123");
    expect(request.allowPrinting).toBe(true);
  });

  it("sends the watermark options as edited", async () => {
    const user = userEvent.setup();
    render(<Watermark {...props} />);
    const runButton = await screen.findByRole("button", { name: "Watermark" });
    await waitFor(() => expect(runButton).toBeEnabled());
    await user.click(runButton);
    await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "watermark_pdf")).toBe(true));
    const request = requestOf<{ request: { options: { text: string; kind: string } } }>("watermark_pdf").request;
    expect(request.options.kind).toBe("text");
    expect(request.options.text).toBe("CONFIDENTIAL");
  });

  it("seeds the metadata form from pdf_info and saves the edited title", async () => {
    const user = userEvent.setup();
    render(<Metadata {...props} />);
    // Seeding only works when the camelCase metadata fields are read.
    const title = await screen.findByDisplayValue("Quarterly Report");
    expect(screen.getByDisplayValue("Ada Lovelace")).toBeInTheDocument();
    await user.clear(title);
    await user.type(title, "2026 Report");
    await user.click(screen.getByRole("button", { name: "Save metadata" }));
    await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "edit_metadata")).toBe(true));
    const request = requestOf<{ request: { metadata: { title: string }; remove: boolean } }>("edit_metadata").request;
    expect(request.metadata.title).toBe("2026 Report");
    expect(request.remove).toBe(false);
  });

  it("shows the document info the backend returned", async () => {
    render(<InfoScreen initialFiles={["C:/docs/a.pdf"]} />);
    // `width_pt` is the wire spelling; a camelCase-only read would print NaN.
    await waitFor(() => expect(screen.getAllByText(/595/).length).toBeGreaterThan(0));
    expect(screen.getAllByText(/2 pages/).length).toBeGreaterThan(0);
    expect(screen.getAllByText(/PDF 1\.7/).length).toBeGreaterThan(0);
    expect(screen.queryByText(/NaN/)).toBeNull();
  });
});

describe("History, Home and Settings act on the local stores", () => {
  beforeEach(() => {
    invoke.mockClear();
  });

  it("lists recent files, clears them, and shows the operation log", async () => {
    const user = userEvent.setup();
    render(<History onNavigate={vi.fn()} />);
    expect(await screen.findByText("report.pdf")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Clear history" }));
    await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "clear_recent")).toBe(true));
    await waitFor(() => expect(screen.queryByText("report.pdf")).toBeNull());

    await user.click(screen.getByRole("tab", { name: "Operations" }));
    expect(await screen.findByText("merged")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Clear log" }));
    await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "clear_operations")).toBe(true));
  });

  it("filters the operation log and exports only the matching rows as CSV", async () => {
    const user = userEvent.setup();
    render(<History onNavigate={vi.fn()} />);
    await user.click(screen.getByRole("tab", { name: "Operations" }));
    expect(await screen.findByText("merged")).toBeInTheDocument();
    expect(screen.getByText("compressed")).toBeInTheDocument();

    await user.type(screen.getByLabelText("Filter"), "water");
    expect(screen.queryByText("merged")).toBeNull();
    expect(screen.getByText("watermark")).toBeInTheDocument();

    writeFile.mockClear();
    await user.click(screen.getByRole("button", { name: "Export CSV" }));
    await waitFor(() => expect(writeFile).toHaveBeenCalledTimes(1));
    const [target, bytes] = writeFile.mock.calls[0];
    expect(target).toBe("C:/docs/out.pdf");
    const csv = new TextDecoder().decode(bytes);
    const lines = csv.split("\r\n");
    expect(lines[0]).toBe("operation,input,output,pages,bytesIn,bytesOut,ok,detail");
    expect(lines).toHaveLength(2);
    expect(csv).toContain('"C:/docs/in,voice.pdf"');
    expect(csv).toContain('"C:/docs/in ""final""\n.pdf"');
    expect(csv).toContain('"failed, ""please retry"""');
    expect(csv).not.toContain("C:/docs/out.pdf");
    await waitFor(() =>
      expect(useToasts.getState().toasts.some((toast) => toast.title === "Operation log exported")).toBe(true),
    );
  });

  it("reports an empty export instead of writing a file", async () => {
    const user = userEvent.setup();
    render(<History onNavigate={vi.fn()} />);
    await user.click(screen.getByRole("tab", { name: "Operations" }));
    await screen.findByText("merged");
    await user.type(screen.getByLabelText("Filter"), "no-such-operation");
    expect(await screen.findByText("Nothing to export yet")).toBeInTheDocument();

    writeFile.mockClear();
    await user.click(screen.getByRole("button", { name: "Export CSV" }));
    expect(writeFile).not.toHaveBeenCalled();
    await waitFor(() =>
      expect(useToasts.getState().toasts.some((toast) => toast.title === "Nothing to export yet")).toBe(true),
    );
  });

  it("filters the Home tool directory and opens a recent file in the Reader", async () => {
    const user = userEvent.setup();
    const onNavigate = vi.fn();
    const onFileList = vi.fn();
    render(<Home onNavigate={onNavigate} onDropFiles={vi.fn()} dragging={false} onFileList={onFileList} />);
    expect(await screen.findByText("PDF & documents")).toBeInTheDocument();
    expect(screen.getByText("Productivity tools")).toBeInTheDocument();

    await user.type(screen.getByLabelText("Search tools"), "watermark");
    expect(screen.getByRole("button", { name: /Watermark/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Merge/ })).toBeNull();

    await user.clear(screen.getByLabelText("Search tools"));
    const openButtons = await screen.findAllByRole("button", { name: "Open" });
    await user.click(openButtons[0]);
    expect(onFileList).toHaveBeenCalledWith(["C:/docs/report.pdf"]);
    expect(onNavigate).toHaveBeenCalledWith("reader");
  });

  it("opens a recent office document in its editor and keeps it in the recent list", async () => {
    const user = userEvent.setup();
    const onNavigate = vi.fn();
    render(<Home onNavigate={onNavigate} onDropFiles={vi.fn()} dragging={false} onFileList={vi.fn()} />);
    expect(await screen.findByText("budget.xlsx")).toBeInTheDocument();
    const openButtons = await screen.findAllByRole("button", { name: "Open" });
    await user.click(openButtons[2]);
    expect(onNavigate).toHaveBeenCalledWith("office");
    expect(requestOf("office_open_document")).toEqual({ path: "C:/docs/budget.xlsx" });
    await waitFor(() =>
      expect(requestOf("add_recent")).toEqual({
        entry: expect.objectContaining({ path: "C:/docs/budget.xlsx", fileName: "budget.xlsx", tool: "_office" }),
      }),
    );
    await waitFor(() =>
      expect(useOfficeTabs.getState().tabs.some((tab) => tab.path === "C:/docs/budget.xlsx")).toBe(true),
    );
  });

  it("persists theme and office default choices through save_settings", async () => {
    const user = userEvent.setup();
    render(<Settings />);
    expect(await screen.findByText("PDF rendering (pdfium)")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Paper" }));
    await waitFor(() => {
      const saves = (invoke.mock.calls as unknown as [string, { settings: { theme: string } }][]).filter(
        ([name]) => name === "save_settings",
      );
      expect(saves.some(([, payload]) => payload.settings.theme === "paper")).toBe(true);
    });

    await user.click(screen.getByRole("button", { name: "ODT" }));
    await waitFor(() => {
      const saves = (invoke.mock.calls as unknown as [string, { settings: { defaultWriterFormat: string } }][]).filter(
        ([name]) => name === "save_settings",
      );
      expect(saves.some(([, payload]) => payload.settings.defaultWriterFormat === "odt")).toBe(true);
    });
  });

  it("exports the diagnostics report to the file the user picks", async () => {
    const user = userEvent.setup();
    useToasts.setState({ toasts: [] });
    render(<Settings />);
    await user.click(await screen.findByRole("button", { name: "Export diagnostics" }));
    await waitFor(() =>
      expect(useToasts.getState().toasts.some((toast) => toast.title === "Diagnostics report saved")).toBe(true),
    );
    const names = (invoke.mock.calls as unknown as [string][]).map(([name]) => name);
    expect(names).toContain("diagnostics_report");
    expect(writeFile).toHaveBeenCalledWith(
      "C:/docs/out.pdf",
      new TextEncoder().encode("OmniOffice diagnostics\nVersion: 3.9.0\n"),
    );
  });
});

describe("operation log CSV helpers", () => {
  it("escapes commas, quotes and newlines per RFC 4180", () => {
    expect(csvField("plain")).toBe("plain");
    expect(csvField("a,b")).toBe('"a,b"');
    expect(csvField('say "hi"')).toBe('"say ""hi"""');
    expect(csvField("line1\r\nline2")).toBe('"line1\r\nline2"');
  });

  it("serializes the header and leaves absent optional fields empty", () => {
    const csv = operationsToCsv([
      {
        id: "x",
        createdAt: 0,
        operation: "merged",
        inputPath: "C:/a.pdf",
        outputPath: "C:/b.pdf",
        ok: true,
      },
    ]);
    expect(csv).toBe("operation,input,output,pages,bytesIn,bytesOut,ok,detail\r\nmerged,C:/a.pdf,C:/b.pdf,,,,true,");
  });

  it("filters case-insensitively by operation or either path", () => {
    const base = { createdAt: 0, pageCount: 1, inputBytes: 10, outputBytes: 10, ok: true };
    const entries = [
      { ...base, id: "1", operation: "merged", inputPath: "C:/docs/a.pdf", outputPath: "C:/docs/out.pdf" },
      { ...base, id: "2", operation: "watermark", inputPath: "C:/docs/b.pdf", outputPath: "C:/docs/b_wm.pdf" },
      { ...base, id: "3", operation: "compressed", inputPath: "D:/other/c.pdf", outputPath: "D:/other/c2.pdf" },
    ];
    expect(filterOperations(entries, "WATER").map((entry) => entry.id)).toEqual(["2"]);
    expect(filterOperations(entries, "d:/other").map((entry) => entry.id)).toEqual(["3"]);
    expect(filterOperations(entries, "compressed").map((entry) => entry.id)).toEqual(["3"]);
    // A blank filter returns every row.
    expect(filterOperations(entries, "  ")).toHaveLength(3);
  });
});
