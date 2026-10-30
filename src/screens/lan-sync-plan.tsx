/**
 * The comparison list of the LAN sync card: one row per document, a state
 * badge and, for a conflict, the three explicit choices. No row is ever
 * changed without a click.
 */
import { Badge, Button } from "../components/ui";
import { useT } from "../lib/i18n";
import type { LanPlanView } from "../lib/lan-sync";

type DeletionResolution = "delete_remote" | "restore_local" | "delete_local" | "restore_remote";

const STATE_KEYS: Record<string, string> = {
  synced: "lan.stateSynced",
  local_only: "lan.stateLocalOnly",
  local_ahead: "lan.stateLocalAhead",
  cloud_ahead: "lan.stateCloudAhead",
  conflict: "lan.stateConflict",
};

const ACTION_KEYS: Record<string, string> = {
  upload: "lan.actionUpload",
  download: "lan.actionDownload",
  download_copy: "lan.actionDownloadCopy",
  nothing: "lan.actionNothing",
  conflict: "lan.actionConflict",
};

export function LanSyncPlan({
  plan,
  busy,
  onResolve,
  onSyncSafe,
  onResolveDeletion,
}: {
  plan: LanPlanView;
  busy: boolean;
  onResolve: (deviceId: string, path: string, resolution: "keep_local" | "keep_cloud" | "keep_both") => void;
  onSyncSafe: (deviceId: string) => void;
  onResolveDeletion: (deviceId: string, path: string, side: "local" | "remote", resolution: DeletionResolution) => void;
}) {
  const t = useT();
  return (
    <div className="flex flex-col gap-2 mt-4">
      <div className="flex items-center justify-between gap-2 flex-wrap">
        <strong className="text-sm">{t("lan.planTitle")}</strong>
        <Button size="sm" variant="ghost" disabled={busy} onClick={() => onSyncSafe(plan.deviceId)}>
          {t("lan.syncSafe")}
        </Button>
      </div>
      {plan.items.length === 0 ? <p className="text-xs muted">{t("lan.emptyPlan")}</p> : null}
      <ul className="flex flex-col gap-1 text-sm">
        {plan.items.map((item) => (
          <li
            key={item.path}
            className="flex items-center justify-between gap-2 p-1.5 rounded bg-surface/50 border border-border/40"
          >
            <span className="truncate">{item.path}</span>
            <div className="flex items-center gap-1 shrink-0">
              <Badge tone={item.state === "conflict" ? "warn" : "default"}>
                {t(STATE_KEYS[item.state] ?? "lan.stateSynced")}
              </Badge>
              {item.state === "conflict" ? (
                <>
                  <Button size="sm" disabled={busy} onClick={() => onResolve(plan.deviceId, item.path, "keep_both")}>
                    {t("lan.keepBoth")}
                  </Button>
                  <Button size="sm" disabled={busy} onClick={() => onResolve(plan.deviceId, item.path, "keep_local")}>
                    {t("lan.keepLocal")}
                  </Button>
                  <Button size="sm" disabled={busy} onClick={() => onResolve(plan.deviceId, item.path, "keep_cloud")}>
                    {t("lan.keepRemote")}
                  </Button>
                </>
              ) : (
                <span className="text-xs muted">{t(ACTION_KEYS[item.action ?? "nothing"] ?? "lan.actionNothing")}</span>
              )}
            </div>
          </li>
        ))}
      </ul>
      {plan.deletions.length ? (
        <div className="flex flex-col gap-1 mt-2">
          <strong className="text-sm">{t("lan.deletionsTitle")}</strong>
          <p className="text-xs muted">{t("lan.deletionHint")}</p>
          <ul className="flex flex-col gap-1 text-sm">
            {plan.deletions.map((deletion) => (
              <li
                key={deletion.path}
                className="flex items-center justify-between gap-2 p-1.5 rounded bg-surface/50 border border-border/40"
              >
                <span className="truncate">{deletion.path}</span>
                <div className="flex items-center gap-1 shrink-0">
                  <Badge tone="warn">{t(deletion.side === "local" ? "lan.deletionLocal" : "lan.deletionRemote")}</Badge>
                  {deletion.side === "local" ? (
                    <>
                      <Button
                        size="sm"
                        disabled={busy}
                        onClick={() => onResolveDeletion(plan.deviceId, deletion.path, "local", "delete_remote")}
                      >
                        {t("lan.deleteRemote")}
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        disabled={busy}
                        onClick={() => onResolveDeletion(plan.deviceId, deletion.path, "local", "restore_local")}
                      >
                        {t("lan.restoreLocal")}
                      </Button>
                    </>
                  ) : (
                    <>
                      <Button
                        size="sm"
                        disabled={busy}
                        onClick={() => onResolveDeletion(plan.deviceId, deletion.path, "remote", "delete_local")}
                      >
                        {t("lan.deleteLocal")}
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        disabled={busy}
                        onClick={() => onResolveDeletion(plan.deviceId, deletion.path, "remote", "restore_remote")}
                      >
                        {t("lan.restoreRemote")}
                      </Button>
                    </>
                  )}
                </div>
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </div>
  );
}
