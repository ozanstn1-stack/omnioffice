import { AlertTriangle, CheckCircle2 } from "lucide-react";
import { useT } from "../../../lib/i18n";
import type { LetterViolation } from "../../../lib/official-letter";
import { PaneHead, activateOnKey } from "./PaneHead";

/** Writer's official-letter check (Resmî Yazışma Yönetmeliği) pane. */
export function RegulationPane({
  violations,
  onRecheck,
  onJump,
  onFix,
  onFixAll,
  onClose,
}: {
  violations: LetterViolation[];
  onRecheck: () => void;
  onJump: (blockIndex: number) => void;
  onFix: (id: string) => void;
  onFixAll: () => void;
  onClose: () => void;
}) {
  const t = useT();
  return (
    <div className="comments-sidebar writer-review-pane">
      <PaneHead
        title={t("writer.officialLetterCheck")}
        count={violations.length}
        onRecheck={onRecheck}
        onClose={onClose}
      />
      {violations.some((v) => v.canAutoFix) ? (
        <button type="button" className="btn btn-primary" style={{ width: "100%", marginBottom: 8 }} onClick={onFixAll}>
          {t("writer.fixAllRegulation")}
        </button>
      ) : null}
      {violations.length === 0 ? (
        <p className="muted row" style={{ gap: 6, alignItems: "center" }}>
          <CheckCircle2 size={14} /> {t("writer.regulationCompliant")}
        </p>
      ) : (
        violations.map((v) => {
          const jump = () => {
            if (v.blockIndex !== undefined) onJump(v.blockIndex);
          };
          return (
            <div
              key={v.id}
              className="comment-card"
              role="button"
              tabIndex={0}
              onClick={jump}
              onKeyDown={(event) => {
                if (v.blockIndex !== undefined) activateOnKey(jump)(event);
              }}
            >
              <div className="row" style={{ gap: 6, alignItems: "center" }}>
                <AlertTriangle size={14} />
                <strong>{v.ruleTitle}</strong>
                <span className="spacer" />
                <span className="muted" style={{ fontSize: 11 }}>
                  {v.regulationArticle}
                </span>
              </div>
              <p style={{ fontSize: 12, marginTop: 4 }}>{v.message}</p>
              {v.manualInstructions ? (
                <p className="muted" style={{ fontSize: 11, marginTop: 4 }}>
                  {v.manualInstructions}
                </p>
              ) : null}
              {v.canAutoFix ? (
                <div className="row" style={{ marginTop: 6 }}>
                  <button
                    type="button"
                    className="btn btn-soft"
                    style={{ padding: "2px 8px", fontSize: 12 }}
                    onClick={(event) => {
                      event.stopPropagation();
                      onFix(v.id);
                    }}
                  >
                    {t("writer.autoFix")}
                  </button>
                </div>
              ) : null}
            </div>
          );
        })
      )}
    </div>
  );
}
