/**
 * The properties of a chart already on the sheet: title, kind, legend and the
 * ranges it reads (4.1a); stacking, value labels and axis titles (4.1c).
 */
import { useState } from "react";
import { useT } from "../../../lib/i18n";
import type { ChartData } from "../../../lib/office-types";
import { Dialog } from "../../office-ui";
import { CHART_KINDS } from "./ChartInsertDialog";
import { parseRange } from "../addresses";
import { STACKABLE } from "../chart-stack";
import { DEFAULT_HOLE_SIZE, SCATTER_STYLES, holeSizeOf } from "../chart-geometry";

/** A range field holds a valid `A1:B2` (or single cell) reference. */
const validRange = (text: string) => parseRange(text.trim()) !== null;

/**
 * The chart with a new kind: the doughnut hole and the scatter style only
 * belong to their own kind, as when the chart is inserted.
 */
export function withKind(chart: ChartData, kind: string): ChartData {
  const next: ChartData = { ...chart, kind };
  if (kind === "doughnut") next.holeSize = holeSizeOf({ holeSize: chart.holeSize ?? DEFAULT_HOLE_SIZE });
  else delete next.holeSize;
  if (kind !== "scatter") delete next.scatterStyle;
  return next;
}

export function ChartProperties({
  chart,
  onSave,
  onClose,
}: {
  chart: ChartData;
  onSave: (chart: ChartData) => void;
  onClose: () => void;
}) {
  const t = useT();
  const [draft, setDraft] = useState<ChartData>(chart);
  const patch = (change: Partial<ChartData>) => setDraft((current) => ({ ...current, ...change }));
  const patchSeries = (index: number, change: Partial<ChartData["series"][number]>) =>
    setDraft((current) => ({
      ...current,
      series: current.series.map((entry, position) => (position === index ? { ...entry, ...change } : entry)),
    }));
  const ring = draft.kind === "pie" || draft.kind === "doughnut";
  const valid = validRange(draft.categories) && draft.series.every((entry) => validRange(entry.range));
  const save = () => {
    if (!valid) return;
    onSave({
      ...draft,
      categories: draft.categories.trim(),
      series: draft.series.map((entry) => ({ ...entry, range: entry.range.trim() })),
      // Cached values describe the old ranges; the overlay reads the cells.
      categoriesCache: undefined,
      seriesValuesCache: undefined,
    });
  };

  return (
    <Dialog title={t("calc.chartProperties")} onClose={onClose}>
      <div className="stack">
        <label className="field">
          <span>{t("calc.chartTitleLabel")}</span>
          <input className="input" value={draft.title} onChange={(event) => patch({ title: event.target.value })} />
        </label>
        <div className="chart-kind-grid" role="radiogroup" aria-label={t("calc.chartType")}>
          {CHART_KINDS.map((kind) => (
            <button
              key={kind}
              type="button"
              role="radio"
              aria-checked={draft.kind === kind}
              className={`btn ${draft.kind === kind ? "btn-primary" : "btn-soft"}`}
              onClick={() => setDraft((current) => withKind(current, kind))}
            >
              {t(`calc.chart_${kind}`)}
            </button>
          ))}
        </div>
        {draft.kind === "doughnut" ? (
          <label className="field">
            <span>{t("calc.chartHole")}</span>
            <input
              type="range"
              min={10}
              max={90}
              step={5}
              value={holeSizeOf(draft)}
              onChange={(event) => patch({ holeSize: Number(event.target.value) })}
            />
          </label>
        ) : null}
        {draft.kind === "scatter" ? (
          <label className="field">
            <span>{t("calc.chartScatterStyle")}</span>
            <select
              className="input"
              value={draft.scatterStyle ?? "marker"}
              onChange={(event) => patch({ scatterStyle: event.target.value === "marker" ? null : event.target.value })}
            >
              {SCATTER_STYLES.map((style) => (
                <option key={style} value={style}>
                  {t(`calc.scatter_${style}`)}
                </option>
              ))}
            </select>
          </label>
        ) : null}
        <label className="row">
          <input type="checkbox" checked={draft.legend} onChange={(event) => patch({ legend: event.target.checked })} />
          <span>{t("calc.chartLegend")}</span>
        </label>
        <label className="row">
          <input
            type="checkbox"
            checked={draft.showLabels}
            onChange={(event) => patch({ showLabels: event.target.checked })}
          />
          <span>{t("calc.chartLabels")}</span>
        </label>
        {STACKABLE.has(draft.kind) ? (
          <label className="row">
            <input
              type="checkbox"
              checked={draft.stacked}
              onChange={(event) => patch({ stacked: event.target.checked })}
            />
            <span>{t("calc.chartStacked")}</span>
          </label>
        ) : null}
        {ring ? null : (
          <div className="row">
            <label className="field">
              <span>{t("calc.chartXTitle")}</span>
              <input
                className="input"
                value={draft.xTitle}
                onChange={(event) => patch({ xTitle: event.target.value })}
              />
            </label>
            <label className="field">
              <span>{t("calc.chartYTitle")}</span>
              <input
                className="input"
                value={draft.yTitle}
                onChange={(event) => patch({ yTitle: event.target.value })}
              />
            </label>
          </div>
        )}
        <label className="field">
          <span>{t("calc.chartCategories")}</span>
          <input
            className="input"
            value={draft.categories}
            aria-invalid={!validRange(draft.categories)}
            onChange={(event) => patch({ categories: event.target.value })}
          />
        </label>
        <fieldset className="stack">
          <legend>{t("calc.chartSeries")}</legend>
          {draft.series.map((entry, index) => (
            <div key={index} className="row">
              <input
                className="input"
                aria-label={t("calc.chartSeriesName", { index: index + 1 })}
                value={entry.name}
                onChange={(event) => patchSeries(index, { name: event.target.value })}
              />
              <input
                className="input"
                aria-label={t("calc.chartSeriesRange", { index: index + 1 })}
                value={entry.range}
                aria-invalid={!validRange(entry.range)}
                onChange={(event) => patchSeries(index, { range: event.target.value })}
              />
            </div>
          ))}
        </fieldset>
        {valid ? null : <p className="muted">{t("calc.chartRangeInvalid")}</p>}
        <button type="button" className="btn btn-primary" disabled={!valid} onClick={save}>
          {t("calc.chartSave")}
        </button>
      </div>
    </Dialog>
  );
}
