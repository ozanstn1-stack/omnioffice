import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { create } from "zustand";
import { updateCheck, updateOpen } from "./api";
import { isAndroid } from "./mobile";
import { useSettings } from "./store";
import type { Settings, UpdateInfo } from "./types";

/** Automatic checks run at most once per this interval. */
export const UPDATE_CHECK_INTERVAL_MS = 7 * 24 * 60 * 60 * 1000;

/** True when the weekly automatic check is due. */
export function updateCheckDue(settings: Settings, now: number): boolean {
  if (!settings.updateCheck) return false;
  return now - (settings.lastUpdateCheck || 0) >= UPDATE_CHECK_INTERVAL_MS;
}

/** True when the banner should be shown for `info`. */
export function shouldAnnounce(info: UpdateInfo | null, settings: Settings): info is UpdateInfo {
  return Boolean(info && info.newer && info.latest !== settings.dismissedUpdate);
}

/** Stage of the in-app install pipeline. */
export type UpdatePhase = "idle" | "checking" | "downloading" | "installing" | "ready" | "error";

/** How the update is delivered: the native updater, or the browser fallback. */
export type UpdateMode = "auto" | "fallback" | null;

/** Payload of the Android downloader's `update:progress` events. */
interface UpdateProgress {
  received: number;
  total: number;
}

interface UpdateState {
  info: UpdateInfo | null;
  checking: boolean;
  error: string | null;
  /** Install pipeline state; "idle" until `install` runs. */
  phase: UpdatePhase;
  /** Download percent, 0-100. */
  progress: number;
  /** Whether the native updater or the browser fallback is carrying the update. */
  mode: UpdateMode;
  /**
   * Asks GitHub for the latest release. Automatic calls (force = false) are
   * skipped unless the weekly check is enabled and due; a manual check always
   * runs. The time of every completed check is stored so the interval holds
   * across restarts. Failures are kept quiet for automatic checks.
   */
  check: (force?: boolean) => Promise<UpdateInfo | null>;
  dismiss: () => Promise<void>;
  /**
   * Downloads and installs the announced update: the Tauri updater on
   * desktop, the APK downloader and system installer on Android. Any desktop
   * failure (missing pubkey or manifest, no update for this target, network)
   * opens the release download in the browser instead.
   */
  install: () => Promise<void>;
  /** Restarts into the installed build (desktop only; Android's installer handles it). */
  relaunch: () => Promise<void>;
}

type SetUpdateState = (partial: Partial<UpdateState>) => void;

/**
 * Downloads the release APK and hands it to the system installer. The
 * SHA-256 is passed through when the backend provided one; when it is absent
 * the native side decides whether it can verify the file.
 */
async function installOnAndroid(info: UpdateInfo, set: SetUpdateState): Promise<void> {
  if (!info.downloadUrl) throw new Error("this release has no APK download");
  let unlisten: UnlistenFn | null = null;
  try {
    unlisten = await listen<UpdateProgress>("update:progress", (event) => {
      const { received, total } = event.payload;
      if (total > 0) set({ progress: Math.min(100, Math.round((received / total) * 100)) });
    });
    const path = await invoke<string>("update_download_apk", {
      url: info.downloadUrl,
      expectedSha256: info.sha256 ?? null,
    });
    set({ phase: "installing", progress: 100 });
    await invoke("update_install_apk", { path });
  } finally {
    unlisten?.();
  }
}

/**
 * Runs the Tauri updater. On Windows `downloadAndInstall` exits the app when
 * the installer launches, so `ready` is only reached on platforms where the
 * call returns (the task's plugin events need no separate relaunch there).
 */
async function installOnDesktop(set: SetUpdateState): Promise<void> {
  const { check } = await import("@tauri-apps/plugin-updater");
  const update = await check();
  if (!update) {
    // The configured manifest does not announce this release (uploads lag
    // or a different target); the browser download still gets the user the
    // new build.
    throw new Error("the configured update manifest does not announce this release");
  }
  let received = 0;
  let contentLength = 0;
  await update.downloadAndInstall((event) => {
    if (event.event === "Started") {
      contentLength = event.data.contentLength ?? 0;
    } else if (event.event === "Progress") {
      received += event.data.chunkLength;
      if (contentLength > 0) set({ progress: Math.min(100, Math.round((received / contentLength) * 100)) });
    } else {
      // The installer takes over from here.
      set({ phase: "installing", progress: 100 });
    }
  });
  void update.close().catch(() => undefined);
}

export const useUpdate = create<UpdateState>((set, get) => ({
  info: null,
  checking: false,
  error: null,
  phase: "idle",
  progress: 0,
  mode: null,
  check: async (force = false) => {
    const { settings, update } = useSettings.getState();
    if (!force && !updateCheckDue(settings, Date.now())) return null;
    if (get().checking) return get().info;
    set({ checking: true, error: null });
    try {
      const info = await updateCheck();
      set({ info, checking: false });
      await update({ lastUpdateCheck: Date.now() });
      return info;
    } catch (error) {
      set({ checking: false, error: force ? String(error) : null });
      return null;
    }
  },
  dismiss: async () => {
    const info = get().info;
    if (info) await useSettings.getState().update({ dismissedUpdate: info.latest });
  },
  install: async () => {
    const info = get().info;
    const { phase } = get();
    if (!info?.newer || phase === "downloading" || phase === "installing") return;
    set({ phase: "downloading", progress: 0, mode: "auto", error: null });
    if (isAndroid()) {
      try {
        await installOnAndroid(info, set);
        set({ phase: "ready", progress: 100 });
      } catch (error) {
        set({ phase: "error", error: String(error) });
      }
      return;
    }
    try {
      await installOnDesktop(set);
      set({ phase: "ready", progress: 100 });
    } catch (error) {
      // Missing pubkey or manifest, no update for this target or no network:
      // the browser download is the fallback every build has.
      set({ mode: "fallback", progress: 0 });
      try {
        await updateOpen(info.downloadUrl ?? info.releaseUrl);
        set({ phase: "idle" });
      } catch (openError) {
        set({ phase: "error", error: String(openError ?? error) });
      }
    }
  },
  relaunch: async () => {
    if (isAndroid()) return;
    const { relaunch } = await import("@tauri-apps/plugin-process");
    await relaunch();
  },
}));
