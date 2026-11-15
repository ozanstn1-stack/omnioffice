import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

let launch: Array<{ token: string; name: string }> = [];
const invoke = vi.fn(async (command: string, _payload?: unknown) => {
  if (command === "office_startup_files") return launch;
  if (command === "office_take_launch_files") return [];
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
import { useSettings } from "../lib/store";
import { DEFAULT_SETTINGS } from "../lib/types";

describe("files Windows hands to the app on launch", () => {
  beforeEach(() => {
    invoke.mockClear();
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS }, loaded: true });
  });

  it("opens a launched PDF in the reader instead of staying on Home", async () => {
    launch = [{ token: "3f2a9c1d", name: "rapor.pdf" }];
    render(<App />);
    await waitFor(() => expect(screen.queryByTestId("desktop-home")).not.toBeInTheDocument());
    expect((await screen.findAllByText("rapor.pdf")).length).toBeGreaterThan(0);
  });
});
