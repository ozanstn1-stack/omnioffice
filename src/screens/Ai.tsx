import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  BookOpenCheck,
  Bot,
  Check,
  Brain,
  Copy,
  ExternalLink,
  Languages,
  Layers,
  ListChecks,
  MessageCircleQuestion,
  MessageSquare,
  Plus,
  RefreshCw,
  Save,
  Send,
  Sparkles,
  Square,
  Table,
  Tags,
  Trash2,
  Wand2,
  X,
} from "lucide-react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import {
  isAndroid,
  pickAndroidSaveTarget,
  pickOfficeFiles,
  publishOutputs,
  saveTextOnAndroid,
  type PublishTarget,
} from "../lib/mobile";
import {
  aiAsk,
  aiBatchRun,
  aiChat,
  aiChatDelete,
  aiChatList,
  aiChatLoad,
  aiChatSave,
  aiCleanupText,
  aiDocumentPreview,
  aiExamplePrompts,
  aiExtractTable,
  aiLibrarySave,
  aiSaveOutput,
  aiSuggestMetadata,
  aiSummarize,
  aiTranslate,
  cancelJob,
  editMetadata,
  logFrontend,
  onAiBatchProgress,
  onAiChunk,
  onAiProgress,
} from "../lib/api";
import { Badge, Button, Card, Checkbox, Field, Segmented, Spinner, TextArea, TextInput } from "../components/ui";
import { DropZone, InfoStrip, ResultCard } from "../components/files";
import { OptionCard, Screen, TwoColumn } from "../components/layout";
import { useT } from "../lib/i18n";
import { resolveCitationTarget } from "../lib/citation-target";
import { openOfficePath } from "../lib/office-store";
import { consentKeys, isLocalProvider, plainProviderName, providerDisplayName } from "../lib/ai-providers";
import { useTool } from "../lib/useTool";
import { useDev, useSettings, useToasts } from "../lib/store";
import { clamp, fileBaseName, formatBytes, isPdf, uid } from "../lib/format";
import type {
  AiBatchItemRequest,
  AiBatchResult,
  AiChatScope,
  AiCitation,
  AiExamplePrompts,
  AiLibraryEntry,
  AiMetadataSuggestion,
  AiPreview,
  AiSettingsView,
  AiTableResult,
  AiTextResult,
  DocChatMessage,
  OpResult,
  SavedChatThread,
  SummaryLength,
  SummaryStyle,
} from "../lib/types";
import type { Navigate } from "../lib/nav";

type AiTab = "chat" | "summary" | "translate" | "extract" | "batch" | "ask" | "cleanup" | "metadata";

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
  role: "user" | "assistant" | "system";
  content: string;
  citations?: AiCitation[];
}

const CITATION_PATTERN = /(\[page\s+\d+\])/gi;

