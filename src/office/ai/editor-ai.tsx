/**
 * Shared plumbing for the in-editor AI actions (Writer, Calc, Impress).
 *
 * - `useAiStatus` reads the AI settings so the ribbons can disable their AI
 *   buttons (with a hint) while no provider is configured.
 * - `AiAssistDialog` runs one `ai_edit_text` request: a consent step that
 *   names what will be sent, an optional options form, the running state
 *   (with Stop), and the result preview with Accept / Try again / Cancel.
 *
 * Consent is remembered in memory per open document and provider, never
 * persisted: a new document tab, or a different provider/endpoint, asks again.
 */
import { type ReactNode, useCallback, useEffect, useRef, useState } from "react";
import { Bot, Check, Loader2, ShieldAlert, Square } from "lucide-react";
import { aiEditText, aiGetSettings, cancelJob, onAiChunk, toAppError } from "../../lib/api";
import { uid } from "../../lib/format";
import { useT } from "../../lib/i18n";
import { useSettings } from "../../lib/store";
import type { AiEditRequest, AiEditResult, AiSettingsView } from "../../lib/types";

/** Mirrors `MAX_EDIT_CHARS` in src-tauri/src/ai/edit.rs. */
export const AI_EDIT_MAX_CHARS = 60_000;

export interface AiStatus {
  loaded: boolean;
  configured: boolean;
  provider: string;
  providerLabel: string;
  host: string;
  model: string;
}

const NO_AI: AiStatus = { loaded: false, configured: false, provider: "", providerLabel: "", host: "", model: "" };

function hostOf(baseUrl: string): string {
  try {
    return new URL(baseUrl).host;
  } catch {
    return baseUrl;
  }
}

/** Loads the AI settings once and again whenever the window regains focus. */
export function useAiStatus(): AiStatus {
  const [status, setStatus] = useState<AiStatus>(NO_AI);
  useEffect(() => {
    let alive = true;
    const load = () => {
      void aiGetSettings()
        .then((view) => {
          if (!alive) return;
          const settings: AiSettingsView = view;
          setStatus({
            loaded: true,
            configured: Boolean(settings.configured),
            provider: settings.provider ?? "deepseek",
            providerLabel: settings.providerLabel ?? (settings.provider === "ollama" ? "Ollama (local)" : "DeepSeek"),
            host: hostOf(settings.baseUrl ?? ""),
            model: settings.model ?? "",
          });
        })
        .catch(() => {
          if (alive) setStatus({ ...NO_AI, loaded: true });
        });
    };
    load();
    window.addEventListener("focus", load);
    return () => {
      alive = false;
      window.removeEventListener("focus", load);
    };
  }, []);
  return status;
}

// ---------------------------------------------------------------------------
// Consent (in memory, per document and provider)
// ---------------------------------------------------------------------------

const consented = new Set<string>();

function consentKey(docId: string, status: AiStatus): string {
  return `${docId}|${status.provider}|${status.host}`;
}

export function hasAiConsent(docId: string, status: AiStatus): boolean {
  return consented.has(consentKey(docId, status));
}

/** Forgets every consent (used when a document closes and by tests). */
export function resetAiConsent(docId?: string): void {
  if (!docId) {
    consented.clear();
    return;
  }
  for (const key of [...consented]) if (key.startsWith(`${docId}|`)) consented.delete(key);
}

// ---------------------------------------------------------------------------
// Language / tone choices shared by the editors
// ---------------------------------------------------------------------------

/** UI language code -> {native label, English name used in the prompt}. */
export const AI_LANGUAGES: Array<{ code: string; label: string; name: string }> = [
  { code: "en", label: "English", name: "English" },
  { code: "tr", label: "Türkçe", name: "Turkish" },
  { code: "de", label: "Deutsch", name: "German" },
  { code: "fr", label: "Français", name: "French" },
  { code: "es", label: "Español", name: "Spanish" },
  { code: "it", label: "Italiano", name: "Italian" },
  { code: "pt", label: "Português", name: "Portuguese" },
  { code: "nl", label: "Nederlands", name: "Dutch" },
  { code: "ru", label: "Русский", name: "Russian" },
  { code: "ar", label: "العربية", name: "Arabic" },
  { code: "zh", label: "中文", name: "Chinese" },
  { code: "ja", label: "日本語", name: "Japanese" },
];

