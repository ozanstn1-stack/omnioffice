import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

/**
 * Render and interaction coverage for the library/index screens and the two
 * converter-style tool screens that were previously untested. Payloads use the
 * camelCase wire format the Rust structs serialize.
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
      return { theme: "dark", language: "en", defaultExportFormat: "jpg", defaultImageDpi: 150, ocrLanguages: ["eng"] };
    case "save_settings":
    case "load_recent":
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
    case "ocr_pdf":
      return { path: "C:/docs/out.pdf", pageCount: 2, originalBytes: 2048, outputBytes: 1024, message: "ok" };
    case "pdf_to_images":
      return {
        files: [{ path: "C:/docs/page-1.png", page: 1, bytes: 1024, width: 595, height: 842 }],
        totalBytes: 1024,
        outputDir: "C:/docs",
      };
    case "jobs_list":
      return [
        {
          id: "job-running",
          kind: "vault",
          title: "Vault scan",
          status: "running",
          progress: 0.5,
          detail: null,
          payload: null,
          error: null,
          createdAt: 1_700_000_000,
          updatedAt: 1_700_000_010,
        },
        {
          id: "job-failed",
          kind: "ocr",
          title: "OCR scan.pdf",
          status: "failed",
          progress: 1,
          detail: null,
          payload: null,
          error: "engine crashed",
          createdAt: 1_700_000_000,
          updatedAt: 1_700_000_010,
        },
      ];
    case "jobs_clear_finished":
    case "cancel_job":
    case "jobs_finish":
    case "jobs_register":
    case "jobs_progress":
      return null;
    case "jobs_retry":
      return {
        id: "job-failed",
        kind: "ocr",
        title: "OCR scan.pdf",
        status: "running",
        progress: 0,
        detail: null,
        payload: null,
        error: null,
        createdAt: 1_700_000_000,
        updatedAt: 1_700_000_020,
      };
    case "ai_library_list":
      return [
        {
          id: "entry-1",
          createdAt: 1_700_000_000,
          kind: "summary",
          sourcePath: "C:/docs/notes.pdf",
          sourceName: "notes.pdf",
          model: "deepseek-v4",
          pages: 2,
          characters: 1200,
          options: "brief",
          filePath: "C:/docs/AI/notes.md",
          preview: "A short summary of the document.",
          elapsedMs: 1500,
        },
      ];
    case "ai_library_text":
      return "# Summary\n\nThe full saved text.";
    case "ai_library_default_dir":
      return "C:/docs/AI";
    case "ai_library_delete":
      return [];
    case "ai_library_clear":
      return null;
    case "office_supported_extensions":
      return ["pdf", "docx"];
    case "office_capabilities":
      return (_payload as { extension: string }).extension === "pdf"
        ? {
            extension: "pdf",
            open: true,
            edit: true,
            save: true,
            pdfExport: false,
            losslessNative: true,
            features: [{ feature: "editing", level: "partial", note: "Annotations and forms." }],
          }
        : {
            extension: "docx",
            open: true,
            edit: true,
            save: true,
            pdfExport: true,
            losslessNative: true,
            features: [{ feature: "trackedChanges", level: "partial", note: "Run-level revisions." }],
          };
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

import { AiLibrary } from "./AiLibrary";
import { JobsScreen } from "./Jobs";
import { CompatibilityScreen } from "./Compatibility";
import { Ocr } from "./Ocr";
import { Convert } from "./Convert";
import { registerJobRetryHandler, useJobs } from "../lib/jobs";

describe("library and index screens", () => {
  beforeEach(() => {
    invoke.mockClear();
    useJobs.setState({ jobs: [] });
  });

  it("lists saved AI results and loads the selected text", async () => {
    const user = userEvent.setup();
    render(<AiLibrary onOpenAi={vi.fn()} />);
    expect(await screen.findByText("notes.pdf")).toBeInTheDocument();
    expect(screen.getByText("A short summary of the document.")).toBeInTheDocument();
    await user.click(screen.getByText("notes.pdf"));
    await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "ai_library_text")).toBe(true));
    expect(await screen.findByText(/The full saved text\./)).toBeInTheDocument();
  });

  it("restores persisted jobs and offers the matching actions", async () => {
    // The app registers one handler per tracked kind at startup; the Retry
    // button only renders when a handler exists.
    const unregister = registerJobRetryHandler("ocr", () => undefined);
    try {
      const user = userEvent.setup();
      render(<JobsScreen />);
      expect(await screen.findByText("Vault scan")).toBeInTheDocument();
      expect(screen.getByText("Running")).toBeInTheDocument();
      expect(screen.getAllByText(/Failed/).length).toBeGreaterThan(0);

      await user.click(screen.getByRole("button", { name: /retry/i }));
      await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "jobs_retry")).toBe(true));

      await user.click(screen.getByRole("button", { name: /cancel/i }));
      await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "cancel_job")).toBe(true));
    } finally {
      unregister();
    }
  });

  it("shows the capability matrix per format", async () => {
    const user = userEvent.setup();
    render(<CompatibilityScreen />);
    expect(await screen.findByText(".docx")).toBeInTheDocument();
    // The first format is selected automatically and its features render.
    expect(screen.getByText("editing")).toBeInTheDocument();
    expect(screen.getByText("Annotations and forms.")).toBeInTheDocument();
    await user.click(screen.getByText(".docx"));
    expect(await screen.findByText("trackedChanges")).toBeInTheDocument();
    expect(screen.getByText("2 formats")).toBeInTheDocument();
  });
});

describe("converter-style tool screens", () => {
  beforeEach(() => {
    invoke.mockClear();
  });

  it("runs OCR with the selected languages", async () => {
    const user = userEvent.setup();
    render(<Ocr initialFiles={["C:/docs/a.pdf"]} dragging={false} />);
    const runButton = await screen.findByRole("button", { name: "Run OCR" });
    await waitFor(() => expect(runButton).toBeEnabled());
    await user.click(runButton);
    await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "ocr_pdf")).toBe(true));
    const call = (invoke.mock.calls as unknown as [string, { request: { options: { languages: string[] } } }][]).find(
      ([name]) => name === "ocr_pdf",
    );
    expect(call?.[1].request.options.languages).toEqual(["eng"]);
  });

  it("exports pages to images with the settings defaults", async () => {
    const user = userEvent.setup();
    render(<Convert tab="pdfToImages" initialFiles={["C:/docs/a.pdf"]} dragging={false} />);
    const runButton = await screen.findByRole("button", { name: "Convert" });
    await waitFor(() => expect(runButton).toBeEnabled());
    await user.click(runButton);
    await waitFor(() => expect(invoke.mock.calls.some(([name]) => name === "pdf_to_images")).toBe(true));
    const call = (invoke.mock.calls as unknown as [string, { request: { format: string; dpi: number } }][]).find(
      ([name]) => name === "pdf_to_images",
    );
    expect(call?.[1].request.format).toBe("jpeg");
    expect(call?.[1].request.dpi).toBe(150);
    // The produced files are listed, not just the count.
    expect(await screen.findByText(/page-1\.png/)).toBeInTheDocument();
  });
});