export function Ai({
  initialFiles,
  dragging,
  onNavigate,
}: {
  initialFiles?: string[];
  dragging: boolean;
  onNavigate?: Navigate;
}) {
  const t = useT();
  const label = useCallback(
    (key: string, fallback: string) => {
      const value = t(key);
      return value === key ? fallback : value;
    },
    [t],
  );
  const session = useTool({ suffix: "_ai", accept: "document", initialPaths: initialFiles });
  const settingsLoaded = useSettings((s) => s.loaded);
  const settings = useSettings((s) => s.settings);
  const pushToast = useToasts((s) => s.push);
  const [aiSettings, setAiSettings] = useState<AiSettingsViewExt | null>(null);
  const devTab = useDev((s) => s.tab) as AiTab | null;
  const [tab, setTab] = useState<AiTab>(() =>
    devTab && ["chat", "summary", "translate", "extract", "batch", "ask", "cleanup", "metadata"].includes(devTab)
      ? devTab
      : "chat",
  );
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

  // Summary state
  const [summaryOptions, setSummaryOptions] = useState({
    language: "auto",
    length: "medium" as SummaryLength,
    style: "paragraph" as SummaryStyle,
    focus: "",
  });

  // Translate state
  const [targetLanguage, setTargetLanguage] = useState("tr");
  const [bilingual, setBilingual] = useState(false);

  // Ask state
  const [question, setQuestion] = useState("");

  // Document Chat state
  const [chatMessages, setChatMessages] = useState<ChatEntry[]>([]);
  const [chatInput, setChatInput] = useState("");
  const [chatScope, setChatScope] = useState<AiChatScope>("document");
  const [chatPage, setChatPage] = useState(1);
  const [chatSelection, setChatSelection] = useState("");
  const [sendImage, setSendImage] = useState(false);
  const [savedThreads, setSavedThreads] = useState<SavedChatThread[]>([]);
  const [showThreadsList, setShowThreadsList] = useState(false);

  // Table Extraction state
  const [extractPages, setExtractPages] = useState("");
  const [extractExtraFields, setExtractExtraFields] = useState("");
  const [tableResult, setTableResult] = useState<AiTableResult | null>(null);

  // Batch AI Queue state
  const [batchFiles, setBatchFiles] = useState<string[]>([]);
  const [batchAction, setBatchAction] = useState<"summarize" | "translate" | "classify" | "extract">("summarize");
  const [batchLang, setBatchLang] = useState("tr");
  const [batchProgress, setBatchProgress] = useState<{
    index: number;
    total: number;
    path: string;
    action: string;
  } | null>(null);
  const [batchResult, setBatchResult] = useState<AiBatchResult | null>(null);

  const jobId = useRef(uid("ai"));
  const outputRef = useRef<HTMLDivElement>(null);
  const chatRef = useRef<HTMLDivElement>(null);
  const chatActiveRef = useRef(false);

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
    let unlistenBatch: (() => void) | undefined;

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

    void onAiBatchProgress((payload) => {
      if (payload.jobId !== jobId.current) return;
      setBatchProgress(payload);
    }).then((fn) => {
      unlistenBatch = fn;
    });

    return () => {
      unlistenChunk?.();
      unlistenProgress?.();
      unlistenBatch?.();
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
    /* eslint-disable react-hooks/set-state-in-effect */
    setOutput("");
    setError(null);
    setMetadataSuggestion(null);
    setMetadataResult(null);
    setTableResult(null);
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
  const canRun =
    configured &&
    consent &&
    !running &&
    (tab === "batch"
      ? batchFiles.length > 0
      : tab === "chat" && chatScope === "vault"
        ? true
        : Boolean(session.primary));

  const providerLabel = providerDisplayName(aiSettings?.provider, aiSettings?.providerLabel);
  const notice = consentKeys(aiSettings?.provider);
  const noticeParams = {
    provider: isLocalProvider(aiSettings?.provider) ? plainProviderName(providerLabel) : providerLabel,
  };
  const networkActivityKey = "ai.networkActivity";
  const networkActivityText = t(networkActivityKey, { chars: preview?.characters ?? 0, provider: providerLabel });
  const networkActivity =
    networkActivityText === networkActivityKey
      ? `Sending ${preview?.characters ?? 0} characters to ${providerLabel}`
      : networkActivityText;

  const fail = (message: string) => {
    setError(message);
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
          : tab === "extract"
            ? runExtractTable()
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
        options: `${summaryOptions.length}/${summaryOptions.style}${summaryOptions.language !== "auto" ? ` · ${summaryOptions.language}` : ""}`,
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

  const runChat = async (questionOverride?: string) => {
    const asked = (questionOverride ?? chatInput).trim();
    if (chatScope !== "vault" && !session.primary) return;
    if (!asked || running) return;
    if (chatScope === "selection" && !chatSelection.trim()) return;

    setChatMessages((previous) => [...previous, { role: "user", content: asked }, { role: "assistant", content: "" }]);
    setChatInput("");
    resetJob();
    chatActiveRef.current = true;
    setRunning(true);
    setError(null);

    try {
      const historyPayload: DocChatMessage[] = chatMessages.map((m) => ({
        role: m.role,
        content: m.content,
      }));

      const result = await aiChat({
        path: chatScope === "vault" ? undefined : session.primary?.path,
        question: asked,
        history: historyPayload,
        scope: chatScope,
        page: chatScope === "page" ? chatPage : undefined,
        selection: chatScope === "selection" ? chatSelection : undefined,
        sendImage,
        password: session.password || undefined,
        jobId: jobId.current,
      });

      setChatMessages((previous) => {
        const next = [...previous];
        const last = next[next.length - 1];
        if (last && last.role === "assistant") {
          next[next.length - 1] = {
            role: "assistant",
            content: result.answer || last.content,
            citations: result.citations,
          };
        }
        return next;
      });

      setLastResult({
        kind: "chat",
        result: {
          text: result.answer,
          pages: (result.citations || []).length,
          characters: result.answer.length,
          model: result.model,
          elapsedMs: result.elapsedMs,
        },
        options: asked,
      });
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
        const pageNum = parseInt(match[1], 10);
        return (
          <button
            key={index}
            type="button"
            className="badge inline-flex items-center gap-1 cursor-pointer hover:underline mx-0.5 text-xs"
            style={{ background: "var(--accent-subtle)", color: "var(--accent)" }}
            onClick={() => {
              // The citation may point at another document (the vault scope);
              // open that file, not the one currently loaded.
              const target = resolveCitationTarget(session.primary?.path, pageNum, undefined, session.primary?.path);
              if (!target) return;
              if (target.kind === "pdf") {
                onNavigate?.("reader", { files: [target.path], targetPage: target.page });
              } else {
                void openOfficePath(target.path).then(() => onNavigate?.("office"));
              }
            }}
            title={t("ai.jumpToPage", { page: pageNum })}
          >
            <BookOpenCheck size={11} /> {label("ai.citationPage", "page")} {pageNum}
          </button>
        );
      }
      return <span key={index}>{part}</span>;
    });

  const loadSavedThreads = async () => {
    try {
      const list = await aiChatList();
      setSavedThreads(list);
      setShowThreadsList(true);
    } catch (err) {
      fail(String((err as { message?: string })?.message ?? err));
    }
  };

  const handleSaveThread = async () => {
    if (chatMessages.length === 0) return;
    const firstUser = chatMessages.find((m) => m.role === "user");
    const defaultTitle = firstUser ? firstUser.content.slice(0, 40) : "Document Chat";
    const title = window.prompt(label("ai.saveThreadPrompt", "Chat thread title:"), defaultTitle) || defaultTitle;
    const thread: SavedChatThread = {
      id: uid("chat_thread"),
      title,
      documentPath: session.primary?.path,
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
      messages: chatMessages.map((m) => ({ role: m.role, content: m.content })),
    };
    try {
      await aiChatSave(thread);
      pushToast({ kind: "success", title: label("ai.threadSaved", "Chat thread saved") });
    } catch (err) {
      fail(String((err as { message?: string })?.message ?? err));
    }
  };

  const handleLoadThread = async (id: string) => {
    try {
      const thread = await aiChatLoad(id);
      setChatMessages(thread.messages.map((m) => ({ role: m.role, content: m.content })));
      setShowThreadsList(false);
      pushToast({ kind: "info", title: label("ai.threadLoaded", "Loaded thread"), detail: thread.title });
    } catch (err) {
      fail(String((err as { message?: string })?.message ?? err));
    }
  };

  const handleDeleteThread = async (id: string) => {
    try {
      await aiChatDelete(id);
      setSavedThreads((prev) => prev.filter((t) => t.id !== id));
      pushToast({ kind: "info", title: label("ai.threadDeleted", "Thread deleted") });
    } catch (err) {
      fail(String((err as { message?: string })?.message ?? err));
    }
  };

  const runExtractTable = async () => {
    if (!session.primary) return;
    resetJob();
    setRunning(true);
    setError(null);
    setTableResult(null);
    try {
      const pagesArr = extractPages
        .split(/[,\s]+/)
        .map((p) => parseInt(p.trim(), 10))
        .filter((n) => !isNaN(n) && n > 0);
      const extraFieldsArr = extractExtraFields
        .split(/[,\n]+/)
        .map((s) => s.trim())
        .filter(Boolean);

      const result = await aiExtractTable({
        path: session.primary.path,
        pages: pagesArr.length > 0 ? pagesArr : undefined,
        extraFields: extraFieldsArr.length > 0 ? extraFieldsArr : undefined,
        password: session.password || undefined,
        jobId: jobId.current,
      });
      setTableResult(result);
    } catch (runError) {
      fail(String((runError as { message?: string })?.message ?? runError));
    } finally {
      setRunning(false);
      setStage(null);
    }
  };

  const exportTableCsv = async () => {
    if (!tableResult || tableResult.rows.length === 0) return;
    const cols = tableResult.columns;
    const escapeCsv = (val: unknown) => {
      let str = String(val ?? "");
      // Spreadsheet formula injection: a leading = + - @ (or tab/CR) turns the
      // cell into a formula in Excel/Calc; the apostrophe keeps it text.
      if (/^[=+\-@\t\r]/.test(str)) {
        str = `'${str}`;
      }
      if (str.includes(",") || str.includes('"') || str.includes("\n") || str.includes("\r")) {
        return `"${str.replace(/"/g, '""')}"`;
      }
      return str;
    };
    const headerLine = cols.map(escapeCsv).join(",");
    const rowLines = tableResult.rows.map((row) => {
      return cols
        .map((col) => {
          // Exact field names, as produced by the backend; anything else is a
          // custom field. Substring matching used to cross-map similar names.
          switch (col) {
            case "page":
              return escapeCsv(row.page);
            case "vendor":
              return escapeCsv(row.vendor);
            case "date":
              return escapeCsv(row.date);
            case "invoiceNumber":
              return escapeCsv(row.invoiceNumber);
            case "currency":
              return escapeCsv(row.currency);
            case "subtotal":
              return escapeCsv(row.subtotal ?? "");
            case "tax":
              return escapeCsv(row.tax ?? "");
            case "total":
              return escapeCsv(row.total ?? "");
            case "lineItems":
              return escapeCsv(row.lineItems);
            default:
              return escapeCsv(row.customFields?.[col] ?? "");
          }
        })
        .join(",");
    });
    const csvContent = [headerLine, ...rowLines].join("\r\n");

    const base = session.primary ? session.primary.path.replace(/\.[^.\\/]+$/, "") : "table_extract";
    const defaultName = `${base}_table.csv`;

    if (isAndroid()) {
      try {
        const saved = await saveTextOnAndroid(csvContent, fileBaseName(defaultName));
        if (saved) {
          pushToast({ kind: "success", title: label("ai.csvExportSuccess", "Table exported to CSV") });
        }
      } catch (saveError) {
        fail(String((saveError as { message?: string })?.message ?? saveError));
      }
      return;
    }

    const picked = await saveDialog({
      title: t("common.save"),
      defaultPath: defaultName,
      filters: [{ name: "CSV", extensions: ["csv"] }],
    });
    if (!picked) return;
    try {
      await aiSaveOutput(String(picked), csvContent, "unique_name");
      pushToast({
        kind: "success",
        title: label("ai.csvExportSuccess", "Table exported to CSV"),
        detail: String(picked),
      });
    } catch (saveError) {
      fail(String((saveError as { message?: string })?.message ?? saveError));
    }
  };

  const handlePickBatchFiles = async () => {
    try {
      // Android does not hand out paths for picked files: the SAF bridge in
      // mobile.ts copies them into the cache and returns real paths.
      const paths = isAndroid()
        ? await pickOfficeFiles(true)
        : await (async () => {
            const picked = await openDialog({
              multiple: true,
              filters: [{ name: "Documents", extensions: ["pdf", "docx", "xlsx", "pptx", "txt", "md"] }],
            });
            if (!picked) return [];
            return Array.isArray(picked) ? picked : [picked];
          })();
      if (paths.length) {
        setBatchFiles((prev) => Array.from(new Set([...prev, ...paths])));
      }
    } catch {
      /* open canceled */
    }
  };

  const runBatchQueue = async () => {
    if (batchFiles.length === 0 || running) return;
    resetJob();
    setRunning(true);
    setError(null);
    setBatchProgress(null);
    setBatchResult(null);
    try {
      const items: AiBatchItemRequest[] = batchFiles.map((p) => ({
        path: p,
        action: batchAction,
        targetLang: batchAction === "translate" ? batchLang : undefined,
      }));
      const res = await aiBatchRun({
        items,
        jobId: jobId.current,
      });
      setBatchResult(res);
      const title =
        res.status === "cancelled"
          ? label("ai.batchCancelled", "Batch stopped before every file was processed")
          : res.status === "failed"
            ? label("ai.batchFailed", "Batch failed: every file reported an error")
            : label("ai.batchDone", "Batch processing completed");
      pushToast({
        kind: res.status === "completed" ? "success" : "info",
        title,
        detail: `${res.results.length} items (${(res.totalElapsedMs / 1000).toFixed(1)}s)`,
      });
    } catch (err) {
      fail(String((err as { message?: string })?.message ?? err));
    } finally {
      setRunning(false);
      setBatchProgress(null);
    }
  };

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
        { path: targetPath, overwrite: "replace" },
        {
          title: metadataSuggestion.title,
          author: metadataSuggestion.author,
          subject: metadataSuggestion.subject,
          keywords: metadataSuggestion.keywords.join(", "),
          creator: "OmniOffice (AI)",
          producer: "OmniOffice (AI)",
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
    const extension = "md";
    const base = session.primary ? session.primary.path.replace(/\.[^.\\/]+$/, "") : "ai-output";
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
      { id: "chat", label: label("ai.tabChat", "Document chat"), icon: <MessageSquare size={14} /> },
      { id: "summary", label: t("ai.tabSummary"), icon: <Sparkles size={14} /> },
      { id: "translate", label: t("ai.tabTranslate"), icon: <Languages size={14} /> },
      { id: "extract", label: t("ai.tabExtract"), icon: <Table size={14} /> },
      { id: "batch", label: t("ai.tabBatch"), icon: <Layers size={14} /> },
      { id: "ask", label: t("ai.tabAsk"), icon: <MessageCircleQuestion size={14} /> },
      { id: "cleanup", label: t("ai.tabCleanup"), icon: <Wand2 size={14} /> },
      { id: "metadata", label: t("ai.tabMetadata"), icon: <Tags size={14} /> },
    ],
    [t, label],
  );

  const activeRun = () => {
    switch (tab) {
      case "chat":
        return runChat;
      case "summary":
        return runSummary;
      case "translate":
        return runTranslate;
      case "extract":
        return runExtractTable;
      case "batch":
        return runBatchQueue;
      case "ask":
        return runAsk;
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
                  {t("ai.privacyTitle")}: {t(notice.privacyBody, noticeParams)}
                </div>
              </Card>
            ) : null}

            <Card className="p-4">
              <div className="flex flex-wrap items-center gap-2">
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
            </Card>

            {tab !== "batch" && !session.primary && (tab !== "chat" || chatScope !== "vault") ? (
              <DropZone onPaths={(paths) => void session.addPaths(paths)} dragging={dragging} accept="any" />
            ) : (
              <>
                {session.primary ? (
                  <OptionCard>
                    <div className="flex items-center justify-between gap-3">
                      <InfoStrip info={session.info} error={session.infoError} />
                      <Button size="sm" variant="ghost" onClick={() => void session.pickFiles()}>
                        {t("common.selectFile")}
                      </Button>
                    </div>
                  </OptionCard>
                ) : null}

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
                      <p className="text-[13px] muted">{t(notice.privacyBody, noticeParams)}</p>
                      <Checkbox checked={consent} onChange={setConsent} label={t(notice.consent, noticeParams)} />
                    </>
                  )}
                </Card>

                <Card className="p-4">
                  {tab === "chat" ? (
                    <div className="flex flex-col gap-3">
                      <div className="flex items-center justify-between gap-2 flex-wrap">
                        <Field
                          label={label("ai.sendScope", "Send scope")}
                          hint={label("ai.sendScopeHint", "What the question is restricted to")}
                        >
                          <Segmented<AiChatScope>
                            value={chatScope}
                            onChange={setChatScope}
                            options={[
                              { value: "document", label: label("ai.scopeDocument", "Document") },
                              { value: "page", label: label("ai.scopePage", "Current page") },
                              { value: "selection", label: label("ai.scopeSelection", "Selected text") },
                              { value: "vault", label: label("ai.scopeVault", "Vault (All)") },
                            ]}
                          />
                        </Field>

                        <div className="flex items-center gap-2 mt-auto">
                          <Button size="sm" variant="ghost" icon={<RefreshCw size={13} />} onClick={loadSavedThreads}>
                            {label("ai.savedThreads", "Threads")}
                          </Button>
                          <Button
                            size="sm"
                            variant="ghost"
                            icon={<Save size={13} />}
                            disabled={chatMessages.length === 0}
                            onClick={handleSaveThread}
                          >
                            {label("ai.saveThread", "Save")}
                          </Button>
                          <Button
                            size="sm"
                            variant="ghost"
                            icon={<Plus size={13} />}
                            onClick={() => setChatMessages([])}
                          >
                            {label("ai.newThread", "New")}
                          </Button>
                        </div>
                      </div>

                      {showThreadsList ? (
                        <div className="card-soft p-3 flex flex-col gap-2 max-h-[160px] overflow-y-auto">
                          <div className="flex items-center justify-between text-xs font-semibold">
                            <span>{label("ai.savedThreadsList", "Saved Chat Threads")}</span>
                            <Button size="sm" variant="ghost" onClick={() => setShowThreadsList(false)}>
                              <X size={12} />
                            </Button>
                          </div>
                          {savedThreads.length === 0 ? (
                            <p className="text-xs muted">{label("ai.noSavedThreads", "No saved threads yet.")}</p>
                          ) : (
                            savedThreads.map((thread) => (
                              <div
                                key={thread.id}
                                className="flex items-center justify-between gap-2 text-xs p-1.5 rounded hover:bg-[var(--surface-3)]"
                              >
                                <button
                                  type="button"
                                  className="text-left flex-1 truncate font-medium hover:underline cursor-pointer"
                                  onClick={() => handleLoadThread(thread.id)}
                                >
                                  {thread.title} ({thread.messages.length} msgs)
                                </button>
                                <Button
                                  size="sm"
                                  variant="ghost"
                                  icon={<Trash2 size={12} />}
                                  onClick={() => handleDeleteThread(thread.id)}
                                />
                              </div>
                            ))
                          )}
                        </div>
                      ) : null}

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
                            rows={3}
                            onChange={(event) => setChatSelection(event.target.value)}
                            placeholder={label(
                              "ai.scopeSelectionPlaceholder",
                              "Paste the text you selected in the document",
                            )}
                          />
                        </Field>
                      ) : null}

                      {chatScope === "vault" ? (
                        <p className="text-xs muted">
                          {label(
                            "ai.scopeVaultNote",
                            "Vault mode queries all documents in your library with hybrid semantic embeddings and keyword matching.",
                          )}
                        </p>
                      ) : null}

                      <div className="flex items-center gap-2">
                        <Checkbox
                          checked={sendImage}
                          onChange={setSendImage}
                          label={label("ai.sendImageVision", "Send rendered page as image (Vision)")}
                        />
                      </div>

                      <div ref={chatRef} className="card-soft p-3 max-h-[420px] overflow-y-auto flex flex-col gap-2">
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

                                  {message.citations && message.citations.length > 0 ? (
                                    <div className="flex flex-wrap gap-1.5 mt-2 pt-2 border-t border-[var(--border)]">
                                      <span className="text-[11px] muted w-full font-medium">
                                        {label("ai.sources", "Cited Sources")}:
                                      </span>
                                      {message.citations.map((c, ci) => (
                                        <button
                                          key={ci}
                                          type="button"
                                          className="badge hover:opacity-80 transition cursor-pointer flex items-center gap-1.5 p-1 px-2 text-xs bg-[var(--surface-3)] rounded border border-[var(--border)]"
                                          onClick={() => {
                                            const target = resolveCitationTarget(
                                              c.path,
                                              c.page,
                                              c.location,
                                              session.primary?.path,
                                            );
                                            if (!target) return;
                                            if (target.kind === "pdf") {
                                              onNavigate?.("reader", {
                                                files: [target.path],
                                                targetPage: target.page > 0 ? target.page : undefined,
                                              });
                                            } else {
                                              void openOfficePath(target.path).then(() => onNavigate?.("office"));
                                            }
                                          }}
                                          title={c.quote || c.snippet || `Jump to page ${c.page}`}
                                        >
                                          <BookOpenCheck size={11} style={{ color: "var(--accent)" }} />
                                          <span>
                                            {label("ai.citationPage", "page")} {c.page}
                                          </span>
                                          {c.verified === false ? (
                                            <span
                                              className="badge"
                                              style={{
                                                background: "var(--danger-subtle, #fee2e2)",
                                                color: "var(--danger, #dc2626)",
                                              }}
                                              title={c.quote ? `“${c.quote}”` : undefined}
                                            >
                                              {label("ai.unverified", "unverified")}
                                            </span>
                                          ) : null}
                                          {c.snippet ? (
                                            <span className="opacity-70 max-w-[140px] truncate">({c.snippet})</span>
                                          ) : null}
                                          <ExternalLink size={10} className="opacity-50" />
                                        </button>
                                      ))}
                                    </div>
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
                            type="button"
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

                  {tab === "extract" ? (
                    <div className="flex flex-col gap-3">
                      <p className="text-[13px] muted">
                        {label(
                          "ai.extractIntro",
                          "Extract structured invoice, receipt, and purchase order line items into a table preview and CSV format.",
                        )}
                      </p>
                      <div
                        className="grid gap-3"
                        style={{ gridTemplateColumns: "repeat(auto-fit, minmax(200px, 1fr))" }}
                      >
                        <Field
                          label={label("ai.extractPages", "Pages filter")}
                          hint={label("ai.extractPagesHint", "Optional: e.g. 1, 1-3 (leave empty for all)")}
                        >
                          <TextInput
                            value={extractPages}
                            onChange={(e) => setExtractPages(e.target.value)}
                            placeholder="1, 2"
                          />
                        </Field>
                        <Field
                          label={label("ai.extractExtraFields", "Extra fields")}
                          hint={label("ai.extractExtraFieldsHint", "Comma-separated: e.g. Tax ID, PO, IBAN")}
                        >
                          <TextInput
                            value={extractExtraFields}
                            onChange={(e) => setExtractExtraFields(e.target.value)}
                            placeholder="tax_id, iban, payment_due"
                          />
                        </Field>
                      </div>

                      {tableResult ? (
                        <div className="flex flex-col gap-2 mt-2">
                          <div className="flex items-center justify-between">
                            <span className="text-xs font-semibold">
                              {label("ai.extractedRows", "Extracted Table Rows")} ({tableResult.rows.length})
                            </span>
                            <Button size="sm" icon={<Save size={14} />} onClick={() => void exportTableCsv()}>
                              {label("ai.exportCsv", "Export CSV")}
                            </Button>
                          </div>
                          <div className="card-soft overflow-x-auto max-h-[360px]">
                            <table className="w-full text-xs text-left border-collapse">
                              <thead>
                                <tr className="border-b border-[var(--border)] bg-[var(--surface-3)]">
                                  {tableResult.columns.map((col) => (
                                    <th key={col} className="p-2 font-semibold">
                                      {col}
                                    </th>
                                  ))}
                                </tr>
                              </thead>
                              <tbody>
                                {tableResult.rows.map((row, ri) => (
                                  <tr key={ri} className="border-b border-[var(--border)] hover:bg-[var(--surface-2)]">
                                    <td className="p-2">{row.page}</td>
                                    <td className="p-2 font-medium">{row.vendor || "—"}</td>
                                    <td className="p-2">{row.date || "—"}</td>
                                    <td className="p-2">{row.invoiceNumber || "—"}</td>
                                    <td className="p-2">{row.currency || "—"}</td>
                                    <td className="p-2">{row.subtotal != null ? row.subtotal : "—"}</td>
                                    <td className="p-2">{row.tax != null ? row.tax : "—"}</td>
                                    <td className="p-2 font-semibold">{row.total != null ? row.total : "—"}</td>
                                    <td className="p-2 max-w-[200px] truncate" title={row.lineItems}>
                                      {row.lineItems || "—"}
                                    </td>
                                    {tableResult.columns.slice(9).map((extraCol) => (
                                      <td key={extraCol} className="p-2">
                                        {row.customFields?.[extraCol] || "—"}
                                      </td>
                                    ))}
                                  </tr>
                                ))}
                              </tbody>
                            </table>
                          </div>
                        </div>
                      ) : null}
                    </div>
                  ) : null}

                  {tab === "batch" ? (
                    <div className="flex flex-col gap-3">
                      <p className="text-[13px] muted">
                        {label(
                          "ai.batchIntro",
                          "Queue multiple documents for batch summarization, translation, classification, or extraction.",
                        )}
                      </p>

                      <div className="flex items-center gap-2 flex-wrap">
                        <Button size="sm" icon={<Plus size={14} />} onClick={handlePickBatchFiles}>
                          {label("ai.addBatchFiles", "Add Files")}
                        </Button>
                        {session.primary && !batchFiles.includes(session.primary.path) ? (
                          <Button
                            size="sm"
                            variant="ghost"
                            onClick={() => {
                              const activePath = session.primary?.path;
                              if (activePath) setBatchFiles((prev) => [...prev, activePath]);
                            }}
                          >
                            + {label("ai.addCurrentFile", "Add Active File")}
                          </Button>
                        ) : null}
                        {batchFiles.length > 0 ? (
                          <Button size="sm" variant="ghost" onClick={() => setBatchFiles([])}>
                            {label("ai.clearBatch", "Clear All")}
                          </Button>
                        ) : null}
                      </div>

                      <div
                        className="grid gap-3"
                        style={{ gridTemplateColumns: "repeat(auto-fit, minmax(200px, 1fr))" }}
                      >
                        <Field label={label("ai.batchAction", "Batch Action")}>
                          <Segmented<"summarize" | "translate" | "classify" | "extract">
                            value={batchAction}
                            onChange={setBatchAction}
                            options={[
                              { value: "summarize", label: label("ai.actionSummarize", "Summarize") },
                              { value: "translate", label: label("ai.actionTranslate", "Translate") },
                              { value: "classify", label: label("ai.actionClassify", "Classify") },
                              { value: "extract", label: label("ai.actionExtract", "Extract") },
                            ]}
                          />
                        </Field>

                        {batchAction === "translate" ? (
                          <Field label={label("ai.batchTargetLang", "Target Language")}>
                            <TextInput
                              value={batchLang}
                              onChange={(e) => setBatchLang(e.target.value)}
                              placeholder="tr, en, de"
                            />
                          </Field>
                        ) : null}
                      </div>

                      <div className="card-soft p-3 flex flex-col gap-1.5 max-h-[180px] overflow-y-auto">
                        <span className="text-xs font-semibold">
                          {label("ai.queuedFiles", "Queued Files")} ({batchFiles.length}):
                        </span>
                        {batchFiles.length === 0 ? (
                          <p className="text-xs muted">{label("ai.noFilesQueued", "No files in the batch queue.")}</p>
                        ) : (
                          batchFiles.map((file, idx) => (
                            <div
                              key={idx}
                              className="flex items-center justify-between gap-2 text-xs p-1 rounded bg-[var(--surface-2)]"
                            >
                              <span className="truncate flex-1" title={file}>
                                {fileBaseName(file)}
                              </span>
                              <Button
                                size="sm"
                                variant="ghost"
                                onClick={() => setBatchFiles((prev) => prev.filter((_, i) => i !== idx))}
                              >
                                <X size={12} />
                              </Button>
                            </div>
                          ))
                        )}
                      </div>

                      {batchProgress ? (
                        <div className="card-soft p-2 flex flex-col gap-1 text-xs">
                          <div className="flex justify-between">
                            <span>
                              {label("ai.batchProgress", "Processing file")} {batchProgress.index} /{" "}
                              {batchProgress.total}
                            </span>
                            <span className="font-medium">{fileBaseName(batchProgress.path)}</span>
                          </div>
                          <div className="progress-track">
                            <div
                              className="progress-fill"
                              style={{
                                width: `${Math.round((batchProgress.index / batchProgress.total) * 100)}%`,
                              }}
                            />
                          </div>
                        </div>
                      ) : null}

                      {batchResult ? (
                        <div className="card-soft p-3 flex flex-col gap-2 max-h-[280px] overflow-y-auto">
                          <span className="text-xs font-semibold">
                            {label("ai.batchResults", "Queue Results")} ({batchResult.results.length}):
                          </span>
                          {batchResult.results.map((res, i) => (
                            <div
                              key={i}
                              className="flex flex-col gap-1 p-2 rounded text-xs bg-[var(--surface-2)] border border-[var(--border)]"
                            >
                              <div className="flex items-center justify-between">
                                <span className="font-semibold">{fileBaseName(res.path)}</span>
                                <Badge tone={res.success ? "ok" : "danger"}>
                                  {res.success
                                    ? label("ai.statusSuccess", "Success")
                                    : label("ai.statusFailed", "Failed")}
                                </Badge>
                              </div>
                              {res.output ? (
                                <p className="text-[12px] muted whitespace-pre-wrap max-h-[80px] overflow-y-auto">
                                  {res.output}
                                </p>
                              ) : null}
                              {res.error ? (
                                <p className="text-[12px]" style={{ color: "var(--danger)" }}>
                                  {res.error}
                                </p>
                              ) : null}
                              <span className="text-[10px] muted">
                                {(res.elapsedMs / 1000).toFixed(2)}s · {res.action}
                              </span>
                            </div>
                          ))}
                        </div>
                      ) : null}
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
                          {session.primary && isPdf(session.primary.path) ? (
                            <Button size="sm" icon={<Save size={14} />} onClick={() => void applyMetadata()}>
                              {t("ai.metadataApply")}
                            </Button>
                          ) : (
                            <p className="text-xs muted">{t("ai.metadataPdfOnly")}</p>
                          )}
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
              {tab === "batch" && batchFiles.length === 0 ? (
                <p className="text-xs muted">{label("ai.addFilesPrompt", "Add files to run batch queue")}</p>
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
                    : aiSettings.keyStorage === "keystore"
                      ? t("ai.keyKeystore")
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
