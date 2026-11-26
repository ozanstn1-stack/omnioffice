import { useEffect, useState } from "react";
import { Trash2 } from "lucide-react";
import { Button, Card } from "./ui";
import { aiClearData, clearOperations, loadOperations } from "../lib/api";
import { useT } from "../lib/i18n";
import { useJobs } from "../lib/jobs";
import { reportError, useRecent, useSettings, useToasts } from "../lib/store";
import { clearWorkbenchPrefs } from "../lib/workbench-store";

export function StorageCleanupCard() {
  const t = useT();
  const settings = useSettings((s) => s.settings);
  const updateSettings = useSettings((s) => s.update);
  const recentCount = useRecent((s) => s.entries.length);
  const clearRecent = useRecent((s) => s.clear);
  const jobs = useJobs((s) => s.jobs);
  const clearFinishedJobs = useJobs((s) => s.clearFinished);
  const pushToast = useToasts((s) => s.push);

  const [opsCount, setOpsCount] = useState(0);
  const [includeRecent, setIncludeRecent] = useState(false);
  const [includeAi, setIncludeAi] = useState(false);
  const [busy, setBusy] = useState(false);

  const finishedJobs = jobs.filter((job) => job.status !== "running" && job.status !== "queued").length;

  useEffect(() => {
    let cancelled = false;
    void loadOperations()
      .then((list) => {
        if (!cancelled) setOpsCount(Array.isArray(list) ? list.length : 0);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, []);

  const cleanNow = async () => {
    setBusy(true);
    try {
      let categories = 0;
      clearFinishedJobs();
      categories += 1;

      await clearOperations();
      setOpsCount(0);
      categories += 1;

      clearWorkbenchPrefs();
      categories += 1;

      if (includeRecent) {
        await clearRecent();
        categories += 1;
      }
      if (includeAi) {
        await aiClearData();
        categories += 1;
      }

      pushToast({
        kind: "success",
        title: t("settings.storageCleaned", { count: categories }),
      });
    } catch (error) {
      reportError(error, t);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card className="p-5 flex flex-col gap-3">
      <h3 className="font-semibold flex items-center gap-2">
        <Trash2 size={16} style={{ color: "var(--accent)" }} /> {t("settings.storageTitle")}
      </h3>
      <p className="text-xs muted">
        {t("settings.storageSummary", {
          jobs: finishedJobs,
          logs: opsCount,
          recent: recentCount,
        })}
      </p>
      <label className="checkbox">
        <input
          type="checkbox"
          checked={settings.autoCleanupTemp}
          onChange={(event) => void updateSettings({ autoCleanupTemp: event.target.checked })}
        />
        <span>{t("settings.autoCleanupTemp")}</span>
      </label>
      <label className="checkbox">
        <input type="checkbox" checked={includeRecent} onChange={(event) => setIncludeRecent(event.target.checked)} />
        <span>{t("settings.storageIncludeRecent")}</span>
      </label>
      <label className="checkbox">
        <input type="checkbox" checked={includeAi} onChange={(event) => setIncludeAi(event.target.checked)} />
        <span>{t("settings.storageIncludeAi")}</span>
      </label>
      <div>
        <Button size="sm" variant="ghost" icon={<Trash2 size={14} />} disabled={busy} onClick={() => void cleanNow()}>
          {t("settings.storageCleanBtn")}
        </Button>
      </div>
    </Card>
  );
}
