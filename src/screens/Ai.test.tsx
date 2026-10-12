import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { Ai } from "./Ai";

const invoke = vi.fn(async (command: string, _payload?: unknown) => {
  switch (command) {
    case "app_info":
      return { appVersion: "4.7.0", coreVersion: "4.7.0", platform: "windows" };
    case "ai_get_settings":
      return {
        configured: true,
        provider: "deepseek",
        providerLabel: "DeepSeek",
        model: "deepseek-chat",
        maskedKey: "sk-••••••••",
        keyStorage: "dpapi",
      };
    case "ai_example_prompts":
      return {
        summarize: ["Executive summary"],
        translateTargets: ["tr", "en"],
        ask: ["What is the total amount?"],
      };
    case "ai_document_preview":
      return {
        pages: 3,
        characters: 1500,
        estimatedWords: 250,
      };
    case "ai_chat":
      return {
        answer: "The total invoiced amount is $1,250.00 as indicated on [page 2].",
        citations: [
          {
            page: 2,
            snippet: "Total Amount: $1,250.00",
            score: 0.95,
            cited: true,
          },
        ],
        retrievalMode: "hybrid",
        model: "deepseek-chat",
        elapsedMs: 650,
      };
    case "ai_chat_list":
      return [
        {
          id: "thread-1",
          title: "Invoice Analysis",
          documentPath: "C:/docs/invoice.pdf",
          createdAt: "2026-10-11T12:00:00Z",
          updatedAt: "2026-10-11T12:00:00Z",
          messages: [
            { role: "user", content: "What is the vendor name?" },
            { role: "assistant", content: "The vendor is Acme Corp." },
          ],
        },
      ];
    case "ai_chat_save":
      return null;
    case "ai_chat_load":
      return {
        id: "thread-1",
        title: "Invoice Analysis",
        documentPath: "C:/docs/invoice.pdf",
        createdAt: "2026-10-11T12:00:00Z",
        updatedAt: "2026-10-11T12:00:00Z",
        messages: [
          { role: "user", content: "What is the vendor name?" },
          { role: "assistant", content: "The vendor is Acme Corp." },
        ],
      };
    case "ai_chat_delete":
      return null;
    case "ai_extract_table":
      return {
        rows: [
          {
            page: 1,
            vendor: "Acme Corp",
            date: "2026-10-01",
            invoiceNumber: "INV-1001",
            currency: "USD",
            subtotal: 1000.0,
            tax: 250.0,
            total: 1250.0,
            lineItems: "Consulting Services (10h)",
            customFields: { tax_id: "XX-12345" },
          },
          {
            page: 2,
            vendor: "=cmd|' /C calc'!A0",
            date: "2026-10-02",
            invoiceNumber: "INV-1002",
            currency: "TRY",
            subtotal: 100.0,
            tax: 20.0,
            total: 120.0,
            lineItems: "-dangerous",
            customFields: { tax_id: "=SUM(A1:A9)" },
          },
        ],
        columns: [
          "page",
          "vendor",
          "date",
          "invoiceNumber",
          "currency",
          "subtotal",
          "tax",
          "total",
          "lineItems",
          "tax_id",
        ],
        pages: 2,
        elapsedMs: 400,
      };
    case "ai_batch_run":
      return {
        results: [
          {
            path: "C:/docs/invoice1.pdf",
            action: "summarize",
            success: true,
            output: "Summary of invoice 1",
            elapsedMs: 300,
          },
          {
            path: "C:/docs/invoice2.pdf",
            action: "summarize",
            success: true,
            output: "Summary of invoice 2",
            elapsedMs: 350,
          },
        ],
        totalElapsedMs: 650,
        status: "completed",
      };
    case "ai_save_output":
      return "C:/docs/invoice_table.csv";
    case "file_sizes":
      return [4096];
    case "pdf_info":
      return {
        path: "C:/docs/invoice.pdf",
        fileName: "invoice.pdf",
        fileSizeBytes: 4096,
        pageCount: 3,
        pdfVersion: "1.7",
        encrypted: false,
        hasTextLayer: true,
        metadata: {},
        pageGeometries: [{ page: 1, width_pt: 595, height_pt: 842, rotation: 0 }],
      };
    default:
      return null;
  }
});

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invoke(...(args as [string, unknown])),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => undefined),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(async () => ["C:/docs/invoice1.pdf", "C:/docs/invoice2.pdf"]),
  save: vi.fn(async () => "C:/docs/invoice_table.csv"),
}));

const { pickOfficeFilesMock, androidState } = vi.hoisted(() => ({
  pickOfficeFilesMock: vi.fn(async (_multiple?: boolean) => [] as string[]),
  androidState: { value: false },
}));

vi.mock("../lib/mobile", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../lib/mobile")>()),
  isAndroid: () => androidState.value,
  pickOfficeFiles: (multiple?: boolean) => pickOfficeFilesMock(multiple),
}));

