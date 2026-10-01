import { useCallback, useState } from "react";
import { Activity, Gauge } from "lucide-react";
import { Button, Card, Field, Segmented, Slider, Spinner, Toggle } from "../components/ui";
import { DropZone, FileList, InfoStrip, OutputBar, ResultCard } from "../components/files";
import { OptionCard, Screen, TwoColumn } from "../components/layout";
import { useT } from "../lib/i18n";
import { useTool } from "../lib/useTool";
import { compressPdf, estimateCompression } from "../lib/api";
import { formatBytes } from "../lib/format";
import { useSettings, useToasts } from "../lib/store";
import type { CompressEstimate, CompressOptions } from "../lib/types";

export function Compress({ initialFiles, dragging }: { initialFiles?: string[]; dragging: boolean }) {
  const t = useT();
  const defaults = useSettings((s) => s.settings);
  const pushToast = useToasts((s) => s.push);
  const session = useTool({ suffix: "_compressed", accept: "pdf", initialPaths: initialFiles });

  const [options, setOptions] = useState<CompressOptions>({
    strategy: "raster",
    preset: defaults.defaultCompression,
    dpi: 150,
    jpeg_quality: 60,
    grayscale: false,
    remove_metadata: true,
  });
  const [estimate, setEstimate] = useState<CompressEstimate | null>(null);
  const [estimatedFor, setEstimatedFor] = useState<string | null>(null);
  const [analysing, setAnalysing] = useState(false);

  const patch = (values: Partial<CompressOptions>) => {
    setOptions((previous) => ({ ...previous, ...values }));
    setEstimate(null);
    setEstimatedFor(null);
  };

  // A new document invalidates the previous estimate; the derived guard keeps
  // the stale estimate from rendering instead of clearing it in an effect.
  const estimateFor = `${session.primary?.path ?? ""}`;
  const shownEstimate = estimate && estimateFor === estimatedFor ? estimate : null;

  const analyse = useCallback(async () => {
    if (!session.primary) return;
    setAnalysing(true);
    try {
      const result = await estimateCompression(session.primary.path, options, session.password || undefined);
      setEstimate(result);
      setEstimatedFor(`${session.primary.path}`);
    } catch (error) {
      pushToast({ kind: "error", title: t("errors.title"), detail: String((error as { message?: string })?.message ?? error) });
    } finally {
      setAnalysing(false);
    }
  }, [options, pushToast, session.password, session.primary, t]);

  session.registerAutoRun(() => void run());
  const run = () =>
    session.run(async (jobId, overwrite) => {
      const result = await compressPdf(
        session.primary?.path ?? "",
        session.outputSpec(overwrite),
        options,
        jobId,
        session.password || undefined,
      );
      return result;
    });

  return (
    <Screen
      title={t("compress.title")}
      subtitle={t("compress.subtitle")}
      actions={
        session.primary ? (
          <Button variant="ghost" size="sm" icon={<Activity size={14} />} onClick={() => void analyse()} disabled={analysing}>
            {t("compress.analyze")}
          </Button>
        ) : null
      }
    >
      <TwoColumn
        main={
          <>
            {!session.primary ? (
              <DropZone onPaths={(paths) => void session.addPaths(paths)} dragging={dragging} accept="pdf" />
            ) : (
              <>
                <OptionCard>
                  <FileList files={session.files} onRemove={session.removeFile} onAdd={session.pickFiles} addLabel={t("common.addPdf")} />
                </OptionCard>
                {session.info ? (
                  <Card className="p-4">
                    <InfoStrip info={session.info} error={session.infoError} />
                  </Card>
                ) : null}

                <OptionCard title={t("compress.strategy")}>
                  <Segmented<CompressOptions["strategy"]>
                    value={options.strategy}
                    onChange={(value) => patch({ strategy: value })}
                    options={[
                      { value: "raster", label: t("compress.raster") },
                      { value: "lossless", label: t("compress.lossless") },
                    ]}
                  />
                  <p className="text-xs muted -mt-1">
                    {options.strategy === "raster" ? t("compress.rasterHint") : t("compress.losslessHint")}
                  </p>

                  {options.strategy === "raster" ? (
                    <>
                      <Field label={t("compress.preset")}>
                        <Segmented<CompressOptions["preset"]>
                          value={options.preset}
                          onChange={(value) => patch({ preset: value })}
                          options={[
                            { value: "low", label: t("compress.low") },
                            { value: "medium", label: t("compress.medium") },
                            { value: "high", label: t("compress.high") },
                            { value: "custom", label: t("compress.custom") },
                          ]}
                        />
                      </Field>
                      {options.preset === "custom" ? (
                        <>
                          <Field label={t("compress.dpi")}>
                            <Slider value={options.dpi} min={72} max={400} step={2} onChange={(value) => patch({ dpi: value })} format={(v) => `${v}`} />
                          </Field>
                          <Field label={t("compress.quality")}>
                            <Slider
                              value={options.jpeg_quality}
                              min={20}
                              max={95}
                              onChange={(value) => patch({ jpeg_quality: value })}
                              format={(v) => `${v}%`}
                            />
                          </Field>
                        </>
                      ) : null}
                      <Toggle checked={options.grayscale} onChange={(value) => patch({ grayscale: value })} label={t("compress.grayscale")} />
                    </>
                  ) : null}

                  <Toggle
                    checked={options.remove_metadata}
                    onChange={(value) => patch({ remove_metadata: value })}
                    label={t("compress.removeMetadata")}
                  />
                </OptionCard>

                <Card className="p-4">
                  <div className="flex items-center gap-2 mb-3">
                    <Gauge size={16} style={{ color: "var(--accent)" }} />
                    <p className="font-semibold text-[13.5px]">{t("common.estimated")}</p>
                    {analysing ? <Spinner size={14} /> : null}
                  </div>
                  {shownEstimate ? (
                    <div className="flex flex-col gap-3">
                      <div className="grid grid-cols-3 gap-3 text-center">
                        <div className="card-soft p-3">
                          <p className="text-xs muted">{t("common.original")}</p>
                          <p className="font-bold text-[15px] tabular-nums">{formatBytes(shownEstimate.original_bytes)}</p>
                        </div>
                        <div className="card-soft p-3">
                          <p className="text-xs muted">{t("common.estimated")}</p>
                          <p className="font-bold text-[15px] tabular-nums">{formatBytes(shownEstimate.estimated_bytes)}</p>
                        </div>
                        <div className="card-soft p-3">
                          <p className="text-xs muted">{t("common.reduction")}</p>
                          <p
                            className="font-bold text-[15px] tabular-nums"
                            style={{ color: shownEstimate.reduction > 0 ? "var(--ok)" : "var(--danger)" }}
                          >
                            {Math.round(shownEstimate.reduction * 100)}%
                          </p>
                        </div>
                      </div>
                      <p className="text-xs muted">
                        {shownEstimate.method} · {shownEstimate.sample_pages} {t("common.pages")} ·{" "}
                        {shownEstimate.accurate ? (defaults.language === "tr" ? "tam" : "exact") : t("common.estimated")}
                      </p>
                      <p className="text-xs muted">{t("compress.estimateNote")}</p>
                    </div>
                  ) : (
                    <p className="text-xs muted">{t("compress.analyze")}…</p>
                  )}
                </Card>
              </>
            )}
            {session.result ? <ResultCard result={session.result} onReset={session.resetResult} /> : null}
          </>
        }
        side={<OutputBar session={session} runLabel={t("compress.run")} onRun={() => void run()} disabled={!session.primary} />}
      />
    </Screen>
  );
}
