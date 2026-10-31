/**
 * Phone layout and on-screen keyboard plumbing (D8 mobile ergonomics).
 *
 * The app switches to the phone shell (drawer navigation, bottom tabs, bottom
 * action bar) below 900 px, which is the same breakpoint App.tsx uses. The
 * keyboard helpers are shared by the bottom navigation and the new screen
 * action bar: on Android the keyboard shrinks `visualViewport` but not the
 * layout viewport, so a bottom bar is covered unless it is lifted by the
 * measured inset.
 */
import { useEffect, useState } from "react";

/** Window width below which the phone shell is used (matches App.tsx). */
export const NARROW_BREAKPOINT = 900;

/** True on a phone-sized window; updates when the window is resized. */
export function useNarrowLayout(): boolean {
  const [narrow, setNarrow] = useState(() => typeof window !== "undefined" && window.innerWidth < NARROW_BREAKPOINT);
  useEffect(() => {
    const update = () => setNarrow(window.innerWidth < NARROW_BREAKPOINT);
    update();
    window.addEventListener("resize", update);
    return () => window.removeEventListener("resize", update);
  }, []);
  return narrow;
}

/**
 * The on-screen keyboard takes a large share of the visual viewport; a bottom
 * bar would sit on top of the keyboard and push the field away.
 */
export function keyboardLikelyOpen(viewportHeight: number, windowHeight: number): boolean {
  return windowHeight > 0 && viewportHeight < windowHeight * 0.75;
}

export function useKeyboardOpen(): boolean {
  const [open, setOpen] = useState(false);
  useEffect(() => {
    const viewport = typeof window !== "undefined" ? window.visualViewport : null;
    if (!viewport) return;
    const update = () => setOpen(keyboardLikelyOpen(viewport.height, window.innerHeight));
    update();
    viewport.addEventListener("resize", update);
    return () => viewport.removeEventListener("resize", update);
  }, []);
  return open;
}

/** Pixels of the layout viewport the on-screen keyboard currently covers. */
export function keyboardInset(): number {
  const viewport = typeof window !== "undefined" ? window.visualViewport : null;
  if (!viewport) return 0;
  return Math.max(0, Math.round(window.innerHeight - viewport.height - viewport.offsetTop));
}

/**
 * Publishes the keyboard height as the `--keyboard-inset` CSS variable, so the
 * bottom action bar can lift itself above the keyboard instead of hiding a
 * Save button behind it. Resets to 0 px when the keyboard closes.
 */
export function useKeyboardInset(): void {
  useEffect(() => {
    const viewport = typeof window !== "undefined" ? window.visualViewport : null;
    if (!viewport) return;
    const update = () => document.documentElement.style.setProperty("--keyboard-inset", `${keyboardInset()}px`);
    update();
    viewport.addEventListener("resize", update);
    viewport.addEventListener("scroll", update);
    return () => {
      viewport.removeEventListener("resize", update);
      viewport.removeEventListener("scroll", update);
      document.documentElement.style.removeProperty("--keyboard-inset");
    };
  }, []);
}

/** True for the fields the scroll-into-view safety net applies to. */
export function isTextField(target: Element | null): target is HTMLElement {
  if (!(target instanceof HTMLElement)) return false;
  return /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName) || target.isContentEditable === true;
}

/**
 * Scrolls the focused field into view. The bottom bar and the bottom tabs are
 * `flex-shrink: 0` siblings, so the field can end up under the keyboard; with
 * the keyboard animation finished the browser can scroll it back to centre.
 */
export function scrollFieldIntoView(
  target: Element | null,
  scroll = (element: HTMLElement) => element.scrollIntoView({ block: "center", behavior: "smooth" }),
): boolean {
  if (!isTextField(target)) return false;
  scroll(target);
  return true;
}

/** A global safety net: every focused field is scrolled clear of the keyboard. */
export function useFocusScrollIntoView(): void {
  useEffect(() => {
    const onFocus = (event: FocusEvent) => {
      if (!isTextField(event.target as Element | null)) return;
      const target = event.target as HTMLElement;
      // The keyboard's open animation moves the viewport after focus; scrolling
      // immediately would measure the old, still-full height.
      window.setTimeout(() => scrollFieldIntoView(target), 250);
    };
    document.addEventListener("focusin", onFocus);
    return () => document.removeEventListener("focusin", onFocus);
  }, []);
}
