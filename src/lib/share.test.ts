import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();
const revealMock = vi.fn();
let android = true;

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));
vi.mock("@tauri-apps/api/path", () => ({
  appCacheDir: async () => "/data/cache",
  join: async (...parts: string[]) => parts.join("/"),
}));
vi.mock("./mobile", () => ({
  isAndroid: () => android,
  mimeForName: (name: string) => (name.endsWith(".pdf") ? "application/pdf" : "application/octet-stream"),
  revealAnyFile: (path: string) => revealMock(path),
}));

import { shareDocumentFile, shareExtensionFor, shareFileName, shareOfficeDocument } from "./share";

describe("share formats", () => {
  it("keeps a format the editor can write and maps the rest to the common one", () => {
    expect(shareExtensionFor("writer", "/cache/incoming/a/letter.odt")).toBe("odt");
    expect(shareExtensionFor("writer", "/cache/incoming/a/old.DOC")).toBe("docx");
    expect(shareExtensionFor("calc", "/cache/incoming/a/report.xls")).toBe("xlsx");
    expect(shareExtensionFor("calc", "/cache/incoming/a/data.csv")).toBe("csv");
    expect(shareExtensionFor("impress", "/x/deck.oswk")).toBe("pptx");
    expect(shareExtensionFor("writer", undefined)).toBe("docx");
    expect(shareExtensionFor("calc", "")).toBe("xlsx");
  });

  it("names the share copy after the document title", () => {
    expect(shareFileName("Teklif.docx", "pdf")).toBe("Teklif.pdf");
    expect(shareFileName("Q3 / Satış: özet", "xlsx")).toBe("Q3 - Satış- özet.xlsx");
    expect(shareFileName("", "docx")).toBe("Document.docx");
  });
});

describe("shareDocumentFile", () => {
  beforeEach(() => {
    android = true;
    invokeMock.mockReset();
    revealMock.mockReset();
  });

  it("hands an app-private file to the Android share sheet", async () => {
    invokeMock.mockResolvedValue(undefined);
    await shareDocumentFile("/data/cache/incoming/a/scan.pdf", "Share");
    expect(invokeMock).toHaveBeenCalledWith("share_document_file", {
      path: "/data/cache/incoming/a/scan.pdf",
      mimeType: "application/pdf",
      title: "Share",
    });
    expect(revealMock).not.toHaveBeenCalled();
  });

  it("falls back to sharing a published copy when the bridge refuses", async () => {
    invokeMock.mockRejectedValue(new Error("not in the app's storage"));
    await shareDocumentFile("/somewhere/else.pdf");
    expect(revealMock).toHaveBeenCalledWith("/somewhere/else.pdf");
  });

  it("reveals the file on desktop", async () => {
    android = false;
    await shareDocumentFile("C:/docs/a.pdf");
    expect(invokeMock).not.toHaveBeenCalled();
    expect(revealMock).toHaveBeenCalledWith("C:/docs/a.pdf");
  });
});

describe("shareOfficeDocument", () => {
  beforeEach(() => {
    android = true;
    invokeMock.mockReset();
    revealMock.mockReset();
  });

  it("writes the current model to a named share copy, then shares it", async () => {
    invokeMock.mockImplementation(async (command: string, args: Record<string, unknown>) => {
      if (command === "office_save_document" || command === "office_export_pdf") {
        return { path: args.path, warnings: [] };
      }
      return undefined;
    });
    const tab = {
      kind: "calc" as const,
      title: "Rapor.xls",
      path: "/data/cache/incoming/a/Rapor.xls",
      model: { s: 1 },
    };

    const written = await shareOfficeDocument(tab, "document");
    expect(written).toMatch(/^\/data\/cache\/share\/[^/]+\/Rapor\.xlsx$/);
    expect(invokeMock).toHaveBeenCalledWith("office_save_document", { kind: "calc", model: { s: 1 }, path: written });
    expect(invokeMock).toHaveBeenCalledWith("share_document_file", expect.objectContaining({ path: written }));

    const pdf = await shareOfficeDocument(tab, "pdf");
    expect(pdf).toMatch(/\/Rapor\.pdf$/);
    expect(invokeMock).toHaveBeenCalledWith("office_export_pdf", { kind: "calc", model: { s: 1 }, path: pdf });
  });
});
