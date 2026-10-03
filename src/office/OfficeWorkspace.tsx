/**
 * Office workspace: a tab bar over the Writer/Calc/Impress editors with
 * shared autosave, crash recovery and version-history affordances.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { FilePlus2, FileSpreadsheet, FileText, History, Presentation, RotateCcw, X } from "lucide-react";
import { isOfficePath, openOfficePath, useRecovery, useOfficeTabs } from "../lib/office-store";
import type { OfficeKind } from "../lib/office-types";
import { errorMessage, useDev, useSettings, useToasts } from "../lib/store";
import { useT } from "../lib/i18n";
import * as api from "../lib/office-api";
import { Dialog } from "./office-ui";
import { WriterEditor } from "./WriterEditor";
import { CalcEditor } from "./CalcEditor";
import { ImpressEditor } from "./ImpressEditor";

export function OfficeWorkspace() {
  const t = useT();
  const tabs = useOfficeTabs((state) => state.tabs);
  const activeId = useOfficeTabs((state) => state.activeId);
  const { create, activate, close } = useOfficeTabs();
  const settings = useSettings((state) => state.settings);
  const recovery = useRecovery();
  const dev = useDev();
  const bootstrapped = useRef(false);
  const [recovered, setRecovered] = useState<Array<{ documentId: string; kind: string; title: string }>>([]);
  const [pendingClose, setPendingClose] = useState<{ tabId: string; title: string } | null>(null);
  const [historyFor, setHistoryFor] = useState<string | null>(null);

  // Automation hook (screenshots/tests) and external open requests.
  useEffect(() => {
    if (bootstrapped.current) return;
    const requestedFiles = (dev.files ?? []).filter((path) => isOfficePath(path));
    if (dev.newTab === "writer" || dev.newTab === "calc" || dev.newTab === "impress") {
      bootstrapped.current = true;
      create(dev.newTab);
      for (const path of requestedFiles) void openOfficePath(path);
      return;
    }
    if (requestedFiles.length > 0) {
      bootstrapped.current = true;
      void (async () => {
        for (const path of requestedFiles) await openOfficePath(path);
      })();
    }
  }, [create, dev.files, dev.newTab]);

  useEffect(() => {
    const handler = (event: Event) => {
      const path = (event as CustomEvent<string>).detail;
      if (typeof path === "string" && path) void openOfficePath(path);
    };
    window.addEventListener("oswk-open-path", handler);
    return () => window.removeEventListener("oswk-open-path", handler);
  }, []);

  useEffect(() => {
    void recovery.refresh().then((records) => {
      if (records.length > 0) setRecovered(records);
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Autosave every configured interval; snapshots land in the recovery folder.
  useEffect(() => {
    const seconds = settings.autosaveSeconds ?? 0;
    if (!seconds || seconds <= 0) return;
    const timer = window.setInterval(() => {
      for (const tab of useOfficeTabs.getState().tabs) {
        if (tab.dirty) void useRecovery.getState().save(tab);
      }
    }, seconds * 1000);
    return () => window.clearInterval(timer);
  }, [settings.autosaveSeconds]);

  // Flush pending work when the window goes away. The autosave timer only runs
  // while this component is mounted, so without this a document edited seconds
  // before the last autosave tick would be lost on a hard close.
  useEffect(() => {
    const flush = () => {
      for (const tab of useOfficeTabs.getState().tabs) {
        if (tab.dirty) void useRecovery.getState().save(tab);
      }
    };
    window.addEventListener("beforeunload", flush);
    window.addEventListener("pagehide", flush);
    return () => {
      window.removeEventListener("beforeunload", flush);
      window.removeEventListener("pagehide", flush);
    };
  }, []);

  // Warn before a dirty document disappears: a tab close, and the app close.
  useEffect(() => {
    const onBeforeUnload = (event: BeforeUnloadEvent) => {
      const dirty = useOfficeTabs.getState().tabs.filter((tab) => tab.dirty);
      if (dirty.length === 0) return;
      // Best effort snapshot first, then let the browser show its own prompt.
      for (const tab of dirty) void useRecovery.getState().save(tab);
      event.preventDefault();
      event.returnValue = "";
    };
    window.addEventListener("beforeunload", onBeforeUnload);
    return () => window.removeEventListener("beforeunload", onBeforeUnload);
  }, []);

  const requestClose = useCallback(
    (tabId: string) => {
      const tab = useOfficeTabs.getState().tabs.find((candidate) => candidate.id === tabId);
      if (!tab) return;
      if (tab.dirty) {
        setPendingClose({ tabId, title: tab.title });
        return;
      }
      // A clean document has nothing to lose, so drop its stale snapshot too.
      void useRecovery
        .getState()
        .discard(tabId)
        .catch(() => undefined);
      close(tabId);
    },
    [close],
  );

  const confirmClose = useCallback(
    (discard: boolean) => {
      if (!pendingClose) return;
      if (discard)
        void useRecovery
          .getState()
          .discard(pendingClose.tabId)
          .catch(() => undefined);
      close(pendingClose.tabId);
      setPendingClose(null);
    },
    [close, pendingClose],
  );

  // Ctrl+W mirrors the tab close button, including the unsaved-changes guard.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!event.ctrlKey && !event.metaKey) return;
      if (event.key.toLowerCase() !== "w") return;
      const current = useOfficeTabs.getState().activeId;
      if (!current) return;
      event.preventDefault();
      event.stopImmediatePropagation();
      requestClose(current);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [requestClose]);

  const restore = useCallback(
    async (documentId: string) => {
      const payload = await useRecovery.getState().restore(documentId);
      if (!payload) return;
      const title = recovered.find((entry) => entry.documentId === documentId)?.title ?? "Recovered document";
      const kind = payload.kind as OfficeKind;
      create(kind, title, payload.model as never);
      await useRecovery.getState().discard(documentId);
      setRecovered((current) => current.filter((entry) => entry.documentId !== documentId));
      useToasts.getState().push({ kind: "success", title: t("office.recovered"), detail: title });
    },
    [create, recovered, t],
  );

  const active = tabs.find((tab) => tab.id === activeId) ?? tabs[tabs.length - 1] ?? null;

  return (
    <div className="office-workspace">
      <div className="office-tabs">
        <div className="office-tab-list">
          {tabs.map((tab) => (
            <div
              key={tab.id}
              className={`office-tab${tab.id === active?.id ? " is-active" : ""}`}
              role="tab"
              tabIndex={tab.id === active?.id ? 0 : -1}
              aria-selected={tab.id === active?.id}
              onClick={() => activate(tab.id)}
              onKeyDown={(event) => {
                if (event.key === "Enter" || event.key === " ") {
                  event.preventDefault();
                  activate(tab.id);
                }
              }}
              title={tab.path ?? tab.title}
            >
              <span className={`office-tab-dot kind-${tab.kind}`} />
              <span className="office-tab-title">
                {tab.title}
                {tab.dirty ? " •" : ""}
              </span>
              <button
                type="button"
                className="icon-btn"
                onClick={(event) => {
                  event.stopPropagation();
                  requestClose(tab.id);
                }}
                aria-label={t("common.close")}
              >
                <X size={12} />
              </button>
            </div>
          ))}
        </div>
        <div className="office-tab-actions">
          <button
            type="button"
            className="tool-btn is-icon-only"
            title={t("office.versionHistory")}
            aria-label={t("office.versionHistory")}
            disabled={!active}
            onClick={() => active && setHistoryFor(active.id)}
          >
            <History size={15} />
          </button>
          <button
            type="button"
            className="tool-btn is-icon-only"
            title={t("office.newDocument")}
            onClick={() => create("writer")}
          >
            <FileText size={15} />
          </button>
          <button
            type="button"
            className="tool-btn is-icon-only"
            title={t("office.newSpreadsheet")}
            onClick={() => create("calc")}
          >
            <FileSpreadsheet size={15} />
          </button>
          <button
            type="button"
            className="tool-btn is-icon-only"
            title={t("office.newPresentation")}
            onClick={() => create("impress")}
          >
            <Presentation size={15} />
          </button>
        </div>
      </div>

      {recovered.length > 0 ? (
        <div className="recovery-banner">
          <RotateCcw size={15} />
          <span>{t("office.recoveryAvailable")}</span>
          <span className="spacer" />
          {recovered.slice(0, 3).map((entry) => (
            <button
              key={entry.documentId}
              type="button"
              className="btn btn-soft"
              onClick={() => void restore(entry.documentId)}
            >
              {entry.title}
            </button>
          ))}
          <button
            type="button"
            className="btn btn-soft"
            onClick={async () => {
              for (const entry of recovered) await useRecovery.getState().discard(entry.documentId);
              setRecovered([]);
            }}
          >
            {t("office.discardAll")}
          </button>
        </div>
      ) : null}

      <div className="office-editor-host">
        {active ? (
          active.kind === "writer" ? (
            <WriterEditor key={active.id} tab={active as never} />
          ) : active.kind === "calc" ? (
            <CalcEditor key={active.id} tab={active as never} />
          ) : (
            <ImpressEditor key={active.id} tab={active as never} />
          )
        ) : (
          <div className="empty-state">
            <FilePlus2 size={26} />
            <h3>{t("office.noTabs")}</h3>
            <p className="muted">{t("office.noTabsHint")}</p>
            <div className="row">
              <button type="button" className="btn btn-primary" onClick={() => create("writer")}>
                {t("office.newDocument")}
              </button>
              <button type="button" className="btn btn-soft" onClick={() => create("calc")}>
                {t("office.newSpreadsheet")}
              </button>
              <button type="button" className="btn btn-soft" onClick={() => create("impress")}>
                {t("office.newPresentation")}
              </button>
            </div>
          </div>
        )}
      </div>

      {pendingClose ? (
        <Dialog title={t("office.unsavedTitle")} onClose={() => setPendingClose(null)} wide>
          <p>{t("office.unsavedBody", { name: pendingClose.title })}</p>
          <div className="row" style={{ justifyContent: "flex-end", marginTop: 14 }}>
            <button type="button" className="btn btn-soft" onClick={() => setPendingClose(null)}>
              {t("common.cancel")}
            </button>
            <button type="button" className="btn btn-soft" onClick={() => confirmClose(true)}>
              {t("office.closeDiscard")}
            </button>
            <button type="button" className="btn btn-primary" onClick={() => confirmClose(false)}>
              {t("office.closeKeep")}
            </button>
          </div>
        </Dialog>
      ) : null}

      {historyFor ? <VersionHistoryDialog documentId={historyFor} onClose={() => setHistoryFor(null)} /> : null}

      {/* The Data Loss dialog host lives in App so the converter is covered too. */}
    </div>
  );
}

