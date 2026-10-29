import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const manifest = {
  version: 1,
  id: "EVD-3534E4E0D1FA",
  createdAt: "2026-10-29T08:00:00Z",
  details: { title: "", caseReference: "2026/123 E.", court: "", submittedBy: "", language: "en" },
  entries: [
    {
      number: 1,
      label: "Lease",
      fileName: "lease.pdf",
      sha256: "a".repeat(64),
      size: 3221,
      pages: 2,
      firstPage: 3,
      lastPage: 4,
    },
    {
      number: 2,
      label: "Notice",
      fileName: "notice.pdf",
      sha256: "b".repeat(64),
      size: 1016,
      pages: 1,
      firstPage: 5,
      lastPage: 5,
    },
  ],
  totalPages: 5,
  bundle: { sha256: "c".repeat(64), size: 9000 },
};

let verification: Record<string, unknown> = {};
let picks: { token: string; name: string }[][] = [];
const invoke = vi.fn(async (command: string, _payload?: unknown) => {
  switch (command) {
    case "suggest_output":
      return { token: "C:/docs/out.pdf", path: "C:/docs/out.pdf" };
    case "output_exists":
      return false;
    case "file_sizes":
      return [3221, 1016];
    case "file_token_pick":
      return picks.shift() ?? [];
    case "evidence_bundle_build":
      return {
        result: {
          path: "C:/docs/out.pdf",
          pageCount: 5,
          originalBytes: 4237,
          outputBytes: 9000,
          reduction: null,
          message: manifest.id,
        },
        manifest,
      };
    case "evidence_bundle_verify":
      return verification;
    case "evidence_match_originals":
      return [
        { path: "tok-a", sha256: "b".repeat(64), exhibit: 2 },
        { path: "tok-b", sha256: "d".repeat(64), exhibit: null },
      ];
    default:
      return null;
  }
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string])) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));

import { EvidenceBundleScreen } from "./EvidenceBundle";
import { deskTasks } from "../lib/desks";
import { useSettings } from "../lib/store";
import { DEFAULT_SETTINGS } from "../lib/types";

const payloadOf = <T,>(command: string) => invoke.mock.calls.find(([name]) => name === command)?.[1] as T;

describe("evidence bundle screen", () => {
  beforeEach(() => {
    invoke.mockClear();
    picks = [];
    verification = {};
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS, language: "en" }, loaded: true });
  });

  it("builds the bundle with each exhibit's label and the cover details", async () => {
    const user = userEvent.setup();
    render(<EvidenceBundleScreen initialFiles={["C:/docs/lease.pdf", "C:/docs/notice.pdf"]} dragging={false} />);

    const first = await screen.findByLabelText("Label of exhibit 1");
    expect(first).toHaveValue("lease");
    expect(screen.getByLabelText("Label of exhibit 2")).toHaveValue("notice");
    fireEvent.change(first, { target: { value: "Lease (signed)" } });
    await user.type(screen.getByLabelText("Case number"), "2026/123 E.");

    const run = screen.getByRole("button", { name: "Build the bundle" });
    await waitFor(() => expect(run).toBeEnabled());
    await user.click(run);

    await waitFor(() => expect(payloadOf("evidence_bundle_build")).toBeTruthy());
    const { request } = payloadOf<{
      request: {
        exhibits: { path: string; label: string; name: string }[];
        details: Record<string, string>;
        output: { path: string };
      };
    }>("evidence_bundle_build");
    expect(request.exhibits).toEqual([
      { path: "C:/docs/lease.pdf", label: "Lease (signed)", name: "lease.pdf" },
      { path: "C:/docs/notice.pdf", label: "notice", name: "notice.pdf" },
    ]);
    expect(request.details).toMatchObject({ caseReference: "2026/123 E.", language: "en" });
    expect(request.output.path).toBe("C:/docs/out.pdf");

    const result = await screen.findByTestId("evidence-manifest");
    expect(result).toHaveTextContent("EVD-3534E4E0D1FA");
    expect(result).toHaveTextContent("EX-1 Lease");
    expect(result).toHaveTextContent("p. 3–4");
    expect(result).toHaveTextContent("a".repeat(64));
  });

  it("verifies a bundle on reopen and matches the originals to their exhibits", async () => {
    const user = userEvent.setup();
    verification = { present: true, manifest, contentMatches: true, actualSha256: "c".repeat(64) };
    picks = [
      [{ token: "tok-bundle", name: "bundle.pdf" }],
      [
        { token: "tok-a", name: "notice.pdf" },
        { token: "tok-b", name: "draft.pdf" },
      ],
    ];
    render(<EvidenceBundleScreen dragging={false} />);

    await user.click(screen.getByRole("button", { name: "Choose a bundle" }));
    expect(await screen.findByText("The bundle is unchanged: EVD-3534E4E0D1FA")).toBeInTheDocument();
    expect(payloadOf<{ path: string }>("evidence_bundle_verify").path).toBe("tok-bundle");

    await user.click(screen.getByRole("button", { name: "Compare originals" }));
    expect(await screen.findByText("notice.pdf: same as exhibit 2")).toBeInTheDocument();
    expect(screen.getByText("draft.pdf: matches no exhibit")).toBeInTheDocument();
    expect(payloadOf<{ bundle: string; originals: string[] }>("evidence_match_originals")).toEqual({
      bundle: "tok-bundle",
      originals: ["tok-a", "tok-b"],
    });
  });

  it("says when a bundle was changed or carries no manifest", async () => {
    const user = userEvent.setup();
    verification = { present: true, manifest, contentMatches: false, actualSha256: "e".repeat(64) };
    picks = [[{ token: "tok-bundle", name: "bundle.pdf" }], [{ token: "tok-plain", name: "plain.pdf" }]];
    render(<EvidenceBundleScreen dragging={false} />);

    await user.click(screen.getByRole("button", { name: "Choose a bundle" }));
    expect(await screen.findByText("The bundle was changed after it was built.")).toBeInTheDocument();

    verification = { present: false, manifest: null, contentMatches: false, actualSha256: "" };
    await user.click(screen.getByRole("button", { name: "Choose a bundle" }));
    expect(await screen.findByText("This file carries no evidence bundle manifest.")).toBeInTheDocument();
  });

  it("is what the lawyer desk's evidence task opens", () => {
    expect(deskTasks("lawyer").find((task) => task.key === "evidenceBundle")?.screen).toBe("evidence");
  });
});
