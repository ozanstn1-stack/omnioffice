import { useEffect, useState } from "react";
import { Bot, CheckCircle2, KeyRound, ShieldAlert, ShieldCheck, Trash2, Zap } from "lucide-react";
import { Badge, Button, Card, Checkbox, Field, Segmented, Slider, Spinner, TextInput } from "./ui";
import { useSettings } from "../lib/store";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { isAndroid } from "../lib/mobile";
import { clamp } from "../lib/format";
import { useT } from "../lib/i18n";
import {
  aiClearKey,
  aiGetSettings,
  aiLibraryDefaultDir,
  aiModels,
  aiSaveSettings,
  aiTestConnection,
  toAppError,
} from "../lib/api";
import type { AiModelOption, AiSettingsView, AiTestResult, ReasoningEffort } from "../lib/types";

/**
 * Settings panel for the optional AI integration.
 *
 * This is the only screen that receives an API key. The key is written by the
 * Rust layer (Windows DPAPI encrypted) and is never logged; the UI only ever
 * shows a masked value afterwards.
 */

interface ProviderCapabilities {
  chat: boolean;
  embeddings: boolean;
  vision: boolean;
  structured_output: boolean;
  streaming: boolean;
}

interface ProviderPreset {
  id: string;
  label: string;
  baseUrl: string;
  model: string;
  note: string;
  capabilities: ProviderCapabilities;
}

const PROVIDER_PRESETS: ProviderPreset[] = [
  {
    id: "deepseek",
    label: "DeepSeek",
    baseUrl: "https://api.deepseek.com",
    model: "deepseek-flash",
    note: "Cloud API. The document text is sent to api.deepseek.com together with your API key.",
    capabilities: { chat: true, embeddings: false, vision: true, structured_output: true, streaming: true },
  },
  {
    id: "openai_compatible",
    label: "OpenAI-compatible",
    baseUrl: "",
    model: "",
    note: "Cloud API. The document text is sent to the OpenAI-compatible endpoint you configure.",
    capabilities: { chat: true, embeddings: true, vision: true, structured_output: true, streaming: true },
  },
  {
    id: "ollama",
    label: "Ollama (local)",
    baseUrl: "http://localhost:11434",
    model: "llama3.2",
    note: "Local models. Ollama runs on your computer and the document text never leaves the computer.",
    capabilities: { chat: true, embeddings: true, vision: true, structured_output: true, streaming: true },
  },
  {
    id: "gemini",
    label: "Google Gemini",
    baseUrl: "https://generativelanguage.googleapis.com/v1beta",
    model: "gemini-2.0-flash",
    note: "Cloud API. The document text is sent to Google Gemini together with your API key.",
    capabilities: { chat: true, embeddings: true, vision: true, structured_output: true, streaming: true },
  },
  {
    id: "custom",
    label: "Custom endpoint",
    baseUrl: "",
    model: "",
    note: "Custom endpoint. The document text is sent wherever that endpoint points; check its privacy policy.",
    capabilities: { chat: true, embeddings: false, vision: false, structured_output: false, streaming: true },
  },
];

type AiSettingsViewExt = AiSettingsView & {
  provider?: string;
  providerLabel?: string;
  providerNote?: string;
  capabilities?: ProviderCapabilities;
  embeddingModel?: string | null;
};

const CAPABILITY_CHIPS: { key: keyof ProviderCapabilities; labelKey: string; fallback: string }[] = [
  { key: "chat", labelKey: "ai.capChat", fallback: "Chat" },
  { key: "embeddings", labelKey: "ai.capEmbeddings", fallback: "Embeddings" },
  { key: "vision", labelKey: "ai.capVision", fallback: "Vision" },
  { key: "structured_output", labelKey: "ai.capStructured", fallback: "Structured output" },
  { key: "streaming", labelKey: "ai.capStreaming", fallback: "Streaming" },
];

