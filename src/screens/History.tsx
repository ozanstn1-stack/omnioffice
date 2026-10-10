import { useEffect, useMemo, useState } from "react";
import { CheckCircle2, Download, FilePlus2, FolderOpen, ScrollText, Trash2, XCircle } from "lucide-react";
import { openAnyFile, revealAnyFile, saveFileBytes } from "../lib/mobile";
import { Badge, Button, Card, EmptyState, Segmented, TextInput } from "../components/ui";
import { Screen } from "../components/layout";
import { useT } from "../lib/i18n";
import { reportError, useRecent, useToasts } from "../lib/store";
import { clearOperations, loadOperations } from "../lib/api";
import type { OperationEntry } from "../lib/types";
import { formatBytes, formatDate, isPdf } from "../lib/format";
import type { Navigate } from "../lib/nav";

/** RFC 4180 field: commas, quotes and line breaks force double quotes. */
export function csvField(value: string): string {
  return /[",\r\n]/.test(value) ? `"${value.replace(/"/g, '""')}"` : value;
}

/** Case-insensitive match on the operation name or either path. */
export function filterOperations(entries: OperationEntry[], filter: string): OperationEntry[] {
  const needle = filter.trim().toLowerCase();
  if (!needle) return entries;
  return entries.filter(
    (entry) =>
      entry.operation.toLowerCase().includes(needle) ||
      entry.inputPath.toLowerCase().includes(needle) ||
      entry.outputPath.toLowerCase().includes(needle),
  );
}

/** Serializes the (already filtered) rows with a stable header row. */
export function operationsToCsv(entries: OperationEntry[]): string {
  const header = ["operation", "input", "output", "pages", "bytesIn", "bytesOut", "ok", "detail"];
  const rows = entries.map((entry) =>
    [
      entry.operation,
      entry.inputPath,
      entry.outputPath,
      entry.pageCount ?? "",
      entry.inputBytes ?? "",
      entry.outputBytes ?? "",
      entry.ok ? "true" : "false",
      entry.detail ?? "",
    ]
      .map((cell) => csvField(String(cell)))
      .join(","),
  );
  return [header.join(","), ...rows].join("\r\n");
}

export function History({ onNavigate }: { onNavigate: Navigate }) {
  const t = useT();
  const entries = useRecent((s) => s.entries);
  const refresh = useRecent((s) => s.refresh);
  const clear = useRecent((s) => s.clear);
  const pushToast = useToasts((s) => s.push);

  const [tab, setTab] = useState<"files" | "operations">("files");
  const [operations, setOperations] = useState<OperationEntry[] | null>(null);
  const [filter, setFilter] = useState("");

  const filtered = useMemo(() => filterOperations(operations ?? [], filter), [operations, filter]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    if (tab === "operations" && operations === null) {
      void loadOperations()
        .then(setOperations)
        .catch(() => setOperations([]));
    }
  }, [tab, operations]);

  const exportCsv = async () => {
    if (!filtered.length) {
      pushToast({ kind: "info", title: t("history.exportEmpty") });
      return;
    }
    try {
      // Desktop save dialog or Android SAF: the same helper the diagnostics
      // export uses, so no platform branching is needed here.
      const saved = await saveFileBytes(
        new TextEncoder().encode(operationsToCsv(filtered)),
        "omnioffice-operations.csv",
        { name: "CSV", extensions: ["csv"] },
      );
      if (saved) pushToast({ kind: "success", title: t("history.exported") });
    } catch (error) {
      reportError(error, t);
    }
  };

  return (
    <Screen
      title={t("history.title")}
      subtitle={t("history.subtitle")}
      actions={
        entries.length ? (
          <Button
            variant="danger"
            size="sm"
            icon={<Trash2 size={14} />}
            onClick={() => {
              void clear().then(() => pushToast({ kind: "success", title: t("history.cleared") }));
            }}
          >
            {t("history.clear")}
          </Button>
        ) : null
      }
    >
      <Segmented<"files" | "operations">
        value={tab}
        onChange={setTab}
        options={[
          { value: "files", label: t("history.tabFiles") },
          { value: "operations", label: t("history.tabOperations") },
        ]}
      />

      {tab === "operations" ? (
        <>
          <div className="flex flex-wrap items-center justify-between gap-2">
            <p className="text-xs muted">
              {operations ? `${filtered.length}/${operations.length}` : t("common.loading")}
            </p>
            <div className="flex flex-wrap items-center gap-2">
              <TextInput
                value={filter}
                aria-label={t("history.filter")}
                placeholder={t("history.filterPlaceholder")}
                spellCheck={false}
                className="w-56"
                onChange={(event) => setFilter(event.target.value)}
              />
              <Button size="sm" icon={<Download size={14} />} onClick={() => void exportCsv()}>
                {t("history.export")}
              </Button>
              <Button
                size="sm"
                variant="danger"
                icon={<Trash2 size={14} />}
                disabled={!operations?.length}
                onClick={() => {
                  void clearOperations().then(() => {
                    setOperations([]);
                    pushToast({ kind: "success", title: t("history.operationsCleared") });
                  });
                }}
              >
                {t("history.clearOperations")}
              </Button>
            </div>
          </div>
          {operations && operations.length === 0 ? (
            <Card>
              <EmptyState icon={<ScrollText size={22} />} title={t("history.operationsEmpty")} />
            </Card>
          ) : filtered.length === 0 ? (
            <Card>
              <EmptyState icon={<ScrollText size={22} />} title={t("history.exportEmpty")} />
            </Card>
          ) : (
            <Card className="p-2">
              <table className="table">
                <thead>
                  <tr>
                    <th>{t("history.operation")}</th>
                    <th>{t("history.input")}</th>
                    <th>{t("history.output")}</th>
                    <th style={{ width: 90 }}>{t("history.result")}</th>
                    <th style={{ width: 170 }}>{t("common.status")}</th>
                  </tr>
                </thead>
                <tbody>
                  {filtered.map((entry) => {
                    const delta =
                      entry.inputBytes && entry.outputBytes
                        ? `${formatBytes(entry.inputBytes)} → ${formatBytes(entry.outputBytes)}`
                        : entry.outputBytes
                          ? formatBytes(entry.outputBytes)
                          : "—";
                    return (
                      <tr key={entry.id}>
                        <td>
                          <Badge tone="accent">{entry.operation}</Badge>
                        </td>
                        <td className="truncate max-w-[220px]" title={entry.inputPath}>
                          <span className="text-xs">{entry.inputPath.split(/[\\/]/).pop()}</span>
                        </td>
                        <td className="truncate max-w-[220px]" title={entry.outputPath}>
                          <span className="text-xs">{entry.outputPath.split(/[\\/]/).pop()}</span>
                        </td>
                        <td className="text-xs tabular-nums">{delta}</td>
                        <td>
                          <span className="flex items-center gap-2 text-xs">
                            {entry.ok ? (
                              <CheckCircle2 size={13} style={{ color: "var(--ok)" }} />
                            ) : (
                              <XCircle size={13} style={{ color: "var(--danger)" }} />
                            )}
                            {formatDate(entry.createdAt)}
                          </span>
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </Card>
          )}
        </>
      ) : entries.length === 0 ? (
        <Card>
          <EmptyState
            icon={<FilePlus2 size={22} />}
            title={t("history.empty")}
            hint={t("home.recentEmptyHint")}
            action={
              <Button variant="primary" onClick={() => onNavigate("home")}>
                {t("nav.home")}
              </Button>
            }
          />
        </Card>
      ) : (
        <Card className="p-2">
          <table className="table">
            <thead>
              <tr>
                <th>{t("common.name")}</th>
                <th>{t("common.actions")}</th>
                <th style={{ width: 180 }}>{t("settings.version") === "" ? "Date" : "Date"}</th>
              </tr>
            </thead>
            <tbody>
              {entries.map((entry) => (
                <tr key={entry.path}>
                  <td>
                    <p className="font-medium truncate max-w-[520px]" title={entry.path}>
                      {entry.fileName}
                    </p>
                    <p className="text-xs muted truncate max-w-[520px]">{entry.path}</p>
                  </td>
                  <td>
                    <div className="flex gap-1.5">
                      <Button
                        size="sm"
                        onClick={() => {
                          if (isPdf(entry.path)) {
                            onNavigate("reader", { files: [entry.path] });
                          } else {
                            void openAnyFile(entry.path).catch(() => undefined);
                          }
                        }}
                      >
                        {t("common.open")}
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        icon={<FolderOpen size={13} />}
                        onClick={() => void revealAnyFile(entry.path).catch(() => undefined)}
                      />
                    </div>
                  </td>
                  <td className="muted text-xs">{formatDate(entry.timestamp)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </Card>
      )}
    </Screen>
  );
}
