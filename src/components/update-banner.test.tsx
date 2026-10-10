import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_SETTINGS, type UpdateInfo } from "../lib/types";

// The banner is exercised against the real update store; only the Tauri
// surface (invoke, listen, updater plugin, platform) is mocked.
const { invokeMock, listenMock, updaterCheckMock, processRelaunchMock, android } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  listenMock: vi.fn(),
  updaterCheckMock: vi.fn(),
  processRelaunchMock: vi.fn(),
  android: { value: false },
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: listenMock }));
vi.mock("@tauri-apps/plugin-updater", () => ({ check: updaterCheckMock }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: processRelaunchMock }));
vi.mock("../lib/mobile", () => ({ isAndroid: () => android.value }));

import { useSettings } from "../lib/store";
import { useUpdate } from "../lib/update";
import { UpdateBanner } from "./update-banner";

const NEWER: UpdateInfo = {
  current: "3.8.3",
  latest: "3.9.0",
  newer: true,
  releaseUrl: "https://github.com/ozanstn1-stack/omnioffice/releases/tag/v3.9.0",
  downloadUrl: "https://github.com/ozanstn1-stack/omnioffice/releases/download/v3.9.0/OmniOffice-Setup-3.9.0.exe",
  downloadName: "OmniOffice-Setup-3.9.0.exe",
  notes: "",
};

type UpdateStore = ReturnType<typeof useUpdate.getState>;

/** Renders the banner with the store preset; the weekly check is not due. */
function renderBanner(patch: Partial<UpdateStore> = {}, info: UpdateInfo = NEWER) {
  useUpdate.setState({ info, phase: "idle", progress: 0, mode: null, error: null, ...patch });
  render(<UpdateBanner />);
}

describe("update banner", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(null);
    listenMock.mockReset();
    listenMock.mockResolvedValue(() => undefined);
    updaterCheckMock.mockReset();
    processRelaunchMock.mockReset();
    processRelaunchMock.mockResolvedValue(undefined);
    android.value = false;
    useSettings.setState({
      settings: { ...DEFAULT_SETTINGS, lastUpdateCheck: Date.now() },
      loaded: true,
    });
  });

  it("offers the in-app install with the in-app hint on desktop", () => {
    renderBanner();
    expect(screen.getByText("OmniOffice 3.9.0 is available.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Download and install" })).toBeEnabled();
    expect(screen.getByText("Updates install inside the app; the browser is not needed.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "What's new" })).toBeInTheDocument();
  });

  it("starts the native install from the button and shows the ready state", async () => {
    const downloadAndInstall = vi.fn(async () => undefined);
    updaterCheckMock.mockResolvedValue({ downloadAndInstall, close: vi.fn(async () => undefined) });
    renderBanner();

    await userEvent.click(screen.getByRole("button", { name: "Download and install" }));

    await waitFor(() => expect(downloadAndInstall).toHaveBeenCalledTimes(1));
    expect(await screen.findByText("Update 3.9.0 installed. Restart to finish.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Restart now" })).toBeInTheDocument();
  });

  it("shows the download percent and the installing state while busy", () => {
    renderBanner({ phase: "downloading", progress: 42 });
    expect(screen.getByRole("button", { name: "Downloading 42%" })).toBeDisabled();

    act(() => useUpdate.setState({ phase: "installing", progress: 100 }));
    expect(screen.getByRole("button", { name: "Installing the update…" })).toBeDisabled();
  });

  it("shows the ready state with a restart button on desktop", async () => {
    renderBanner({ phase: "ready", progress: 100, mode: "auto" });
    expect(screen.getByText("Update 3.9.0 installed. Restart to finish.")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Restart now" }));
    expect(processRelaunchMock).toHaveBeenCalledTimes(1);
  });

  it("shows the ready state without a restart button on Android", () => {
    android.value = true;
    renderBanner({ phase: "ready", progress: 100, mode: "auto" });
    expect(screen.getByText("Update 3.9.0 installed. Restart to finish.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Restart now" })).toBeNull();
  });

  it("shows the browser download and the fallback hint after a fallback", () => {
    renderBanner({ mode: "fallback" });
    expect(screen.getByRole("button", { name: "Download" })).toBeEnabled();
    expect(screen.queryByRole("button", { name: "Download and install" })).toBeNull();
    expect(
      screen.getByText("In-app updates are not configured on this build; the download opens in the browser."),
    ).toBeInTheDocument();
  });

  it("opens the release page on Android when the release has no APK", () => {
    android.value = true;
    renderBanner({}, { ...NEWER, downloadUrl: null });
    expect(screen.getByRole("button", { name: "Open release page" })).toBeEnabled();
    expect(screen.queryByRole("button", { name: "Download and install" })).toBeNull();
  });
});
