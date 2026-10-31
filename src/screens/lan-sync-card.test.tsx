import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const identity = { deviceId: "device-a", name: "Atölye", fingerprint: "ab".repeat(32) };
const baseStatus = { identity, encrypted: true, host: null, peers: [] };

let status = { ...baseStatus } as Record<string, unknown>;
let devices: Record<string, unknown>[] = [];
let plan: Record<string, unknown> | null = null;
let trash: Record<string, unknown>[] = [];

const lanStatus = vi.fn(async () => status);
const lanHostStart = vi.fn(async (_folder: string) => {
  status = {
    ...baseStatus,
    host: { port: 47820, folder: "folder-token", code: "123456", expiresInSecs: 120, attemptsLeft: 5 },
  };
  return status;
});
const lanHostOpenPairing = vi.fn(async () => status);
const lanHostStop = vi.fn(async () => {
  status = { ...baseStatus };
  return status;
});
const lanHostUnpair = vi.fn(async (_deviceId: string) => status);
const lanDiscover = vi.fn(async () => devices);
const lanPair = vi.fn(async (_address: string, _code: string) => {
  status = {
    ...baseStatus,
    peers: [
      {
        deviceId: "device-b",
        name: "Telefon",
        address: "192.168.1.20:47820",
        fingerprint: "cd".repeat(32),
        pairedAt: "2026-10-30T12:00:00Z",
        hostSide: false,
      },
    ],
  };
  return status;
});
const lanUnpair = vi.fn(async (_deviceId: string) => status);
const lanPlan = vi.fn(async (_deviceId: string, _folder: string) => plan);
const lanApply = vi.fn(async (_deviceId: string, _folder: string, _path: string, _resolution: string) => ({
  path: "rapor.docx",
  applied: true,
  action: "download_copy",
  warning: null,
}));
const lanSyncSafe = vi.fn(async (_deviceId: string, _folder: string) => []);
const lanTrashList = vi.fn(async () => trash);
const lanTrashRestore = vi.fn(async (_folder: string, _id: string) => status);
const lanTrashPurge = vi.fn(async () => 0);
const lanResolveDeletion = vi.fn(
  async (_deviceId: string, _folder: string, _path: string, _side: string, _resolution: string) => ({
    path: "eski.docx",
    applied: true,
    action: "delete_local",
  }),
);

vi.mock("../lib/lan-sync", () => ({
  lanStatus: (...args: unknown[]) => lanStatus(...(args as [])),
  lanRename: vi.fn(async () => status),
  lanHostStart: (...args: unknown[]) => lanHostStart(...(args as [string])),
  lanHostOpenPairing: (...args: unknown[]) => lanHostOpenPairing(...(args as [])),
  lanHostStop: (...args: unknown[]) => lanHostStop(...(args as [])),
  lanHostUnpair: (...args: unknown[]) => lanHostUnpair(...(args as [string])),
  lanDiscover: (...args: unknown[]) => lanDiscover(...(args as [])),
  lanPair: (...args: unknown[]) => lanPair(...(args as [string, string])),
  lanUnpair: (...args: unknown[]) => lanUnpair(...(args as [string])),
  lanPlan: (...args: unknown[]) => lanPlan(...(args as [string, string])),
  lanApply: (...args: unknown[]) => lanApply(...(args as [string, string, string, string])),
  lanSyncSafe: (...args: unknown[]) => lanSyncSafe(...(args as [string, string])),
  lanTrashList: (...args: unknown[]) => lanTrashList(...(args as [])),
  lanTrashRestore: (...args: unknown[]) => lanTrashRestore(...(args as [string, string])),
  lanTrashPurge: (...args: unknown[]) => lanTrashPurge(...(args as [])),
  lanResolveDeletion: (...args: unknown[]) => lanResolveDeletion(...(args as [string, string, string, string, string])),
}));

vi.mock("../lib/file-tokens", () => ({
  pickDirectoryToken: vi.fn(async () => ({ token: "folder-token", name: "Belgeler" })),
}));

import { LanSyncCard } from "./lan-sync-card";

