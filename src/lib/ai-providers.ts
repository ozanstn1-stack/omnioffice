/**
 * Provider presets for the optional AI integration. Kept apart from the
 * settings component so the screens (consent text) and the tests share one
 * source of truth. The authoritative list lives in `crates/aicore`.
 */

export interface ProviderCapabilities {
  chat: boolean;
  embeddings: boolean;
  vision: boolean;
  structured_output: boolean;
  streaming: boolean;
}

export interface ProviderPreset {
  id: string;
  label: string;
  baseUrl: string;
  model: string;
  note: string;
  /** Placeholder shown in the API key field. */
  keyPlaceholder: string;
  capabilities: ProviderCapabilities;
}

export const PROVIDER_PRESETS: ProviderPreset[] = [
  {
    id: "deepseek",
    label: "DeepSeek",
    baseUrl: "https://api.deepseek.com",
    model: "deepseek-flash",
    note: "Cloud API. The document text is sent to api.deepseek.com together with your API key.",
    keyPlaceholder: "sk-…",
    capabilities: { chat: true, embeddings: false, vision: true, structured_output: true, streaming: true },
  },
  {
    id: "anthropic",
    label: "Anthropic (Claude)",
    baseUrl: "https://api.anthropic.com",
    model: "claude-opus-5-5",
    note: "Cloud API. The document text is sent to Anthropic's API (api.anthropic.com) together with your API key from console.anthropic.com.",
    keyPlaceholder: "sk-ant-…",
    capabilities: { chat: true, embeddings: false, vision: true, structured_output: false, streaming: true },
  },
  {
    id: "openai_compatible",
    label: "OpenAI-compatible",
    baseUrl: "",
    model: "",
    note: "Cloud API. The document text is sent to the OpenAI-compatible endpoint you configure.",
    keyPlaceholder: "sk-…",
    capabilities: { chat: true, embeddings: true, vision: true, structured_output: true, streaming: true },
  },
  {
    id: "ollama",
    label: "Ollama (local)",
    baseUrl: "http://localhost:11434",
    model: "llama3.2",
    note: "Local models. Ollama runs on your computer and the document text never leaves the computer.",
    keyPlaceholder: "",
    capabilities: { chat: true, embeddings: true, vision: true, structured_output: true, streaming: true },
  },
  {
    id: "gemini",
    label: "Google Gemini",
    baseUrl: "https://generativelanguage.googleapis.com/v1beta",
    model: "gemini-2.0-flash",
    note: "Cloud API. The document text is sent to Google Gemini together with your API key.",
    keyPlaceholder: "AIza…",
    capabilities: { chat: true, embeddings: true, vision: true, structured_output: true, streaming: true },
  },
  {
    id: "custom",
    label: "Custom endpoint",
    baseUrl: "",
    model: "",
    note: "Custom endpoint. The document text is sent wherever that endpoint points; check its privacy policy.",
    keyPlaceholder: "sk-…",
    capabilities: { chat: true, embeddings: false, vision: false, structured_output: false, streaming: true },
  },
];

/** Normalizes a stored provider value to a preset id (unknown values fall back to DeepSeek). */
export function parseProviderId(value?: string): string {
  const normalized = (value ?? "").trim().toLowerCase().replace(/[-\s]/g, "_");
  if (normalized === "deepseek" || normalized === "deep_seek") return "deepseek";
  if (["openai_compatible", "openai", "open_ai", "compatible"].includes(normalized)) return "openai_compatible";
  if (normalized === "ollama" || normalized === "local") return "ollama";
  if (["gemini", "google", "google_gemini"].includes(normalized)) return "gemini";
  if (["anthropic", "claude", "anthropic_claude"].includes(normalized)) return "anthropic";
  if (normalized === "custom" || normalized === "other") return "custom";
  return "deepseek";
}

/** True for providers that run on this computer / local network (nothing is sent to the internet). */
export function isLocalProvider(value?: string): boolean {
  return parseProviderId(value) === "ollama";
}

/** i18n keys for the data notice shown before a document's text is processed. */
export function consentKeys(value?: string): { consent: string; privacyBody: string } {
  return isLocalProvider(value)
    ? { consent: "ai.consentLocal", privacyBody: "ai.privacyBodyLocal" }
    : { consent: "ai.consent", privacyBody: "ai.privacyBody" };
}

/** The label without a trailing "(local)", for sentences that already say it runs locally. */
export function plainProviderName(label: string): string {
  return label.replace(/\s*\(local\)\s*$/i, "");
}

/** Display name used in the notices: the preset label, or the label the backend reported. */
export function providerDisplayName(value?: string, backendLabel?: string): string {
  if (backendLabel) return backendLabel;
  const id = parseProviderId(value);
  return PROVIDER_PRESETS.find((preset) => preset.id === id)?.label ?? "DeepSeek";
}
