/** The chart overlay drawn over the sheet and the dialog that picks its kind. */
import { useMemo, useState, type ReactNode } from "react";
import { Trash2 } from "lucide-react";
import { useT } from "../../../lib/i18n";
import type { ChartData, Sheet, Workbook } from "../../../lib/office-types";
import { Dialog } from "../../office-ui";
import { computeSheetValues } from "../cells";
import type { ChartOptions } from "../chart-data";
import {
  DEFAULT_HOLE_SIZE,
  SCATTER_STYLES,
  formatTick,
  holeSizeOf,
  niceScale,
  ringBands,
  ringSlices,
  scatterPoints,
  scatterStyleOf,
  scatterXValues,
  smoothPath,
} from "../chart-geometry";
import { addressesInRange } from "../formula";

const PALETTE = ["#2563eb", "#059669", "#d97706", "#dc2626", "#7c3aed", "#0891b2"];
/** Height of the title bar above the drawing, and of the legend row below it. */
const HEAD_HEIGHT = 26;
const LEGEND_HEIGHT = 22;
const AXIS_COLOR = "#cbd5e1";
const LABEL_COLOR = "#64748b";

interface Series {
  name: string;
  values: number[];
  color: string | null;
}

/** The plot rectangle inside the SVG: room on the left for the value labels and below for the category labels. */
interface Plot {
  left: number;
  top: number;
  width: number;
  height: number;
}

const colorOf = (entry: Series, index: number) => entry.color ?? PALETTE[index % PALETTE.length];

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
  const t = useT();
  const values = useMemo(() => computeSheetValues(workbook, sheet), [workbook, sheet]);
  const data = chart.chart;
  const categoryCells = addressesInRange(data.categories, 5000).map((address) => values.get(address) ?? "");
  const categories = categoryCells.map(String);
  const series: Series[] = data.series.map((entry) => ({
    name: entry.name,
    values: addressesInRange(entry.range, 5000).map((address) => Number(values.get(address) ?? 0)),
    color: entry.color,
  }));
  const width = chart.widthPx;
  const height = chart.heightPx;
  const svgHeight = height - HEAD_HEIGHT - (data.legend ? LEGEND_HEIGHT : 0);
  const plot: Plot = { left: 40, top: 10, width: width - 48, height: Math.max(20, svgHeight - 10 - 22) };
  const ring = data.kind === "pie" || data.kind === "doughnut";

  return (
    <div className="chart-box" style={{ left: x, top: y, width, height }}>
      <div className="chart-head">
        <strong>{data.title}</strong>
        <button type="button" className="icon-btn" onClick={onRemove} title={t("calc.chartDelete")}>
          <Trash2 size={12} />
        </button>
      </div>
      <svg
        width={width}
        height={svgHeight}
        viewBox={`0 0 ${width} ${svgHeight}`}
        role="img"
        aria-label={`${t(`calc.chart_${data.kind}`)}: ${data.title}`}
        data-chart-kind={data.kind}
      >
        {data.kind === "scatter" ? (
          <ScatterPlot chart={data} plot={plot} categories={categoryCells} series={series} />
        ) : ring ? (
          <RingPlot chart={data} width={width} height={svgHeight} series={series} />
        ) : (
          <CartesianPlot kind={data.kind} plot={plot} categories={categories} series={series} />
        )}
      </svg>
      {data.legend ? (
        <div className="chart-legend">
          {(ring
            ? categories.map((name, index) => ({ name, color: PALETTE[index % PALETTE.length] }))
            : series.map((entry, index) => ({ name: entry.name, color: colorOf(entry, index) }))
          ).map((entry, index) => (
            <span key={index}>
              <i style={{ background: entry.color }} />
              {entry.name}
            </span>
          ))}
        </div>
      ) : null}
    </div>
  );
}

/** Column, bar, line and area charts: values against categories. */
function CartesianPlot({
  kind,
  plot,
  categories,
  series,
}: {
  kind: string;
  plot: Plot;
  categories: string[];
  series: Series[];
}) {
  const all = series.flatMap((entry) => entry.values).filter(Number.isFinite);
  const max = Math.max(1, ...all);
  const min = Math.min(0, ...all);
  const count = Math.max(1, categories.length, ...series.map((entry) => entry.values.length));
  const horizontal = kind === "bar";
  const span = max - min || 1;
  const bottom = plot.top + plot.height;
  /** Pixel position of a value on the value axis. */
  const along = (value: number) =>
    horizontal ? plot.left + ((value - min) / span) * plot.width : bottom - ((value - min) / span) * plot.height;
  const zero = along(0);
  const band = (horizontal ? plot.height : plot.width) / count;

  const linePoints = (entry: Series) =>
    entry.values
      .map((value, index) => {
        const px = plot.left + (count === 1 ? plot.width / 2 : (index / (count - 1)) * plot.width);
        return `${px},${along(value)}`;
      })
      .join(" ");

  return (
    <>
      <line x1={plot.left} y1={bottom} x2={plot.left + plot.width} y2={bottom} stroke={AXIS_COLOR} />
      <line x1={plot.left} y1={plot.top} x2={plot.left} y2={bottom} stroke={AXIS_COLOR} />
      {kind === "column" || kind === "bar"
        ? series.map((entry, seriesIndex) =>
            entry.values.map((value, index) => {
              const thick = Math.max(2, (band * 0.7) / series.length);
              const offset = index * band + band * 0.15 + seriesIndex * thick;
              const at = along(value);
              return (
                <rect
                  key={`${seriesIndex}-${index}`}
                  x={horizontal ? Math.min(zero, at) : plot.left + offset}
                  y={horizontal ? plot.top + offset : Math.min(zero, at)}
                  width={horizontal ? Math.abs(at - zero) : thick}
                  height={horizontal ? thick : Math.abs(at - zero)}
                  fill={colorOf(entry, seriesIndex)}
                  opacity={0.85}
                />
              );
            }),
          )
        : null}
      {kind === "line" || kind === "area"
        ? series.map((entry, index) => (
            <g key={index}>
              {kind === "area" ? (
                <polygon
                  points={`${plot.left},${bottom} ${linePoints(entry)} ${plot.left + plot.width},${bottom}`}
                  fill={colorOf(entry, index)}
                  opacity={0.25}
                />
              ) : null}
              <polyline points={linePoints(entry)} fill="none" stroke={colorOf(entry, index)} strokeWidth={2} />
            </g>
          ))
        : null}
      {categories.map((label, index) => (
        <text
          key={index}
          x={horizontal ? plot.left - 4 : plot.left + (index + 0.5) * band}
          y={horizontal ? plot.top + (index + 0.5) * band + 3 : bottom + 14}
          fontSize={9}
          textAnchor={horizontal ? "end" : "middle"}
          fill={LABEL_COLOR}
        >
          {label.length > 8 ? `${label.slice(0, 7)}…` : label}
        </text>
      ))}
    </>
  );
}

