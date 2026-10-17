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

  it("honours a development override", () => {
    localStorage.setItem("flag:experimentalScanner", "1");
    expect(flagEnabled("experimentalScanner")).toBe(true);
    localStorage.setItem("flag:experimentalScanner", "0");
    expect(flagEnabled("experimentalScanner")).toBe(false);
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
