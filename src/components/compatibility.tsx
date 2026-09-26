import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Dialog } from "../office/office-ui";
import { Badge, SectionTitle, Spinner } from "./ui";
import { useT } from "../lib/i18n";
import { errorMessage } from "../lib/store";

export interface CompatibilityItem {
  feature: string;
  status: string;
  message: string;
}

export interface CompatibilityReport {
  target: string;
  items: CompatibilityItem[];
}

export interface FeatureSupport {
  feature: string;
  level: string;
  note: string;
}

export interface FormatCapabilities {
  extension: string;
  open: boolean;
  edit: boolean;
  save: boolean;
  pdfExport: boolean;
  losslessNative: boolean;
  features: FeatureSupport[];
}

const TARGETS: Record<string, string[]> = {
  writer: ["docx", "odt", "rtf", "txt", "md", "html", "pdf"],
  calc: ["xlsx", "ods", "csv", "pdf"],
  impress: ["pptx", "odp", "pdf"],
};

const ORDER = ["lost", "transformed", "unchanged"] as const;

type StatusGroup = (typeof ORDER)[number];

const STATUS_TONE: Record<StatusGroup, "danger" | "warn" | "ok"> = {
  lost: "danger",
  transformed: "warn",
  unchanged: "ok",
};

function groupFor(status: string): StatusGroup {
  if (status === "lost") return "lost";
  if (status === "unchanged") return "unchanged";
  return "transformed";
}

const supportedExtensions = () => invoke<string[]>("office_supported_extensions");
const formatCapabilities = (extension: string) => invoke<FormatCapabilities>("office_capabilities", { extension });
const compatibilityReport = (kind: string, model: unknown, target: string) =>
  invoke<CompatibilityReport>("office_compatibility", { kind, model, target });

function YesNo({ value }: { value: boolean }) {
  const t = useT();
  return <span style={{ color: value ? "var(--ok)" : "var(--muted)" }}>{value ? t("compat.yes") : t("compat.no")}</span>;
}

