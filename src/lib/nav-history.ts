/**
 * Bridge between the WebView history and the system back button.
 *
 * Android's back button drives `WebView.goBack()` (the `handleBackNavigation`
 * override in MainActivity enables the native callback), so the app records one
 * history entry per screen and turns `popstate` events back into navigation.
 * Overlays (command palette, global search, navigation drawer) push a guard
 * entry so that the first back press closes them instead of leaving the screen.
 *
 * The parsing functions are pure so the popstate logic can be unit tested
 * without a browser.
 */
import type { ScreenId } from "./nav";

const SCREEN_MARKER = "pdfsak-screen";
const OVERLAY_MARKER = "pdfsak-overlay";

/** Screen plus the file list it was entered with. */
export interface NavigationSnapshot {
  screen: ScreenId;
  files: string[];
}

/** History entry recorded for a screen change. */
export interface ScreenHistoryState {
  marker: typeof SCREEN_MARKER;
  screen: ScreenId;
  files: string[];
}

/** History entry recorded while an overlay is open. */
export interface OverlayHistoryState {
  marker: typeof OVERLAY_MARKER;
}

/** Builds the history state stored for a screen visit. */
export function screenHistoryState(screen: ScreenId, files: string[] = []): ScreenHistoryState {
  return { marker: SCREEN_MARKER, screen, files: [...files] };
}

/** Builds the history state stored while an overlay is open. */
export function overlayHistoryState(): OverlayHistoryState {
  return { marker: OVERLAY_MARKER };
}

/** True when `state` is an overlay guard pushed by this module. */
export function isOverlayHistoryState(state: unknown): state is OverlayHistoryState {
  return typeof state === "object" && state !== null && (state as { marker?: unknown }).marker === OVERLAY_MARKER;
}

/** Parses a screen entry; returns null for foreign or malformed states. */
export function parseScreenHistoryState(state: unknown): NavigationSnapshot | null {
  if (typeof state !== "object" || state === null) return null;
  const candidate = state as { marker?: unknown; screen?: unknown; files?: unknown };
  if (candidate.marker !== SCREEN_MARKER || typeof candidate.screen !== "string") return null;
  const files = Array.isArray(candidate.files) ? candidate.files.filter((file): file is string => typeof file === "string") : [];
  return { screen: candidate.screen as ScreenId, files };
}

/**
 * Records a screen visit. Consecutive visits to the same screen replace the
 * current entry (file list refreshes must not stack), screen changes push.
 * Returns the operation performed for easier testing.
 */
export function recordScreenVisit(
  history: Pick<History, "pushState" | "replaceState">,
  previous: NavigationSnapshot | null,
  next: NavigationSnapshot,
): "push" | "replace" {
  const state = screenHistoryState(next.screen, next.files);
  if (previous && previous.screen === next.screen) {
    history.replaceState(state, "");
    return "replace";
  }
  history.pushState(state, "");
  return "push";
}

/** What the app should do when a `popstate` event arrives. */
export type NavigationAction =
  | { kind: "close-overlays" }
  | { kind: "navigate"; screen: ScreenId; files: string[] }
  | { kind: "home" };

/**
 * Maps a history state to an app action. An unknown (null) state means the
 * webview walked past the first app entry, which is the cue to return Home so
 * the next back press can exit the app.
 */
export function navigationActionFromState(state: unknown): NavigationAction {
  if (isOverlayHistoryState(state)) return { kind: "close-overlays" };
  const snapshot = parseScreenHistoryState(state);
  if (snapshot) return { kind: "navigate", screen: snapshot.screen, files: snapshot.files };
  return { kind: "home" };
}
