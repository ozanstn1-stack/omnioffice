/**
 * Turkish AI task packs (C7): pick a pack, pick a document, run it through the
 * configured provider and check the structured answer. The default path is the
 * local Ollama server (nothing is bundled with the installer); when the saved
 * provider is not local the screen says so and offers a one-click switch.
 */
import { useEffect, useMemo, useState } from "react";
import { FileSearch, Play, ShieldAlert, ShieldCheck, Wifi, WifiOff } from "lucide-react";
import { Badge, Button, Card, Checkbox, Field } from "../components/ui";
import { Screen } from "../components/layout";
import { AiPackGrid } from "../components/ai-pack-grid";
import { pickDocumentToken, type FileToken } from "../lib/file-tokens";
import { uid } from "../lib/format";
import { useT } from "../lib/i18n";
import { aiChat, aiGetSettings, aiSaveSettings, toAppError } from "../lib/api";
import { PROVIDER_PRESETS, isLocalProvider, parseProviderId } from "../lib/ai-providers";
import {
  TASK_PACKS,
  getTaskPack,
  type TaskPackId,
  type TaskPackResult,
  type VerificationIssue,
} from "../lib/ai-task-packs";
import { runTaskPack } from "../lib/ai-task-pack-runner";
import { useScreenReady } from "../lib/screen-ready";
import { reportError, useToasts } from "../lib/store";
import type { AiSettingsView } from "../lib/types";

function issueText(
  t: (key: string, params?: Record<string, string | number>) => string,
  issue: VerificationIssue,
): string {
  const text = t(`aiPacks.issue.${issue.code}`, issue.params ?? {});
  return issue.field ? `${issue.field}: ${text}` : text;
}