export const AI_TONES = ["formal", "friendly", "confident", "simple"] as const;
export type AiTone = (typeof AI_TONES)[number];

/** The language the prompt should use for a UI language code. */
export function aiLanguageName(code: string): string {
  return AI_LANGUAGES.find((language) => language.code === code)?.name ?? code;
}

// ---------------------------------------------------------------------------
// The dialog
// ---------------------------------------------------------------------------

type Phase = "consent" | "form" | "running" | "result" | "error";

export interface AiAssistDialogProps {
  title: string;
  /** Open document id: consent is remembered per document and provider. */
  docId: string;
  status: AiStatus;
  /** Names what will be sent, shown in the consent step. */
  sends: string;
  /** Options shown before the request is made (language, tone, description). */
  form?: ReactNode;
  /** Whether the form is complete enough to run. */
  canRun?: boolean;
  /** Run as soon as consent is given instead of waiting for the form. */
  autoRun?: boolean;
  /** Builds the request when running; return an error message to refuse it. */
  buildRequest: () => Omit<AiEditRequest, "jobId"> | { error: string };
  /** Show streamed text while the model is working (plain-text tasks only). */
  streamPreview?: boolean;
  /** Original text, shown next to the suggestion. */
  original?: string;
  renderResult?: (result: AiEditResult) => ReactNode;
  acceptLabel: string;
  onAccept: (result: AiEditResult) => void;
  extraActions?: (result: AiEditResult) => ReactNode;
  onClose: () => void;
}

