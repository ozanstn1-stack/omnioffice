import { useEffect } from "react";

/**
 * Marks the screen as ready to run on `<html data-screen-ready="…">` (with an
 * optional `data-screen-ready-detail`, e.g. the selected file's name) while
 * `ready` holds, and clears it when the screen unmounts or loses its input.
 *
 * The desktop E2E harness waits on this signal instead of counting buttons
 * (#407): "the screen rendered" and "the screen holds its file" used to be
 * indistinguishable, which is what the flaky Deep flows step tripped on.
 */
export function useScreenReady(screen: string, ready: boolean, detail?: string) {
  useEffect(() => {
    if (!ready) return;
    const root = document.documentElement;
    root.dataset.screenReady = screen;
    if (detail) root.dataset.screenReadyDetail = detail;
    else delete root.dataset.screenReadyDetail;
    return () => {
      // A newer screen may have claimed the signal already.
      if (root.dataset.screenReady !== screen) return;
      delete root.dataset.screenReady;
      delete root.dataset.screenReadyDetail;
    };
  }, [screen, ready, detail]);
}
