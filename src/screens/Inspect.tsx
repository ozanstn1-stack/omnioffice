import { useCallback, useEffect, useMemo, useState } from "react";
import { FileSearch, RefreshCw, ShieldAlert, ShieldCheck, ShieldQuestion } from "lucide-react";
import { Badge, Card, Spinner } from "../components/ui";
import { DropZone, FileList } from "../components/files";
import { OptionCard, Screen, TwoColumn } from "../components/layout";
import { useT } from "../lib/i18n";
import { useTool } from "../lib/useTool";
import { inspectDocument, toAppError } from "../lib/api";
import { formatBytes } from "../lib/format";
import type { DocumentInspection, InspectionFinding, Severity } from "../lib/types";

type Tab = "overview" | "fonts" | "images" | "structure" | "findings";

const TABS: Tab[] = ["overview", "fonts", "images", "structure", "findings"];

function severityTone(severity: Severity): "danger" | "warn" | "accent" {
  if (severity === "error") return "danger";
  if (severity === "warning") return "warn";
  return "accent";
}

function severityIcon(severity: Severity) {
  if (severity === "error") return <ShieldAlert size={13} />;
  if (severity === "warning") return <ShieldQuestion size={13} />;
  return <ShieldCheck size={13} />;
}

function Row({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div
      className="flex items-baseline justify-between gap-3 py-1 border-b last:border-0"
      style={{ borderColor: "var(--border)" }}
    >
      <span className="text-xs muted">{label}</span>
      <span className="text-xs text-right break-words" style={{ color: "var(--text-1)" }}>
        {value}
      </span>
    </div>
  );
}

function Finding({ finding }: { finding: InspectionFinding }) {
  const t = useT();
  return (
    <div className="flex items-start gap-2 py-1.5">
      <span className="mt-0.5" style={{ color: "var(--text-1)" }}>
        {severityIcon(finding.severity)}
      </span>
      <div className="min-w-0">
        <div className="flex items-center gap-2 flex-wrap">
          <Badge tone={severityTone(finding.severity)}>{t(`inspect.severity.${finding.severity}`)}</Badge>
          <span className="text-xs" style={{ color: "var(--text-1)" }}>
            {finding.title}
          </span>
          <span className="text-xs muted">{finding.code}</span>
        </div>
        <p className="text-xs mt-0.5" style={{ color: "var(--text-1)" }}>
          {finding.detail}
        </p>
      </div>
    </div>
  );
}

