import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  BookOpenCheck,
  Bot,
  Check,
  Brain,
  Copy,
  Languages,
  ListChecks,
  MessageCircleQuestion,
  MessageSquare,
  Save,
  Send,
  Sparkles,
  Square,
  Tags,
  Wand2,
} from "lucide-react";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { isAndroid, pickAndroidSaveTarget, publishOutputs, saveTextOnAndroid, type PublishTarget } from "../lib/mobile";
import {
  aiAsk,
  aiCleanupText,
  aiDocumentPreview,
  aiExamplePrompts,
  aiLibrarySave,
  aiSaveOutput,
  aiSuggestMetadata,
  aiSummarize,
  aiTranslate,
  cancelJob,
  editMetadata,
  onAiChunk,
  onAiProgress,
} from "../lib/api";
import { Badge, Button, Card, Checkbox, Field, Segmented, Spinner, TextArea, TextInput } from "../components/ui";
import { DropZone, InfoStrip, ResultCard } from "../components/files";
import { OptionCard, Screen, TwoColumn } from "../components/layout";
import { useT } from "../lib/i18n";
import { logFrontend } from "../lib/api";
import { useTool } from "../lib/useTool";
import { useDev, useSettings, useToasts } from "../lib/store";
import { clamp, fileBaseName, formatBytes, uid } from "../lib/format";
import type {
  AiExamplePrompts,
  AiLibraryEntry,
  AiMetadataSuggestion,
  AiPreview,
  AiSettingsView,
  AiTextResult,
  OpResult,
  SummaryLength,
  SummaryStyle,
} from "../lib/types";

type AiTab = "summary" | "translate" | "ask" | "chat" | "cleanup" | "metadata";

type ChatScope = "document" | "page" | "selection";

interface ProviderCapabilities {
  chat: boolean;
  embeddings: boolean;
  vision: boolean;
  structured_output: boolean;
  streaming: boolean;
}

type AiSettingsViewExt = AiSettingsView & {
  provider?: string;
  providerLabel?: string;
  providerNote?: string;
  capabilities?: ProviderCapabilities;
  embeddingModel?: string | null;
};

interface ChatEntry {
  role: "user" | "assistant";
  content: string;
}

const CITATION_PATTERN = /(\[page\s+\d+\])/gi;

