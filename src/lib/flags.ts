/**
 * Feature flags for work that is not finished yet.
 *
 * A half-built feature is **hidden** rather than shipped broken. The current
 * examples: the document scanner (camera capture landed, but it still needs a
 * device test) and the two formats whose external verification is missing:
 * UYAP `.udf` (no real UYAP file has ever been round-tripped) and the
 * Luca/Logo/Zirve accounting exports (no real import template exists). Both
 * stay behind an off-by-default flag with their code and tests intact; see
 * RELEASE_READINESS.md for the reopen criteria.
 *
 * Development override: `localStorage.setItem("flag:experimentalScanner", "1")`
 * turns a flag on (or `"0"` forces a shipped feature off); the app reads it on
 * start. Anything else is ignored.
 */
export const FEATURE_FLAGS = {
  /** Document scanner; camera capture is a v6.0 deliverable. */
  experimentalScanner: false,
  /** UYAP `.udf`: hidden until real UYAP files verify the round trip. */
  experimentalUdf: false,
  /** Luca/Logo/Zirve exports: hidden until a real import template exists. */
  accountingVendorExport: false,
  /** Document health: the checks are real, the file inputs are still fixed. */
  experimentalDocumentHealth: false,
  /** KVKK wizard: templates are real, the output is clipboard-only. */
  experimentalKvkkWizard: false,
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
