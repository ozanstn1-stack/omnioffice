import { describe, expect, it } from "vitest";
import {
  PROVIDER_PRESETS,
  consentKeys,
  isLocalProvider,
  parseProviderId,
  plainProviderName,
  providerDisplayName,
} from "./ai-providers";
import { makeTranslate } from "./i18n";

describe("AI provider presets", () => {
  it("offers an Anthropic (Claude) preset with the API defaults", () => {
    const preset = PROVIDER_PRESETS.find((entry) => entry.id === "anthropic");
    expect(preset).toBeDefined();
    expect(preset?.label).toBe("Anthropic (Claude)");
    expect(preset?.baseUrl).toBe("https://api.anthropic.com");
    expect(preset?.model).toBe("claude-opus-5-5");
    expect(preset?.model).not.toMatch(/\d{8}$/);
    expect(preset?.note).toContain("console.anthropic.com");
    expect(preset?.keyPlaceholder).toBe("sk-ant-…");
    expect(preset?.capabilities).toEqual({
      chat: true,
      embeddings: false,
      vision: true,
      structured_output: false,
      streaming: true,
    });
  });

  it("keeps the other presets and gives every preset a unique id", () => {
    const ids = PROVIDER_PRESETS.map((entry) => entry.id);
    expect(ids).toEqual(["deepseek", "anthropic", "openai_compatible", "ollama", "gemini", "custom"]);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("parses provider ids and aliases", () => {
    expect(parseProviderId("anthropic")).toBe("anthropic");
    expect(parseProviderId("Claude")).toBe("anthropic");
    expect(parseProviderId("Anthropic Claude")).toBe("anthropic");
    expect(parseProviderId("local")).toBe("ollama");
    expect(parseProviderId("google")).toBe("gemini");
    expect(parseProviderId(undefined)).toBe("deepseek");
    expect(parseProviderId("nonsense")).toBe("deepseek");
  });
});

describe("provider-aware consent text", () => {
  const en = makeTranslate("en");
  const tr = makeTranslate("tr");

  it("names the selected cloud provider in the consent and the privacy notice", () => {
    for (const provider of ["Anthropic (Claude)", "DeepSeek", "Google Gemini"]) {
      const keys = consentKeys("anthropic");
      expect(keys.consent).toBe("ai.consent");
      expect(en(keys.consent, { provider })).toBe(`I understand that the extracted text will be sent to ${provider}`);
      expect(en(keys.privacyBody, { provider })).toContain(`sent to ${provider}`);
      expect(tr(keys.consent, { provider })).toContain(provider);
    }
  });

  it("no longer hard-codes DeepSeek", () => {
    const text = en("ai.consent", { provider: "Anthropic (Claude)" });
    expect(text).not.toContain("DeepSeek");
    for (const key of ["ai.consent", "ai.privacyBody", "ai.title", "ai.addKeyFirst", "settings.aiKey"]) {
      expect(en(key, { provider: "X" })).not.toContain("DeepSeek");
      expect(tr(key, { provider: "X" })).not.toContain("DeepSeek");
    }
  });

  it("says Ollama stays on this computer or local network", () => {
    expect(isLocalProvider("ollama")).toBe(true);
    expect(isLocalProvider("anthropic")).toBe(false);
    const keys = consentKeys("ollama");
    expect(keys.consent).toBe("ai.consentLocal");
    expect(keys.privacyBody).toBe("ai.privacyBodyLocal");
    const provider = plainProviderName("Ollama (local)");
    expect(provider).toBe("Ollama");
    expect(en(keys.consent, { provider })).toBe(
      "I understand that the extracted text will be processed by Ollama on this computer or local network",
    );
    expect(en(keys.privacyBody, { provider })).toContain("nothing goes to the internet");
    expect(tr(keys.consent, { provider })).toContain("yerel ağda");
    expect(tr(keys.consent, { provider })).toContain("Ollama");
  });

  it("uses the backend label when present and the preset label otherwise", () => {
    expect(providerDisplayName("anthropic", "Anthropic (Claude)")).toBe("Anthropic (Claude)");
    expect(providerDisplayName("anthropic")).toBe("Anthropic (Claude)");
    expect(providerDisplayName(undefined)).toBe("DeepSeek");
  });
});