describe("LAN sync card", () => {
  beforeEach(() => {
    status = { ...baseStatus };
    devices = [];
    plan = null;
    trash = [];
    vi.clearAllMocks();
  });

  it("shows the device identity and the encrypted store state", async () => {
    render(<LanSyncCard />);
    expect(await screen.findByText(/Atölye/)).toBeInTheDocument();
    expect(await screen.findByText(/store encrypted by the OS/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Start sharing/i })).toBeDisabled();
  });

  it("shares a picked folder and shows the six-digit code", async () => {
    const user = userEvent.setup();
    render(<LanSyncCard />);
    await user.click(await screen.findByRole("button", { name: /Choose folder/i }));
    await waitFor(() => expect(lanHostStart).not.toHaveBeenCalled());
    await user.click(screen.getByRole("button", { name: /Start sharing/i }));
    await waitFor(() => expect(lanHostStart).toHaveBeenCalledWith("folder-token"));
    expect(await screen.findByText("123456")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Stop sharing/i })).toBeInTheDocument();
  });

  it("discovers a host and pairs with its code", async () => {
    devices = [
      {
        deviceId: "device-b",
        name: "Telefon",
        address: "192.168.1.20:47820",
        fingerprint: "cd".repeat(32),
        paired: false,
      },
    ];
    const user = userEvent.setup();
    render(<LanSyncCard />);
    await user.click(await screen.findByRole("button", { name: /Find devices/i }));
    expect(await screen.findByText(/Telefon/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /^Pair$/i }));
    const input = screen.getByLabelText(/Pairing code/i);
    await user.type(input, "654321");
    await user.click(screen.getByRole("button", { name: /Pair now/i }));
    await waitFor(() => expect(lanPair).toHaveBeenCalledWith("192.168.1.20:47820", "654321"));
  });

  it("shows a conflict and sends the chosen resolution", async () => {
    status = {
      ...baseStatus,
      peers: [
        {
          deviceId: "device-b",
          name: "Telefon",
          address: "192.168.1.20:47820",
          fingerprint: "cd".repeat(32),
          pairedAt: "2026-10-30T12:00:00Z",
          hostSide: false,
        },
      ],
    };
    plan = {
      deviceId: "device-b",
      folder: "folder-token",
      items: [
        { path: "rapor.docx", state: "conflict", action: null, localSha256: "aa", remoteSha256: "bb" },
        { path: "yeni.xlsx", state: "cloud_ahead", action: "download", localSha256: null, remoteSha256: "cc" },
      ],
      deletions: [],
    };
    const { pickDirectoryToken } = await import("../lib/file-tokens");
    expect(await pickDirectoryToken()).toEqual({ token: "folder-token", name: "Belgeler" });
    const user = userEvent.setup();
    render(<LanSyncCard />);
    await user.click(await screen.findByRole("button", { name: /Local folder/i }));
    expect((await screen.findAllByText("Belgeler")).length).toBeGreaterThan(0);
    await user.click(screen.getByRole("button", { name: /Compare/i }));
    await waitFor(() => expect(lanPlan).toHaveBeenCalled());
    expect(await screen.findByText("rapor.docx")).toBeInTheDocument();
    expect(screen.getByText(/conflict/i)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Keep both/i }));
    await waitFor(() => expect(lanApply).toHaveBeenCalledWith("device-b", "folder-token", "rapor.docx", "keep_both"));
    expect(screen.getByText(/download/i)).toBeInTheDocument();
  });

  it("lists a removed document and sends only the confirmed deletion", async () => {
    status = {
      ...baseStatus,
      peers: [
        {
          deviceId: "device-b",
          name: "Telefon",
          address: "192.168.1.20:47820",
          fingerprint: "cd".repeat(32),
          pairedAt: "2026-10-30T12:00:00Z",
          hostSide: false,
        },
      ],
    };
    plan = {
      deviceId: "device-b",
      folder: "folder-token",
      items: [],
      deletions: [{ path: "eski.docx", side: "remote" }],
    };
    const user = userEvent.setup();
    render(<LanSyncCard />);
    await user.click(await screen.findByRole("button", { name: /Local folder/i }));
    await user.click(await screen.findByRole("button", { name: /Compare/i }));
    expect(await screen.findByText("eski.docx")).toBeInTheDocument();
    expect(screen.getByText(/removed on the peer/i)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Delete here/i }));
    await waitFor(() =>
      expect(lanResolveDeletion).toHaveBeenCalledWith(
        "device-b",
        "folder-token",
        "eski.docx",
        "remote",
        "delete_local",
      ),
    );
  });

  it("lists a trashed deletion and restores it into the picked folder", async () => {
    trash = [
      {
        id: "trash-1",
        path: "eski.docx",
        peerDeviceId: "device-b",
        peerName: "Telefon",
        side: "remote",
        sha256: "ee".repeat(32),
        size: 42,
        deletedAt: 1_800_000_000,
      },
    ];
    const user = userEvent.setup();
    render(<LanSyncCard />);
    expect(await screen.findByText("eski.docx")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Local folder/i }));
    expect((await screen.findAllByText("Belgeler")).length).toBeGreaterThan(0);
    await user.click(screen.getByRole("button", { name: /^Restore$/i }));
    await waitFor(() => expect(lanTrashRestore).toHaveBeenCalledWith("folder-token", "trash-1"));
  });

  it("reports a failed pairing instead of pretending it worked", async () => {
    devices = [
      {
        deviceId: "device-b",
        name: "Telefon",
        address: "192.168.1.20:47820",
        fingerprint: "cd".repeat(32),
        paired: false,
      },
    ];
    lanPair.mockRejectedValueOnce(new Error("the pairing code is not correct"));
    const user = userEvent.setup();
    render(<LanSyncCard />);
    await user.click(await screen.findByRole("button", { name: /Find devices/i }));
    await user.click(await screen.findByRole("button", { name: /^Pair$/i }));
    await user.type(screen.getByLabelText(/Pairing code/i), "000000");
    await user.click(screen.getByRole("button", { name: /Pair now/i }));
    expect(await screen.findByRole("alert")).toHaveTextContent(/pairing code is not correct/i);
  });
});
