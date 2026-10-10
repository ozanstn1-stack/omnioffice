import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

/**
 * "Delete from cloud" regression: the destructive call must be gated behind
 * the confirmation dialog (nothing is sent before the user confirms), it must
 * carry the exact `{ localPath }` payload, and the returned status must flip
 * the row from "Synced" to "Local only".
 */
const view = (state: string) => ({
  file: "report.oswk",
  localPath: "C:/docs/report.oswk",
  remotePath: "OmniOffice/report.oswk",
  state,
  tracked: state === "synced",
  localSize: 2048,
  remoteSize: state === "synced" ? 2048 : null,
  localSha256: "a".repeat(64),
  cloudSha256: state === "synced" ? "a".repeat(64) : null,
  remoteEtag: state === "synced" ? "etag-1" : null,
  baseEtag: state === "synced" ? "etag-1" : null,
  baseSha256: state === "synced" ? "a".repeat(64) : null,
  localRevision: 1,
  lastSyncedAt: null,
  updatedAt: null,
  note: null,
});

const invoke = vi.fn(async (command: string, payload?: unknown) => {
  switch (command) {
    case "sync_get_config":
      return {
        enabled: true,
        provider: "webdav",
        url: "https://cloud.example.org/dav",
        username: "ada",
        hasPassword: true,
        passwordStorage: "dpapi",
        allowInsecureHttp: false,
        remoteDir: "OmniOffice",
      };
    case "sync_capabilities":
      return { maxTransferBytes: 1024, backgroundSync: false, autoMerge: false, providers: [] };
    case "oauth_status":
      return [];
    case "load_recent":
      return [{ path: "C:/docs/report.oswk", fileName: "report.oswk", tool: "_office", timestamp: 1_700_000_000 }];
    case "sync_status":
      return view("synced");
    case "sync_delete_remote":
      expect((payload as { localPath: string }).localPath).toBe("C:/docs/report.oswk");
      return view("local_only");
    default:
      return null;
  }
});

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string])) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/api/path", () => ({
  appCacheDir: vi.fn(async () => "C:/cache"),
  join: vi.fn(async (...parts: string[]) => parts.join("/")),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(async () => null),
  save: vi.fn(async () => null),
}));

import { Sync } from "./Sync";
import { useToasts } from "../lib/store";

async function openDeleteDialog(user: ReturnType<typeof userEvent.setup>) {
  const buttons = await screen.findAllByRole("button", { name: "Delete from cloud" });
  await waitFor(() => expect(buttons[0]).toBeEnabled());
  await user.click(buttons[0]);
  return buttons[0];
}

describe("cloud sync delete-from-cloud", () => {
  beforeEach(() => {
    invoke.mockClear();
    useToasts.setState({ toasts: [] });
  });

  it("gates the destructive call behind the confirmation dialog", async () => {
    const user = userEvent.setup();
    render(<Sync />);
    await openDeleteDialog(user);
    expect(await screen.findByText("Delete the cloud copy of report.oswk?")).toBeInTheDocument();
    expect(screen.getByText(/The local file stays untouched/)).toBeInTheDocument();
    expect(invoke.mock.calls.some(([command]) => command === "sync_delete_remote")).toBe(false);

    await user.click(screen.getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(invoke.mock.calls.some(([command]) => command === "sync_delete_remote")).toBe(true));
    // The row is refreshed from the response: the cloud copy is gone.
    await waitFor(() => expect(screen.getByText("Local only")).toBeInTheDocument());
    await waitFor(() =>
      expect(useToasts.getState().toasts.some((toast) => toast.title === "Cloud copy deleted")).toBe(true),
    );
  });

  it("cancel keeps the cloud copy", async () => {
    const user = userEvent.setup();
    render(<Sync />);
    await openDeleteDialog(user);
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByText("Delete the cloud copy of report.oswk?")).toBeNull();
    expect(invoke.mock.calls.some(([command]) => command === "sync_delete_remote")).toBe(false);
  });
});