describe("Ai Screen - Belge Asistanı 2.0", () => {
  beforeEach(() => {
    invoke.mockClear();
    pickOfficeFilesMock.mockClear();
    androidState.value = false;
  });

  it("handles document chat with clickable citations jumping to Reader", async () => {
    const user = userEvent.setup();
    const onNavigate = vi.fn();

    render(<Ai initialFiles={["C:/docs/invoice.pdf"]} dragging={false} onNavigate={onNavigate} />);

    // Consent checkbox
    const consentBoxes = await screen.findAllByRole("checkbox");
    await user.click(consentBoxes[0]);

    // Document Chat tab is default
    const chatInput = await screen.findByPlaceholderText(/Ask about this document/i);
    expect(chatInput).toBeInTheDocument();

    await user.type(chatInput, "What is the total amount?");
    const sendButton = screen.getByRole("button", { name: /Send/i });
    await user.click(sendButton);

    // ai_chat should be invoked
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "ai_chat",
        expect.objectContaining({
          request: expect.objectContaining({
            question: "What is the total amount?",
            path: "C:/docs/invoice.pdf",
          }),
        }),
      );
    });

    // Citations rendered
    expect(await screen.findByText(/The total invoiced amount is \$1,250\.00/i)).toBeInTheDocument();
    const citationButtons = await screen.findAllByRole("button", { name: /page 2/i });
    expect(citationButtons.length).toBeGreaterThan(0);

    // Clicking citation jumps to reader
    await user.click(citationButtons[0]);
    expect(onNavigate).toHaveBeenCalledWith("reader", {
      files: ["C:/docs/invoice.pdf"],
      targetPage: 2,
    });
  });

  it("saves and loads chat threads", async () => {
    const user = userEvent.setup();
    render(<Ai initialFiles={["C:/docs/invoice.pdf"]} dragging={false} />);

    // View saved threads list
    const threadsButton = await screen.findByRole("button", { name: /Threads/i });
    await user.click(threadsButton);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("ai_chat_list");
    });

    expect(await screen.findByText(/Invoice Analysis/i)).toBeInTheDocument();

    // Click to load thread
    await user.click(screen.getByText(/Invoice Analysis/i));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("ai_chat_load", { id: "thread-1" });
    });

    expect(await screen.findByText("What is the vendor name?")).toBeInTheDocument();
    expect(await screen.findByText("The vendor is Acme Corp.")).toBeInTheDocument();
  });

  it("extracts structured tables and exports to CSV", async () => {
    const user = userEvent.setup();
    render(<Ai initialFiles={["C:/docs/invoice.pdf"]} dragging={false} />);

    // Check consent
    const consentBoxes = await screen.findAllByRole("checkbox");
    await user.click(consentBoxes[0]);

    // Switch to Table Extraction tab
    const extractTabButton = await screen.findByRole("button", { name: /Invoice/i });
    await user.click(extractTabButton);

    // Click Run button in side panel
    const runButton = screen.getByRole("button", { name: /Run/i });
    await user.click(runButton);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "ai_extract_table",
        expect.objectContaining({
          request: expect.objectContaining({
            path: "C:/docs/invoice.pdf",
          }),
        }),
      );
    });

    // Preview table rendered
    expect(await screen.findByText("Acme Corp")).toBeInTheDocument();
    expect(await screen.findByText("INV-1001")).toBeInTheDocument();
    expect(await screen.findByText("1250")).toBeInTheDocument();

    // Export CSV
    const exportButton = screen.getByRole("button", { name: /Export CSV/i });
    await user.click(exportButton);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "ai_save_output",
        expect.objectContaining({
          path: "C:/docs/invoice_table.csv",
        }),
      );
    });

    // The exported CSV keeps exact field columns and neutralises formulas.
    const saveCall = invoke.mock.calls.find((call) => call[0] === "ai_save_output") as
      [string, { text?: string }] | undefined;
    const csv = saveCall?.[1]?.text ?? "";
    expect(csv.startsWith("page,vendor,date,invoiceNumber")).toBe(true);
    expect(csv).toContain("'=cmd");
    expect(csv).toContain("'=SUM(A1:A9)");
    expect(csv).toContain("'-dangerous");
  });

  it("queues and executes batch operations across multiple files", async () => {
    const user = userEvent.setup();
    render(<Ai dragging={false} />);

    // Switch to Batch AI tab
    const batchTabButton = await screen.findByRole("button", { name: /Batch AI/i });
    await user.click(batchTabButton);

    // Check consent
    const consentBoxes = await screen.findAllByRole("checkbox");
    await user.click(consentBoxes[0]);

    // Add files
    const addFilesButton = await screen.findByRole("button", { name: /Add Files/i });
    await user.click(addFilesButton);

    expect(await screen.findByText("invoice1.pdf")).toBeInTheDocument();
    expect(await screen.findByText("invoice2.pdf")).toBeInTheDocument();

    // Run batch queue
    const runButton = screen.getByRole("button", { name: /Run/i });
    await user.click(runButton);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "ai_batch_run",
        expect.objectContaining({
          request: expect.objectContaining({
            items: expect.arrayContaining([
              expect.objectContaining({ path: "C:/docs/invoice1.pdf", action: "summarize" }),
              expect.objectContaining({ path: "C:/docs/invoice2.pdf", action: "summarize" }),
            ]),
          }),
        }),
      );
    });

    // Results rendered
    expect(await screen.findByText("Summary of invoice 1")).toBeInTheDocument();
    expect(await screen.findByText("Summary of invoice 2")).toBeInTheDocument();
  });

  it("picks batch files through the Android SAF bridge", async () => {
    androidState.value = true;
    pickOfficeFilesMock.mockResolvedValue(["/cache/imports/a.pdf", "/cache/imports/b.docx"]);
    const user = userEvent.setup();
    render(<Ai dragging={false} />);

    const batchTabButton = await screen.findByRole("button", { name: /Batch AI/i });
    await user.click(batchTabButton);
    const consentBoxes = await screen.findAllByRole("checkbox");
    await user.click(consentBoxes[0]);
    const addFilesButton = await screen.findByRole("button", { name: /Add Files/i });
    await user.click(addFilesButton);

    expect(pickOfficeFilesMock).toHaveBeenCalledWith(true);
    expect(await screen.findByText("a.pdf")).toBeInTheDocument();
    expect(await screen.findByText("b.docx")).toBeInTheDocument();
  });
});