export function Inspect({ initialFiles, dragging }: { initialFiles?: string[]; dragging: boolean }) {
  const t = useT();
  const session = useTool({ suffix: "_inspected", accept: "pdf", loadInfo: false, initialPaths: initialFiles });
  const [report, setReport] = useState<DocumentInspection | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>("overview");

  const load = useCallback(async () => {
    if (!session.primary) return;
    setLoading(true);
    setError(null);
    setReport(null);
    try {
      const result = await inspectDocument(session.primary.path, session.password || undefined);
      setReport(result);
    } catch (err) {
      setReport(null);
      setError(toAppError(err).message);
    } finally {
      setLoading(false);
    }
  }, [session.primary, session.password]);

  // `load` already clears the report/error it owns; the only thing the
  // document change has to do is kick off the new load. Stale results from the
  // previous document are ignored because `load` is keyed on the active file.
  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect -- the loader owns the report/error state
    if (session.primary) void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [session.primary?.path]);

  const counts = useMemo(() => {
    const found = report?.findings ?? [];
    return {
      error: found.filter((entry) => entry.severity === "error").length,
      warning: found.filter((entry) => entry.severity === "warning").length,
      info: found.filter((entry) => entry.severity === "info").length,
    };
  }, [report]);

  const embedded = useMemo(() => (report?.fonts ?? []).filter((font) => font.embedded).length, [report]);
  const unembedded = (report?.fonts.length ?? 0) - embedded;

  return (
    <Screen
      title={t("nav.inspect")}
      subtitle={t("inspect.subtitle")}
      actions={<FileSearch size={18} className="muted" />}
    >
      <TwoColumn
        main={
          <>
            <OptionCard title={t("inspect.document")}>
              {!session.primary ? (
                <DropZone onPaths={(paths) => void session.addPaths(paths)} dragging={dragging} accept="pdf" />
              ) : (
                <FileList
                  files={session.files}
                  onRemove={session.removeFile}
                  onAdd={session.pickFiles}
                  addLabel={t("common.addPdf")}
                />
              )}
            </OptionCard>

            {loading ? (
              <Card className="p-6 flex items-center justify-center gap-2 text-sm muted">
                <Spinner size={18} /> {t("inspect.analyzing")}
              </Card>
            ) : error ? (
              <Card className="p-4">
                <p className="text-xs" style={{ color: "var(--danger)" }}>
                  {error}
                </p>
              </Card>
            ) : report ? (
              <Card className="p-4 flex flex-col gap-3">
                <div className="flex items-center gap-2 flex-wrap">
                  {report.accessibilityConformance === "passes" ? (
                    <Badge tone="ok">
                      <span className="flex items-center gap-1">
                        <ShieldCheck size={12} /> {t("inspect.a11yPasses")}
                      </span>
                    </Badge>
                  ) : (
                    <Badge tone="danger">
                      <span className="flex items-center gap-1">
                        <ShieldAlert size={12} /> {t("inspect.a11yFails")}
                      </span>
                    </Badge>
                  )}
                  {counts.error ? (
                    <Badge tone="danger">
                      {counts.error} {t("inspect.severity.error")}
                    </Badge>
                  ) : null}
                  {counts.warning ? (
                    <Badge tone="warn">
                      {counts.warning} {t("inspect.severity.warning")}
                    </Badge>
                  ) : null}
                  {counts.info ? (
                    <Badge>
                      {counts.info} {t("inspect.severity.info")}
                    </Badge>
                  ) : null}
                </div>

                <div className="flex flex-wrap gap-1.5">
                  {TABS.map((item) => (
                    <button
                      key={item}
                      type="button"
                      className={`btn btn-sm ${tab === item ? "btn-primary" : ""}`}
                      onClick={() => setTab(item)}
                    >
                      {t(`inspect.tab.${item}`)}
                    </button>
                  ))}
                </div>

                {tab === "overview" ? (
                  <div>
                    <Row label={t("inspect.file")} value={report.path.split(/[\\/]/).pop() ?? report.path} />
                    <Row label={t("inspect.size")} value={formatBytes(report.fileSizeBytes)} />
                    <Row label={t("inspect.pages")} value={report.pageCount} />
                    <Row label={t("inspect.objects")} value={report.objectCount} />
                    <Row label={t("inspect.version")} value={report.pdfVersion} />
                    <Row
                      label={t("inspect.encryption")}
                      value={report.encrypted ? t("inspect.yes") : t("inspect.no")}
                    />
                    <Row
                      label={t("inspect.linearized")}
                      value={report.linearized ? t("inspect.yes") : t("inspect.no")}
                    />
                    <Row
                      label={t("inspect.javascript")}
                      value={
                        report.hasJavascript
                          ? report.javascriptEntries.length
                            ? report.javascriptEntries.join(", ")
                            : t("inspect.yes")
                          : t("inspect.no")
                      }
                    />
                    <Row
                      label={t("inspect.acroForm")}
                      value={
                        report.hasAcroForm ? `${report.formFields.length} ${t("inspect.formFields")}` : t("inspect.no")
                      }
                    />
                    <Row
                      label={t("inspect.structTree")}
                      value={report.hasStructTree ? t("inspect.yes") : t("inspect.no")}
                    />
                    <Row
                      label={t("inspect.structTreeParsed")}
                      value={report.structTree ? t("inspect.yes") : t("inspect.no")}
                    />
                    <Row label={t("inspect.marked")} value={report.marked ? t("inspect.yes") : t("inspect.no")} />
                    <Row label={t("inspect.language")} value={report.language || t("inspect.none")} />
                    <Row label={t("inspect.title")} value={report.titleOverride || t("inspect.none")} />
                    <Row label={t("inspect.author")} value={report.authorOverride || t("inspect.none")} />
                    <Row label={t("inspect.producer")} value={report.producer || t("inspect.none")} />
                    <Row label={t("inspect.creator")} value={report.creator || t("inspect.none")} />
                    <Row label={t("inspect.subject")} value={report.subjectOverride || t("inspect.none")} />
                    <Row
                      label={t("inspect.openAction")}
                      value={report.hasOpenAction ? t("inspect.yes") : t("inspect.no")}
                    />
                    <Row label={t("inspect.attachments")} value={report.attachmentCount} />
                    <Row
                      label={t("inspect.embeddedFiles")}
                      value={report.embeddedFiles.length ? report.embeddedFiles.join(", ") : t("inspect.none")}
                    />
                    <Row label={t("inspect.imagePixels")} value={report.totalImagePixels.toLocaleString()} />
                  </div>
                ) : null}

                {tab === "fonts" ? (
                  report.fonts.length ? (
                    <div className="flex flex-col">
                      <div className="flex items-center gap-2 pb-2">
                        <Badge tone="ok">
                          {embedded} {t("inspect.embedded")}
                        </Badge>
                        {unembedded ? (
                          <Badge tone="warn">
                            {unembedded} {t("inspect.notEmbedded")}
                          </Badge>
                        ) : null}
                      </div>
                      <div className="flex flex-col max-h-[520px] overflow-auto">
                        {report.fonts.map((font, index) => (
                          <div
                            key={`${font.name}-${index}`}
                            className="flex items-center justify-between gap-2 py-1.5 border-b last:border-0"
                            style={{ borderColor: "var(--border)" }}
                          >
                            <span className="text-xs truncate" style={{ color: "var(--text-1)" }}>
                              {font.name || t("inspect.none")}
                            </span>
                            <span className="flex items-center gap-2 shrink-0">
                              <span className="text-xs muted">{font.subtype}</span>
                              <Badge tone={font.embedded ? "ok" : "warn"}>
                                {font.embedded ? t("inspect.embedded") : t("inspect.notEmbedded")}
                              </Badge>
                              {font.composite ? <Badge tone="accent">Type0</Badge> : null}
                              {font.embeddedFormats.length ? (
                                <span className="text-xs muted">{font.embeddedFormats.join(", ")}</span>
                              ) : null}
                            </span>
                          </div>
                        ))}
                      </div>
                    </div>
                  ) : (
                    <p className="text-xs muted">{t("inspect.noFonts")}</p>
                  )
                ) : null}

                {tab === "images" ? (
                  report.images.length ? (
                    <div className="flex flex-col max-h-[520px] overflow-auto">
                      {report.images.map((image, index) => (
                        <div
                          key={`${image.width}x${image.height}-${index}`}
                          className="flex items-center justify-between gap-2 py-1.5 border-b last:border-0"
                          style={{ borderColor: "var(--border)" }}
                        >
                          <span className="text-xs" style={{ color: "var(--text-1)" }}>
                            {image.width} × {image.height} · {image.colorSpace} · {image.bitsPerComponent} bit ·{" "}
                            {image.filter}
                          </span>
                          {image.occurrences > 1 ? <span className="text-xs muted">×{image.occurrences}</span> : null}
                        </div>
                      ))}
                      <div className="pt-2">
                        <p className="text-xs muted">{t("inspect.colorSpaces")}</p>
                        <div className="flex flex-wrap gap-1.5 mt-1">
                          {report.colorSpaces.map((space) => (
                            <Badge key={space.name} tone={space.deviceDependent ? "warn" : "default"}>
                              {space.name}
                              {space.components ? ` (${space.components})` : ""}
                              {space.occurrences > 1 ? ` ×${space.occurrences}` : ""}
                            </Badge>
                          ))}
                        </div>
                      </div>
                    </div>
                  ) : (
                    <p className="text-xs muted">{t("inspect.noImages")}</p>
                  )
                ) : null}

                {tab === "structure" ? (
                  <div className="flex flex-col gap-4">
                    <div>
                      <p className="text-xs muted mb-1">{t("inspect.outline")}</p>
                      {report.outline.length ? (
                        <div className="flex flex-col max-h-52 overflow-auto">
                          {report.outline.map((entry, index) => (
                            <div
                              key={`${entry.title}-${index}`}
                              className="text-xs py-1 flex items-baseline justify-between gap-2"
                              style={{ paddingLeft: entry.depth * 12 }}
                            >
                              <span className="truncate" style={{ color: "var(--text-1)" }}>
                                {entry.title}
                              </span>
                              {entry.page ? <span className="text-xs muted shrink-0">{entry.page}</span> : null}
                            </div>
                          ))}
                        </div>
                      ) : (
                        <p className="text-xs muted">{t("inspect.noOutline")}</p>
                      )}
                    </div>
                    <div>
                      <p className="text-xs muted mb-1">{t("inspect.formFields")}</p>
                      {report.formFields.length ? (
                        <div className="flex flex-col max-h-52 overflow-auto">
                          {report.formFields.map((field, index) => (
                            <div
                              key={`${field.name}-${index}`}
                              className="text-xs py-1 flex items-baseline justify-between gap-2"
                            >
                              <span className="truncate" style={{ color: "var(--text-1)" }}>
                                {field.name || `(${t("inspect.unnamed")})`}
                              </span>
                              <span className="flex items-center gap-1.5 shrink-0">
                                <Badge>{field.fieldType || "—"}</Badge>
                                {field.required ? <Badge tone="accent">{t("inspect.required")}</Badge> : null}
                                {field.missingLabel ? <Badge tone="warn">{t("inspect.missingLabel")}</Badge> : null}
                              </span>
                            </div>
                          ))}
                        </div>
                      ) : (
                        <p className="text-xs muted">{t("inspect.noFields")}</p>
                      )}
                    </div>
                    <div>
                      <p className="text-xs muted mb-1">{t("inspect.annotations")}</p>
                      {report.annotations.length ? (
                        <div className="flex flex-col max-h-40 overflow-auto">
                          {report.annotations.map((annotation, index) => (
                            <div
                              key={`${annotation.subtype}-${index}`}
                              className="text-xs py-1 flex items-baseline justify-between gap-2"
                            >
                              <span className="truncate" style={{ color: "var(--text-1)" }}>
                                {annotation.contents || annotation.subtype}
                              </span>
                              <span className="flex items-center gap-1.5 shrink-0">
                                <Badge>{annotation.subtype}</Badge>
                                {annotation.hidden ? <Badge tone="warn">{t("inspect.hidden")}</Badge> : null}
                              </span>
                            </div>
                          ))}
                        </div>
                      ) : (
                        <p className="text-xs muted">{t("inspect.noAnnotations")}</p>
                      )}
                    </div>
                    <div>
                      <p className="text-xs muted mb-1">{t("inspect.embeddedFiles")}</p>
                      {report.embeddedFiles.length ? (
                        <div className="flex flex-col max-h-32 overflow-auto">
                          {report.embeddedFiles.map((name) => (
                            <span key={name} className="text-xs truncate" style={{ color: "var(--text-1)" }}>
                              {name}
                            </span>
                          ))}
                        </div>
                      ) : (
                        <p className="text-xs muted">{t("inspect.noEmbeddedFiles")}</p>
                      )}
                    </div>
                  </div>
                ) : null}

                {tab === "findings" ? (
                  report.findings.length ? (
                    <div className="flex flex-col max-h-[520px] overflow-auto">
                      {report.findings.map((finding, index) => (
                        <Finding key={`${finding.code}-${index}`} finding={finding} />
                      ))}
                    </div>
                  ) : (
                    <p className="text-xs muted">{t("inspect.noFindings")}</p>
                  )
                ) : null}
              </Card>
            ) : null}
          </>
        }
        side={
          <>
            <button
              className="btn self-start"
              type="button"
              onClick={() => void load()}
              disabled={!session.primary || loading}
            >
              {loading ? <Spinner size={14} /> : <RefreshCw size={14} />} {t("inspect.analyze")}
            </button>
            <Card className="p-4 flex items-start gap-2 text-xs muted">
              <FileSearch size={14} className="mt-0.5" />
              {t("inspect.localNote")}
            </Card>
          </>
        }
      />
    </Screen>
  );
}
