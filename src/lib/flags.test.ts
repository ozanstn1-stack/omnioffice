import { afterEach, describe, expect, it } from "vitest";

import appSource from "../App.tsx?raw";
import { FEATURE_FLAGS, flagEnabled } from "./flags";

afterEach(() => {
  localStorage.clear();
});

describe("feature flags", () => {
  it("keeps the experimental scanner hidden by default", () => {
    expect(FEATURE_FLAGS.experimentalScanner).toBe(false);
    expect(flagEnabled("experimentalScanner")).toBe(false);
  });

  it("keeps the unverified formats hidden by default", () => {
    // UDF and the Luca/Logo/Zirve exports are only reopened when their
    // external verification lands (RELEASE_READINESS.md).
    expect(FEATURE_FLAGS.experimentalUdf).toBe(false);
    expect(FEATURE_FLAGS.accountingVendorExport).toBe(false);
    expect(flagEnabled("experimentalUdf")).toBe(false);
    expect(flagEnabled("accountingVendorExport")).toBe(false);
  });

  it("keeps the unfinished screens hidden by default", () => {
    // Document health has fixed inputs and the KVKK wizard is clipboard-only;
    // both stay off until the audit plan's A-phase work lands.
    expect(FEATURE_FLAGS.experimentalDocumentHealth).toBe(false);
    expect(FEATURE_FLAGS.experimentalKvkkWizard).toBe(false);
    expect(flagEnabled("experimentalDocumentHealth")).toBe(false);
    expect(flagEnabled("experimentalKvkkWizard")).toBe(false);
  });

  it("honours a development override", () => {
    localStorage.setItem("flag:experimentalScanner", "1");
    expect(flagEnabled("experimentalScanner")).toBe(true);
    localStorage.setItem("flag:experimentalScanner", "0");
    expect(flagEnabled("experimentalScanner")).toBe(false);
    localStorage.setItem("flag:experimentalUdf", "1");
    expect(flagEnabled("experimentalUdf")).toBe(true);
  });

  it("ignores an unknown override value", () => {
    localStorage.setItem("flag:experimentalScanner", "yes");
    expect(flagEnabled("experimentalScanner")).toBe(false);
  });

  it("hides the scanner nav entry behind the flag", () => {
    // A weak but useful guard: the nav entry must be gated, not deleted, so the
    // deep link still works while the feature stays hidden.
    expect(appSource).toContain('hidden: !flagEnabled("experimentalScanner")');
    expect(appSource).toContain("item.hidden");
  });
});
