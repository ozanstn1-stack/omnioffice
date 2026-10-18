/**
 * Feature flags for work that is not finished yet.
 *
 * A half-built feature is **hidden** rather than shipped broken. The current
 * examples: the document scanner (the screen is a scaffold; camera capture is a
 * v6.0 deliverable) and the plugin `batch-step` hook (the manifest surface
 * exists, the Rust recipe engine does not call it yet).
 *
 * Development override: `localStorage.setItem("flag:experimentalScanner", "1")`
 * turns a flag on (or `"0"` forces a shipped feature off); the app reads it on
 * start. Anything else is ignored.
 */
export const FEATURE_FLAGS = {
  /** Document scanner; camera capture + perspective correction shipped in v6.0. */
  experimentalScanner: true,
} as const;

export type FeatureFlag = keyof typeof FEATURE_FLAGS;

/** True when the flag is on, honouring a `localStorage` override. */
export function flagEnabled(name: FeatureFlag): boolean {
  if (typeof localStorage !== "undefined") {
    const override = localStorage.getItem(`flag:${name}`);
    if (override === "1") return true;
    if (override === "0") return false;
  }
  return FEATURE_FLAGS[name];
}
