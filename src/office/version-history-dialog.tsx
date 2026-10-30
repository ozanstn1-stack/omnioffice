/**
 * Version history dialog: reads the on-disk snapshots, restores one into the
 * open tab and compares a saved version with the current document (C13). Moved
 * out of OfficeWorkspace so neither file grows past the size ratchet.
 */
import { useEffect, useState } from "react";
import { useOfficeTabs } from "../lib/office-store";
import { errorMessage, useToasts } from "../lib/store";
import { useT } from "../lib/i18n";
import * as api from "../lib/office-api";
import { OfficeDiffView } from "../components/office-diff-view";
import { diffOfficeDocuments, type OfficeDiff } from "../lib/office-diff";
import { Dialog } from "./office-ui";

/**
 * Reads the on-disk snapshot list and lets the user roll a document back.
 *
 * History is written on every save but used to be write-only: no screen ever
 * called `history_list`, so 25 snapshots per document accumulated unreadable.
 * `historyKey` identifies the document's history (its path, see `historyKey`),
 * `tabId` is the tab a restored version is written into.
 */
export function VersionHistoryDialog({
  tabId,
  historyKey,
  onClose,
}: {
  tabId: string;
  historyKey: string;
  onClose: () => void;
}) {
  const t = useT();
  const [entries, setEntries] = useState<
    Array<{ version: number; savedAt: string; title: string; kind: string; size: number }>
  >([]);
  const [busy, setBusy] = useState(true);
  const [compare, setCompare] = useState<{ version: number; diff: OfficeDiff } | null>(null);

  useEffect(() => {
    let alive = true;
    void api
      .historyList(historyKey)
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
  }, [historyKey, t]);

  const restore = async (version: number) => {
    setBusy(true);
    try {
      const model = await api.historyLoad(historyKey, version);
      useOfficeTabs.getState().edit(tabId, () => model as never);
      useToasts.getState().push({ kind: "success", title: t("office.versionRestored"), detail: `v${version}` });
      onClose();
    } catch (error) {
      useToasts.getState().push({ kind: "error", title: t("common.error"), detail: errorMessage(error, t) });
      setBusy(false);
    }
  };

  const compareWithCurrent = async (version: number, kind: string) => {
    setBusy(true);
    try {
      const snapshot = await api.historyLoad(historyKey, version);
      const tab = useOfficeTabs.getState().tabs.find((candidate) => candidate.id === tabId);
      if (!tab) throw new Error("The document is no longer open.");
      setCompare({ version, diff: diffOfficeDocuments(kind as "writer" | "calc" | "impress", snapshot, tab.model) });
    } catch (error) {
      useToasts.getState().push({ kind: "error", title: t("common.error"), detail: errorMessage(error, t) });
    } finally {
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
            <div className="row">
              <button
                type="button"
                className="btn btn-soft"
                disabled={busy}
                onClick={() => void compareWithCurrent(entry.version, entry.kind)}
              >
                {t("office.versionCompare")}
              </button>
              <button
                type="button"
                className="btn btn-soft"
                disabled={busy}
                onClick={() => void restore(entry.version)}
              >
                {t("office.versionRestore")}
              </button>
            </div>
          </div>
        ))}
      </div>
      {compare ? (
        <Dialog
          title={t("office.versionCompareTitle", { version: compare.version })}
          onClose={() => setCompare(null)}
          wide
        >
          <OfficeDiffView
            diff={compare.diff}
            leftLabel={`v${compare.version}`}
            rightLabel={t("office.versionCurrent")}
          />
        </Dialog>
      ) : null}
      {entries.length > 0 ? (
        <div className="row" style={{ justifyContent: "flex-end", marginTop: 12 }}>
          <button
            type="button"
            className="btn btn-soft"
            disabled={busy}
            onClick={async () => {
              setBusy(true);
              try {
                await api.historyClear(historyKey);
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
