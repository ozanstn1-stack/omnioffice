import { Copy, FileText } from "lucide-react";
import { Button } from "../../components/ui";
import { useT } from "../../lib/i18n";
import type { Navigate } from "../../lib/nav";
import { useToasts } from "../../lib/store";
import { openAnswerInWriter } from "./reader-ai-export";

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
