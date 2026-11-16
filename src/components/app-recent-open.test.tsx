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
      return ["sözleşme.pdf"];
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
  });
});
