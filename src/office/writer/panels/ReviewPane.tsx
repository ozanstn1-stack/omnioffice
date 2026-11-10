import { useT } from "../../../lib/i18n";
import type { RevisionSummary } from "../revisions";
import { PaneHead, activateOnKey } from "./PaneHead";

/** Writer's tracked-changes pane: pick a revision, accept or reject it. */
export function ReviewPane({
  revisions,
  active,
  onSelect,
  onResolve,
  onClose,
}: {
  revisions: RevisionSummary[];
  active: string | null;
  onSelect: (id: string) => void;
  /** Accept (`true`) or reject (`false`) one revision. */
  onResolve: (id: string, accept: boolean) => void;
  onClose: () => void;
}) {
  const t = useT();
  return (
    <div className="comments-sidebar writer-review-pane">
      <PaneHead title={t("writer.reviewPane")} count={revisions.length} onClose={onClose} />
      {revisions.length === 0 ? <p className="muted">{t("writer.noRevisions")}</p> : null}
      {revisions.map((revision) => (
        <div
          key={revision.id}
          className={`comment-card${active === revision.id ? " is-active" : ""}`}
          role="button"
          tabIndex={0}
          aria-pressed={active === revision.id}
          onClick={() => onSelect(revision.id)}
          onKeyDown={activateOnKey(() => onSelect(revision.id))}
        >
          <div className="row">
            <strong>{t(`writer.revision_${revision.kind}`)}</strong>
            <span className="spacer" />
            <span className="muted">{revision.author}</span>
          </div>
          <p
            className={
              revision.kind === "delete" ? "writer-rev-delete" : revision.kind === "insert" ? "writer-rev-insert" : ""
            }
          >
            {revision.text || "…"}
          </p>
          <div className="row">
            {[true, false].map((accept) => (
              <button
                key={String(accept)}
                type="button"
                className="btn btn-soft"
                onClick={(event) => {
                  event.stopPropagation();
                  onResolve(revision.id, accept);
                }}
              >
                {t(accept ? "writer.accept" : "writer.reject")}
              </button>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}
