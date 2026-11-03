import { render, screen, waitFor, within } from "@testing-library/react";
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

const handler = async (command: string) => {
  switch (command) {
    case "vault_status":
      return statusFixture;
    case "vault_scan":
      return statusFixture;
    case "vault_search_hybrid":
      return searchFixture;
    case "vault_document_text":
      return "Full preview text of the indexed document.";
    case "vault_configure":
      return {
        folders: ["C:/vault"],
        includePdf: true,
        includeOffice: true,
        maxFileMb: 25,
        updatedAt: "2026-01-02T03:04:05Z",
      };
    case "vault_import_files":
      return {
        imported: [
          {
            source: "C:/cache/imports/report.txt",
            path: "C:/appdata/vault/imported/0123456789abcdef-report.txt",
            name: "0123456789abcdef-report.txt",
            size: 12,
            sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
          },
        ],
        skipped: [],
      };
    case "vault_clear":
      return { ...statusFixture, indexed: 0, indexBytes: 0, warnings: [] };
    default:
      return null;
  }
};

const invoke = vi.fn(handler);

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string])) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null) }));
// Keep the real mobile helpers (isAndroid is a platform probe) and only mock
// the SAF picker plus the probe; each test sets what it needs.
vi.mock("../lib/mobile", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/mobile")>();
  return { ...actual, isAndroid: vi.fn(() => false), pickOfficeFiles: vi.fn(async () => []) };
});
// The vault.* / compat.* keys are added to the i18n table by another engineer;
// identity translation keeps the assertions stable either way, exactly like
// the missing-key fallback does at runtime.
vi.mock("../lib/i18n", () => ({ useT: () => (key: string) => key }));

import { useDev } from "../lib/store";
import { Vault } from "./Vault";
import { isAndroid, pickOfficeFiles } from "../lib/mobile";

