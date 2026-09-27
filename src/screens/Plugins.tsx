/**
 * Plugins screen: installed sandboxed extensions, their manifests, runtime
 * status, command runner and the host-side log. Every plugin runs in a Web
 * Worker; this screen only talks to the plugin manager in `src/lib/plugins.ts`.
 */
import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderPlus, Package, Play, Power, RefreshCw, RotateCw, Trash2 } from "lucide-react";
import { Badge, Card, EmptyState } from "../components/ui";
import { useT } from "../lib/i18n";
import { isAndroid } from "../lib/mobile";
import { useOfficeTabs } from "../lib/office-store";
import { manifestNeedsDocument, usePluginStore, type PluginInfo, type PluginStatus } from "../lib/plugins";

const STATUS_TONE: Record<PluginStatus, "default" | "ok" | "warn" | "danger" | "accent"> = {
  disabled: "default",
  idle: "warn",
  running: "ok",
  crashed: "danger",
};

export function Plugins() {
  const t = useT();
  const plugins = usePluginStore((state) => state.plugins);
  const busy = usePluginStore((state) => state.busy);
  const load = usePluginStore((state) => state.load);
  const install = usePluginStore((state) => state.install);
  const reloadSample = usePluginStore((state) => state.reloadSample);
  const enable = usePluginStore((state) => state.enable);
  const disable = usePluginStore((state) => state.disable);
  const restart = usePluginStore((state) => state.restart);
  const remove = usePluginStore((state) => state.remove);
  const run = usePluginStore((state) => state.run);
  const [confirmRemove, setConfirmRemove] = useState<string | null>(null);
  const documentOpen = useOfficeTabs((state) => state.tabs.some((tab) => tab.id === state.activeId));

  useEffect(() => {
    void load();
  }, [load]);

  const pickFolder = async () => {
    const picked = await open({ directory: true, multiple: false, title: t("plugins.installFolder") });
    if (typeof picked === "string" && picked) await install(picked);
  };

  return (
    <div className="screen">
      <div className="screen-head">
        <div>
          <h1>
            <Package size={18} /> {t("plugins.title")}
          </h1>
          <p className="muted">{t("plugins.subtitle")}</p>
        </div>
        <div className="row">
          <button type="button" className="btn btn-soft" onClick={() => void load()} disabled={busy}>
            <RefreshCw size={13} /> {t("plugins.refresh")}
          </button>
          <button type="button" className="btn btn-soft" onClick={() => void reloadSample()} disabled={busy}>
            {t("plugins.reloadSample")}
          </button>
          {!isAndroid() ? (
            <button type="button" className="btn btn-soft" onClick={() => void pickFolder()} disabled={busy}>
              <FolderPlus size={13} /> {t("plugins.installFolder")}
            </button>
          ) : null}
        </div>
      </div>

      <Card soft>
        <p className="muted small">{t("plugins.sandboxNote")}</p>
      </Card>

      {plugins.length === 0 ? <EmptyState title={t("plugins.empty")} hint={t("plugins.emptyHint")} /> : null}

      <div className="stack">
        {plugins.map((plugin) => (
          <PluginCard
            key={plugin.manifest.id}
            plugin={plugin}
            documentOpen={documentOpen}
            confirmRemove={confirmRemove === plugin.manifest.id}
            onAskRemove={() => setConfirmRemove(plugin.manifest.id)}
            onCancelRemove={() => setConfirmRemove(null)}
            onEnable={() => void enable(plugin.manifest.id)}
            onDisable={() => disable(plugin.manifest.id)}
            onRestart={() => void restart(plugin.manifest.id)}
            onRemove={() => {
              setConfirmRemove(null);
              void remove(plugin.manifest.id);
            }}
            onRun={(commandId) => void run(plugin.manifest.id, commandId)}
          />
        ))}
      </div>
    </div>
  );
}

