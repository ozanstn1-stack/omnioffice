import { useMemo, useState } from "react";
import { Columns2, FileDiff, Images } from "lucide-react";
import { Badge, Card, Field, Segmented, Slider, Spinner, Toggle } from "../components/ui";
import { DropZone, FileList, OutputBar } from "../components/files";
import { OptionCard, Screen, TwoColumn } from "../components/layout";
import { useT } from "../lib/i18n";
import { useTool } from "../lib/useTool";
import { comparePdfs, toAppError } from "../lib/api";
import type { CompareOptions, CompareReport, TextDifference } from "../lib/types";

const DEFAULTS: CompareOptions = {
  maxPages: 2000,
  tolerance: 12,
  dpi: 96,
  visual: false,
  ignoreWhitespace: false,
  maxDifferences: 200,
};

function statusTone(kind: TextDifference["kind"]): "ok" | "warn" | "danger" {
  if (kind === "added") return "ok";
  if (kind === "removed") return "danger";
  return "warn";
}

export function Compare({ initialFiles, dragging }: { initialFiles?: string[]; dragging: boolean }) {
  const t = useT();
  // Two documents, so this tool takes multiple files and treats them as an
  // ordered pair rather than a batch.
  const session = useTool({
    suffix: "_compare",
    accept: "pdf",
    multiple: true,
    loadInfo: false,
    initialPaths: initialFiles,
  });
  const [options, setOptions] = useState<CompareOptions>(DEFAULTS);
  const [report, setReport] = useState<CompareReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [view, setView] = useState<"text" | "visual">("text");
  const [passwords, setPasswords] = useState<Record<number, string>>({});

  const patch = (values: Partial<CompareOptions>) => {
    setOptions((previous) => ({ ...previous, ...values }));
    setReport(null);
  };

  const left = session.files[0]?.path ?? "";
  const right = session.files[1]?.path ?? "";
  const ready = Boolean(left && right);

  const run = async () => {
    if (!ready) return;
    setReport(null);
    setError(null);
    await session.run(async (jobId) => {
      try {
        const result = await comparePdfs(
          left,
          right,
          options,
          jobId,
          passwords[0] || undefined,
          passwords[1] || undefined,
        );
        setReport(result);
        return undefined;
      } catch (err) {
        const appError = toAppError(err);
        if (appError.code !== "cancelled") {
          setError(appError.message);
        }
        return undefined;
      }
    });
  };

  const summary = useMemo(() => {
    if (!report) return null;
    const changed = report.textDifferences.filter((entry) => entry.kind === "changed").length;
    const added = report.textDifferences.filter((entry) => entry.kind === "added").length;
    const removed = report.textDifferences.filter((entry) => entry.kind === "removed").length;
    return { changed, added, removed };
  }, [report]);

  return (
    <Screen
      title={t("nav.compare")}
      subtitle={t("compare.subtitle")}
      actions={<FileDiff size={18} className="muted" />}
    >
      <TwoColumn
        main={
          <>
            <OptionCard title={t("compare.documents")}>
              {!session.files.length ? (
                <DropZone onPaths={(paths) => void session.addPaths(paths)} dragging={dragging} accept="pdf" />
              ) : (
                <FileList
                  files={session.files}
                  onRemove={session.removeFile}
                  onAdd={session.pickFiles}
                  addLabel={t("common.addPdf")}
                />
              )}
              {session.files.length < 2 ? <p className="text-xs muted">{t("compare.needTwo")}</p> : null}
            </OptionCard>

            {report ? (
              <Card className="p-4 flex flex-col gap-3">
                <div className="flex items-center gap-2 flex-wrap">
                  {report.identical ? (
                    <Badge tone="ok">{t("compare.identical")}</Badge>
                  ) : (
                    <Badge tone="warn">{t("compare.different")}</Badge>
                  )}
                  <span className="text-xs muted">
                    {report.leftPages} / {report.rightPages} {t("compare.pages")}
                  </span>
                  {summary ? (
                    <span className="text-xs muted">
                      {summary.changed} {t("compare.changed")} · {summary.added} {t("common.added")} · {summary.removed}{" "}
                      {t("common.removed")}
                    </span>
                  ) : null}
                </div>
                {report.warnings.length ? (
                  <ul className="text-xs muted flex flex-col gap-1">
                    {report.warnings.map((warning) => (
                      <li key={warning}>{warning}</li>
                    ))}
                  </ul>
                ) : null}

                <Segmented<"text" | "visual">
                  value={view}
                  onChange={setView}
                  options={[
                    { value: "text", label: t("compare.textView") },
                    { value: "visual", label: t("compare.visualView") },
                  ]}
                />

                {view === "text" ? (
                  report.textDifferences.length ? (
                    <div className="flex flex-col gap-2 max-h-[520px] overflow-auto">
                      {report.textDifferences.map((entry) => (
                        <div key={`${entry.page}-${entry.kind}`} className="border-t pt-2 first:border-0 first:pt-0">
                          <div className="flex items-center gap-2 mb-1">
                            <Badge tone={statusTone(entry.kind)}>{t(`compare.kind.${entry.kind}`)}</Badge>
                            <span className="text-xs muted">
                              {t("common.page")} {entry.page}
                            </span>
                          </div>
                          <div className="grid gap-1 sm:grid-cols-2">
                            {entry.left ? (
                              <pre
                                className="text-xs whitespace-pre-wrap break-words p-2 rounded"
                                style={{ background: "var(--surface-2)" }}
                              >
                                {entry.left.slice(0, 1200)}
                              </pre>
                            ) : (
                              <span className="text-xs muted">—</span>
                            )}
                            {entry.right ? (
                              <pre
                                className="text-xs whitespace-pre-wrap break-words p-2 rounded"
                                style={{ background: "var(--surface-2)" }}
                              >
                                {entry.right.slice(0, 1200)}
                              </pre>
                            ) : (
                              <span className="text-xs muted">—</span>
                            )}
                          </div>
                        </div>
                      ))}
                    </div>
                  ) : (
                    <p className="text-xs muted">
                      {report.identical ? t("compare.noTextDiff") : t("compare.noTextDiffShort")}
                    </p>
                  )
                ) : report.visualDifferences.length ? (
                  <>
                    <div className="flex flex-col gap-3 max-h-[560px] overflow-auto">
                      {report.visualDifferences.map((entry) => (
                        <div key={entry.page} className="flex flex-col gap-1">
                          <span className="text-xs muted">
                            {t("common.page")} {entry.page} · {(entry.difference * 100).toFixed(2)}%{" "}
                            {t("compare.pixels")}
                          </span>
                          <img
                            src={entry.preview}
                            alt={t("compare.visualView")}
                            className="w-full rounded"
                            style={{ border: "1px solid var(--border)" }}
                          />
                        </div>
                      ))}
                    </div>
                    {report.visualTruncated ? <p className="text-xs muted">{t("compare.visualTruncated")}</p> : null}
                  </>
                ) : (
                  <p className="text-xs muted">{t("compare.noVisualDiff")}</p>
                )}
              </Card>
            ) : error ? (
              <Card className="p-4">
                <p className="text-xs" style={{ color: "var(--danger)" }}>
                  {error}
                </p>
              </Card>
            ) : null}
          </>
        }
        side={
          <>
            <OutputBar session={session} runLabel={t("nav.compare")} onRun={() => void run()} disabled={!ready} />
            <OptionCard title={t("compare.options")}>
              <Toggle
                checked={options.visual}
                onChange={(value) => patch({ visual: value })}
                label={
                  <span className="flex items-center gap-1.5">
                    <Images size={13} /> {t("compare.visualToggle")}
                  </span>
                }
              />
              <p className="text-xs muted">{t("compare.visualHint")}</p>
              {options.visual ? (
                <>
                  <Field label={t("compare.dpi")}>
                    <Slider
                      value={options.dpi}
                      min={48}
                      max={200}
                      step={8}
                      onChange={(value) => patch({ dpi: value })}
                      format={(value) => `${value} dpi`}
                    />
                  </Field>
                  <Field label={t("compare.tolerance")}>
                    <Slider
                      value={options.tolerance}
                      min={0}
                      max={64}
                      onChange={(value) => patch({ tolerance: value })}
                    />
                  </Field>
                </>
              ) : null}
              <Toggle
                checked={options.ignoreWhitespace}
                onChange={(value) => patch({ ignoreWhitespace: value })}
                label={t("compare.ignoreWhitespace")}
              />
              <Field label={t("compare.maxPages")}>
                <Slider
                  value={options.maxPages}
                  min={10}
                  max={5000}
                  step={10}
                  onChange={(value) => patch({ maxPages: value })}
                  format={(value) => String(value)}
                />
              </Field>
              <Field label={t("compare.maxDifferences")}>
                <Slider
                  value={options.maxDifferences}
                  min={10}
                  max={1000}
                  step={10}
                  onChange={(value) => patch({ maxDifferences: value })}
                  format={(value) => String(value)}
                />
              </Field>
            </OptionCard>
            {session.files.length ? (
              <OptionCard title={t("security.passwords")}>
                <div className="flex flex-col gap-2">
                  {session.files.slice(0, 2).map((file, index) => (
                    <Field key={file.path} label={`${t("compare.left")} ${index + 1}: ${file.name}`}>
                      <input
                        className="input"
                        type="password"
                        value={passwords[index] ?? ""}
                        onChange={(event) => {
                          setPasswords((previous) => ({ ...previous, [index]: event.target.value }));
                          setReport(null);
                        }}
                        placeholder={t("compare.optional")}
                      />
                    </Field>
                  ))}
                </div>
              </OptionCard>
            ) : null}
            {session.running ? (
              <Card className="p-4 flex items-center gap-2 text-xs muted">
                <Spinner size={14} /> {t("compare.running")}
              </Card>
            ) : null}
            {session.error ? (
              <Card className="p-4">
                <p className="text-xs" style={{ color: "var(--danger)" }}>
                  {session.error.message}
                </p>
              </Card>
            ) : null}
            <Card className="p-4 flex items-start gap-2 text-xs muted">
              <Columns2 size={14} className="mt-0.5" />
              {t("compare.localNote")}
            </Card>
          </>
        }
      />
    </Screen>
  );
}
