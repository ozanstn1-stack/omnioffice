import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { createElement } from "react";
import { DEFAULT_SETTINGS, type UpdateInfo } from "./types";

// Tauri, the updater plugin and the platform are mocked with configurable
// spies; the store's own logic (weekly check, install pipeline, fallback and
// progress mapping) runs for real.
const { invokeMock, listenMock, updaterCheckMock, processRelaunchMock, android } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  listenMock: vi.fn(),
  updaterCheckMock: vi.fn(),
  processRelaunchMock: vi.fn(),
  android: { value: false },
}));

const updateCheck = vi.fn<() => Promise<UpdateInfo>>();
const updateOpen = vi.fn(async (_url: string) => undefined);
const saveSettings = vi.fn(async (_settings: unknown) => undefined);

vi.mock("./api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./api")>()),
  updateCheck: () => updateCheck(),
  updateOpen: (url: string) => updateOpen(url),
  saveSettings: (settings: unknown) => saveSettings(settings),
}));
vi.mock("./mobile", () => ({ isAndroid: () => android.value }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: listenMock }));
vi.mock("@tauri-apps/plugin-updater", () => ({ check: updaterCheckMock }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: processRelaunchMock }));

import { useSettings } from "./store";
import { UPDATE_CHECK_INTERVAL_MS, shouldAnnounce, updateCheckDue, useUpdate } from "./update";
import { UpdateBanner } from "../components/update-banner";

const NEWER: UpdateInfo = {
  current: "3.8.3",
  latest: "3.9.0",
  newer: true,
  releaseUrl: "https://github.com/ozanstn1-stack/omnioffice/releases/tag/v3.9.0",
  downloadUrl: "https://github.com/ozanstn1-stack/omnioffice/releases/download/v3.9.0/OmniOffice-Setup-3.9.0.exe",
  downloadName: "OmniOffice-Setup-3.9.0.exe",
  notes: "",
};

type UpdaterEvent =
  | { event: "Started"; data: { contentLength?: number } }
  | { event: "Progress"; data: { chunkLength: number } }
  | { event: "Finished" };

type ProgressHandler = (event: { payload: { received: number; total: number } }) => void;

let progressHandler: ProgressHandler | null = null;

beforeEach(() => {
  updateCheck.mockReset();
  updateOpen.mockClear();
  saveSettings.mockClear();
  updaterCheckMock.mockReset();
  processRelaunchMock.mockReset();
  processRelaunchMock.mockResolvedValue(undefined);
  android.value = false;
  progressHandler = null;
  listenMock.mockReset();
  listenMock.mockImplementation(async (_event: string, handler: ProgressHandler) => {
    progressHandler = handler;
    return () => {
      progressHandler = null;
    };
  });
  invokeMock.mockReset();
  invokeMock.mockResolvedValue(null);
  useSettings.setState({ settings: { ...DEFAULT_SETTINGS }, loaded: true });
  useUpdate.setState({ info: null, checking: false, error: null, phase: "idle", progress: 0, mode: null });
});

describe("update check", () => {
  it("is due weekly, and never when switched off", () => {
    const now = 10 * UPDATE_CHECK_INTERVAL_MS;
    expect(updateCheckDue({ ...DEFAULT_SETTINGS, lastUpdateCheck: 0 }, now)).toBe(true);
    expect(updateCheckDue({ ...DEFAULT_SETTINGS, lastUpdateCheck: now - 1000 }, now)).toBe(false);
    expect(updateCheckDue({ ...DEFAULT_SETTINGS, lastUpdateCheck: now - UPDATE_CHECK_INTERVAL_MS }, now)).toBe(true);
    expect(updateCheckDue({ ...DEFAULT_SETTINGS, updateCheck: false }, now)).toBe(false);
  });

  it("announces only a newer version that was not dismissed", () => {
    expect(shouldAnnounce(NEWER, DEFAULT_SETTINGS)).toBe(true);
    expect(shouldAnnounce({ ...NEWER, newer: false }, DEFAULT_SETTINGS)).toBe(false);
    expect(shouldAnnounce(NEWER, { ...DEFAULT_SETTINGS, dismissedUpdate: "3.9.0" })).toBe(false);
    expect(shouldAnnounce(null, DEFAULT_SETTINGS)).toBe(false);
  });

  it("skips an automatic check that is not due but always runs a manual one", async () => {
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS, lastUpdateCheck: Date.now() } });
    updateCheck.mockResolvedValue(NEWER);
    expect(await useUpdate.getState().check(false)).toBeNull();
    expect(updateCheck).not.toHaveBeenCalled();

    expect(await useUpdate.getState().check(true)).toEqual(NEWER);
    expect(updateCheck).toHaveBeenCalledTimes(1);
    expect(useSettings.getState().settings.lastUpdateCheck).toBeGreaterThan(0);
  });

  it("keeps automatic failures quiet and reports manual ones", async () => {
    updateCheck.mockRejectedValue("offline");
    expect(await useUpdate.getState().check(false)).toBeNull();
    expect(useUpdate.getState().error).toBeNull();
    await useUpdate.getState().check(true);
    expect(useUpdate.getState().error).toBe("offline");
  });

  it("shows the banner, installs, falls back to the browser and remembers a dismissal", async () => {
    updateCheck.mockResolvedValue(NEWER);
    // No updater configuration on this build: the install click must land on
    // the browser download and say why.
    updaterCheckMock.mockRejectedValue("missing pubkey");
    render(createElement(UpdateBanner));
    expect(await screen.findByText("OmniOffice 3.9.0 is available.")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Download and install" }));
    await waitFor(() => expect(updateOpen).toHaveBeenCalledWith(NEWER.downloadUrl));
    expect(
      await screen.findByText("In-app updates are not configured on this build; the download opens in the browser."),
    ).toBeInTheDocument();
    expect(useUpdate.getState().mode).toBe("fallback");

    await userEvent.click(screen.getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByText("OmniOffice 3.9.0 is available.")).not.toBeInTheDocument());
    expect(useSettings.getState().settings.dismissedUpdate).toBe("3.9.0");
  });
});

