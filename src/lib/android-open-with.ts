/**
 * Android open-with queue drain.
 *
 * `MainActivity` copies a shared document into the app cache on a background
 * thread and appends its path to `cacheDir/pending-open.txt`; this hook polls
 * that queue through `android_take_pending_open` and hands new entries to the
 * app's router. It was extracted from `App.tsx` (which is at its line budget)
 * together with the pipeline trace that makes an on-device failure visible in
 * Settings -> Diagnostics.
 *
 * Why a poll and not one event: a share sent to an app that is already on
 * screen (`singleTask` -> `onNewIntent`) never triggers a visibility change, so
 * the interval is what makes that case work. The queue read is a small file
 * read and returns nothing while it is empty.
 */
import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { isAndroid } from "./mobile";
import { parseIncomingError } from "./incoming";
import { recordOpenWith } from "./open-with-diagnostics";

/** Poll interval while the activity is visible. */
const POLL_MS = 2500;

/** One early retry after mount: the copy thread may still be writing. */
const EARLY_RETRY_MS = 900;

/**
 * Starts draining the Android pending-open queue for as long as the calling
 * component is mounted. `onPaths` receives every newly seen entry, including
 * `__action:*` shortcut tokens and `__error:*` refusal tokens, in queue order.
 * No-op on desktop and when the command is unavailable.
 */
export function useAndroidPendingOpen(onPaths: (paths: string[]) => void): void {
  useEffect(() => {
    if (!isAndroid()) return;
    let cancelled = false;
    const seen = new Set<string>();
    const drain = () => {
      void invoke<string[]>("android_take_pending_open")
        .then((paths) => {
          if (cancelled || !paths.length) return;
          recordOpenWith(`drain:${paths.length}`);
          // Launcher shortcuts arrive as `__action:*` tokens, and the same
          // token is legitimate every time it is used: deduplicating them
          // would let each shortcut fire only once per session. Only real
          // file paths are remembered.
          const fresh = paths.filter((path) => path.startsWith("__action:") || !seen.has(path));
          for (const path of fresh) {
            if (path.startsWith("__error:")) {
              // Keep the reason, not the provider's file name, in the report.
              recordOpenWith(`__error:${parseIncomingError(path)?.reason ?? "UNKNOWN"}`);
            }
            if (!path.startsWith("__action:")) seen.add(path);
          }
          if (fresh.length > 0) onPaths(fresh);
        })
        .catch((error: unknown) => recordOpenWith(`drain-failed:${String(error)}`));
    };
    drain();
    const retry = window.setTimeout(drain, EARLY_RETRY_MS);
    const poll = window.setInterval(() => {
      if (document.visibilityState === "visible") drain();
    }, POLL_MS);
    const onVisibility = () => {
      if (document.visibilityState === "visible") drain();
    };
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      cancelled = true;
      window.clearTimeout(retry);
      window.clearInterval(poll);
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, [onPaths]);
}
