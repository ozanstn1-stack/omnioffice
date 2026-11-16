import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

let launch: Array<{ token: string; name: string }> = [];
let skipped: string[] = [];
const invoke = vi.fn(async (command: string, _payload?: unknown) => {
  if (command === "office_startup_files") return launch;
  if (command === "office_take_launch_files") return [];
  if (command === "office_take_launch_skipped") {
    const names = skipped;
    skipped = [];
    return names;
  }
  if (command === "file_sizes") return [1024];
  if (command === "file_names") return launch.map((ref) => ref.name);
  return null;
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
vi.mock("../screens/Home", () => ({ Home: () => <div data-testid="desktop-home" /> }));
vi.mock("../components/sidebar", () => ({
  DesktopSidebar: () => <aside data-testid="desktop-sidebar" />,
  DrawerNav: () => <aside />,
}));

import App from "../App";
import { useSettings, useToasts } from "../lib/store";
import { DEFAULT_SETTINGS } from "../lib/types";

describe("files Windows hands to the app on launch", () => {
  beforeEach(() => {
    invoke.mockClear();
    skipped = [];
    useToasts.setState({ toasts: [] });
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS }, loaded: true });
  });

  it("opens a launched PDF in the reader instead of staying on Home", async () => {
    launch = [{ token: "3f2a9c1d", name: "rapor.pdf" }];
    render(<App />);
    await waitFor(() => expect(screen.queryByTestId("desktop-home")).not.toBeInTheDocument());
    expect((await screen.findAllByText("rapor.pdf")).length).toBeGreaterThan(0);
    // The document is loaded by its token (it used to stop at "1 / 0").
    await waitFor(() =>
      expect(
        invoke.mock.calls.some(
          ([command, payload]) => command === "pdf_info" && JSON.stringify(payload).includes("3f2a9c1d"),
        ),
      ).toBe(true),
    );
  });

  it("says which launched file could not be opened instead of staying silent on Home", async () => {
    launch = [];
    skipped = ["kayıp rapor.pdf (not found)"];
    render(<App />);
    await waitFor(() =>
      expect(useToasts.getState().toasts.some((toast) => toast.detail?.includes("kayıp rapor.pdf (not found)"))).toBe(
        true,
      ),
    );
  });

  it("opens a launched image in the image-to-PDF tool", async () => {
    launch = [{ token: "9b8c7d6e", name: "fatura.jpg" }];
    render(<App />);
    await waitFor(() => expect(screen.queryByTestId("desktop-home")).not.toBeInTheDocument());
    // The image tool took the launched token (its size is looked up by token).
    await waitFor(
      () =>
        expect(
          invoke.mock.calls.some(
            ([command, payload]) => command === "file_sizes" && JSON.stringify(payload).includes("9b8c7d6e"),
          ),
        ).toBe(true),
      { timeout: 8000 },
    );
  });
});
