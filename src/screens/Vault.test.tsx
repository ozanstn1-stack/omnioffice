import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const statusFixture = {
  indexed: 3,
  folders: 1,
  indexBytes: 2048,
  lastScan: "2026-01-02T03:04:05Z",
  scanning: false,
  warnings: ["A configured folder no longer exists."],
};

const searchFixture = {
  hits: [
    {
      documentId: "a1",
      path: "C:/vault/alpha.txt",
      fileName: "alpha.txt",
      extension: "txt",
      size: 120,
      modified: "2026-01-02T00:00:00Z",
      score: 45,
      matchLabel: "paragraph 1",
      snippet: "The <<vault>> keeps every word on this machine.",
      matchedTerms: ["vault"],
    },
    {
      documentId: "b2",
      path: "C:/vault/sub/beta.md",
      fileName: "beta.md",
      extension: "md",
      size: 220,
      modified: "2026-02-02T00:00:00Z",
      score: 30,
      matchLabel: "paragraph 2",
      snippet: "Another <<vault>> mention lives here.",
      matchedTerms: ["vault"],
    },
  ],
  total: 2,
  tookMs: 4,
  indexMissing: false,
};

const invoke = vi.fn(async (command: string) => {
  switch (command) {
    case "vault_status":
      return statusFixture;
    case "vault_search":
      return searchFixture;
    case "vault_document_text":
      return "Full preview text of the indexed document.";
    case "vault_configure":
      return { folders: ["C:/vault"], includePdf: true, includeOffice: true, maxFileMb: 25, updatedAt: "2026-01-02T03:04:05Z" };
    default:
      return null;
  }
});

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string])) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null) }));
// The vault.* / compat.* keys are added to the i18n table by another engineer;
// identity translation keeps the assertions stable either way, exactly like
// the missing-key fallback does at runtime.
vi.mock("../lib/i18n", () => ({ useT: () => (key: string) => key }));

import { Vault } from "./Vault";

describe("the vault screen renders against the real wire format", () => {
  beforeEach(() => {
    invoke.mockClear();
  });

  it("states the privacy contract and shows the index status", async () => {
    const { container } = render(<Vault />);
    await waitFor(() => expect(screen.getByText("vault.indexed")).toBeInTheDocument());
    expect(screen.getByText("vault.privacyTitle")).toBeInTheDocument();
    expect(screen.getByText("vault.privacyBody")).toBeInTheDocument();
    expect(screen.getByText("3")).toBeInTheDocument();
    expect(screen.getByText("2.00 KB")).toBeInTheDocument();
    expect(screen.getByText("A configured folder no longer exists.")).toBeInTheDocument();
    expect(container.textContent).toContain("vault.foldersTitle");
  });

  it("searches, renders two hits with highlighted snippets and previews one", async () => {
    const user = userEvent.setup();
    const { container } = render(<Vault />);
    await waitFor(() => expect(screen.getByText("vault.indexed")).toBeInTheDocument());

    await user.type(screen.getByPlaceholderText("vault.queryPlaceholder"), "vault");
    await user.click(screen.getByRole("button", { name: "vault.searchButton" }));

    await waitFor(() => expect(screen.getByText("alpha.txt")).toBeInTheDocument());
    expect(screen.getByText("beta.md")).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("vault_search", { request: expect.objectContaining({ query: "vault" }) });

    const marks = Array.from(container.querySelectorAll("mark"));
    expect(marks.map((mark) => mark.textContent)).toEqual(["vault", "vault"]);

    await user.click(screen.getByText("alpha.txt"));
    await waitFor(() => expect(screen.getByText("Full preview text of the indexed document.")).toBeInTheDocument());
  });
});
