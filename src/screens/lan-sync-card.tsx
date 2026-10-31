/**
 * The LAN sync card on the Sync screen (C14).
 *
 * Two roles share one card: a device can share a folder (host, and shows the
 * six-digit code) and it can pair with another device (client, and compares or
 * moves documents). Everything network-facing is a command; the card keeps the
 * picked folder token, the plan and the user's choices. A conflict is never
 * auto-resolved: the row shows three buttons and nothing happens until one is
 * pressed.
 */
import { useCallback, useEffect, useState } from "react";
import { Copy, FolderOpen, Loader2, Radio, RefreshCw, Share2, ShieldCheck, Unlink } from "lucide-react";
import { Badge, Button, Card } from "../components/ui";
import { LanSyncPlan } from "./lan-sync-plan";
import { pickDirectoryToken, type FileToken } from "../lib/file-tokens";
import { useT } from "../lib/i18n";
import {
  lanApply,
  lanDiscover,
  lanHostOpenPairing,
  lanHostStart,
  lanHostStop,
  lanHostUnpair,
  lanPair,
  lanPlan,
  lanResolveDeletion,
  lanStatus,
  lanSyncSafe,
  lanTrashList,
  lanTrashPurge,
  lanTrashRestore,
  lanUnpair,
  type LanDeviceView,
  type LanPlanView,
  type LanStatusView,
  type LanTrashEntryView,
} from "../lib/lan-sync";

