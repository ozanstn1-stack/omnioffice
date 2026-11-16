import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

// The whole shell with the real Home screen: tapping a recent PDF must hand the
// file to the reader. Home used to set the file list and then navigate, and
// navigate() replaced the list with an empty one, so the reader showed its
// file picker (reported on Android).
const invoke = vi.fn(async (command: string, _payload?: unknown) => {
  switch (command) {
    case "load_recent":
      return [{ token: "tok-recent-1", fileName: "sözleşme.pdf", tool: "_merged", timestamp: 1_700_000_000 }];
    case "load_settings":
      return { theme: "dark", language: "en", showRecentFiles: true, profession: "general" };
    case "file_sizes":
      return [2048];
    case "file_names":
      // Android keeps an opened document as a cache copy with a UUID name.
      return ["8F2B5B96-3BDB-4E4B-B740-195A0C1D2E3F.pdf"];
    case "pdf_info":
      return {
        path: "tok-recent-1",
        fileName: "sözleşme.pdf",
        fileSizeBytes: 2048,
        pageCount: 3,
        pdfVersion: "1.7",
        encrypted: false,
        hasTextLayer: true,
        metadata: {},
        pageGeometries: [1, 2, 3].map((page) => ({
          page,
          width_pt: 595,
          height_pt: 842,
          display_width_pt: 595,
          display_height_pt: 842,
          rotation: 0,
        })),
        outline: [],
      };
    case "office_startup_files":
    case "office_take_launch_files":
    case "office_take_launch_skipped":
      return [];
    default:
      return null;
  }
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string, unknown])) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: () => ({ onDragDropEvent: vi.fn(async () => () => undefined) }),
}));
vi.mock("../lib/viewport", () => ({
  useNarrowLayout: () => false,
  useKeyboardInset: () => undefined,
  useFocusScrollIntoView: () => undefined,
}));
vi.mock("../lib/mobile", () => ({
  isAndroid: () => false,
  openAnyFile: vi.fn(async () => undefined),
  pickAndroidFiles: vi.fn(async () => []),
}));
vi.mock("../components/sidebar", () => ({
  DesktopSidebar: () => <aside data-testid="desktop-sidebar" />,
  DrawerNav: () => <aside />,
}));

import App from "../App";
import { useRecent, useSettings } from "../lib/store";
import { DEFAULT_SETTINGS } from "../lib/types";

describe("opening a recent file from Home", () => {
  beforeEach(() => {
    invoke.mockClear();
    useRecent.setState({ entries: [] });
    useSettings.setState({
      settings: { ...DEFAULT_SETTINGS, showRecentFiles: true, profession: "general" },
      loaded: true,
    });
  });

  it("opens the tapped PDF in the reader instead of its file picker", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole("button", { name: /sözleşme\.pdf/ }, { timeout: 5000 }));
    // The reader took the recent token (its size is looked up by token) ...
    await waitFor(
      () =>
        expect(
          invoke.mock.calls.some(
            ([command, payload]) => command === "file_sizes" && JSON.stringify(payload).includes("tok-recent-1"),
          ),
        ).toBe(true),
      { timeout: 8000 },
    );
    // ... and does not offer to pick a file.
    expect(screen.queryByRole("button", { name: "Select files" })).not.toBeInTheDocument();
    // It loads the document by its token (the extension check used to look at
    // the token itself and skip the load: an empty page and "1 / 0") ...
    await waitFor(() =>
      expect(
        invoke.mock.calls.some(
          ([command, payload]) => command === "pdf_info" && JSON.stringify(payload).includes("tok-recent-1"),
        ),
      ).toBe(true),
    );
    // ... and shows the document's name, not the cache copy's.
    expect((await screen.findAllByText("sözleşme.pdf")).length).toBeGreaterThan(0);
    expect(screen.queryByText(/8F2B5B96/)).not.toBeInTheDocument();
  });
});
