import { create } from "zustand";
import { updateCheck } from "./api";
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

interface UpdateState {
  info: UpdateInfo | null;
  checking: boolean;
  error: string | null;
  /**
   * Asks GitHub for the latest release. Automatic calls (force = false) are
   * skipped unless the weekly check is enabled and due; a manual check always
   * runs. The time of every completed check is stored so the interval holds
   * across restarts. Failures are kept quiet for automatic checks.
   */
  check: (force?: boolean) => Promise<UpdateInfo | null>;
  dismiss: () => Promise<void>;
}

export const useUpdate = create<UpdateState>((set, get) => ({
  info: null,
  checking: false,
  error: null,
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
}));
