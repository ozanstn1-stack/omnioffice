/**
 * Background job center: every long operation (PDF, OCR, AI, vault scan)
 * reports here with progress, cancellation and retry.
 */
import { Ban, CheckCircle2, ListChecks, RefreshCw, XCircle } from "lucide-react";
import { useJobs as useBackgroundJobs } from "../lib/jobs";
import { useT } from "../lib/i18n";
import { Badge, Card, EmptyState } from "../components/ui";

export function JobsScreen() {
  const t = useT();
  const jobs = useBackgroundJobs((state) => state.jobs);
  const cancel = useBackgroundJobs((state) => state.cancel);
  const retry = useBackgroundJobs((state) => state.retry);
  const clearFinished = useBackgroundJobs((state) => state.clearFinished);

  return (
    <div className="screen">
      <div className="screen-head">
        <div>
          <h1>
            <ListChecks size={18} /> {t("jobs.title")}
          </h1>
          <p className="muted">{t("jobs.subtitle")}</p>
        </div>
        <button type="button" className="btn btn-soft" onClick={clearFinished}>
          {t("jobs.clearFinished")}
        </button>
      </div>

      {jobs.length === 0 ? <EmptyState title={t("jobs.empty")} hint={t("jobs.emptyHint")} /> : null}

      <div className="stack">
        {jobs.map((job) => {
          const percent = job.total > 0 ? Math.min(100, Math.round((job.current / job.total) * 100)) : 0;
          return (
            <Card key={job.id}>
              <div className="row">
                <strong>{job.title}</strong>
                <Badge tone={job.status === "failed" ? "danger" : job.status === "succeeded" ? "ok" : job.status === "cancelled" ? "warn" : "accent"}>
                  {t(`jobs.status.${job.status}`)}
                </Badge>
                <span className="spacer" />
                <span className="muted small">{job.kind.toUpperCase()}</span>
              </div>
              {job.status === "running" ? (
                <>
                  <div className="progress-track" style={{ marginTop: 8 }}>
                    <div className="progress-fill" style={{ width: `${percent}%` }} />
                  </div>
                  <p className="muted small">
                    {job.stage} {job.total > 0 ? `${job.current}/${job.total}` : ""} {job.message ?? ""}
                  </p>
                </>
              ) : null}
              {job.error ? <p className="muted small">{job.error}</p> : null}
              <div className="row" style={{ marginTop: 8 }}>
                {job.status === "running" ? (
                  <button type="button" className="btn btn-soft" onClick={() => cancel(job.id)}>
                    <Ban size={13} /> {t("common.cancel")}
                  </button>
                ) : null}
                {job.status === "failed" || job.status === "cancelled" ? (
                  <button type="button" className="btn btn-soft" onClick={() => retry(job.id)}>
                    <RefreshCw size={13} /> {t("jobs.retry")}
                  </button>
                ) : null}
                {job.status === "succeeded" ? (
                  <span className="muted small">
                    <CheckCircle2 size={13} /> {t("jobs.done")}
                  </span>
                ) : null}
                {job.status === "failed" ? (
                  <span className="muted small">
                    <XCircle size={13} /> {t("jobs.failed")}
                  </span>
                ) : null}
              </div>
            </Card>
          );
        })}
      </div>
    </div>
  );
}