export function AiPacks() {
  const t = useT();
  const push = useToasts((state) => state.push);
  const [settings, setSettings] = useState<AiSettingsView | null>(null);
  const [packId, setPackId] = useState<TaskPackId>(TASK_PACKS[0].id);
  const [file, setFile] = useState<FileToken | null>(null);
  const [consent, setConsent] = useState(false);
  const [running, setRunning] = useState(false);
  const [switching, setSwitching] = useState(false);
  const [raw, setRaw] = useState("");
  const [result, setResult] = useState<TaskPackResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showRaw, setShowRaw] = useState(false);

  const loadSettings = () =>
    aiGetSettings()
      .then((view) => setSettings(view as AiSettingsView))
      .catch(() => setSettings(null));

  useEffect(() => {
    // The loader owns the settings state; a failed read leaves the cloud notice on.
    void loadSettings();
  }, []);

  useScreenReady("aiPacks", true);

  const pack = useMemo(() => getTaskPack(packId), [packId]);
  const providerId = settings ? parseProviderId(settings.provider) : "deepseek";
  const local = settings ? isLocalProvider(settings.provider, settings.baseUrl) : false;
  const providerLabel =
    settings?.providerLabel ?? PROVIDER_PRESETS.find((preset) => preset.id === providerId)?.label ?? providerId;

  const switchToLocal = async () => {
    const ollama = PROVIDER_PRESETS.find((preset) => preset.id === "ollama");
    if (!ollama) return;
    setSwitching(true);
    try {
      await aiSaveSettings({
        baseUrl: ollama.baseUrl,
        model: ollama.model,
        temperature: 0.2,
        maxTokens: 2048,
        provider: "ollama",
      } as Parameters<typeof aiSaveSettings>[0]);
      await loadSettings();
      push({ kind: "success", title: t("aiPacks.switched") });
    } catch (caught) {
      reportError(caught, t);
    } finally {
      setSwitching(false);
    }
  };

  const pickPack = (id: TaskPackId) => {
    setPackId(id);
    setResult(null);
    setRaw("");
  };

  const pickFile = async () => {
    const picked = await pickDocumentToken(["pdf"]).catch(() => null);
    if (!picked) return;
    setFile(picked);
    setResult(null);
    setRaw("");
    setError(null);
  };

  const run = async () => {
    if (!file) return;
    setRunning(true);
    setError(null);
    setResult(null);
    setRaw("");
    try {
      // D5: a strict JSON contract with one repair turn. The runner never
      // touches the network; the provider call below is the only one.
      const run = await runTaskPack(pack, async (question) => {
        const answer = await aiChat({
          path: file.token,
          question,
          scope: "document",
          jobId: uid("ai-pack"),
          consent: true,
        });
        return answer.answer;
      });
      setRaw(run.answers[run.answers.length - 1] ?? "");
      setResult(run.result);
      if (run.attempts > 1) {
        push({ kind: "info", title: t("aiPacks.repaired") });
      }
      push({
        kind: run.result.ok ? "success" : "info",
        title: run.result.ok ? t("aiPacks.verified") : t("aiPacks.verificationFailed"),
      });
    } catch (caught) {
      const appError = toAppError(caught);
      if (appError.code !== "cancelled") {
        setError(appError.message);
        reportError(caught, t);
      }
    } finally {
      setRunning(false);
    }
  };

  return (
    <Screen
      title={t("aiPacks.title")}
      subtitle={t("aiPacks.subtitle")}
      actions={<Badge tone={local ? "ok" : "warn"}>{local ? t("aiPacks.local") : t("aiPacks.cloud")}</Badge>}
    >
      <Card>
        <div className="row" style={{ justifyContent: "space-between", flexWrap: "wrap", gap: 8 }}>
          <div className="row">
            {local ? <WifiOff size={15} /> : <Wifi size={15} />}
            <strong>{providerLabel}</strong>
            <span className="muted small">{settings?.baseUrl ?? ""}</span>
          </div>
          {!local ? (
            <Button size="sm" variant="ghost" disabled={switching} onClick={() => void switchToLocal()}>
              {t("aiPacks.switchLocal")}
            </Button>
          ) : null}
        </div>
        <p className="muted small mt-2">{t("aiPacks.providerNote")}</p>
        {!local ? <p className="small mt-1">{t("aiPacks.cloudWarning")}</p> : null}
        <p className="muted small mt-1">{t("aiPacks.modelNote")}</p>
      </Card>

      <AiPackGrid packs={TASK_PACKS} activeId={packId} onPick={pickPack} />
      <Card>
        <p className="muted small">{t(`aiPacks.pack.${pack.id}.hint`)}</p>
        <div className="stack mt-2">
          <Field label={t("aiPacks.document")}>
            <div className="row">
              <span
                className="small"
                style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}
              >
                {file ? file.name : t("aiPacks.noDocument")}
              </span>
              <Button size="sm" variant="ghost" icon={<FileSearch size={13} />} onClick={() => void pickFile()}>
                {t("aiPacks.pick")}
              </Button>
            </div>
          </Field>
          <Checkbox
            checked={consent}
            onChange={(value) => setConsent(value)}
            label={local ? t("aiPacks.consentLocal") : t("aiPacks.consentCloud")}
          />
          <div className="row">
            <Button
              size="sm"
              variant="primary"
              icon={<Play size={13} />}
              disabled={running || !file || !consent}
              onClick={() => void run()}
            >
              {running ? t("aiPacks.running") : t("aiPacks.run")}
            </Button>
          </div>
          {error ? (
            <p className="small" style={{ color: "var(--danger, #ef4444)" }}>
              {error}
            </p>
          ) : null}
        </div>
      </Card>

      {result ? (
        <Card>
          <div className="row" style={{ justifyContent: "space-between", flexWrap: "wrap", gap: 8 }}>
            <div className="row">
              {result.ok ? <ShieldCheck size={16} /> : <ShieldAlert size={16} />}
              <strong>{result.ok ? t("aiPacks.verified") : t("aiPacks.verificationFailed")}</strong>
              <Badge tone={result.parsed ? "ok" : "danger"}>{t("aiPacks.jsonLabel")}</Badge>
              <Badge tone={result.ok ? "ok" : "warn"}>
                {result.ok ? t("aiPacks.checksPassed") : t("aiPacks.problemCount", { count: result.issues.length })}
              </Badge>
            </div>
            <Button size="sm" variant="ghost" onClick={() => setShowRaw(!showRaw)}>
              {showRaw ? t("aiPacks.hideRaw") : t("aiPacks.showRaw")}
            </Button>
          </div>
          {result.issues.length ? (
            <ul className="small mt-2" style={{ paddingLeft: 18 }}>
              {result.issues.map((issue, index) => (
                <li key={index}>{issueText(t, issue)}</li>
              ))}
            </ul>
          ) : null}
          <pre
            className="small mt-2"
            style={{ whiteSpace: "pre-wrap", wordBreak: "break-word", background: "var(--surface-2)", padding: 10 }}
          >
            {showRaw ? raw : result.parsed ? JSON.stringify(result.parsed, null, 2) : raw}
          </pre>
        </Card>
      ) : null}
    </Screen>
  );
}
