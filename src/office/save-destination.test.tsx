import { useEffect } from "react";
import { act, render, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

/**
 * The first save of a document opened from another app (Android) asks where
 * the changes go: Downloads, a picked location, or save and share it back.
 */

const calls: { command: string; args: Record<string, unknown> }[] = [];
const invoke = vi.fn(async (command: string, args?: Record<string, unknown>) => {
  const payload = args ?? {};
  calls.push({ command, args: payload });
  if (command === "office_compatibility") return { target: String(payload.target ?? ""), items: [] };
  if (command === "office_save_document") return { path: String(payload.path), warnings: [] };
  if (command === "file_fingerprint") return { exists: true, sha256: "abc" };
  return null;
});

const mobile = vi.hoisted(() => ({
  android: true,
  published: new Set<string>(),
  publishOutputs: vi.fn(async (_paths: string[], _target?: unknown) => [] as { path: string; error: string }[]),
  pickAndroidSaveTarget: vi.fn(async (_name: string, _mime?: string) => null as { uri: unknown; name: string } | null),
}));
const shareDocumentFile = vi.hoisted(() => vi.fn(async (_path: string, _title?: string) => undefined));

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string])) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/api/path", () => ({
  appCacheDir: vi.fn(async () => "/data/cache"),
  join: vi.fn(async (...parts: string[]) => parts.join("/")),
}));
vi.mock("../lib/mobile", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../lib/mobile")>()),
  isAndroid: () => mobile.android,
  publishedUri: (path: string) => (mobile.published.has(path) ? `content://published/${path}` : undefined),
  publishOutputs: (paths: string[], target?: unknown) => {
    for (const path of paths) mobile.published.add(path);
    return mobile.publishOutputs(paths, target);
  },
  updatePublishedOutput: async (path: string) => mobile.published.has(path),
  pickAndroidSaveTarget: (name: string, mime?: string) => mobile.pickAndroidSaveTarget(name, mime),
}));
vi.mock("../lib/share", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../lib/share")>()),
  shareDocumentFile: (path: string, title?: string) => shareDocumentFile(path, title),
}));

import { useOfficeSession } from "./useOfficeSession";
import { SaveDestinationHost, useSaveDestinationPrompt, type SaveDestinationChoice } from "./save-destination";
import { useOfficeTabs, type OfficeTab } from "../lib/office-store";
import { useToasts } from "../lib/store";

const INCOMING = "/data/user/0/app/cache/incoming/42/Teklif.odt";

let session: ReturnType<typeof useOfficeSession> | null = null;

function Harness({ tab }: { tab: OfficeTab }) {
  const current = useOfficeSession(tab);
  useEffect(() => {
    session = current;
  }, [current]);
  return <SaveDestinationHost />;
}

function renderSession(path: string | undefined) {
  useOfficeTabs.setState({ tabs: [], activeId: null });
  const id = useOfficeTabs.getState().create("writer", "Teklif");
  useOfficeTabs.setState((state) => ({
    tabs: state.tabs.map((tab) => (tab.id === id ? { ...tab, path: path ?? null } : tab)),
  }));
  const tab = useOfficeTabs.getState().tabs.find((candidate) => candidate.id === id)!;
  render(<Harness tab={tab} />);
}

/** Starts Ctrl+S, answers the destination question, and returns the save's result. */
async function saveAnswering(choice: SaveDestinationChoice): Promise<string | null> {
  let pending: Promise<string | null> | undefined;
  act(() => {
    pending = session!.save();
  });
  await waitFor(() => expect(useSaveDestinationPrompt.getState().request).not.toBeNull());
  expect(useSaveDestinationPrompt.getState().request?.name).toBe("Teklif.odt");
  await act(async () => useSaveDestinationPrompt.getState().answer(choice));
  return pending!;
}

beforeEach(() => {
  calls.length = 0;
  mobile.android = true;
  mobile.published.clear();
  mobile.publishOutputs.mockClear();
  mobile.pickAndroidSaveTarget.mockReset();
  mobile.pickAndroidSaveTarget.mockResolvedValue(null);
  shareDocumentFile.mockClear();
  useSaveDestinationPrompt.setState({ request: null });
  useToasts.setState({ toasts: [] });
  session = null;
});

describe("first save of a document opened from another app", () => {
  it("saves to Downloads and says where", async () => {
    renderSession(INCOMING);
    expect(await saveAnswering("downloads")).toBe(INCOMING);
    expect(calls.find((call) => call.command === "office_save_document")?.args.path).toBe(INCOMING);
    expect(mobile.publishOutputs).toHaveBeenCalledWith([INCOMING], undefined);
    const toast = useToasts.getState().toasts.at(-1);
    expect(toast?.detail).toBe("Downloads/OmniOffice/Teklif.odt");
    expect(shareDocumentFile).not.toHaveBeenCalled();
  });

  it("saves and opens the share sheet for the saved file", async () => {
    renderSession(INCOMING);
    expect(await saveAnswering("share")).toBe(INCOMING);
    expect(shareDocumentFile).toHaveBeenCalledWith(INCOMING, "Share");
  });

  it("lets the user pick a location, keeping the document's format", async () => {
    renderSession(INCOMING);
    expect(await saveAnswering("pick")).toBeNull(); // the picker is cancelled in this test
    expect(mobile.pickAndroidSaveTarget).toHaveBeenCalledWith("Teklif.odt", undefined);
    expect(calls.find((call) => call.command === "office_save_document")).toBeUndefined();
  });

  it("writes nothing when the question is cancelled", async () => {
    renderSession(INCOMING);
    expect(await saveAnswering("cancel")).toBeNull();
    expect(calls.find((call) => call.command === "office_save_document")).toBeUndefined();
  });

  it("asks only once: a later save goes to the same place", async () => {
    renderSession(INCOMING);
    await saveAnswering("downloads");
    let second: string | null = null;
    await act(async () => {
      second = await session!.save();
    });
    expect(second).toBe(INCOMING);
    expect(useSaveDestinationPrompt.getState().request).toBeNull();
  });

  it("does not ask for an app document or on desktop", async () => {
    renderSession("/data/user/0/app/cache/office/office-1.docx");
    await act(async () => {
      await session!.save();
    });
    expect(useSaveDestinationPrompt.getState().request).toBeNull();

    mobile.android = false;
    renderSession(INCOMING);
    await act(async () => {
      await session!.save();
    });
    expect(useSaveDestinationPrompt.getState().request).toBeNull();
  });
});
