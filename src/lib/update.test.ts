import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { createElement } from "react";
import { DEFAULT_SETTINGS, type UpdateInfo } from "./types";

const updateCheck = vi.fn<() => Promise<UpdateInfo>>();
const updateOpen = vi.fn(async (_url: string) => undefined);
const saveSettings = vi.fn(async (_settings: unknown) => undefined);

vi.mock("./api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./api")>()),
  updateCheck: () => updateCheck(),
  updateOpen: (url: string) => updateOpen(url),
  saveSettings: (settings: unknown) => saveSettings(settings),
}));

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

describe("update check", () => {
  beforeEach(() => {
    updateCheck.mockReset();
    updateOpen.mockClear();
    saveSettings.mockClear();
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS }, loaded: true });
    useUpdate.setState({ info: null, checking: false, error: null });
  });

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

  it("shows the banner, opens the download and remembers a dismissal", async () => {
    updateCheck.mockResolvedValue(NEWER);
    render(createElement(UpdateBanner));
    expect(await screen.findByText("OmniOffice 3.9.0 is available.")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Download" }));
    expect(updateOpen).toHaveBeenCalledWith(NEWER.downloadUrl);

    await userEvent.click(screen.getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByText("OmniOffice 3.9.0 is available.")).not.toBeInTheDocument());
    expect(useSettings.getState().settings.dismissedUpdate).toBe("3.9.0");
  });
});
