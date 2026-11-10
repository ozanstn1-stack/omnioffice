import { CheckCircle2 } from "lucide-react";
import { useT } from "../../../lib/i18n";
import type { WordSpellError } from "../spell";
import { PaneHead, activateOnKey } from "./PaneHead";

/** Writer's spelling pane: misspelled words, where they are and suggestions. */
export function SpellPane({
  errors,
  loading,
  onRecheck,
  onJump,
  onApply,
  onClose,
}: {
  errors: WordSpellError[];
  loading: boolean;
  onRecheck: () => void;
  onJump: (blockIndex: number) => void;
  onApply: (error: WordSpellError, suggestion: string) => void;
  onClose: () => void;
}) {
  const t = useT();
  return (
    <div className="comments-sidebar writer-review-pane">
      <PaneHead title={t("writer.spellCheck")} count={errors.length} onRecheck={onRecheck} onClose={onClose} />
      {loading ? (
        <p className="muted">{t("common.processing")}</p>
      ) : errors.length === 0 ? (
        <p className="muted row" style={{ gap: 6, alignItems: "center" }}>
          <CheckCircle2 size={14} /> {t("writer.noSpellErrors")}
        </p>
      ) : (
        errors.map((err, idx) => (
          <div
            key={`${err.blockIndex}-${err.start}-${idx}`}
            className="comment-card"
            onClick={() => onJump(err.blockIndex)}
            role="button"
            tabIndex={0}
            onKeyDown={activateOnKey(() => onJump(err.blockIndex))}
          >
            <div className="row">
              <strong style={{ color: "var(--danger, #dc2626)" }}>{err.word}</strong>
              <span className="spacer" />
              <span className="muted">§{err.blockIndex + 1}</span>
            </div>
            {err.suggestions.length > 0 ? (
              <div className="row" style={{ flexWrap: "wrap", gap: 4, marginTop: 6 }}>
                {err.suggestions.map((sug) => (
                  <button
                    key={sug}
                    type="button"
                    className="btn btn-soft"
                    style={{ padding: "2px 8px", fontSize: 12 }}
                    onClick={(event) => {
                      event.stopPropagation();
                      onApply(err, sug);
                    }}
                  >
                    {sug}
                  </button>
                ))}
              </div>
            ) : (
              <p className="muted" style={{ fontSize: 12 }}>
                {t("writer.noSuggestions")}
              </p>
            )}
          </div>
        ))
      )}
    </div>
  );
}
