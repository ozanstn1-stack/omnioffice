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
 *
 * Why the handler lives in a ref: the Rust side truncates the queue before it
 * answers, so a drained path exists nowhere else. The effect used to depend on
 * `onPaths`; on a cold start from "Open with" the first drain was still in
 * flight when the settings loaded and App passed a new handler, the cleanup
 * marked that drain cancelled and its result - the shared document - was
 * dropped, leaving the app on Home (v6.2.9 on a Pixel 8 Pro). The poll now
 * starts once and every drained path reaches the current handler.
 *
 * Recents: an office document is listed under Home's "Recent" once it opens
 * (`rememberOfficePath`), but a PDF or image shared from WhatsApp went to the
 * reader without a trace and could only be found again by sharing it anew.
 * Every other drained file is now remembered here, as soon as it arrives.
 */
import { useEffect, useRef } from "react";
import { callPlain } from "./typed-call";
import { fileBaseName, isOfficeDocument } from "./format";
import { isAndroid } from "./mobile";
import { useRecent } from "./store";
import { parseIncomingError } from "./incoming";
import { recordOpenWith } from "./open-with-diagnostics";

/** Poll interval while the activity is visible. */
const POLL_MS = 2500;

/** One early retry after mount: the copy thread may still be writing. */
const EARLY_RETRY_MS = 900;

/** Recent-files tag for documents another app handed over. */
export const OPEN_WITH_RECENT_TOOL = "_openWith";

/**
 * Lists drained files under Home's "Recent". Shortcut and refusal tokens are
 * skipped, and so are office documents: those are remembered once they open.
 */
export function rememberIncomingPaths(paths: string[]): void {
  const timestamp = Math.floor(Date.now() / 1000);
  for (const path of paths) {
    if (path.startsWith("__action:") || path.startsWith("__error:") || isOfficeDocument(path)) continue;
    void useRecent.getState().add({ path, fileName: fileBaseName(path), tool: OPEN_WITH_RECENT_TOOL, timestamp });
  }
}

/**
 * Starts draining the Android pending-open queue for as long as the calling
 * component is mounted. `onPaths` receives every newly seen entry, including
 * `__action:*` shortcut tokens and `__error:*` refusal tokens, in queue order.
 * No-op on desktop and when the command is unavailable.
 */
export function useAndroidPendingOpen(onPaths: (paths: string[]) => void): void {
  const onPathsRef = useRef(onPaths);
  useEffect(() => {
    onPathsRef.current = onPaths;
  }, [onPaths]);

  useEffect(() => {
    if (!isAndroid()) return;
    const seen = new Set<string>();
    const drain = () => {
      void callPlain((c) => c.androidTakePendingOpen())
        .then((paths) => {
          // No `cancelled` check: the queue is already empty on the Rust side,
          // so a result that arrives after a cleanup is still delivered.
          if (!paths.length) return;
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
          if (fresh.length > 0) {
            rememberIncomingPaths(fresh);
            onPathsRef.current(fresh);
          }
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
      window.clearTimeout(retry);
      window.clearInterval(poll);
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, []);
}