/**
 * Reads the on-disk snapshot list and lets the user roll a document back.
 *
 * History is written on every save but used to be write-only: no screen ever
 * called `history_list`, so 25 snapshots per document accumulated unreadable.
 */
function VersionHistoryDialog({ documentId, onClose }: { documentId: string; onClose: () => void }) {
  const t = useT();
  const [entries, setEntries] = useState<
    Array<{ version: number; savedAt: string; title: string; kind: string; size: number }>
  >([]);
  const [busy, setBusy] = useState(true);

  useEffect(() => {
    let alive = true;
    void api
      .historyList(documentId)
      .then((list) => {
        if (alive) setEntries([...list].sort((a, b) => b.version - a.version));
      })
      .catch((error) => {
        if (alive)
          useToasts.getState().push({ kind: "error", title: t("common.error"), detail: errorMessage(error, t) });
      })
      .finally(() => {
        if (alive) setBusy(false);
      });
    return () => {
      alive = false;
    };
  }, [documentId, t]);

  const restore = async (version: number) => {
    setBusy(true);
    try {
      const model = await api.historyLoad(documentId, version);
      useOfficeTabs.getState().edit(documentId, () => model as never);
      useToasts.getState().push({ kind: "success", title: t("office.versionRestored"), detail: `v${version}` });
      onClose();
    } catch (error) {
      useToasts.getState().push({ kind: "error", title: t("common.error"), detail: errorMessage(error, t) });
      setBusy(false);
    }
  };

  const formatStamp = (value: string) => {
    const parsed = new Date(value);
    return Number.isNaN(parsed.getTime()) ? value : parsed.toLocaleString();
  };

  return (
    <Dialog title={t("office.versionHistory")} onClose={onClose} wide>
      {busy && entries.length === 0 ? <p className="muted">{t("common.loading")}</p> : null}
      {!busy && entries.length === 0 ? <p className="muted">{t("office.noVersions")}</p> : null}
      <div className="version-list">
        {entries.map((entry) => (
          <div key={entry.version} className="version-row">
            <div>
              <strong>v{entry.version}</strong>
              <span className="muted"> · {formatStamp(entry.savedAt)}</span>
              <div className="muted small">
                {entry.title} · {(entry.size / 1024).toFixed(1)} KB
              </div>
            </div>
            <button type="button" className="btn btn-soft" disabled={busy} onClick={() => void restore(entry.version)}>
              {t("office.versionRestore")}
            </button>
          </div>
        ))}
      </div>
      {entries.length > 0 ? (
        <div className="row" style={{ justifyContent: "flex-end", marginTop: 12 }}>
          <button
            type="button"
            className="btn btn-soft"
            disabled={busy}
            onClick={async () => {
              setBusy(true);
              try {
                await api.historyClear(documentId);
                setEntries([]);
              } finally {
                setBusy(false);
              }
            }}
          >
            {t("office.versionClear")}
          </button>
        </div>
      ) : null}
    </Dialog>
  );
}
