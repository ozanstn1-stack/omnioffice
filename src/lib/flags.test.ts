import { afterEach, describe, expect, it } from "vitest";

import appSource from "../App.tsx?raw";
import { FEATURE_FLAGS, flagEnabled } from "./flags";

afterEach(() => {
  localStorage.clear();
});

describe("feature flags", () => {
  it("enables the scanner by default in v6.0", () => {
    expect(FEATURE_FLAGS.experimentalScanner).toBe(true);
    expect(flagEnabled("experimentalScanner")).toBe(true);
  });

  it("honours a development override", () => {
    localStorage.setItem("flag:experimentalScanner", "0");
    expect(flagEnabled("experimentalScanner")).toBe(false);
    localStorage.setItem("flag:experimentalScanner", "1");
    expect(flagEnabled("experimentalScanner")).toBe(true);
  });

  it("ignores an unknown override value", () => {
    localStorage.setItem("flag:experimentalScanner", "no");
    expect(flagEnabled("experimentalScanner")).toBe(true);
  });

  it("hides the scanner nav entry behind the flag", () => {
    // A weak but useful guard: the nav entry must be gated so the override
    // can still hide it if needed.
    expect(appSource).toContain('hidden: !flagEnabled("experimentalScanner")');
    expect(appSource).toContain("item.hidden");
  });
});