describe("in-app install", () => {
  it("runs the Tauri updater on desktop and maps its progress events", async () => {
    useUpdate.setState({ info: NEWER });
    const downloadAndInstall = vi.fn(async (onEvent: (event: UpdaterEvent) => void) => {
      onEvent({ event: "Started", data: { contentLength: 200 } });
      onEvent({ event: "Progress", data: { chunkLength: 50 } });
      expect(useUpdate.getState().progress).toBe(25);
      onEvent({ event: "Progress", data: { chunkLength: 150 } });
      expect(useUpdate.getState().progress).toBe(100);
      onEvent({ event: "Finished" });
      expect(useUpdate.getState().phase).toBe("installing");
    });
    const close = vi.fn(async () => undefined);
    updaterCheckMock.mockResolvedValue({ downloadAndInstall, close });

    await useUpdate.getState().install();

    expect(updaterCheckMock).toHaveBeenCalledTimes(1);
    expect(downloadAndInstall).toHaveBeenCalledTimes(1);
    expect(close).toHaveBeenCalledTimes(1);
    expect(updateOpen).not.toHaveBeenCalled();
    const state = useUpdate.getState();
    expect(state.phase).toBe("ready");
    expect(state.progress).toBe(100);
    expect(state.mode).toBe("auto");
  });

  it("falls back to the browser download when the updater check fails", async () => {
    useUpdate.setState({ info: NEWER });
    updaterCheckMock.mockRejectedValue("no pubkey configured");

    await useUpdate.getState().install();

    expect(updateOpen).toHaveBeenCalledWith(NEWER.downloadUrl);
    const state = useUpdate.getState();
    expect(state.mode).toBe("fallback");
    expect(state.phase).toBe("idle");
    expect(state.error).toBeNull();
  });

  it("falls back when the configured manifest announces no update", async () => {
    useUpdate.setState({ info: NEWER });
    updaterCheckMock.mockResolvedValue(null);

    await useUpdate.getState().install();

    expect(updateOpen).toHaveBeenCalledWith(NEWER.downloadUrl);
    expect(useUpdate.getState().mode).toBe("fallback");
  });

  it("downloads and installs the APK on Android, tracking update:progress", async () => {
    android.value = true;
    useUpdate.setState({ info: NEWER });
    let midway = 0;
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "update_download_apk") {
        progressHandler?.({ payload: { received: 30, total: 60 } });
        midway = useUpdate.getState().progress;
        return "/cache/update.apk";
      }
      if (command === "update_install_apk") return undefined;
      return null;
    });

    await useUpdate.getState().install();

    expect(invokeMock).toHaveBeenCalledWith("update_download_apk", {
      url: NEWER.downloadUrl,
      expectedSha256: null,
    });
    expect(invokeMock).toHaveBeenCalledWith("update_install_apk", { path: "/cache/update.apk" });
    expect(midway).toBe(50);
    expect(updaterCheckMock).not.toHaveBeenCalled();
    expect(updateOpen).not.toHaveBeenCalled();
    const state = useUpdate.getState();
    expect(state.phase).toBe("ready");
    expect(state.progress).toBe(100);
    expect(state.mode).toBe("auto");
  });

  it("passes the release digest to the APK downloader when the backend provides one", async () => {
    android.value = true;
    useUpdate.setState({ info: { ...NEWER, sha256: "abc123" } });
    invokeMock.mockResolvedValueOnce("/cache/update.apk").mockResolvedValueOnce(undefined);

    await useUpdate.getState().install();

    expect(invokeMock).toHaveBeenCalledWith("update_download_apk", {
      url: NEWER.downloadUrl,
      expectedSha256: "abc123",
    });
  });

  it("reports an Android download failure without opening the browser", async () => {
    android.value = true;
    useUpdate.setState({ info: NEWER });
    invokeMock.mockRejectedValue("checksum mismatch");

    await useUpdate.getState().install();

    const state = useUpdate.getState();
    expect(state.phase).toBe("error");
    expect(state.error).toBe("checksum mismatch");
    expect(updateOpen).not.toHaveBeenCalled();
  });

  it("relaunches on desktop only", async () => {
    await useUpdate.getState().relaunch();
    expect(processRelaunchMock).toHaveBeenCalledTimes(1);
    android.value = true;
    await useUpdate.getState().relaunch();
    expect(processRelaunchMock).toHaveBeenCalledTimes(1);
  });
});
