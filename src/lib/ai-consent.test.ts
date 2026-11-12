import { describe, expect, it } from "vitest";
import { consentScope, consentScopeLabel, isConsentRemembered } from "./ai-consent";

describe("remembered AI data notice (4.5b)", () => {
  it("remembers a consent only for the same provider and host", () => {
    const saved = consentScope("deepseek", "https://API.deepseek.com/v1");
    expect(saved).toBe("deepseek|api.deepseek.com");
    expect(isConsentRemembered(saved, "deepseek", "https://api.deepseek.com")).toBe(true);
    // Another host or another provider asks again.
    expect(isConsentRemembered(saved, "deepseek", "https://gateway.example.com/v1")).toBe(false);
    expect(isConsentRemembered(saved, "openai", "https://api.deepseek.com")).toBe(false);
    // Nothing saved, or settings not loaded yet, is never a consent.
    expect(isConsentRemembered("", "deepseek", "https://api.deepseek.com")).toBe(false);
    expect(isConsentRemembered(saved, undefined, undefined)).toBe(false);
  });

  it("labels the saved scope for the Settings screen", () => {
    expect(consentScopeLabel("ollama|localhost:11434")).toBe("ollama · localhost:11434");
  });
});
