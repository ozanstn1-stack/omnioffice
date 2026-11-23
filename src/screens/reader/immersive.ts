/**
 * Immersive reading on touch screens: the reader's toolbar and the phone
 * shell's top bar and bottom navigation step aside so the page fills the
 * screen. A single tap on the page brings them back (or hides them again), and
 * scrolling down into the document hides them, like a mobile browser.
 *
 * The flag lives in a tiny store because the shell (App) hides its own bars
 * while the reader owns the gesture.
 */
import { useCallback, useEffect, useRef } from "react";
import { create } from "zustand";
import { isAndroid } from "../../lib/mobile";

interface ImmersiveState {
  immersive: boolean;
  set: (immersive: boolean) => void;
  /**
   * Desktop focus mode: the app sidebar and the reader's side panel step
   * aside and the window goes full screen, so the page gets the whole display.
   */
  focus: boolean;
  setFocus: (focus: boolean) => void;
}

export const useReaderImmersive = create<ImmersiveState>((set) => ({
  immersive: false,
  set: (immersive) => set({ immersive }),
  focus: false,
  setFocus: (focus) => set({ focus }),
}));

/** Puts the native window in (or out of) full screen; a no-op outside Tauri. */
export async function setWindowFullscreen(on: boolean): Promise<void> {
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().setFullscreen(on);
  } catch {
    // Browser preview, Android, or the permission is missing: the layout
    // change alone still gives the page the whole window.
  }
}

/** Waits out a possible second tap, so a double-tap zoom never also toggles. */
export const SINGLE_TAP_DELAY_MS = 320;
/** Scrolling down this far in one go hides the chrome. */
const HIDE_AFTER_SCROLL_PX = 48;

export function touchScreen(): boolean {
  try {
    return typeof window !== "undefined" && window.matchMedia?.("(pointer: coarse)").matches === true;
  } catch {
    return false;
  }
}

/** The keyboard shortcut line is for desktops: hidden on Android and touch screens. */
export function showShortcutHint(): boolean {
  return !isAndroid() && !touchScreen();
}

/**
 * Tap and scroll handling for the reader. `onSingleTap` toggles after the
 * double-tap window; `onDoubleTap` cancels a pending toggle; `onScroll` hides
 * the chrome when the reader scrolls down on a touch screen.
 */
export function useImmersiveReader() {
  const set = useReaderImmersive((state) => state.set);
  const pending = useRef<number | null>(null);
  const lastTop = useRef(0);

  const cancel = useCallback(() => {
    if (pending.current !== null) window.clearTimeout(pending.current);
    pending.current = null;
  }, []);

  // Leaving the reader always restores the shell.
  useEffect(
    () => () => {
      cancel();
      set(false);
    },
    [cancel, set],
  );

  const onSingleTap = useCallback(() => {
    cancel();
    pending.current = window.setTimeout(() => {
      pending.current = null;
      set(!useReaderImmersive.getState().immersive);
    }, SINGLE_TAP_DELAY_MS);
  }, [cancel, set]);

  const onScroll = useCallback(
    (top: number) => {
      const delta = top - lastTop.current;
      if (Math.abs(delta) < HIDE_AFTER_SCROLL_PX) return;
      lastTop.current = top;
      if (delta > 0 && top > HIDE_AFTER_SCROLL_PX && touchScreen()) set(true);
    },
    [set],
  );

  return { onSingleTap, onDoubleTap: cancel, onScroll };
}
