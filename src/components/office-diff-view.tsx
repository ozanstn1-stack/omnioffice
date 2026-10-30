/**
 * The viewer for a model-level office diff (C13). Presentation only: the
 * comparison itself is `office-diff.ts`, so this renders the summary, the
 * Writer paragraph/table changes (with word-level segments), the Calc cell
 * changes and the Impress slide changes.
 */
import { Badge, Card } from "./ui";
import { useT } from "../lib/i18n";
import type { CalcDiff, DiffKind, DiffSegment, ImpressDiff, OfficeDiff, WriterDiff } from "../lib/office-diff";

function tone(kind: DiffKind): "ok" | "danger" | "warn" {
  if (kind === "added") return "ok";
  if (kind === "removed") return "danger";
  return "warn";
}

function KindBadge({ kind }: { kind: DiffKind }) {
  const t = useT();
  return <Badge tone={tone(kind)}>{t(`diff.kind.${kind}`)}</Badge>;
}

function SegmentText({ segments }: { segments?: DiffSegment[] }) {
  if (!segments) return null;
  return (
    <>
      {segments.map((segment, index) => {
        if (segment.kind === "same") return <span key={index}>{segment.text}</span>;
        const style =
          segment.kind === "added"
            ? { background: "var(--ok-soft, rgba(34,197,94,.18))", textDecoration: "none" }
            : { background: "var(--danger-soft, rgba(239,68,68,.16))", textDecoration: "line-through" };
        return (
          <span key={index} style={style}>
            {segment.text}
          </span>
        );
      })}
    </>
  );
}

function Summary({ diff }: { diff: OfficeDiff }) {
  const t = useT();
  return (
    <div className="row" style={{ flexWrap: "wrap" }}>
      <Badge tone="ok">{t("diff.countAdded", { count: diff.summary.added })}</Badge>
      <Badge tone="danger">{t("diff.countRemoved", { count: diff.summary.removed })}</Badge>
      <Badge tone="warn">{t("diff.countChanged", { count: diff.summary.changed })}</Badge>
      {diff.truncated ? <Badge tone="warn">{t("diff.truncated")}</Badge> : null}
    </div>
  );
}

function WriterView({ diff }: { diff: WriterDiff }) {
  const t = useT();
  return (
    <div className="stack">
      {diff.changes.map((change, index) => {
        const area = t(`diff.area.${change.area}`);
        return (
          <div key={index} className="stack" style={{ gap: 2 }}>
            <p className="muted small">
              {area} · {t("diff.paragraph", { index: change.blockIndex })}
              {change.cell ? ` · ${t("diff.tableCell", { row: change.cell.row, col: change.cell.col })}` : ""}
            </p>
            <div className="row" style={{ alignItems: "flex-start", gap: 8 }}>
              <KindBadge kind={change.kind} />
              <span className="small" style={{ wordBreak: "break-word" }}>
                <SegmentText segments={change.segments ?? [{ kind: "same", text: change.text }]} />
              </span>
            </div>
          </div>
        );
      })}
    </div>
  );
}

function CalcView({ diff }: { diff: CalcDiff }) {
  const t = useT();
  return (
    <div className="stack">
      {diff.sheetsAdded.length ? (
        <p className="small">
          <Badge tone="ok">{t("diff.sheetsAdded", { count: diff.sheetsAdded.length })}</Badge>{" "}
          {diff.sheetsAdded.join(", ")}
        </p>
      ) : null}
      {diff.sheetsRemoved.length ? (
        <p className="small">
          <Badge tone="danger">{t("diff.sheetsRemoved", { count: diff.sheetsRemoved.length })}</Badge>{" "}
          {diff.sheetsRemoved.join(", ")}
        </p>
      ) : null}
      <div className="stack">
        {diff.cells.map((cell, index) => (
          <div key={index} className="row" style={{ alignItems: "baseline", gap: 8, flexWrap: "wrap" }}>
            <KindBadge kind={cell.kind} />
            <strong className="small">
              {cell.sheet}!{cell.address}
            </strong>
            {cell.before ? (
              <span className="muted small" style={{ textDecoration: "line-through" }}>
                {cell.before}
              </span>
            ) : null}
            {cell.after ? <span className="small">{cell.after}</span> : null}
          </div>
        ))}
      </div>
    </div>
  );
}

function ImpressView({ diff }: { diff: ImpressDiff }) {
  const t = useT();
  return (
    <div className="stack">
      {diff.slides.map((slide, index) => (
        <div key={index} className="stack" style={{ gap: 2 }}>
          <div className="row">
            <KindBadge kind={slide.kind} />
            <span className="small">
              {t("diff.slide", { index: slide.index })} · <strong>{slide.title}</strong>
            </span>
          </div>
          <p className="muted small" style={{ wordBreak: "break-word" }}>
            <SegmentText segments={slide.segments ?? [{ kind: "same", text: slide.after || slide.before }]} />
          </p>
        </div>
      ))}
    </div>
  );
}

/**
 * One comparison result. `leftLabel`/`rightLabel` explain which side is which
 * (a saved version and the current document, or two picked files).
 */
export function OfficeDiffView({
  diff,
  leftLabel,
  rightLabel,
}: {
  diff: OfficeDiff;
  leftLabel?: string;
  rightLabel?: string;
}) {
  const t = useT();
  const empty =
    diff.kind === "writer"
      ? diff.changes.length === 0
      : diff.kind === "calc"
        ? diff.cells.length === 0 && diff.sheetsAdded.length === 0 && diff.sheetsRemoved.length === 0
        : diff.slides.length === 0;
  return (
    <Card>
      <div className="row" style={{ justifyContent: "space-between", flexWrap: "wrap", gap: 8 }}>
        <strong>{t("diff.title")}</strong>
        {leftLabel || rightLabel ? (
          <span className="muted small">
            {leftLabel ?? t("diff.left")} → {rightLabel ?? t("diff.right")}
          </span>
        ) : null}
      </div>
      {empty ? (
        <p className="muted small mt-2">{t("diff.unchanged")}</p>
      ) : (
        <div className="mt-2">
          <Summary diff={diff} />
        </div>
      )}
      <div className="stack mt-3">
        {empty ? null : diff.kind === "writer" ? (
          <WriterView diff={diff} />
        ) : diff.kind === "calc" ? (
          <CalcView diff={diff} />
        ) : (
          <ImpressView diff={diff} />
        )}
      </div>
    </Card>
  );
}