/** Pie and doughnut: one series is a ring (a full disc for a pie); a doughnut with more series nests them. */
function RingPlot({
  chart,
  width,
  height,
  series,
}: {
  chart: ChartData;
  width: number;
  height: number;
  series: Series[];
}) {
  const radius = Math.max(10, Math.min(width, height) / 2 - 8);
  const cx = width / 2;
  const cy = height / 2;
  const doughnut = chart.kind === "doughnut";
  const bands = doughnut ? ringBands(series.length, radius, holeSizeOf(chart)) : [{ inner: 0, outer: radius }];
  const rings: ReactNode[] = [];
  (doughnut ? series : series.slice(0, 1)).forEach((entry, ringIndex) => {
    const { inner, outer } = bands[ringIndex];
    ringSlices(entry.values, PALETTE, { cx, cy, inner, outer }).forEach((slice, index) => {
      rings.push(
        <path
          key={`${ringIndex}-${index}`}
          d={slice.path}
          fill={slice.color}
          stroke="var(--surface)"
          strokeWidth={1}
          opacity={0.9}
        />,
      );
      if (chart.showLabels && slice.fraction >= 0.04) {
        rings.push(
          <text
            key={`label-${ringIndex}-${index}`}
            x={slice.label.x}
            y={slice.label.y + 3}
            fontSize={10}
            textAnchor="middle"
            fill="#ffffff"
          >
            {Math.round(slice.fraction * 100)}%
          </text>,
        );
      }
    });
  });
  return <>{rings}</>;
}

/** XY scatter: the categories range holds the X values, each series range the Y values. */
function ScatterPlot({
  chart,
  plot,
  categories,
  series,
}: {
  chart: ChartData;
  plot: Plot;
  categories: unknown[];
  series: Series[];
}) {
  const look = scatterStyleOf(chart.scatterStyle);
  const length = Math.max(categories.length, ...series.map((entry) => entry.values.length));
  const xs = scatterXValues(categories, length);
  const drawn = series.map((entry) => scatterPoints(xs, entry.values));
  const every = drawn.flat();
  const xScale = niceScale(Math.min(...every.map((p) => p.x)), Math.max(...every.map((p) => p.x)));
  const yScale = niceScale(Math.min(...every.map((p) => p.y)), Math.max(...every.map((p) => p.y)));
  const bottom = plot.top + plot.height;
  const px = (value: number) => plot.left + ((value - xScale.min) / (xScale.max - xScale.min || 1)) * plot.width;
  const py = (value: number) => bottom - ((value - yScale.min) / (yScale.max - yScale.min || 1)) * plot.height;

  return (
    <>
      {yScale.ticks.map((tick) => (
        <g key={`y${tick}`}>
          <line
            x1={plot.left}
            y1={py(tick)}
            x2={plot.left + plot.width}
            y2={py(tick)}
            stroke={AXIS_COLOR}
            opacity={0.5}
          />
          <text x={plot.left - 4} y={py(tick) + 3} fontSize={9} textAnchor="end" fill={LABEL_COLOR}>
            {formatTick(tick)}
          </text>
        </g>
      ))}
      {xScale.ticks.map((tick) => (
        <text key={`x${tick}`} x={px(tick)} y={bottom + 14} fontSize={9} textAnchor="middle" fill={LABEL_COLOR}>
          {formatTick(tick)}
        </text>
      ))}
      <line x1={plot.left} y1={bottom} x2={plot.left + plot.width} y2={bottom} stroke={AXIS_COLOR} />
      <line x1={plot.left} y1={plot.top} x2={plot.left} y2={bottom} stroke={AXIS_COLOR} />
      {series.map((entry, index) => {
        const color = colorOf(entry, index);
        const points = drawn[index].map((point) => ({ x: px(point.x), y: py(point.y) }));
        return (
          <g key={index} data-series={index}>
            {look.line && points.length > 1 ? (
              look.smooth ? (
                <path d={smoothPath(points)} fill="none" stroke={color} strokeWidth={2} />
              ) : (
                <polyline
                  points={points.map((point) => `${point.x},${point.y}`).join(" ")}
                  fill="none"
                  stroke={color}
                  strokeWidth={2}
                />
              )
            ) : null}
            {look.markers
              ? points.map((point, at) => (
                  <circle key={at} cx={point.x} cy={point.y} r={3.5} fill={color} opacity={0.9} />
                ))
              : null}
          </g>
        );
      })}
    </>
  );
}

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
