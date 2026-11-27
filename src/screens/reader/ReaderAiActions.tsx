import { Copy, FileText, Trash2 } from "lucide-react";
import { Button, IconButton } from "../../components/ui";
import { useT } from "../../lib/i18n";
import type { Navigate } from "../../lib/nav";
import { useToasts } from "../../lib/store";
import {
  formatConversationMarkdown,
  openAnswerInWriter,
  openConversationInWriter,
  type ReaderConversationEntry,
} from "./reader-ai-export";

/** Copy and Open-in-Writer actions rendered below completed Reader AI answers. */
export function ReaderAiAnswerActions({
  path,
  content,
  onNavigate,
}: {
  path: string;
  content: string;
  onNavigate?: Navigate;
}) {
  const t = useT();
  return (
    <div className="flex items-center gap-1 pt-1">
      <Button
        size="sm"
        variant="ghost"
        icon={<Copy size={12} />}
        onClick={() => {
          void navigator.clipboard?.writeText(content).catch(() => undefined);
          useToasts.getState().push({ kind: "info", title: t("reader.aiCopied") });
        }}
      >
        {t("common.copy")}
      </Button>
      {onNavigate ? (
        <Button
          size="sm"
          variant="ghost"
          icon={<FileText size={12} />}
          onClick={() => openAnswerInWriter(path, content, onNavigate)}
        >
          {t("reader.aiOpenInWriter")}
        </Button>
      ) : null}
    </div>
  );
}

/** Session-wide actions in ReaderAiPanel: export transcript to Writer, copy Markdown, clear. */
export function ReaderAiHeaderActions({
  path,
  entries,
  running,
  onClear,
  onNavigate,
}: {
  path: string;
  entries: readonly ReaderConversationEntry[];
  running: boolean;
  onClear: () => void;
  onNavigate?: Navigate;
}) {
  const t = useT();
  const hasContent = entries.some((entry) => entry.content.trim().length > 0);
  if (entries.length === 0) return null;

  return (
    <div className="flex items-center gap-1 ml-auto">
      {hasContent ? (
        <IconButton
          label={t("reader.aiCopyTranscript")}
          disabled={running}
          onClick={() => {
            const md = formatConversationMarkdown(path, entries);
            void navigator.clipboard?.writeText(md).catch(() => undefined);
            useToasts.getState().push({ kind: "info", title: t("reader.aiTranscriptCopied") });
          }}
        >
          <Copy size={13} />
        </IconButton>
      ) : null}
      {hasContent && onNavigate ? (
        <IconButton
          label={t("reader.aiExportAll")}
          disabled={running}
          onClick={() => openConversationInWriter(path, entries, onNavigate)}
        >
          <FileText size={13} />
        </IconButton>
      ) : null}
      <Button size="sm" variant="ghost" icon={<Trash2 size={13} />} disabled={running} onClick={onClear}>
        {t("reader.aiClear")}
      </Button>
    </div>
  );
}
