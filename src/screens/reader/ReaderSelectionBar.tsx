import { useCallback, useEffect, useState, type RefObject } from "react";
import { Copy, FileText, PenLine, Sparkles } from "lucide-react";
import { useT } from "../../lib/i18n";
import type { Navigate } from "../../lib/nav";
import { useSettings, useToasts } from "../../lib/store";
import { formatSelectionPrompt, openExcerptInWriter } from "./reader-ai-export";

export interface ContainerSelectionInfo {
  text: string;
  top: number;
  left: number;
}

/** Reads the active non-collapsed DOM text selection if it starts/lives inside `container`. */
export function readContainerSelection(container: HTMLElement | null): ContainerSelectionInfo | null {
  if (!container || typeof window === "undefined" || typeof window.getSelection !== "function") return null;
  const selection = window.getSelection();
  if (!selection || selection.rangeCount === 0 || selection.isCollapsed) return null;
  const range = selection.getRangeAt(0);
  if (!container.contains(range.commonAncestorContainer)) return null;
  const text = selection.toString().replace(/\s+/g, " ").trim();
  if (text.length < 2) return null;

  const rect = typeof range.getBoundingClientRect === "function" ? range.getBoundingClientRect() : null;
  const fallback =
    typeof container.getBoundingClientRect === "function"
      ? container.getBoundingClientRect()
      : { top: 60, left: 200, width: 400 };
  const anchorTop = rect && (rect.width > 0 || rect.height > 0) ? rect.top : fallback.top + 60;
  const anchorLeft =
    rect && (rect.width > 0 || rect.height > 0) ? rect.left + rect.width / 2 : fallback.left + fallback.width / 2;
  const maxWidth = typeof window !== "undefined" ? Math.max(220, window.innerWidth - 150) : 800;

  return {
    text,
    top: Math.max(12, Math.round(anchorTop - 44)),
    left: Math.max(150, Math.min(Math.round(anchorLeft), maxWidth)),
  };
}

/** Floating action bar displayed over selected text in the PDF Reader. */
export function ReaderSelectionBar({
  containerRef,
  path,
  page,
  onAskAi,
  onNavigate,
}: {
  containerRef: RefObject<HTMLDivElement | null>;
  path: string;
  page: number;
  onAskAi: (prompt: string) => void;
  onNavigate?: Navigate;
}) {
  const t = useT();
  const language = useSettings((s) => s.settings.language);
  const [selectionInfo, setSelectionInfo] = useState<ContainerSelectionInfo | null>(null);

  const syncSelection = useCallback(() => {
    setSelectionInfo(readContainerSelection(containerRef.current));
  }, [containerRef]);

  const clearSelection = useCallback(() => {
    setSelectionInfo(null);
    if (typeof window !== "undefined" && typeof window.getSelection === "function") {
      window.getSelection()?.removeAllRanges();
    }
  }, []);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setSelectionInfo(null);
    };
    document.addEventListener("selectionchange", syncSelection);
    document.addEventListener("mouseup", syncSelection);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("selectionchange", syncSelection);
      document.removeEventListener("mouseup", syncSelection);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [syncSelection]);

  if (!selectionInfo) return null;

  return (
    <div
      role="toolbar"
      aria-label={t("reader.selectionToolbar")}
      className="reader-selection-bar fade-in"
      style={{ top: selectionInfo.top, left: selectionInfo.left }}
      onMouseDown={(event) => event.preventDefault()}
    >
      <button
        type="button"
        className="reader-selection-btn"
        onClick={() => {
          void navigator.clipboard?.writeText(selectionInfo.text).catch(() => undefined);
          useToasts.getState().push({ kind: "info", title: t("reader.selectionCopied") });
          clearSelection();
        }}
      >
        <Copy size={12} aria-hidden />
        <span>{t("common.copy")}</span>
      </button>

      <button
        type="button"
        className="reader-selection-btn"
        onClick={() => {
          onAskAi(formatSelectionPrompt(selectionInfo.text, page, language));
          clearSelection();
        }}
      >
        <Sparkles size={12} aria-hidden />
        <span>{t("reader.selectionAskAi")}</span>
      </button>

      {onNavigate ? (
        <button
          type="button"
          className="reader-selection-btn"
          onClick={() => {
            onNavigate("annotate", { files: [path] });
            clearSelection();
          }}
        >
          <PenLine size={12} aria-hidden />
          <span>{t("reader.openInAnnotate")}</span>
        </button>
      ) : null}

      {onNavigate ? (
        <button
          type="button"
          className="reader-selection-btn"
          onClick={() => {
            openExcerptInWriter(path, selectionInfo.text, page, onNavigate);
            clearSelection();
          }}
        >
          <FileText size={12} aria-hidden />
          <span>{t("reader.selectionToWriter")}</span>
        </button>
      ) : null}
    </div>
  );
}

export default ReaderSelectionBar;