export function CompatibilityReportView({ report }: { report: CompatibilityReport | null }) {
  const t = useT();
  const [capabilities, setCapabilities] = useState<FormatCapabilities[] | null>(null);
  const [tableError, setTableError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    void (async () => {
      try {
        const extensions = await supportedExtensions();
        const rows = await Promise.all(extensions.map((extension) => formatCapabilities(extension)));
        if (alive) {
          setCapabilities(rows);
          setTableError(null);
        }
      } catch (error) {
        if (alive) setTableError(errorMessage(error, t));
      }
    })();
    return () => {
      alive = false;
    };
  }, [t]);

  const groups = useMemo(() => {
    const buckets: Record<StatusGroup, CompatibilityItem[]> = { lost: [], transformed: [], unchanged: [] };
    for (const item of report?.items ?? []) {
      buckets[groupFor(item.status)].push(item);
    }
    return buckets;
  }, [report]);

  return (
    <div className="flex flex-col gap-4">
      <div>
        {report ? (
          <>
            <div className="flex items-center gap-2 flex-wrap mb-2">
              <Badge>{report.target.toUpperCase()}</Badge>
              <span className="text-xs muted">
                {t("compat.summary", {
                  lost: groups.lost.length,
                  transformed: groups.transformed.length,
                  unchanged: groups.unchanged.length,
                })}
              </span>
            </div>
            {ORDER.map((status) =>
              groups[status].length ? (
                <div key={status} className="mb-3">
                  <div className="flex items-center gap-2 mb-1">
                    <Badge tone={STATUS_TONE[status]}>{t(`compat.${status}`)}</Badge>
                    <span className="text-xs muted">{groups[status].length}</span>
                  </div>
                  <div className="flex flex-col">
                    {groups[status].map((item, index) => (
                      <div
                        key={`${item.feature}-${index}`}
                        className="py-1.5 border-b last:border-0"
                        style={{ borderColor: "var(--border)" }}
                      >
                        <p className="text-[13px]" style={{ color: "var(--text-1)" }}>
                          {item.feature}
                        </p>
                        <p className="text-xs muted">{item.message}</p>
                      </div>
                    ))}
                  </div>
                </div>
              ) : null,
            )}
          </>
        ) : (
          <p className="text-xs muted">{t("compat.noReport")}</p>
        )}
      </div>

      <div>
        <SectionTitle>{t("compat.supportedFormats")}</SectionTitle>
        {tableError ? (
          <p className="text-xs" style={{ color: "var(--danger)" }}>
            {tableError}
          </p>
        ) : capabilities ? (
          <div className="overflow-auto">
            <table className="w-full text-xs">
              <thead>
                <tr className="text-left muted">
                  <th className="py-1 pr-3 font-medium">{t("compat.format")}</th>
                  <th className="py-1 px-3 font-medium">{t("compat.open")}</th>
                  <th className="py-1 px-3 font-medium">{t("compat.edit")}</th>
                  <th className="py-1 px-3 font-medium">{t("compat.save")}</th>
                  <th className="py-1 pl-3 font-medium">{t("compat.pdfExport")}</th>
                </tr>
              </thead>
              <tbody>
                {capabilities.map((capability) => (
                  <tr key={capability.extension} className="border-t" style={{ borderColor: "var(--border)" }}>
                    <td className="py-1 pr-3 font-medium">{capability.extension.toUpperCase()}</td>
                    <td className="py-1 px-3">
                      <YesNo value={capability.open} />
                    </td>
                    <td className="py-1 px-3">
                      <YesNo value={capability.edit} />
                    </td>
                    <td className="py-1 px-3">
                      <YesNo value={capability.save} />
                    </td>
                    <td className="py-1 pl-3">
                      <YesNo value={capability.pdfExport} />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : (
          <div className="flex items-center gap-2 text-xs muted">
            <Spinner size={13} /> {t("compat.loading")}
          </div>
        )}
      </div>
    </div>
  );
}

export function CompatibilityCenterDialog({
  open,
  onClose,
  kind,
  model,
}: {
  open: boolean;
  onClose: () => void;
  kind: string;
  model: unknown;
}) {
  const t = useT();
  const targets = TARGETS[kind] ?? TARGETS.writer;
  const [target, setTarget] = useState(targets[0]);
  const [report, setReport] = useState<CompatibilityReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const modelRef = useRef(model);
  modelRef.current = model;

  useEffect(() => {
    setTarget(TARGETS[kind]?.[0] ?? TARGETS.writer[0]);
  }, [kind]);

  useEffect(() => {
    if (!open) {
      setReport(null);
      setError(null);
      return;
    }
    let alive = true;
    setLoading(true);
    setError(null);
    compatibilityReport(kind, modelRef.current, target)
      .then((value) => {
        if (alive) setReport(value);
      })
      .catch((err) => {
        if (alive) setError(errorMessage(err, t));
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [open, kind, target, t]);

  if (!open) return null;

  return (
    <Dialog title={t("compat.title")} onClose={onClose} wide>
      <div className="stack">
        <p className="muted">{t("compat.intro")}</p>
        <label className="field">
          <span>{t("compat.target")}</span>
          <select value={target} onChange={(event) => setTarget(event.target.value)}>
            {targets.map((option) => (
              <option key={option} value={option}>
                {option.toUpperCase()}
              </option>
            ))}
          </select>
        </label>
        {loading ? <p className="muted">{t("compat.loading")}</p> : null}
        {error ? (
          <p style={{ color: "var(--danger)" }}>{error}</p>
        ) : null}
        <CompatibilityReportView report={report} />
        <div className="row" style={{ justifyContent: "flex-end" }}>
          <button type="button" className="btn btn-soft" onClick={onClose}>
            {t("compat.close")}
          </button>
        </div>
      </div>
    </Dialog>
  );
}
