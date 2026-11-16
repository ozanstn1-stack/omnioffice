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

interface ImmersiveState {
  immersive: boolean;
  set: (immersive: boolean) => void;
}

export const useReaderImmersive = create<ImmersiveState>((set) => ({
  immersive: false,
  set: (immersive) => set({ immersive }),
}));

/** Waits out a possible second tap, so a double-tap zoom never also toggles. */
export const SINGLE_TAP_DELAY_MS = 320;
/** Scrolling down this far in one go hides the chrome. */
const HIDE_AFTER_SCROLL_PX = 48;

function touchScreen(): boolean {
  try {
    return typeof window !== "undefined" && window.matchMedia?.("(pointer: coarse)").matches === true;
  } catch {
    return false;
  }
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
