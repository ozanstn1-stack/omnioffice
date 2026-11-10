import { useJobs } from "../lib/jobs";
import { useT } from "../lib/i18n";
import { useSettings } from "../lib/store";
import type { Navigate } from "../lib/nav";

/**
 * Home's side card for work running in the background (7.32): the first
 * running job with its progress, and how many more are queued. Nothing is
 * shown while nothing runs.
 */
export function JobsGlance({ onNavigate }: { onNavigate: Navigate }) {
  const t = useT();
  const language = useSettings((state) => state.settings.language);
  // Select the stable array and filter here: a filtering selector returns a
  // new array on every store read and re-renders without end.
  const jobs = useJobs((state) => state.jobs);
  const running = jobs.filter((job) => job.status === "running");
  const first = running[0];
  if (!first) return null;
  const percent = first.total > 0 ? Math.min(100, Math.round((first.current / first.total) * 100)) : null;
  return (
    <button type="button" className="jobs-glance" onClick={() => onNavigate("jobs")}>
      <span className="jobs-glance-head">
        <span className="truncate font-semibold">{first.title}</span>
        {percent !== null ? (
          <span className="muted">{new Intl.NumberFormat(language, { style: "percent" }).format(percent / 100)}</span>
        ) : null}
      </span>
      <span
        className="progress-track"
        role="progressbar"
        aria-label={first.title}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={percent ?? undefined}
      >
        <span className="progress-fill" style={{ width: `${percent ?? 35}%` }} />
      </span>
      <span className="text-xs muted">{t("jobs.runningCount", { count: running.length })}</span>
    </button>
  );
}