export function Ai({ initialFiles, dragging }: { initialFiles?: string[]; dragging: boolean }) {
  const t = useT();
  const label = useCallback(
    (key: string, fallback: string) => {
      const value = t(key);
      return value === key ? fallback : value;
    },
    [t],
  );
  const session = useTool({ suffix: "_ai", accept: "pdf", initialPaths: initialFiles });
  const settingsLoaded = useSettings((s) => s.loaded);
  const settings = useSettings((s) => s.settings);
  const pushToast = useToasts((s) => s.push);
  const [aiSettings, setAiSettings] = useState<AiSettingsViewExt | null>(null);
  const devTab = useDev((s) => s.tab) as AiTab | null;
  const [tab, setTab] = useState<AiTab>("summary");
  const [examples, setExamples] = useState<AiExamplePrompts | null>(null);
  const [preview, setPreview] = useState<AiPreview | null>(null);
  const [consent, setConsent] = useState(false);
  const [output, setOutput] = useState("");
  const [reasoning, setReasoning] = useState("");
  const [running, setRunning] = useState(false);
  const [stage, setStage] = useState<{ stage: string; current: number; total: number } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [metadataSuggestion, setMetadataSuggestion] = useState<AiMetadataSuggestion | null>(null);
  const [metadataResult, setMetadataResult] = useState<OpResult | null>(null);
  const [lastResult, setLastResult] = useState<{ kind: string; result: AiTextResult; options: string } | null>(null);
  const [savedEntry, setSavedEntry] = useState<AiLibraryEntry | null>(null);
  const [showReasoning, setShowReasoning] = useState(false);
  const [summaryOptions, setSummaryOptions] = useState({
    language: "auto",
    length: "medium" as SummaryLength,
    style: "paragraph" as SummaryStyle,
    focus: "",
  });
  const [targetLanguage, setTargetLanguage] = useState("tr");
  const [bilingual, setBilingual] = useState(false);
  const [question, setQuestion] = useState("");
  const [chatMessages, setChatMessages] = useState<ChatEntry[]>([]);
  const [chatInput, setChatInput] = useState("");
  const [chatScope, setChatScope] = useState<ChatScope>("document");
  const [chatPage, setChatPage] = useState(1);
  const [chatSelection, setChatSelection] = useState("");
  const jobId = useRef(uid("ai"));
  const outputRef = useRef<HTMLDivElement>(null);
  const chatRef = useRef<HTMLDivElement>(null);
  const chatActiveRef = useRef(false);

  // Development hook: start on a specific tab for screenshots.
  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect -- dev/screenshot-only initial tab
    if (devTab && ["summary", "translate", "ask", "chat", "cleanup", "metadata"].includes(devTab)) setTab(devTab);
  }, [devTab]);

  // Load AI settings and example prompts once.
  useEffect(() => {
    void import("../lib/api").then(async ({ aiGetSettings }) => {
      setAiSettings(await aiGetSettings().catch(() => null));
    });
    void aiExamplePrompts()
      .then(setExamples)
      .catch(() => undefined);
  }, [settingsLoaded]);

  // Streamed answer chunks + progress.
  useEffect(() => {
    let unlistenChunk: (() => void) | undefined;
    let unlistenProgress: (() => void) | undefined;
    void onAiChunk((payload) => {
      if (payload.jobId !== jobId.current) return;
      if (payload.kind === "reasoning") {
        setReasoning((previous) => previous + payload.delta);
      } else if (chatActiveRef.current) {
        setChatMessages((previous) => {
          const next = [...previous];
          const last = next[next.length - 1];
          if (last && last.role === "assistant") {
            next[next.length - 1] = { ...last, content: last.content + payload.delta };
          }
          return next;
        });
      } else {
        setOutput((previous) => previous + payload.delta);
      }
    }).then((fn) => {
      unlistenChunk = fn;
    });
    void onAiProgress((payload) => {
      if (payload.jobId !== jobId.current) return;
      setStage({ stage: payload.stage, current: payload.current, total: payload.total });
    }).then((fn) => {
      unlistenProgress = fn;
    });
    return () => {
      unlistenChunk?.();
      unlistenProgress?.();
    };
  }, []);

  // Auto-scroll the output while streaming.
  useEffect(() => {
    const element = outputRef.current;
    if (element) element.scrollTop = element.scrollHeight;
  }, [output]);

  useEffect(() => {
    const element = chatRef.current;
    if (element) element.scrollTop = element.scrollHeight;
  }, [chatMessages]);

  // `fail` is declared later; the ref indirection lets `refreshPreview` call
  // the current implementation without referencing a not-yet-declared binding.
  const failRef = useRef<(message: string) => void>(() => undefined);

  const refreshPreview = useCallback(async () => {
    if (!session.primary) {
      setPreview(null);
      return;
    }
    try {
      const result = await aiDocumentPreview(session.primary.path, undefined, session.password || undefined);
      setPreview(result);
    } catch (previewError) {
      setPreview(null);
      failRef.current(String((previewError as { message?: string })?.message ?? previewError));
    }
  }, [session.password, session.primary]);

  useEffect(() => {
    // Resetting the per-document run state when the document changes is the
    // point of this effect; `refreshPreview` depends on the active file.
    /* eslint-disable react-hooks/set-state-in-effect */
    setOutput("");
    setError(null);
    setMetadataSuggestion(null);
    setMetadataResult(null);
    setConsent(false);
    setChatMessages([]);
    setChatPage(1);
    /* eslint-enable react-hooks/set-state-in-effect */
    void refreshPreview();
  }, [refreshPreview]);

  // Auto-save: every finished AI result is written to the library as Markdown.
  useEffect(() => {
    if (!lastResult || !settings.aiAutoSave || !session.primary) return;
    const { kind, result, options } = lastResult;
    void aiLibrarySave({
      kind,
      sourcePath: session.primary.path,
      sourceName: session.primary.name,
      model: result.model,
      pages: result.pages,
      characters: result.characters,
      options,
      text: result.text,
      elapsedMs: result.elapsedMs,
      directory: settings.aiLibraryDir || undefined,
    })
      .then((entry) => {
        setSavedEntry(entry);
        pushToast({ kind: "success", title: t("ai.savedToLibrary"), detail: entry.filePath });
      })
      .catch(() => undefined);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [lastResult]);

  const saveToLibrary = () => {
    if (!lastResult) return;
    setLastResult({ ...lastResult });
  };

  const configured = aiSettings?.configured ?? false;
  const canRun = configured && consent && Boolean(session.primary) && !running;

  const providerLabel =
    aiSettings?.providerLabel ?? (aiSettings?.provider === "ollama" ? "Ollama (local)" : "DeepSeek");
  const networkActivityKey = "ai.networkActivity";
  const networkActivityText = t(networkActivityKey, { chars: preview?.characters ?? 0, provider: providerLabel });
  const networkActivity =
    networkActivityText === networkActivityKey
      ? `Sending ${preview?.characters ?? 0} characters to ${providerLabel}`
      : networkActivityText;

  const fail = (message: string) => {
    setError(message);
    // A provider (or a compromised endpoint) can echo the request back in its
    // error body; bound what is persisted to the local log.
    void logFrontend("ai-error", message.slice(0, 500));
    console.warn("[ai]", message);
  };
  useEffect(() => {
    failRef.current = fail;
  });
  const resetJob = () => {
    jobId.current = uid("ai");
    setStage(null);
    setReasoning("");
  };

  // Development automation (PDFSAK_DEV_RUN=1) runs the active tab once ready.
  // `registerAutoRun` stores the handler in a ref (no behavior change); the
  // assignment happens inside useTool's callback, not here.
  // eslint-disable-next-line react-hooks/refs -- registering the dev auto-run handler
  session.registerAutoRun(() => {
    setConsent(true);
    if (tab === "ask") setQuestion("What is the total amount?");
    if (tab === "chat") setChatInput("What is the total amount?");
    void (tab === "translate"
      ? runTranslate()
      : tab === "ask"
        ? runAsk("What is the total amount?")
        : tab === "chat"
          ? runChat("What is the total amount?")
          : tab === "cleanup"
            ? runCleanup()
            : tab === "metadata"
              ? runMetadata()
              : runSummary());
  });
  const runSummary = async () => {
    if (!session.primary) return;
    resetJob();
    setRunning(true);
    setOutput("");
    setError(null);
    try {
      const result = await aiSummarize({
        path: session.primary.path,
        options: {
          language: summaryOptions.language.trim() || "auto",
          length: summaryOptions.length,
          style: summaryOptions.style,
          focus: summaryOptions.focus,
        },
        password: session.password || undefined,
        jobId: jobId.current,
      });
      setOutput(result.text);
      setLastResult({
        kind: "summary",
        result,
        options: `${summaryOptions.length}/${summaryOptions.style}${summaryOptions.language !== "auto" ? ` � ${summaryOptions.language}` : ""}`,
      });
    } catch (runError) {
      fail(String((runError as { message?: string })?.message ?? runError));
    } finally {
      setRunning(false);
      setStage(null);
    }
  };

  const runTranslate = async () => {
    if (!session.primary) return;
    resetJob();
    setRunning(true);
    setOutput("");
    setError(null);
    try {
      const result = await aiTranslate({
        path: session.primary.path,
        options: { targetLanguage, bilingual },
        password: session.password || undefined,
        jobId: jobId.current,
      });
      setOutput(result.text);
      setLastResult({ kind: "translate", result, options: `${targetLanguage}${bilingual ? " / bilingual" : ""}` });
    } catch (runError) {
      fail(String((runError as { message?: string })?.message ?? runError));
    } finally {
      setRunning(false);
      setStage(null);
    }
  };

  const runAsk = async (questionOverride?: string) => {
    const asked = (questionOverride ?? question).trim();
    if (!session.primary || !asked) return;
    resetJob();
    setRunning(true);
    setOutput("");
    setError(null);
    try {
      const result = await aiAsk({
        path: session.primary.path,
        question: asked,
        password: session.password || undefined,
        jobId: jobId.current,
      });
      setOutput(result.text);
      setLastResult({ kind: "ask", result, options: asked });
    } catch (runError) {
      fail(String((runError as { message?: string })?.message ?? runError));
    } finally {
      setRunning(false);
      setStage(null);
    }
  };

  const scopedChatQuestion = (asked: string) => {
    if (chatScope === "page") {
      return `Scope: answer from page ${chatPage} only.\n\nQuestion: ${asked}`;
    }
    if (chatScope === "selection") {
      return `Scope: answer from the selected text below only.\n\nSelected text:\n${chatSelection.trim()}\n\nQuestion: ${asked}`;
    }
    return asked;
  };

  const runChat = async (questionOverride?: string) => {
    const asked = (questionOverride ?? chatInput).trim();
    if (!session.primary || !asked || running) return;
    if (chatScope === "selection" && !chatSelection.trim()) return;
    setChatMessages((previous) => [...previous, { role: "user", content: asked }, { role: "assistant", content: "" }]);
    setChatInput("");
    resetJob();
    chatActiveRef.current = true;
    setRunning(true);
    setError(null);
    try {
      const result = await aiAsk({
        path: session.primary.path,
        question: scopedChatQuestion(asked),
        password: session.password || undefined,
        jobId: jobId.current,
      });
      setChatMessages((previous) => {
        const next = [...previous];
        const last = next[next.length - 1];
        if (last && last.role === "assistant") next[next.length - 1] = { ...last, content: result.text };
        return next;
      });
      setLastResult({ kind: "ask", result, options: asked });
    } catch (runError) {
      setChatMessages((previous) => {
        const last = previous[previous.length - 1];
        if (last && last.role === "assistant" && !last.content) return previous.slice(0, -1);
        return previous;
      });
      fail(String((runError as { message?: string })?.message ?? runError));
    } finally {
      chatActiveRef.current = false;
      setRunning(false);
      setStage(null);
    }
  };

  const renderChatContent = (content: string) =>
    content.split(CITATION_PATTERN).map((part, index) => {
      const match = /^\[page\s+(\d+)\]$/i.exec(part);
      if (match) {
        return (
          <Badge key={index} tone="accent">
            {label("ai.citationPage", "page")} {match[1]}
          </Badge>
        );
      }
      return <span key={index}>{part}</span>;
    });

  const runCleanup = async () => {
    if (!session.primary) return;
    resetJob();
    setRunning(true);
    setOutput("");
    setError(null);
    try {
      const result = await aiCleanupText({
        path: session.primary.path,
        password: session.password || undefined,
        jobId: jobId.current,
      });
      setOutput(result.text);
      setLastResult({ kind: "cleanup", result, options: "ocr-text-repair" });
    } catch (runError) {
      fail(String((runError as { message?: string })?.message ?? runError));
    } finally {
      setRunning(false);
      setStage(null);
    }
  };

  const runMetadata = async () => {
    if (!session.primary) return;
    resetJob();
    setRunning(true);
    setMetadataSuggestion(null);
    setError(null);
    try {
      const suggestion = await aiSuggestMetadata(session.primary.path, session.password || undefined, jobId.current);
      setMetadataSuggestion(suggestion);
    } catch (runError) {
      fail(String((runError as { message?: string })?.message ?? runError));
    } finally {
      setRunning(false);
      setStage(null);
    }
  };

  const applyMetadata = async () => {
    if (!session.primary || !metadataSuggestion) return;
    let targetPath = session.outputPath;
    let androidTarget: PublishTarget["file"] = null;
    if (isAndroid()) {
      const chosen = await pickAndroidSaveTarget(fileBaseName(session.outputPath) || "document.pdf").catch(() => null);
      if (!chosen) return;
      androidTarget = chosen;
      targetPath = session.outputPath;
    } else {
      const picked = await saveDialog({
        title: t("common.save"),
        defaultPath: session.outputPath,
        filters: [{ name: "PDF", extensions: ["pdf"] }],
      });
      if (!picked) return;
      targetPath = String(picked);
    }
    setRunning(true);
    try {
      const result = await editMetadata(
        session.primary.path,
        { path: targetPath, overwrite: isAndroid() ? "replace" : "error" },
        {
          title: metadataSuggestion.title,
          author: metadataSuggestion.author,
          subject: metadataSuggestion.subject,
          keywords: metadataSuggestion.keywords.join(", "),
          creator: "PDF Swiss Army Knife (AI)",
          producer: "PDF Swiss Army Knife (AI)",
          creation_date: "",
          mod_date: "",
        },
        false,
        uid("ai-meta"),
        session.password || undefined,
      );
      if (androidTarget) {
        await publishOutputs([result.path], { file: androidTarget });
      }
      setMetadataResult(result);
    } catch (applyError) {
      fail(String((applyError as { message?: string })?.message ?? applyError));
    } finally {
      setRunning(false);
    }
  };

  const stop = () => {
    void cancelJob(jobId.current).catch(() => undefined);
    setRunning(false);
  };

  const saveOutput = async () => {
    if (!output.trim()) return;
    const extension = tab === "summary" || tab === "ask" || tab === "metadata" ? "md" : "md";
    const base = session.primary ? session.primary.path.replace(/\.pdf$/i, "") : "ai-output";
    const defaultName = `${base}${tab === "translate" ? "_translated" : tab === "cleanup" ? "_clean" : "_ai"}.${extension}`;
    if (isAndroid()) {
      try {
        const saved = await saveTextOnAndroid(output, fileBaseName(defaultName));
        if (saved) {
          setError(null);
          setSavedPath(defaultName);
        }
      } catch (saveError) {
        fail(String((saveError as { message?: string })?.message ?? saveError));
      }
      return;
    }
    const picked = await saveDialog({
      title: t("common.save"),
      defaultPath: defaultName,
      filters: [
        { name: "Markdown", extensions: ["md"] },
        { name: "Text", extensions: ["txt"] },
      ],
    });
    if (!picked) return;
    try {
      const saved = await aiSaveOutput(String(picked), output, "unique_name");
      setError(null);
      setSavedPath(saved);
    } catch (saveError) {
      fail(String((saveError as { message?: string })?.message ?? saveError));
    }
  };

  const [savedPath, setSavedPath] = useState<string | null>(null);

  const copyOutput = async () => {
    try {
      await navigator.clipboard.writeText(output);
    } catch {
      /* clipboard unavailable */
    }
  };

  const tabs: { id: AiTab; label: string; icon: React.ReactNode }[] = useMemo(
    () => [
      { id: "summary", label: t("ai.tabSummary"), icon: <Sparkles size={14} /> },
      { id: "translate", label: t("ai.tabTranslate"), icon: <Languages size={14} /> },
      { id: "ask", label: t("ai.tabAsk"), icon: <MessageCircleQuestion size={14} /> },
      { id: "chat", label: label("ai.tabChat", "Document chat"), icon: <MessageSquare size={14} /> },
      { id: "cleanup", label: t("ai.tabCleanup"), icon: <Wand2 size={14} /> },
      { id: "metadata", label: t("ai.tabMetadata"), icon: <Tags size={14} /> },
    ],
    [t, label],
  );

  const activeRun = () => {
    switch (tab) {
      case "summary":
        return runSummary;
      case "translate":
        return runTranslate;
      case "ask":
        return runAsk;
      case "chat":
        return runChat;
      case "cleanup":
        return runCleanup;
      default:
        return runMetadata;
    }
  };

  return (
    <Screen
      title={t("ai.title")}
      subtitle={t("ai.subtitle")}
      actions={
        aiSettings ? (
          <Badge tone={configured ? "ok" : "warn"}>
            <Bot size={11} /> {configured ? aiSettings.model : t("ai.notConfigured")}
          </Badge>
        ) : null
      }
    >
      <TwoColumn
        main={
          <>
            {!configured ? (
              <Card className="p-4 flex flex-col gap-3">
                <p className="font-semibold text-[14px]">{t("ai.notConfigured")}</p>
                <p className="text-[13px] muted">{t("ai.notConfiguredBody")}</p>
                <div className="text-xs muted">
                  {t("ai.privacyTitle")}: {t("ai.privacyBody")}
                </div>
              </Card>
            ) : null}

            {!session.primary ? (
              <DropZone onPaths={(paths) => void session.addPaths(paths)} dragging={dragging} accept="pdf" />
            ) : (
              <>
                <OptionCard>
                  <div className="flex items-center justify-between gap-3">
                    <InfoStrip info={session.info} error={session.infoError} />
                    <Button size="sm" variant="ghost" onClick={() => void session.pickFiles()}>
                      {t("common.selectFile")}
                    </Button>
                  </div>
                </OptionCard>

                <Card className="p-4 flex flex-col gap-3">
                  <div className="flex items-center gap-2 flex-wrap">
                    <BookOpenCheck size={16} style={{ color: "var(--accent)" }} />
                    <p className="font-semibold text-[13.5px]">{t("ai.privacyTitle")}</p>
                    <span className="ml-auto text-xs muted">
                      {preview
                        ? `${t("ai.preview", { pages: preview.pages, chars: preview.characters })} · ~${preview.estimatedWords} ${t("ai.words")}`
                        : t("common.loading")}
                    </span>
                  </div>
                  {consent ? (
                    <div className="flex items-center gap-2 text-xs">
                      <span className="flex items-center gap-1.5" style={{ color: "var(--ok)" }}>
                        <Check size={13} /> {t("ai.consentShort")}
                      </span>
                      <Button size="sm" variant="ghost" onClick={() => setConsent(false)}>
                        {t("ai.consentChange")}
                      </Button>
                    </div>
                  ) : (
                    <>
                      <p className="text-[13px] muted">{t("ai.privacyBody")}</p>
                      <Checkbox checked={consent} onChange={setConsent} label={t("ai.consent")} />
                    </>
                  )}
                </Card>

                <Card className="p-4">
                  <div className="flex flex-wrap items-center gap-2 mb-3">
                    {tabs.map((entry) => (
                      <Button
                        key={entry.id}
                        size="sm"
                        variant={tab === entry.id ? "primary" : "default"}
                        icon={entry.icon}
                        onClick={() => {
                          setTab(entry.id);
                          setOutput("");
                          setError(null);
                          setMetadataSuggestion(null);
                        }}
                      >
                        {entry.label}
                      </Button>
                    ))}
                  </div>

                  {tab === "summary" ? (
                    <div className="grid gap-3" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(180px, 1fr))" }}>
                      <Field label={t("ai.language")} hint={t("ai.languageHint")}>
                        <TextInput
                          value={summaryOptions.language}
                          onChange={(event) => setSummaryOptions({ ...summaryOptions, language: event.target.value })}
                        />
                      </Field>
                      <Field label={t("ai.length")}>
                        <Segmented<SummaryLength>
                          value={summaryOptions.length}
                          onChange={(value) => setSummaryOptions({ ...summaryOptions, length: value })}
                          options={[
                            { value: "short", label: t("ai.lengthShort") },
                            { value: "medium", label: t("ai.lengthMedium") },
                            { value: "detailed", label: t("ai.lengthDetailed") },
                          ]}
                        />
                      </Field>
                      <Field label={t("ai.style")}>
                        <Segmented<SummaryStyle>
                          value={summaryOptions.style}
                          onChange={(value) => setSummaryOptions({ ...summaryOptions, style: value })}
                          options={[
                            { value: "paragraph", label: t("ai.styleParagraph") },
                            { value: "bullets", label: t("ai.styleBullets") },
                            { value: "executive", label: t("ai.styleExecutive") },
                          ]}
                        />
                      </Field>
                      <Field label={t("ai.focus")} hint={t("common.optional")}>
                        <TextInput
                          value={summaryOptions.focus}
                          onChange={(event) => setSummaryOptions({ ...summaryOptions, focus: event.target.value })}
                          placeholder={t("ai.focusPlaceholder")}
                        />
                      </Field>
                    </div>
                  ) : null}

                  {tab === "translate" ? (
                    <div className="flex flex-col gap-3">
                      <div className="flex flex-wrap gap-2">
                        {(examples?.translateTargets ?? ["tr", "en", "de", "fr", "es"]).map((code) => (
                          <button
                            key={code}
                            className="badge"
                            style={{
                              cursor: "pointer",
                              padding: "6px 12px",
                              background: targetLanguage === code ? "var(--accent)" : "var(--surface-3)",
                              color: targetLanguage === code ? "var(--accent-text)" : "var(--muted)",
                            }}
                            onClick={() => setTargetLanguage(code)}
                          >
                            {code.toUpperCase()}
                          </button>
                        ))}
                        <TextInput
                          className="input-sm"
                          style={{ width: 140 }}
                          value={targetLanguage}
                          onChange={(event) => setTargetLanguage(event.target.value)}
                          placeholder="tr / Turkish"
                        />
                      </div>
                      <Checkbox checked={bilingual} onChange={setBilingual} label={t("ai.bilingual")} />
                      <p className="text-xs muted">{t("ai.translateHint")}</p>
                    </div>
                  ) : null}

                  {tab === "ask" ? (
                    <div className="flex flex-col gap-3">
                      <Field label={t("ai.question")}>
                        <TextInput
                          value={question}
                          onChange={(event) => setQuestion(event.target.value)}
                          placeholder={t("ai.questionPlaceholder")}
                          onKeyDown={(event) => {
                            if (event.key === "Enter" && canRun) void runAsk();
                          }}
                        />
                      </Field>
                      <div className="flex flex-wrap gap-2">
                        {(examples?.ask ?? []).map((example) => (
                          <Button key={example} size="sm" variant="ghost" onClick={() => setQuestion(example)}>
                            {example}
                          </Button>
                        ))}
                      </div>
                    </div>
                  ) : null}

                  {tab === "chat" ? (
                    <div className="flex flex-col gap-3">
                      <Field
                        label={label("ai.sendScope", "Send scope")}
                        hint={label("ai.sendScopeHint", "What the question is restricted to")}
                      >
                        <Segmented<ChatScope>
                          value={chatScope}
                          onChange={setChatScope}
                          options={[
                            { value: "document", label: label("ai.scopeDocument", "Whole document") },
                            { value: "page", label: label("ai.scopePage", "Current page") },
                            { value: "selection", label: label("ai.scopeSelection", "Selected text only") },
                          ]}
                        />
                      </Field>

                      {chatScope === "page" ? (
                        <Field label={label("ai.scopePageNumber", "Page")}>
                          <TextInput
                            type="number"
                            min={1}
                            max={preview?.pages ?? undefined}
                            value={chatPage}
                            onChange={(event) =>
                              setChatPage(clamp(Number(event.target.value) || 1, 1, preview?.pages || 1))
                            }
                          />
                        </Field>
                      ) : null}

                      {chatScope === "selection" ? (
                        <Field label={label("ai.scopeSelectionText", "Selected text")}>
                          <TextArea
                            value={chatSelection}
                            rows={4}
                            onChange={(event) => setChatSelection(event.target.value)}
                            placeholder={label(
                              "ai.scopeSelectionPlaceholder",
                              "Paste the text you selected in the document",
                            )}
                          />
                        </Field>
                      ) : null}

                      <p className="text-xs muted">
                        {label(
                          "ai.scopeNote",
                          "Document chat calls ai_ask, which has no page or selection filter: the scope only narrows the question context sent to the model, and the retriever may still attach other pages.",
                        )}
                      </p>

                      <div ref={chatRef} className="card-soft p-3 max-h-[380px] overflow-y-auto flex flex-col gap-2">
                        {chatMessages.length === 0 ? (
                          <p className="text-xs muted">
                            {label("ai.chatEmpty", "Ask a question about this document.")}
                          </p>
                        ) : (
                          chatMessages.map((message, index) => (
                            <div
                              key={index}
                              className="rounded-xl px-3 py-2 text-[13px]"
                              style={{
                                alignSelf: message.role === "user" ? "flex-end" : "flex-start",
                                maxWidth: "85%",
                                background: message.role === "user" ? "var(--accent)" : "var(--surface-2)",
                                color: message.role === "user" ? "var(--accent-text)" : "var(--text)",
                              }}
                            >
                              {message.role === "assistant" ? (
                                <div className="whitespace-pre-wrap leading-relaxed">
                                  {message.content ? (
                                    renderChatContent(message.content)
                                  ) : running ? (
                                    <Spinner size={13} />
                                  ) : null}
                                </div>
                              ) : (
                                <div className="whitespace-pre-wrap leading-relaxed">{message.content}</div>
                              )}
                            </div>
                          ))
                        )}
                      </div>

                      <div className="flex items-center gap-2">
                        <TextInput
                          value={chatInput}
                          onChange={(event) => setChatInput(event.target.value)}
                          placeholder={label("ai.chatPlaceholder", "Ask about this document…")}
                          onKeyDown={(event) => {
                            if (event.key === "Enter" && canRun) void runChat();
                          }}
                        />
                        <Button
                          variant="primary"
                          icon={running ? <Spinner size={14} /> : <Send size={14} />}
                          disabled={
                            !canRun || !chatInput.trim() || (chatScope === "selection" && !chatSelection.trim())
                          }
                          onClick={() => void runChat()}
                        >
                          {label("ai.chatSend", "Send")}
                        </Button>
                      </div>
                    </div>
                  ) : null}

                  {tab === "cleanup" ? (
                    <div className="flex flex-col gap-2">
                      <p className="text-[13px]">{t("ai.cleanupIntro")}</p>
                      <p className="text-xs muted">{t("ai.cleanupHint")}</p>
                    </div>
                  ) : null}

                  {tab === "metadata" ? (
                    <div className="flex flex-col gap-3">
                      <p className="text-[13px]">{t("ai.metadataIntro")}</p>
                      {metadataSuggestion ? (
                        <div className="card-soft p-3 flex flex-col gap-2 text-[13px]">
                          <div>
                            <strong>{t("metadata.fieldTitle")}:</strong> {metadataSuggestion.title || "—"}
                          </div>
                          <div>
                            <strong>{t("metadata.author")}:</strong> {metadataSuggestion.author || "—"}
                          </div>
                          <div>
                            <strong>{t("metadata.subject")}:</strong> {metadataSuggestion.subject || "—"}
                          </div>
                          <div className="flex items-center gap-2 flex-wrap">
                            <strong>{t("metadata.keywords")}:</strong>
                            {metadataSuggestion.keywords.map((keyword) => (
                              <Badge key={keyword}>{keyword}</Badge>
                            ))}
                          </div>
                          <Button size="sm" icon={<Save size={14} />} onClick={() => void applyMetadata()}>
                            {t("ai.metadataApply")}
                          </Button>
                        </div>
                      ) : (
                        <p className="text-xs muted">{t("ai.metadataHint")}</p>
                      )}
                    </div>
                  ) : null}
                </Card>

                {reasoning && !output && (tab !== "chat" || running) ? (
                  <Card className="p-4 flex flex-col gap-2">
                    <p className="font-semibold text-[13.5px] flex items-center gap-2">
                      <Brain size={15} /> {t("ai.thinkingTitle")}
                      {running ? <Spinner size={13} /> : null}
                    </p>
                    <p className="text-xs muted">{t("ai.thinkingHint")}</p>
                    <div className="card-soft p-3 max-h-[220px] overflow-y-auto whitespace-pre-wrap text-[12.5px] muted italic">
                      {reasoning}
                    </div>
                  </Card>
                ) : null}

                {output ? (
                  <Card className="p-4 flex flex-col gap-2">
                    <div className="flex items-center justify-between">
                      <p className="font-semibold text-[13.5px] flex items-center gap-2">
                        <ListChecks size={15} /> {t("ai.output")}
                        {running ? <Spinner size={13} /> : null}
                      </p>
                      <div className="flex gap-1.5">
                        <Button size="sm" variant="ghost" icon={<Copy size={14} />} onClick={() => void copyOutput()}>
                          {t("ai.copy")}
                        </Button>
                        <Button
                          size="sm"
                          icon={<Save size={14} />}
                          onClick={() => void saveOutput()}
                          disabled={running}
                        >
                          {t("ai.save")}
                        </Button>
                      </div>
                    </div>
                    {reasoning ? (
                      <div className="text-xs">
                        <button
                          className="btn btn-sm btn-ghost"
                          onClick={() => setShowReasoning((previous) => !previous)}
                        >
                          <Brain size={13} /> {showReasoning ? t("ai.hideThinking") : t("ai.showThinking")}
                        </button>
                        {showReasoning ? (
                          <div className="card-soft p-3 mt-2 max-h-[200px] overflow-y-auto whitespace-pre-wrap muted italic text-[12.5px]">
                            {reasoning}
                          </div>
                        ) : null}
                      </div>
                    ) : null}
                    <div
                      ref={outputRef}
                      className="card-soft p-3 max-h-[420px] overflow-y-auto whitespace-pre-wrap text-[13.5px] leading-relaxed"
                    >
                      {output}
                    </div>
                    {savedPath ? (
                      <p className="text-xs" style={{ color: "var(--ok)" }}>
                        {t("ai.saved")}: {savedPath}
                      </p>
                    ) : null}
                  </Card>
                ) : null}

                {error ? (
                  <Card className="p-3.5 flex items-start gap-3" soft>
                    <Badge tone="danger">!</Badge>
                    <p className="text-[13px] flex-1">{error}</p>
                  </Card>
                ) : null}
              </>
            )}
          </>
        }
        side={
          <>
            <Card className="p-4 flex flex-col gap-3">
              <div className="flex items-center gap-2">
                <Bot size={16} style={{ color: "var(--accent)" }} />
                <p className="font-semibold text-[13.5px]">{t("ai.runPanel")}</p>
              </div>
              {running ? (
                <>
                  <p className="text-xs muted">
                    {stage
                      ? t("ai.stage", { stage: stage.stage, current: stage.current, total: stage.total })
                      : t("common.processing")}
                  </p>
                  <div className="progress-track">
                    <div
                      className="progress-fill"
                      style={{
                        width: `${stage && stage.total > 0 ? Math.max(6, Math.round((stage.current / stage.total) * 100)) : 10}%`,
                      }}
                    />
                  </div>
                  <p className="text-xs muted" data-testid="ai-network-activity">
                    {networkActivity}
                  </p>
                  <Button variant="danger" icon={<Square size={14} />} onClick={stop}>
                    {t("ai.stop")}
                  </Button>
                </>
              ) : (
                <Button
                  variant="primary"
                  size="lg"
                  icon={<Sparkles size={16} />}
                  disabled={!canRun}
                  onClick={() => void activeRun()()}
                >
                  {t("ai.run")}
                </Button>
              )}
              {!configured ? (
                <p className="text-xs" style={{ color: "var(--warn)" }}>
                  {t("ai.addKeyFirst")}
                </p>
              ) : !consent ? (
                <p className="text-xs muted">{t("ai.needConsent")}</p>
              ) : null}
              {tab === "ask" && !question.trim() ? (
                <p className="text-xs muted">{t("ai.questionPlaceholder")}</p>
              ) : null}
              {tab === "chat" && !chatInput.trim() ? (
                <p className="text-xs muted">{label("ai.chatPlaceholder", "Ask about this document…")}</p>
              ) : null}
              {settings.aiAutoSave ? <p className="text-xs muted">{t("ai.autoSaveOn")}</p> : null}
              {lastResult && !settings.aiAutoSave ? (
                <Button size="sm" variant="ghost" icon={<Save size={14} />} onClick={saveToLibrary}>
                  {t("ai.saveToLibrary")}
                </Button>
              ) : null}
              {savedEntry ? (
                <p className="text-xs break-all" style={{ color: "var(--ok)" }}>
                  {t("ai.savedToLibrary")}: {savedEntry.filePath}
                </p>
              ) : null}
            </Card>

            {aiSettings ? (
              <Card soft className="p-4 text-xs muted flex flex-col gap-1.5">
                <p>
                  {t("ai.model")}: <strong className="text-[var(--text)]">{aiSettings.model}</strong>
                </p>
                {aiSettings.providerLabel ? (
                  <p>
                    {label("ai.provider", "Provider")}:{" "}
                    <strong className="text-[var(--text)]">{aiSettings.providerLabel}</strong>
                  </p>
                ) : null}
                <p>Key: {aiSettings.maskedKey || "—"}</p>
                <p>
                  {aiSettings.keyStorage === "dpapi"
                    ? t("ai.keySecure")
                    : aiSettings.keyStorage === "plain"
                      ? t("ai.keyPlain")
                      : t("ai.keyMissing")}
                </p>
                {aiSettings.providerNote ? <p>{aiSettings.providerNote}</p> : null}
                {preview ? <p>{formatBytes(preview.characters)}</p> : null}
              </Card>
            ) : null}

            {metadataResult ? <ResultCard result={metadataResult} onReset={() => setMetadataResult(null)} /> : null}
          </>
        }
      />
    </Screen>
  );
}