function parseProviderId(value?: string): string {
  const normalized = (value ?? "").trim().toLowerCase().replace(/[-\s]/g, "_");
  if (normalized === "deepseek" || normalized === "deep_seek") return "deepseek";
  if (["openai_compatible", "openai", "open_ai", "compatible"].includes(normalized)) return "openai_compatible";
  if (normalized === "ollama" || normalized === "local") return "ollama";
  if (["gemini", "google", "google_gemini"].includes(normalized)) return "gemini";
  if (normalized === "custom" || normalized === "other") return "custom";
  return "deepseek";
}

export function AiSettings() {
  const t = useT();
  const settings = useSettings((s) => s.settings);
  const updateSettings = useSettings((s) => s.update);
  const [libraryDefault, setLibraryDefault] = useState("");
  const [view, setView] = useState<AiSettingsViewExt | null>(null);
  const [apiKey, setApiKey] = useState("");
  const [baseUrl, setBaseUrl] = useState("https://api.deepseek.com");
  const [model, setModel] = useState("deepseek-flash");
  const [provider, setProvider] = useState("deepseek");
  const [embeddingModel, setEmbeddingModel] = useState("");
  const [models, setModels] = useState<AiModelOption[]>([]);
  const [temperature, setTemperature] = useState(0.2);
  const [maxTokens, setMaxTokens] = useState(4096);
  const [thinking, setThinking] = useState(true);
  const [reasoningEffort, setReasoningEffort] = useState<ReasoningEffort>("high");
  const [contextTokens, setContextTokens] = useState(200_000);
  const [maxOutputTokens, setMaxOutputTokens] = useState(384_000);
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [testResult, setTestResult] = useState<AiTestResult | null>(null);

  useEffect(() => {
    void aiLibraryDefaultDir()
      .then(setLibraryDefault)
      .catch(() => undefined);
    void aiModels()
      .then(setModels)
      .catch(() => undefined);
    void aiGetSettings()
      .then((settings: AiSettingsViewExt) => {
        setView(settings);
        setBaseUrl(settings.baseUrl);
        setModel(settings.model);
        setProvider(parseProviderId(settings.provider));
        setEmbeddingModel(settings.embeddingModel ?? "");
        setTemperature(settings.temperature);
        setMaxTokens(settings.maxTokens);
        setThinking(settings.thinking);
        setReasoningEffort((settings.reasoningEffort as ReasoningEffort) ?? "high");
        setContextTokens(settings.contextTokens ?? 200_000);
        setMaxOutputTokens(settings.maxOutputTokens ?? 384_000);
      })
      .catch(() => undefined);
  }, []);

  const label = (key: string, fallback: string) => {
    const value = t(key);
    return value === key ? fallback : value;
  };

  const preset = PROVIDER_PRESETS.find((entry) => entry.id === provider) ?? PROVIDER_PRESETS[0];
  const savedProvider = parseProviderId(view?.provider);
  const capabilities: ProviderCapabilities =
    view?.capabilities && savedProvider === provider ? view.capabilities : preset.capabilities;
  const providerNote = view?.providerNote && savedProvider === provider ? view.providerNote : preset.note;
  const needsApiKey = provider !== "ollama";

  const changeProvider = (value: string) => {
    setProvider(value);
    setTestResult(null);
    const next = PROVIDER_PRESETS.find((entry) => entry.id === value);
    if (next) {
      if (next.baseUrl) setBaseUrl(next.baseUrl);
      if (next.model) setModel(next.model);
    }
  };

  const normalizedModel = model.trim().toLowerCase();
  const isV4Model =
    normalizedModel.startsWith("deepseek-v4") ||
    normalizedModel === "deepseek-flash" ||
    normalizedModel.startsWith("deepseek-flash-") ||
    normalizedModel === "deepseek-reasoner";

  const save = async (): Promise<boolean> => {
    setSaving(true);
    setSaveError(null);
    try {
      const updated = await aiSaveSettings({
        apiKey: apiKey.trim() ? apiKey.trim() : undefined,
        baseUrl,
        model,
        temperature,
        maxTokens,
        thinking,
        reasoningEffort,
        contextTokens,
        provider,
        embeddingModel: capabilities.embeddings && embeddingModel.trim() ? embeddingModel.trim() : undefined,
      } as Parameters<typeof aiSaveSettings>[0] & { provider: string; embeddingModel?: string });
      setView(updated);
      setApiKey("");
      setTestResult(null);
      return true;
    } catch (error) {
      // The backend refuses e.g. a public http:// base URL so the API key can
      // never be sent unencrypted; show the reason instead of failing silently.
      setSaveError(toAppError(error).message);
      return false;
    } finally {
      setSaving(false);
    }
  };

  const test = async () => {
    setTesting(true);
    setTestResult(null);
    try {
      if (!(await save())) return;
      const result = await aiTestConnection();
      setTestResult(result);
    } finally {
      setTesting(false);
    }
  };

  const clear = async () => {
    const updated = await aiClearKey();
    setView(updated);
    setTestResult(null);
  };

  return (
    <Card className="p-5 flex flex-col gap-4">
      <h3 className="font-semibold flex items-center gap-2">
        <Bot size={16} style={{ color: "var(--accent)" }} /> {t("settings.aiTitle")}
        <Badge tone={view?.configured ? "ok" : "warn"}>
          {view?.configured ? t("common.ready") : t("ai.notConfigured")}
        </Badge>
      </h3>

      <div className="card-soft p-3 flex items-start gap-2 text-xs">
        <ShieldAlert size={14} style={{ color: "var(--warn)", marginTop: 2 }} />
        <span>{t("settings.aiWarning")}</span>
      </div>

      <div className="grid gap-3" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(200px, 1fr))" }}>
        <Field
          label={label("ai.provider", "Provider")}
          hint={label("ai.providerHint", "Where the extracted text is sent")}
        >
          <select
            className="select"
            value={provider}
            aria-label={label("ai.provider", "Provider")}
            onChange={(event) => changeProvider(event.target.value)}
          >
            {PROVIDER_PRESETS.map((entry) => (
              <option key={entry.id} value={entry.id}>
                {entry.label}
              </option>
            ))}
          </select>
        </Field>
        <Field
          label={label("ai.embeddingModel", "Embedding model")}
          hint={
            capabilities.embeddings
              ? label("ai.embeddingHint", "Optional; used by providers with embedding support")
              : label("ai.embeddingUnsupported", "This provider does not support embeddings")
          }
        >
          <TextInput
            value={embeddingModel}
            disabled={!capabilities.embeddings}
            onChange={(event) => setEmbeddingModel(event.target.value)}
            placeholder="nomic-embed-text"
            spellCheck={false}
          />
        </Field>
      </div>

      <div className="card-soft p-3 flex items-start gap-2 text-xs">
        {provider === "ollama" ? (
          <ShieldCheck size={14} style={{ color: "var(--ok)", marginTop: 2 }} />
        ) : (
          <ShieldAlert size={14} style={{ color: "var(--warn)", marginTop: 2 }} />
        )}
        <span>{providerNote}</span>
      </div>

      <Field label={label("ai.capabilities", "Provider capabilities")}>
        <div className="flex flex-wrap gap-1.5">
          {CAPABILITY_CHIPS.map((chip) => (
            <Badge key={chip.key} tone={capabilities[chip.key] ? "ok" : "default"}>
              {label(chip.labelKey, chip.fallback)}: {capabilities[chip.key] ? t("info.yes") : t("info.no")}
            </Badge>
          ))}
        </div>
      </Field>

      {provider === "ollama" ? (
        <div className="card-soft p-3 flex items-start gap-2 text-xs">
          <CheckCircle2 size={14} style={{ color: "var(--ok)", marginTop: 2 }} />
          <span>{label("ai.ollamaNoKey", "Ollama runs on this computer, so no API key is needed.")}</span>
        </div>
      ) : (
        <Field label={t("settings.aiKey")} hint={t("settings.aiKeyHint")}>
          <div className="flex gap-2">
            <TextInput
              type="password"
              value={apiKey}
              placeholder={view?.maskedKey || "sk-…"}
              onChange={(event) => setApiKey(event.target.value)}
              autoComplete="off"
              spellCheck={false}
            />
            <Button
              variant="ghost"
              icon={<KeyRound size={15} />}
              onClick={() => void clear()}
              disabled={!view?.configured}
            >
              {t("settings.aiClear")}
            </Button>
          </div>
        </Field>
      )}

      <div className="grid gap-3" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(200px, 1fr))" }}>
        <Field label={t("settings.aiBaseUrl")} hint={t("settings.aiBaseUrlHint")}>
          <TextInput value={baseUrl} onChange={(event) => setBaseUrl(event.target.value)} spellCheck={false} />
        </Field>
        <Field label={t("settings.aiModel")} hint={t("settings.aiModelHint")}>
          <div className="flex flex-col gap-2">
            <select
              className="select"
              value={models.some((option) => option.id === model) ? model : "__custom"}
              onChange={(event) => {
                if (event.target.value !== "__custom") setModel(event.target.value);
              }}
            >
              {models.map((option) => (
                <option key={option.id} value={option.id}>
                  {option.id} — {option.label}
                </option>
              ))}
              <option value="__custom">{t("settings.aiModelCustom")}</option>
            </select>
            {!models.some((option) => option.id === model) ? (
              <TextInput
                value={model}
                onChange={(event) => setModel(event.target.value)}
                spellCheck={false}
                placeholder="deepseek-v4-flash"
              />
            ) : null}
          </div>
        </Field>
      </div>

      <div className="grid gap-3" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(200px, 1fr))" }}>
        <Field label={t("settings.aiTemperature")}>
          <Slider
            value={temperature}
            min={0}
            max={1.5}
            step={0.05}
            onChange={setTemperature}
            format={(value) => value.toFixed(2)}
          />
        </Field>
        <Field label={t("settings.aiMaxTokens")} hint={t("settings.aiMaxTokensHint")}>
          <div className="flex flex-col gap-2">
            <div className="flex items-center gap-2">
              <TextInput
                type="number"
                value={maxTokens}
                min={256}
                max={maxOutputTokens}
                step={256}
                onChange={(event) => setMaxTokens(clamp(Number(event.target.value) || 256, 256, maxOutputTokens))}
              />
              <span className="text-xs muted shrink-0">/ {maxOutputTokens.toLocaleString()}</span>
            </div>
            <div className="flex flex-wrap gap-1.5">
              {[4_096, 16_384, 32_768, 65_536, 131_072, 384_000].map((preset) => (
                <Button
                  key={preset}
                  size="sm"
                  variant={maxTokens === preset ? "primary" : "default"}
                  onClick={() => setMaxTokens(preset)}
                >
                  {preset >= 1000 ? `${Math.round(preset / 1024)}K` : preset}
                </Button>
              ))}
            </div>
          </div>
        </Field>
      </div>

      <Field label={t("settings.aiContextTokens")} hint={t("settings.aiContextTokensHint")}>
        <div className="flex items-center gap-2">
          <TextInput
            type="number"
            value={contextTokens}
            min={8_000}
            max={1_000_000}
            step={10_000}
            onChange={(event) => setContextTokens(clamp(Number(event.target.value) || 200_000, 8_000, 1_000_000))}
          />
          <span className="text-xs muted shrink-0">/ 1,000,000</span>
        </div>
        <div className="flex flex-wrap gap-1.5 mt-2">
          {[65_536, 131_072, 200_000, 500_000, 1_000_000].map((preset) => (
            <Button
              key={preset}
              size="sm"
              variant={contextTokens === preset ? "primary" : "default"}
              onClick={() => setContextTokens(preset)}
            >
              {preset === 1_000_000 ? "1M" : `${Math.round(preset / 1024)}K`}
            </Button>
          ))}
        </div>
      </Field>

      <div className="flex flex-col gap-2">
        <Checkbox checked={thinking} onChange={setThinking} label={t("settings.aiThinking")} />
        <p className="text-xs muted -mt-1">{t("settings.aiThinkingHint")}</p>
        {thinking ? (
          <Field label={t("settings.aiReasoningEffort")} hint={t("settings.aiReasoningEffortHint")}>
            <Segmented<ReasoningEffort>
              value={reasoningEffort}
              onChange={setReasoningEffort}
              options={[
                { value: "low", label: t("settings.aiEffortLow") },
                { value: "high", label: t("settings.aiEffortHigh") },
                { value: "max", label: t("settings.aiEffortMax") },
              ]}
            />
          </Field>
        ) : null}
        {!isV4Model ? (
          <p className="text-xs" style={{ color: "var(--warn)" }}>
            {t("settings.aiThinkingModelWarning")}
          </p>
        ) : null}
      </div>

      {saveError ? (
        <p className="text-xs" style={{ color: "var(--danger, #b91c1c)" }}>
          {saveError}
        </p>
      ) : null}

      <div className="flex items-center gap-2">
        <Button
          variant="primary"
          icon={saving ? <Spinner size={14} /> : <Bot size={15} />}
          onClick={() => void save()}
          disabled={saving}
        >
          {t("common.save")}
        </Button>
        <Button
          variant="ghost"
          icon={testing ? <Spinner size={14} /> : <Zap size={15} />}
          onClick={() => void test()}
          disabled={testing || (needsApiKey && !view?.configured && !apiKey.trim())}
        >
          {t("settings.aiTestConnection")}
        </Button>
        {view?.configured ? (
          <Button variant="danger" icon={<Trash2 size={15} />} onClick={() => void clear()}>
            {t("settings.aiClearKey")}
          </Button>
        ) : null}
      </div>

      {testResult ? (
        <div className="flex items-center gap-2 text-[13px]">
          {testResult.ok ? <CheckCircle2 size={15} style={{ color: "var(--ok)" }} /> : <Badge tone="danger">!</Badge>}
          <span>{testResult.ok ? t("settings.aiTestOk", { model: testResult.model }) : testResult.message}</span>
        </div>
      ) : null}

      <div className="card-soft p-3 flex flex-col gap-2">
        <Checkbox
          checked={settings.aiAutoSave}
          onChange={(value) => void updateSettings({ aiAutoSave: value })}
          label={t("settings.aiAutoSave")}
        />
        <p className="text-xs muted -mt-1">{t("settings.aiAutoSaveHint")}</p>
        <label className="label">{t("settings.aiLibraryDir")}</label>
        <div className="flex items-center gap-2">
          <TextInput
            value={settings.aiLibraryDir}
            placeholder={libraryDefault}
            onChange={(event) => void updateSettings({ aiLibraryDir: event.target.value })}
          />
          {/* Android has no folder path picker (SAF returns content:// trees). */}
          {!isAndroid() ? (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => {
                void openDialog({ directory: true, multiple: false, title: t("ai.libraryFolder") }).then((picked) => {
                  if (picked) void updateSettings({ aiLibraryDir: String(picked) });
                });
              }}
            >
              {t("common.chooseFolder")}
            </Button>
          ) : null}
        </div>
        <Checkbox
          checked={settings.keepOperationLog}
          onChange={(value) => void updateSettings({ keepOperationLog: value })}
          label={t("settings.keepOperationLog")}
        />
      </div>

      <p className="text-xs muted">
        {t("settings.aiStorage")}:{" "}
        {view?.keyStorage === "dpapi"
          ? t("ai.keySecure")
          : view?.keyStorage === "plain"
            ? t("ai.keyPlain")
            : t("ai.keyMissing")}
      </p>
    </Card>
  );
}
