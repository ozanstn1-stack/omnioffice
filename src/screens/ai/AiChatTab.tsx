import type { ReactNode, RefObject } from "react";
import { BookOpenCheck, ExternalLink, Plus, RefreshCw, Save, Send, Trash2, X } from "lucide-react";
import { clamp } from "../../lib/format";
import { useT } from "../../lib/i18n";
import { resolveCitationTarget } from "../../lib/citation-target";
import { openOfficePath } from "../../lib/office-store";
import type { Navigate } from "../../lib/nav";
import type { AiChatScope, AiCitation, AiPreview, SavedChatThread } from "../../lib/types";
import { Button, Checkbox, Field, Segmented, Spinner, TextArea, TextInput } from "../../components/ui";

export interface ChatEntry {
  role: "user" | "assistant" | "system";
  content: string;
  citations?: AiCitation[];
}

export interface AiChatTabProps {
  label: (key: string, fallback: string) => string;
  chatScope: AiChatScope;
  setChatScope: (scope: AiChatScope) => void;
  chatMessages: ChatEntry[];
  setChatMessages: (messages: ChatEntry[]) => void;
  showThreadsList: boolean;
  setShowThreadsList: (show: boolean) => void;
  savedThreads: SavedChatThread[];
  loadSavedThreads: () => void;
  handleSaveThread: () => void;
  handleLoadThread: (id: string) => void;
  handleDeleteThread: (id: string) => void;
  chatPage: number;
  setChatPage: (page: number) => void;
  preview: AiPreview | null;
  chatSelection: string;
  setChatSelection: (value: string) => void;
  vaultConsent: boolean;
  setVaultConsent: (value: boolean) => void;
  embeddingConsent: boolean;
  setEmbeddingConsent: (value: boolean) => void;
  effectiveSendImage: boolean;
  visionSupported: boolean;
  setSendImage: (value: boolean) => void;
  imageConsent: boolean;
  setImageConsent: (value: boolean) => void;
  chatRef: RefObject<HTMLDivElement | null>;
  running: boolean;
  renderChatContent: (text: string, citations?: AiCitation[]) => ReactNode;
  primaryPath: string | undefined;
  onNavigate?: Navigate;
  chatInput: string;
  setChatInput: (value: string) => void;
  canRun: boolean;
  runChat: () => Promise<void>;
}

export function AiChatTab(props: AiChatTabProps) {
  const t = useT();
  const {
    label,
    chatScope,
    setChatScope,
    chatMessages,
    setChatMessages,
    showThreadsList,
    setShowThreadsList,
    savedThreads,
    loadSavedThreads,
    handleSaveThread,
    handleLoadThread,
    handleDeleteThread,
    chatPage,
    setChatPage,
    preview,
    chatSelection,
    setChatSelection,
    vaultConsent,
    setVaultConsent,
    embeddingConsent,
    setEmbeddingConsent,
    effectiveSendImage,
    visionSupported,
    setSendImage,
    imageConsent,
    setImageConsent,
    chatRef,
    running,
    renderChatContent,
    primaryPath,
    onNavigate,
    chatInput,
    setChatInput,
    canRun,
    runChat,
  } = props;

  return (
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
          <Button size="sm" variant="ghost" icon={<Plus size={13} />} onClick={() => setChatMessages([])}>
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
                  {thread.title} ({thread.messages.length} {label("ai.msgsShort", "msgs")})
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
            onChange={(event) => setChatPage(clamp(Number(event.target.value) || 1, 1, preview?.pages || 1))}
          />
        </Field>
      ) : null}

      {chatScope === "selection" ? (
        <Field label={label("ai.scopeSelectionText", "Selected text")}>
          <TextArea
            value={chatSelection}
            rows={3}
            onChange={(event) => setChatSelection(event.target.value)}
            placeholder={label("ai.scopeSelectionPlaceholder", "Paste the text you selected in the document")}
          />
        </Field>
      ) : null}

      {chatScope === "vault" ? (
        <div className="flex flex-col gap-1">
          <p className="text-xs muted">
            {label(
              "ai.scopeVaultNote",
              "Vault mode queries all documents in your library with hybrid semantic embeddings and keyword matching.",
            )}
          </p>
          <Checkbox
            checked={vaultConsent}
            onChange={setVaultConsent}
            label={label("ai.consentVault", "Query the whole vault (all library documents)")}
          />
          <Checkbox
            checked={embeddingConsent}
            onChange={setEmbeddingConsent}
            label={label("ai.consentEmbedding", "Embed document text for semantic search")}
          />
        </div>
      ) : null}

      <div className="flex items-center gap-2">
        <Checkbox
          checked={effectiveSendImage}
          disabled={!visionSupported}
          onChange={setSendImage}
          label={label("ai.sendImageVision", "Send rendered page as image (Vision)")}
        />
        {!visionSupported ? (
          <span className="text-xs muted">
            {label("ai.visionUnsupported", "This provider does not accept image input.")}
          </span>
        ) : null}
      </div>

      {effectiveSendImage ? (
        <Checkbox
          checked={imageConsent}
          onChange={setImageConsent}
          label={label("ai.consentImage", "Send the rendered page image")}
        />
      ) : null}

      <div ref={chatRef} className="card-soft p-3 max-h-[420px] overflow-y-auto flex flex-col gap-2">
        {chatMessages.length === 0 ? (
          <p className="text-xs muted">{label("ai.chatEmpty", "Ask a question about this document.")}</p>
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
                    renderChatContent(message.content, message.citations)
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
                            const target = resolveCitationTarget(c.path, c.page, c.location, primaryPath);
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
                          title={c.quote || c.snippet || t("ai.jumpToPage", { page: c.page })}
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
                          {c.snippet ? <span className="opacity-70 max-w-[140px] truncate">({c.snippet})</span> : null}
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
          disabled={!canRun || !chatInput.trim() || (chatScope === "selection" && !chatSelection.trim())}
          onClick={() => void runChat()}
        >
          {label("ai.chatSend", "Send")}
        </Button>
      </div>
    </div>
  );
}
