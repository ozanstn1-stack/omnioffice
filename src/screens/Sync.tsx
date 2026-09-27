import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { appCacheDir, join } from "@tauri-apps/api/path";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  CheckCircle2,
  Cloud,
  CloudOff,
  Download,
  FileText,
  FolderOpen,
  GitMerge,
  Plus,
  RefreshCw,
  ShieldCheck,
  UploadCloud,
  X,
} from "lucide-react";
import { Badge, Button, Card, EmptyState, Field, IconButton, Spinner, TextInput, Toggle } from "../components/ui";
import { Screen, TwoColumn } from "../components/layout";
import { useT } from "../lib/i18n";
import { useRecent, useToasts } from "../lib/store";
import { toAppError } from "../lib/api";
import {
  syncCapabilities,
  syncDownload,
  syncForget,
  syncGetConfig,
  syncList,
  syncResolve,
  syncSaveConfig,
  syncStatus,
  syncTestConnection,
  syncUpload,
  type SyncCapabilities,
  type SyncConfigView,
  type SyncListEntry,
  type SyncProviderId,
  type SyncResolutionId,
  type SyncStateId,
  type SyncStatusView,
} from "../lib/api";
import { isOswkPath, needsResolution, shortHash, syncStateTone } from "../lib/sync";
import { formatBytes, formatDate } from "../lib/format";
import { isAndroid, saveFileOnAndroid } from "../lib/mobile";

/**
 * Cloud sync screen.
 *
 * Safety contract mirroring the backend:
 *  - the master switch is off by default and saving it transfers nothing;
 *  - every row action (check / upload / resolve / download) is a deliberate
 *    click; there is no polling and no automatic merge;
 *  - conflicts show all three resolutions and nothing is overwritten until
 *    one of them is chosen.
 */

interface Draft {
  enabled: boolean;
  provider: SyncProviderId;
  url: string;
  username: string;
  password: string;
  remoteDir: string;
}

type RowState =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "ready"; view: SyncStatusView }
  | { kind: "error"; message: string };

function draftFrom(config: SyncConfigView): Draft {
  return {
    enabled: config.enabled,
    provider: config.provider,
    url: config.url,
    username: config.username,
    password: "",
    remoteDir: config.remoteDir,
  };
}

/**
 * Sync errors are already written as user-facing sentences by the backend;
 * the generic `errors.*` localization would replace them with wrong text
 * ("This file type is not supported." for an OAuth refusal, for instance).
 */
function syncMessage(error: unknown, fallback: string): string {
  const appError = toAppError(error);
  return appError.message || fallback;
}

