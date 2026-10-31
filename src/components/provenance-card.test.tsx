import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { SourceChainReport } from "../lib/provenance-api";

const manifest = {
  formatVersion: 1,
  createdAt: "2026-10-29T20:00:00Z",
  appVersion: "6.6.0",
  module: "manual",
  note: "",
  sources: [{ fileName: "kaynak.pdf", sha256: "a".repeat(64), size: 1024, role: "source" }],
  ai: null,
  documentSha256: "b".repeat(64),
};

function chainOf(overrides: Partial<SourceChainReport> = {}): SourceChainReport {
  const intact = overrides.intact ?? true;
  return {
    format: "pdf",
    present: true,
    intact,
    verdict: intact ? "intact" : "changed",
    outputSha256: intact ? manifest.documentSha256 : "c".repeat(64),
    recordedSha256: manifest.documentSha256,
    module: "manual",
    appVersion: "6.6.0",
    createdAt: "2026-10-29T20:00:00Z",
    note: "",
    ai: null,
    links: [
      { kind: "source", label: "kaynak.pdf", sha256: "a".repeat(64), size: 1024, role: "source" },
      { kind: "transform", label: "manual", sha256: "", size: null, role: "" },
      {
        kind: "output",
        label: "belge.pdf",
        sha256: intact ? manifest.documentSha256 : "c".repeat(64),
        size: 1200,
        role: "output",
      },
    ],
    warnings: [],
    ...overrides,
  };
}

const absent: SourceChainReport = {
  format: "pdf",
  present: false,
  intact: false,
  verdict: "absent",
  outputSha256: "",
  recordedSha256: "",
  module: "",
  appVersion: "",
  createdAt: "",
  note: "",
  ai: null,
  links: [],
  warnings: [],
};

let report: SourceChainReport = absent;
let savePick: { token: string; name: string } | null = { token: "tok-out", name: "belge-provenance.pdf" };

const invoke = vi.fn(async (command: string, _payload?: unknown) => {
  switch (command) {
    case "provenance_read_chain":
      return report;
    case "provenance_write":
      return {
        result: {
          path: "/docs/belge-provenance.pdf",
          pageCount: 1,
          originalBytes: 1000,
          outputBytes: 1200,
          reduction: null,
          message: "manual",
        },
        manifest,
      };
    case "app_info":
      return { appVersion: "6.5.1", coreVersion: "6.5.1", platform: "linux" };
    case "file_token_pick":
      return [{ token: "tok-src", name: "kaynak.pdf" }];
    case "file_token_save":
      return savePick ? [savePick] : [];
    case "file_sizes":
      return [1024];
    case "file_names":
      return ["kaynak.pdf"];
    default:
      return null;
  }
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string, unknown])) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null), save: vi.fn(async () => null) }));

import { useToasts } from "../lib/store";
import { ProvenanceCard } from "./provenance-card";

describe("ProvenanceCard (C8 + D7)", () => {
  beforeEach(() => {
    invoke.mockClear();
    useToasts.setState({ toasts: [] });
    report = absent;
    savePick = { token: "tok-out", name: "belge-provenance.pdf" };
  });

  it("says a document carries no manifest and offers to add one", async () => {
    render(<ProvenanceCard path="/docs/belge.pdf" />);
    expect(await screen.findByText("This document carries no source manifest.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add source manifest" })).toBeInTheDocument();
  });

  it("renders the derivation chain of an intact manifest", async () => {
    report = chainOf();
    render(<ProvenanceCard path="/docs/belge.pdf" />);

    expect(await screen.findByText("Matches the recorded digest")).toBeInTheDocument();
    expect(screen.getByText("Derivation chain")).toBeInTheDocument();
    expect(screen.getAllByText("manual").length).toBeGreaterThan(0);
    // Sources, the transformation step and the output are numbered links.
    expect(screen.getByText("Source")).toBeInTheDocument();
    expect(screen.getByText("Transformation")).toBeInTheDocument();
    expect(screen.getByText("Output")).toBeInTheDocument();
    expect(screen.getByText(/kaynak\.pdf/)).toBeInTheDocument();
    expect(screen.getByText("Recorded SHA-256", { exact: false })).toBeInTheDocument();
    expect(screen.getByText("Output SHA-256", { exact: false })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add source manifest" })).toBeInTheDocument();
  });

  it("warns clearly when the document no longer matches its manifest", async () => {
    report = chainOf({ intact: false, verdict: "changed" });
    render(<ProvenanceCard path="/docs/belge.pdf" />);

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Changed after the manifest");
    expect(alert).toHaveTextContent("changed after its source manifest was written");
  });

  it("writes a DOCX copy with the picked sources and the app version", async () => {
    report = absent;
    savePick = { token: "tok-out", name: "belge-provenance.docx" };
    const user = userEvent.setup();
    render(<ProvenanceCard path="/docs/belge.docx" />);

    await user.click(await screen.findByRole("button", { name: "Add source manifest" }));
    expect(screen.getByText("The manifest is embedded in the DOCX package as one extra part.")).toBeInTheDocument();
    await user.click(await screen.findByRole("button", { name: "Add source files" }));
    await user.click(screen.getByRole("button", { name: "Save a copy with the manifest" }));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("provenance_write", expect.anything()));
    const request = invoke.mock.calls.find(([command]) => command === "provenance_write")?.[1] as {
      request: Record<string, unknown>;
    };
    expect(request.request.input).toBe("/docs/belge.docx");
    expect(request.request.appVersion).toBe("6.5.1");
    expect(request.request.sources).toEqual([{ path: "tok-src", name: "kaynak.pdf", role: "source" }]);
    await waitFor(() =>
      expect(useToasts.getState().toasts.map((toast) => toast.title)).toContain("Source manifest written"),
    );
  });

  it("reports a file that is neither a PDF nor a DOCX package", async () => {
    report = { ...absent, format: "", verdict: "unsupported", warnings: ["not a package"] };
    render(<ProvenanceCard path="/docs/notes.txt" />);
    expect(
      await screen.findByText("Not a PDF or DOCX package, so it cannot carry a source manifest."),
    ).toBeInTheDocument();
  });
});
