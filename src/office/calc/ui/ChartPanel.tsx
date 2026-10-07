/** The chart overlay drawn over the sheet and the dialog that picks its kind. */
import { Trash2 } from "lucide-react";
import { useT } from "../../../lib/i18n";
import type { ChartData, Sheet, Workbook } from "../../../lib/office-types";
import { Dialog } from "../../office-ui";
import { computeSheetValues } from "../cells";
import { addressesInRange } from "../formula";

export function ChartBox({
  chart,
  sheet,
  workbook,
  x,
  y,
  onRemove,
}: {
  chart: { id: string; chart: ChartData; anchor: string; widthPx: number; heightPx: number };
  sheet: Sheet;
  workbook: Workbook;
  x: number;
  y: number;
  onRemove: () => void;
}) {
  const values = computeSheetValues(workbook, sheet);
  const categories = addressesInRange(chart.chart.categories, 5000).map((address) => String(values.get(address) ?? ""));
  const series = chart.chart.series.map((entry) => ({
    name: entry.name,
    values: addressesInRange(entry.range, 5000).map((address) => Number(values.get(address) ?? 0)),
    color: entry.color,
  }));
  const palette = ["#2563eb", "#059669", "#d97706", "#dc2626", "#7c3aed", "#0891b2"];
  const all = series.flatMap((entry) => entry.values).filter(Number.isFinite);
  const max = Math.max(1, ...all);
  const min = Math.min(0, ...all);
  const width = chart.widthPx;
  const height = chart.heightPx;
  const plotWidth = width - 48;
  const plotHeight = height - 56;
  const count = Math.max(1, categories.length);

  const pointsFor = (values2: number[]) =>
    values2
      .map((value, index) => {
        const px = 40 + (count === 1 ? plotWidth / 2 : (index / (count - 1)) * plotWidth);
        const py = 34 + plotHeight - ((value - min) / (max - min || 1)) * plotHeight;
        return `${px},${py}`;
      })
      .join(" ");

  return (
    <div className="chart-box" style={{ left: x, top: y, width, height }}>
      <div className="chart-head">
        <strong>{chart.chart.title}</strong>
        <button type="button" className="icon-btn" onClick={onRemove} title="Delete chart">
          <Trash2 size={12} />
        </button>
      </div>
      <svg width={width} height={height - 26} viewBox={`0 0 ${width} ${height - 26}`}>
        <line x1={40} y1={height - 22} x2={width - 8} y2={height - 22} stroke="#cbd5e1" />
        <line x1={40} y1={34} x2={40} y2={height - 22} stroke="#cbd5e1" />
        {chart.chart.kind === "pie"
          ? pieSlices(series[0]?.values ?? [], palette).map((slice, index) => (
              <path key={index} d={slice.path} fill={slice.color} opacity={0.85} />
            ))
          : null}
        {chart.chart.kind === "column" || chart.chart.kind === "bar"
          ? series.map((entry, seriesIndex) =>
              entry.values.map((value, index) => {
                const bandWidth = plotWidth / count;
                const barWidth = Math.max(2, (bandWidth * 0.7) / series.length);
                const px = 40 + index * bandWidth + bandWidth * 0.15 + seriesIndex * barWidth;
                const py = 34 + plotHeight - ((value - min) / (max - min || 1)) * plotHeight;
                return (
                  <rect
                    key={`${seriesIndex}-${index}`}
                    x={chart.chart.kind === "bar" ? py : px}
                    y={chart.chart.kind === "bar" ? 34 + index * bandWidth : py}
                    width={chart.chart.kind === "bar" ? 40 + plotHeight - py : barWidth}
                    height={chart.chart.kind === "bar" ? barWidth : 34 + plotHeight - py}
                    fill={entry.color ?? palette[seriesIndex % palette.length]}
                    opacity={0.85}
                  />
                );
              }),
            )
          : null}
        {chart.chart.kind === "line" || chart.chart.kind === "area"
          ? series.map((entry, index) => (
              <g key={index}>
                {chart.chart.kind === "area" ? (
                  <polygon
                    points={`40,${34 + plotHeight} ${pointsFor(entry.values)} ${40 + plotWidth},${34 + plotHeight}`}
                    fill={entry.color ?? palette[index % palette.length]}
                    opacity={0.25}
                  />
                ) : null}
                <polyline
                  points={pointsFor(entry.values)}
                  fill="none"
                  stroke={entry.color ?? palette[index % palette.length]}
                  strokeWidth={2}
                />
              </g>
            ))
          : null}
        {categories.map((label, index) => (
          <text
            key={index}
            x={40 + (index + 0.5) * (plotWidth / count)}
            y={height - 8}
            fontSize={9}
            textAnchor="middle"
            fill="#64748b"
          >
            {label.length > 8 ? `${label.slice(0, 7)}…` : label}
          </text>
        ))}
      </svg>
      {chart.chart.legend ? (
        <div className="chart-legend">
          {series.map((entry, index) => (
            <span key={index}>
              <i style={{ background: entry.color ?? palette[index % palette.length] }} />
              {entry.name}
            </span>
          ))}
        </div>
      ) : null}
    </div>
  );
}

function pieSlices(values: number[], palette: string[]): Array<{ path: string; color: string }> {
  const total = values.reduce((sum, value) => sum + Math.max(0, value), 0) || 1;
  let angle = -Math.PI / 2;
  const radius = 60;
  const cx = 110;
  const cy = 90;
  return values.map((value, index) => {
    const sweep = (Math.max(0, value) / total) * Math.PI * 2;
    const x1 = cx + radius * Math.cos(angle);
    const y1 = cy + radius * Math.sin(angle);
    angle += sweep;
    const x2 = cx + radius * Math.cos(angle);
    const y2 = cy + radius * Math.sin(angle);
    const large = sweep > Math.PI ? 1 : 0;
    return {
      path: `M ${cx} ${cy} L ${x1} ${y1} A ${radius} ${radius} 0 ${large} 1 ${x2} ${y2} Z`,
      color: palette[index % palette.length],
    };
  });
}

export const CHART_KINDS = ["column", "bar", "line", "pie", "area"] as const;

/** Picks the kind of the chart built from the current selection. */
export function ChartDialog({ onPick, onClose }: { onPick: (kind: string) => void; onClose: () => void }) {
  const t = useT();
  return (
    <Dialog title={t("calc.chart")} onClose={onClose}>
      <div className="chart-kind-grid">
        {CHART_KINDS.map((kind) => (
          <button key={kind} type="button" className="btn btn-soft" onClick={() => onPick(kind)}>
            {t(`calc.chart_${kind}`)}
          </button>
        ))}
      </div>
      <p className="muted">{t("calc.chartHint")}</p>
    </Dialog>
  );
}