export function AiAssistDialog(props: AiAssistDialogProps) {
  const { title, docId, status, sends, form, autoRun, streamPreview, original, onClose } = props;
  const t = useT();
  const language = useSettings((state) => state.settings.language);
  const [phase, setPhase] = useState<Phase>(() => (hasAiConsent(docId, status) ? "form" : "consent"));
  const [streamed, setStreamed] = useState("");
  const [result, setResult] = useState<AiEditResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const jobRef = useRef<string | null>(null);
  const seqRef = useRef(0);
  const propsRef = useRef(props);
  useEffect(() => {
    propsRef.current = props;
  });

  const run = useCallback(() => {
    const built = propsRef.current.buildRequest();
    if ("error" in built) {
      setError(built.error);
      setPhase("error");
      return;
    }
    const jobId = uid("aiedit");
    const seq = ++seqRef.current;
    jobRef.current = jobId;
    setStreamed("");
    setResult(null);
    setError(null);
    setPhase("running");
    aiEditText({ ...built, jobId })
      .then((reply) => {
        if (seq !== seqRef.current) return;
        jobRef.current = null;
        setResult(reply);
        setPhase("result");
      })
      .catch((failure: unknown) => {
        if (seq !== seqRef.current) return;
        jobRef.current = null;
        setError(toAppError(failure).message);
        setPhase("error");
      });
  }, []);

  // Streamed text for the preview while the model is working.
  useEffect(() => {
    if (!streamPreview) return;
    let unlisten: (() => void) | undefined;
    let alive = true;
    void onAiChunk((payload) => {
      if (payload.jobId !== jobRef.current || payload.kind !== "content") return;
      setStreamed((previous) => previous + payload.delta);
    }).then((fn) => {
      if (alive) unlisten = fn;
      else fn();
    });
    return () => {
      alive = false;
      unlisten?.();
    };
  }, [streamPreview]);

  // Actions that only need consent (Rewrite, Shorten, ...) start by themselves.
  // The timer keeps StrictMode's mount/cleanup/mount probe from starting the
  // request twice or cancelling it right after it started.
  useEffect(() => {
    if (phase !== "form" || !autoRun) return;
    const handle = window.setTimeout(run, 0);
    return () => window.clearTimeout(handle);
  }, [phase, autoRun, run]);

  const cancelRunning = useCallback(() => {
    seqRef.current += 1;
    const jobId = jobRef.current;
    jobRef.current = null;
    if (jobId) void cancelJob(jobId).catch(() => undefined);
  }, []);

  // Closing the dialog (or leaving the editor) stops a request in flight.
  useEffect(() => cancelRunning, [cancelRunning]);

  const close = useCallback(() => {
    cancelRunning();
    onClose();
  }, [cancelRunning, onClose]);

  const stop = () => {
    cancelRunning();
    if (form) setPhase("form");
    else onClose();
  };

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        close();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [close]);

  const giveConsent = () => {
    consented.add(consentKey(docId, status));
    setPhase("form");
  };

  const retry = () => {
    if (autoRun) run();
    else setPhase("form");
  };

  const defaultResult = (reply: AiEditResult) => (
    <div className="ai-dlg-compare">
      {original !== undefined ? (
        <div className="ai-dlg-pane">
          <div className="ai-dlg-label">{t("ai.edit.original")}</div>
          <pre className="ai-dlg-text" data-testid="ai-original">
            {original}
          </pre>
        </div>
      ) : null}
      <div className="ai-dlg-pane">
        <div className="ai-dlg-label">{t("ai.edit.suggestion")}</div>
        <pre className="ai-dlg-text" data-testid="ai-suggestion">
          {reply.text}
        </pre>
      </div>
    </div>
  );

  return (
    <div className="overlay ai-dlg-overlay" role="presentation">
      <div className="modal ai-dlg" role="dialog" aria-modal="true" aria-label={title} lang={language}>
        <div className="ai-dlg-head">
          <Bot size={16} aria-hidden />
          <h3>{title}</h3>
          <button type="button" className="icon-btn" onClick={close} aria-label={t("common.close")}>
            ×
          </button>
        </div>

        <div className="ai-dlg-body">
          {phase === "consent" ? (
            <div className="ai-dlg-consent" role="group" aria-label={t("ai.edit.consentTitle")}>
              <p className="ai-dlg-notice">
                <ShieldAlert size={16} aria-hidden />
                <strong>{t("ai.edit.consentTitle")}</strong>
              </p>
              <p>
                {t("ai.edit.consentBody", {
                  what: sends,
                  provider: status.providerLabel,
                  host: status.host || "-",
                })}
              </p>
            </div>
          ) : null}

          {phase === "form" ? <div className="ai-dlg-form">{form}</div> : null}

          {phase === "running" ? (
            <div className="ai-dlg-running" role="status">
              <p>
                <Loader2 size={14} className="spin" aria-hidden /> {t("ai.edit.working")}
              </p>
              {streamPreview && streamed ? <pre className="ai-dlg-text">{streamed}</pre> : null}
            </div>
          ) : null}

          {phase === "result" && result ? (props.renderResult ?? defaultResult)(result) : null}

          {phase === "error" ? (
            <p className="ai-dlg-error" role="alert">
              {error}
            </p>
          ) : null}
        </div>

        <div className="ai-dlg-foot">
          {phase === "consent" ? (
            <>
              <button type="button" className="btn btn-ghost" onClick={close}>
                {t("ai.edit.cancel")}
              </button>
              <button type="button" className="btn btn-primary" onClick={giveConsent}>
                {t("ai.edit.consentAction")}
              </button>
            </>
          ) : null}
          {phase === "form" ? (
            <>
              <button type="button" className="btn btn-ghost" onClick={close}>
                {t("ai.edit.cancel")}
              </button>
              <button
                type="button"
                className="btn btn-primary"
                onClick={run}
                disabled={props.canRun === false}
                title={props.canRun === false ? t("ai.edit.formIncomplete") : undefined}
              >
                {t("ai.edit.run")}
              </button>
            </>
          ) : null}
          {phase === "running" ? (
            <button type="button" className="btn" onClick={stop}>
              <Square size={13} aria-hidden /> {t("ai.edit.stop")}
            </button>
          ) : null}
          {phase === "result" && result ? (
            <>
              <button type="button" className="btn btn-ghost" onClick={close}>
                {t("ai.edit.cancel")}
              </button>
              {props.extraActions?.(result)}
              <button type="button" className="btn" onClick={retry}>
                {t("ai.edit.tryAgain")}
              </button>
              <button type="button" className="btn btn-primary" onClick={() => props.onAccept(result)}>
                <Check size={13} aria-hidden /> {props.acceptLabel}
              </button>
            </>
          ) : null}
          {phase === "error" ? (
            <>
              <button type="button" className="btn btn-ghost" onClick={close}>
                {t("ai.edit.close")}
              </button>
              <button type="button" className="btn btn-primary" onClick={retry}>
                {t("ai.edit.tryAgain")}
              </button>
            </>
          ) : null}
        </div>
      </div>
    </div>
  );
}

/** Copies text to the clipboard; resolves false when the browser refuses. */
export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}