function PluginCard({
  plugin,
  documentOpen,
  confirmRemove,
  onAskRemove,
  onCancelRemove,
  onEnable,
  onDisable,
  onRestart,
  onRemove,
  onRun,
}: {
  plugin: PluginInfo;
  documentOpen: boolean;
  confirmRemove: boolean;
  onAskRemove: () => void;
  onCancelRemove: () => void;
  onEnable: () => void;
  onDisable: () => void;
  onRestart: () => void;
  onRemove: () => void;
  onRun: (commandId: string) => void;
}) {
  const t = useT();
  const { manifest, status, error, lastError, logs, lastResult } = plugin;
  const waitingForDocument = manifestNeedsDocument(manifest) && !documentOpen;

  return (
    <Card>
      <div className="row">
        <strong>{manifest.name}</strong>
        <span className="muted small">{manifest.id}</span>
        <Badge tone={STATUS_TONE[status]}>{t(`plugins.status.${status}`)}</Badge>
        <span className="spacer" />
        <span className="muted small">{t("plugins.version", { version: manifest.version })}</span>
      </div>
      <p className="muted small">
        {t("plugins.apiVersion", { version: manifest.apiVersion })} · {t("plugins.requiresApp", { range: manifest.compatibility.app })}
      </p>
      <div className="row" style={{ flexWrap: "wrap" }}>
        <span className="muted small">{t("plugins.permissions")}:</span>
        {manifest.permissions.map((permission) => (
          <Badge key={permission} tone="accent">
            {permission}
          </Badge>
        ))}
      </div>

      {error ? <p className="muted small">{t("plugins.crashed", { message: error })}</p> : null}
      {lastError ? <p className="muted small">{t("plugins.commandFailed", { message: lastError })}</p> : null}

      <div className="stack" style={{ marginTop: 8 }}>
        {manifest.commands.length === 0 ? <p className="muted small">{t("plugins.noCommands")}</p> : null}
        {manifest.commands.map((command) => (
          <div className="row" key={command.id}>
            <button
              type="button"
              className="btn btn-soft"
              disabled={status !== "running" || waitingForDocument}
              onClick={() => onRun(command.id)}
            >
              <Play size={13} /> {command.title}
            </button>
            {command.description ? <span className="muted small">{command.description}</span> : null}
            {waitingForDocument ? <span className="muted small">{t("plugins.needDocument")}</span> : null}
          </div>
        ))}
      </div>

      {lastResult ? (
        <p className="muted small">
          <strong>{t("plugins.lastResult")}:</strong> {lastResult}
        </p>
      ) : null}

      <div className="row" style={{ marginTop: 8 }}>
        {status === "disabled" ? (
          <button type="button" className="btn btn-soft" onClick={onEnable}>
            <Power size={13} /> {t("plugins.enable")}
          </button>
        ) : (
          <>
            <button type="button" className="btn btn-soft" onClick={onRestart}>
              <RotateCw size={13} /> {t("plugins.restart")}
            </button>
            <button type="button" className="btn btn-soft" onClick={onDisable}>
              <Power size={13} /> {t("plugins.disable")}
            </button>
          </>
        )}
        {confirmRemove ? (
          <>
            <span className="muted small">{t("plugins.removeConfirm", { name: manifest.name })}</span>
            <button type="button" className="btn btn-soft" onClick={onRemove}>
              {t("plugins.remove")}
            </button>
            <button type="button" className="btn btn-soft" onClick={onCancelRemove}>
              {t("common.cancel")}
            </button>
          </>
        ) : (
          <button type="button" className="btn btn-soft" onClick={onAskRemove}>
            <Trash2 size={13} /> {t("plugins.remove")}
          </button>
        )}
      </div>

      {logs.length > 0 ? (
        <details style={{ marginTop: 8 }}>
          <summary className="muted small">{t("plugins.log")}</summary>
          <div className="stack">
            {logs.slice(-8).map((line, index) => (
              <span className="muted small" key={`${index}-${line}`}>
                {line}
              </span>
            ))}
          </div>
        </details>
      ) : null}
    </Card>
  );
}
