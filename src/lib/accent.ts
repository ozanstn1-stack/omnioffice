import type { Settings } from "./types";

/** The four accent colours of the approved design (Settings > Appearance). */
export const ACCENTS = ["indigo", "blue", "teal", "pink"] as const;
export type Accent = (typeof ACCENTS)[number];

/** Swatch colour shown in the picker (the light theme's accent). */
export const ACCENT_SWATCH: Record<Accent, string> = {
  indigo: "#4f46e5",
  blue: "#2563eb",
  teal: "#0f766e",
  pink: "#be185d",
};

/**
 * Marks the root with the chosen accent; styles.css maps `data-accent` to the
 * light and dark accent tokens. Indigo is the default palette, and the
 * midnight and paper themes keep their own accents, so neither gets a mark.
 */
export function paintAccent(root: HTMLElement, accent: Accent | undefined, theme: Settings["theme"]): void {
  if (!accent || accent === "indigo" || theme === "midnight" || theme === "paper") {
    delete root.dataset.accent;
  } else {
    root.dataset.accent = accent;
  }
}
