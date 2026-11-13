import { act, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (command: string) => {
    if (command === "inspect_document") {
      return {
        pageCount: 1,
        annotations: [],
        embeddedFiles: [],
        attachmentCount: 0,
        hasJavascript: false,
        javascriptEntries: [],
        hasOpenAction: false,
        formFields: [],
        authorOverride: "",
        subjectOverride: "",
        creator: "",
        producer: "",
      };
    }
    if (command === "pdf_verify_signatures") return { signatures: [], warnings: [] };
    if (command === "pdfa_validate") return { valid: true };
    if (command === "page_text") return "";
    return null;
  }),
}));
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
import { shareWithCheck, usePresend } from "../lib/presend-gate";
import { useSettings } from "../lib/store";
import { DEFAULT_SETTINGS } from "../lib/types";

describe("desktop shell pre-send dialog", () => {
  beforeEach(() => {
    usePresend.setState({ request: null });
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS }, loaded: true });
  });

  it("renders PresendDialog on the desktop shell when shareWithCheck opens a request", async () => {
    render(<App />);
    expect(await screen.findByTestId("desktop-sidebar")).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    await act(async () => {
      await shareWithCheck("/docs/desktop-contract.pdf", "Share");
    });

    expect(await screen.findByRole("dialog")).toBeInTheDocument();
    expect(await screen.findByText(/desktop-contract\.pdf/)).toBeInTheDocument();
  });
});
