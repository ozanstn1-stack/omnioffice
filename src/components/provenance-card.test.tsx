import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { ProvenanceReport } from "../lib/provenance-api";

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

let report: ProvenanceReport = {
  present: false,
  intact: false,
  manifest: null,
  actualSha256: "",
  warnings: [],
};
let savePick: { token: string; name: string } | null = { token: "tok-out", name: "belge-provenance.pdf" };

const invoke = vi.fn(async (command: string, _payload?: unknown) => {
  switch (command) {
    case "pdf_provenance_read":
      return report;
    case "pdf_provenance_write":
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

describe("ProvenanceCard (C8)", () => {
  beforeEach(() => {
    invoke.mockClear();
    useToasts.setState({ toasts: [] });
    report = { present: false, intact: false, manifest: null, actualSha256: "", warnings: [] };
    savePick = { token: "tok-out", name: "belge-provenance.pdf" };
  });

  it("says a document carries no manifest and offers to add one", async () => {
    render(<ProvenanceCard path="/docs/belge.pdf" />);
    expect(await screen.findByText("This document carries no source manifest.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add source manifest" })).toBeInTheDocument();
  });

  it("shows an intact manifest with its sources and digest", async () => {
    report = { present: true, intact: true, manifest, actualSha256: manifest.documentSha256, warnings: [] };
    render(<ProvenanceCard path="/docs/belge.pdf" />);

    expect(await screen.findByText("Matches the recorded digest")).toBeInTheDocument();
    expect(screen.getByText("manual")).toBeInTheDocument();
    expect(screen.getByText(/kaynak\.pdf/)).toBeInTheDocument();
    expect(screen.getByText("Document SHA-256", { exact: false })).toBeInTheDocument();
    // Replacing an existing manifest stays possible from the same button.
    expect(screen.getByRole("button", { name: "Add source manifest" })).toBeInTheDocument();
  });

  it("calls out a document changed after the manifest", async () => {
    report = { present: true, intact: false, manifest, actualSha256: "c".repeat(64), warnings: [] };
    render(<ProvenanceCard path="/docs/belge.pdf" />);

    expect(await screen.findByText("Changed after the manifest")).toBeInTheDocument();
    expect(screen.getByText("The document differs from the bytes the manifest was written for.")).toBeInTheDocument();
  });

  it("writes a copy with the picked sources and the app version", async () => {
    const user = userEvent.setup();
    render(<ProvenanceCard path="/docs/belge.pdf" />);

    await user.click(await screen.findByRole("button", { name: "Add source manifest" }));
    await user.click(await screen.findByRole("button", { name: "Add source files" }));
    await user.click(screen.getByRole("button", { name: "Save a copy with the manifest" }));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("pdf_provenance_write", expect.anything()));
    const request = invoke.mock.calls.find(([command]) => command === "pdf_provenance_write")?.[1] as {
      request: Record<string, unknown>;
    };
    expect(request.request.input).toBe("/docs/belge.pdf");
    expect(request.request.appVersion).toBe("6.5.1");
    expect(request.request.sources).toEqual([{ path: "tok-src", name: "kaynak.pdf", role: "source" }]);
    await waitFor(() =>
      expect(useToasts.getState().toasts.map((toast) => toast.title)).toContain("Source manifest written"),
    );
  });
});
