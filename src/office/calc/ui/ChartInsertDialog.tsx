/** The Insert chart dialog: the kind built from the current selection, and that kind's options. */
import { useState } from "react";
import { useT } from "../../../lib/i18n";
import { Dialog } from "../../office-ui";
import type { ChartOptions } from "../chart-data";
import { DEFAULT_HOLE_SIZE, SCATTER_STYLES } from "../chart-geometry";

export const CHART_KINDS = ["column", "bar", "line", "pie", "doughnut", "area", "scatter"] as const;
export type ChartKind = (typeof CHART_KINDS)[number];

/** Picks the kind of the chart built from the current selection, and the options that kind has. */
export function ChartDialog({
  onPick,
  onClose,
}: {
  onPick: (kind: string, options: ChartOptions) => void;
  onClose: () => void;
}) {
  const t = useT();
  const [kind, setKind] = useState<ChartKind>("column");
  const [holeSize, setHoleSize] = useState(DEFAULT_HOLE_SIZE);
  const [scatterStyle, setScatterStyle] = useState<string>("marker");
  return (
    <Dialog title={t("calc.chart")} onClose={onClose}>
      <div className="stack">
        <div className="chart-kind-grid" role="radiogroup" aria-label={t("calc.chartType")}>
          {CHART_KINDS.map((candidate) => (
            <button
              key={candidate}
              type="button"
              role="radio"
              aria-checked={kind === candidate}
              className={`btn ${kind === candidate ? "btn-primary" : "btn-soft"}`}
              onClick={() => setKind(candidate)}
            >
              {t(`calc.chart_${candidate}`)}
            </button>
          ))}
        </div>
        {kind === "doughnut" ? (
          <label className="field">
            <span>{t("calc.chartHole")}</span>
            <div className="row">
              <input
                type="range"
                min={10}
                max={90}
                step={5}
                value={holeSize}
                onChange={(event) => setHoleSize(Number(event.target.value))}
              />
              <output>{holeSize}%</output>
            </div>
          </label>
        ) : null}
        {kind === "scatter" ? (
          <label className="field">
            <span>{t("calc.chartScatterStyle")}</span>
            <select value={scatterStyle} onChange={(event) => setScatterStyle(event.target.value)}>
              {SCATTER_STYLES.map((style) => (
                <option key={style} value={style}>
                  {t(`calc.scatter_${style}`)}
                </option>
              ))}
            </select>
          </label>
        ) : null}
        <p className="muted">{t(kind === "scatter" ? "calc.chartHintScatter" : "calc.chartHint")}</p>
        <button type="button" className="btn btn-primary" onClick={() => onPick(kind, { holeSize, scatterStyle })}>
          {t("calc.chartInsert")}
        </button>
      </div>
    </Dialog>
  );
}