describe("the vault screen renders against the real wire format", () => {
  beforeEach(() => {
    invoke.mockClear();
    invoke.mockImplementation(handler);
    // restoreMocks resets the factory implementations before every test.
    vi.mocked(isAndroid).mockReturnValue(false);
    vi.mocked(pickOfficeFiles).mockResolvedValue([]);
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
    // Desktop keeps the folder flow: the picker button stays available.
    expect(screen.getAllByRole("button", { name: "vault.addFolder" }).length).toBeGreaterThan(0);
  });

  it("searches, renders two hits with highlighted snippets and previews one", async () => {
    const user = userEvent.setup();
    const { container } = render(<Vault />);
    await waitFor(() => expect(screen.getByText("vault.indexed")).toBeInTheDocument());

    await user.type(screen.getByPlaceholderText("vault.queryPlaceholder"), "vault");
    await user.click(screen.getByRole("button", { name: "vault.searchButton" }));

    await waitFor(() => expect(screen.getByText("alpha.txt")).toBeInTheDocument());
    expect(screen.getByText("beta.md")).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("vault_search_hybrid", {
      request: expect.objectContaining({ query: "vault" }),
    });

    const marks = Array.from(container.querySelectorAll("mark"));
    expect(marks.map((mark) => mark.textContent)).toEqual(["vault", "vault"]);

    await user.click(screen.getByText("alpha.txt"));
    await waitFor(() => expect(screen.getByText("Full preview text of the indexed document.")).toBeInTheDocument());
  });

  it("imports picked documents on Android instead of scanning folders", async () => {
    vi.mocked(isAndroid).mockReturnValue(true);
    vi.mocked(pickOfficeFiles).mockResolvedValue(["C:/cache/imports/report.txt"]);
    const user = userEvent.setup();
    render(<Vault />);
    await waitFor(() => expect(screen.getByText("vault.indexed")).toBeInTheDocument());

    // Android hides the folder picker and explains the import model.
    expect(screen.queryByRole("button", { name: "vault.addFolder" })).toBeNull();
    expect(screen.getByText("vault.androidImportNote")).toBeInTheDocument();
    expect(screen.getByText("vault.importedRootName")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "vault.importDocuments" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("vault_import_files", { paths: ["C:/cache/imports/report.txt"] }),
    );
    // The import is followed by a scan of the configured roots (empty on a
    // fresh Android vault; the backend adds the always-on import root).
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "vault_scan",
        expect.objectContaining({ request: expect.objectContaining({ folders: [] }) }),
      ),
    );
    expect(pickOfficeFiles).toHaveBeenCalledWith(true);
  });

  it("clears the index after confirmation and can delete imported copies on Android", async () => {
    const user = userEvent.setup();
    const desktop = render(<Vault />);
    await waitFor(() => expect(screen.getByText("vault.indexed")).toBeInTheDocument());
    await user.click(screen.getByRole("button", { name: "vault.clearAction" }));
    const dialog = await screen.findByRole("dialog");
    // A plain clear keeps the imported copies (the checkbox is Android-only).
    expect(within(dialog).queryByRole("checkbox")).toBeNull();
    await user.click(within(dialog).getByRole("button", { name: "vault.clearAction" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("vault_clear", { deleteImports: false }));
    desktop.unmount();

    invoke.mockClear();
    vi.mocked(isAndroid).mockReturnValue(true);
    render(<Vault />);
    await waitFor(() => expect(screen.getByText("vault.indexed")).toBeInTheDocument());
    await user.click(screen.getByRole("button", { name: "vault.clearAction" }));
    const androidDialog = await screen.findByRole("dialog");
    await user.click(within(androidDialog).getByRole("checkbox"));
    await user.click(within(androidDialog).getByRole("button", { name: "vault.clearAction" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("vault_clear", { deleteImports: true }));
  });
});

it("pages through results with dictionary keys, not Turkish literals", async () => {
  invoke.mockImplementation(async (command: string) =>
    command === "vault_search_hybrid" ? { ...searchFixture, total: 120 } : handler(command),
  );
  const user = userEvent.setup();
  const { container } = render(<Vault />);
  await waitFor(() => expect(screen.getByText("vault.indexed")).toBeInTheDocument());

  await user.type(screen.getByPlaceholderText("vault.queryPlaceholder"), "vault");
  await user.click(screen.getByRole("button", { name: "vault.searchButton" }));

  const previous = await screen.findByRole("button", { name: /vault\.pagePrevious/ });
  const next = screen.getByRole("button", { name: /vault\.pageNext/ });
  expect(previous).toBeDisabled();

  await user.click(next);
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("vault_search_hybrid", {
      request: expect.objectContaining({ query: "vault", offset: 50 }),
    }),
  );
  // The screen no longer carries Turkish paging labels of its own.
  expect(container.textContent).not.toContain("Önceki");
  expect(container.textContent).not.toContain("Sonraki");
  expect(container.textContent).not.toContain("Semantik");
  expect(screen.getByText("vault.buildSemanticIndex")).toBeInTheDocument();
});

describe("vault development seeding", () => {
  beforeEach(() => {
    useDev.getState().set({ autoRun: false, startScreen: null, files: [] });
    delete document.documentElement.dataset.screenReady;
    delete document.documentElement.dataset.screenReadyDetail;
  });

  it("indexes the handed-over file's folder in a development auto-run", async () => {
    useDev.getState().set({ autoRun: true, startScreen: "vault", files: [] });
    render(<Vault initialFiles={["C:/seed/field-notes.md"]} />);
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("vault_scan", {
        request: expect.objectContaining({ folders: ["C:/seed"] }),
      }),
    );
    expect(await screen.findByTestId("vault-folder")).toHaveAttribute("data-path", "C:/seed");
    await waitFor(() => expect(document.documentElement.dataset.screenReady).toBe("vault"));
    expect(document.documentElement.dataset.screenReadyDetail).toBe("3");
    // The scan runs once, not on every render.
    expect(invoke.mock.calls.filter(([command]) => command === "vault_scan")).toHaveLength(1);
  });

  it("never indexes anything on a normal launch", async () => {
    render(<Vault initialFiles={["C:/seed/field-notes.md"]} />);
    await waitFor(() => expect(screen.getByText("vault.indexed")).toBeInTheDocument());
    expect(invoke.mock.calls.filter(([command]) => command === "vault_scan")).toHaveLength(0);
  });

  it("never seeds on Android, where folders cannot be browsed", async () => {
    useDev.getState().set({ autoRun: true, startScreen: "vault", files: [] });
    vi.mocked(isAndroid).mockReturnValue(true);
    render(<Vault initialFiles={["C:/seed/field-notes.md"]} />);
    await waitFor(() => expect(screen.getByText("vault.indexed")).toBeInTheDocument());
    expect(invoke.mock.calls.filter(([command]) => command === "vault_scan")).toHaveLength(0);
  });

  it("holds the ready signal until the seeded scan finishes", async () => {
    useDev.getState().set({ autoRun: true, startScreen: "vault", files: [] });
    let release: (() => void) | undefined;
    invoke.mockImplementation(async (command: string) => {
      if (command === "vault_scan") {
        await new Promise<void>((resolve) => {
          release = resolve;
        });
        return statusFixture;
      }
      return handler(command);
    });
    render(<Vault initialFiles={["C:/seed/field-notes.md"]} />);
    await waitFor(() => expect(invoke.mock.calls.some(([command]) => command === "vault_scan")).toBe(true));
    // The screen must not report "ready with 0" while the scan is still running.
    expect(document.documentElement.dataset.screenReady).toBeUndefined();
    release?.();
    await waitFor(() => expect(document.documentElement.dataset.screenReady).toBe("vault"));
  });

  it("exposes the result attributes the driver reads", async () => {
    const user = userEvent.setup();
    render(<Vault />);
    await waitFor(() => expect(screen.getByText("vault.indexed")).toBeInTheDocument());
    await user.type(screen.getByPlaceholderText("vault.queryPlaceholder"), "vault");
    await user.click(screen.getByRole("button", { name: "vault.searchButton" }));
    const hits = await screen.findByTestId("vault-hits");
    expect(hits).toHaveAttribute("data-total", "2");
    const rows = screen.getAllByTestId("vault-hit");
    expect(rows.map((row) => row.getAttribute("data-file-name"))).toEqual(["alpha.txt", "beta.md"]);
    await user.click(rows[0]);
    await waitFor(() => expect(screen.getByTestId("vault-preview")).toHaveTextContent("Full preview text"));
  });
});
