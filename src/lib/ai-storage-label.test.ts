import { describe, expect, it } from "vitest";
import { storageLabelKey } from "./ai-storage-label";

describe("API key storage labels", () => {
  it("names every storage tier the Rust layer reports", () => {
    expect(storageLabelKey("dpapi")).toBe("ai.keySecure");
    expect(storageLabelKey("keystore")).toBe("ai.keyKeystore");
    expect(storageLabelKey("keyring")).toBe("ai.keyKeyring");
    expect(storageLabelKey("plain")).toBe("ai.keyPlain");
  });

  it("falls back to the missing-key label for an unknown tier", () => {
    expect(storageLabelKey("none")).toBe("ai.keyMissing");
    expect(storageLabelKey("")).toBe("ai.keyMissing");
    expect(storageLabelKey("something-else")).toBe("ai.keyMissing");
  });
});