export function Sync() {
  const t = useT();
  const pushToast = useToasts((state) => state.push);
  const recent = useRecent((state) => state.entries);
  const refreshRecent = useRecent((state) => state.refresh);

  const [config, setConfig] = useState<SyncConfigView | null>(null);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [capabilities, setCapabilities] = useState<SyncCapabilities | null>(null);
  const [settingsError, setSettingsError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; message: string } | null>(null);

  const [added, setAdded] = useState<string[]>([]);
  const [rows, setRows] = useState<Record<string, RowState>>({});
  const [selected, setSelected] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const [remote, setRemote] = useState<SyncListEntry[] | null>(null);
  const [remoteError, setRemoteError] = useState<string | null>(null);
  const [loadingRemote, setLoadingRemote] = useState(false);

  useEffect(() => {
    void refreshRecent();
    void syncGetConfig()
      .then((next) => {
        setConfig(next);
        setDraft(draftFrom(next));
      })
      .catch((error) => setSettingsError(syncMessage(error, t("sync.errorTitle"))));
    void syncCapabilities()
      .then(setCapabilities)
      .catch(() => setCapabilities(null));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const enabled = Boolean(config?.enabled);

  // Local .oswk documents: recent files (most common flow) plus anything the
  // user added with the picker in this session. Capped so a long history
  // cannot turn status refreshing into a batch job.
  const tracked = useMemo(() => {
    const seen = new Set<string>();
    const list: string[] = [];
    for (const entry of recent) {
      if (!isOswkPath(entry.path) || seen.has(entry.path)) continue;
      seen.add(entry.path);
      list.push(entry.path);
      if (list.length >= 25) break;
    }
    for (const path of added) {
      if (seen.has(path)) continue;
      seen.add(path);
      list.push(path);
    }
    return list;
  }, [added, recent]);

  const setRow = useCallback((path: string, state: RowState) => {
    setRows((current) => ({ ...current, [path]: state }));
  }, []);

  const checkStatus = useCallback(
    async (path: string) => {
      setRow(path, { kind: "loading" });
      try {
        const view = await syncStatus(path);
        setRow(path, { kind: "ready", view });
        return view;
      } catch (error) {
        setRow(path, { kind: "error", message: syncMessage(error, t("sync.errorTitle")) });
        return null;
      }
    },
    [setRow, t],
  );

  const requireEnabled = useCallback((): boolean => {
    if (enabled) return true;
    pushToast({ kind: "error", title: t("sync.errorTitle"), detail: t("sync.setupFirst") });
    return false;
  }, [enabled, pushToast, t]);

  const runUpload = useCallback(
    async (path: string) => {
      if (!requireEnabled()) return;
      setBusy(path);
      try {
        const view = await syncUpload(path);
        setRow(path, { kind: "ready", view });
        setSelected(path);
        pushToast({ kind: "success", title: t("sync.upload"), detail: view.note ?? undefined });
      } catch (error) {
        const message = syncMessage(error, t("sync.errorTitle"));
        setRow(path, { kind: "error", message });
        pushToast({ kind: "error", title: t("sync.errorTitle"), detail: message });
      } finally {
        setBusy(null);
      }
    },
    [pushToast, requireEnabled, setRow, t],
  );

  const runResolve = useCallback(
    async (path: string, resolution: SyncResolutionId) => {
      if (!requireEnabled()) return;
      setBusy(path);
      try {
        const view = await syncResolve(path, resolution);
        setRow(path, { kind: "ready", view });
        pushToast({ kind: "success", title: t("sync.resolve"), detail: view.note ?? undefined });
      } catch (error) {
        const message = syncMessage(error, t("sync.errorTitle"));
        setRow(path, { kind: "error", message });
        pushToast({ kind: "error", title: t("sync.errorTitle"), detail: message });
      } finally {
        setBusy(null);
      }
    },
    [pushToast, requireEnabled, setRow, t],
  );

  const runForget = useCallback(
    async (path: string) => {
      try {
        await syncForget(path);
        setRows((current) => {
          const next = { ...current };
          delete next[path];
          return next;
        });
        setAdded((current) => current.filter((value) => value !== path));
        if (selected === path) setSelected(null);
      } catch (error) {
        pushToast({ kind: "error", title: t("sync.errorTitle"), detail: syncMessage(error, t("sync.errorTitle")) });
      }
    },
    [pushToast, selected, t],
  );

  const addDocuments = useCallback(async () => {
    try {
      const picked = await openDialog({
        multiple: true,
        title: t("sync.addFile"),
        filters: [{ name: "Office Swiss Army Knife", extensions: ["oswk"] }],
      });
      const paths = Array.isArray(picked) ? picked : picked ? [picked] : [];
      const documents = paths.filter((path): path is string => typeof path === "string" && isOswkPath(path));
      if (!documents.length) return;
      setAdded((current) => [...new Set([...current, ...documents])]);
      setSelected(documents[0]);
      if (enabled) {
        for (const path of documents) {
          await checkStatus(path);
        }
      }
    } catch (error) {
      pushToast({ kind: "error", title: t("sync.errorTitle"), detail: syncMessage(error, t("sync.errorTitle")) });
    }
  }, [checkStatus, enabled, pushToast, t]);

  const checkAll = useCallback(async () => {
    if (!requireEnabled()) return;
    for (const path of tracked) {
      // Sequential on purpose: each check may download a file to hash it, and
      // hammering the server with parallel PROPFINDs is not polite.
      await checkStatus(path);
    }
  }, [checkStatus, requireEnabled, tracked]);

  const loadRemote = useCallback(async () => {
    if (!requireEnabled()) return;
    setLoadingRemote(true);
    setRemoteError(null);
    try {
      setRemote(await syncList());
    } catch (error) {
      setRemoteError(syncMessage(error, t("sync.errorTitle")));
    } finally {
      setLoadingRemote(false);
    }
  }, [requireEnabled, t]);

  const downloadRemote = useCallback(
    async (entry: SyncListEntry) => {
      if (!requireEnabled()) return;
      setBusy(`remote:${entry.name}`);
      try {
        if (isAndroid()) {
          // Android hands out content:// URIs, not paths: download into the
          // app cache first, then publish through the SAF save dialog.
          const cache = await appCacheDir();
          const directory = await join(cache, "sync-downloads");
          await invoke("ensure_dir", { path: directory });
          const target = await join(directory, entry.name);
          const view = await syncDownload(entry.name, target);
          await saveFileOnAndroid(target, entry.name);
          setRow(view.localPath, { kind: "ready", view });
        } else {
          const target = await saveDialog({ defaultPath: entry.name, title: t("sync.downloadTitle") });
          if (!target) return;
          const view = await syncDownload(entry.name, target);
          setRow(view.localPath, { kind: "ready", view });
          setSelected(view.localPath);
        }
        pushToast({ kind: "success", title: t("sync.download"), detail: entry.name });
      } catch (error) {
        pushToast({ kind: "error", title: t("sync.errorTitle"), detail: syncMessage(error, t("sync.errorTitle")) });
      } finally {
        setBusy(null);
      }
    },
    [pushToast, requireEnabled, setRow, t],
  );

  const saveSettings = useCallback(async () => {
    if (!draft) return;
    setSaving(true);
    setSettingsError(null);
    try {
      const next = await syncSaveConfig({
        enabled: draft.enabled,
        provider: draft.provider,
        url: draft.url,
        username: draft.username,
        // Empty field + existing password = keep it (null); a previously
        // empty field with no stored password is also null.
        password: draft.password.length > 0 ? draft.password : config?.hasPassword ? null : "",
        remoteDir: draft.remoteDir,
      });
      setConfig(next);
      setDraft(draftFrom(next));
      setTestResult(null);
      pushToast({ kind: "success", title: t("sync.save"), detail: t("sync.saved") });
    } catch (error) {
      setSettingsError(syncMessage(error, t("sync.errorTitle")));
    } finally {
      setSaving(false);
    }
  }, [config?.hasPassword, draft, pushToast, t]);

  const testConnection = useCallback(async () => {
    setTesting(true);
    setTestResult(null);
    try {
      const result = await syncTestConnection();
      setTestResult({
        ok: true,
        message: result.remoteDirExists
          ? `${result.server} · ${result.message}`
          : `${result.server} · ${t("sync.testFolderMissing")}`,
      });
    } catch (error) {
      setTestResult({ ok: false, message: syncMessage(error, t("sync.errorTitle")) });
    } finally {
      setTesting(false);
    }
  }, [t]);

  const oauthSelected = draft?.provider === "onedrive" || draft?.provider === "google-drive";

  return (
    <Screen
      title={
        <span className="flex items-center gap-2">
          <Cloud size={21} /> {t("sync.title")}
        </span>
      }
      subtitle={t("sync.subtitle")}
      actions={
        <Badge tone={enabled ? "ok" : "default"}>
          {enabled ? t("sync.enable") : t("sync.offTitle")}
        </Badge>
      }
    >
      <TwoColumn
        main={
          <>
            <Card>
              <div className="flex items-start gap-3">
                {enabled ? <CheckCircle2 size={18} className="mt-0.5" /> : <CloudOff size={18} className="mt-0.5 muted" />}
                <div className="flex-1">
                  <Toggle
                    checked={draft?.enabled ?? false}
                    label={t("sync.enable")}
                    hint={t("sync.enableHint")}
                    onChange={(value) => setDraft((current) => (current ? { ...current, enabled: value } : current))}
                  />
                </div>
              </div>
              <div className="text-xs muted flex items-start gap-2">
                <ShieldCheck size={13} className="mt-0.5 shrink-0" />
                <span>
                  {t("sync.noBackground")}
                  {capabilities ? ` ${t("sync.limit", { size: formatBytes(capabilities.maxTransferBytes) })}` : ""}
                </span>
              </div>
            </Card>

            <Card>
              <Field label={t("sync.provider")}>
                {/* Native select so the OAuth providers can be rendered as
                    disabled options instead of being silently selectable. */}
                <select
                  className="select"
                  value={draft?.provider ?? "webdav"}
                  onChange={(event) => {
                    const provider = event.target.value as SyncProviderId;
                    setDraft((current) => (current ? { ...current, provider } : current));
                  }}
                >
                  <option value="webdav">{t("sync.provider.webdav")}</option>
                  <option value="onedrive" disabled>
                    {t("sync.provider.onedrive")}
                  </option>
                  <option value="google-drive" disabled>
                    {t("sync.provider.google")}
                  </option>
                </select>
              </Field>
              {oauthSelected ? (
                <p className="text-xs flex items-start gap-2" style={{ color: "var(--warn, #b45309)" }}>
                  <AlertTriangle size={13} className="mt-0.5 shrink-0" />
                  {t("sync.providerUnavailable")}
                </p>
              ) : null}

              <div className="grid gap-3 sm:grid-cols-2">
                <Field label={t("sync.url")}>
                  <TextInput
                    value={draft?.url ?? ""}
                    placeholder={t("sync.urlPlaceholder")}
                    spellCheck={false}
                    onChange={(event) => setDraft((current) => (current ? { ...current, url: event.target.value } : current))}
                  />
                </Field>
                <Field label={t("sync.remoteDir")} hint={t("sync.remoteDirHint")}>
                  <TextInput
                    value={draft?.remoteDir ?? "/"}
                    spellCheck={false}
                    onChange={(event) => setDraft((current) => (current ? { ...current, remoteDir: event.target.value } : current))}
                  />
                </Field>
                <Field label={t("sync.username")}>
                  <TextInput
                    value={draft?.username ?? ""}
                    autoComplete="off"
                    spellCheck={false}
                    onChange={(event) => setDraft((current) => (current ? { ...current, username: event.target.value } : current))}
                  />
                </Field>
                <Field
                  label={t("sync.password")}
                  hint={
                    config?.hasPassword
                      ? t("sync.passwordStored", { storage: config.passwordStorage })
                      : t("sync.passwordMissing")
                  }
                >
                  <TextInput
                    type="password"
                    value={draft?.password ?? ""}
                    autoComplete="new-password"
                    placeholder={config?.hasPassword ? "••••••••" : ""}
                    onChange={(event) => setDraft((current) => (current ? { ...current, password: event.target.value } : current))}
                  />
                </Field>
              </div>
              <p className="text-xs muted">{t("sync.passwordHint")}</p>

              {settingsError ? (
                <p className="text-xs" style={{ color: "var(--danger, #b91c1c)" }}>
                  {settingsError}
                </p>
              ) : null}

              <div className="flex flex-wrap items-center gap-2">
                <Button variant="primary" onClick={() => void saveSettings()} disabled={saving || !draft} icon={saving ? <Spinner size={14} /> : undefined}>
                  {t("sync.save")}
                </Button>
                <Button onClick={() => void testConnection()} disabled={testing || !enabled} icon={testing ? <Spinner size={14} /> : <RefreshCw size={14} />}>
                  {testing ? t("sync.testing") : t("sync.test")}
                </Button>
              </div>
              {testResult ? (
                <p className="text-xs flex items-start gap-2" style={{ color: testResult.ok ? "var(--text-2)" : "var(--danger, #b91c1c)" }}>
                  {testResult.ok ? <CheckCircle2 size={13} className="mt-0.5 shrink-0" /> : <AlertTriangle size={13} className="mt-0.5 shrink-0" />}
                  {testResult.message}
                </p>
              ) : null}
            </Card>

            <Card>
              <div className="flex items-center justify-between gap-2 mb-2">
                <h3 className="text-[13px] font-bold uppercase tracking-wider muted">{t("sync.remoteTitle")}</h3>
                <Button size="sm" variant="ghost" onClick={() => void loadRemote()} disabled={loadingRemote || !enabled} icon={loadingRemote ? <Spinner size={13} /> : <FolderOpen size={13} />}>
                  {t("sync.loadRemote")}
                </Button>
              </div>
              {remoteError ? (
                <p className="text-xs" style={{ color: "var(--danger, #b91c1c)" }}>
                  {remoteError}
                </p>
              ) : null}
              {remote && remote.length === 0 ? <p className="text-xs muted">{t("sync.remoteEmpty")}</p> : null}
              {remote && remote.length > 0 ? (
                <div className="flex flex-col gap-1.5">
                  {remote.map((entry) => (
                    <div key={entry.name} className="flex items-center gap-2 card-soft px-3 py-2">
                      <FileText size={14} className="shrink-0 muted" />
                      <span className="text-[13px] truncate flex-1" title={entry.name}>
                        {entry.name}
                      </span>
                      <span className="text-xs muted">
                        {formatBytes(entry.size)} · {formatDate(entry.modified)}
                      </span>
                      <IconButton
                        label={t("sync.download")}
                        onClick={() => void downloadRemote(entry)}
                        disabled={busy === `remote:${entry.name}` || !enabled}
                      >
                        {busy === `remote:${entry.name}` ? <Spinner size={13} /> : <Download size={14} />}
                      </IconButton>
                    </div>
                  ))}
                </div>
              ) : null}
            </Card>
          </>
        }
        side={
          <>
            <Card>
              <div className="flex items-center justify-between gap-2 mb-2">
                <h3 className="text-[13px] font-bold uppercase tracking-wider muted">{t("sync.filesTitle")}</h3>
                <div className="flex items-center gap-1">
                  <IconButton label={t("sync.addFile")} onClick={() => void addDocuments()}>
                    <Plus size={15} />
                  </IconButton>
                  <IconButton label={t("sync.checkAll")} onClick={() => void checkAll()} disabled={!enabled || tracked.length === 0}>
                    <RefreshCw size={15} />
                  </IconButton>
                </div>
              </div>
              <p className="text-xs muted mb-2">{t("sync.filesHint")}</p>

              {tracked.length === 0 ? (
                <EmptyState
                  icon={<FileText size={22} />}
                  title={t("sync.noFiles")}
                  hint={t("sync.noFilesHint")}
                  action={
                    <Button size="sm" onClick={() => void addDocuments()} icon={<Plus size={13} />}>
                      {t("sync.addFile")}
                    </Button>
                  }
                />
              ) : (
                <div className="flex flex-col gap-2">
                  {tracked.map((path) => {
                    const row = rows[path] ?? { kind: "idle" as const };
                    const view = row.kind === "ready" ? row.view : null;
                    const state: SyncStateId | null = view?.state ?? null;
                    const fileName = path.split(/[\\/]/).pop() ?? path;
                    const isSelected = selected === path;
                    return (
                      <div
                        key={path}
                        className="card-soft px-3 py-2.5 flex flex-col gap-1.5"
                        style={isSelected ? { outline: "2px solid var(--accent)" } : undefined}
                        onClick={() => setSelected(path)}
                        role="button"
                        tabIndex={0}
                        onKeyDown={(event) => {
                          if (event.key === "Enter") setSelected(path);
                        }}
                      >
                        <div className="flex items-center gap-2 min-w-0">
                          <span className="text-[13px] font-medium truncate" title={path}>
                            {fileName}
                          </span>
                          {row.kind === "loading" ? <Spinner size={13} /> : null}
                          <span className="ml-auto shrink-0">
                            <Badge tone={syncStateTone(state)}>
                              {state ? t(`sync.state.${state}`) : view ? t("sync.untracked") : "—"}
                            </Badge>
                          </span>
                        </div>
                        {view ? (
                          <span className="text-xs muted">
                            {t("sync.local")}: {formatBytes(view.localSize)}
                            {" · "}
                            {t("sync.cloud")}: {formatBytes(view.remoteSize)}
                            {" · "}
                            {t("sync.lastSynced")}: {view.lastSyncedAt ? formatDate(view.lastSyncedAt) : t("sync.never")}
                            {view.localRevision > 0 ? ` · r${view.localRevision}` : ""}
                          </span>
                        ) : (
                          <span className="text-xs muted">
                            {row.kind === "error" ? row.message : enabled ? t("sync.check") : t("sync.offHint")}
                          </span>
                        )}
                        <div className="flex items-center gap-1">
                          <IconButton
                            label={t("sync.check")}
                            onClick={(event) => {
                              event.stopPropagation();
                              void checkStatus(path);
                            }}
                            disabled={!enabled || busy === path}
                          >
                            <RefreshCw size={13} />
                          </IconButton>
                          <IconButton
                            label={t("sync.upload")}
                            onClick={(event) => {
                              event.stopPropagation();
                              void runUpload(path);
                            }}
                            disabled={!enabled || busy === path}
                          >
                            {busy === path ? <Spinner size={13} /> : <UploadCloud size={13} />}
                          </IconButton>
                          {needsResolution(state) ? (
                            <IconButton
                              label={t("sync.resolve")}
                              onClick={(event) => {
                                event.stopPropagation();
                                setSelected(path);
                              }}
                              disabled={busy === path}
                            >
                              <GitMerge size={13} />
                            </IconButton>
                          ) : null}
                          <span className="ml-auto">
                            <IconButton
                              label={t("sync.forget")}
                              onClick={(event) => {
                                event.stopPropagation();
                                void runForget(path);
                              }}
                            >
                              <X size={13} />
                            </IconButton>
                          </span>
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}
            </Card>

            <Card>
              <h3 className="text-[13px] font-bold uppercase tracking-wider muted mb-2">{t("sync.resolveTitle")}</h3>
              {!selected || !rows[selected] || rows[selected].kind !== "ready" || !needsResolution((rows[selected] as { kind: "ready"; view: SyncStatusView }).view.state) ? (
                <p className="text-xs muted">{t("sync.resolveHint")}</p>
              ) : (
                (() => {
                  const view = (rows[selected] as { kind: "ready"; view: SyncStatusView }).view;
                  return (
                    <div className="flex flex-col gap-3">
                      <p className="text-[13px] font-medium">
                        {t("sync.resolveFor", { file: view.file })}
                      </p>
                      {view.state === "conflict" ? <p className="text-xs" style={{ color: "var(--danger, #b91c1c)" }}>{t("sync.conflictExplanation")}</p> : null}
                      <p className="text-xs muted">{t("sync.resolveHint")}</p>
                      <div className="text-xs muted">
                        {t("sync.local")}: {shortHash(view.localSha256)} · {t("sync.cloud")}: {shortHash(view.cloudSha256)} ·{" "}
                        {t("sync.baseSha256Label", { hash: shortHash(view.baseSha256) })}
                      </div>
                      <div className="flex flex-col gap-2">
                        <Button variant="primary" size="sm" onClick={() => void runResolve(view.localPath, "keep_local")} disabled={busy === view.localPath} icon={<UploadCloud size={13} />}>
                          {t("sync.keepLocal")}
                        </Button>
                        <p className="text-xs muted -mt-1">{t("sync.keepLocalHint")}</p>
                        <Button variant="danger" size="sm" onClick={() => void runResolve(view.localPath, "keep_cloud")} disabled={busy === view.localPath} icon={<Download size={13} />}>
                          {t("sync.keepCloud")}
                        </Button>
                        <p className="text-xs muted -mt-1">{t("sync.keepCloudHint")}</p>
                        <Button size="sm" onClick={() => void runResolve(view.localPath, "keep_both")} disabled={busy === view.localPath} icon={<GitMerge size={13} />}>
                          {t("sync.keepBoth")}
                        </Button>
                        <p className="text-xs muted -mt-1">{t("sync.keepBothHint")}</p>
                      </div>
                    </div>
                  );
                })()
              )}
            </Card>

            <Card>
              <h3 className="text-[13px] font-bold uppercase tracking-wider muted mb-2">{t("sync.downloadTitle")}</h3>
              <p className="text-xs muted">{t("sync.downloadHint")}</p>
            </Card>
          </>
        }
      />
    </Screen>
  );
}
