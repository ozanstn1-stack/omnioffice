import { useEffect } from "react";
import { act, render } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

/**
 * Printing from the office editors: desktop WebViews print the page with
 * window.print(); the Android WebView ignores that call, so the document is
 * rendered to a PDF in the cache and opened in the system viewer instead.
 */
const invoke = vi.fn(async (command: string, args?: Record<string, unknown>) =>
  command === "office_export_pdf" ? { path: String(args?.path ?? ""), warnings: [] } : null,
);
let android = false;
const openAnyFile = vi.fn(async (_path: string) => undefined);

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: Record<string, unknown>) => invoke(command, args),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/api/path", () => ({
  appCacheDir: vi.fn(async () => "/cache"),
  join: vi.fn(async (...parts: string[]) => parts.join("/")),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null), save: vi.fn(async () => null) }));
vi.mock("../lib/mobile", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../lib/mobile")>()),
  isAndroid: () => android,
  openAnyFile: (path: string) => openAnyFile(path),
}));

import { useOfficeSession } from "./useOfficeSession";
import { useOfficeTabs, type OfficeTab } from "../lib/office-store";

let session: ReturnType<typeof useOfficeSession> | null = null;

function Harness({ tab }: { tab: OfficeTab }) {
  const current = useOfficeSession(tab);
  useEffect(() => {
    session = current;
  }, [current]);
  return null;
}

function renderSession() {
  useOfficeTabs.setState({ tabs: [], activeId: null });
  const id = useOfficeTabs.getState().create("writer", "Letter");
  const tab = useOfficeTabs.getState().tabs.find((candidate) => candidate.id === id)!;
  render(<Harness tab={tab} />);
  return session!;
}

describe("office print", () => {
  beforeEach(() => {
    invoke.mockClear();
    openAnyFile.mockClear();
    session = null;
  });

  it("uses the WebView print dialog on the desktop", async () => {
    android = false;
    const print = vi.spyOn(window, "print").mockImplementation(() => undefined);
    const current = renderSession();
    await act(() => current.print());
    expect(print).toHaveBeenCalledTimes(1);
    expect(invoke).not.toHaveBeenCalledWith("office_export_pdf", expect.anything());
    print.mockRestore();
  });

  it("renders a PDF into the cache and opens it on Android", async () => {
    android = true;
    const print = vi.spyOn(window, "print").mockImplementation(() => undefined);
    const current = renderSession();
    await act(() => current.print());
    const exportCall = invoke.mock.calls.find(([command]) => command === "office_export_pdf");
    expect(exportCall?.[1]?.path).toMatch(/^\/cache\/.*\.pdf$/);
    expect(openAnyFile).toHaveBeenCalledWith(exportCall?.[1]?.path);
    expect(print).not.toHaveBeenCalled();
    print.mockRestore();
  });
});
