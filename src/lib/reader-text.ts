/**
 * Reader text layer and speech helpers.
 *
 * The Reader shows pages as bitmaps, so text is not selectable on its own. The
 * text layer overlays transparent boxes (one per PDF text run) on top of the
 * bitmap: the browser then handles selection, copy and accessibility for free.
 * Everything here is pure so the geometry and the speech detection can be
 * tested in jsdom without a real PDF or a real speech engine.
 */
import type { TextRunInfo } from "./api";

export interface TextLayerGeometry {
  display_width_pt: number;
  display_height_pt: number;
}

/** One positioned overlay box, in CSS pixels, over a page rendered at a width. */
export interface TextLayerItem {
  text: string;
  left: number;
  top: number;
  width: number;
  height: number;
  fontSize: number;
}

/**
 * Converts text runs (PDF user space, y growing upwards from the page bottom)
 * into absolutely positioned overlay boxes for a page displayed at `cssWidth`.
 * The vertical position is the run's baseline shifted by its font size, which
 * is close enough for selection; a rotated page is not corrected.
 */
export function textLayerItems(
  runs: readonly TextRunInfo[],
  geometry: TextLayerGeometry,
  cssWidth: number,
): TextLayerItem[] {
  const widthPt = geometry.display_width_pt || 1;
  const heightPt = geometry.display_height_pt || 1;
  const scale = cssWidth / widthPt;
  const items: TextLayerItem[] = [];
  for (const run of runs) {
    if (!run.text) continue;
    const sizePt = Math.max(1, run.fontSizePt);
    const fontSize = sizePt * scale;
    items.push({
      text: run.text,
      left: run.x * scale,
      top: (heightPt - run.y - sizePt) * scale,
      width: Math.max(1, run.widthPt) * scale,
      height: fontSize,
      fontSize,
    });
  }
  return items;
}

/**
 * The page's text as one string. Runs are joined using their horizontal
 * positions: a gap wider than a quarter of the font size becomes a space, so a
 * word split across runs (kerning) stays glued while separate words are
 * separated. Hyphenation and punctuation stay intact.
 */
export function pageTextFromRuns(runs: readonly TextRunInfo[]): string {
  let out = "";
  let previousEnd: number | null = null;
  let previousPage: number | null = null;
  for (const run of runs) {
    if (!run.text) continue;
    const sameLine = previousEnd !== null && previousPage === run.page;
    const gap = sameLine ? run.x - (previousEnd as number) : 0;
    const threshold = Math.max(1, run.fontSizePt * 0.25);
    if (out && sameLine && gap > threshold) out += " ";
    out += run.text;
    previousEnd = run.x + Math.max(0, run.widthPt);
    previousPage = run.page;
  }
  return out.trim();
}

/** The subset of `window` the speech detection reads. */
export interface SpeechWindow {
  speechSynthesis?: {
    speak?: unknown;
    cancel?: unknown;
    pause?: unknown;
    resume?: unknown;
  } | null;
  SpeechSynthesisUtterance?: unknown;
}

/**
 * True when the Web Speech synthesis API is present. The Reader disables the
 * "read aloud" controls when this is false, rather than offering a button that
 * silently does nothing.
 */
export function speechSupported(win: SpeechWindow | undefined | null): boolean {
  if (!win) return false;
  const synth = win.speechSynthesis;
  return Boolean(
    synth &&
    typeof synth.speak === "function" &&
    typeof synth.cancel === "function" &&
    typeof win.SpeechSynthesisUtterance === "function",
  );
}

/** Clamps a speech rate to the range the control offers (0.5×–2×). */
export function clampSpeechRate(rate: number): number {
  if (!Number.isFinite(rate)) return 1;
  return Math.min(2, Math.max(0.5, Math.round(rate * 10) / 10));
}