export function LanSyncCard() {
  const t = useT();
  const [status, setStatus] = useState<LanStatusView | null>(null);
  const [folder, setFolder] = useState<FileToken | null>(null);
  const [devices, setDevices] = useState<LanDeviceView[]>([]);
  const [plan, setPlan] = useState<LanPlanView | null>(null);
  const [trash, setTrash] = useState<LanTrashEntryView[]>([]);
  const [pairTarget, setPairTarget] = useState<LanDeviceView | null>(null);
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const [next, entries] = await Promise.all([lanStatus(), lanTrashList()]);
      setStatus(next);
      setTrash(entries);
      setError(null);
    } catch (reason) {
      setError(String(reason));
    }
  }, []);

  useEffect(() => {
    let live = true;
    Promise.all([lanStatus(), lanTrashList()])
      .then(([next, entries]) => {
        if (!live) return;
        setStatus(next);
        setTrash(entries);
        setError(null);
      })
      .catch((reason: unknown) => {
        if (live) setError(String(reason));
      });
    return () => {
      live = false;
    };
  }, []);

  const run = async (work: () => Promise<void>) => {
    setBusy(true);
    try {
      await work();
      setError(null);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  };

  const pickFolder = () =>
    run(async () => {
      const picked = await pickDirectoryToken();
      if (picked) setFolder(picked);
    });

  const startHosting = () =>
    run(async () => {
      if (!folder) return;
      setStatus(await lanHostStart(folder.token));
    });

  const openPairing = () =>
    run(async () => {
      setStatus(await lanHostOpenPairing());
    });

  const stopHosting = () =>
    run(async () => {
      setStatus(await lanHostStop());
    });

  const discover = () =>
    run(async () => {
      setDevices(await lanDiscover());
    });

  const pair = () =>
    run(async () => {
      if (!pairTarget) return;
      setStatus(await lanPair(pairTarget.address, code));
      setPairTarget(null);
      setCode("");
      setDevices([]);
    });

  const compare = (deviceId: string) =>
    run(async () => {
      if (!folder) return;
      setPlan(await lanPlan(deviceId, folder.token));
    });

  const syncSafe = (deviceId: string) =>
    run(async () => {
      if (!folder) return;
      await lanSyncSafe(deviceId, folder.token);
      setPlan(await lanPlan(deviceId, folder.token));
      setTrash(await lanTrashList());
    });

  const resolveDeletion = (
    deviceId: string,
    path: string,
    side: "local" | "remote",
    resolution: "delete_remote" | "restore_local" | "delete_local" | "restore_remote",
  ) =>
    run(async () => {
      if (!folder) return;
      await lanResolveDeletion(deviceId, folder.token, path, side, resolution);
      setPlan(await lanPlan(deviceId, folder.token));
      setTrash(await lanTrashList());
    });

  const resolve = (deviceId: string, path: string, resolution: "keep_local" | "keep_cloud" | "keep_both") =>
    run(async () => {
      if (!folder) return;
      await lanApply(deviceId, folder.token, path, resolution);
      setPlan(await lanPlan(deviceId, folder.token));
      setTrash(await lanTrashList());
    });

  const restoreTrash = (id: string) =>
    run(async () => {
      if (!folder) return;
      setStatus(await lanTrashRestore(folder.token, id));
      setTrash(await lanTrashList());
    });

  const purgeTrash = () =>
    run(async () => {
      await lanTrashPurge();
      setTrash(await lanTrashList());
    });

  const copyCode = () => {
    const hostCode = status?.host?.code;
    if (hostCode) void navigator.clipboard?.writeText(hostCode);
  };

  const host = status?.host ?? null;
  const hostPeers = (status?.peers ?? []).filter((peer) => peer.hostSide);
  const clientPeers = (status?.peers ?? []).filter((peer) => !peer.hostSide);

  return (
    <Card>
      <div className="flex items-center justify-between gap-2 flex-wrap">
        <div className="flex items-center gap-2">
          <Radio size={16} style={{ color: "var(--accent)" }} />
          <strong>{t("lan.title")}</strong>
          <Badge tone="ok">
            <ShieldCheck size={11} /> {t("lan.badge")}
          </Badge>
        </div>
        <Button size="sm" variant="ghost" disabled={busy} onClick={() => void refresh()}>
          <RefreshCw size={13} /> {t("lan.refresh")}
        </Button>
      </div>
      <p className="text-sm muted mt-2">{t("lan.subtitle")}</p>
      {status ? (
        <p className="text-xs muted mt-1">
          {t("lan.device", { name: status.identity.name, fingerprint: status.identity.fingerprint.slice(0, 12) })}
          {status.encrypted ? ` · ${t("lan.encrypted")}` : ` · ${t("lan.plainWarning")}`}
        </p>
      ) : null}
      {error ? (
        <p className="text-xs text-[var(--danger)] mt-2" role="alert">
          {error}
        </p>
      ) : null}

      <div className="grid gap-4 md:grid-cols-2 mt-4">
        <div className="flex flex-col gap-2 p-3 rounded bg-surface/50 border border-border/40">
          <div className="flex items-center gap-2">
            <Share2 size={14} /> <strong className="text-sm">{t("lan.hostTitle")}</strong>
          </div>
          <p className="text-xs muted">{t("lan.hostNote")}</p>
          {host ? (
            <>
              <div className="flex items-center gap-2 flex-wrap text-sm">
                <span className="muted">{t("lan.port")}:</span> <span>{host.port}</span>
                <span className="muted">{t("lan.code")}:</span>
                <strong className="font-mono tracking-widest text-lg">{host.code ?? t("lan.codeClosed")}</strong>
                {host.code ? (
                  <Button size="sm" variant="ghost" aria-label={t("lan.copyCode")} onClick={copyCode}>
                    <Copy size={13} />
                  </Button>
                ) : null}
              </div>
              <p className="text-xs muted">
                {t("lan.codeHint", { seconds: host.expiresInSecs, attempts: host.attemptsLeft })}
              </p>
              <div className="flex items-center gap-2 flex-wrap">
                <Button size="sm" disabled={busy} onClick={() => void openPairing()}>
                  {t("lan.newCode")}
                </Button>
                <Button size="sm" variant="ghost" disabled={busy} onClick={() => void stopHosting()}>
                  {t("lan.stopSharing")}
                </Button>
              </div>
              <p className="text-xs muted">
                {t("lan.folderLabel")}: {folder?.name ?? t("lan.folderUnknown")}
              </p>
              {hostPeers.length ? (
                <ul className="flex flex-col gap-1 text-sm">
                  {hostPeers.map((peer) => (
                    <li key={peer.deviceId} className="flex items-center justify-between gap-2">
                      <span className="truncate">{peer.name}</span>
                      <Button
                        size="sm"
                        variant="ghost"
                        disabled={busy}
                        onClick={() => run(async () => setStatus(await lanHostUnpair(peer.deviceId)))}
                      >
                        <Unlink size={12} /> {t("lan.unpair")}
                      </Button>
                    </li>
                  ))}
                </ul>
              ) : (
                <p className="text-xs muted">{t("lan.noHostPeers")}</p>
              )}
            </>
          ) : (
            <>
              <div className="flex items-center gap-2 flex-wrap">
                <Button size="sm" variant="ghost" disabled={busy} onClick={() => void pickFolder()}>
                  <FolderOpen size={13} /> {t("lan.pickFolder")}
                </Button>
                <Button size="sm" disabled={busy || !folder} onClick={() => void startHosting()}>
                  {busy ? <Loader2 size={13} className="animate-spin" /> : null} {t("lan.startSharing")}
                </Button>
              </div>
              {folder ? (
                <p className="text-xs muted">{folder.name}</p>
              ) : (
                <p className="text-xs muted">{t("lan.pickFolderHint")}</p>
              )}
            </>
          )}
        </div>

        <div className="flex flex-col gap-2 p-3 rounded bg-surface/50 border border-border/40">
          <div className="flex items-center gap-2">
            <Radio size={14} /> <strong className="text-sm">{t("lan.clientTitle")}</strong>
          </div>
          <p className="text-xs muted">{t("lan.clientNote")}</p>
          <div className="flex items-center gap-2 flex-wrap">
            <Button size="sm" variant="ghost" disabled={busy} onClick={() => void discover()}>
              {busy ? <Loader2 size={13} className="animate-spin" /> : null} {t("lan.discover")}
            </Button>
            <Button size="sm" variant="ghost" disabled={busy} onClick={() => void pickFolder()}>
              <FolderOpen size={13} /> {t("lan.localFolder")}
            </Button>
          </div>
          <p className="text-xs muted">{folder ? folder.name : t("lan.pickFolderHint")}</p>
          {devices.length ? (
            <ul className="flex flex-col gap-1 text-sm">
              {devices.map((device) => (
                <li key={device.deviceId} className="flex items-center justify-between gap-2">
                  <span className="truncate">
                    {device.name} <span className="muted text-xs">{device.address}</span>
                  </span>
                  <Button size="sm" disabled={busy} onClick={() => setPairTarget(device)}>
                    {t("lan.pair")}
                  </Button>
                </li>
              ))}
            </ul>
          ) : (
            <p className="text-xs muted">{t("lan.noneFound")}</p>
          )}
          {pairTarget ? (
            <div className="flex flex-col gap-2 p-2 rounded border border-border/40">
              <p className="text-xs muted">{t("lan.pairHint", { name: pairTarget.name })}</p>
              <input
                className="input"
                inputMode="numeric"
                maxLength={6}
                placeholder="000000"
                aria-label={t("lan.pairCode")}
                value={code}
                onChange={(event) => setCode(event.target.value.replace(/\D/g, "").slice(0, 6))}
              />
              <div className="flex items-center gap-2">
                <Button size="sm" disabled={busy || code.length !== 6} onClick={() => void pair()}>
                  {t("lan.pairAction")}
                </Button>
                <Button size="sm" variant="ghost" onClick={() => setPairTarget(null)}>
                  {t("common.cancel")}
                </Button>
              </div>
            </div>
          ) : null}
          {clientPeers.length ? (
            <ul className="flex flex-col gap-2">
              {clientPeers.map((peer) => (
                <li key={peer.deviceId} className="flex items-center justify-between gap-2 text-sm">
                  <span className="truncate">{peer.name}</span>
                  <div className="flex items-center gap-1">
                    <Button size="sm" disabled={busy || !folder} onClick={() => void compare(peer.deviceId)}>
                      {t("lan.compare")}
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      disabled={busy}
                      onClick={() => run(async () => setStatus(await lanUnpair(peer.deviceId)))}
                    >
                      <Unlink size={12} /> {t("lan.unpair")}
                    </Button>
                  </div>
                </li>
              ))}
            </ul>
          ) : (
            <p className="text-xs muted">{t("lan.noClientPeers")}</p>
          )}
        </div>
      </div>

      {plan ? (
        <LanSyncPlan
          plan={plan}
          busy={busy}
          onResolve={resolve}
          onSyncSafe={syncSafe}
          onResolveDeletion={resolveDeletion}
        />
      ) : null}
      <div className="flex flex-col gap-2 mt-4">
        <div className="flex items-center justify-between gap-2 flex-wrap">
          <strong className="text-sm">{t("lan.trashTitle")}</strong>
          <Button size="sm" variant="ghost" disabled={busy} onClick={() => void purgeTrash()}>
            {t("lan.trashPurge")}
          </Button>
        </div>
        <p className="text-xs muted">{t("lan.trashNote")}</p>
        {trash.length ? (
          <ul className="flex flex-col gap-1 text-sm">
            {trash.map((entry) => (
              <li
                key={entry.id}
                className="flex items-center justify-between gap-2 p-1.5 rounded bg-surface/50 border border-border/40"
              >
                <span className="truncate">{entry.path}</span>
                <div className="flex items-center gap-1 shrink-0">
                  <Badge tone="warn">{t(entry.side === "local" ? "lan.deletionLocal" : "lan.deletionRemote")}</Badge>
                  <span className="text-xs muted">{entry.peerName}</span>
                  <Button size="sm" disabled={busy || !folder} onClick={() => void restoreTrash(entry.id)}>
                    {t("lan.trashRestore")}
                  </Button>
                </div>
              </li>
            ))}
          </ul>
        ) : (
          <p className="text-xs muted">{t("lan.trashEmpty")}</p>
        )}
      </div>
      <p className="text-xs muted mt-3">{t("lan.note")}</p>
    </Card>
  );
}
