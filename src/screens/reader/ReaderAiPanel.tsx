/** "Ask this document" AI side panel for the PDF Reader. */
import { useCallback, useEffect, useRef, useState } from "react";
import { FileText, Send, Settings as SettingsIcon, Sparkles, Square, Trash2 } from "lucide-react";
import { aiChat, aiGetSettings, aiSummarize, cancelJob, onAiChunk } from "../../lib/api";
import { consentKeys, isLocalProvider, plainProviderName, providerDisplayName } from "../../lib/ai-providers";
import { consentScope, isConsentRemembered } from "../../lib/ai-consent";
import { AiConsentNotice } from "../../components/ai-consent-notice";
import { Button, Spinner, TextArea } from "../../components/ui";
import { useT } from "../../lib/i18n";
import { useSettings } from "../../lib/store";
import { uid } from "../../lib/format";
import type { AiSettingsView, DocChatMessage } from "../../lib/types";
import type { Navigate } from "../../lib/nav";
import { ReaderAiAnswerActions } from "./ReaderAiActions";

type PanelSettings = AiSettingsView & { provider?: string; providerLabel?: string };

export interface ReaderChatEntry {
  role: "user" | "assistant";
  content: string;
  /** A summary bubble (not sent back as chat history). */
  summary?: boolean;
}

/** One piece of an answer: plain text, or a page citation the reader can jump to. */
export type AnswerPart = { kind: "text"; text: string } | { kind: "page"; page: number; quote?: string };

const CITATION = /\[(?:page|sayfa|p\.?|s\.?)\s*(\d+)(?:\s*:\s*([^\]]*))?\]/gi;

