import { afterEach, describe, expect, it } from "vitest";

import { FEATURE_FLAGS, flagEnabled } from "./flags";

afterEach(() => {
  localStorage.clear();
});

describe("feature flags", () => {
  it("ships the document scanner without a flag", () => {
    // The camera capture and corner picker landed in 6.3.2.
    expect(Object.keys(FEATURE_FLAGS)).not.toContain("experimentalScanner");
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
    localStorage.setItem("flag:experimentalKvkkWizard", "1");
    expect(flagEnabled("experimentalKvkkWizard")).toBe(true);
    localStorage.setItem("flag:experimentalKvkkWizard", "0");
    expect(flagEnabled("experimentalKvkkWizard")).toBe(false);
    localStorage.setItem("flag:experimentalUdf", "1");
    expect(flagEnabled("experimentalUdf")).toBe(true);
  });

  it("ignores an unknown override value", () => {
    localStorage.setItem("flag:experimentalKvkkWizard", "yes");
    expect(flagEnabled("experimentalKvkkWizard")).toBe(false);
  });
});
