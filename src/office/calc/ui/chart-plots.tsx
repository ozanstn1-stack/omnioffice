/** The SVG drawings inside a Calc chart box: cartesian, ring and scatter plots. */
import type { ReactNode } from "react";
import type { ChartData } from "../../../lib/office-types";
import {
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

export const PALETTE = ["#2563eb", "#059669", "#d97706", "#dc2626", "#7c3aed", "#0891b2"];
export const AXIS_COLOR = "#cbd5e1";
export const LABEL_COLOR = "#64748b";

export interface Series {
  name: string;
  values: number[];
  color: string | null;
}

/** The plot rectangle inside the SVG: room on the left for the value labels and below for the category labels. */
export interface Plot {
  left: number;
  top: number;
  width: number;
  height: number;
}

export const colorOf = (entry: Series, index: number) => entry.color ?? PALETTE[index % PALETTE.length];

/** Pie and doughnut: one series is a ring (a full disc for a pie); a doughnut with more series nests them. */
export function RingPlot({
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
export function ScatterPlot({
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
