import type { ReactNode } from "react";
import { RefreshCw, X } from "lucide-react";
import { useT } from "../../../lib/i18n";

/** The title row every Writer side pane shares: title, count, re-check, close. */
export function PaneHead({
  title,
  count,
  onRecheck,
  onClose,
}: {
  title: ReactNode;
  count?: number;
  onRecheck?: () => void;
  onClose: () => void;
}) {
  const t = useT();
  return (
    <div className="comments-head">
      <strong>{title}</strong>
      {count === undefined ? null : (
        <>
          <span className="spacer" />
          <span className="muted">{count}</span>
        </>
      )}
      {onRecheck ? (
        <button type="button" className="icon-btn" onClick={onRecheck} title={t("writer.recheck")}>
          <RefreshCw size={14} />
        </button>
      ) : null}
      <button type="button" className="icon-btn" onClick={onClose} aria-label={t("common.close")}>
        <X size={14} />
      </button>
    </div>
  );
}

/** Enter or Space on a card acts like a click, as on a button. */
export function activateOnKey(action: () => void) {
  return (event: { key: string; preventDefault: () => void }) => {
    if (event.key !== "Enter" && event.key !== " ") return;
    event.preventDefault();
    action();
  };
}