/** Splits an answer into text and `[page N: "quote"]` citations. */
export function splitCitations(text: string): AnswerPart[] {
  const parts: AnswerPart[] = [];
  let last = 0;
  for (const match of text.matchAll(CITATION)) {
    const index = match.index ?? 0;
    if (index > last) parts.push({ kind: "text", text: text.slice(last, index) });
    const quote = match[2]?.trim().replace(/^["“”']|["“”']$/g, "");
    parts.push({ kind: "page", page: Number(match[1]), quote: quote || undefined });
    last = index + match[0].length;
  }
  if (last < text.length) parts.push({ kind: "text", text: text.slice(last) });
  return parts;
}

/** The model is told the language by name; the app has two UI languages. */
export function summaryLanguage(uiLanguage: string): string {
  return uiLanguage === "tr" ? "Turkish" : "English";
}

/**
 * The reader's one-click summary: in the app's language (not the document's,
 * so an English report is summarized in Turkish for a Turkish user), long
 * enough to span every page, and with page citations the panel turns into
 * jump links.
 */
export function readerSummaryOptions(uiLanguage: string, pageCount: number) {
  const pages = pageCount > 1 ? `pages 1-${pageCount}` : "the page";
  return {
    language: summaryLanguage(uiLanguage),
    length: "detailed" as const,
    style: "bullets" as const,
    focus: `every part of the document (${pages}), not only the first page; end each bullet with the page it comes from as [page N]`,
  };
}

/** Chat history for the backend: real question/answer turns only. */
export function chatHistory(entries: readonly ReaderChatEntry[]): DocChatMessage[] {
  return entries
    .filter((entry) => !entry.summary && entry.content.trim())
    .map(({ role, content }) => ({ role, content }));
}

export function ReaderAiPanel({
  path,
  password,
  pageCount,
  onJumpToPage,
  onNavigate,
}: {
  path: string;
  password?: string;
  pageCount: number;
  onJumpToPage: (page: number) => void;
  onNavigate?: Navigate;
}) {
  const t = useT();
  const appSettings = useSettings((s) => s.settings);
  const settingsLoaded = useSettings((s) => s.loaded);
  const updateSettings = useSettings((s) => s.update);
  const [aiSettings, setAiSettings] = useState<PanelSettings | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [consentChecked, setConsent] = useState(false);
  const [messages, setMessages] = useState<ReaderChatEntry[]>([]);
  const [input, setInput] = useState("");
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const jobId = useRef(uid("reader-ai"));
  const activeRef = useRef(false);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let cancelled = false;
    void aiGetSettings()
      .then((value) => {
        if (!cancelled) setAiSettings(value as PanelSettings);
      })
      .catch(() => undefined)
      .finally(() => {
        if (!cancelled) setLoaded(true);
      });
    return () => {
      cancelled = true;
    };
  }, [settingsLoaded]);

  // Streamed answer chunks go into the last (assistant) bubble.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void onAiChunk((payload) => {
      if (payload.jobId !== jobId.current || payload.kind === "reasoning" || !activeRef.current) return;
      setMessages((previous) => {
        const next = [...previous];
        const last = next[next.length - 1];
        if (last && last.role === "assistant")
          next[next.length - 1] = { ...last, content: last.content + payload.delta };
        return next;
      });
    })
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  // A running request is cancelled when the panel goes away.
  useEffect(
    () => () => {
      if (activeRef.current) void cancelJob(jobId.current).catch(() => undefined);
    },
    [],
  );

  useEffect(() => {
    const element = listRef.current;
    if (element) element.scrollTop = element.scrollHeight;
  }, [messages]);

  const configured = aiSettings?.configured ?? false;
  const remembered = isConsentRemembered(appSettings.aiConsentRemembered, aiSettings?.provider, aiSettings?.baseUrl);
  const consent = consentChecked || remembered;
  const providerLabel = providerDisplayName(aiSettings?.provider, aiSettings?.providerLabel);
  const notice = consentKeys(aiSettings?.provider, aiSettings?.baseUrl);
  const noticeParams = {
    provider: isLocalProvider(aiSettings?.provider, aiSettings?.baseUrl)
      ? plainProviderName(providerLabel)
      : providerLabel,
  };
  const canRun = configured && consent && !running;

  const run = useCallback(async (entry: ReaderChatEntry, request: () => Promise<string>) => {
    jobId.current = uid("reader-ai");
    activeRef.current = true;
    setError(null);
    setRunning(true);
    setMessages((previous) => [...previous, entry, { role: "assistant", content: "", summary: entry.summary }]);
    try {
      const answer = await request();
      setMessages((previous) => {
        const next = [...previous];
        const last = next[next.length - 1];
        if (last && last.role === "assistant") next[next.length - 1] = { ...last, content: answer || last.content };
        return next;
      });
    } catch (runError) {
      setMessages((previous) => {
        const last = previous[previous.length - 1];
        return last && last.role === "assistant" && !last.content ? previous.slice(0, -2) : previous;
      });
      const message = String((runError as { message?: string })?.message ?? runError);
      if (!/cancel/i.test(message)) setError(message);
    } finally {
      activeRef.current = false;
      setRunning(false);
    }
  }, []);

  const summarize = () => {
    if (!canRun) return;
    void run({ role: "user", content: t("reader.aiSummarizeRequest"), summary: true }, async () => {
      const result = await aiSummarize({
        path,
        options: readerSummaryOptions(appSettings.language, pageCount),
        password: password || undefined,
        jobId: jobId.current,
        consent,
      });
      return result.text;
    });
  };

  const ask = (override?: string) => {
    const question = (override ?? input).trim();
    if (!question || !canRun) return;
    const history = chatHistory(messages);
    setInput("");
    void run({ role: "user", content: question }, async () => {
      const result = await aiChat({
        path,
        question,
        history,
        scope: "document",
        sendImage: false,
        password: password || undefined,
        jobId: jobId.current,
        consent,
        vaultConsent: false,
        embeddingConsent: false,
        imageConsent: false,
      });
      return result.answer;
    });
  };

  const stop = () => void cancelJob(jobId.current).catch(() => undefined);

  const renderAnswer = (text: string) =>
    splitCitations(text).map((part, index) =>
      part.kind === "text" ? (
        <span key={index}>{part.text}</span>
      ) : (
        <button
          key={index}
          type="button"
          className="badge badge-accent mx-0.5 text-[11px] cursor-pointer hover:underline"
          title={part.quote ?? t("reader.aiJumpToPage", { page: part.page })}
          disabled={part.page < 1 || part.page > Math.max(1, pageCount)}
          onClick={() => onJumpToPage(part.page)}
        >
          {t("reader.aiPageShort", { page: part.page })}
        </button>
      ),
    );

  if (!loaded) {
    return (
      <div className="flex items-center gap-2 text-xs muted">
        <Spinner size={12} /> {t("common.loading")}
      </div>
    );
  }

  if (!configured) {
    return (
      <div className="card-soft p-3 flex flex-col gap-2">
        <p className="text-[13px] font-semibold">{t("ai.notConfigured")}</p>
        <p className="text-xs muted">{t("ai.notConfiguredBody")}</p>
        {onNavigate ? (
          <Button size="sm" icon={<SettingsIcon size={13} />} onClick={() => onNavigate("settings")}>
            {t("reader.aiOpenSettings")}
          </Button>
        ) : null}
      </div>
    );
  }

  const suggestions = [t("reader.aiSuggestKeyPoints"), t("reader.aiSuggestNumbers"), t("reader.aiSuggestConclusion")];

  return (
    <div className="flex flex-col gap-2.5 min-h-0 flex-1">
      <div className="card-soft p-2.5 text-[12px] flex flex-col gap-1.5">
        <AiConsentNotice
          consent={consent}
          remembered={remembered}
          notice={notice}
          params={noticeParams}
          onConsent={setConsent}
          onRemember={() =>
            void updateSettings({ aiConsentRemembered: consentScope(aiSettings?.provider, aiSettings?.baseUrl) })
          }
          onForget={() => {
            setConsent(false);
            void updateSettings({ aiConsentRemembered: "" });
          }}
        />
      </div>

      <div className="flex items-center gap-1.5">
        <Button size="sm" variant="primary" icon={<FileText size={13} />} disabled={!canRun} onClick={summarize}>
          {t("reader.aiSummarize")}
        </Button>
        {messages.length ? (
          <Button
            size="sm"
            variant="ghost"
            icon={<Trash2 size={13} />}
            disabled={running}
            onClick={() => {
              setMessages([]);
              setError(null);
            }}
          >
            {t("reader.aiClear")}
          </Button>
        ) : null}
      </div>

      <div
        ref={listRef}
        className="flex-1 min-h-[160px] overflow-y-auto flex flex-col gap-2"
        aria-live="polite"
        data-testid="reader-ai-messages"
      >
        {messages.length === 0 ? (
          <div className="text-xs muted flex flex-col gap-2">
            <p className="flex items-start gap-1.5">
              <Sparkles size={13} className="shrink-0 mt-0.5" style={{ color: "var(--accent)" }} />
              {t("reader.aiIntro")}
            </p>
            {suggestions.map((suggestion) => (
              <button
                key={suggestion}
                type="button"
                className="text-left card-soft px-2.5 py-1.5 text-[12px] hover:border-[var(--accent)] disabled:opacity-50"
                disabled={!canRun}
                onClick={() => ask(suggestion)}
              >
                {suggestion}
              </button>
            ))}
          </div>
        ) : (
          messages.map((message, index) => (
            <div
              key={index}
              className={`rounded-lg px-2.5 py-2 text-[12.5px] whitespace-pre-wrap break-words ${
                message.role === "user" ? "self-end max-w-[90%]" : "self-start w-full"
              }`}
              style={
                message.role === "user"
                  ? { background: "var(--accent)", color: "white" }
                  : { background: "var(--surface-2)", border: "1px solid var(--border)" }
              }
            >
              {message.role === "assistant" ? (
                message.content ? (
                  <div className="flex flex-col gap-1.5">
                    <div>{renderAnswer(message.content)}</div>
                    {!running ? (
                      <ReaderAiAnswerActions path={path} content={message.content} onNavigate={onNavigate} />
                    ) : null}
                  </div>
                ) : (
                  <span className="inline-flex items-center gap-1.5 muted">
                    <Spinner size={12} /> {t("reader.aiThinking")}
                  </span>
                )
              ) : (
                message.content
              )}
            </div>
          ))
        )}
      </div>

      {error ? (
        <p className="text-xs" style={{ color: "var(--danger)" }}>
          {error}
        </p>
      ) : null}

      <div className="flex items-end gap-1.5">
        <TextArea
          className="flex-1 text-[12.5px]"
          rows={2}
          value={input}
          placeholder={t("ai.chatPlaceholder")}
          aria-label={t("ai.chatPlaceholder")}
          disabled={!consent}
          onChange={(event) => setInput(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              ask();
            }
          }}
        />
        {running ? (
          <Button size="sm" variant="ghost" icon={<Square size={13} />} onClick={stop} aria-label={t("ai.stop")} />
        ) : (
          <Button
            size="sm"
            variant="primary"
            icon={<Send size={13} />}
            disabled={!canRun || !input.trim()}
            onClick={() => ask()}
            aria-label={t("reader.aiSend")}
          />
        )}
      </div>
      <p className="text-[11px] muted">{t("reader.aiOnlyThisDocument")}</p>
    </div>
  );
}
